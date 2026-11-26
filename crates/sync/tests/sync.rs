//! End-to-end: engines on temporary libraries talk over real iroh endpoints on localhost
//! (no relays, no address lookup).

use std::path::{Path, PathBuf};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use n0te_core::ink::{Brush, InkPoint};
use n0te_core::notebook::{Background, SheetSpec};
use n0te_core::store::Library;
use n0te_sync::{Engine, EngineOptions, Event, InviteOptions, LiveBrush, Role};

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

#[test]
fn owner_switches_a_guest_between_read_and_write_and_sees_its_live_view() {
    let (ro, rg) = (root("ro"), root("rg"));
    let owner = engine(&ro, "Laptop");
    let (nb, s1) = notebook_with_strokes(&ro, "Rollen", 1);
    let guest = engine(&rg, "Mia");
    let owner_events = owner.events().unwrap();
    let guest_events = guest.events().unwrap();
    let link = owner.create_invite(&nb, Role::Read, 0).unwrap();
    guest.join(&link).unwrap();
    wait("guest synced", || strokes(&rg, &nb, &s1) == 1);
    // The owner learns who joined, with the id their live view carries.
    let device = match wait_event(&owner_events, "guest joined", |e| matches!(e, Event::GuestJoined { .. })) {
        Event::GuestJoined { name, device, .. } => {
            assert_eq!(name, "Mia");
            device
        }
        _ => unreachable!(),
    };
    assert_eq!(owner.status().guests[0].device, device);
    guest.send_presence(&nb, Some(&s1), [0.0, 0.0, 300.0, 400.0], 1.0, None);
    wait_event(&owner_events, "the guest's view", |e| matches!(e, Event::Presence { device: d, .. } if *d == device));

    // Read → write: the guest is told, and its edits arrive.
    let id = owner.list_invites()[0].id.clone();
    owner.set_role(&id, Role::ReadWrite).unwrap();
    wait_event(&guest_events, "rights changed", |e| matches!(e, Event::RoleChanged { write: true, .. }));
    assert_eq!(guest.joined()[0].role, Role::ReadWrite);
    wait("guest back online", || owner.status().guests.len() == 1);
    add_strokes(&rg, &nb, &s1, 1);
    guest.notify_local_change(&nb);
    wait("the owner has the guest's stroke", || strokes(&ro, &nb, &s1) == 2);

    // And back to read-only.
    owner.set_role(&id, Role::Read).unwrap();
    wait_event(&guest_events, "read-only again", |e| matches!(e, Event::RoleChanged { write: false, .. }));
    assert_eq!(owner.list_invites()[0].role, Role::Read);

    // The owner can keep where it looks to itself.
    wait("guest back again", || owner.status().guests.len() == 1);
    owner.set_share_view(false);
    // The guest is told once that the owner looks nowhere (its frame goes) ...
    wait_event(&guest_events, "frame cleared", |e| matches!(e, Event::Presence { sheet: None, .. }));
    owner.send_presence(&nb, Some(&s1), [0.0, 0.0, 100.0, 100.0], 2.0, None);
    std::thread::sleep(Duration::from_millis(800));
    // ... and then gets nothing more.
    assert!(guest_events.try_iter().all(|e| !matches!(e, Event::Presence { .. })), "no presence to guests when not shared");
    owner.set_share_view(true);
    owner.send_presence(&nb, Some(&s1), [0.0, 0.0, 100.0, 100.0], 2.0, None);
    wait_event(&guest_events, "presence when shared", |e| matches!(e, Event::Presence { .. }));
}

