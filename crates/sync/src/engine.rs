//! The engine: one iroh endpoint, a session per connected device or guest, tasks that keep
//! own devices and joined notebooks connected, a flush loop that announces local changes.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use bytes::Bytes;
use iroh::endpoint::{Connection, RecvStream, SendStream, presets};
use iroh::{Endpoint, EndpointAddr, EndpointId, RelayMode, SecretKey};
use loro::VersionVector;
use n0te_core::store::Library;
use tokio::sync::{Notify, mpsc};

use crate::proto::{self, ALPN, Auth, CHUNK, DocKey, Invite, Live, LiveBrush, Msg, NbState, PROTO, PairCode, Role, SignedInvite, now_secs};
use crate::state::{self, Banned, DeviceRecord, InviteRecord, JoinedRecord, PendingPair, State};
use crate::{Error, Event, Result};

pub struct EngineOptions {
    /// Shown to other devices ("Laptop", "Tablet").
    pub device_name: String,
    /// Tests: localhost only — no relays, no address lookup. Production: n0 relays + lookup.
    pub local_only: bool,
    /// Receives events on an engine thread. Without one, use [`Engine::events`].
    pub listener: Option<Arc<dyn Fn(Event) + Send + Sync>>,
    /// Test hook: as a guest, push edits even with a read-only invite (the owner must refuse).
    #[doc(hidden)]
    pub guest_ignores_role: bool,
}

