//! The connection to the n0te-sync service (`$XDG_RUNTIME_DIR/n0te-sync.sock`; Windows: the
//! user's named pipe, see `n0te_platform::ipc`): a long-lived "session" for live events
//! (other devices' changes, presence, live strokes, open/follow requests) and live commands,
//! plus one-shot requests (devices, pairing, invites). Linux: systemd's user unit runs the
//! service; Windows: the apps start it themselves when it isn't running.

use std::io::{BufRead, BufReader, Write};
use std::sync::{Mutex, OnceLock};

use iced::Subscription;
use iced::futures::StreamExt;
use iced::futures::channel::mpsc;
use iced::futures::stream::{self, BoxStream};
use n0te_platform::ipc::{Endpoint, Stream};
use serde_json::Value;

use crate::i18n::tr;

/// A connection to the service (fails at once when it isn't running).
fn connect() -> std::io::Result<Stream> {
    Stream::connect(&Endpoint::service())
}

/// What to tell people when the sync service isn't running.
pub fn service_hint() -> &'static str {
    if cfg!(windows) {
        tr("Der Sync-Dienst läuft nicht. n0te startet ihn gleich selbst (n0te-sync.exe im Programmordner).")
    } else {
        tr("Der Sync-Dienst läuft nicht. Starten: systemctl --user enable --now n0te-sync")
    }
}

/// Windows: starts the sync service (`n0te-sync.exe run` next to this program, windowless and
/// detached), at most every 30 s; the next [`Session::connect`] finds it. A second one started
/// meanwhile by the other app just ends (one service per library). Elsewhere nothing: systemd
/// runs it.
fn start_service() {
    #[cfg(windows)]
    {
        use std::time::{Duration, Instant};
        static LAST: Mutex<Option<Instant>> = Mutex::new(None);
        let Ok(mut last) = LAST.lock() else { return };
        if last.is_some_and(|t| t.elapsed() < Duration::from_secs(30)) {
            return;
        }
        *last = Some(Instant::now());
        let Some(exe) = std::env::current_exe().ok().and_then(|e| Some(e.parent()?.join("n0te-sync.exe"))) else { return };
        if exe.is_file() {
            let _ = n0te_platform::spawn_detached(&exe, &["run"]);
        }
    }
}

/// What the service reports.
#[derive(Debug, Clone, PartialEq)]
pub enum LiveEvent {
    Hello { node: String },
    Changed { notebook: String, index: bool, sheets: Vec<String> },
    Online { id: String, name: String },
    Offline { id: String },
    Paired { id: String, name: String },
    /// `viewport` and `laser` are in the coordinates of `sheet` (sheet points).
    Presence { device: String, notebook: String, sheet: Option<String>, viewport: [f32; 4], zoom: f32, laser: Option<[f32; 2]> },
    Stroke { device: String, notebook: String, sheet: String, stroke: u64, brush: (f32, f32, u32, u8), samples: Vec<u8>, done: bool },
    /// Someone erasing: strokes hidden, area-eraser dabs (x, y, r), in `sheet` points.
    Erase { notebook: String, sheet: String, ids: Vec<String>, dabs: Vec<[f32; 3]> },
    Open { device: String, notebook: String },
    Follow { device: String, on: bool },
    Guest { name: String, notebook: String, device: String, on: bool },
    /// As a guest: the owner changed our rights.
    Role { notebook: String, write: bool },
    /// Owner: a new guest waits to be let in (`on`), or no longer.
    Waiting { name: String, notebook: String, device: String, on: bool },
    /// Joining: the owner lets us in by hand.
    JoinWaiting { owner: String },
    /// As a guest: the owner brings this view to `viewport` on `sheet` (sheet points).
    Summoned { device: String, notebook: String, sheet: String, viewport: [f32; 4] },
    Revoked { notebook: String },
    /// As a guest: the connection to the notebook's owner came up / went down.
    Host { notebook: String, on: bool },
    Error { message: String },
    Disconnected,
}

