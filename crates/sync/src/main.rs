//! `n0te-sync` — keeps the laptop's n0te library in sync with paired devices and guests.
//!
//!   n0te-sync                         run the service (systemd user unit n0te-sync.service)
//!   n0te-sync pair-code               code for pairing a device (tablet: paste it in n0te)
//!   n0te-sync accept <code>           pair with the device that made <code>
//!   n0te-sync devices | status
//!   n0te-sync invite <notebook> [--rw] [--days N] [--approve] [--single] [--pin P]
//!                                     guest link (read-only unless --rw)
//!   n0te-sync invites | revoke <invite-id>
//!   n0te-sync set-role <invite> read|write | set-expiry <invite> <days|0>
//!   n0te-sync set-options <invite> [--approve] [--single] | set-pin <invite> <pin|off>
//!   n0te-sync admit <device> | deny <device>         let a waiting guest in / turn it away
//!   n0te-sync remove-guest <device> | allow-again <invite> <device>
//!   n0te-sync join <link> [--pin P] | leave <invite-id>
//!   n0te-sync remove-device <id>
//!   n0te-sync history <notebook> | restore <notebook> <unix-seconds> | checkpoint
//!
//! With the service running, commands go to it over a unix socket
//! (`$XDG_RUNTIME_DIR/n0te-sync.sock`); otherwise they start a short-lived engine.
//! Library: `$N0TE_LIBRARY`, else `$XDG_DATA_HOME/n0te` (~/.local/share/n0te).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

use n0te_sync::history::local_time;
use n0te_sync::{Engine, EngineOptions, Event, LiveBrush, Role};

/// Open GUI sessions: every engine event goes to each as a JSON line.
mod recognize;

/// The running engine (for answers sent from event handlers).
static ENGINE: std::sync::OnceLock<Arc<Engine>> = std::sync::OnceLock::new();
static SUBS: std::sync::OnceLock<std::sync::Mutex<Vec<mpsc::Sender<String>>>> = std::sync::OnceLock::new();

fn broadcast(line: String) {
    if let Some(subs) = SUBS.get()
        && let Ok(mut v) = subs.lock()
    {
        v.retain(|tx| tx.send(line.clone()).is_ok());
    }
}

/// An engine event as a JSON line for GUI sessions (None: not of interest to apps).
fn event_json(e: &Event) -> Option<String> {
    use serde_json::json;
    let v = match e {
        Event::RemoteChanged { notebook, index, sheets } => json!({ "t": "changed", "notebook": notebook, "index": index, "sheets": sheets }),
        Event::DeviceOnline { device } => json!({ "t": "online", "id": device.id, "name": device.name }),
        Event::DeviceOffline { id } => json!({ "t": "offline", "id": id }),
        Event::Paired { device } => json!({ "t": "paired", "id": device.id, "name": device.name }),
        Event::Presence { device, notebook, sheet, viewport, zoom, laser } => json!({ "t": "presence", "device": device, "notebook": notebook, "sheet": sheet, "viewport": viewport, "zoom": zoom, "laser": laser }),
        Event::LiveStroke { device, notebook, sheet, stroke, seq, brush, samples, done } => json!({
            "t": "live", "device": device, "notebook": notebook, "sheet": sheet, "stroke": stroke, "seq": seq,
            "brush": { "width": brush.width, "thinning": brush.thinning, "color": brush.color, "kind": brush.kind }, "samples": samples, "done": done
        }),
        Event::Screenshot { device, image } => json!({ "t": "screenshot", "device": device, "bytes": image.len(), "path": shot_path(device) }),
        // The laptop has no screen to send (the app is the GUI, not this service).
        Event::ScreenshotRequest { device } => json!({ "t": "shot_request", "device": device }),
        Event::LatexRequest { device, id, png } => json!({ "t": "latex_request", "device": device, "id": id, "bytes": png.len() }),
        Event::LatexResult { device, id, latex, engine, error } => json!({ "t": "latex_result", "device": device, "id": id, "latex": latex, "engine": engine, "error": error }),
        Event::LiveErase { device, notebook, sheet, ids, dabs } => json!({ "t": "erase", "device": device, "notebook": notebook, "sheet": sheet, "ids": ids, "dabs": dabs }),
        Event::OpenRequest { device, notebook } => json!({ "t": "open", "device": device, "notebook": notebook }),
        Event::FollowRequest { device, on } => json!({ "t": "follow", "device": device, "on": on }),
        Event::GuestJoined { invite, notebook, name, device } => json!({ "t": "guest", "invite": invite, "notebook": notebook, "name": name, "device": device, "on": true }),
        Event::GuestLeft { invite, notebook, device } => json!({ "t": "guest", "invite": invite, "notebook": notebook, "device": device, "on": false }),
        Event::RoleChanged { notebook, write } => json!({ "t": "role", "notebook": notebook, "write": write }),
        Event::AccessRevoked { notebook } => json!({ "t": "revoked", "notebook": notebook }),
        Event::GuestWaiting { invite, notebook, name, device, on } => json!({ "t": "waiting", "invite": invite, "notebook": notebook, "name": name, "device": device, "on": on }),
        Event::JoinWaiting { owner } => json!({ "t": "join_waiting", "owner": owner }),
        Event::SyncError { message } => json!({ "t": "error", "message": message }),
        Event::BlobArrived { .. } => return None,
    };
    Some(v.to_string())
}

