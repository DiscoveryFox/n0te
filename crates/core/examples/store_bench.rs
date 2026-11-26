//! What saving, loading and syncing cost on disk: a dense sheet saved again and again (the
//! app saves after strokes), loaded, its version read (what the sync engine announces).
//! Each figure is the best of several runs (the least disturbed by whatever else runs).
//! `cargo run --profile fast -p n0te-core --example store_bench`

use std::time::{Duration, Instant};

use n0te_core::ink::{Brush, InkPoint};
use n0te_core::notebook::{Background, SheetSpec};
use n0te_core::store::Library;

/// The fastest of `n` runs of `f`.
fn best(n: usize, mut f: impl FnMut()) -> Duration {
    (0..n)
        .map(|_| {
            let t = Instant::now();
            f();
            t.elapsed()
        })
        .min()
        .unwrap_or_default()
}

fn main() {
    let dir = std::env::temp_dir().join(format!("n0te-store-bench-{}", std::process::id()));
    let lib = Library::open(&dir).unwrap();
    let nb = lib.create_notebook("Bench").unwrap();
    let (_, sid) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Grid)).unwrap();
    lib.save_notebook(&nb).unwrap();
    let id = nb.id();
    let sheet = lib.load_sheet(&id, &sid).unwrap();
    let mut seed = 1u64;
    for s in 0..1500u32 {
        let pts: Vec<InkPoint> = (0..120)
            .map(|i| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                InkPoint { x: 40.0 + (s % 20) as f32 * 25.0 + i as f32 * 0.4, y: 40.0 + (s / 20) as f32 * 10.0 + ((seed >> 40) % 10) as f32 * 0.3, pressure: 0.3 + ((seed >> 33) % 60) as f32 / 100.0, t_ms: i * 4 }
            })
            .collect();
        sheet.add_stroke(Brush::default(), &pts).unwrap();
    }
    lib.save_sheet(&id, &sid, &sheet).unwrap();
    let path = dir.join("notebooks").join(&id).join("sheets").join(format!("{sid}.loro"));
    let size = std::fs::metadata(&path).unwrap().len();
    let n = 15;

    let mut k = 0.0;
    let save = best(n, || {
        k += 1.0;
        sheet.add_stroke(Brush::default(), &[InkPoint { x: k, y: 1.0, pressure: 0.5, t_ms: 0 }, InkPoint { x: k + 5.0, y: 2.0, pressure: 0.5, t_ms: 8 }]).unwrap();
        lib.save_sheet(&id, &sid, &sheet).unwrap();
    });
    let refresh = best(n, || assert!(!lib.refresh_sheet(&id, &sid, &sheet).unwrap()));
    let load = best(n, || drop(std::hint::black_box(lib.load_sheet(&id, &sid).unwrap())));
    let version = best(n, || drop(std::hint::black_box(lib.load_sheet(&id, &sid).unwrap().version())));
    let stored = best(n, || drop(std::hint::black_box(lib.sheet_version(&id, &sid).unwrap())));
    assert_eq!(
        loro::VersionVector::decode(&lib.sheet_version(&id, &sid).unwrap()).unwrap(),
        loro::VersionVector::decode(&lib.load_sheet(&id, &sid).unwrap().version()).unwrap(),
        "the file's version is the document's"
    );
    let strokes = best(n, || drop(std::hint::black_box(lib.load_sheet(&id, &sid).unwrap().strokes())));
    // What a save is made of: merging the file (nothing new in it) and writing the snapshot.
    let file = std::fs::read(&path).unwrap();
    let merge = best(n, || sheet.import(&file).unwrap());
    let export = best(n, || drop(std::hint::black_box(sheet.snapshot().unwrap())));
    let write = best(n, || n0te_core::store::write_atomic(&dir.join("scratch.loro"), &file).unwrap());
    let changed = best(n, || {
        k += 1.0;
        sheet.add_stroke(Brush::default(), &[InkPoint { x: k, y: 3.0, pressure: 0.5, t_ms: 0 }, InkPoint { x: k + 5.0, y: 4.0, pressure: 0.5, t_ms: 8 }]).unwrap();
        drop(std::hint::black_box(sheet.snapshot().unwrap()));
    });
    println!("  a stroke and the snapshot after it {changed:?}");

    println!("sheet of 1500 strokes x 120 samples ({} KB), best of {n}:", size / 1024);
    println!("  save after a stroke {save:?} (merge the file {merge:?}, snapshot {export:?}, write and sync {write:?})");
    println!("  refresh, file unchanged {refresh:?}");
    println!("  load {load:?}, load + version {version:?}, version from the file {stored:?}, load + decode strokes {strokes:?}");
    let _ = std::fs::remove_dir_all(&dir);
}