#[test]
fn one_guest_can_be_removed_the_others_stay_and_it_can_be_let_back() {
    let (ro, ra, rb) = (root("rmo"), root("rma"), root("rmb"));
    let owner = engine(&ro, "Laptop");
    let (nb, s1) = notebook_with_strokes(&ro, "Entfernen", 1);
    let (mia, ben) = (engine(&ra, "Mia"), engine(&rb, "Ben"));
    let mia_events = mia.events().unwrap();
    let link = owner.create_invite(&nb, Role::Read, 0).unwrap();
    mia.join(&link).unwrap();
    ben.join(&link).unwrap();
    wait("both guests in", || owner.status().guests.len() == 2);
    wait("mia synced", || strokes(&ra, &nb, &s1) == 1);
    let dev = |name: &str| owner.status().guests.iter().find(|g| g.name == name).map(|g| g.device.clone()).unwrap();
    let mia_dev = dev("Mia");

    owner.remove_guest(&mia_dev).unwrap();
    // Mia is out for good (her reconnect is turned away), Ben stays.
    wait_event(&mia_events, "mia told she is out", |e| matches!(e, Event::AccessRevoked { .. }));
    assert!(mia.joined()[0].revoked);
    std::thread::sleep(Duration::from_millis(500));
    let names: Vec<String> = owner.status().guests.iter().map(|g| g.name.clone()).collect();
    assert_eq!(names, vec!["Ben".to_string()]);
    let inv = owner.list_invites()[0].clone();
    assert_eq!((inv.banned.len(), inv.banned[0].name.as_str(), inv.members), (1, "Mia", 1));
    let err = mia.join(&link).unwrap_err().to_string();
    assert!(err.contains("entfernt"), "{err}");

    // Let back in: the same link works again.
    owner.allow_again(&inv.id, &mia_dev).unwrap();
    mia.join(&link).unwrap();
    wait("mia back", || owner.status().guests.len() == 2);
}

#[test]
fn pin_approval_single_use_and_a_changed_expiry() {
    let (ro, ra, rb) = (root("gto"), root("gta"), root("gtb"));
    let owner = engine(&ro, "Laptop");
    let (nb, s1) = notebook_with_strokes(&ro, "Tür", 1);
    let (mia, ben) = (engine(&ra, "Mia"), engine(&rb, "Ben"));
    let owner_events = owner.events().unwrap();
    let mia_events = mia.events().unwrap();
    let link = owner.create_invite_with(&nb, Role::Read, 0, InviteOptions { approve: true, single: true, pin: Some("4821".into()) }).unwrap();
    let id = owner.list_invites()[0].id.clone();

    // PIN first.
    let err = mia.join(&link).unwrap_err().to_string();
    assert!(err.contains("PIN nötig"), "{err}");
    let err = mia.join_with(&link, Some("1111")).unwrap_err().to_string();
    assert!(err.contains("PIN falsch"), "{err}");

    // Right PIN: Mia waits at the door until the owner lets her in.
    std::thread::scope(|sc| {
        let joining = sc.spawn(|| mia.join_with(&link, Some("4821")));
        let device = match wait_event(&owner_events, "mia knocks", |e| matches!(e, Event::GuestWaiting { on: true, .. })) {
            Event::GuestWaiting { name, device, .. } => {
                assert_eq!(name, "Mia");
                device
            }
            _ => unreachable!(),
        };
        assert_eq!(owner.status().waiting.len(), 1);
        wait_event(&mia_events, "mia is told to wait", |e| matches!(e, Event::JoinWaiting { owner } if owner == "Laptop"));
        owner.admit(&device, true).unwrap();
        assert_eq!(joining.join().unwrap().unwrap(), nb);
    });
    wait("mia synced", || strokes(&ra, &nb, &s1) == 1);
    assert!(owner.status().waiting.is_empty());
    assert_eq!(owner.list_invites()[0].members, 1);

    // Single use: nobody else gets in, even with the PIN.
    let err = ben.join_with(&link, Some("4821")).unwrap_err().to_string();
    assert!(err.contains("schon benutzt"), "{err}");

    // Mia, once in, comes back without PIN or approval (a role change reconnects her).
    owner.set_role(&id, Role::ReadWrite).unwrap();
    wait_event(&mia_events, "rights changed", |e| matches!(e, Event::RoleChanged { write: true, .. }));
    wait("mia back without asking", || owner.status().guests.len() == 1);
    assert!(owner.status().waiting.is_empty());

    // Expiry: the owner's date counts, not the one in the link.
    let short = owner.create_invite(&nb, Role::Read, n0te_sync::proto::now_secs() + 1).unwrap();
    let short_id = owner.list_invites().iter().find(|i| i.link == short).unwrap().id.clone();
    std::thread::sleep(Duration::from_millis(2100));
    let err = ben.join(&short).unwrap_err().to_string();
    assert!(err.contains("abgelaufen"), "{err}");
    owner.set_expiry(&short_id, n0te_sync::proto::now_secs() + 3600).unwrap();
    ben.join(&short).unwrap();
    wait("ben in with the extended link", || owner.status().guests.len() == 2);

    // Turned away at the door.
    let rc = root("gtc");
    let cleo = engine(&rc, "Cleo");
    let door = owner.create_invite_with(&nb, Role::Read, 0, InviteOptions { approve: true, ..Default::default() }).unwrap();
    std::thread::scope(|sc| {
        let joining = sc.spawn(|| cleo.join(&door));
        let device = match wait_event(&owner_events, "cleo knocks", |e| matches!(e, Event::GuestWaiting { on: true, name, .. } if name == "Cleo")) {
            Event::GuestWaiting { device, .. } => device,
            _ => unreachable!(),
        };
        owner.admit(&device, false).unwrap();
        let err = joining.join().unwrap().unwrap_err().to_string();
        assert!(err.contains("nicht hereingelassen"), "{err}");
    });
}