impl EngineOptions {
    pub fn new(device_name: impl Into<String>) -> Self {
        EngineOptions { device_name: device_name.into(), local_only: false, listener: None, guest_ignores_role: false }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub online: bool,
    /// Unix seconds (0 = not yet).
    pub last_seen: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GuestInfo {
    pub invite: String,
    pub notebook: String,
    pub name: String,
    /// The guest's id (its presence and live strokes carry it).
    pub device: String,
    pub role: Role,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InviteInfo {
    pub id: String,
    pub notebook: String,
    pub title: String,
    pub role: Role,
    pub expires: u64,
    pub created: u64,
    pub revoked: bool,
    pub link: String,
    /// Guests connected with it right now.
    pub online: usize,
    /// New guests wait until the owner lets them in.
    pub approve: bool,
    /// Only one device may come in with it.
    pub single: bool,
    pub pin: Option<String>,
    /// Devices that came in so far.
    pub members: usize,
    /// Devices the owner removed.
    pub banned: Vec<Banned>,
}

/// How a new invite lets guests in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InviteOptions {
    /// New guests wait until the owner lets them in ([`Engine::admit`]).
    pub approve: bool,
    /// Only the first device may come in.
    pub single: bool,
    /// New guests have to give it (devices already in don't).
    pub pin: Option<String>,
}

/// Why a guest was turned away (the app asks for the PIN on these).
pub const PIN_NEEDED: &str = "PIN nötig";
pub const PIN_WRONG: &str = "PIN falsch";

#[derive(Debug, Clone, PartialEq)]
pub struct JoinedInfo {
    pub invite: String,
    pub notebook: String,
    pub title: String,
    pub role: Role,
    pub owner: String,
    pub owner_name: String,
    pub online: bool,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub node_id: String,
    pub devices: Vec<DeviceInfo>,
    pub guests: Vec<GuestInfo>,
    /// New guests waiting to be let in.
    pub waiting: Vec<GuestInfo>,
    pub joined: Vec<JoinedInfo>,
    /// (notebook, unix seconds of the last change received or sent).
    pub last_sync: Vec<(String, u64)>,
    /// The home relay this device is reachable through (None: only directly / on the LAN).
    pub relay: Option<String>,
    /// Direct addresses (LAN / public) known for this device.
    pub direct_addrs: usize,
}

/// Who is on the other end of a session.
#[derive(Debug, Clone, PartialEq)]
enum Kind {
    /// A paired own device: everything syncs both ways.
    Own,
    /// Our guest (we own the notebook).
    Guest { invite: String, notebook: String, role: Role },
    /// The owner of a notebook we joined as a guest.
    Host { invite: String, notebook: String, role: Role },
}

impl Kind {
    fn scope(&self) -> Option<&str> {
        match self {
            Kind::Own => None,
            Kind::Guest { notebook, .. } | Kind::Host { notebook, .. } => Some(notebook),
        }
    }
    fn covers(&self, notebook: &str) -> bool {
        self.scope().is_none_or(|s| s == notebook)
    }
    fn invite(&self) -> Option<&str> {
        match self {
            Kind::Own => None,
            Kind::Guest { invite, .. } | Kind::Host { invite, .. } => Some(invite),
        }
    }
}

/// Where a session sits in [`Inner::sessions`]: the peer, plus the invite for guests and hosts.
/// Two devices share one connection for syncing as own devices, but one per shared notebook
/// (a guest of two notebooks of the same owner: two connections, side by side).
type SessionKey = (EndpointId, Option<String>);

fn own_key(id: EndpointId) -> SessionKey {
    (id, None)
}

struct Session {
    peer: EndpointId,
    name: String,
    kind: Kind,
    conn: Connection,
    tx: mpsc::UnboundedSender<Msg>,
    dialed_by_me: bool,
    /// What the peer has, as far as we know (from its announces and what we sent).
    their: Mutex<HashMap<(String, DocKey), Vec<u8>>>,
    blobs_in: Mutex<HashMap<String, Vec<u8>>>,
    wanted: Mutex<HashSet<String>>,
    ended: Notify,
}

impl Session {
    fn send(&self, m: Msg) {
        let _ = self.tx.send(m);
    }
    fn id(&self) -> String {
        self.peer.to_string()
    }
    fn key(&self) -> SessionKey {
        (self.peer, self.kind.invite().map(str::to_string))
    }
}

struct Inner {
    root: PathBuf,
    lib: Library,
    ep: Endpoint,
    key: SecretKey,
    name: String,
    guest_ignores_role: bool,
    /// Whether guests get our presence (where we look, the laser); own devices always do.
    share_view: AtomicBool,
    emit: Box<dyn Fn(Event) + Send + Sync>,
    state: Mutex<State>,
    sessions: Mutex<HashMap<SessionKey, Arc<Session>>>,
    /// Own devices with a running dial loop.
    dialing: Mutex<HashSet<EndpointId>>,
    /// Joined invites with a running host loop (one each: two would take turns forever).
    hosting: Mutex<HashSet<String>>,
    dirty: Mutex<HashSet<String>>,
    dirty_notify: Notify,
    vv_cache: Mutex<HashMap<PathBuf, (SystemTime, u64, Vec<u8>)>>,
    last_sync: Mutex<HashMap<String, u64>>,
    /// Change stamps (see `Library::change_stamp`) last seen per notebook.
    stamps: Mutex<HashMap<String, u64>>,
    closed: AtomicBool,
    shutdown: Notify,
    /// New guests waiting for the owner (invites with approval).
    lobby: Mutex<Vec<Knock>>,
}

/// A guest at the door: let in (true) or turned away (false) through `tx`.
struct Knock {
    invite: String,
    notebook: String,
    name: String,
    device: EndpointId,
    role: Role,
    tx: tokio::sync::oneshot::Sender<bool>,
}

/// The running sync engine. Methods block briefly (they hand work to the engine's runtime).
pub struct Engine {
    rt: Option<tokio::runtime::Runtime>,
    inner: Arc<Inner>,
    events: Mutex<Option<std::sync::mpsc::Receiver<Event>>>,
}

fn empty_vv() -> Vec<u8> {
    VersionVector::new().encode()
}

/// Whether `a` lacks something `b` has (version vectors, Loro encoding).
fn lacks(a: &[u8], b: &[u8]) -> bool {
    if b.is_empty() {
        return false;
    }
    if a.is_empty() {
        return true;
    }
    match (VersionVector::decode(a), VersionVector::decode(b)) {
        (Ok(a), Ok(b)) => !matches!(a.partial_cmp(&b), Some(std::cmp::Ordering::Greater | std::cmp::Ordering::Equal)),
        _ => true,
    }
}

fn merged(a: &[u8], b: &[u8]) -> Vec<u8> {
    match (VersionVector::decode(a), VersionVector::decode(b)) {
        (Ok(mut a), Ok(b)) => {
            a.merge(&b);
            a.encode()
        }
        (Err(_), _) => b.to_vec(),
        (_, Err(_)) => a.to_vec(),
    }
}

fn parse_id(s: &str) -> Result<EndpointId> {
    EndpointId::from_str(s).map_err(|_| Error::Invalid(format!("unbekanntes Gerät {s}")))
}

impl Engine {
    /// Starts the engine for the library at `root` (the notes app's library directory).
    pub fn start(root: impl AsRef<Path>, opts: EngineOptions) -> Result<Engine> {
        let root = root.as_ref().to_path_buf();
        let rt = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().thread_name("n0te-sync").build()?;
        let lib = Library::open_as(&root, "sync")?;
        let key = state::load_or_create_key(&root)?;
        let state = State::load(&root)?;
        let ep = rt.block_on(bind(key.clone(), opts.local_only))?;
        let (events_tx, events_rx) = std::sync::mpsc::channel();
        let emit: Box<dyn Fn(Event) + Send + Sync> = match opts.listener {
            Some(l) => Box::new(move |e| l(e)),
            None => {
                let tx = Mutex::new(events_tx);
                Box::new(move |e| {
                    let _ = tx.lock().expect("events").send(e);
                })
            }
        };
        let inner = Arc::new(Inner {
            root,
            lib,
            ep,
            key,
            name: opts.device_name,
            guest_ignores_role: opts.guest_ignores_role,
            share_view: AtomicBool::new(true),
            emit,
            state: Mutex::new(state),
            sessions: Mutex::new(HashMap::new()),
            dialing: Mutex::new(HashSet::new()),
            hosting: Mutex::new(HashSet::new()),
            dirty: Mutex::new(HashSet::new()),
            dirty_notify: Notify::new(),
            vv_cache: Mutex::new(HashMap::new()),
            last_sync: Mutex::new(HashMap::new()),
            stamps: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
            shutdown: Notify::new(),
            lobby: Mutex::new(Vec::new()),
        });
        // Changes made before we started count as seen; the first announce covers them.
        for id in inner.lib.notebook_ids().unwrap_or_default() {
            let s = inner.lib.change_stamp(&id);
            inner.stamps.lock().expect("stamps").insert(id, s);
        }
        rt.spawn(accept_loop(inner.clone()));
        rt.spawn(flush_loop(inner.clone()));
        rt.spawn(watch_loop(inner.clone()));
        let devices: Vec<EndpointId> = inner.state.lock().expect("state").devices.iter().filter_map(|d| parse_id(&d.id).ok()).collect();
        for id in devices {
            spawn_dial(&rt, &inner, id, None);
        }
        let joined: Vec<String> = {
            let mut st = inner.state.lock().expect("state");
            // One membership per notebook and owner (0.12.0 could keep two; they blocked each
            // other as duplicate connections): the newest — the last joined — stays.
            let before = st.joined.len();
            let mut seen = HashSet::new();
            let mut keep: Vec<JoinedRecord> = st.joined.drain(..).rev().filter(|j| seen.insert((j.notebook.clone(), j.owner.clone()))).collect();
            keep.reverse();
            st.joined = keep;
            if st.joined.len() != before {
                let _ = st.save_joined();
            }
            st.joined.iter().filter(|j| !j.revoked).map(|j| j.invite.clone()).collect()
        };
        for invite in joined {
            spawn_host(&rt, &inner, invite, None);
        }
        Ok(Engine { rt: Some(rt), inner, events: Mutex::new(Some(events_rx)) })
    }

    fn rt(&self) -> &tokio::runtime::Runtime {
        self.rt.as_ref().expect("running")
    }

    /// Events, when no listener was given (take once).
    pub fn events(&self) -> Option<std::sync::mpsc::Receiver<Event>> {
        self.events.lock().expect("events").take()
    }

    pub fn node_id(&self) -> String {
        self.inner.ep.id().to_string()
    }

    /// This device's current address (endpoint id, relay, direct addresses).
    pub fn addr(&self) -> EndpointAddr {
        self.inner.ep.addr()
    }

    /// A one-time code (valid 15 minutes) for pairing another own device.
    pub fn pair_code(&self) -> Result<String> {
        let inner = &self.inner;
        // Wait briefly for a relay so the code works beyond the LAN.
        let ep = inner.ep.clone();
        let _ = self.rt().block_on(async move { tokio::time::timeout(Duration::from_secs(4), ep.online()).await });
        let mut secret = [0u8; 32];
        rand::Rng::fill(&mut rand::rng(), &mut secret);
        {
            let mut st = inner.state.lock().expect("state");
            st.reload_pending();
            let now = now_secs();
            st.pending.retain(|p| p.expires > now);
            st.pending.push(PendingPair { secret, expires: now + 15 * 60 });
            st.save_pending()?;
        }
        Ok(PairCode { addr: inner.ep.addr(), secret, name: inner.name.clone() }.to_text())
    }

    /// Pairs with the device that made `code`; both sides trust each other from now on.
    pub fn accept_pair(&self, code: &str) -> Result<DeviceInfo> {
        let code = PairCode::parse(code)?;
        let inner = self.inner.clone();
        if code.addr.id == inner.ep.id() {
            return Err(Error::Invalid("Das ist der Code dieses Geräts".into()));
        }
        let proof = proto::pair_proof(&code.secret, &code.addr.id, &inner.ep.id());
        let est = self.rt().block_on(connect(&inner, code.addr.clone(), Auth::Pair { proof }))?;
        let device = DeviceRecord { id: code.addr.id.to_string(), name: est.name.clone(), addr: code.addr.clone(), paired: now_secs(), last_seen: now_secs() };
        let info = inner.add_device(device);
        (inner.emit)(Event::Paired { device: info.clone() });
        spawn_dial(self.rt(), &inner, code.addr.id, Some(est));
        Ok(info)
    }

    pub fn devices(&self) -> Vec<DeviceInfo> {
        self.inner.device_infos()
    }

    /// Forgets an own device (it can no longer sync with this one).
    pub fn remove_device(&self, id: &str) -> Result<()> {
        let pid = parse_id(id)?;
        {
            let mut st = self.inner.state.lock().expect("state");
            st.devices.retain(|d| d.id != id);
            st.save_devices()?;
        }
        if let Some(s) = self.inner.sessions.lock().expect("sessions").remove(&own_key(pid)) {
            s.conn.close(4u32.into(), b"unpaired");
        }
        Ok(())
    }

    pub fn status(&self) -> Status {
        let inner = &self.inner;
        let sessions = inner.sessions.lock().expect("sessions");
        let guests = sessions
            .values()
            .filter_map(|s| match &s.kind {
                Kind::Guest { invite, notebook, role } => Some(GuestInfo { invite: invite.clone(), notebook: notebook.clone(), name: s.name.clone(), device: s.id(), role: *role }),
                _ => None,
            })
            .collect();
        let online_hosts: HashSet<String> = sessions
            .values()
            .filter_map(|s| match &s.kind {
                Kind::Host { invite, .. } => Some(invite.clone()),
                _ => None,
            })
            .collect();
        drop(sessions);
        let waiting = inner
            .lobby
            .lock()
            .expect("lobby")
            .iter()
            .map(|k| GuestInfo { invite: k.invite.clone(), notebook: k.notebook.clone(), name: k.name.clone(), device: k.device.to_string(), role: k.role })
            .collect();
        let st = inner.state.lock().expect("state");
        let joined = st
            .joined
            .iter()
            .map(|j| JoinedInfo { invite: j.invite.clone(), notebook: j.notebook.clone(), title: j.title.clone(), role: j.role, owner: j.owner.clone(), owner_name: j.owner_name.clone(), online: online_hosts.contains(&j.invite), revoked: j.revoked })
            .collect();
        drop(st);
        let mut last_sync: Vec<(String, u64)> = inner.last_sync.lock().expect("sync").iter().map(|(k, v)| (k.clone(), *v)).collect();
        last_sync.sort();
        let addr = inner.ep.addr();
        let relay = addr.relay_urls().next().map(|u| u.to_string());
        let direct_addrs = addr.ip_addrs().count();
        Status { node_id: inner.ep.id().to_string(), devices: inner.device_infos(), guests, waiting, joined, last_sync, relay, direct_addrs }
    }

    /// Tell the engine a notebook was saved here (it also notices by itself within a second).
    pub fn notify_local_change(&self, notebook: &str) {
        self.inner.mark_dirty(notebook);
    }

    /// Where this device looks in `notebook` (viewport in page points: x0, y0, x1, y1) and its
    /// laser pointer, to all devices (and guests) with that notebook.
    pub fn send_presence(&self, notebook: &str, sheet: Option<&str>, viewport: [f32; 4], zoom: f32, laser: Option<[f32; 2]>) {
        let live = Live::Presence { notebook: notebook.into(), sheet: sheet.map(Into::into), viewport, zoom, laser };
        if self.inner.share_view.load(Ordering::Relaxed) {
            self.inner.send_live(notebook, None, live);
        } else {
            // Not to our guests: only own devices (and, as a guest, the owner).
            let bytes = proto::encode(&live);
            for s in self.inner.sessions_for(notebook).into_iter().filter(|s| !matches!(s.kind, Kind::Guest { .. })) {
                s.send(Msg::Live(bytes.clone()));
            }
        }
    }

    /// Whether our guests see where we look (presence); own devices always do. Turned off,
    /// guests get a last "looking nowhere" (no sheet), so our frame goes from their screen.
    pub fn set_share_view(&self, on: bool) {
        let was = self.inner.share_view.swap(on, Ordering::Relaxed);
        if was && !on {
            let sessions: Vec<Arc<Session>> = self.inner.sessions.lock().expect("sessions").values().cloned().collect();
            for s in sessions {
                if let Kind::Guest { notebook, .. } = &s.kind {
                    s.send(Msg::Live(proto::encode(&Live::Presence { notebook: notebook.clone(), sheet: None, viewport: [0.0; 4], zoom: 1.0, laser: None })));
                }
            }
        }
    }

    /// Changes an invite's rights. Its guests reconnect at once and get the new rights (their
    /// app is told: [`Event::RoleChanged`]).
    pub fn set_role(&self, invite: &str, role: Role) -> Result<()> {
        {
            let mut st = self.inner.state.lock().expect("state");
            let r = st.invites.iter_mut().find(|r| r.id == invite && !r.revoked).ok_or_else(|| Error::Invalid(format!("keine Einladung {invite}")))?;
            if r.role == role {
                return Ok(());
            }
            r.role = role;
            st.save_invites()?;
        }
        let removed: Vec<Arc<Session>> = {
            let mut sessions = self.inner.sessions.lock().expect("sessions");
            let gone: Vec<SessionKey> = sessions.iter().filter(|(_, s)| matches!(&s.kind, Kind::Guest { invite: i, .. } if i == invite)).map(|(k, _)| k.clone()).collect();
            gone.into_iter().filter_map(|k| sessions.remove(&k)).collect()
        };
        for s in removed {
            s.conn.close(4u32.into(), b"role");
            if let Kind::Guest { invite, notebook, .. } = &s.kind {
                (self.inner.emit)(Event::GuestLeft { invite: invite.clone(), notebook: notebook.clone(), device: s.id() });
            }
        }
        Ok(())
    }

    /// New samples of a stroke being drawn (packed LE f32 x, y, pressure, t_ms; sheet points).
    #[allow(clippy::too_many_arguments)]
    pub fn send_live_stroke(&self, notebook: &str, sheet: &str, stroke: u64, seq: u32, brush: LiveBrush, samples: &[u8], done: bool) {
        // Split into datagram-sized pieces (16 bytes per sample).
        let per = 60 * 16;
        let chunks: Vec<&[u8]> = if samples.is_empty() { vec![&[][..]] } else { samples.chunks(per).collect() };
        let n = chunks.len();
        for (i, c) in chunks.into_iter().enumerate() {
            let live = Live::Stroke { notebook: notebook.into(), sheet: sheet.into(), stroke, seq: seq.wrapping_mul(1024).wrapping_add(i as u32), brush, samples: c.to_vec(), done: done && i + 1 == n };
            self.inner.send_live(notebook, None, live);
        }
    }

    /// Erasing as it happens on `sheet`: strokes just hidden by the stroke eraser and / or new
    /// area-eraser dabs (x, y, radius). The stored result follows with the next save.
    pub fn send_live_erase(&self, notebook: &str, sheet: &str, ids: &[String], dabs: &[[f32; 3]]) {
        // Split so each message stays datagram-sized.
        for chunk in ids.chunks(40) {
            self.inner.send_live(notebook, None, Live::Erase { notebook: notebook.into(), sheet: sheet.into(), ids: chunk.to_vec(), dabs: Vec::new() });
        }
        for chunk in dabs.chunks(60) {
            self.inner.send_live(notebook, None, Live::Erase { notebook: notebook.into(), sheet: sheet.into(), ids: Vec::new(), dabs: chunk.to_vec() });
        }
    }

    /// Asks an own device to open a notebook (live session).
    pub fn request_open(&self, device: &str, notebook: &str) -> Result<()> {
        self.own_session(device)?.send(Msg::Open { notebook: notebook.into() });
        Ok(())
    }

    /// Asks an own device (the laptop) to read handwriting as LaTeX (answer: `Event::LatexResult`).
    pub fn request_latex(&self, device: &str, id: u64, png: &[u8]) -> Result<()> {
        self.own_session(device)?.send(Msg::LatexRequest { id, png: png.to_vec() });
        Ok(())
    }

    /// Answers a LaTeX request.
    pub fn send_latex_result(&self, device: &str, id: u64, latex: &str, engine: &str, error: Option<&str>) -> Result<()> {
        self.own_session(device)?.send(Msg::LatexResult { id, latex: latex.into(), engine: engine.into(), error: error.map(Into::into) });
        Ok(())
    }

    /// Asks an own device for a picture of its n0te screen (arrives as `Event::Screenshot`).
    pub fn request_screenshot(&self, device: &str) -> Result<()> {
        self.own_session(device)?.send(Msg::Shot);
        Ok(())
    }

    /// Answers a screenshot request (JPEG of this app's screen).
    pub fn send_screenshot(&self, device: &str, image: &[u8]) -> Result<()> {
        self.own_session(device)?.send(Msg::ShotData { image: image.to_vec() });
        Ok(())
    }

    /// Asks an own device to follow (or stop following) this device's view.
    /// Brings guests' views to ours: one guest (`device`) or all guests of `notebook`. Returns
    /// how many were reached. Their app jumps there ([`Event::Summoned`]).
    pub fn summon(&self, notebook: &str, device: Option<&str>, sheet: &str, viewport: [f32; 4]) -> Result<usize> {
        let only = device.map(parse_id).transpose()?;
        let msg = Msg::Summon { notebook: notebook.into(), sheet: sheet.into(), viewport };
        let targets: Vec<Arc<Session>> = self
            .inner
            .sessions_for(notebook)
            .into_iter()
            .filter(|s| matches!(s.kind, Kind::Guest { .. }) || (only.is_some() && s.kind == Kind::Own))
            .filter(|s| only.is_none_or(|d| s.peer == d))
            .collect();
        if targets.is_empty() {
            return Err(Error::Net(if only.is_some() { "Gast ist nicht verbunden".into() } else { "Kein Gast verbunden".into() }));
        }
        for s in &targets {
            s.send(msg.clone());
        }
        Ok(targets.len())
    }

    pub fn set_follow(&self, device: &str, on: bool) -> Result<()> {
        self.own_session(device)?.send(Msg::Follow { on });
        Ok(())
    }

    fn own_session(&self, device: &str) -> Result<Arc<Session>> {
        let id = parse_id(device)?;
        self.inner.sessions.lock().expect("sessions").get(&own_key(id)).filter(|s| s.kind == Kind::Own).cloned().ok_or_else(|| Error::Net("Gerät ist nicht verbunden".into()))
    }

    /// An invite link for one notebook. `expires`: unix seconds, 0 = never.
    pub fn create_invite(&self, notebook: &str, role: Role, expires: u64) -> Result<String> {
        self.create_invite_with(notebook, role, expires, InviteOptions::default())
    }

    /// The same with approval, single use and / or a PIN.
    pub fn create_invite_with(&self, notebook: &str, role: Role, expires: u64, opts: InviteOptions) -> Result<String> {
        let inner = &self.inner;
        if inner.lib.is_trashed(notebook) {
            return Err(Error::Invalid("Notizbuch ist im Papierkorb".into()));
        }
        let title = inner.lib.load_notebook(notebook)?.title();
        let ep = inner.ep.clone();
        let _ = self.rt().block_on(async move { tokio::time::timeout(Duration::from_secs(4), ep.online()).await });
        let mut idb = [0u8; 12];
        rand::Rng::fill(&mut rand::rng(), &mut idb);
        let id = data_encoding::HEXLOWER.encode(&idb);
        let invite = Invite { v: 1, id: id.clone(), notebook: notebook.into(), title: title.clone(), role, expires, owner: inner.ep.addr(), owner_name: inner.name.clone() };
        let link = SignedInvite::sign(&invite, &inner.key).to_link();
        let mut st = inner.state.lock().expect("state");
        st.invites.push(InviteRecord {
            id,
            notebook: notebook.into(),
            title,
            role,
            expires,
            created: now_secs(),
            revoked: false,
            link: link.clone(),
            approve: opts.approve,
            single: opts.single,
            pin: opts.pin.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()),
            members: Vec::new(),
            banned: Vec::new(),
        });
        st.save_invites()?;
        Ok(link)
    }

    pub fn list_invites(&self) -> Vec<InviteInfo> {
        let sessions = self.inner.sessions.lock().expect("sessions");
        let online = |id: &str| sessions.values().filter(|s| matches!(&s.kind, Kind::Guest { invite, .. } if invite == id)).count();
        self.inner
            .state
            .lock()
            .expect("state")
            .invites
            .iter()
            .map(|r| InviteInfo {
                id: r.id.clone(),
                notebook: r.notebook.clone(),
                title: r.title.clone(),
                role: r.role,
                expires: r.expires,
                created: r.created,
                revoked: r.revoked,
                link: r.link.clone(),
                online: online(&r.id),
                approve: r.approve,
                single: r.single,
                pin: r.pin.clone(),
                members: r.members.len(),
                banned: r.banned.clone(),
            })
            .collect()
    }

    /// Lets a waiting guest in (`yes`) or turns it away.
    pub fn admit(&self, device: &str, yes: bool) -> Result<()> {
        let id = parse_id(device)?;
        let knock = {
            let mut lobby = self.inner.lobby.lock().expect("lobby");
            let i = lobby.iter().position(|k| k.device == id).ok_or_else(|| Error::Invalid("niemand wartet mit dieser Id".into()))?;
            lobby.remove(i)
        };
        let _ = knock.tx.send(yes);
        Ok(())
    }

    /// Removes one guest device: disconnected, and its invite won't let it back in
    /// ([`Engine::allow_again`] undoes it). The invite stays for everyone else.
    pub fn remove_guest(&self, device: &str) -> Result<()> {
        let id = parse_id(device)?;
        let dev = id.to_string();
        let sessions: Vec<Arc<Session>> = {
            let mut map = self.inner.sessions.lock().expect("sessions");
            let keys: Vec<SessionKey> = map.iter().filter(|(k, s)| k.0 == id && matches!(s.kind, Kind::Guest { .. })).map(|(k, _)| k.clone()).collect();
            keys.iter().filter_map(|k| map.remove(k)).collect()
        };
        let waiting = self.inner.lobby.lock().expect("lobby").iter().find(|k| k.device == id).map(|k| (k.invite.clone(), k.name.clone()));
        {
            let mut st = self.inner.state.lock().expect("state");
            let name_of = |inv: &str| sessions.iter().find(|s| matches!(&s.kind, Kind::Guest { invite, .. } if invite == inv)).map(|s| s.name.clone());
            let mut hit = false;
            for r in st.invites.iter_mut() {
                let here = name_of(&r.id).or_else(|| waiting.as_ref().filter(|(i, _)| *i == r.id).map(|(_, n)| n.clone()));
                if here.is_some() || r.members.contains(&dev) {
                    hit = true;
                    r.members.retain(|m| *m != dev);
                    if !r.banned.iter().any(|b| b.device == dev) {
                        r.banned.push(Banned { device: dev.clone(), name: here.unwrap_or_else(|| "Gast".into()) });
                    }
                }
            }
            if !hit {
                return Err(Error::Invalid("kein Gast mit dieser Id".into()));
            }
            st.save_invites()?;
        }
        if waiting.is_some() {
            let _ = self.admit(&dev, false);
        }
        for s in sessions {
            s.conn.close(5u32.into(), b"removed");
            if let Kind::Guest { invite, notebook, .. } = &s.kind {
                (self.inner.emit)(Event::GuestLeft { invite: invite.clone(), notebook: notebook.clone(), device: s.id() });
            }
        }
        Ok(())
    }

    /// A removed device may come in with `invite` again.
    pub fn allow_again(&self, invite: &str, device: &str) -> Result<()> {
        self.edit_invite(invite, |r| r.banned.retain(|b| b.device != device))
    }

    /// Changes until when an invite works (unix seconds, 0 = never) — also for links already
    /// sent: the owner checks, not the date in the link.
    pub fn set_expiry(&self, invite: &str, expires: u64) -> Result<()> {
        self.edit_invite(invite, |r| r.expires = expires)
    }

    /// Approval and single use of an invite (for devices that aren't in yet).
    pub fn set_invite_options(&self, invite: &str, approve: bool, single: bool) -> Result<()> {
        self.edit_invite(invite, |r| {
            r.approve = approve;
            r.single = single;
        })
    }

    /// The PIN new guests have to give (None: no PIN).
    pub fn set_pin(&self, invite: &str, pin: Option<&str>) -> Result<()> {
        let pin = pin.map(|p| p.trim().to_string()).filter(|p| !p.is_empty());
        self.edit_invite(invite, |r| r.pin = pin.clone())
    }

    /// Changes an invite record; guests connected with it count as in from now on (so new
    /// conditions only ever apply to new devices).
    fn edit_invite(&self, invite: &str, f: impl FnOnce(&mut InviteRecord)) -> Result<()> {
        let here: Vec<String> = self
            .inner
            .sessions
            .lock()
            .expect("sessions")
            .values()
            .filter(|s| matches!(&s.kind, Kind::Guest { invite: i, .. } if i == invite))
            .map(|s| s.id())
            .collect();
        let mut st = self.inner.state.lock().expect("state");
        let r = st.invites.iter_mut().find(|r| r.id == invite).ok_or_else(|| Error::Invalid(format!("keine Einladung {invite}")))?;
        for d in here {
            if !r.members.contains(&d) {
                r.members.push(d);
            }
        }
        f(r);
        st.save_invites()
    }

    /// Revokes an invite: its guests are disconnected and can't come back.
    pub fn revoke(&self, invite: &str) -> Result<()> {
        {
            let mut st = self.inner.state.lock().expect("state");
            let r = st.invites.iter_mut().find(|r| r.id == invite).ok_or_else(|| Error::Invalid(format!("keine Einladung {invite}")))?;
            r.revoked = true;
            st.save_invites()?;
        }
        let removed: Vec<Arc<Session>> = {
            let mut sessions = self.inner.sessions.lock().expect("sessions");
            let gone: Vec<SessionKey> = sessions.iter().filter(|(_, s)| matches!(&s.kind, Kind::Guest { invite: i, .. } if i == invite)).map(|(k, _)| k.clone()).collect();
            gone.into_iter().filter_map(|k| sessions.remove(&k)).collect()
        };
        // Taken out of the map here, so the session's own end won't report it: do it now
        // (outside the lock, listeners may call back in).
        for s in removed {
            s.conn.close(3u32.into(), b"revoked");
            if let Kind::Guest { invite, notebook, .. } = &s.kind {
                (self.inner.emit)(Event::GuestLeft { invite: invite.clone(), notebook: notebook.clone(), device: s.id() });
            }
        }
        Ok(())
    }

    /// Joins a notebook as a guest; it syncs into this library from now on. Returns its id.
    pub fn join(&self, link: &str) -> Result<String> {
        self.join_with(link, None)
    }

    /// Joins with the invite's PIN. With approval this waits (up to 3 minutes) until the
    /// owner lets us in ([`Event::JoinWaiting`] meanwhile). Turned away: `Rejected` with the
    /// reason ([`PIN_NEEDED`], [`PIN_WRONG`], …).
    pub fn join_with(&self, link: &str, pin: Option<&str>) -> Result<String> {
        let signed = SignedInvite::parse_link(link)?;
        let invite = signed.verify()?;
        let inner = self.inner.clone();
        if invite.owner.id == inner.ep.id() {
            return Err(Error::Invalid("Das ist eine eigene Einladung".into()));
        }
        // Expiry is the owner's to decide (it can be changed after the link went out).
        let bytes = proto::encode(&signed);
        let auth = match pin.map(str::trim).filter(|p| !p.is_empty()) {
            Some(p) => Auth::GuestPin { invite: bytes, pin: p.to_string() },
            None => Auth::Guest { invite: bytes },
        };
        // One membership per notebook: an older invite to the same notebook (from the same
        // owner) gives way — its connection would block the new one. Back if this join fails.
        let owner_id = invite.owner.id.to_string();
        let older: Vec<JoinedRecord> = {
            let mut st = inner.state.lock().expect("state");
            let (old, keep): (Vec<JoinedRecord>, Vec<JoinedRecord>) = st.joined.drain(..).partition(|j| j.notebook == invite.notebook && j.owner == owner_id && j.invite != invite.id);
            st.joined = keep;
            if !old.is_empty() {
                st.save_joined()?;
            }
            old
        };
        if !older.is_empty() {
            let mut sessions = inner.sessions.lock().expect("sessions");
            let gone: Vec<SessionKey> = sessions.iter().filter(|(_, s)| matches!(&s.kind, Kind::Host { invite: i, .. } if older.iter().any(|o| o.invite == *i))).map(|(k, _)| k.clone()).collect();
            for k in gone {
                if let Some(s) = sessions.remove(&k) {
                    s.conn.close(0u32.into(), b"replaced");
                }
            }
        }
        let est = match self.rt().block_on(connect(&inner, invite.owner.clone(), auth)) {
            Ok(est) => est,
            Err(e) => {
                if !older.is_empty() {
                    let mut st = inner.state.lock().expect("state");
                    st.joined.extend(older.iter().cloned());
                    let _ = st.save_joined();
                    drop(st);
                    for o in older {
                        spawn_host(self.rt(), &inner, o.invite, None);
                    }
                }
                return Err(e);
            }
        };
        {
            let mut st = inner.state.lock().expect("state");
            st.joined.retain(|j| j.invite != invite.id);
            st.joined.push(JoinedRecord {
                link: link.trim().to_string(),
                invite: invite.id.clone(),
                notebook: invite.notebook.clone(),
                title: invite.title.clone(),
                role: invite.role,
                owner: invite.owner.id.to_string(),
                owner_name: if est.name.is_empty() { invite.owner_name.clone() } else { est.name.clone() },
                revoked: false,
            });
            st.save_joined()?;
        }
        spawn_host(self.rt(), &inner, invite.id, Some(est));
        Ok(invite.notebook)
    }

    pub fn joined(&self) -> Vec<JoinedInfo> {
        self.status().joined
    }

    /// Leaves a joined notebook (the local copy stays).
    pub fn leave(&self, invite: &str) -> Result<()> {
        let mut st = self.inner.state.lock().expect("state");
        st.joined.retain(|j| j.invite != invite);
        st.save_joined()?;
        drop(st);
        let mut sessions = self.inner.sessions.lock().expect("sessions");
        let gone: Vec<SessionKey> = sessions.iter().filter(|(_, s)| matches!(&s.kind, Kind::Host { invite: i, .. } if i == invite)).map(|(k, _)| k.clone()).collect();
        for k in gone {
            if let Some(s) = sessions.remove(&k) {
                s.conn.close(0u32.into(), b"left");
            }
        }
        Ok(())
    }

    /// The checkpoints of a notebook (see [`crate::history`]).
    pub fn history(&self, notebook: &str) -> Vec<crate::history::Checkpoint> {
        crate::history::list(&self.inner.root, notebook)
    }

    /// Restores a checkpoint as a new notebook; returns its id.
    pub fn restore_as_copy(&self, notebook: &str, at: u64) -> Result<String> {
        let id = crate::history::restore_as_copy(&self.inner.lib, notebook, at)?;
        self.inner.mark_dirty(&id);
        Ok(id)
    }

    /// Writes checkpoints of changed notebooks now (the service does this periodically).
    pub fn checkpoint(&self) -> crate::history::Pass {
        crate::history::checkpoint_changed(&self.inner.lib, now_secs())
    }

    /// Stops all sessions and closes the endpoint.
    pub fn shutdown(&mut self) {
        let inner = self.inner.clone();
        if inner.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        inner.shutdown.notify_waiters();
        for s in inner.sessions.lock().expect("sessions").drain().map(|(_, s)| s) {
            s.conn.close(0u32.into(), b"shutdown");
        }
        if let Some(rt) = self.rt.take() {
            let ep = inner.ep.clone();
            rt.block_on(async move {
                let _ = tokio::time::timeout(Duration::from_secs(2), ep.close()).await;
            });
            rt.shutdown_timeout(Duration::from_secs(1));
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

async fn bind(key: SecretKey, local_only: bool) -> Result<Endpoint> {
    let b = if local_only {
        Endpoint::builder(presets::Minimal).relay_mode(RelayMode::Disabled).bind_addr("127.0.0.1:0").map_err(|e| Error::Net(e.to_string()))?
    } else {
        Endpoint::builder(presets::N0)
    };
    b.secret_key(key).alpns(vec![ALPN.to_vec()]).bind().await.map_err(|e| Error::Net(e.to_string()))
}

/// A handshaken connection, before its session runs.
struct Established {
    conn: Connection,
    send: SendStream,
    recv: RecvStream,
    name: String,
    role: Option<Role>,
}

/// Dials, says hello, waits for the welcome.
async fn connect(inner: &Arc<Inner>, addr: EndpointAddr, auth: Auth) -> Result<Established> {
    let conn = tokio::time::timeout(Duration::from_secs(20), inner.ep.connect(addr, ALPN))
        .await
        .map_err(|_| Error::Net("Gerät nicht erreichbar".into()))?
        .map_err(|e| Error::Net(e.to_string()))?;
    let (mut send, mut recv) = conn.open_bi().await.map_err(|e| Error::Net(e.to_string()))?;
    proto::write_msg(&mut send, &Msg::Hello { proto: PROTO, name: inner.name.clone(), addr: inner.ep.addr(), auth }).await?;
    let mut reply = tokio::time::timeout(Duration::from_secs(15), proto::read_msg(&mut recv)).await.map_err(|_| Error::Net("keine Antwort".into()))??;
    if let Msg::Waiting { name } = reply {
        // The owner lets new guests in by hand: wait for it (a few minutes at most).
        (inner.emit)(Event::JoinWaiting { owner: name });
        reply = tokio::time::timeout(Duration::from_secs(180), proto::read_msg(&mut recv))
            .await
            .map_err(|_| Error::Rejected("Der Besitzer hat dich nicht hereingelassen (keine Antwort)".into()))??;
    }
    match reply {
        Msg::Welcome { name, role, .. } => Ok(Established { conn, send, recv, name, role }),
        Msg::Reject { reason } => {
            conn.close(0u32.into(), b"rejected");
            Err(Error::Rejected(reason))
        }
        _ => Err(Error::Protocol("unerwartete Antwort".into())),
    }
}

fn spawn_dial(rt: &tokio::runtime::Runtime, inner: &Arc<Inner>, id: EndpointId, first: Option<Established>) {
    if !inner.dialing.lock().expect("dialing").insert(id) {
        // A loop already runs; still use a fresh pairing connection as a session.
        if let Some(est) = first {
            let inner = inner.clone();
            rt.spawn(async move {
                let _ = run_session(inner, id, Kind::Own, est, true).await;
            });
        }
        return;
    }
    rt.spawn(dial_loop(inner.clone(), id, first));
}

/// A session that lasted this long was a real one: the next redial starts quick again. One
/// that ended sooner (replaced, refused as a duplicate, dropped at once) keeps the wait
/// growing, so two connections can never take turns every second.
const STEADY: Duration = Duration::from_secs(20);

/// Keeps an own device connected (redials with backoff while it is paired).
async fn dial_loop(inner: Arc<Inner>, id: EndpointId, mut first: Option<Established>) {
    let mut backoff = Duration::from_secs(1);
    loop {
        if inner.closed.load(Ordering::SeqCst) {
            break;
        }
        let Some(addr) = inner.device_addr(&id) else { break };
        let existing = inner.sessions.lock().expect("sessions").get(&own_key(id)).cloned();
        if let Some(s) = existing {
            tokio::select! {
                _ = s.ended.notified() => {}
                _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                _ = inner.shutdown.notified() => break,
            }
            continue;
        }
        let est = match first.take() {
            Some(e) => Ok(e),
            None => connect(&inner, addr, Auth::Own).await,
        };
        match est {
            Ok(est) => {
                let began = std::time::Instant::now();
                let _ = run_session(inner.clone(), id, Kind::Own, est, true).await;
                if began.elapsed() >= STEADY {
                    backoff = Duration::from_secs(1);
                }
            }
            Err(Error::Rejected(reason)) => (inner.emit)(Event::SyncError { message: format!("Gerät lehnt ab: {reason}") }),
            Err(_) => {}
        }
        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = inner.shutdown.notified() => break,
        }
        backoff = (backoff * 2).min(Duration::from_secs(60));
    }
    inner.dialing.lock().expect("dialing").remove(&id);
}

/// Starts the loop that keeps joined `invite` connected, unless one runs already (joined
/// again): then a fresh connection (`first`) becomes the session and the loop waits for it.
fn spawn_host(rt: &tokio::runtime::Runtime, inner: &Arc<Inner>, invite: String, first: Option<Established>) {
    if !inner.hosting.lock().expect("hosting").insert(invite.clone()) {
        if let Some(est) = first {
            let inner = inner.clone();
            rt.spawn(async move {
                let record = inner.state.lock().expect("state").joined.iter().find(|j| j.invite == invite && !j.revoked).cloned();
                if let Some(record) = record {
                    host_session(&inner, &record, est).await;
                }
            });
        }
        return;
    }
    let inner2 = inner.clone();
    rt.spawn(async move {
        host_loop(inner2.clone(), invite.clone(), first).await;
        inner2.hosting.lock().expect("hosting").remove(&invite);
    });
}

/// Runs a session with the owner of joined notebook `record` (its rights may have changed).
async fn host_session(inner: &Arc<Inner>, record: &JoinedRecord, est: Established) {
    let owner = est.conn.remote_id();
    let role = est.role.unwrap_or(record.role);
    if role != record.role {
        // The owner changed our rights: remember them and tell the app.
        let mut st = inner.state.lock().expect("state");
        if let Some(j) = st.joined.iter_mut().find(|j| j.invite == record.invite) {
            j.role = role;
        }
        let _ = st.save_joined();
        drop(st);
        (inner.emit)(Event::RoleChanged { notebook: record.notebook.clone(), write: role == Role::ReadWrite });
    }
    let kind = Kind::Host { invite: record.invite.clone(), notebook: record.notebook.clone(), role };
    let _ = run_session(inner.clone(), owner, kind, est, true).await;
}

/// Keeps a joined notebook connected to its owner (one loop per invite, see [`spawn_host`]).
async fn host_loop(inner: Arc<Inner>, invite: String, mut first: Option<Established>) {
    let mut backoff = Duration::from_secs(1);
    loop {
        if inner.closed.load(Ordering::SeqCst) {
            break;
        }
        let record = inner.state.lock().expect("state").joined.iter().find(|j| j.invite == invite && !j.revoked).cloned();
        let Some(record) = record else { break };
        // Connected already (joined again meanwhile): wait for that session to end.
        if first.is_none()
            && let Ok(owner) = parse_id(&record.owner)
        {
            let existing = inner.sessions.lock().expect("sessions").get(&(owner, Some(invite.clone()))).cloned();
            if let Some(s) = existing {
                tokio::select! {
                    _ = s.ended.notified() => {}
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                    _ = inner.shutdown.notified() => break,
                }
                continue;
            }
        }
        let est = match first.take() {
            Some(e) => Ok(e),
            None => match SignedInvite::parse_link(&record.link).and_then(|s| s.verify().map(|i| (s, i))) {
                Ok((signed, inv)) => connect(&inner, inv.owner, Auth::Guest { invite: proto::encode(&signed) }).await,
                Err(e) => Err(e),
            },
        };
        match est {
            Ok(est) => {
                let began = std::time::Instant::now();
                host_session(&inner, &record, est).await;
                if began.elapsed() >= STEADY {
                    backoff = Duration::from_secs(1);
                }
            }
            Err(Error::Rejected(reason)) => {
                // Revoked or expired: stop for good.
                let mut st = inner.state.lock().expect("state");
                if let Some(j) = st.joined.iter_mut().find(|j| j.invite == invite) {
                    j.revoked = true;
                }
                let _ = st.save_joined();
                drop(st);
                (inner.emit)(Event::AccessRevoked { notebook: record.notebook.clone() });
                (inner.emit)(Event::SyncError { message: format!("„{}“: {reason}", record.title) });
                break;
            }
            Err(_) => {}
        }
        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = inner.shutdown.notified() => break,
        }
        backoff = (backoff * 2).min(Duration::from_secs(60));
    }
}

async fn accept_loop(inner: Arc<Inner>) {
    while let Some(incoming) = inner.ep.accept().await {
        let inner = inner.clone();
        tokio::spawn(async move {
            let Ok(conn) = incoming.await else { return };
            let _ = accept_one(inner, conn).await;
        });
    }
}

async fn accept_one(inner: Arc<Inner>, conn: Connection) -> Result<()> {
    let (mut send, mut recv) = tokio::time::timeout(Duration::from_secs(15), conn.accept_bi()).await.map_err(|_| Error::Net("kein Stream".into()))?.map_err(|e| Error::Net(e.to_string()))?;
    let hello = tokio::time::timeout(Duration::from_secs(15), proto::read_msg(&mut recv)).await.map_err(|_| Error::Net("kein Hello".into()))??;
    let Msg::Hello { proto: p, name, addr, auth } = hello else { return Err(Error::Protocol("Hello erwartet".into())) };
    let peer = conn.remote_id();
    let reject = |reason: &str| Msg::Reject { reason: reason.to_string() };
    if p != PROTO {
        proto::write_msg(&mut send, &reject("andere n0te-Version")).await?;
        return Ok(());
    }
    let mut needs_ok = false;
    let decision: std::result::Result<Kind, String> = match auth {
        Auth::Own => {
            if inner.is_device(&peer) {
                inner.touch_device(&peer, &name, &addr);
                Ok(Kind::Own)
            } else {
                Err("nicht gekoppelt".into())
            }
        }
        Auth::Pair { proof } => {
            let mut st = inner.state.lock().expect("state");
            st.reload_pending();
            let now = now_secs();
            let me = inner.ep.id();
            let hit = st.pending.iter().position(|pp| pp.expires > now && proto::pair_proof(&pp.secret, &me, &peer) == proof);
            match hit {
                Some(i) => {
                    st.pending.remove(i);
                    let _ = st.save_pending();
                    drop(st);
                    let info = inner.add_device(DeviceRecord { id: peer.to_string(), name: name.clone(), addr: addr.clone(), paired: now, last_seen: now });
                    (inner.emit)(Event::Paired { device: info });
                    Ok(Kind::Own)
                }
                None => Err("Kopplung fehlgeschlagen (Code falsch oder abgelaufen)".into()),
            }
        }
        Auth::Guest { invite } => inner.check_invite(&invite).and_then(|(inv, rec)| inner.gate(&rec, &peer, None).map(|wait| (Kind::Guest { invite: inv.id, notebook: rec.notebook, role: rec.role }, wait))).map(|(k, wait)| {
            needs_ok = wait;
            k
        }),
        Auth::GuestPin { invite, pin } => inner.check_invite(&invite).and_then(|(inv, rec)| inner.gate(&rec, &peer, Some(&pin)).map(|wait| (Kind::Guest { invite: inv.id, notebook: rec.notebook, role: rec.role }, wait))).map(|(k, wait)| {
            needs_ok = wait;
            k
        }),
    };
    // A new guest of an invite with approval: wait at the door until the owner decides.
    let decision = match decision {
        Ok(Kind::Guest { invite, notebook, role }) if needs_ok => {
            let (tx, rx) = tokio::sync::oneshot::channel();
            inner.lobby.lock().expect("lobby").push(Knock { invite: invite.clone(), notebook: notebook.clone(), name: name.clone(), device: peer, role, tx });
            (inner.emit)(Event::GuestWaiting { invite: invite.clone(), notebook: notebook.clone(), name: name.clone(), device: peer.to_string(), on: true });
            let _ = proto::write_msg(&mut send, &Msg::Waiting { name: inner.name.clone() }).await;
            let answer = tokio::select! {
                a = rx => a.ok(),
                _ = conn.closed() => None,
                _ = tokio::time::sleep(Duration::from_secs(200)) => None,
            };
            inner.lobby.lock().expect("lobby").retain(|k| !(k.device == peer && k.invite == invite));
            (inner.emit)(Event::GuestWaiting { invite: invite.clone(), notebook: notebook.clone(), name: name.clone(), device: peer.to_string(), on: false });
            match answer {
                Some(true) => Ok(Kind::Guest { invite, notebook, role }),
                Some(false) => Err("Der Besitzer hat dich nicht hereingelassen".to_string()),
                None => return Ok(()),
            }
        }
        other => other,
    };
    // In: remember the device (it comes back without the checks); single use binds here.
    let decision = match decision {
        Ok(Kind::Guest { invite, notebook, role }) => inner.add_member(&invite, &peer).map(|_| Kind::Guest { invite, notebook, role }),
        other => other,
    };
    let kind = match decision {
        Ok(k) => k,
        Err(reason) => {
            proto::write_msg(&mut send, &reject(&reason)).await?;
            // Let the reject reach the peer before the connection goes.
            tokio::time::sleep(Duration::from_millis(200)).await;
            conn.close(1u32.into(), b"rejected");
            return Ok(());
        }
    };
    let (scope, role) = match &kind {
        Kind::Guest { notebook, role, .. } => (Some(notebook.clone()), Some(*role)),
        _ => (None, None),
    };
    proto::write_msg(&mut send, &Msg::Welcome { name: inner.name.clone(), scope: scope.clone(), role }).await?;
    if kind == Kind::Own {
        // An own device dialing us also gets a dial loop here (to come back after drops).
        inner.ensure_dial_loop(peer);
    }
    let est = Established { conn, send, recv, name, role };
    run_session(inner, peer, kind, est, false).await
}

/// Runs a session until the connection ends.
async fn run_session(inner: Arc<Inner>, peer: EndpointId, kind: Kind, est: Established, dialed_by_me: bool) -> Result<()> {
    let Established { conn, mut send, mut recv, name, .. } = est;
    let (tx, mut rx) = mpsc::unbounded_channel::<Msg>();
    let session = Arc::new(Session {
        peer,
        name,
        kind: kind.clone(),
        conn: conn.clone(),
        tx,
        dialed_by_me,
        their: Mutex::new(HashMap::new()),
        blobs_in: Mutex::new(HashMap::new()),
        wanted: Mutex::new(HashSet::new()),
        ended: Notify::new(),
    });
    if !inner.register(&session) {
        conn.close(2u32.into(), b"duplicate");
        return Ok(());
    }
    match &kind {
        Kind::Own => (inner.emit)(Event::DeviceOnline { device: inner.device_info(&peer, true) }),
        Kind::Guest { invite, notebook, .. } => (inner.emit)(Event::GuestJoined { invite: invite.clone(), notebook: notebook.clone(), name: session.name.clone(), device: session.id() }),
        Kind::Host { notebook, .. } => (inner.emit)(Event::HostOnline { notebook: notebook.clone(), online: true }),
    }
    // Writer.
    let writer = tokio::spawn(async move {
        while let Some(m) = rx.recv().await {
            if proto::write_msg(&mut send, &m).await.is_err() {
                break;
            }
        }
    });
    // Datagrams (live).
    let dg_inner = inner.clone();
    let dg_session = session.clone();
    let datagrams = tokio::spawn(async move {
        while let Ok(d) = dg_session.conn.read_datagram().await {
            if let Ok(live) = proto::decode::<Live>(&d) {
                dg_inner.on_live(&dg_session, live);
            }
        }
    });
    // Start: tell the peer what we have.
    let notebooks: Vec<String> = match kind.scope() {
        Some(nb) => vec![nb.to_string()],
        None => inner.lib.notebook_ids().unwrap_or_default().into_iter().filter(|id| !inner.lib.is_trashed(id)).collect(),
    };
    for nb in notebooks {
        let state = inner.my_state(&nb);
        session.send(Msg::Announce { state, want_reply: true });
    }
    // Reader.
    loop {
        let msg = tokio::select! {
            m = proto::read_msg(&mut recv) => m,
            _ = inner.shutdown.notified() => break,
        };
        let Ok(msg) = msg else { break };
        inner.on_msg(&session, msg);
    }
    writer.abort();
    datagrams.abort();
    conn.close(0u32.into(), b"bye");
    inner.unregister(&session);
    Ok(())
}

/// Announces notebooks that changed here.
async fn flush_loop(inner: Arc<Inner>) {
    loop {
        tokio::select! {
            _ = inner.dirty_notify.notified() => {}
            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
            _ = inner.shutdown.notified() => break,
        }
        if inner.closed.load(Ordering::SeqCst) {
            break;
        }
        // Coalesce a burst of changes.
        tokio::time::sleep(Duration::from_millis(60)).await;
        let dirty: Vec<String> = inner.dirty.lock().expect("dirty").drain().collect();
        for nb in dirty {
            inner.announce(&nb);
        }
    }
}

/// Notices saves by other processes on this device (the notes app) via change stamps.
async fn watch_loop(inner: Arc<Inner>) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(800)) => {}
            _ = inner.shutdown.notified() => break,
        }
        let ids = inner.lib.notebook_ids().unwrap_or_default();
        let mut changed = Vec::new();
        {
            let mut stamps = inner.stamps.lock().expect("stamps");
            for id in ids {
                let s = inner.lib.change_stamp(&id);
                if stamps.get(&id).copied() != Some(s) {
                    stamps.insert(id.clone(), s);
                    changed.push(id);
                }
            }
        }
        for id in changed {
            inner.mark_dirty(&id);
        }
    }
}

