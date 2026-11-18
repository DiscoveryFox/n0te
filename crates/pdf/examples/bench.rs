//! Timing of the operations n0te needs, on real PDFs.
//!
//!   cargo run --release -p n0te-pdf --example bench -- <libpdfium.so> <export-source.pdf> [more.pdf ...]
//!
//! Set N0TE_BENCH_OUT=<dir> to keep the inked export for inspection.
//!
//! "cold" = fresh document, the page is loaded and parsed as part of the call; "warm" = the page
//! is already in the document's page cache. Tablet numbers: a page 1600 px wide, tiles of
//! 512 × 512 px at 4× that zoom.

use std::time::{Duration, Instant};

use n0te_core::ink::{Brush, InkPoint};
use n0te_pdf::{ExportPage, PageSize, PageSource, PdfEngine, Stroke, Tile, page_pixels};

const PAGE_WIDTH_PX: u32 = 1600;
const TILE: u32 = 512;
const ZOOM: f32 = 4.0;
const SAMPLE_PAGES: u32 = 10;

fn main() {
    let mut args = std::env::args().skip(1);
    let lib = args.next().expect("usage: bench <libpdfium.so> <export-source.pdf> [more.pdf ...]");
    let files: Vec<String> = args.collect();
    assert!(!files.is_empty(), "give at least one PDF");

    let t = Instant::now();
    let e = PdfEngine::load(&lib).expect("load pdfium");
    println!("load libpdfium + init: {:.1} ms\n", ms(t.elapsed()));

    for f in &files {
        bench_file(&e, f);
    }
    bench_export(&e, &files[0]);
    // The longest document: export all of it, with ink on every 10th page.
    let longest = files.iter().max_by_key(|f| e.open(std::fs::read(f).unwrap()).unwrap().page_count()).unwrap();
    bench_export_long(&e, longest);
}

fn bench_file(e: &PdfEngine, path: &str) {
    let bytes = std::fs::read(path).expect("read pdf");
    let name = path.rsplit('/').next().unwrap_or(path);

    let mut opens = Vec::new();
    for _ in 0..7 {
        let b = bytes.clone();
        let t = Instant::now();
        let d = e.open(b).expect("open");
        opens.push(t.elapsed());
        drop(d);
    }
    let doc = e.open(bytes.clone()).unwrap();
    let n = doc.page_count();
    let t = Instant::now();
    let sizes = doc.page_sizes().unwrap();
    let t_sizes = t.elapsed();
    println!(
        "== {name}: {n} pages, {:.0}x{:.0} pt, {:.1} MB",
        sizes[0].width,
        sizes[0].height,
        bytes.len() as f64 / 1e6
    );
    println!("  open                      {}   (first {:.2} ms)", stats(&opens), ms(opens[0]));
    println!("  page sizes, all pages     {:.2} ms", ms(t_sizes));

    let pages: Vec<u32> = (0..SAMPLE_PAGES.min(n)).map(|k| k * n / SAMPLE_PAGES.min(n)).collect();
    let (mut full_cold, mut full_warm, mut tile_cold, mut tile_warm, mut text, mut chars) = (vec![], vec![], vec![], vec![], vec![], 0);
    for &p in &pages {
        let size = sizes[p as usize];
        // Whole page for the tablet, cold then warm.
        let d = e.open(bytes.clone()).unwrap();
        full_cold.push(time(|| d.render_page(p, PAGE_WIDTH_PX).unwrap()));
        full_warm.push(time(|| d.render_page(p, PAGE_WIDTH_PX).unwrap()));
        // A 512 px tile from the middle of the page at 4x zoom, cold then warm.
        let scale = ZOOM * PAGE_WIDTH_PX as f32 / size.width;
        let tile = middle_tile(size, scale);
        let d = e.open(bytes.clone()).unwrap();
        tile_cold.push(time(|| d.render_tile(p, scale, tile).unwrap()));
        tile_warm.push(time(|| d.render_tile(p, scale, tile).unwrap()));
        // Text with positions, cold.
        let d = e.open(bytes.clone()).unwrap();
        let t = Instant::now();
        chars += d.page_text(p).unwrap().chars.len();
        text.push(t.elapsed());
    }
    println!("  page {PAGE_WIDTH_PX} px wide, cold     {}", stats(&full_cold));
    println!("  page {PAGE_WIDTH_PX} px wide, warm     {}", stats(&full_warm));
    println!("  tile {TILE}² at {ZOOM}x, cold      {}", stats(&tile_cold));
    println!("  tile {TILE}² at {ZOOM}x, warm      {}", stats(&tile_warm));
    println!("  text + boxes per page     {}   ({} chars/page)", stats(&text), chars / pages.len());

    // Cost of the 64 px overscan: the same corner region with and without the extra pixels.
    let size = sizes[pages[pages.len() / 2] as usize];
    let p = pages[pages.len() / 2];
    let scale = ZOOM * PAGE_WIDTH_PX as f32 / size.width;
    let (w, h) = page_pixels(size, scale);
    if w >= TILE + 64 && h >= TILE + 64 {
        let small: Vec<_> = (0..5).map(|_| time(|| doc.render_tile(p, scale, Tile { x: 0, y: 0, width: TILE, height: TILE }).unwrap())).collect();
        let big: Vec<_> = (0..5).map(|_| time(|| doc.render_tile(p, scale, Tile { x: 0, y: 0, width: TILE + 64, height: TILE + 64 }).unwrap())).collect();
        println!("  overscan cost: 512² {:.2} ms vs 576² {:.2} ms (warm, page corner)", ms(median(&small)), ms(median(&big)));
    }
    println!();
}