#[test]
fn a_new_invite_to_the_same_notebook_replaces_the_old_membership() {
    let (ro, rg) = (root("rpo"), root("rpg"));
    let owner = engine(&ro, "Laptop");
    let (nb, s1) = notebook_with_strokes(&ro, "Zweimal", 1);
    let guest = engine(&rg, "Mia");
    let first = owner.create_invite(&nb, Role::Read, 0).unwrap();
    guest.join(&first).unwrap();
    wait("in with the first invite", || owner.status().guests.len() == 1);
    // A wrong PIN on the new invite keeps the old membership.
    let second = owner.create_invite_with(&nb, Role::ReadWrite, 0, InviteOptions { pin: Some("1234".into()), ..Default::default() }).unwrap();
    assert!(guest.join_with(&second, Some("9999")).is_err());
    assert_eq!(guest.joined().len(), 1);
    wait("still in with the first", || owner.status().guests.len() == 1);
    // The right PIN: one membership, the new one, and it connects (not refused as a duplicate).
    guest.join_with(&second, Some("1234")).unwrap();
    let j = guest.joined();
    assert_eq!((j.len(), j[0].role), (1, Role::ReadWrite));
    let second_id = owner.list_invites().iter().find(|i| i.link == second).unwrap().id.clone();
    wait("connected with the second invite", || owner.status().guests.iter().any(|g| g.invite == second_id) && guest.joined()[0].online);
    assert_eq!(strokes(&rg, &nb, &s1), 1);
}

#[test]
fn the_owner_summons_guests_to_its_view_but_not_the_other_way() {
    let (ro, ra, rb) = (root("smo"), root("sma"), root("smb"));
    let owner = engine(&ro, "Laptop");
    let (nb, s1) = notebook_with_strokes(&ro, "Holen", 1);
    let (mia, ben) = (engine(&ra, "Mia"), engine(&rb, "Ben"));
    let owner_events = owner.events().unwrap();
    let (mia_events, ben_events) = (mia.events().unwrap(), ben.events().unwrap());
    let link = owner.create_invite(&nb, Role::Read, 0).unwrap();
    mia.join(&link).unwrap();
    ben.join(&link).unwrap();
    wait("both in", || owner.status().guests.len() == 2);
    let mia_dev = owner.status().guests.iter().find(|g| g.name == "Mia").unwrap().device.clone();

    // One guest: only Mia comes.
    assert_eq!(owner.summon(&nb, Some(&mia_dev), &s1, [10.0, 20.0, 310.0, 420.0]).unwrap(), 1);
    match wait_event(&mia_events, "mia summoned", |e| matches!(e, Event::Summoned { .. })) {
        Event::Summoned { notebook, sheet, viewport, .. } => assert_eq!((notebook, sheet, viewport), (nb.clone(), s1.clone(), [10.0, 20.0, 310.0, 420.0])),
        _ => unreachable!(),
    }
    std::thread::sleep(Duration::from_millis(500));
    assert!(ben_events.try_iter().all(|e| !matches!(e, Event::Summoned { .. })), "ben stays where he is");

    // Everyone.
    assert_eq!(owner.summon(&nb, None, &s1, [0.0, 0.0, 100.0, 100.0]).unwrap(), 2);
    wait_event(&ben_events, "ben summoned", |e| matches!(e, Event::Summoned { .. }));
    wait_event(&mia_events, "mia summoned again", |e| matches!(e, Event::Summoned { .. }));

    // A guest has no one to summon (its only session is the owner, who isn't its guest).
    assert!(mia.summon(&nb, None, &s1, [0.0, 0.0, 1.0, 1.0]).is_err());
    std::thread::sleep(Duration::from_millis(300));
    assert!(owner_events.try_iter().all(|e| !matches!(e, Event::Summoned { .. })));
}

