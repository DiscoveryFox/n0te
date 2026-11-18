//! Tests against the real pdfium library (run `scripts/fetch-pdfium.sh` first, or point
//! `N0TE_PDFIUM_LIB` at a libpdfium.so). Tests run on parallel threads and share one engine,
//! which also exercises the locking.

use std::path::PathBuf;
use std::sync::Arc;

use n0te_core::ink::{Brush, InkPoint};
use n0te_pdf::pdfium_render::prelude::*;
use n0te_pdf::{Error, ExportPage, PageSource, PdfDoc, PdfEngine, Rect, Rgba, Stroke, Tile, hide_ink, page_pixels, tiles};

fn library() -> PathBuf {
    if let Some(p) = std::env::var_os("N0TE_PDFIUM_LIB") {
        return p.into();
    }
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../third_party/pdfium/linux-x64/lib/libpdfium.so");
    assert!(p.exists(), "{} missing: run n0te/scripts/fetch-pdfium.sh or set N0TE_PDFIUM_LIB", p.display());
    p
}

fn engine() -> PdfEngine {
    PdfEngine::load(library()).expect("load pdfium")
}

/// A PDF built with pdfium itself: one page per (width, height, rotation, lines of text).
/// Text baselines start 100 pt below the top, 30 pt apart, at x = 72 (in unrotated page space).
fn make_pdf(e: &PdfEngine, pages: &[(f32, f32, PdfPageRenderRotation, &[&str])]) -> Vec<u8> {
    e.with_raw(|pdfium| {
        let mut doc = pdfium.create_new_pdf().unwrap();
        let font = doc.fonts_mut().helvetica();
        for &(w, h, rot, lines) in pages {
            let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::from_points(PdfPoints::new(w), PdfPoints::new(h))).unwrap();
            for (i, line) in lines.iter().enumerate() {
                let y = h - 100.0 - 30.0 * i as f32;
                page.objects_mut().create_text_object(PdfPoints::new(72.0), PdfPoints::new(y), *line, font, PdfPoints::new(24.0)).unwrap();
            }
            // A filled shape so renders have something besides text.
            page.objects_mut()
                .create_path_object_rect(
                    PdfRect::new_from_values(40.0, 40.0, 120.0, 200.0),
                    None,
                    None,
                    Some(PdfColor::new(30, 90, 200, 255)),
                )
                .unwrap();
            page.set_rotation(rot);
        }
        doc.save_to_bytes().unwrap()
    })
}

const A4: (f32, f32) = (595.0, 842.0);
const NONE: PdfPageRenderRotation = PdfPageRenderRotation::None;

fn line(x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<InkPoint> {
    (0..=40)
        .map(|i| {
            let t = i as f32 / 40.0;
            InkPoint { x: x0 + (x1 - x0) * t, y: y0 + (y1 - y0) * t, pressure: 0.5, t_ms: i * 8 }
        })
        .collect()
}

fn brush(color: u32) -> Brush {
    Brush { width: 6.0, thinning: 0.0, color, kind: 0 }
}

/// Number of path objects on each page.
fn path_counts(e: &PdfEngine, bytes: &[u8]) -> Vec<usize> {
    e.with_raw(|pdfium| {
        let doc = pdfium.load_pdf_from_byte_slice(bytes, None).unwrap();
        doc.pages().iter().map(|p| p.objects().iter().filter(|o| o.object_type() == PdfPageObjectType::Path).count()).collect()
    })
}

fn pixel(img: &Rgba, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * img.width + x) * 4) as usize;
    img.data[i..i + 4].try_into().unwrap()
}