impl Inner {
    fn mark_dirty(&self, notebook: &str) {
        self.dirty.lock().expect("dirty").insert(notebook.to_string());
        self.dirty_notify.notify_one();
    }

    fn sessions_for(&self, notebook: &str) -> Vec<Arc<Session>> {
        self.sessions.lock().expect("sessions").values().filter(|s| s.kind.covers(notebook)).cloned().collect()
    }

    fn may_send(&self, kind: &Kind) -> bool {
        match kind {
            Kind::Own | Kind::Guest { .. } => true,
            Kind::Host { role, .. } => *role == Role::ReadWrite || self.guest_ignores_role,
        }
    }

    fn may_receive(kind: &Kind) -> bool {
        match kind {
            Kind::Own | Kind::Host { .. } => true,
            Kind::Guest { role, .. } => *role == Role::ReadWrite,
        }
    }

    fn register(&self, s: &Arc<Session>) -> bool {
        let mut map = self.sessions.lock().expect("sessions");
        if let Some(old) = map.get(&s.key()) {
            // Both dialed at once: keep the connection dialed by the smaller id; a reconnect
            // in the same direction replaces the old one.
            let new_wins = if old.dialed_by_me == s.dialed_by_me { true } else { s.dialed_by_me == (self.ep.id() < s.peer) };
            if !new_wins {
                return false;
            }
            old.conn.close(2u32.into(), b"replaced");
        }
        map.insert(s.key(), s.clone());
        true
    }