#[test]
fn two_memberships_of_one_notebook_left_by_an_older_version_become_one() {
    let r = root("dup");
    std::fs::create_dir_all(r.join("sync")).unwrap();
    let rec = |invite: &str, role: &str| serde_json::json!({ "link": "n0te://invite/x", "invite": invite, "notebook": "nb1", "title": "T", "role": role, "owner": "own1", "owner_name": "Alex", "revoked": false });
    let other = serde_json::json!({ "link": "n0te://invite/y", "invite": "c", "notebook": "nb2", "title": "U", "role": "Read", "owner": "own1", "owner_name": "Alex", "revoked": false });
    std::fs::write(r.join("sync/joined.json"), serde_json::to_vec(&vec![rec("a", "Read"), other, rec("b", "ReadWrite")]).unwrap()).unwrap();
    let e = engine(&r, "Tablet");
    let j: Vec<(String, String)> = e.joined().into_iter().map(|j| (j.invite, j.notebook)).collect();
    assert_eq!(j, vec![("c".to_string(), "nb2".to_string()), ("b".to_string(), "nb1".to_string())], "the newest of nb1 stays, nb2 untouched");
}

#[test]
fn a_guest_of_two_notebooks_of_one_owner_stays_in_both() {
    // Before 0.34.1 the two connections replaced each other about every second: the owner saw
    // the guest come and go, and the guest's writing hardly ever arrived.
    let (ro, rg) = (root("two-o"), root("two-g"));
    let owner = engine(&ro, "Tablet");
    let (nb1, s1) = notebook_with_strokes(&ro, "Mathe", 1);
    let (nb2, s2) = notebook_with_strokes(&ro, "Physik", 1);
    let owner_events = owner.events().unwrap();
    {
        let guest = engine(&rg, "Mia");
        guest.join(&owner.create_invite(&nb1, Role::ReadWrite, 0).unwrap()).unwrap();
        guest.join(&owner.create_invite(&nb2, Role::ReadWrite, 0).unwrap()).unwrap();
        wait("the guest is in both", || owner.status().guests.len() == 2 && guest.joined().iter().all(|j| j.online));
        wait("both notebooks arrived", || strokes(&rg, &nb1, &s1) == 1 && strokes(&rg, &nb2, &s2) == 1);
        // Steady: no more comings and goings once both are in.
        std::thread::sleep(Duration::from_secs(3));
        let flaps = owner_events.try_iter().filter(|e| matches!(e, Event::GuestJoined { .. } | Event::GuestLeft { .. })).count();
        assert!(flaps <= 2, "{flaps} joins/leaves: the connections take turns");
        // Writing in either notebook reaches the owner, saved and live.
        add_strokes(&rg, &nb1, &s1, 1);
        guest.notify_local_change(&nb1);
        add_strokes(&rg, &nb2, &s2, 2);
        guest.notify_local_change(&nb2);
        wait("the owner has the guest's strokes", || strokes(&ro, &nb1, &s1) == 2 && strokes(&ro, &nb2, &s2) == 3);
        let brush = LiveBrush { width: 2.0, thinning: 0.5, color: 0xFF111111, kind: 0 };
        guest.send_live_stroke(&nb2, &s2, 9, 0, brush, &[0u8; 64], true);
        wait_event(&owner_events, "the guest's live stroke", |e| matches!(e, Event::LiveStroke { notebook, stroke: 9, .. } if *notebook == nb2));
    }
    // After a restart the guest comes back into both, side by side.
    let guest = engine(&rg, "Mia");
    wait("back in both", || owner.status().guests.len() == 2 && guest.joined().iter().all(|j| j.online));
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(owner.status().guests.len(), 2, "still both");
}