#[test]
fn opens_and_reports_page_sizes() {
    let e = engine();
    let bytes = make_pdf(&e, &[(A4.0, A4.1, NONE, &[]), (792.0, 612.0, NONE, &[]), (400.0, 300.0, PdfPageRenderRotation::Degrees90, &[])]);
    let doc = e.open(bytes).unwrap();
    assert_eq!(doc.page_count(), 3);
    let sizes = doc.page_sizes().unwrap();
    let want = [(595.0, 842.0), (792.0, 612.0), (300.0, 400.0)]; // rotated page is displayed upright
    for (s, w) in sizes.iter().zip(want) {
        assert!((s.width - w.0).abs() < 0.01 && (s.height - w.1).abs() < 0.01, "{s:?} != {w:?}");
    }
    assert!(matches!(doc.page_size(3), Err(Error::PageOutOfRange { page: 3, count: 3 })));
}

#[test]
fn rejects_garbage_and_second_library() {
    let e = engine();
    assert!(e.open(b"%PDF-1.7 definitely not a pdf".to_vec()).is_err());
    assert!(e.open(Vec::new()).is_err());
    assert!(matches!(PdfEngine::load("/nonexistent/libpdfium.so"), Err(Error::AlreadyLoaded(_))));
}

#[test]
fn extracts_text_with_positions() {
    let e = engine();
    let doc = e.open(make_pdf(&e, &[(A4.0, A4.1, NONE, &["Hello n0te", "Second line"])])).unwrap();
    let text = doc.page_text(0).unwrap();
    let s = text.text();
    assert!(s.contains("Hello n0te") && s.contains("Second line"), "{s:?}");

    let hits = text.find("N0TE");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].len(), 1, "one line");
    let r = hits[0][0];
    // Baseline 100 pt below the top, 24 pt Helvetica: glyphs sit between ~80 and ~105 pt.
    assert!(r.x0 > 72.0 + 40.0 && r.x1 < 72.0 + 200.0, "{r:?}");
    assert!(r.y0 > 70.0 && r.y0 < 100.0 && r.y1 > 95.0 && r.y1 < 110.0, "{r:?}");

    let second = text.find("second")[0][0];
    assert!(second.y0 > r.y1 - 2.0, "second line is below the first: {second:?} vs {r:?}");
    assert!(text.find("missing").is_empty());
    // Every drawn character has a box, generated ones don't.
    assert!(text.chars.iter().all(|c| c.generated == c.bbox.is_empty()));
}

#[test]
fn text_positions_follow_page_rotation() {
    let e = engine();
    // Rotated 90° clockwise: unrotated top-left text ends up at the top-right when displayed.
    let doc = e.open(make_pdf(&e, &[(A4.0, A4.1, PdfPageRenderRotation::Degrees90, &["Rotated"])])).unwrap();
    let size = doc.page_size(0).unwrap();
    // Vertical text gives one box per character; look at their union.
    let hit = &doc.page_text(0).unwrap().find("rotated")[0];
    let r = hit.iter().fold(Rect::EMPTY, |u, b| u.include(b.x0, b.y0).include(b.x1, b.y1));
    assert!(r.x0 > size.width / 2.0, "text should be on the right half: {r:?} in {size:?}");
    assert!(r.y1 - r.y0 > 2.0 * (r.x1 - r.x0), "rotated text runs downwards: {r:?}");
    assert!((r.y0 - 72.0).abs() < 2.0, "text starts 72 pt from the top: {r:?}");
}

