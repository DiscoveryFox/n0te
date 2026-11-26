//! The library on disk under stress: damaged and truncated documents, hostile bundles,
//! several writers at once, leftovers of a crash. Nothing may panic, one damaged file must not
//! take other pages or notebooks with it, and no edit may get lost.

mod common;

use std::sync::Arc;

use common::{Rng, TempLib};
use n0te_core::ink::{Brush, InkPoint};
use n0te_core::notebook::{Background, Notebook, SheetSpec};
use n0te_core::sheet::Sheet;
use n0te_core::store::Library;

fn line(x: f32, n: usize) -> Vec<InkPoint> {
    (0..n).map(|i| InkPoint { x: x + i as f32, y: 10.0 + i as f32 * 0.5, pressure: 0.5, t_ms: i as u32 * 5 }).collect()
}

/// A notebook with two drawn pages; returns (notebook id, sheet ids).
fn notebook_with_pages(lib: &Library) -> (String, Vec<String>) {
    let nb = lib.create_notebook("Analysis").unwrap();
    let (_, s1) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Grid)).unwrap();
    let (_, s2) = nb.add_sheet(None, 1, &SheetSpec::a4(Background::Lines)).unwrap();
    lib.save_notebook(&nb).unwrap();
    for (k, s) in [&s1, &s2].into_iter().enumerate() {
        let sheet = lib.load_sheet(&nb.id(), s).unwrap();
        sheet.add_stroke(Brush::default(), &line(k as f32 * 100.0, 20)).unwrap();
        sheet.add_text_box(10.0, 10.0, 100.0, 12.0, "Grenzwert").unwrap();
        lib.save_sheet(&nb.id(), s, &sheet).unwrap();
    }
    (nb.id(), vec![s1, s2])
}

/// Damage done to a document file: cut short, bytes flipped, garbage, empty.
fn damage(r: &mut Rng, good: &[u8]) -> Vec<u8> {
    let mut b = good.to_vec();
    match r.below(5) {
        0 => b.truncate(r.below(b.len().max(1))),
        1 => {
            for _ in 0..1 + r.below(8) {
                let i = r.below(b.len().max(1));
                if i < b.len() {
                    b[i] ^= 1 << r.below(8);
                }
            }
        }
        2 => {
            let n = r.below(300);
            b = r.bytes(n)
        }
        3 => b.clear(),
        _ => {
            let i = r.below(b.len().max(1));
            let n = 1 + r.below(64);
            let junk = r.bytes(n);
            b.splice(i..i, junk);
        }
    }
    b
}

#[test]
fn damaged_documents_never_panic_and_stay_local() {
    let t = TempLib::new("damaged");
    let lib = t.open();
    let (id, sheets) = notebook_with_pages(&lib);
    let other = lib.create_notebook("Other").unwrap().id();
    let sheet_path = t.path().join("notebooks").join(&id).join("sheets").join(format!("{}.loro", sheets[0]));
    let index_path = t.path().join("notebooks").join(&id).join("index.loro");
    let good_sheet = std::fs::read(&sheet_path).unwrap();
    let good_index = std::fs::read(&index_path).unwrap();
    let mut r = Rng::new(11);
    for _ in 0..150 {
        std::fs::write(&sheet_path, damage(&mut r, &good_sheet)).unwrap();
        std::fs::write(&index_path, damage(&mut r, &good_index)).unwrap();
        // Whatever loads, loads; what doesn't, is an error — never a panic.
        if let Ok(s) = lib.load_sheet(&id, &sheets[0]) {
            let _ = (s.strokes(), s.text_boxes(), s.plots(), s.images(), s.headings(), s.index_lines());
        }
        if let Ok(nb) = lib.load_notebook(&id) {
            let _ = (nb.outline(true), nb.sheet_order(), nb.title());
        }
        // The other page and the other notebook don't care.
        assert_eq!(lib.load_sheet(&id, &sheets[1]).unwrap().strokes().len(), 1);
        assert_eq!(lib.load_notebook(&other).unwrap().title(), "Other");
        assert!(lib.notebook_ids().unwrap().contains(&other));
        let _ = lib.export_bundle(&id);
        let _ = lib.recent_sheets(10);
        let _ = lib.image_blobs(&id);
    }
}