fn parse(line: &str) -> Option<LiveEvent> {
    let v: Value = serde_json::from_str(line).ok()?;
    let s = |k: &str| v[k].as_str().unwrap_or_default().to_string();
    let f4 = |k: &str| -> Option<[f32; 4]> {
        let a = v[k].as_array()?;
        Some([a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32, a.get(2)?.as_f64()? as f32, a.get(3)?.as_f64()? as f32])
    };
    Some(match v["t"].as_str()? {
        "hello" => LiveEvent::Hello { node: s("node") },
        "changed" => LiveEvent::Changed {
            notebook: s("notebook"),
            index: v["index"].as_bool().unwrap_or(true),
            sheets: v["sheets"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
        },
        "online" => LiveEvent::Online { id: s("id"), name: s("name") },
        "offline" => LiveEvent::Offline { id: s("id") },
        "paired" => LiveEvent::Paired { id: s("id"), name: s("name") },
        "presence" => LiveEvent::Presence {
            device: s("device"),
            notebook: s("notebook"),
            sheet: v["sheet"].as_str().map(String::from),
            viewport: f4("viewport").unwrap_or_default(),
            zoom: v["zoom"].as_f64().unwrap_or(1.0) as f32,
            laser: v["laser"].as_array().and_then(|a| Some([a.first()?.as_f64()? as f32, a.get(1)?.as_f64()? as f32])),
        },
        "erase" => LiveEvent::Erase {
            notebook: s("notebook"),
            sheet: s("sheet"),
            ids: v["ids"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default(),
            dabs: v["dabs"]
                .as_array()
                .map(|a| a.iter().filter_map(|d| { let d = d.as_array()?; Some([d.first()?.as_f64()? as f32, d.get(1)?.as_f64()? as f32, d.get(2)?.as_f64()? as f32]) }).collect())
                .unwrap_or_default(),
        },
        "live" => LiveEvent::Stroke {
            device: s("device"),
            notebook: s("notebook"),
            sheet: s("sheet"),
            stroke: v["stroke"].as_u64().unwrap_or(0),
            brush: (
                v["brush"]["width"].as_f64().unwrap_or(2.0) as f32,
                v["brush"]["thinning"].as_f64().unwrap_or(0.5) as f32,
                v["brush"]["color"].as_u64().unwrap_or(0xFF111111) as u32,
                v["brush"]["kind"].as_u64().unwrap_or(0) as u8,
            ),
            samples: v["samples"].as_array().map(|a| a.iter().filter_map(|x| x.as_u64().map(|b| b as u8)).collect()).unwrap_or_default(),
            done: v["done"].as_bool().unwrap_or(false),
        },
        "open" => LiveEvent::Open { device: s("device"), notebook: s("notebook") },
        "follow" => LiveEvent::Follow { device: s("device"), on: v["on"].as_bool().unwrap_or(false) },
        "guest" => LiveEvent::Guest { name: s("name"), notebook: s("notebook"), device: s("device"), on: v["on"].as_bool().unwrap_or(false) },
        "role" => LiveEvent::Role { notebook: s("notebook"), write: v["write"].as_bool().unwrap_or(false) },
        "waiting" => LiveEvent::Waiting { name: s("name"), notebook: s("notebook"), device: s("device"), on: v["on"].as_bool().unwrap_or(false) },
        "join_waiting" => LiveEvent::JoinWaiting { owner: s("owner") },
        "summoned" => LiveEvent::Summoned { device: s("device"), notebook: s("notebook"), sheet: s("sheet"), viewport: f4("viewport").unwrap_or_default() },
        "revoked" => LiveEvent::Revoked { notebook: s("notebook") },
        "host" => LiveEvent::Host { notebook: s("notebook"), on: v["on"].as_bool().unwrap_or(false) },
        "error" => LiveEvent::Error { message: s("message") },
        _ => return None,
    })
}

/// Events from every session go through one channel the subscription reads.
fn channel() -> &'static (mpsc::UnboundedSender<LiveEvent>, Mutex<Option<mpsc::UnboundedReceiver<LiveEvent>>>) {
    static C: OnceLock<(mpsc::UnboundedSender<LiveEvent>, Mutex<Option<mpsc::UnboundedReceiver<LiveEvent>>>)> = OnceLock::new();
    C.get_or_init(|| {
        let (tx, rx) = mpsc::unbounded();
        (tx, Mutex::new(Some(rx)))
    })
}

fn events_stream() -> BoxStream<'static, LiveEvent> {
    match channel().1.lock().ok().and_then(|mut r| r.take()) {
        Some(rx) => rx.boxed(),
        None => stream::pending().boxed(),
    }
}

pub fn subscription() -> Subscription<LiveEvent> {
    Subscription::run(events_stream)
}

/// An open session with the service.
pub struct Session {
    out: Mutex<Stream>,
}

