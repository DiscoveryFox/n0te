//! A notebook of very large pages for measuring the Android app (not used by tests):
//! 2000 × 2000 mm pages with dense handwriting on squared paper, pencil, an empty lined page, a
//! dotted page with a graph, a PDF page of that size with ink, a board with ink spread far and
//! wide, and an A4 page to compare.
//!
//! `cargo run --release -p n0te-ffi --example big_pages -- <library dir> [strokes on page 1]`
//!
//! The library dir is a throwaway one (never the real library): push it to the emulator's
//! `files/library` (see n0te/scripts/emu.sh).

use std::sync::Arc;

use n0te_ffi::pdf::{FfiTile, PdfService, TileSink};
use n0te_ffi::{FfiBackground, FfiBrush, FfiSheetSpec, NoteLibrary};
use n0te_pdf::PdfEngine;
use n0te_pdf::pdfium_render::prelude::*;

/// 2000 mm in points.
const BIG: f32 = 2000.0 / 25.4 * 72.0;

struct NoTiles;
impl TileSink for NoTiles {
    fn on_tile(&self, _tile: FfiTile) {}
}

fn pack(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}

/// A small deterministic random source.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 40) as f32) / (1u64 << 24) as f32
    }
}

/// One handwritten "letter" at (x, y) (its top-left), `w` × `h` points: a wobbly loop or hook.
fn letter(rng: &mut Rng, x: f32, y: f32, w: f32, h: f32) -> Vec<f32> {
    let n = 24 + (rng.next() * 30.0) as usize;
    let (a, b, ph) = (1.0 + rng.next() * 2.0, 0.5 + rng.next() * 1.5, rng.next() * 6.28);
    let mut out = Vec::with_capacity(n * 4);
    for i in 0..n {
        let t = i as f32 / (n - 1) as f32;
        let px = x + w * (t + 0.25 * (a * 6.28 * t + ph).sin());
        let py = y + h * (0.5 + 0.45 * (b * 6.28 * t + ph).cos());
        out.extend_from_slice(&[px, py, 0.35 + 0.4 * (3.14 * t).sin(), (i * 8) as f32]);
    }
    out
}

/// Fills `area` (x0, y0, x1, y1) with lines of letters until `count` strokes; returns how many.
fn write(sheet: &n0te_ffi::SheetHandle, rng: &mut Rng, area: (f32, f32, f32, f32), size: f32, count: usize, brushes: &[FfiBrush]) -> usize {
    let (x0, y0, x1, y1) = area;
    let (lw, lh, gap) = (size * 0.7, size, size * 1.6);
    let mut done = 0;
    let mut y = y0;
    'lines: while y + lh < y1 {
        let mut x = x0;
        while x + lw < x1 {
            if done >= count {
                break 'lines;
            }
            let b = brushes[done % brushes.len()];
            sheet.add_stroke(b, pack(&letter(rng, x, y, lw, lh))).unwrap();
            done += 1;
            x += lw * (1.0 + rng.next() * 0.3);
            // A space between words.
            if rng.next() < 0.18 {
                x += lw;
            }
        }
        y += gap;
    }
    done
}