#[test]
fn tiles_stitch_to_the_full_page() {
    let e = engine();
    // Many lines of 24 pt text so glyphs cross tile seams at every scale.
    let lines: Vec<String> = (0..24).map(|i| format!("{i:02} The quick brown fox jumps over the lazy dog")).collect();
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let doc = e.open(make_pdf(&e, &[(A4.0, A4.1, NONE, &lines)])).unwrap();
    let size = doc.page_size(0).unwrap();
    for (width, tile) in [(700u32, 128u32), (595, 100), (1600, 512)] {
        let full = doc.render_page(0, width).unwrap();
        let scale = width as f32 / size.width;
        assert_eq!((full.width, full.height), page_pixels(size, scale));

        let mut stitched = vec![0u8; full.data.len()];
        for t in tiles((full.width, full.height), tile) {
            let img = doc.render_tile(0, scale, t).unwrap();
            let row = (t.width * 4) as usize;
            for y in 0..t.height {
                let (src, dst) = ((y * t.width * 4) as usize, (((t.y + y) * full.width + t.x) * 4) as usize);
                stitched[dst..dst + row].copy_from_slice(&img.data[src..src + row]);
            }
        }
        // Exact while glyphs come from pdfium's glyph cache (text below ~50 px). Bigger text is
        // filled as paths, and anti-aliasing at a clip edge may then differ by a level or two
        // (the seam bug fixed by TILE_OVERSCAN shows up as differences of ~135).
        let max_diff = stitched.iter().zip(&full.data).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
        let allowed = if 24.0 * scale < 50.0 { 0 } else { 2 };
        assert!(max_diff <= allowed, "{width} px / {tile} px tiles differ from the full render by up to {max_diff}");

        // Opaque white paper, blue rectangle in RGBA order.
        assert_eq!(pixel(&full, 2, 2), [255, 255, 255, 255]);
        let (bx, by) = ((120.0 * scale) as u32, ((842.0 - 80.0) * scale) as u32); // rect centre
        assert_eq!(pixel(&full, bx, by), [30, 90, 200, 255]);
    }
}

#[test]
fn rejects_bad_tiles() {
    let e = engine();
    let doc = e.open(make_pdf(&e, &[(100.0, 100.0, NONE, &[])])).unwrap();
    assert!(doc.render_tile(0, 2.0, Tile { x: 150, y: 0, width: 100, height: 10 }).is_err());
    assert!(doc.render_tile(0, 2.0, Tile { x: 0, y: 0, width: 0, height: 10 }).is_err());
    assert!(doc.render_tile(0, f32::NAN, Tile { x: 0, y: 0, width: 1, height: 1 }).is_err());
    assert!(doc.render_tile_into(0, 2.0, Tile { x: 0, y: 0, width: 10, height: 10 }, &mut [0u8; 12]).is_err());
    assert!(doc.render_tile(1, 2.0, Tile { x: 0, y: 0, width: 1, height: 1 }).is_err());
}