fn library_root() -> PathBuf {
    if let Some(p) = std::env::var_os("N0TE_LIBRARY") {
        return PathBuf::from(p);
    }
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("n0te")
}

/// Where a device's screenshot lands (next to the socket).
fn shot_path(device: &str) -> PathBuf {
    let short: String = device.chars().take(10).collect();
    socket_path().with_file_name(format!("n0te-shot-{short}.jpg"))
}

/// Writes a file only this user can read.
fn write_private(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_extension("tmp");
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    o.mode(0o600);
    o.open(&tmp)?.write_all(data)?;
    std::fs::rename(tmp, path)
}

fn socket_path() -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(d) => PathBuf::from(d).join("n0te-sync.sock"),
        None => library_root().join("sync").join("n0te-sync.sock"),
    }
}

fn device_name() -> String {
    std::env::var("N0TE_DEVICE_NAME").ok().filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "Laptop".to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        None | Some("run") => service().map(|_| 0),
        Some("help" | "-h" | "--help") => {
            println!("{}", include_str!("main.rs").lines().take_while(|l| l.starts_with("//!")).map(|l| l.trim_start_matches("//!").trim_start_matches(' ')).collect::<Vec<_>>().join("\n"));
            Ok(0)
        }
        Some(_) => client(&args),
    };
    match code {
        Ok(c) => std::process::exit(c),
        Err(e) => {
            eprintln!("n0te-sync: {e}");
            std::process::exit(1);
        }
    }
}

type Res<T> = std::result::Result<T, String>;

fn log(msg: &str) {
    eprintln!("{msg}");
}

fn describe(e: &Event) -> Option<String> {
    Some(match e {
        Event::RemoteChanged { notebook, index, sheets } => format!("geändert: {notebook}{}{}", if *index { " (Gliederung)" } else { "" }, if sheets.is_empty() { String::new() } else { format!(" {} Seite(n)", sheets.len()) }),
        Event::BlobArrived { id } => format!("Datei angekommen: {}", &id[..12.min(id.len())]),
        Event::DeviceOnline { device } => format!("verbunden: {} ({})", device.name, short(&device.id)),
        Event::DeviceOffline { id } => format!("getrennt: {}", short(id)),
        Event::Paired { device } => format!("gekoppelt: {} ({})", device.name, short(&device.id)),
        Event::GuestJoined { notebook, name, .. } => format!("Gast verbunden: {name} → {notebook}"),
        Event::GuestLeft { notebook, .. } => format!("Gast getrennt: {notebook}"),
        Event::RoleChanged { notebook, write } => format!("Rechte geändert: {notebook} → {}", if *write { "lesen & schreiben" } else { "nur lesen" }),
        Event::GuestWaiting { name, notebook, device, on: true, .. } => format!("{name} wartet auf Einlass in {notebook} (n0te-sync admit {device})"),
        Event::GuestWaiting { name, on: false, .. } => format!("{name} wartet nicht mehr"),
        Event::JoinWaiting { owner } => format!("Warte, bis {owner} dich hereinlässt …"),
        Event::AccessRevoked { notebook } => format!("Zugriff widerrufen: {notebook}"),
        Event::SyncError { message } => format!("Fehler: {message}"),
        Event::OpenRequest { notebook, .. } => format!("Öffnen angefragt: {notebook}"),
        _ => return None,
    })
}