fn middle_tile(size: PageSize, scale: f32) -> Tile {
    let (w, h) = page_pixels(size, scale);
    let (tw, th) = (TILE.min(w), TILE.min(h));
    Tile { x: (w - tw) / 2, y: (h - th) / 2, width: tw, height: th }
}

/// Export: 50 pages of the source with 100 strokes each (and variants), plus file sizes.
fn bench_export(e: &PdfEngine, path: &str) {
    let src = e.open(std::fs::read(path).unwrap()).unwrap();
    let n = src.page_count();
    let pages = 50.min(n);
    let sizes = src.page_sizes().unwrap();
    let strokes: Vec<Vec<(Vec<InkPoint>, Brush)>> = (0..pages).map(|p| handwriting(sizes[p as usize], p as u64, 100, 80)).collect();
    let refs: Vec<Vec<Stroke>> = strokes.iter().map(|s| s.iter().map(|(pts, b)| Stroke { points: pts, brush: *b }).collect()).collect();
    let light: Vec<&[Stroke]> = refs.iter().map(|r| &r[..20]).collect();
    let vertices: usize = strokes.iter().flatten().map(|(pts, b)| n0te_core::ink::outline(pts, b).len() / 2).sum();
    let name = path.rsplit('/').next().unwrap_or(path);
    println!("== export from {name}: {pages} pages x 100 strokes x 80 samples ({vertices} outline vertices)");

    let plain: Vec<ExportPage> = (0..pages).map(|p| ExportPage { source: PageSource::Pdf { doc: &src, page: p }, strokes: &[] }).collect();
    let inked: Vec<ExportPage> = (0..pages).map(|p| ExportPage { source: PageSource::Pdf { doc: &src, page: p }, strokes: &refs[p as usize] }).collect();
    let lightly: Vec<ExportPage> = (0..pages).map(|p| ExportPage { source: PageSource::Pdf { doc: &src, page: p }, strokes: light[p as usize] }).collect();
    // Reversed order with a blank page after every slide: exercises the reorder pass.
    let mixed: Vec<ExportPage> = (0..pages)
        .rev()
        .flat_map(|p| {
            [
                ExportPage { source: PageSource::Pdf { doc: &src, page: p }, strokes: &refs[p as usize] },
                ExportPage { source: PageSource::Blank { width: 595.0, height: 842.0 }, strokes: &refs[p as usize] },
            ]
        })
        .collect();
    for (label, list) in [("pages only", &plain), ("pages + 20 strokes each", &lightly), ("pages + ink", &inked), ("reversed + blank pages, all inked", &mixed)] {
        let mut times = Vec::new();
        let mut out = Vec::new();
        for _ in 0..3 {
            let t = Instant::now();
            out = e.export(list).unwrap();
            times.push(t.elapsed());
        }
        println!("  {label:<36} {}   -> {:.2} MB ({} pages)", stats(&times), out.len() as f64 / 1e6, list.len());
        if let (Some(dir), "pages + ink") = (std::env::var_os("N0TE_BENCH_OUT"), label) {
            let file = std::path::Path::new(&dir).join("n0te-bench-export.pdf");
            std::fs::write(&file, &out).unwrap();
            println!("    written to {}", file.display());
        }
    }

    // What one import call per page would cost in size (resources copied once per page).
    let naive = e.with_raw(|pdfium| {
        let source = pdfium.load_pdf_from_byte_vec(std::fs::read(path).unwrap(), None).unwrap();
        let mut doc = pdfium.create_new_pdf().unwrap();
        for p in 0..pages as i32 {
            let at = doc.pages().len();
            doc.pages_mut().copy_page_from_document(&source, p, at).unwrap();
        }
        doc.save_to_bytes().unwrap()
    });
    println!("  one import per page instead (size only)  -> {:.2} MB", naive.len() as f64 / 1e6);
}