#[test]
fn export_round_trip() {
    let e = engine();
    let a = e.open(make_pdf(&e, &[(A4.0, A4.1, NONE, &["A page one"]), (A4.0, A4.1, NONE, &["A page two"])])).unwrap();
    let b = e.open(make_pdf(&e, &[(612.0, 792.0, NONE, &["B page one"])])).unwrap();

    let (l1, l2, l3) = (line(100.0, 300.0, 400.0, 320.0), line(50.0, 50.0, 300.0, 250.0), line(200.0, 600.0, 220.0, 700.0));
    let red = brush(0xFFE0_2020);
    let s3 = [Stroke { points: &l1, brush: red }, Stroke { points: &l2, brush: brush(0x8000_00FF) }, Stroke { points: &l3, brush: red }];
    let s2 = [Stroke { points: &l2, brush: red }, Stroke { points: &l3, brush: red }];
    let s1 = [Stroke { points: &l1, brush: brush(0xFF00_A000) }];
    let pages = [
        ExportPage { source: PageSource::Pdf { doc: &a, page: 1 }, strokes: &s3 },
        ExportPage { source: PageSource::Blank { width: 400.0, height: 300.0 }, strokes: &s2 },
        ExportPage { source: PageSource::Pdf { doc: &b, page: 0 }, strokes: &[] },
        ExportPage { source: PageSource::Pdf { doc: &a, page: 0 }, strokes: &[] },
        ExportPage { source: PageSource::Pdf { doc: &a, page: 1 }, strokes: &s1 }, // same page again
    ];
    let bytes = e.export(&pages).unwrap();

    let out = e.open(bytes.clone()).unwrap();
    assert_eq!(out.page_count(), 5);
    let want = [(595.0, 842.0), (400.0, 300.0), (612.0, 792.0), (595.0, 842.0), (595.0, 842.0)];
    for (s, w) in out.page_sizes().unwrap().iter().zip(want) {
        assert!((s.width - w.0).abs() < 0.01 && (s.height - w.1).abs() < 0.01, "{s:?} != {w:?}");
    }
    // Each source page brings its blue rectangle; blank pages bring nothing. The strokes of the
    // two copies of A/1 stay apart.
    assert_eq!(path_counts(&e, &bytes), vec![1 + 3, 2, 1, 1, 1 + 1]);
    let texts: Vec<String> = (0..5).map(|i| out.page_text(i).unwrap().text()).collect();
    assert_eq!(texts[0], "A page two");
    assert_eq!(texts[1], "");
    assert_eq!(texts[2], "B page one");
    assert_eq!(texts[3], "A page one");
    assert_eq!(texts[4], "A page two");

    // Colour and alpha survive.
    let fills: Vec<(u8, u8, u8, u8)> = e.with_raw(|pdfium| {
        let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
        let page = doc.pages().get(0).unwrap();
        page.objects()
            .iter()
            .filter(|o| o.object_type() == PdfPageObjectType::Path)
            .map(|o| {
                let c = o.fill_color().unwrap();
                (c.red(), c.green(), c.blue(), c.alpha())
            })
            .collect()
    });
    assert!(fills.contains(&(0xE0, 0x20, 0x20, 0xFF)), "{fills:?}");
    assert!(fills.contains(&(0, 0, 0xFF, 0x80)), "{fills:?}");

    // The ink lands where it was drawn: middle of l1 on page 0 is red.
    let img = out.render_page(0, 595).unwrap();
    assert_eq!(pixel(&img, 250, 310), [0xE0, 0x20, 0x20, 255]);
    assert_eq!(pixel(&img, 250, 280), [255, 255, 255, 255]);
}

#[test]
fn ink_on_a_rotated_page_lands_where_drawn() {
    let e = engine();
    let src = e.open(make_pdf(&e, &[(A4.0, A4.1, PdfPageRenderRotation::Degrees90, &[])])).unwrap();
    let size = src.page_size(0).unwrap();
    assert!(size.width > size.height, "rotated A4 is displayed landscape");
    let l = line(100.0, 60.0, 500.0, 60.0); // horizontal on screen, near the top
    let strokes = [Stroke { points: &l, brush: brush(0xFF00_0000) }];
    let bytes = e.export(&[ExportPage { source: PageSource::Pdf { doc: &src, page: 0 }, strokes: &strokes }]).unwrap();
    let out = e.open(bytes).unwrap();
    let img = out.render_page(0, size.width as u32).unwrap();
    for x in [200, 300, 480] { // the blue rect sits at x 40..120 here
        assert_eq!(pixel(&img, x, 60), [0, 0, 0, 255], "stroke at ({x}, 60)");
        assert_eq!(pixel(&img, x, 75), [255, 255, 255, 255], "paper below the stroke at ({x}, 75)");
    }
}

#[test]
fn export_of_one_source_in_order_needs_no_reordering() {
    let e = engine();
    let a = e.open(make_pdf(&e, &[(A4.0, A4.1, NONE, &["one"]), (A4.0, A4.1, NONE, &["two"])])).unwrap();
    let pages: Vec<ExportPage> = (0..2).map(|i| ExportPage { source: PageSource::Pdf { doc: &a, page: i }, strokes: &[] }).collect();
    let out = e.open(e.export(&pages).unwrap()).unwrap();
    assert_eq!(out.page_text(1).unwrap().text(), "two");
    assert!(e.export(&[ExportPage { source: PageSource::Pdf { doc: &a, page: 2 }, strokes: &[] }]).is_err());
    assert!(e.export(&[ExportPage { source: PageSource::Blank { width: 0.0, height: 10.0 }, strokes: &[] }]).is_err());
}