#[test]
fn random_bytes_as_documents_or_updates_never_panic() {
    let mut r = Rng::new(12);
    let good = {
        let s = Sheet::create(1).unwrap();
        s.add_stroke(Brush::default(), &line(0.0, 30)).unwrap();
        s.add_text_box(0.0, 0.0, 50.0, 12.0, "ä").unwrap();
        s.snapshot().unwrap()
    };
    let nb_good = Notebook::create(1, "x").unwrap().snapshot().unwrap();
    for _ in 0..400 {
        let src = if r.chance(50) { &good } else { &nb_good };
        let bytes = damage(&mut r, src);
        if let Ok(s) = Sheet::load(2, &bytes) {
            let _ = s.strokes();
        }
        let _ = Notebook::load(2, &bytes).map(|n| n.outline(true));
        let s = Sheet::create(3).unwrap();
        let _ = s.import(&bytes);
        let _ = s.updates_since(&bytes);
        let n = Notebook::create(3, "y").unwrap();
        let _ = n.import(&bytes);
        let _ = n.updates_since(&bytes);
    }
}

#[test]
fn saving_over_a_damaged_file_keeps_a_copy_of_it() {
    let t = TempLib::new("aside");
    let lib = t.open();
    let (id, sheets) = notebook_with_pages(&lib);
    let dir = t.path().join("notebooks").join(&id).join("sheets");
    let path = dir.join(format!("{}.loro", sheets[0]));
    let sheet = lib.load_sheet(&id, &sheets[0]).unwrap();
    std::fs::write(&path, b"\x00not loro at all").unwrap();
    sheet.add_stroke(Brush::default(), &line(300.0, 10)).unwrap();
    lib.save_sheet(&id, &sheets[0], &sheet).unwrap();
    assert_eq!(lib.load_sheet(&id, &sheets[0]).unwrap().strokes().len(), 2, "our complete copy replaced the damaged file");
    let kept: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).filter(|n| n.contains("damaged")).collect();
    assert_eq!(kept.len(), 1, "the damaged file is kept aside: {kept:?}");
    assert_eq!(std::fs::read(dir.join(&kept[0])).unwrap(), b"\x00not loro at all");
    assert_eq!(lib.sheet_ids_on_disk(&id).unwrap().len(), 2, "the copy is no page of its own");
}

#[test]
fn sync_repairs_a_damaged_document_instead_of_failing_forever() {
    let t = TempLib::new("repair");
    let lib = t.open();
    let (id, sheets) = notebook_with_pages(&lib);
    let peer = TempLib::new("repair-peer");
    let plib = peer.open();
    plib.import_bundle(&lib.export_bundle(&id).unwrap()).unwrap();
    // Damage both files here; the peer sends everything (it heard we have nothing).
    let nbdir = t.path().join("notebooks").join(&id);
    std::fs::write(nbdir.join("index.loro"), b"garbage").unwrap();
    std::fs::write(nbdir.join("sheets").join(format!("{}.loro", sheets[0])), b"garbage").unwrap();
    let nothing = Sheet::create(9).unwrap().version();
    let all_index = plib.load_notebook(&id).unwrap().updates_since(&nothing).unwrap();
    let all_sheet = plib.load_sheet(&id, &sheets[0]).unwrap().updates_since(&nothing).unwrap();
    assert!(lib.merge_notebook(&id, &all_index).unwrap());
    assert!(lib.merge_sheet(&id, &sheets[0], &all_sheet).unwrap());
    assert_eq!(lib.load_notebook(&id).unwrap().sheet_count(), 2);
    assert_eq!(lib.load_sheet(&id, &sheets[0]).unwrap().strokes().len(), 1);
}