    fn unregister(&self, s: &Arc<Session>) {
        let removed = {
            let mut map = self.sessions.lock().expect("sessions");
            let key = s.key();
            match map.get(&key) {
                Some(cur) if Arc::ptr_eq(cur, s) => {
                    map.remove(&key);
                    true
                }
                _ => false,
            }
        };
        s.ended.notify_waiters();
        if removed {
            match &s.kind {
                Kind::Own => (self.emit)(Event::DeviceOffline { id: s.id() }),
                Kind::Guest { invite, notebook, .. } => (self.emit)(Event::GuestLeft { invite: invite.clone(), notebook: notebook.clone(), device: s.id() }),
                Kind::Host { notebook, .. } => (self.emit)(Event::HostOnline { notebook: notebook.clone(), online: false }),
            }
        }
    }

    fn ensure_dial_loop(self: &Arc<Self>, id: EndpointId) {
        if self.dialing.lock().expect("dialing").insert(id) {
            tokio::spawn(dial_loop(self.clone(), id, None));
        }
    }

    // --- devices ---------------------------------------------------------------------------

    fn is_device(&self, id: &EndpointId) -> bool {
        let s = id.to_string();
        self.state.lock().expect("state").devices.iter().any(|d| d.id == s)
    }

    fn device_addr(&self, id: &EndpointId) -> Option<EndpointAddr> {
        let s = id.to_string();
        self.state.lock().expect("state").devices.iter().find(|d| d.id == s).map(|d| d.addr.clone())
    }