#[test]
fn renders_from_many_threads() {
    let e = engine();
    let doc: Arc<PdfDoc> = Arc::new(e.open(make_pdf(&e, &[(A4.0, A4.1, NONE, &["threads"]), (A4.0, A4.1, NONE, &["more"])])).unwrap());
    let scale = 2.0;
    let grid = tiles(page_pixels(doc.page_size(0).unwrap(), scale), 256);
    let reference: Vec<Rgba> = grid.iter().map(|t| doc.render_tile(0, scale, *t).unwrap()).collect();
    let workers: Vec<_> = (0..6)
        .map(|k| {
            let (doc, grid) = (doc.clone(), grid.clone());
            std::thread::spawn(move || {
                let mut out = Vec::new();
                for (i, t) in grid.iter().enumerate() {
                    // Mix pages and text extraction so the page cache churns.
                    out.push(doc.render_tile(0, scale, *t).unwrap());
                    if (i + k) % 3 == 0 {
                        doc.page_text(1).unwrap();
                        doc.render_page(1, 64).unwrap();
                    }
                }
                out
            })
        })
        .collect();
    for w in workers {
        assert_eq!(w.join().unwrap(), reference);
    }
}

/// One page of `media` points with a crop box (left, bottom, right, top) and a rotation, and a
/// blue square near the middle of the crop box (so the page has content to wrap).
fn make_cropped(e: &PdfEngine, media: (f32, f32), crop: (f32, f32, f32, f32), rot: PdfPageRenderRotation) -> Vec<u8> {
    e.with_raw(|pdfium| {
        let mut doc = pdfium.create_new_pdf().unwrap();
        let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::from_points(PdfPoints::new(media.0), PdfPoints::new(media.1))).unwrap();
        page.objects_mut()
            .create_path_object_rect(PdfRect::new_from_values(350.0, 250.0, 400.0, 300.0), None, None, Some(PdfColor::new(30, 90, 200, 255)))
            .unwrap();
        page.boundaries_mut().set_crop(PdfRect::new_from_values(crop.1, crop.0, crop.3, crop.2)).unwrap();
        page.set_rotation(rot);
        doc.save_to_bytes().unwrap()
    })
}

const ROTATIONS: [PdfPageRenderRotation; 4] =
    [PdfPageRenderRotation::None, PdfPageRenderRotation::Degrees90, PdfPageRenderRotation::Degrees180, PdfPageRenderRotation::Degrees270];

#[test]
fn ink_lands_where_drawn_for_every_rotation_and_crop_box() {
    let e = engine();
    for rot in ROTATIONS {
        // Crop box 450 x 600 pt, offset into a 600 x 800 media box.
        let src = e.open(make_cropped(&e, (600.0, 800.0), (50.0, 100.0, 500.0, 700.0), rot)).unwrap();
        let size = src.page_size(0).unwrap();
        let turned = matches!(rot, PdfPageRenderRotation::Degrees90 | PdfPageRenderRotation::Degrees270);
        assert_eq!((size.width, size.height), if turned { (600.0, 450.0) } else { (450.0, 600.0) }, "{rot:?}");
        let (w, h) = (size.width, size.height);
        let top = line(40.0, 30.0, 200.0, 30.0); // horizontal, near the top-left corner
        let corner = line(w - 30.0, h - 90.0, w - 30.0, h - 30.0); // vertical, near the bottom-right
        let strokes = [Stroke { points: &top, brush: brush(0xFF00_0000) }, Stroke { points: &corner, brush: brush(0xFFD7_1921) }];
        let bytes = e.export(&[ExportPage { source: PageSource::Pdf { doc: &src, page: 0 }, strokes: &strokes }]).unwrap();
        let out = e.open(bytes).unwrap();
        let img = out.render_page(0, w as u32).unwrap();
        for x in [60, 120, 180] {
            assert_eq!(pixel(&img, x, 30), [0, 0, 0, 255], "{rot:?}: stroke at ({x}, 30)");
            assert_eq!(pixel(&img, x, 45), [255, 255, 255, 255], "{rot:?}: paper at ({x}, 45)");
        }
        let (cx, cy) = (w as u32 - 30, h as u32 - 60);
        assert_eq!(pixel(&img, cx, cy), [0xD7, 0x19, 0x21, 255], "{rot:?}: corner stroke");
        assert_eq!(pixel(&img, cx - 15, cy), [255, 255, 255, 255], "{rot:?}: paper beside the corner stroke");
        // The original content is still there (the square sits near the middle of the page).
        let blue = (0..img.height).flat_map(|y| (0..img.width).map(move |x| (x, y))).filter(|&(x, y)| pixel(&img, x, y) == [30, 90, 200, 255]).count();
        assert!((2000..=2600).contains(&blue), "{rot:?}: {blue} px of the 50x50 pt square");
    }
}