#[test]
fn hostile_bundles_never_panic_and_never_import_half() {
    let t = TempLib::new("bundle-src");
    let lib = t.open();
    let (id, _) = notebook_with_pages(&lib);
    let blob = lib.put_blob(b"%PDF-1.4 fake").unwrap();
    let good = lib.export_bundle(&id).unwrap();
    let mut r = Rng::new(13);
    for _ in 0..200 {
        let dst = TempLib::new("bundle-dst");
        let dlib = dst.open();
        let bad = damage(&mut r, &good);
        match dlib.import_bundle(&bad) {
            Ok(got) => {
                // Whatever came in is complete enough to open.
                let nb = dlib.load_notebook(&got).unwrap();
                let _ = nb.outline(true);
            }
            Err(_) => assert!(dlib.notebook_ids().unwrap().is_empty(), "a failed import leaves nothing behind"),
        }
        let _ = dlib.import_bundle_as_copy(&bad, "Kopie");
    }
    // A bundle whose blob doesn't match its name, or with a bad sheet, fails as a whole.
    let mut forged = good.clone();
    let name = format!("blobs/{blob}");
    let entry = |out: &mut Vec<u8>, n: &str, d: &[u8]| {
        out.extend_from_slice(&(n.len() as u16).to_le_bytes());
        out.extend_from_slice(n.as_bytes());
        out.extend_from_slice(&(d.len() as u64).to_le_bytes());
        out.extend_from_slice(d);
    };
    entry(&mut forged, &name, b"not what the hash says");
    let dst = TempLib::new("bundle-forged");
    assert!(dst.open().import_bundle(&forged).is_err());
    assert!(dst.open().notebook_ids().unwrap().is_empty(), "nothing of a forged bundle stays");
    let mut bad_sheet = good.clone();
    entry(&mut bad_sheet, "sheets/abc.loro", b"garbage");
    let dst = TempLib::new("bundle-badsheet");
    assert!(dst.open().import_bundle(&bad_sheet).is_err());
    assert!(dst.open().notebook_ids().unwrap().is_empty());
    let mut traversal = good.clone();
    entry(&mut traversal, "sheets/../../evil.loro", b"x");
    let dst = TempLib::new("bundle-traversal");
    assert!(dst.open().import_bundle(&traversal).is_err());
    assert!(!dst.path().join("evil.loro").exists());
}