    fn touch_device(&self, id: &EndpointId, name: &str, addr: &EndpointAddr) {
        let s = id.to_string();
        let mut st = self.state.lock().expect("state");
        if let Some(d) = st.devices.iter_mut().find(|d| d.id == s) {
            d.name = name.to_string();
            if !addr.is_empty() {
                d.addr = addr.clone();
            }
            d.last_seen = now_secs();
            let _ = st.save_devices();
        }
    }

    fn add_device(&self, d: DeviceRecord) -> DeviceInfo {
        let mut st = self.state.lock().expect("state");
        st.devices.retain(|x| x.id != d.id);
        let info = DeviceInfo { id: d.id.clone(), name: d.name.clone(), online: false, last_seen: d.last_seen };
        st.devices.push(d);
        let _ = st.save_devices();
        info
    }

    fn device_info(&self, id: &EndpointId, online: bool) -> DeviceInfo {
        let s = id.to_string();
        let st = self.state.lock().expect("state");
        let d = st.devices.iter().find(|d| d.id == s);
        DeviceInfo { id: s.clone(), name: d.map(|d| d.name.clone()).unwrap_or_default(), online, last_seen: d.map(|d| d.last_seen).unwrap_or(0) }
    }

    fn device_infos(&self) -> Vec<DeviceInfo> {
        let online: HashSet<String> = self.sessions.lock().expect("sessions").values().filter(|s| s.kind == Kind::Own).map(|s| s.id()).collect();
        self.state
            .lock()
            .expect("state")
            .devices
            .iter()
            .map(|d| DeviceInfo { id: d.id.clone(), name: d.name.clone(), online: online.contains(&d.id), last_seen: d.last_seen })
            .collect()
    }

