//! Speed budget check for the M0 spike: a semester notebook (500 sheets) and a dense sheet.
//! `cargo run --release -p n0te-core --example bench`

use std::time::Instant;

use n0te_core::ink::{self, Brush, InkPoint};
use n0te_core::notebook::{Background, Notebook, SheetSpec, Side};
use n0te_core::sheet::Sheet;

fn main() {
    let t = Instant::now();
    let nb = Notebook::create(1, "Mathe WS26").unwrap();
    let mut group = None;
    for i in 0..500 {
        if i % 25 == 0 {
            group = Some(nb.add_group(None, usize::MAX, &format!("Woche {:02}", i / 25 + 1)).unwrap());
        }
        let (node, _) = nb.add_sheet(group.as_deref(), usize::MAX, &SheetSpec::a4(Background::Grid)).unwrap();
        if i % 10 == 0 {
            nb.add_side_sheet(&node, Side::Right, 0, &SheetSpec::a4(Background::Blank)).unwrap();
        }
    }
    let build = t.elapsed();
    let snap = nb.snapshot().unwrap();
    let t = Instant::now();
    let nb2 = Notebook::load(2, &snap).unwrap();
    let load = t.elapsed();
    let t = Instant::now();
    let rows = nb2.outline(true);
    let outline = t.elapsed();
    println!("notebook: 500 sheets + 50 side sheets, build {:?}, snapshot {} KB, load {:?}, outline {:?} ({} rows)", build, snap.len() / 1024, load, outline, rows.len());

    let sheet = Sheet::create(1).unwrap();
    let mut seed = 1u64;
    for s in 0..400 {
        let pts: Vec<InkPoint> = (0..120)
            .map(|i| {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                InkPoint { x: 40.0 + (s % 20) as f32 * 25.0 + i as f32 * 0.4, y: 40.0 + (s / 20) as f32 * 38.0 + ((seed >> 40) % 10) as f32 * 0.3, pressure: 0.3 + ((seed >> 33) % 60) as f32 / 100.0, t_ms: i * 4 }
            })
            .collect();
        sheet.add_stroke(Brush::default(), &pts).unwrap();
    }
    let ssnap = sheet.snapshot().unwrap();
    let t = Instant::now();
    let s2 = Sheet::load(2, &ssnap).unwrap();
    let sload = t.elapsed();
    let t = Instant::now();
    let strokes = s2.strokes();
    let decode = t.elapsed();
    let t = Instant::now();
    let verts: usize = strokes.iter().map(|s| ink::outline(&s.points, &s.brush).len() / 2).sum();
    let outl = t.elapsed();
    let t = Instant::now();
    let part = s2.strokes_in(Some(ink::Rect::new(0.0, 0.0, 300.0, 200.0)));
    let cull = t.elapsed();
    println!(
        "sheet: 400 strokes x 120 samples, snapshot {} KB, load {:?}, decode {:?}, outlines {:?} ({} vertices), cull {:?} ({} visible)",
        ssnap.len() / 1024, sload, decode, outl, verts, cull, part.len()
    );
}
