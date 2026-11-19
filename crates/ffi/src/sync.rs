//! Sync for the Android app: [`SyncEngine`] wraps `n0te_sync::Engine` (pairing, own-device
//! sync, guests, live presence/strokes, history) and reports to a [`SyncListener`].
//! Open notebooks and sheets pick up remote changes with `refresh()`.

use std::sync::{Arc, Mutex};

use n0te_sync::{Engine, EngineOptions, Event, LiveBrush, Role};

use crate::{FfiBrush, N0teError, NoteLibrary, NotebookHandle, R, SheetHandle, lock};

impl From<n0te_sync::Error> for N0teError {
    fn from(e: n0te_sync::Error) -> Self {
        N0teError::Failed { msg: e.to_string() }
    }
}

/// Receives sync events, on a sync thread (post to the UI thread yourself).
#[uniffi::export(foreign)]
pub trait SyncListener: Send + Sync {
    fn on_event(&self, event: FfiSyncEvent);
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiDevice {
    pub id: String,
    pub name: String,
    pub online: bool,
    /// Unix seconds (0 = not yet).
    pub last_seen: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiRole {
    Read,
    ReadWrite,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum FfiSyncEvent {
    /// Another device changed a notebook here: refresh `sheets` (and the outline if `index`).
    RemoteChanged { notebook: String, index: bool, sheets: Vec<String> },
    BlobArrived { id: String },
    DeviceOnline { device: FfiDevice },
    DeviceOffline { id: String },
    Paired { device: FfiDevice },
    /// Viewport x0, y0, x1, y1 in page points; laser point if shown.
    Presence { device: String, notebook: String, sheet: Option<String>, viewport: Vec<f32>, zoom: f32, laser: Option<Vec<f32>> },
    /// Samples (packed LE f32 x, y, pressure, t_ms) of a stroke being drawn elsewhere.
    LiveStroke { device: String, notebook: String, sheet: String, stroke: u64, seq: u32, brush: FfiBrush, samples: Vec<u8>, done: bool },
    OpenRequest { device: String, notebook: String },
    FollowRequest { device: String, on: bool },
    GuestJoined { invite: String, notebook: String, name: String },
    GuestLeft { invite: String, notebook: String },
    AccessRevoked { notebook: String },
    SyncError { message: String },
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiGuest {
    pub invite: String,
    pub notebook: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiInvite {
    pub id: String,
    pub notebook: String,
    pub title: String,
    pub role: FfiRole,
    pub expires: u64,
    pub created: u64,
    pub revoked: bool,
    pub link: String,
    pub online: u32,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiJoined {
    pub invite: String,
    pub notebook: String,
    pub title: String,
    pub role: FfiRole,
    pub owner: String,
    pub owner_name: String,
    pub online: bool,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiLastSync {
    pub notebook: String,
    pub at: u64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiSyncStatus {
    pub node_id: String,
    pub devices: Vec<FfiDevice>,
    pub guests: Vec<FfiGuest>,
    pub joined: Vec<FfiJoined>,
    pub last_sync: Vec<FfiLastSync>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiCheckpoint {
    /// Unix seconds (pass to `restore_as_copy`).
    pub at: u64,
    pub size: u64,
    /// "YYYY-MM-DD HH:MM", local time.
    pub label: String,
}

fn device(d: n0te_sync::DeviceInfo) -> FfiDevice {
    FfiDevice { id: d.id, name: d.name, online: d.online, last_seen: d.last_seen }
}

fn role(r: Role) -> FfiRole {
    match r {
        Role::Read => FfiRole::Read,
        Role::ReadWrite => FfiRole::ReadWrite,
    }
}

fn event(e: Event) -> FfiSyncEvent {
    match e {
        Event::RemoteChanged { notebook, index, sheets } => FfiSyncEvent::RemoteChanged { notebook, index, sheets },
        Event::BlobArrived { id } => FfiSyncEvent::BlobArrived { id },
        Event::DeviceOnline { device: d } => FfiSyncEvent::DeviceOnline { device: device(d) },
        Event::DeviceOffline { id } => FfiSyncEvent::DeviceOffline { id },
        Event::Paired { device: d } => FfiSyncEvent::Paired { device: device(d) },
        Event::Presence { device, notebook, sheet, viewport, zoom, laser } => FfiSyncEvent::Presence { device, notebook, sheet, viewport: viewport.to_vec(), zoom, laser: laser.map(|l| l.to_vec()) },
        Event::LiveStroke { device, notebook, sheet, stroke, seq, brush, samples, done } => FfiSyncEvent::LiveStroke {
            device,
            notebook,
            sheet,
            stroke,
            seq,
            brush: FfiBrush { width: brush.width, thinning: brush.thinning, color: brush.color, kind: brush.kind },
            samples,
            done,
        },
        Event::OpenRequest { device, notebook } => FfiSyncEvent::OpenRequest { device, notebook },
        Event::FollowRequest { device, on } => FfiSyncEvent::FollowRequest { device, on },
        Event::GuestJoined { invite, notebook, name } => FfiSyncEvent::GuestJoined { invite, notebook, name },
        Event::GuestLeft { invite, notebook } => FfiSyncEvent::GuestLeft { invite, notebook },
        Event::AccessRevoked { notebook } => FfiSyncEvent::AccessRevoked { notebook },
        Event::SyncError { message } => FfiSyncEvent::SyncError { message },
    }
}

/// The sync engine for one library (start once per app process).
#[derive(uniffi::Object)]
pub struct SyncEngine {
    engine: Mutex<Option<Engine>>,
}

impl SyncEngine {
    fn with<T>(&self, f: impl FnOnce(&Engine) -> R<T>) -> R<T> {
        match lock(&self.engine).as_ref() {
            Some(e) => f(e),
            None => Err(N0teError::Failed { msg: "Sync ist beendet".into() }),
        }
    }
}

#[uniffi::export]
impl SyncEngine {
    /// Starts syncing the library at `library_root` (the same directory as `NoteLibrary`).
    #[uniffi::constructor]
    pub fn start(library_root: String, device_name: String, listener: Arc<dyn SyncListener>) -> R<Arc<Self>> {
        let mut opts = EngineOptions::new(device_name);
        opts.listener = Some(Arc::new(move |e| listener.on_event(event(e))));
        Ok(Arc::new(SyncEngine { engine: Mutex::new(Some(Engine::start(library_root, opts)?)) }))
    }

    pub fn node_id(&self) -> R<String> {
        self.with(|e| Ok(e.node_id()))
    }
    /// One-time code (15 min) for pairing another own device.
    pub fn pair_code(&self) -> R<String> {
        self.with(|e| Ok(e.pair_code()?))
    }
    /// Pairs with the device that made `code` (e.g. pasted from the laptop).
    pub fn accept_pair(&self, code: String) -> R<FfiDevice> {
        self.with(|e| Ok(device(e.accept_pair(&code)?)))
    }
    pub fn devices(&self) -> R<Vec<FfiDevice>> {
        self.with(|e| Ok(e.devices().into_iter().map(device).collect()))
    }
    pub fn remove_device(&self, id: String) -> R<()> {
        self.with(|e| Ok(e.remove_device(&id)?))
    }
    pub fn status(&self) -> R<FfiSyncStatus> {
        self.with(|e| {
            let s = e.status();
            Ok(FfiSyncStatus {
                node_id: s.node_id,
                devices: s.devices.into_iter().map(device).collect(),
                guests: s.guests.into_iter().map(|g| FfiGuest { invite: g.invite, notebook: g.notebook, name: g.name }).collect(),
                joined: s.joined.into_iter().map(joined).collect(),
                last_sync: s.last_sync.into_iter().map(|(notebook, at)| FfiLastSync { notebook, at }).collect(),
            })
        })
    }
    /// Call after saving a notebook (the engine also notices within a second).
    pub fn notify_local_change(&self, notebook: String) -> R<()> {
        self.with(|e| {
            e.notify_local_change(&notebook);
            Ok(())
        })
    }
    /// Where this device looks (viewport x0, y0, x1, y1 in page points) and its laser point.
    pub fn send_presence(&self, notebook: String, sheet: Option<String>, viewport: Vec<f32>, zoom: f32, laser: Option<Vec<f32>>) -> R<()> {
        self.with(|e| {
            let v = [0, 1, 2, 3].map(|i| viewport.get(i).copied().unwrap_or(0.0));
            let l = laser.filter(|l| l.len() >= 2).map(|l| [l[0], l[1]]);
            e.send_presence(&notebook, sheet.as_deref(), v, zoom, l);
            Ok(())
        })
    }
    /// New samples of the stroke being drawn (packed LE f32 x, y, pressure, t_ms).
    pub fn send_live_stroke(&self, notebook: String, sheet: String, stroke: u64, seq: u32, brush: FfiBrush, samples: Vec<u8>, done: bool) -> R<()> {
        self.with(|e| {
            e.send_live_stroke(&notebook, &sheet, stroke, seq, LiveBrush { width: brush.width, thinning: brush.thinning, color: brush.color, kind: brush.kind }, &samples, done);
            Ok(())
        })
    }
    pub fn request_open(&self, device: String, notebook: String) -> R<()> {
        self.with(|e| Ok(e.request_open(&device, &notebook)?))
    }
    pub fn set_follow(&self, device: String, on: bool) -> R<()> {
        self.with(|e| Ok(e.set_follow(&device, on)?))
    }
    /// Guest link for one notebook. `expires`: unix seconds, 0 = never.
    pub fn create_invite(&self, notebook: String, role: FfiRole, expires: u64) -> R<String> {
        self.with(|e| Ok(e.create_invite(&notebook, if role == FfiRole::ReadWrite { Role::ReadWrite } else { Role::Read }, expires)?))
    }
    pub fn list_invites(&self) -> R<Vec<FfiInvite>> {
        self.with(|e| {
            Ok(e.list_invites()
                .into_iter()
                .map(|i| FfiInvite { id: i.id, notebook: i.notebook, title: i.title, role: role(i.role), expires: i.expires, created: i.created, revoked: i.revoked, link: i.link, online: i.online as u32 })
                .collect())
        })
    }
    pub fn revoke(&self, invite: String) -> R<()> {
        self.with(|e| Ok(e.revoke(&invite)?))
    }
    /// Joins a notebook from an invite link; returns its id.
    pub fn join(&self, link: String) -> R<String> {
        self.with(|e| Ok(e.join(&link)?))
    }
    pub fn joined(&self) -> R<Vec<FfiJoined>> {
        self.with(|e| Ok(e.joined().into_iter().map(joined).collect()))
    }
    pub fn leave(&self, invite: String) -> R<()> {
        self.with(|e| Ok(e.leave(&invite)?))
    }
    pub fn history(&self, notebook: String) -> R<Vec<FfiCheckpoint>> {
        self.with(|e| Ok(e.history(&notebook).into_iter().map(|c| FfiCheckpoint { at: c.at, size: c.size, label: n0te_sync::history::local_time(c.at) }).collect()))
    }
    /// Restores a checkpoint as a new notebook; returns its id.
    pub fn restore_as_copy(&self, notebook: String, at: u64) -> R<String> {
        self.with(|e| Ok(e.restore_as_copy(&notebook, at)?))
    }
    /// Stops syncing (the object is unusable afterwards).
    pub fn shutdown(&self) {
        if let Some(mut e) = lock(&self.engine).take() {
            e.shutdown();
        }
    }
}

fn joined(j: n0te_sync::JoinedInfo) -> FfiJoined {
    FfiJoined { invite: j.invite, notebook: j.notebook, title: j.title, role: role(j.role), owner: j.owner, owner_name: j.owner_name, online: j.online, revoked: j.revoked }
}

#[uniffi::export]
impl NotebookHandle {
    /// Merges what other processes / the sync engine saved into this open notebook. Returns
    /// whether anything changed (then re-read the outline).
    pub fn refresh(&self) -> R<bool> {
        Ok(self.lib.inner.refresh_notebook(&lock(&self.nb))?)
    }
}

#[uniffi::export]
impl SheetHandle {
    /// Merges the sheet's file (e.g. strokes from another device) into this open sheet.
    /// Returns whether anything changed (then reload its strokes).
    pub fn refresh(&self) -> R<bool> {
        let id = self.notebook.id();
        Ok(self.notebook.lib.inner.refresh_sheet(&id, &self.sheet_id, &lock(&self.sheet))?)
    }
}

#[uniffi::export]
impl NoteLibrary {
    /// Changes whenever a notebook is saved or merged by anyone on this device (poll it).
    pub fn change_stamp(&self, notebook: String) -> u64 {
        self.inner.change_stamp(&notebook)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FfiBackground, FfiSheetSpec};
    use std::time::{Duration, Instant};

    struct Collect(Mutex<Vec<FfiSyncEvent>>);
    impl SyncListener for Collect {
        fn on_event(&self, event: FfiSyncEvent) {
            lock(&self.0).push(event);
        }
    }

    fn local(root: &str, name: &str, l: Arc<Collect>) -> Arc<SyncEngine> {
        let mut o = EngineOptions::new(name);
        o.local_only = true;
        o.listener = Some(Arc::new(move |e| l.on_event(event(e))));
        Arc::new(SyncEngine { engine: Mutex::new(Some(Engine::start(root, o).unwrap())) })
    }

    fn wait(what: &str, mut f: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while Instant::now() < deadline {
            if f() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("timed out: {what}");
    }

    #[test]
    fn pair_sync_and_refresh_through_the_ffi() {
        let tag = n0te_core::ids::new_id(31);
        let ra = std::env::temp_dir().join(format!("n0te-ffi-sync-a-{tag}")).to_string_lossy().into_owned();
        let rb = std::env::temp_dir().join(format!("n0te-ffi-sync-b-{tag}")).to_string_lossy().into_owned();
        let (la, lb) = (Arc::new(Collect(Mutex::new(Vec::new()))), Arc::new(Collect(Mutex::new(Vec::new()))));
        let laptop = local(&ra, "Laptop", la.clone());
        let tablet = local(&rb, "Tablet", lb.clone());
        let dev = tablet.accept_pair(laptop.pair_code().unwrap()).unwrap();
        assert_eq!(dev.name, "Laptop");
        wait("paired event on the laptop", || lock(&la.0).iter().any(|e| matches!(e, FfiSyncEvent::Paired { .. })));

        // The laptop app draws; the tablet's open sheet refreshes into the stroke.
        let lib_a = NoteLibrary::open(ra.clone()).unwrap();
        let nb = lib_a.clone().create_notebook("Mathe".into()).unwrap();
        let ids = nb.add_sheet(None, 0, FfiSheetSpec { width: 595.0, height: 842.0, background: FfiBackground::Grid, spacing: 17.0, pdf_blob: None, pdf_page: 0 }).unwrap();
        nb.save().unwrap();
        let id = nb.id();
        laptop.notify_local_change(id.clone()).unwrap();
        let lib_b = NoteLibrary::open(rb.clone()).unwrap();
        wait("notebook on the tablet", || lib_b.clone().load_notebook(id.clone()).is_ok());
        let tnb = lib_b.clone().load_notebook(id.clone()).unwrap();
        let tsheet = tnb.clone().open_sheet(ids[1].clone()).unwrap();
        assert!(tsheet.all_strokes().is_empty());
        let sheet = nb.clone().open_sheet(ids[1].clone()).unwrap();
        let samples = crate::pack(&[10.0, 10.0, 0.5, 0.0, 30.0, 12.0, 0.5, 8.0]);
        sheet.add_stroke(FfiBrush { width: 2.0, thinning: 0.5, color: 0xFF111111, kind: 0 }, samples).unwrap();
        sheet.save().unwrap();
        wait("remote change event", || lock(&lb.0).iter().any(|e| matches!(e, FfiSyncEvent::RemoteChanged { sheets, .. } if sheets.contains(&ids[1]))));
        assert!(tsheet.refresh().unwrap());
        assert_eq!(tsheet.all_strokes().len(), 1);
        assert!(lib_b.change_stamp(id.clone()) > 0);
        assert_eq!(laptop.status().unwrap().devices.len(), 1);
        laptop.shutdown();
        assert!(laptop.status().is_err());
    }
}
