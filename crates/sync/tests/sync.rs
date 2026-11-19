//! End-to-end: engines on temporary libraries talk over real iroh endpoints on localhost
//! (no relays, no address lookup).

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use n0te_core::ink::{Brush, InkPoint};
use n0te_core::notebook::{Background, SheetSpec};
use n0te_core::store::Library;
use n0te_sync::{Engine, EngineOptions, Event, LiveBrush, Role};

fn root(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("n0te-sync-{tag}-{}-{}", std::process::id(), n0te_core::ids::new_id(21)))
}

fn engine(root: &Path, name: &str) -> Engine {
    let mut o = EngineOptions::new(name);
    o.local_only = true;
    Engine::start(root, o).expect("engine")
}

fn line(y: f32) -> Vec<InkPoint> {
    (0..6).map(|i| InkPoint { x: 10.0 + i as f32 * 4.0, y, pressure: 0.5, t_ms: i * 8 }).collect()
}

/// Waits until `f` holds (polling), panicking with `what` after 20 s.
fn wait(what: &str, mut f: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while Instant::now() < deadline {
        if f() {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("timed out waiting for: {what}");
}

fn strokes(root: &Path, nb: &str, sheet: &str) -> usize {
    Library::open(root).ok().and_then(|l| l.load_sheet(nb, sheet).ok()).map(|s| s.len()).unwrap_or(0)
}

/// A notebook with one sheet and `n` strokes, written through the app's library handle.
fn notebook_with_strokes(root: &Path, title: &str, n: usize) -> (String, String) {
    let lib = Library::open(root).unwrap();
    let nb = lib.create_notebook(title).unwrap();
    let (_, sheet) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Grid)).unwrap();
    lib.save_notebook(&nb).unwrap();
    add_strokes(root, &nb.id(), &sheet, n);
    (nb.id(), sheet)
}

fn add_strokes(root: &Path, nb: &str, sheet: &str, n: usize) {
    let lib = Library::open(root).unwrap();
    let s = lib.load_sheet(nb, sheet).unwrap();
    for i in 0..n {
        s.add_stroke(Brush::default(), &line(20.0 + i as f32 * 10.0)).unwrap();
    }
    lib.save_sheet(nb, sheet, &s).unwrap();
}

fn pair(a: &Engine, b: &Engine) {
    let code = a.pair_code().unwrap();
    let dev = b.accept_pair(&code).unwrap();
    assert_eq!(dev.name, "Laptop");
    wait("both sides list each other", || a.devices().len() == 1 && b.devices().len() == 1);
}

#[test]
fn pairing_needs_the_right_secret() {
    let (ra, rb, rc) = (root("pa"), root("pb"), root("pc"));
    let a = engine(&ra, "Laptop");
    let b = engine(&rb, "Tablet");
    let c = engine(&rc, "Fremd");
    let code = a.pair_code().unwrap();
    // Same address, wrong secret.
    let mut parsed = n0te_sync::proto::PairCode::parse(&code).unwrap();
    parsed.secret[0] ^= 1;
    let err = c.accept_pair(&parsed.to_text()).unwrap_err();
    assert!(matches!(err, n0te_sync::Error::Rejected(_)), "{err}");
    assert!(a.devices().is_empty() && c.devices().is_empty());
    // The right one works, and only once.
    b.accept_pair(&code).unwrap();
    wait("paired", || a.devices().iter().any(|d| d.name == "Tablet") && b.devices().iter().any(|d| d.name == "Laptop"));
    wait("online", || a.devices().iter().all(|d| d.online) && b.devices().iter().all(|d| d.online));
    assert!(c.accept_pair(&code).is_err(), "pair codes are one-time");
}