impl Session {
    /// Connects (None when the service isn't running; on Windows it gets started then).
    pub fn connect() -> Option<Session> {
        let Ok(mut s) = connect() else {
            start_service();
            return None;
        };
        writeln!(s, "[\"session\"]").ok()?;
        let read = s.try_clone().ok()?;
        let tx = channel().0.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(read).lines() {
                let Ok(line) = line else { break };
                if let Some(e) = parse(&line) {
                    let _ = tx.unbounded_send(e);
                }
            }
            let _ = tx.unbounded_send(LiveEvent::Disconnected);
        });
        Some(Session { out: Mutex::new(s) })
    }

    /// A live command (presence, live, open, follow); false if the connection broke.
    pub fn send(&self, args: &[String]) -> bool {
        let line = serde_json::to_string(args).unwrap_or_default();
        self.out.lock().map(|mut s| writeln!(s, "{line}").is_ok()).unwrap_or(false)
    }
}

/// A one-shot command to the service; its text output or the error.
pub fn request(args: &[&str]) -> Result<String, String> {
    request_within(args, limit_for(args.first().copied().unwrap_or_default()))
}

/// How long a request may take. Most are answered in milliseconds and many run on the UI
/// thread (the 10 s device poll among them), so a service that hangs must not freeze the
/// window; the ones that wait for another device or copy a notebook get longer (they run in
/// the background).
pub fn limit_for(command: &str) -> std::time::Duration {
    std::time::Duration::from_secs(match command {
        "accept" | "join" | "restore" => 120,
        "history" => 20,
        _ => 3,
    })
}

/// After a quick request timed out, the next quick ones fail at once for a while instead of
/// each waiting out its limit again.
static STALLED_UNTIL: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
const STALL_PAUSE: std::time::Duration = std::time::Duration::from_secs(30);

/// [`request`] with its own limit: the exchange runs on a helper thread; when it takes longer,
/// the connection is shut (which ends the helper's read) and the request fails.
pub fn request_within(args: &[&str], limit: std::time::Duration) -> Result<String, String> {
    let quick = limit <= std::time::Duration::from_secs(5);
    let stalled = || STALLED_UNTIL.lock().unwrap_or_else(|e| e.into_inner());
    if quick && stalled().is_some_and(|t| std::time::Instant::now() < t) {
        return Err(tr("Der Sync-Dienst antwortet nicht").to_string());
    }
    let s = connect().map_err(|_| tr("Sync-Dienst läuft nicht").to_string())?;
    let closer = s.try_clone().ok();
    let line = serde_json::to_string(args).unwrap_or_default();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut s = s;
        let r = (|| {
            writeln!(s, "{line}")?;
            let mut out = String::new();
            BufReader::new(s).read_line(&mut out)?;
            Ok::<_, std::io::Error>(out)
        })();
        let _ = tx.send(r);
    });
    let line = match rx.recv_timeout(limit) {
        Ok(r) => r.map_err(|e| e.to_string())?,
        Err(_) => {
            if let Some(c) = closer {
                let _ = c.shutdown();
            }
            if quick {
                *stalled() = Some(std::time::Instant::now() + STALL_PAUSE);
            }
            return Err(tr("Der Sync-Dienst antwortet nicht").to_string());
        }
    };
    *stalled() = None;
    let v: Value = serde_json::from_str(&line).map_err(|_| tr("keine Antwort vom Sync-Dienst").to_string())?;
    let out = v["out"].as_str().unwrap_or_default().to_string();
    if v["ok"].as_bool() == Some(true) { Ok(out) } else { Err(out) }
}

/// A paired device.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub online: bool,
}

pub fn devices() -> Vec<Device> {
    let Ok(out) = request(&["devices-json"]) else { return Vec::new() };
    let v: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
    v.as_array()
        .map(|a| a.iter().map(|d| Device { id: d["id"].as_str().unwrap_or_default().into(), name: d["name"].as_str().unwrap_or_default().into(), online: d["online"].as_bool().unwrap_or(false) }).collect())
        .unwrap_or_default()
}

/// An invite to a notebook.
#[derive(Debug, Clone, PartialEq)]
pub struct Invite {
    pub id: String,
    pub notebook: String,
    pub rw: bool,
    pub revoked: bool,
    pub online: u64,
    pub link: String,
    pub expires: u64,
    /// New guests wait until the owner lets them in.
    pub approve: bool,
    /// Only one device may come in.
    pub single: bool,
    pub pin: Option<String>,
    /// Devices that came in so far.
    pub members: u64,
    /// Removed devices: (device, name).
    pub banned: Vec<(String, String)>,
}