fn short(id: &str) -> &str {
    &id[..10.min(id.len())]
}

/// The service: engine + checkpoints + command socket.
fn service() -> Res<()> {
    let root = library_root();
    std::fs::create_dir_all(root.join("sync")).map_err(|e| e.to_string())?;
    let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(root.join("sync").join("service.lock")).map_err(|e| e.to_string())?;
    if lock.try_lock().is_err() {
        return Err("läuft schon".into());
    }
    let mut opts = EngineOptions::new(device_name());
    // Tests: no relays, no address lookup.
    opts.local_only = std::env::var_os("N0TE_SYNC_LOCAL_ONLY").is_some();
    let _ = SUBS.set(std::sync::Mutex::new(Vec::new()));
    opts.listener = Some(Arc::new(|e: Event| {
        if let Event::LatexRequest { device, id, png } = &e {
            // Handwriting from the tablet: read it here, answer when done.
            let (device, id, png) = (device.clone(), *id, png.clone());
            std::thread::spawn(move || {
                let r = recognize::recognize(&png);
                for _ in 0..50 {
                    if let Some(engine) = ENGINE.get() {
                        let _ = match &r {
                            Ok((latex, eng)) => engine.send_latex_result(&device, id, latex, eng, None),
                            Err(err) => engine.send_latex_result(&device, id, "", "", Some(err)),
                        };
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            });
        }
        if let Event::Screenshot { device, image } = &e {
            // For `n0te-sync screenshot`: next to the socket, only for this user.
            let _ = write_private(&shot_path(device), image);
        }
        if let Some(s) = describe(&e) {
            log(&s);
        }
        if let Some(line) = event_json(&e) {
            broadcast(line);
        }
    }));
    let engine = Arc::new(Engine::start(&root, opts).map_err(|e| e.to_string())?);
    let _ = ENGINE.set(engine.clone());
    log(&format!("n0te-sync {} · Bibliothek {} · Gerät {}", env!("CARGO_PKG_VERSION"), root.display(), engine.node_id()));

    // Checkpoints: now and every 5 minutes.
    let hist = engine.clone();
    std::thread::spawn(move || loop {
        let pass = hist.checkpoint();
        for (nb, _) in &pass.written {
            log(&format!("Stand gesichert: {nb}"));
        }
        for (nb, err) in &pass.failed {
            log(&format!("Sicherung fehlgeschlagen ({nb}): {err}"));
        }
        std::thread::sleep(Duration::from_secs(300));
    });

    let sock = socket_path();
    let _ = std::fs::remove_file(&sock);
    let listener = UnixListener::bind(&sock).map_err(|e| format!("{}: {e}", sock.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600));
    }
    for conn in listener.incoming().flatten() {
        let engine = engine.clone();
        std::thread::spawn(move || {
            let _ = serve(&engine, conn);
        });
    }
    drop(lock);
    Ok(())
}

fn serve(engine: &Engine, conn: UnixStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(conn.try_clone()?);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let args: Vec<String> = serde_json::from_str(&line).unwrap_or_default();
    if args.first().map(String::as_str) == Some("session") {
        return session(engine, conn, reader);
    }
    let reply = match dispatch(engine, &args) {
        Ok(out) => serde_json::json!({ "ok": true, "out": out }),
        Err(e) => serde_json::json!({ "ok": false, "out": e }),
    };
    let mut w = conn;
    writeln!(w, "{reply}")
}

/// A GUI session: the app sends live commands (one JSON array per line, no replies) and gets
/// every engine event as a JSON line, until either side closes.
fn session(engine: &Engine, conn: UnixStream, reader: BufReader<UnixStream>) -> std::io::Result<()> {
    let (tx, rx) = mpsc::channel::<String>();
    if let Some(subs) = SUBS.get() {
        subs.lock().expect("subs").push(tx.clone());
    }
    // A hello with this device's id, so the app can tell its own echo apart.
    let _ = tx.send(serde_json::json!({ "t": "hello", "node": engine.node_id() }).to_string());
    let mut w = conn.try_clone()?;
    let writer = std::thread::spawn(move || {
        for line in rx {
            if writeln!(w, "{line}").is_err() {
                break;
            }
        }
    });
    for line in reader.lines() {
        let Ok(line) = line else { break };
        let args: Vec<String> = serde_json::from_str(&line).unwrap_or_default();
        if let Err(err) = dispatch(engine, &args) {
            let _ = tx.send(serde_json::json!({ "t": "error", "message": err }).to_string());
        }
    }
    drop(tx);
    let _ = conn.shutdown(std::net::Shutdown::Both);
    let _ = writer.join();
    Ok(())
}

/// A CLI command: through the service if it runs, else with a short-lived engine.
fn client(args: &[String]) -> Res<i32> {
    if let Ok(mut s) = UnixStream::connect(socket_path()) {
        writeln!(s, "{}", serde_json::to_string(args).expect("json")).map_err(|e| e.to_string())?;
        let mut line = String::new();
        BufReader::new(s).read_line(&mut line).map_err(|e| e.to_string())?;
        let v: serde_json::Value = serde_json::from_str(&line).map_err(|_| "keine Antwort vom Dienst".to_string())?;
        let out = v["out"].as_str().unwrap_or_default();
        return if v["ok"].as_bool() == Some(true) {
            println!("{out}");
            Ok(0)
        } else {
            Err(out.to_string())
        };
    }
    let (tx, rx) = mpsc::channel::<Event>();
    let mut opts = EngineOptions::new(device_name());
    // Tests: no relays, no address lookup.
    opts.local_only = std::env::var_os("N0TE_SYNC_LOCAL_ONLY").is_some();
    let tx = std::sync::Mutex::new(tx);
    opts.listener = Some(Arc::new(move |e| {
        let _ = tx.lock().expect("tx").send(e);
    }));
    let engine = Engine::start(library_root(), opts).map_err(|e| e.to_string())?;
    let out = dispatch(&engine, args)?;
    println!("{out}");
    match args[0].as_str() {
        // Without the service: stay up until the other side paired, then sync a moment.
        "pair-code" => {
            eprintln!("(Dienst läuft nicht: warte bis zu 10 Minuten auf das andere Gerät …)");
            let deadline = Instant::now() + Duration::from_secs(600);
            while let Some(left) = deadline.checked_duration_since(Instant::now()) {
                if let Ok(e) = rx.recv_timeout(left)
                    && let Event::Paired { device } = &e
                {
                    eprintln!("gekoppelt: {}", device.name);
                    settle(&rx, Duration::from_secs(20));
                    break;
                }
            }
        }
        "accept" | "join" => settle(&rx, Duration::from_secs(30)),
        _ => {}
    }
    Ok(0)
}

/// Lets the first sync run: until nothing changed for 3 s (at most `max`).
fn settle(rx: &mpsc::Receiver<Event>, max: Duration) {
    let deadline = Instant::now() + max;
    let mut quiet_since = Instant::now();
    while Instant::now() < deadline && quiet_since.elapsed() < Duration::from_secs(3) {
        if let Ok(e) = rx.recv_timeout(Duration::from_millis(250)) {
            if let Some(s) = describe(&e) {
                eprintln!("{s}");
            }
            quiet_since = Instant::now();
        }
    }
}

fn dispatch(engine: &Engine, args: &[String]) -> Res<String> {
    let arg = |i: usize, what: &str| args.get(i).cloned().ok_or_else(|| format!("fehlt: {what}"));
    let e = |r: n0te_sync::Error| r.to_string();
    Ok(match args.first().map(String::as_str).unwrap_or("status") {
        "pair-code" => engine.pair_code().map_err(e)?,
        "accept" => {
            let d = engine.accept_pair(&arg(1, "Code")?).map_err(e)?;
            format!("Gekoppelt mit {} ({})", d.name, short(&d.id))
        }
        "devices" => {
            let list = engine.devices();
            if list.is_empty() {
                "Keine gekoppelten Geräte. `n0te-sync pair-code` erzeugt einen Code.".into()
            } else {
                list.iter().map(|d| format!("{} {:<14} {}  {}", if d.online { "●" } else { "○" }, d.name, d.id, if d.last_seen > 0 { format!("zuletzt {}", local_time(d.last_seen)) } else { String::new() })).collect::<Vec<_>>().join("\n")
            }
        }
        "status" => {
            let s = engine.status();
            let mut out = vec![format!("Gerät {}", s.node_id)];
            out.push(match &s.relay {
                Some(r) => format!("  Netz: Relay {r} · {} direkte Adressen", s.direct_addrs),
                None => format!("  Netz: kein Relay (nur direkt/LAN) · {} direkte Adressen", s.direct_addrs),
            });
            out.extend(s.devices.iter().map(|d| format!("  {} {} ({})", if d.online { "●" } else { "○" }, d.name, short(&d.id))));
            out.extend(s.guests.iter().map(|g| format!("  Gast {} → {}", g.name, g.notebook)));
            out.extend(s.joined.iter().map(|j| format!("  beigetreten: {} von {}{}", j.title, j.owner_name, if j.revoked { " (widerrufen)" } else if j.online { " ●" } else { "" })));
            out.extend(s.last_sync.iter().map(|(nb, t)| format!("  {nb}: zuletzt {}", local_time(*t))));
            out.join("\n")
        }
        "invite" => {
            let nb = arg(1, "Notizbuch-ID")?;
            let rw = args.iter().any(|a| a == "--rw");
            let days = args.iter().position(|a| a == "--days").and_then(|i| args.get(i + 1)).and_then(|d| d.parse::<u64>().ok());
            let expires = days.map(|d| n0te_sync::proto::now_secs() + d * 86_400).unwrap_or(0);
            let opts = n0te_sync::InviteOptions {
                approve: args.iter().any(|a| a == "--approve"),
                single: args.iter().any(|a| a == "--single"),
                pin: args.iter().position(|a| a == "--pin").and_then(|i| args.get(i + 1)).cloned(),
            };
            engine.create_invite_with(&nb, if rw { Role::ReadWrite } else { Role::Read }, expires, opts).map_err(e)?
        }
        "invites" => engine
            .list_invites()
            .iter()
            .map(|i| format!("{}  {:<20} {:?}{}{}{}{}{}  {} verbunden", i.id, i.title, i.role, if i.revoked { " widerrufen" } else { "" }, if i.expires > 0 { format!(" bis {}", local_time(i.expires)) } else { String::new() }, if i.approve { " · bestätigen" } else { "" }, if i.single { " · ein Gerät" } else { "" }, if i.pin.is_some() { " · PIN" } else { "" }, i.online))
            .collect::<Vec<_>>()
            .join("\n"),
        "revoke" => {
            engine.revoke(&arg(1, "Einladungs-ID")?).map_err(e)?;
            "Widerrufen.".into()
        }
        // set-role <invite> read|write
        "set-role" => {
            let id = arg(1, "Einladungs-ID")?;
            let write = matches!(arg(2, "read|write")?.as_str(), "write" | "rw");
            engine.set_role(&id, if write { Role::ReadWrite } else { Role::Read }).map_err(e)?;
            if write { "Gäste dürfen jetzt schreiben.".into() } else { "Gäste dürfen jetzt nur lesen.".into() }
        }
        // share-view on|off: whether guests see where this laptop looks
        "share-view" => {
            engine.set_share_view(arg(1, "on|off")? != "off");
            "OK".into()
        }
        "guests-json" => serde_json::to_string(&engine.status().guests.iter().map(|g| serde_json::json!({ "invite": g.invite, "notebook": g.notebook, "name": g.name, "device": g.device, "write": g.role == Role::ReadWrite })).collect::<Vec<_>>()).unwrap_or_default(),
        "set-expiry" => {
            let id = arg(1, "Einladungs-ID")?;
            let days: u64 = arg(2, "Tage (0 = ohne Ablauf)")?.parse().map_err(|_| "Tage als Zahl".to_string())?;
            engine.set_expiry(&id, if days == 0 { 0 } else { n0te_sync::proto::now_secs() + days * 86_400 }).map_err(e)?;
            if days == 0 { "Ohne Ablauf.".into() } else { format!("Gilt noch {days} Tag(e).") }
        }
        "set-options" => {
            let id = arg(1, "Einladungs-ID")?;
            let (approve, single) = (args.iter().any(|a| a == "--approve"), args.iter().any(|a| a == "--single"));
            engine.set_invite_options(&id, approve, single).map_err(e)?;
            format!("Bestätigen: {} · nur ein Gerät: {}", if approve { "an" } else { "aus" }, if single { "an" } else { "aus" })
        }
        "set-pin" => {
            let id = arg(1, "Einladungs-ID")?;
            let pin = arg(2, "PIN oder off")?;
            engine.set_pin(&id, if pin == "off" { None } else { Some(pin.as_str()) }).map_err(e)?;
            if pin == "off" { "Ohne PIN.".into() } else { format!("PIN: {pin}") }
        }
        "admit" | "deny" => {
            engine.admit(&arg(1, "Geräte-ID")?, args[0] == "admit").map_err(e)?;
            if args[0] == "admit" { "Hereingelassen.".into() } else { "Abgewiesen.".into() }
        }
        "remove-guest" => {
            engine.remove_guest(&arg(1, "Geräte-ID")?).map_err(e)?;
            "Entfernt: das Gerät kommt mit dieser Einladung nicht mehr herein.".into()
        }
        "allow-again" => {
            engine.allow_again(&arg(1, "Einladungs-ID")?, &arg(2, "Geräte-ID")?).map_err(e)?;
            "Darf wieder herein.".into()
        }
        "waiting-json" => serde_json::to_string(&engine.status().waiting.iter().map(|g| serde_json::json!({ "invite": g.invite, "notebook": g.notebook, "name": g.name, "device": g.device, "write": g.role == Role::ReadWrite })).collect::<Vec<_>>()).unwrap_or_default(),
        "join" => {
            let pin = args.iter().position(|a| a == "--pin").and_then(|i| args.get(i + 1)).cloned();
            let nb = engine.join_with(&arg(1, "Link")?, pin.as_deref()).map_err(e)?;
            format!("Beigetreten: {nb}")
        }
        "leave" => {
            engine.leave(&arg(1, "Einladungs-ID")?).map_err(e)?;
            "Verlassen (die Kopie hier bleibt).".into()
        }
        "remove-device" => {
            engine.remove_device(&arg(1, "Geräte-ID")?).map_err(e)?;
            "Entkoppelt.".into()
        }
        "history" => {
            let list = engine.history(&arg(1, "Notizbuch-ID")?);
            if list.is_empty() {
                "Noch keine Stände.".into()
            } else {
                list.iter().map(|c| format!("{}  {}  {} KB", c.at, local_time(c.at), c.size / 1024)).collect::<Vec<_>>().join("\n")
            }
        }
        "restore" => {
            let at: u64 = arg(2, "Zeitpunkt")?.parse().map_err(|_| "Zeitpunkt: Unix-Sekunden aus `history`".to_string())?;
            let id = engine.restore_as_copy(&arg(1, "Notizbuch-ID")?, at).map_err(e)?;
            format!("Wiederhergestellt als neues Notizbuch {id}")
        }
        "checkpoint" => {
            let p = engine.checkpoint();
            format!("{} gesichert, {} fehlgeschlagen, {} alte entfernt", p.written.len(), p.failed.len(), p.removed)
        }
        // For apps (JSON).
        "devices-json" => serde_json::to_string(&engine.devices().iter().map(|d| serde_json::json!({ "id": d.id, "name": d.name, "online": d.online, "last_seen": d.last_seen })).collect::<Vec<_>>()).unwrap_or_default(),
        "invites-json" => serde_json::to_string(
            &engine
                .list_invites()
                .iter()
                .map(|i| {
                    serde_json::json!({
                        "id": i.id, "notebook": i.notebook, "title": i.title, "rw": i.role == Role::ReadWrite, "revoked": i.revoked,
                        "expires": i.expires, "online": i.online, "link": i.link, "approve": i.approve, "single": i.single, "pin": i.pin,
                        "members": i.members, "banned": i.banned.iter().map(|b| serde_json::json!({ "device": b.device, "name": b.name })).collect::<Vec<_>>()
                    })
                })
                .collect::<Vec<_>>(),
        )
        .unwrap_or_default(),
        "joined-json" => serde_json::to_string(&engine.joined().iter().map(|j| serde_json::json!({ "invite": j.invite, "notebook": j.notebook, "title": j.title, "owner": j.owner_name, "owner_id": j.owner, "owner_name": j.owner_name, "write": j.role == Role::ReadWrite, "online": j.online, "revoked": j.revoked })).collect::<Vec<_>>()).unwrap_or_default(),
        "node" => engine.node_id(),
        // Live (sessions): presence, stroke samples, open / follow requests.
        "presence" => {
            let f = |i: usize| args.get(i).and_then(|v| v.parse::<f32>().ok());
            let sheet = args.get(2).filter(|s| !s.is_empty()).map(String::as_str);
            let viewport = [f(3).unwrap_or(0.0), f(4).unwrap_or(0.0), f(5).unwrap_or(0.0), f(6).unwrap_or(0.0)];
            let laser = match (f(8), f(9)) {
                (Some(x), Some(y)) => Some([x, y]),
                _ => None,
            };
            engine.send_presence(&arg(1, "Notizbuch")?, sheet, viewport, f(7).unwrap_or(1.0), laser);
            String::new()
        }
        "live" => {
            let num = |i: usize| args.get(i).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0);
            let brush = LiveBrush { width: num(5) as f32, thinning: num(6) as f32, color: num(7) as u32, kind: num(8) as u8 };
            let samples: Vec<u8> = args.get(9).and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
            engine.send_live_stroke(&arg(1, "Notizbuch")?, &arg(2, "Blatt")?, num(3) as u64, num(4) as u32, brush, &samples, args.get(10).is_some_and(|d| d == "1"));
            String::new()
        }
        // latex <png>: handwriting to LaTeX with the recognizers the tablet uses (for tests).
        "latex" => {
            let png = std::fs::read(arg(1, "PNG")?).map_err(|e| e.to_string())?;
            let (latex, engine) = recognize::recognize(&png)?;
            format!("{latex}\n({engine})")
        }
        // screenshot <device>: a picture of n0te on that own device (path printed when it's in).
        "screenshot" => {
            let dev = arg(1, "Gerät")?;
            let path = shot_path(&dev);
            let before = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
            engine.request_screenshot(&dev).map_err(e)?;
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                let now = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
                if now.is_some() && now != before {
                    break path.display().to_string();
                }
                if Instant::now() > deadline {
                    return Err("kein Bild bekommen (ist n0te dort offen?)".into());
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        // erase <notebook> <sheet> <ids json array> <dabs json array of [x, y, r]>
        "erase" => {
            let ids: Vec<String> = args.get(3).and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
            let dabs: Vec<[f32; 3]> = args.get(4).and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
            engine.send_live_erase(&arg(1, "Notizbuch")?, &arg(2, "Blatt")?, &ids, &dabs);
            String::new()
        }
        "open" => {
            engine.request_open(&arg(1, "Gerät")?, &arg(2, "Notizbuch")?).map_err(e)?;
            String::new()
        }
        "follow" => {
            engine.set_follow(&arg(1, "Gerät")?, args.get(2).is_some_and(|v| v == "1")).map_err(e)?;
            String::new()
        }
        other => return Err(format!("unbekannter Befehl {other} (n0te-sync help)")),
    })
}