#[test]
fn concurrent_writers_lose_nothing() {
    // The app, the sync engine and a second app window write the same page at once.
    let t = TempLib::new("concurrent");
    let lib = t.open();
    let (id, sheets) = notebook_with_pages(&lib);
    let root = t.path().to_path_buf();
    let writers = 4;
    let per = 6;
    let handles: Vec<_> = (0..writers)
        .map(|w| {
            let (root, id, sheet) = (root.clone(), id.clone(), sheets[0].clone());
            std::thread::spawn(move || {
                let lib = Library::open_as(&root, &format!("w{w}")).unwrap();
                let s = lib.load_sheet(&id, &sheet).unwrap();
                for k in 0..per {
                    s.add_stroke(Brush::default(), &line((w * 100 + k * 10) as f32, 5)).unwrap();
                    lib.save_sheet(&id, &sheet, &s).unwrap();
                    let nb = lib.load_notebook(&id).unwrap();
                    nb.add_sheet(None, 99, &SheetSpec::a4(Background::Blank)).unwrap();
                    lib.save_notebook(&nb).unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(lib.load_sheet(&id, &sheets[0]).unwrap().strokes().len(), 1 + writers * per);
    assert_eq!(lib.load_notebook(&id).unwrap().sheet_count(), 2 + writers * per);
}

#[test]
fn leftovers_of_a_crash_mid_save_are_ignored() {
    let t = TempLib::new("crash");
    let lib = t.open();
    let (id, sheets) = notebook_with_pages(&lib);
    let dir = t.path().join("notebooks").join(&id).join("sheets");
    // A temp file the crash left behind, half written.
    std::fs::write(dir.join(format!(".{}.loro.tmp-deadbeef", sheets[0])), b"half").unwrap();
    std::fs::write(t.path().join("notebooks").join(&id).join(".index.loro.tmp-x"), b"half").unwrap();
    assert_eq!(lib.sheet_ids_on_disk(&id).unwrap().len(), 2);
    assert_eq!(lib.recent_sheets(10).unwrap().len(), 2);
    assert_eq!(lib.load_notebook(&id).unwrap().sheet_count(), 2);
    let bundle = lib.export_bundle(&id).unwrap();
    let dst = TempLib::new("crash-dst");
    let got = dst.open().import_bundle(&bundle).unwrap();
    assert_eq!(dst.open().sheet_ids_on_disk(&got).unwrap().len(), 2, "no temp file travels in a bundle");
}

#[test]
fn a_damaged_device_id_file_does_not_lock_the_library() {
    // An empty or garbled `device` file (a broken copy of the library) must not keep the app
    // from opening the library: a fresh id is as good as the old one.
    let t = TempLib::new("device");
    let first = t.open().peer();
    for junk in [&b""[..], b"not a number", b"\xff\xfe", b"-5"] {
        std::fs::write(t.path().join("device"), junk).unwrap();
        let lib = Library::open(t.path()).expect("opens anyway");
        assert_ne!(lib.peer(), 0);
        assert_eq!(Library::open(t.path()).unwrap().peer(), lib.peer(), "the new id is kept");
    }
    std::fs::write(t.path().join("device.sync"), b"x").unwrap();
    assert!(Library::open_as(t.path(), "sync").is_ok());
    let _ = first;
}

#[test]
fn a_truncated_blob_is_repaired_when_it_comes_again() {
    let t = TempLib::new("blob");
    let lib = t.open();
    let data = vec![7u8; 5000];
    let id = lib.put_blob(&data).unwrap();
    std::fs::write(lib.blob_path(&id).unwrap(), &data[..100]).unwrap();
    assert!(lib.blob(&id).is_err(), "damaged");
    lib.put_blob(&data).unwrap();
    assert_eq!(lib.blob(&id).unwrap(), data, "stored again in full");
}

#[test]
fn two_open_copies_of_one_document_in_one_app_keep_both_edits() {
    // The library screen pins a notebook while the editor has it open; the editor and a dialog
    // both hold a page. Each copy edits and saves: nothing may get lost, and another device that
    // got the first save must still end up with the same document.
    let t = TempLib::new("twice");
    let lib = t.open();
    let (id, sheets) = notebook_with_pages(&lib);
    let a = lib.load_notebook(&id).unwrap();
    let b = lib.load_notebook(&id).unwrap();
    a.set_pinned(true).unwrap();
    lib.save_notebook(&a).unwrap();
    let other = TempLib::new("twice-other");
    let olib = other.open();
    olib.import_bundle(&lib.export_bundle(&id).unwrap()).unwrap();
    b.add_sheet(None, 9, &SheetSpec::a4(Background::Dots)).unwrap();
    lib.save_notebook(&b).unwrap();
    let n = lib.load_notebook(&id).unwrap();
    assert!(n.pinned(), "the first copy's edit survived the second copy's save");
    assert_eq!(n.sheet_count(), 3);
    olib.merge_notebook(&id, &n.updates_since(&olib.load_notebook(&id).unwrap().version()).unwrap()).unwrap();
    let o = olib.load_notebook(&id).unwrap();
    assert_eq!((o.pinned(), o.sheet_count()), (true, 3), "the other device converges");

    let s1 = lib.load_sheet(&id, &sheets[0]).unwrap();
    let s2 = lib.load_sheet(&id, &sheets[0]).unwrap();
    s1.add_stroke(Brush::default(), &line(200.0, 5)).unwrap();
    lib.save_sheet(&id, &sheets[0], &s1).unwrap();
    s2.add_stroke(Brush::default(), &line(300.0, 5)).unwrap();
    lib.save_sheet(&id, &sheets[0], &s2).unwrap();
    assert_eq!(lib.load_sheet(&id, &sheets[0]).unwrap().strokes().len(), 3);
    // Copies opened after the others are gone may use the device's own id again.
    drop((a, b, s1, s2));
    let again = lib.load_sheet(&id, &sheets[0]).unwrap();
    again.add_stroke(Brush::default(), &line(400.0, 5)).unwrap();
    lib.save_sheet(&id, &sheets[0], &again).unwrap();
    assert_eq!(lib.load_sheet(&id, &sheets[0]).unwrap().strokes().len(), 4);
}

#[test]
fn many_handles_share_a_library_from_threads() {
    // NoteLibrary on the tablet is shared by the UI and background threads.
    let t = TempLib::new("shared");
    let lib = Arc::new(t.open());
    let (id, sheets) = notebook_with_pages(&lib);
    let handles: Vec<_> = (0..4)
        .map(|k| {
            let (lib, id, sheet) = (lib.clone(), id.clone(), sheets[k % 2].clone());
            std::thread::spawn(move || {
                for _ in 0..5 {
                    let s = lib.load_sheet(&id, &sheet).unwrap();
                    s.add_stroke(Brush::default(), &line(k as f32, 3)).unwrap();
                    lib.save_sheet(&id, &sheet, &s).unwrap();
                    let _ = lib.refresh_sheet(&id, &sheet, &s).unwrap();
                    let _ = lib.change_stamp(&id);
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(lib.load_sheet(&id, &sheets[0]).unwrap().strokes().len(), 1 + 10);
    assert_eq!(lib.load_sheet(&id, &sheets[1]).unwrap().strokes().len(), 1 + 10);
}