pub fn invites() -> Vec<Invite> {
    let Ok(out) = request(&["invites-json"]) else { return Vec::new() };
    let v: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|i| Invite {
                    id: i["id"].as_str().unwrap_or_default().into(),
                    notebook: i["notebook"].as_str().unwrap_or_default().into(),
                    rw: i["rw"].as_bool().unwrap_or(false),
                    revoked: i["revoked"].as_bool().unwrap_or(false),
                    online: i["online"].as_u64().unwrap_or(0),
                    link: i["link"].as_str().unwrap_or_default().into(),
                    expires: i["expires"].as_u64().unwrap_or(0),
                    approve: i["approve"].as_bool().unwrap_or(false),
                    single: i["single"].as_bool().unwrap_or(false),
                    pin: i["pin"].as_str().map(Into::into),
                    members: i["members"].as_u64().unwrap_or(0),
                    banned: i["banned"]
                        .as_array()
                        .map(|b| b.iter().map(|x| (x["device"].as_str().unwrap_or_default().to_string(), x["name"].as_str().unwrap_or_default().to_string())).collect())
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A guest connected to one of our notebooks.
#[derive(Debug, Clone)]
pub struct Guest {
    pub invite: String,
    pub notebook: String,
    pub name: String,
    pub device: String,
    pub write: bool,
}

pub fn guests() -> Vec<Guest> {
    guest_list("guests-json")
}

/// New guests waiting to be let in (invites with approval).
pub fn waiting() -> Vec<Guest> {
    guest_list("waiting-json")
}

fn guest_list(cmd: &str) -> Vec<Guest> {
    let Ok(out) = request(&[cmd]) else { return Vec::new() };
    let v: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
    v.as_array()
        .map(|a| {
            a.iter()
                .map(|g| Guest {
                    invite: g["invite"].as_str().unwrap_or_default().into(),
                    notebook: g["notebook"].as_str().unwrap_or_default().into(),
                    name: g["name"].as_str().unwrap_or_default().into(),
                    device: g["device"].as_str().unwrap_or_default().into(),
                    write: g["write"].as_bool().unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A notebook we joined as a guest.
#[derive(Debug, Clone)]
pub struct Joined {
    pub notebook: String,
    pub owner: String,
    pub owner_name: String,
    pub write: bool,
    pub online: bool,
}

pub fn joined() -> Vec<Joined> {
    let Ok(out) = request(&["joined-json"]) else { return Vec::new() };
    let v: Value = serde_json::from_str(&out).unwrap_or(Value::Null);
    v.as_array()
        .map(|a| {
            a.iter()
                .filter(|j| !j["revoked"].as_bool().unwrap_or(false))
                .map(|j| Joined {
                    notebook: j["notebook"].as_str().unwrap_or_default().into(),
                    owner: j["owner_id"].as_str().unwrap_or_default().into(),
                    owner_name: j["owner_name"].as_str().unwrap_or_default().into(),
                    write: j["write"].as_bool().unwrap_or(false),
                    online: j["online"].as_bool().unwrap_or(false),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_service_events() {
        assert_eq!(parse(r#"{"t":"changed","notebook":"n1","index":false,"sheets":["s1","s2"]}"#), Some(LiveEvent::Changed { notebook: "n1".into(), index: false, sheets: vec!["s1".into(), "s2".into()] }));
        let p = parse(r#"{"t":"presence","device":"d","notebook":"n","sheet":"s","viewport":[1,2,3,4],"zoom":1.5,"laser":[10,20]}"#).unwrap();
        assert_eq!(p, LiveEvent::Presence { device: "d".into(), notebook: "n".into(), sheet: Some("s".into()), viewport: [1.0, 2.0, 3.0, 4.0], zoom: 1.5, laser: Some([10.0, 20.0]) });
        let s = parse(r#"{"t":"live","device":"d","notebook":"n","sheet":"s","stroke":7,"seq":0,"brush":{"width":2,"thinning":0.5,"color":4278190080,"kind":1},"samples":[0,0,128,63],"done":true}"#).unwrap();
        assert!(matches!(s, LiveEvent::Stroke { stroke: 7, brush: (2.0, 0.5, 0xFF000000, 1), done: true, .. }));
        assert_eq!(parse(r#"{"t":"blob"}"#), None);
    }
}
