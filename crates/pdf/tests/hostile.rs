//! PDFs and requests that are wrong: tiles past the page or past u32, absurd scales, damaged
//! and truncated files, every kind of page content with broken numbers in an export. Errors,
//! never a panic, never an allocation of gigabytes (an abort on the tablet).

use std::path::PathBuf;
use std::time::{Duration, Instant};

use n0te_core::ink::{Brush, InkPoint};
use n0te_core::notebook::Background;
use n0te_pdf::pdfium_render::prelude::*;
use n0te_pdf::{Paper, PdfEngine, PdfImage, PdfText, Stroke, Tile, add_ink_paper_text_images_gaps, hide_ink};

fn engine() -> PdfEngine {
    let lib = std::env::var_os("N0TE_PDFIUM_LIB")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../third_party/pdfium/linux-x64/lib/libpdfium.so"));
    PdfEngine::load(lib).expect("load pdfium (run n0te/scripts/fetch-pdfium.sh)")
}

/// Ends the run with the case printed when one takes longer than `limit` (a hang would else
/// just keep the test running).
struct Watchdog(std::sync::Arc<std::sync::Mutex<(Instant, String, bool)>>);
impl Watchdog {
    fn new(limit: Duration) -> Self {
        let state = std::sync::Arc::new(std::sync::Mutex::new((Instant::now(), String::new(), false)));
        let s = state.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_millis(100));
            let g = s.lock().unwrap_or_else(|p| p.into_inner());
            if g.2 {
                return;
            }
            if g.0.elapsed() > limit {
                use std::io::Write;
                let _ = writeln!(std::io::stderr(), "stuck for more than {limit:?} on: {}", g.1);
                std::process::exit(101);
            }
        });
        Watchdog(state)
    }
    fn case(&self, what: String) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = (Instant::now(), what, false);
    }
}
impl Drop for Watchdog {
    fn drop(&mut self) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).2 = true;
    }
}

struct Rng(u64);
impl Rng {
    fn u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.u64() % n as u64) as usize
    }
    fn f(&mut self) -> f32 {
        match self.below(10) {
            0 => f32::NAN,
            1 => f32::INFINITY,
            2 => f32::NEG_INFINITY,
            3 => 1e30,
            4 => 0.0,
            _ => (self.u64() % 1200) as f32 - 100.0,
        }
    }
}

fn pages(e: &PdfEngine, n: usize) -> Vec<u8> {
    e.with_raw(|pdfium| {
        let mut doc = pdfium.create_new_pdf().unwrap();
        let font = doc.fonts_mut().helvetica();
        for i in 0..n {
            let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::a4()).unwrap();
            page.objects_mut().create_text_object(PdfPoints::new(72.0), PdfPoints::new(742.0), &format!("Seite {i}"), font, PdfPoints::new(24.0)).unwrap();
        }
        doc.save_to_bytes().unwrap()
    })
}

#[test]
fn tiles_past_the_page_or_past_u32_are_errors_not_panics() {
    let e = engine();
    let doc = e.open(pages(&e, 1)).unwrap();
    let bad = [
        (2.0, Tile { x: u32::MAX - 10, y: 0, width: 100, height: 10 }),
        (2.0, Tile { x: 0, y: u32::MAX, width: 10, height: 10 }),
        (2.0, Tile { x: 0, y: 0, width: u32::MAX, height: u32::MAX }),
        // Inside a page rendered huge, but a tile of 40 GB.
        (500.0, Tile { x: 0, y: 0, width: 100_000, height: 100_000 }),
        (1e30, Tile { x: 0, y: 0, width: 256, height: 256 }),
        (f32::INFINITY, Tile { x: 0, y: 0, width: 256, height: 256 }),
        (-1.0, Tile { x: 0, y: 0, width: 256, height: 256 }),
        (0.0, Tile { x: 0, y: 0, width: 256, height: 256 }),
    ];
    for (scale, tile) in bad {
        let t = Instant::now();
        assert!(doc.render_tile(0, scale, tile).is_err(), "{scale} {tile:?}");
        assert!(t.elapsed() < Duration::from_secs(1));
    }
    assert!(doc.render_page(0, u32::MAX).is_err(), "a page 4 billion pixels wide");
    assert!(doc.render_page(0, 0).is_ok() || doc.render_page(0, 0).is_err());
    // Still renders afterwards.
    assert_eq!(doc.render_tile(0, 1.0, Tile { x: 0, y: 0, width: 64, height: 64 }).unwrap().data.len(), 64 * 64 * 4);
}