fn spec(w: f32, h: f32, bg: FfiBackground, spacing: f32) -> FfiSheetSpec {
    FfiSheetSpec { width: w, height: h, background: bg, spacing, pdf_blob: None, pdf_page: 0, gaps: vec![] }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args.next().expect("usage: big_pages <library dir> [strokes on page 1]");
    let dense: usize = args.next().map(|s| s.parse().unwrap()).unwrap_or(40_000);
    let pdfium = std::env::var("N0TE_PDFIUM_LIB").unwrap_or_else(|_| format!("{}/../../third_party/pdfium/linux-x64/lib/libpdfium.so", env!("CARGO_MANIFEST_DIR")));
    let lib = NoteLibrary::open(root.clone()).unwrap();
    let pdf = PdfService::start(lib.clone(), pdfium.clone(), vec![], Arc::new(NoTiles)).unwrap();
    let nb = lib.clone().create_notebook("Riesenseiten".into()).unwrap();
    let mut rng = Rng(42);
    let ink = |c: u32| FfiBrush { width: 1.6, thinning: 0.6, color: c, kind: 0 };
    let pencil = FfiBrush { width: 1.8, thinning: 0.2, color: 0xF0333333, kind: 1 };
    let marker = FfiBrush { width: 10.0, thinning: 0.0, color: 0x66FFD600, kind: 2 };
    let t = std::time::Instant::now();

    // 1: squared paper, dense handwriting over the whole page (a few pencil and marker strokes).
    let ids = nb.add_sheet(None, u32::MAX, spec(BIG, BIG, FfiBackground::Grid, 14.17)).unwrap();
    let s1 = nb.clone().open_sheet(ids[1].clone()).unwrap();
    let mut brushes = vec![ink(0xFF111111); 18];
    brushes.push(ink(0xFF1E5BD8));
    brushes.push(pencil);
    let n = write(&s1, &mut rng, (40.0, 40.0, BIG - 40.0, BIG - 40.0), 16.0, dense, &brushes);
    for i in 0..200 {
        let y = 60.0 + i as f32 * 27.0;
        let line: Vec<f32> = (0..40).flat_map(|k| [200.0 + k as f32 * 8.0, y, 0.5, (k * 8) as f32]).collect();
        s1.add_stroke(marker, pack(&line)).unwrap();
    }
    s1.save().unwrap();
    println!("page 1: {n} strokes + 200 marker ({:?})", t.elapsed());

    // 2: lined paper, empty.
    nb.add_sheet(None, u32::MAX, spec(BIG, BIG, FfiBackground::Lines, 22.0)).unwrap();

    // 3: dotted paper, a graph and some writing.
    let ids = nb.add_sheet(None, u32::MAX, spec(BIG, BIG, FfiBackground::Dots, 14.17)).unwrap();
    let s3 = nb.clone().open_sheet(ids[1].clone()).unwrap();
    let mut plot = n0te_core::plot::Plot::centered(300.0, 300.0, 600.0, 400.0, 28.0, 0xFF555555, 1.2);
    for f in ["f(x) = x^2/4 - 2", "g(x) = sin(x)", "A = (1, 2)", "a = 2"] {
        plot = plot.add(f).unwrap();
    }
    plot.interactive = true;
    s3.add_plot(plot.into()).unwrap();
    let n3 = write(&s3, &mut rng, (1000.0, 300.0, 3000.0, 2000.0), 16.0, 2000, &[ink(0xFF111111)]);
    s3.save().unwrap();
    println!("page 3: graph + {n3} strokes");

    // 4: a PDF page of 2000 × 2000 mm (vector lines, text) with ink.
    let bytes = PdfEngine::load(&pdfium).unwrap().with_raw(|p| {
        let mut doc = p.create_new_pdf().unwrap();
        let font = doc.fonts_mut().helvetica();
        let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::Custom(PdfPoints::new(BIG), PdfPoints::new(BIG))).unwrap();
        for i in 0..60 {
            let y = 80.0 + i as f32 * 90.0;
            page.objects_mut().create_text_object(PdfPoints::new(80.0), PdfPoints::new(BIG - y), &format!("Plakat Zeile {i}: Ein langer Satz auf einer sehr großen Seite"), font, PdfPoints::new(40.0)).unwrap();
            page.objects_mut()
                .create_path_object_line(PdfPoints::new(60.0), PdfPoints::new(BIG - y - 20.0), PdfPoints::new(BIG - 60.0), PdfPoints::new(BIG - y - 20.0), PdfColor::new(40, 90, 200, 255), PdfPoints::new(2.0))
                .unwrap();
        }
        doc.save_to_bytes().unwrap()
    });
    let imported = nb.import_pdf(pdf.clone(), bytes, None, u32::MAX).unwrap();
    let s4 = nb.clone().open_sheet(imported[0].sheet_id.clone()).unwrap();
    let n4 = write(&s4, &mut rng, (100.0, 100.0, 2500.0, 1500.0), 18.0, 3000, &[ink(0xFFD71921)]);
    s4.save().unwrap();
    println!("page 4: PDF {}x{} + {n4} strokes", imported[0].width, imported[0].height);

    // 5: a board with clusters of writing far apart.
    let ids = nb.add_sheet(None, u32::MAX, spec(0.0, 0.0, FfiBackground::Dots, 24.0)).unwrap();
    let s5 = nb.clone().open_sheet(ids[1].clone()).unwrap();
    let mut n5 = 0;
    for cy in 0..5 {
        for cx in 0..5 {
            let (x, y) = (cx as f32 * 4000.0, cy as f32 * 4000.0);
            n5 += write(&s5, &mut rng, (x, y, x + 1400.0, y + 1000.0), 16.0, 1200, &brushes);
        }
    }
    s5.save().unwrap();
    println!("board: {n5} strokes");

    // 6: pencil only, 2000 × 2000 mm blank paper.
    let ids = nb.add_sheet(None, u32::MAX, spec(BIG, BIG, FfiBackground::Blank, 20.0)).unwrap();
    let s6 = nb.clone().open_sheet(ids[1].clone()).unwrap();
    let n6 = write(&s6, &mut rng, (40.0, 40.0, BIG - 40.0, 2400.0), 18.0, 6000, &[pencil]);
    s6.save().unwrap();
    println!("page 6: {n6} pencil strokes");

    // 7: an A4 page to compare.
    let ids = nb.add_sheet(None, u32::MAX, spec(595.0, 842.0, FfiBackground::Lines, 22.0)).unwrap();
    let s7 = nb.clone().open_sheet(ids[1].clone()).unwrap();
    let n7 = write(&s7, &mut rng, (40.0, 60.0, 555.0, 800.0), 14.0, 1500, &[ink(0xFF111111)]);
    s7.save().unwrap();
    println!("page 7: A4 + {n7} strokes");

    nb.save().unwrap();
    for row in nb.outline(true) {
        if let n0te_ffi::FfiOutlineRow::Sheet { sheet_id, spec, .. } = row {
            println!("sheet {sheet_id}: {} x {} {:?}{}", spec.width, spec.height, spec.background, if spec.pdf_blob.is_some() { " PDF" } else { "" });
        }
    }
    drop(pdf);
    println!("notebook {} in {root} ({:?})", nb.id(), t.elapsed());
}