#[test]
fn joining_the_same_invite_again_keeps_one_steady_connection() {
    let (ro, rg) = (root("again-o"), root("again-g"));
    let owner = engine(&ro, "Tablet");
    let (nb, s1) = notebook_with_strokes(&ro, "Mathe", 1);
    let owner_events = owner.events().unwrap();
    let guest = engine(&rg, "Mia");
    let link = owner.create_invite(&nb, Role::ReadWrite, 0).unwrap();
    guest.join(&link).unwrap();
    wait("in", || owner.status().guests.len() == 1);
    // The link opened a second time: still one connection, and it stays.
    guest.join(&link).unwrap();
    std::thread::sleep(Duration::from_secs(2));
    let _ = owner_events.try_iter().count();
    std::thread::sleep(Duration::from_secs(5));
    let flaps = owner_events.try_iter().filter(|e| matches!(e, Event::GuestJoined { .. } | Event::GuestLeft { .. })).count();
    assert_eq!(flaps, 0, "the guest keeps coming and going");
    assert_eq!((owner.status().guests.len(), guest.joined().len()), (1, 1));
    add_strokes(&rg, &nb, &s1, 1);
    guest.notify_local_change(&nb);
    wait("the owner has the guest's stroke", || strokes(&ro, &nb, &s1) == 2);
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

/// Scribbles out (deletes) `n` strokes of the sheet, as the app does (one edit, saved).
fn scribble_out(root: &Path, nb: &str, sheet: &str, n: usize) {
    let lib = Library::open(root).unwrap();
    let s = lib.load_sheet(nb, sheet).unwrap();
    let ids: Vec<String> = s.strokes().into_iter().take(n).map(|st| st.id).collect();
    s.scribble_out(&n0te_core::scribble::Verdict { delete: ids, cut: vec![], path: vec![] }).unwrap();
    lib.save_sheet(nb, sheet, &s).unwrap();
}

#[test]
fn what_a_guest_scribbles_out_goes_at_the_owner_and_the_other_way_round() {
    // Reported on 0.35.0: something a guest scribbled out was sometimes still there at the
    // owner (and the other way round).
    let (ro, rg) = (root("scr-o"), root("scr-g"));
    let owner = engine(&ro, "Laptop");
    let (nb, sheet) = notebook_with_strokes(&ro, "Gemeinsam", 4);
    let owner_events = owner.events().unwrap();
    let guest = engine(&rg, "Mia");
    let guest_events = guest.events().unwrap();
    guest.join(&owner.create_invite(&nb, Role::ReadWrite, 0).unwrap()).unwrap();
    wait("the guest has the notebook", || strokes(&rg, &nb, &sheet) == 4);
    // The guest scribbles one out: the live stroke goes at once, the stroke with the save.
    guest.send_live_cancel(&nb, &sheet, 77);
    wait_event(&owner_events, "the guest's scribble dropped", |e| matches!(e, Event::LiveCancel { stroke: 77, .. }));
    scribble_out(&rg, &nb, &sheet, 1);
    guest.notify_local_change(&nb);
    wait("gone at the owner", || strokes(&ro, &nb, &sheet) == 3);
    // The owner scribbles two out: gone at the guest.
    owner.send_live_cancel(&nb, &sheet, 78);
    wait_event(&guest_events, "the owner's scribble dropped", |e| matches!(e, Event::LiveCancel { stroke: 78, .. }));
    scribble_out(&ro, &nb, &sheet, 2);
    owner.notify_local_change(&nb);
    wait("gone at the guest", || strokes(&rg, &nb, &sheet) == 1);
    // Both at once, offline from each other's view: both deletions stay.
    add_strokes(&ro, &nb, &sheet, 3);
    owner.notify_local_change(&nb);
    wait("the guest has the new ones", || strokes(&rg, &nb, &sheet) == 4);
    scribble_out(&rg, &nb, &sheet, 1);
    scribble_out(&ro, &nb, &sheet, 1);
    guest.notify_local_change(&nb);
    owner.notify_local_change(&nb);
    wait("the same on both", || strokes(&ro, &nb, &sheet) == strokes(&rg, &nb, &sheet) && strokes(&ro, &nb, &sheet) <= 3);
}

#[test]
fn the_laser_reaches_guests_through_the_owner_and_guests_point_back() {
    // The tablet points, the guest is connected to the laptop: before 0.36 the laptop showed
    // the tablet's laser but never passed it on.
    let (rl, rt, rg) = (root("las-l"), root("las-t"), root("las-g"));
    let laptop = engine(&rl, "Laptop");
    let tablet = engine(&rt, "Tablet");
    pair(&laptop, &tablet);
    wait("online", || laptop.devices().iter().all(|d| d.online) && tablet.devices().iter().all(|d| d.online));
    let (nb, sheet) = notebook_with_strokes(&rl, "Vortrag", 1);
    wait("the tablet has the notebook", || strokes(&rt, &nb, &sheet) == 1);
    let guest = engine(&rg, "Mia");
    let (guest_events, tablet_events, laptop_events) = (guest.events().unwrap(), tablet.events().unwrap(), laptop.events().unwrap());
    guest.join(&laptop.create_invite(&nb, Role::Read, 0).unwrap()).unwrap();
    wait("the guest is in", || guest.joined().iter().all(|j| j.online));

    let pts: Vec<[f32; 3]> = (0..10).map(|i| [100.0 + i as f32, 200.0, i as f32 * 16.0]).collect();
    tablet.send_laser(&nb, &sheet, &pts, true);
    let e = wait_event(&guest_events, "the tablet's laser at the guest", |e| matches!(e, Event::Laser { .. }));
    assert!(matches!(&e, Event::Laser { who, samples, guest: false, down: true, .. } if *who == tablet.node_id() && samples.len() == 10), "{e:?}");
    wait_event(&laptop_events, "the tablet's laser at the laptop", |e| matches!(e, Event::Laser { guest: false, .. }));

    // A read-only guest points too: the owner's own devices see it, marked as a guest's.
    guest.send_laser(&nb, &sheet, &pts[..3], false);
    let e = wait_event(&tablet_events, "the guest's laser at the tablet", |e| matches!(e, Event::Laser { .. }));
    assert!(matches!(&e, Event::Laser { who, guest: true, down: false, .. } if *who == guest.node_id()), "{e:?}");
    // Nothing comes back round to the one who pointed.
    std::thread::sleep(Duration::from_millis(400));
    assert!(guest_events.try_iter().all(|e| !matches!(e, Event::Laser { who, .. } if who == guest.node_id())));
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
    // Erasing shows live too: hidden strokes and area-eraser dabs (many ids are split up).
    let ids: Vec<String> = (0..90).map(|i| format!("stroke{i:03}")).collect();
    a.send_live_erase(&nb, &sheet, &ids, &[[10.0, 20.0, 3.0], [12.0, 21.0, 3.5]]);
    let (mut got_ids, mut got_dabs) = (Vec::new(), Vec::new());
    let deadline = Instant::now() + Duration::from_secs(10);
    while (got_ids.len() < ids.len() || got_dabs.len() < 2) && Instant::now() < deadline {
        if let Ok(Event::LiveErase { ids, dabs, sheet: sh, .. }) = events.recv_timeout(Duration::from_secs(1)) {
            assert_eq!(sh, sheet);
            got_ids.extend(ids);
            got_dabs.extend(dabs);
        }
    }
    got_ids.sort();
    assert_eq!(got_ids, ids);
    assert_eq!(got_dabs, vec![[10.0, 20.0, 3.0], [12.0, 21.0, 3.5]]);
    a.set_follow(&tablet, true).unwrap();
    wait_event(&events, "follow", |e| matches!(e, Event::FollowRequest { on: true, .. }));
    // The laptop asks for a picture of the tablet's screen; it answers with one.
    let a_events = a.events().unwrap();
    a.request_screenshot(&tablet).unwrap();
    let laptop = match wait_event(&events, "screenshot request", |e| matches!(e, Event::ScreenshotRequest { .. })) {
        Event::ScreenshotRequest { device } => device,
        _ => unreachable!(),
    };
    let jpeg: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    b.send_screenshot(&laptop, &jpeg).unwrap();
    match wait_event(&a_events, "screenshot", |e| matches!(e, Event::Screenshot { .. })) {
        Event::Screenshot { image, .. } => assert_eq!(image, jpeg, "the picture arrives whole"),
        _ => unreachable!(),
    }
}

#[test]
fn a_damaged_document_is_repaired_from_a_paired_device() {
    // A file cut short here (a full disk, a broken copy): the engine announces it has nothing
    // of it, the other device sends everything, and the page and index are whole again.
    let (ra, rb) = (root("da"), root("db"));
    let (nb, sheet) = notebook_with_strokes(&ra, "Repariert", 2);
    let a = engine(&ra, "Laptop");
    let b = engine(&rb, "Tablet");
    pair(&a, &b);
    a.notify_local_change(&nb);
    wait("B has the notebook", || strokes(&rb, &nb, &sheet) == 2);
    drop(b);
    let dir = rb.join("notebooks").join(&nb);
    let cut = |p: PathBuf| {
        let bytes = std::fs::read(&p).unwrap();
        std::fs::write(&p, &bytes[..bytes.len() / 2]).unwrap();
    };
    cut(dir.join("index.loro"));
    cut(dir.join("sheets").join(format!("{sheet}.loro")));
    assert!(Library::open(&rb).unwrap().load_notebook(&nb).is_err());
    let b = engine(&rb, "Tablet");
    wait("B's page is whole again", || strokes(&rb, &nb, &sheet) == 2);
    wait("B's index is whole again", || Library::open(&rb).unwrap().load_notebook(&nb).is_ok_and(|n| n.title() == "Repariert"));
    drop((a, b));
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