    // --- guests ----------------------------------------------------------------------------

    /// Owner side: is this a valid, unrevoked invite of ours for an existing notebook?
    /// Whether a guest device may come in with this invite: Ok(true) = only after the owner
    /// lets it in. Devices already in skip approval, PIN and single use.
    fn gate(&self, rec: &InviteRecord, peer: &EndpointId, pin: Option<&str>) -> std::result::Result<bool, String> {
        let dev = peer.to_string();
        if rec.banned.iter().any(|b| b.device == dev) {
            return Err("Vom Besitzer entfernt".into());
        }
        if rec.members.contains(&dev) {
            return Ok(false);
        }
        if rec.single && !rec.members.is_empty() {
            return Err("Diese Einladung wurde schon benutzt".into());
        }
        if let Some(want) = &rec.pin {
            match pin.map(str::trim) {
                None | Some("") => return Err(PIN_NEEDED.into()),
                Some(p) if p != want => return Err(PIN_WRONG.into()),
                _ => {}
            }
        }
        Ok(rec.approve)
    }

    /// A guest device is in: remembered for the invite (single use: only if it's the first).
    fn add_member(&self, invite: &str, peer: &EndpointId) -> std::result::Result<(), String> {
        let dev = peer.to_string();
        let mut st = self.state.lock().expect("state");
        let Some(r) = st.invites.iter_mut().find(|r| r.id == invite) else { return Err("Einladung unbekannt".into()) };
        if r.members.contains(&dev) {
            return Ok(());
        }
        if r.single && !r.members.is_empty() {
            return Err("Diese Einladung wurde schon benutzt".into());
        }
        r.members.push(dev);
        let _ = st.save_invites();
        Ok(())
    }