#[test]
fn strokes_sync_and_offline_edits_merge() {
    let (ra, rb) = (root("sa"), root("sb"));
    let a = engine(&ra, "Laptop");
    let mut b = engine(&rb, "Tablet");
    pair(&a, &b);
    let (nb, sheet) = notebook_with_strokes(&ra, "Analysis", 1);
    a.notify_local_change(&nb);
    wait("notebook and stroke arrive on B", || strokes(&rb, &nb, &sheet) == 1);
    assert_eq!(Library::open(&rb).unwrap().load_notebook(&nb).unwrap().title(), "Analysis");

    // B goes offline; both draw; B comes back: both have all three strokes.
    b.shutdown();
    drop(b);
    add_strokes(&ra, &nb, &sheet, 1);
    add_strokes(&rb, &nb, &sheet, 1);
    let b = engine(&rb, "Tablet");
    wait("A has B's offline stroke", || strokes(&ra, &nb, &sheet) == 3);
    wait("B has A's stroke", || strokes(&rb, &nb, &sheet) == 3);

    // A change saved by the app on B (no notify) is noticed and arrives on A.
    add_strokes(&rb, &nb, &sheet, 1);
    wait("change noticed via stamps", || strokes(&ra, &nb, &sheet) == 4);
    drop(b);
}

#[test]
fn new_notebook_brings_its_pdf_blob() {
    let (ra, rb) = (root("ba"), root("bb"));
    let a = engine(&ra, "Laptop");
    let b = engine(&rb, "Tablet");
    pair(&a, &b);
    let lib = Library::open(&ra).unwrap();
    let pdf: Vec<u8> = (0..700_000u32).map(|i| (i % 251) as u8).collect(); // > 2 chunks
    let blob = lib.put_blob(&pdf).unwrap();
    let nb = lib.create_notebook("Folien").unwrap();
    nb.add_sheet(None, 0, &SheetSpec { pdf: Some((blob.clone(), 0)), ..SheetSpec::a4(Background::Blank) }).unwrap();
    lib.save_notebook(&nb).unwrap();
    a.notify_local_change(&nb.id());
    wait("PDF arrives intact", || Library::open(&rb).unwrap().blob(&blob).map(|d| d == pdf).unwrap_or(false));
}

#[test]
fn guests_get_one_notebook_with_their_rights_and_can_be_revoked() {
    let (ro, rr, rw) = (root("go"), root("gr"), root("gw"));
    let owner = engine(&ro, "Laptop");
    let (nb1, s1) = notebook_with_strokes(&ro, "Geteilt", 1);
    let (nb2, _) = notebook_with_strokes(&ro, "Privat", 1);

    // A read-only guest that (via the test hook) still pushes its edits.
    let mut ro_opts = EngineOptions::new("Gast R");
    ro_opts.local_only = true;
    ro_opts.guest_ignores_role = true;
    let reader = Engine::start(&rr, ro_opts).unwrap();
    let read_link = owner.create_invite(&nb1, Role::Read, 0).unwrap();
    assert_eq!(reader.join(&read_link).unwrap(), nb1);
    wait("reader gets the shared notebook", || strokes(&rr, &nb1, &s1) == 1);
    assert!(Library::open(&rr).unwrap().load_notebook(&nb2).is_err(), "only the invited notebook");
    add_strokes(&rr, &nb1, &s1, 1);
    reader.notify_local_change(&nb1);
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(strokes(&ro, &nb1, &s1), 1, "the owner ignores a read-only guest's edits");

    // A read-write guest's edits arrive; the owner's reach the guest.
    let writer = engine(&rw, "Gast W");
    let rw_link = owner.create_invite(&nb1, Role::ReadWrite, 0).unwrap();
    writer.join(&rw_link).unwrap();
    wait("writer synced", || strokes(&rw, &nb1, &s1) >= 1);
    add_strokes(&rw, &nb1, &s1, 1);
    writer.notify_local_change(&nb1);
    wait("owner has the writer's stroke", || strokes(&ro, &nb1, &s1) == 2);
    assert_eq!(owner.status().guests.len(), 2);

    // Revoke the writer: disconnected, refused when it comes back, owner edits stay home.
    let events = writer.events().unwrap();
    let owner_events = owner.events().unwrap();
    let rw_id = owner.list_invites().into_iter().find(|i| i.role == Role::ReadWrite).unwrap().id;
    owner.revoke(&rw_id).unwrap();
    wait_event(&events, "access revoked", |e| matches!(e, Event::AccessRevoked { notebook } if *notebook == nb1));
    wait_event(&owner_events, "owner sees the guest leave", |e| matches!(e, Event::GuestLeft { invite, .. } if *invite == rw_id));
    let before = strokes(&rw, &nb1, &s1);
    add_strokes(&ro, &nb1, &s1, 1);
    owner.notify_local_change(&nb1);
    wait("the reader still gets owner edits", || strokes(&rr, &nb1, &s1) >= 3);
    assert_eq!(strokes(&rw, &nb1, &s1), before, "revoked guest gets nothing new");
    assert!(writer.joined()[0].revoked);
    assert!(writer.join(&rw_link).is_err(), "joining again is refused");
}

