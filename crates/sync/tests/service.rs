//! The service binary end to end: a GUI session on its unix socket gets engine events as JSON
//! lines and sends live commands; one-shot JSON queries work.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use n0te_sync::{Engine, EngineOptions, Event};
use serde_json::Value;

fn root(tag: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("n0te-svc-{tag}-{}-{}", std::process::id(), n0te_core::ids::new_id(23)));
    std::fs::create_dir_all(&p).unwrap();
    p
}

fn request(sock: &Path, args: &[&str]) -> Value {
    let mut s = UnixStream::connect(sock).unwrap();
    writeln!(s, "{}", serde_json::to_string(args).unwrap()).unwrap();
    let mut line = String::new();
    BufReader::new(s).read_line(&mut line).unwrap();
    serde_json::from_str(&line).unwrap()
}

fn recv_until<T>(rx: &mpsc::Receiver<T>, what: &str, mut f: impl FnMut(&T) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(25);
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        if let Ok(v) = rx.recv_timeout(left)
            && f(&v)
        {
            return;
        }
    }
    panic!("timed out waiting for {what}");
}

struct Kill(std::process::Child);
impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
    }
}

#[test]
fn gui_session_gets_events_and_sends_live_commands() {
    let (rt, laptop, tablet_root) = (root("rt"), root("laptop"), root("tablet"));
    let _svc = Kill(
        Command::new(env!("CARGO_BIN_EXE_n0te-sync"))
            .env("N0TE_LIBRARY", &laptop)
            .env("XDG_RUNTIME_DIR", &rt)
            .env("N0TE_SYNC_LOCAL_ONLY", "1")
            .env("N0TE_DEVICE_NAME", "Laptop")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("service binary"),
    );
    let sock = rt.join("n0te-sync.sock");
    let t0 = Instant::now();
    while !sock.exists() || UnixStream::connect(&sock).is_err() {
        assert!(t0.elapsed() < Duration::from_secs(20), "service socket");
        std::thread::sleep(Duration::from_millis(50));
    }

    // A GUI session: lines in a channel.
    let mut session = UnixStream::connect(&sock).unwrap();
    writeln!(session, "[\"session\"]").unwrap();
    let (tx, lines) = mpsc::channel::<Value>();
    let read = session.try_clone().unwrap();
    std::thread::spawn(move || {
        for l in BufReader::new(read).lines().map_while(Result::ok) {
            if let Ok(v) = serde_json::from_str::<Value>(&l) {
                let _ = tx.send(v);
            }
        }
    });
    recv_until(&lines, "hello", |v| v["t"] == "hello");

    // Pair a "tablet" engine with a code fetched over the socket.
    let code = request(&sock, &["pair-code"])["out"].as_str().unwrap().to_string();
    let (etx, tablet_events) = mpsc::channel::<Event>();
    let etx = Mutex::new(etx);
    let mut o = EngineOptions::new("Tablet");
    o.local_only = true;
    o.listener = Some(Arc::new(move |e| {
        let _ = etx.lock().unwrap().send(e);
    }));
    let tablet = Engine::start(&tablet_root, o).unwrap();
    tablet.accept_pair(&code).expect("pairing");
    recv_until(&lines, "paired/online", |v| v["t"] == "paired" || v["t"] == "online");
    let devices = request(&sock, &["devices-json"]);
    let list: Value = serde_json::from_str(devices["out"].as_str().unwrap()).unwrap();
    assert_eq!(list[0]["name"], "Tablet");

    // Tablet → GUI: presence arrives as a JSON line.
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut got = false;
    while !got && Instant::now() < deadline {
        tablet.send_presence("nb1", Some("s1"), [1.0, 2.0, 300.0, 400.0], 1.5, Some([5.0, 6.0]));
        got = matches!(lines.recv_timeout(Duration::from_millis(300)), Ok(v) if v["t"] == "presence" && v["notebook"] == "nb1" && v["laser"][0] == 5.0);
    }
    assert!(got, "presence reached the GUI session");

    // GUI → tablet: an open request through the session.
    let tablet_id = tablet.node_id();
    writeln!(session, "{}", serde_json::to_string(&["open", tablet_id.as_str(), "nb1"]).unwrap()).unwrap();
    recv_until(&tablet_events, "open request", |e| matches!(e, Event::OpenRequest { notebook, .. } if notebook == "nb1"));
}