    fn check_invite(&self, bytes: &[u8]) -> std::result::Result<(Invite, InviteRecord), String> {
        let signed: SignedInvite = proto::decode(bytes).map_err(|_| "Einladung unlesbar".to_string())?;
        let inv = signed.verify().map_err(|e| e.to_string())?;
        if inv.owner.id != self.ep.id() {
            return Err("Einladung gehört zu einem anderen Gerät".into());
        }
        let rec = self.state.lock().expect("state").invites.iter().find(|r| r.id == inv.id).cloned().ok_or("Einladung unbekannt")?;
        if rec.revoked {
            return Err("Einladung widerrufen".into());
        }
        if rec.expires != 0 && rec.expires < now_secs() {
            return Err("Einladung abgelaufen".into());
        }
        if rec.notebook != inv.notebook || self.lib.is_trashed(&rec.notebook) || self.lib.load_notebook(&rec.notebook).is_err() {
            return Err("Notizbuch nicht mehr da".into());
        }
        Ok((inv, rec))
    }

    // --- state and updates -----------------------------------------------------------------

    fn cached_vv(&self, path: &Path, load: impl FnOnce() -> Option<Vec<u8>>) -> Vec<u8> {
        let Ok(meta) = std::fs::metadata(path) else { return Vec::new() };
        let key = (meta.modified().unwrap_or(SystemTime::UNIX_EPOCH), meta.len());
        if let Some((m, l, vv)) = self.vv_cache.lock().expect("vv").get(path)
            && (*m, *l) == key
        {
            return vv.clone();
        }
        let vv = load().unwrap_or_default();
        self.vv_cache.lock().expect("vv").insert(path.to_path_buf(), (key.0, key.1, vv.clone()));
        vv
    }

    fn my_state(&self, nb: &str) -> NbState {
        let dir = self.root.join("notebooks").join(nb);
        let index_path = dir.join("index.loro");
        if self.lib.is_trashed(nb) || !index_path.is_file() {
            return NbState { id: nb.to_string(), index: Vec::new(), sheets: Vec::new() };
        }
        // Versions from the files' metadata: a whole library's worth of documents is announced
        // when a device connects, and loading each one only for its version cost seconds.
        let index = self.cached_vv(&index_path, || self.lib.notebook_version(nb).or_else(|| self.lib.load_notebook(nb).ok().map(|n| n.version())));
        let sheets = self
            .lib
            .sheet_ids_on_disk(nb)
            .unwrap_or_default()
            .into_iter()
            .map(|sid| {
                let p = dir.join("sheets").join(format!("{sid}.loro"));
                let vv = self.cached_vv(&p, || self.lib.sheet_version(nb, &sid).or_else(|| self.lib.load_sheet(nb, &sid).ok().map(|s| s.version())));
                (sid, vv)
            })
            .collect();
        NbState { id: nb.to_string(), index, sheets }
    }

    fn updates(&self, nb: &str, doc: &DocKey, since: &[u8]) -> Option<Vec<u8>> {
        let since = if since.is_empty() { empty_vv() } else { since.to_vec() };
        match doc {
            DocKey::Index => self.lib.load_notebook(nb).ok()?.updates_since(&since).ok(),
            DocKey::Sheet(sid) => self.lib.load_sheet(nb, sid).ok()?.updates_since(&since).ok(),
        }
    }

    /// Sends `s` what it lacks of `mine`, judged by `theirs` (doc → version vector).
    fn send_missing(&self, s: &Session, mine: &NbState, theirs: &HashMap<DocKey, Vec<u8>>) {
        let nb = &mine.id;
        if mine.index.is_empty() {
            return;
        }
        let mut docs = vec![(DocKey::Index, mine.index.clone())];
        docs.extend(mine.sheets.iter().map(|(sid, vv)| (DocKey::Sheet(sid.clone()), vv.clone())));
        for (doc, my_vv) in docs {
            let their_vv = theirs.get(&doc).cloned().unwrap_or_default();
            if !lacks(&their_vv, &my_vv) {
                continue;
            }
            if let Some(data) = self.updates(nb, &doc, &their_vv) {
                s.send(Msg::Updates { notebook: nb.clone(), doc: doc.clone(), data });
                s.their.lock().expect("their").insert((nb.clone(), doc), merged(&their_vv, &my_vv));
            }
        }
        self.last_sync.lock().expect("sync").insert(nb.clone(), now_secs());
    }

    /// Local change: push to everyone with this notebook what they lack, then announce.
    fn announce(&self, nb: &str) {
        let mine = self.my_state(nb);
        for s in self.sessions_for(nb) {
            if !self.may_send(&s.kind) {
                continue;
            }
            let known: HashMap<DocKey, Vec<u8>> = s.their.lock().expect("their").iter().filter(|((n, _), _)| n == nb).map(|((_, d), v)| (d.clone(), v.clone())).collect();
            if !known.is_empty() {
                self.send_missing(&s, &mine, &known);
            }
            s.send(Msg::Announce { state: mine.clone(), want_reply: true });
        }
    }