/// Mean and worst per-channel difference of two renders, and the share of pixels off by > 32.
fn diff(a: &Rgba, b: &Rgba) -> (f64, u8, f64) {
    assert_eq!((a.width, a.height), (b.width, b.height));
    let (mut sum, mut max, mut off) = (0u64, 0u8, 0usize);
    for (pa, pb) in a.data.chunks_exact(4).zip(b.data.chunks_exact(4)) {
        let d = pa.iter().zip(pb).map(|(x, y)| x.abs_diff(*y)).max().unwrap();
        sum += d as u64;
        max = max.max(d);
        off += (d > 32) as usize;
    }
    let n = (a.data.len() / 4) as f64;
    (sum as f64 / n, max, off as f64 / n)
}

#[test]
fn own_writer_renders_like_the_pdfium_objects_path() {
    let e = engine();
    let text = e.open(make_pdf(&e, &[(A4.0, A4.1, NONE, &["Compare", "the two writers"])])).unwrap();
    let docs: Vec<PdfDoc> = ROTATIONS.iter().map(|&r| e.open(make_cropped(&e, (600.0, 800.0), (50.0, 100.0, 500.0, 700.0), r)).unwrap()).collect();
    let lines: Vec<Vec<InkPoint>> = (0..12).map(|i| line(30.0 + i as f32 * 20.0, 40.0, 60.0 + i as f32 * 25.0, 400.0 - i as f32 * 7.0)).collect();
    let colors = [0xFF11_1111, 0xFF1E_5AC8, 0x80FF_EB3B, 0x40D0_2020];
    let strokes: Vec<Stroke> =
        lines.iter().enumerate().map(|(i, l)| Stroke { points: l, brush: Brush { width: 3.0 + i as f32, thinning: 0.6, color: colors[i % 4], kind: 0 } }).collect();
    let mut pages = vec![ExportPage { source: PageSource::Pdf { doc: &text, page: 0 }, strokes: &strokes }];
    pages.extend(docs.iter().map(|d| ExportPage { source: PageSource::Pdf { doc: d, page: 0 }, strokes: &strokes }));
    pages.push(ExportPage { source: PageSource::Blank { width: 400.0, height: 500.0 }, strokes: &strokes });

    let ours = e.open(e.export(&pages).unwrap()).unwrap();
    let old = e.open(e.export_via_page_objects(&pages).unwrap()).unwrap();
    for i in 0..pages.len() as u32 {
        let width = (ours.page_size(i).unwrap().width * 2.0) as u32;
        let (mean, max, off) = diff(&ours.render_page(i, width).unwrap(), &old.render_page(i, width).unwrap());
        // Same outlines, same 1/100 pt rounding: only edge anti-aliasing differs (max ~35/255).
        assert!(mean < 0.1 && max <= 64 && off < 1e-3, "page {i}: mean diff {mean:.4}, max {max}, {:.4} % of pixels off by > 32", off * 100.0);
    }
}