#[test]
fn damaged_pdfs_are_errors() {
    let e = engine();
    let good = pages(&e, 3);
    let mut r = Rng(7);
    for _ in 0..30 {
        let mut b = good.clone();
        match r.below(3) {
            0 => b.truncate(r.below(b.len())),
            1 => {
                for _ in 0..20 {
                    let i = r.below(b.len());
                    b[i] = r.u64() as u8;
                }
            }
            _ => b = (0..r.below(500)).map(|_| r.u64() as u8).collect(),
        }
        if let Ok(doc) = e.open(b.clone()) {
            let n = doc.page_count();
            for p in 0..n.min(3) {
                let _ = doc.page_size(p);
                let _ = doc.render_tile(p, 0.5, Tile { x: 0, y: 0, width: 32, height: 32 });
                let _ = doc.page_text(p);
            }
        }
        let _ = hide_ink(b.clone());
        let none: Vec<&[Stroke]> = vec![&[]];
        let _ = add_ink_paper_text_images_gaps(b, &none, &[None], &[&[]], &[&[]], &[&[]]);
    }
}

#[test]
fn export_of_every_kind_of_content_with_broken_numbers_still_opens() {
    let e = engine();
    let base = pages(&e, 2);
    let mut r = Rng(8);
    let not_jpeg = b"PNG? no, just bytes".to_vec(); // must be left out, not break the file
    let w = Watchdog::new(Duration::from_secs(20));
    for _ in 0..20 {
        let strokes: Vec<(Vec<InkPoint>, Brush)> = (0..1 + r.below(6))
            .map(|_| {
                let n = r.below(30);
                let pts = (0..n).map(|i| InkPoint { x: if r.below(8) == 0 { r.f() } else { 50.0 + i as f32 * 5.0 }, y: if r.below(8) == 0 { r.f() } else { 300.0 }, pressure: r.f(), t_ms: i as u32 }).collect();
                (pts, Brush { width: r.f(), thinning: r.f(), color: r.u64() as u32, kind: (r.u64() % 4) as u8 })
            })
            .collect();
        let s: Vec<Stroke> = strokes.iter().map(|(p, b)| Stroke { points: p, brush: *b }).collect();
        let texts = vec![PdfText { x: r.f(), y: r.f(), w: r.f(), size: r.f(), text: ["# Überschrift\n- Punkt **fett** `code`", "😀 ä ö ü ß €", "", "\u{1f}\u{0}\t", &"lang ".repeat(400)][r.below(5)].to_string() }];
        let images = vec![PdfImage { jpeg: if r.below(2) == 0 { not_jpeg.clone() } else { vec![0xFF, 0xD8, 0xFF] }, x: r.f(), y: r.f(), w: r.f(), h: r.f() }];
        let paper = Some(Paper { background: [Background::Lines, Background::Grid, Background::Dots, Background::Cornell, Background::Blank][r.below(5)], spacing: r.f() });
        let gaps: Vec<(f32, f32)> = (0..r.below(3)).map(|_| (r.f(), r.f())).collect();
        let what = format!("strokes {strokes:?} texts {texts:?} images {:?} paper {paper:?} gaps {gaps:?}", images.iter().map(|i| (i.x, i.y, i.w, i.h)).collect::<Vec<_>>());
        w.case(format!("export: {what}"));
        let t = Instant::now();
        let out = add_ink_paper_text_images_gaps(base.clone(), &[&s, &[]], &[paper, None], &[&texts, &[]], &[&images, &[]], &[&gaps, &[]]);
        assert!(t.elapsed() < Duration::from_secs(3), "slow export");
        if let Ok(bytes) = out {
            w.case(format!("open: {what}"));
            let doc = e.open(bytes).expect("what we export opens");
            assert_eq!(doc.page_count(), 2);
            w.case(format!("render: {what}"));
            let _ = doc.render_tile(0, 0.3, Tile { x: 0, y: 0, width: 64, height: 64 });
        }
    }
}