/// Whole document exported, 20 strokes on every 10th page (shows the O(N x document) cost).
fn bench_export_long(e: &PdfEngine, path: &str) {
    let src = e.open(std::fs::read(path).unwrap()).unwrap();
    let n = src.page_count();
    let sizes = src.page_sizes().unwrap();
    let strokes: Vec<Vec<(Vec<InkPoint>, Brush)>> = (0..n).map(|p| if p % 10 == 0 { handwriting(sizes[p as usize], p as u64, 20, 80) } else { Vec::new() }).collect();
    let refs: Vec<Vec<Stroke>> = strokes.iter().map(|s| s.iter().map(|(pts, b)| Stroke { points: pts, brush: *b }).collect()).collect();
    let list: Vec<ExportPage> = (0..n).map(|p| ExportPage { source: PageSource::Pdf { doc: &src, page: p }, strokes: &refs[p as usize] }).collect();
    let name = path.rsplit('/').next().unwrap_or(path);
    let t = Instant::now();
    let plain: Vec<ExportPage> = list.iter().map(|p| ExportPage { strokes: &[], ..*p }).collect();
    let out = e.export(&plain).unwrap();
    println!("== export all {n} pages of {name}: no ink {:.0} ms -> {:.2} MB", ms(t.elapsed()), out.len() as f64 / 1e6);
    let t = Instant::now();
    let out = e.export(&list).unwrap();
    println!("   20 strokes on every 10th page ({} inked): {:.0} ms -> {:.2} MB", n.div_ceil(10), ms(t.elapsed()), out.len() as f64 / 1e6);
}

/// Deterministic handwriting-like strokes: wandering curves of `samples` points, ~0.8 pt apart.
fn handwriting(size: PageSize, seed: u64, count: usize, samples: usize) -> Vec<(Vec<InkPoint>, Brush)> {
    let mut rng = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    let mut next = move || {
        rng = rng.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((rng >> 33) as f32) / (1u64 << 31) as f32
    };
    let colors = [0xFF11_1111, 0xFF1E_5AC8, 0xFFD0_2020, 0x80FF_EB3B];
    (0..count)
        .map(|i| {
            let (mut x, mut y) = (20.0 + next() * (size.width - 40.0), 20.0 + next() * (size.height - 40.0));
            let mut dir = next() * std::f32::consts::TAU;
            let pts = (0..samples)
                .map(|k| {
                    dir += (next() - 0.5) * 0.6;
                    x = (x + dir.cos() * 0.8).clamp(0.0, size.width);
                    y = (y + dir.sin() * 0.8).clamp(0.0, size.height);
                    InkPoint { x, y, pressure: 0.35 + 0.3 * next(), t_ms: k as u32 * 8 }
                })
                .collect();
            let color = colors[i % colors.len()];
            let width = if color >> 24 == 0x80 { 12.0 } else { 1.8 };
            (pts, Brush { width, thinning: 0.6, color })
        })
        .collect()
}

fn time<T>(f: impl FnOnce() -> T) -> Duration {
    let t = Instant::now();
    std::hint::black_box(f());
    t.elapsed()
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1e3
}

fn median(v: &[Duration]) -> Duration {
    let mut s = v.to_vec();
    s.sort();
    s[s.len() / 2]
}

fn stats(v: &[Duration]) -> String {
    let max = v.iter().max().copied().unwrap_or_default();
    format!("median {:7.2} ms, max {:7.2} ms", ms(median(v)), ms(max))
}