fn wait_event(rx: &Receiver<Event>, what: &str, f: impl Fn(&Event) -> bool) -> Event {
    let deadline = Instant::now() + Duration::from_secs(20);
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        if let Ok(e) = rx.recv_timeout(left)
            && f(&e)
        {
            return e;
        }
    }
    panic!("timed out waiting for event: {what}");
}

#[test]
fn live_presence_strokes_and_open_requests() {
    let (ra, rb) = (root("la"), root("lb"));
    let a = engine(&ra, "Laptop");
    let b = engine(&rb, "Tablet");
    pair(&a, &b);
    wait("online", || a.devices().iter().all(|d| d.online) && b.devices().iter().all(|d| d.online));
    let (nb, sheet) = notebook_with_strokes(&ra, "Live", 0);
    let events = b.events().unwrap();
    let tablet = a.devices()[0].id.clone();
    a.request_open(&tablet, &nb).unwrap();
    wait_event(&events, "open request", |e| matches!(e, Event::OpenRequest { notebook, .. } if *notebook == nb));
    a.send_presence(&nb, Some(&sheet), [0.0, 0.0, 595.0, 842.0], 1.5, Some([100.0, 200.0]));
    let p = wait_event(&events, "presence", |e| matches!(e, Event::Presence { .. }));
    assert!(matches!(p, Event::Presence { zoom, laser: Some([x, _]), .. } if zoom == 1.5 && x == 100.0));
    let samples: Vec<u8> = (0..200).flat_map(|i| [i as f32, 1.0, 0.5, i as f32].into_iter().flat_map(f32::to_le_bytes)).collect();
    let brush = LiveBrush { width: 2.0, thinning: 0.5, color: 0xFF111111, kind: 0 };
    a.send_live_stroke(&nb, &sheet, 7, 0, brush, &samples, true);
    let mut got = 0;
    let mut done = false;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done && Instant::now() < deadline {
        if let Ok(Event::LiveStroke { stroke: 7, samples: s, done: d, .. }) = events.recv_timeout(Duration::from_secs(1)) {
            got += s.len();
            done = d;
        }
    }
    assert!(done && got == samples.len(), "{got} of {} bytes, done={done}", samples.len());
    a.set_follow(&tablet, true).unwrap();
    wait_event(&events, "follow", |e| matches!(e, Event::FollowRequest { on: true, .. }));
}

#[test]
fn history_checkpoint_and_restore_through_the_engine() {
    let r = root("h");
    let e = engine(&r, "Laptop");
    let (nb, sheet) = notebook_with_strokes(&r, "Chemie", 2);
    let pass = e.checkpoint();
    assert_eq!(pass.written.len(), 1, "{pass:?}");
    let cp = e.history(&nb);
    assert_eq!(cp.len(), 1);
    add_strokes(&r, &nb, &sheet, 1);
    let copy = e.restore_as_copy(&nb, cp[0].at).unwrap();
    assert_eq!(strokes(&r, &copy, &sheet), 2);
    assert_eq!(strokes(&r, &nb, &sheet), 3);
}
