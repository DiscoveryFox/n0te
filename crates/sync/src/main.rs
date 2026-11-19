//! `n0te-sync` — keeps the laptop's n0te library in sync with paired devices and guests.
//!
//!   n0te-sync                         run the service (systemd user unit n0te-sync.service)
//!   n0te-sync pair-code               code for pairing a device (tablet: paste it in n0te)
//!   n0te-sync accept <code>           pair with the device that made <code>
//!   n0te-sync devices | status
//!   n0te-sync invite <notebook> [--rw] [--days N]    guest link (read-only unless --rw)
//!   n0te-sync invites | revoke <invite-id>
//!   n0te-sync join <link> | leave <invite-id>
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
use n0te_sync::{Engine, EngineOptions, Event, Role};

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
    opts.listener = Some(Arc::new(|e: Event| {
        if let Some(s) = describe(&e) {
            log(&s);
        }
    }));
    let engine = Arc::new(Engine::start(&root, opts).map_err(|e| e.to_string())?);
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
    let mut line = String::new();
    BufReader::new(conn.try_clone()?).read_line(&mut line)?;
    let args: Vec<String> = serde_json::from_str(&line).unwrap_or_default();
    let reply = match dispatch(engine, &args) {
        Ok(out) => serde_json::json!({ "ok": true, "out": out }),
        Err(e) => serde_json::json!({ "ok": false, "out": e }),
    };
    let mut w = conn;
    writeln!(w, "{reply}")
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
            engine.create_invite(&nb, if rw { Role::ReadWrite } else { Role::Read }, expires).map_err(e)?
        }
        "invites" => engine
            .list_invites()
            .iter()
            .map(|i| format!("{}  {:<20} {:?}{}{}  {} verbunden", i.id, i.title, i.role, if i.revoked { " widerrufen" } else { "" }, if i.expires > 0 { format!(" bis {}", local_time(i.expires)) } else { String::new() }, i.online))
            .collect::<Vec<_>>()
            .join("\n"),
        "revoke" => {
            engine.revoke(&arg(1, "Einladungs-ID")?).map_err(e)?;
            "Widerrufen.".into()
        }
        "join" => {
            let nb = engine.join(&arg(1, "Link")?).map_err(e)?;
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
        other => return Err(format!("unbekannter Befehl {other} (n0te-sync help)")),
    })
}