    fn on_msg(&self, s: &Arc<Session>, msg: Msg) {
        match msg {
            Msg::Announce { state, want_reply } => self.on_announce(s, state, want_reply),
            Msg::Updates { notebook, doc, data } => self.on_updates(s, notebook, doc, data),
            Msg::WantBlob { id } => self.on_want_blob(s, id),
            Msg::BlobChunk { id, offset, total, data } => self.on_chunk(s, id, offset, total, data),
            Msg::NoBlob { id } => {
                s.wanted.lock().expect("wanted").remove(&id);
            }
            Msg::Open { notebook } if s.kind == Kind::Own => (self.emit)(Event::OpenRequest { device: s.id(), notebook }),
            Msg::Follow { on } if s.kind == Kind::Own => (self.emit)(Event::FollowRequest { device: s.id(), on }),
            // Only the owner (or an own device) may move this view, and only in its notebook.
            Msg::Summon { notebook, sheet, viewport } if (s.kind == Kind::Own || matches!(s.kind, Kind::Host { .. })) && s.kind.covers(&notebook) => {
                (self.emit)(Event::Summoned { device: s.id(), notebook, sheet, viewport })
            }
            Msg::Shot if s.kind == Kind::Own => (self.emit)(Event::ScreenshotRequest { device: s.id() }),
            Msg::LatexRequest { id, png } if s.kind == Kind::Own => (self.emit)(Event::LatexRequest { device: s.id(), id, png }),
            Msg::LatexResult { id, latex, engine, error } if s.kind == Kind::Own => (self.emit)(Event::LatexResult { device: s.id(), id, latex, engine, error }),
            Msg::ShotData { image } if s.kind == Kind::Own => (self.emit)(Event::Screenshot { device: s.id(), image }),
            Msg::Live(bytes) => {
                if let Ok(live) = proto::decode::<Live>(&bytes) {
                    self.on_live(s, live);
                }
            }
            _ => {}
        }
    }

    fn on_announce(&self, s: &Arc<Session>, state: NbState, want_reply: bool) {
        let nb = state.id.clone();
        if !s.kind.covers(&nb) {
            return;
        }
        if self.lib.is_trashed(&nb) {
            return; // trashed here: never brought back by sync
        }
        let theirs: HashMap<DocKey, Vec<u8>> = std::iter::once((DocKey::Index, state.index.clone())).chain(state.sheets.iter().map(|(k, v)| (DocKey::Sheet(k.clone()), v.clone()))).collect();
        {
            let mut known = s.their.lock().expect("their");
            for (d, v) in &theirs {
                known.insert((nb.clone(), d.clone()), v.clone());
            }
        }
        let mine = self.my_state(&nb);
        if self.may_send(&s.kind) {
            self.send_missing(s, &mine, &theirs);
        }
        if want_reply && Self::may_receive(&s.kind) {
            let they_have_more = lacks(&mine.index, &state.index) || state.sheets.iter().any(|(sid, vv)| lacks(mine.sheets.iter().find(|(m, _)| m == sid).map(|(_, v)| v.as_slice()).unwrap_or(&[]), vv));
            if they_have_more {
                s.send(Msg::Announce { state: mine.clone(), want_reply: false });
            }
        }
        if !mine.index.is_empty() {
            self.want_missing_blobs(s, &nb);
        }
    }

    fn on_updates(&self, s: &Arc<Session>, nb: String, doc: DocKey, data: Vec<u8>) {
        if !s.kind.covers(&nb) || !Self::may_receive(&s.kind) {
            return; // e.g. a read-only guest: never applied
        }
        if self.lib.is_trashed(&nb) {
            return;
        }
        let changed = match &doc {
            DocKey::Index => self.lib.merge_notebook(&nb, &data),
            DocKey::Sheet(sid) => self.lib.merge_sheet(&nb, sid, &data),
        };
        match changed {
            Ok(true) => {
                // Keep our own stamp bookkeeping from re-announcing this as a local change
                // to its sender only; other peers must hear about it (relay).
                self.last_sync.lock().expect("sync").insert(nb.clone(), now_secs());
                let (index, sheets) = match &doc {
                    DocKey::Index => (true, Vec::new()),
                    DocKey::Sheet(sid) => (false, vec![sid.clone()]),
                };
                (self.emit)(Event::RemoteChanged { notebook: nb.clone(), index, sheets });
                self.mark_dirty(&nb);
                match &doc {
                    DocKey::Index => self.want_missing_blobs(s, &nb),
                    // A sheet that changed may carry new images.
                    DocKey::Sheet(sid) => {
                        if let Ok(sheet) = self.lib.load_sheet(&nb, sid) {
                            self.want_blobs(s, sheet.images().into_iter().map(|i| i.blob));
                        }
                    }
                }
            }
            Ok(false) => {}
            Err(e) => (self.emit)(Event::SyncError { message: format!("{nb}: {e}") }),
        }
    }

    fn want_missing_blobs(&self, s: &Arc<Session>, nb: &str) {
        let Ok(notebook) = self.lib.load_notebook(nb) else { return };
        let mut ids = notebook.pdf_blobs();
        ids.extend(self.lib.image_blobs(nb));
        self.want_blobs(s, ids.into_iter());
    }

    /// Asks the peer for every blob of `ids` not here yet (PDFs, images).
    fn want_blobs(&self, s: &Arc<Session>, ids: impl Iterator<Item = String>) {
        for id in ids {
            let have = self.lib.blob_path(&id).map(|p| p.exists()).unwrap_or(true);
            if !have && s.wanted.lock().expect("wanted").insert(id.clone()) {
                s.send(Msg::WantBlob { id });
            }
        }
    }

    fn on_want_blob(&self, s: &Arc<Session>, id: String) {
        // Guests only get blobs of their notebook.
        if let Some(nb) = s.kind.scope() {
            let ok = self.lib.load_notebook(nb).map(|n| n.pdf_blobs().contains(&id)).unwrap_or(false) || self.lib.image_blobs(nb).contains(&id);
            if !ok {
                s.send(Msg::NoBlob { id });
                return;
            }
        }
        match self.lib.blob(&id) {
            Ok(data) => {
                let total = data.len() as u64;
                if data.is_empty() {
                    s.send(Msg::BlobChunk { id, offset: 0, total: 0, data: Vec::new() });
                    return;
                }
                for (i, c) in data.chunks(CHUNK).enumerate() {
                    s.send(Msg::BlobChunk { id: id.clone(), offset: (i * CHUNK) as u64, total, data: c.to_vec() });
                }
            }
            Err(_) => s.send(Msg::NoBlob { id }),
        }
    }

    fn on_chunk(&self, s: &Arc<Session>, id: String, offset: u64, total: u64, data: Vec<u8>) {
        if !s.wanted.lock().expect("wanted").contains(&id) || total > (2u64 << 30) {
            return;
        }
        let done = {
            let mut map = s.blobs_in.lock().expect("blobs");
            // Grown as chunks come, not reserved from what the peer claims: a `total` of 2 GB
            // would be an allocation the tablet can't make (an abort, not an error).
            let buf = map.entry(id.clone()).or_insert_with(|| Vec::with_capacity((total as usize).min(4 * CHUNK)));
            if offset as usize != buf.len() {
                map.remove(&id);
                s.wanted.lock().expect("wanted").remove(&id);
                return;
            }
            buf.extend_from_slice(&data);
            if buf.len() as u64 >= total { map.remove(&id) } else { None }
        };
        if let Some(bytes) = done {
            s.wanted.lock().expect("wanted").remove(&id);
            if n0te_core::store::blob_id(&bytes) == id {
                if self.lib.put_blob(&bytes).is_ok() {
                    (self.emit)(Event::BlobArrived { id });
                }
            } else {
                (self.emit)(Event::SyncError { message: format!("Datei {id} kam beschädigt an") });
            }
        }
    }

    // --- live ------------------------------------------------------------------------------

    fn send_live(&self, nb: &str, except: Option<EndpointId>, live: Live) {
        let bytes = proto::encode(&live);
        for s in self.sessions_for(nb) {
            if Some(s.peer) == except {
                continue;
            }
            let fits = s.conn.max_datagram_size().is_some_and(|m| bytes.len() <= m);
            if !fits || s.conn.send_datagram(Bytes::from(bytes.clone())).is_err() {
                s.send(Msg::Live(bytes.clone()));
            }
        }
    }

    fn on_live(&self, s: &Arc<Session>, live: Live) {
        match live {
            Live::Presence { notebook, sheet, viewport, zoom, laser } if s.kind.covers(&notebook) => {
                (self.emit)(Event::Presence { device: s.id(), notebook, sheet, viewport, zoom, laser });
            }
            Live::Stroke { notebook, sheet, stroke, seq, brush, samples, done } if s.kind.covers(&notebook) => {
                (self.emit)(Event::LiveStroke { device: s.id(), notebook, sheet, stroke, seq, brush, samples, done });
            }
            // Erasing changes the notebook: only from those who may write it.
            Live::Erase { notebook, sheet, ids, dabs } if s.kind.covers(&notebook) && Self::may_receive(&s.kind) => {
                (self.emit)(Event::LiveErase { device: s.id(), notebook, sheet, ids, dabs });
            }
            _ => {}
        }
    }
}