#[test]
fn exported_file_passes_qpdf_check() {
    let e = engine();
    let a = e.open(make_pdf(&e, &[(A4.0, A4.1, NONE, &["qpdf"]), (A4.0, A4.1, PdfPageRenderRotation::Degrees90, &["two"])])).unwrap();
    let c = e.open(make_cropped(&e, (600.0, 800.0), (50.0, 100.0, 500.0, 700.0), PdfPageRenderRotation::Degrees270)).unwrap();
    let l = line(50.0, 50.0, 300.0, 200.0);
    let s = [Stroke { points: &l, brush: brush(0xFFE0_2020) }, Stroke { points: &l, brush: brush(0x8000_00FF) }];
    let pages = [
        ExportPage { source: PageSource::Pdf { doc: &a, page: 1 }, strokes: &s },
        ExportPage { source: PageSource::Blank { width: 300.0, height: 300.0 }, strokes: &s },
        ExportPage { source: PageSource::Pdf { doc: &c, page: 0 }, strokes: &s },
        ExportPage { source: PageSource::Pdf { doc: &a, page: 0 }, strokes: &[] },
    ];
    let bytes = e.export(&pages).unwrap();
    let hidden = hide_ink(bytes.clone()).unwrap();
    let dir = std::env::temp_dir().join(format!("n0te-qpdf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for (name, data) in [("export.pdf", &bytes), ("hidden.pdf", &hidden)] {
        let path = dir.join(name);
        std::fs::write(&path, data).unwrap();
        match std::process::Command::new("qpdf").arg("--check").arg(&path).output() {
            Ok(o) => assert!(o.status.success(), "qpdf --check {name}:\n{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)),
            Err(_) => eprintln!("qpdf not installed, skipping the qpdf check"),
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn ink_layer_can_be_hidden_even_after_a_second_export() {
    let e = engine();
    let src = e.open(make_pdf(&e, &[(A4.0, A4.1, NONE, &["Layer"])])).unwrap();
    let first = line(100.0, 300.0, 400.0, 300.0);
    let s1 = [Stroke { points: &first, brush: brush(0xFF00_0000) }];
    let once = e.export(&[ExportPage { source: PageSource::Pdf { doc: &src, page: 0 }, strokes: &s1 }]).unwrap();

    // Export the exported page again with more ink: its old ink layer is now only referenced
    // from the page, the new one is listed in the catalog.
    let again_src = e.open(once).unwrap();
    let second = line(100.0, 500.0, 400.0, 500.0);
    let s2 = [Stroke { points: &second, brush: brush(0xFFD7_1921) }];
    let twice = e.export(&[ExportPage { source: PageSource::Pdf { doc: &again_src, page: 0 }, strokes: &s2 }]).unwrap();

    let render = |b: &[u8]| e.open(b.to_vec()).unwrap().render_page(0, 595).unwrap();
    let shown = render(&twice);
    assert_eq!(pixel(&shown, 250, 300), [0, 0, 0, 255]);
    assert_eq!(pixel(&shown, 250, 500), [0xD7, 0x19, 0x21, 255]);
    let hidden_bytes = hide_ink(twice).unwrap();
    let hidden = render(&hidden_bytes);
    assert_eq!(pixel(&hidden, 250, 300), [255, 255, 255, 255], "first ink hidden");
    assert_eq!(pixel(&hidden, 250, 500), [255, 255, 255, 255], "second ink hidden");
    assert_eq!(pixel(&hidden, 120, 842 - 80), [30, 90, 200, 255], "the page's own rectangle stays");
    assert_eq!(e.open(hidden_bytes).unwrap().page_text(0).unwrap().text(), "Layer");
    // A PDF without n0te ink comes back unchanged.
    let plain = make_pdf(&e, &[(A4.0, A4.1, NONE, &[])]);
    assert_eq!(hide_ink(plain.clone()).unwrap(), plain);
}
