//! A sync service that accepts but never answers must not freeze the app: requests give up
//! after their limit, and the next quick ones fail at once for a while. Its own process: it
//! points XDG_RUNTIME_DIR at a scratch folder.
#![cfg(unix)]

use std::os::unix::net::UnixListener;
use std::time::{Duration, Instant};

#[test]
fn a_service_that_hangs_costs_one_short_wait_not_a_frozen_window() {
    let dir = std::env::temp_dir().join(format!("n0te-hang-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // SAFETY: set before any other thread of this test process reads the environment.
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", &dir) };
    let listener = UnixListener::bind(dir.join("n0te-sync.sock")).unwrap();
    // Accepts every connection and keeps it open without a word.
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        for s in listener.incoming().flatten() {
            kept.push(s);
        }
    });

    let started = Instant::now();
    let r = n0te_desktop::live::request_within(&["devices-json"], Duration::from_millis(400));
    assert!(r.is_err(), "no answer is an error");
    assert!(started.elapsed() < Duration::from_secs(3), "gave up after its limit: {:?}", started.elapsed());

    // The next quick requests don't wait again.
    let started = Instant::now();
    for _ in 0..5 {
        assert!(n0te_desktop::live::devices().is_empty());
    }
    assert!(started.elapsed() < Duration::from_millis(300), "failed at once: {:?}", started.elapsed());
    assert!(n0te_desktop::live::limit_for("accept") > n0te_desktop::live::limit_for("devices-json"));
    let _ = std::fs::remove_dir_all(&dir);
}
