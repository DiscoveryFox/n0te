//! The PDF worker against the real pdfium library (see tests/pdf.rs for how it is found).

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use n0te_core::ink::{Brush, InkPoint};
use n0te_core::notebook::{Background, SheetSpec};
use n0te_core::store::Library;
use n0te_pdf::notebook::{export_sheets, import_pdf};
use n0te_pdf::pdfium_render::prelude::*;
use n0te_pdf::worker::priority;
use n0te_pdf::{Error, ExportSheet, PdfEngine, PdfWorker, Rgba, SheetSource, Tile, TileRequest, TileResult, WorkerOptions, page_pixels, tiles};

fn engine() -> PdfEngine {
    let lib = std::env::var_os("N0TE_PDFIUM_LIB")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../third_party/pdfium/linux-x64/lib/libpdfium.so"));
    PdfEngine::load(lib).expect("load pdfium (run n0te/scripts/fetch-pdfium.sh)")
}

/// `n` A4 pages, each with a line of text and a blue rectangle.
fn make_pdf(e: &PdfEngine, n: usize) -> Vec<u8> {
    e.with_raw(|pdfium| {
        let mut doc = pdfium.create_new_pdf().unwrap();
        let font = doc.fonts_mut().helvetica();
        for i in 0..n {
            let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::a4()).unwrap();
            page.objects_mut().create_text_object(PdfPoints::new(72.0), PdfPoints::new(742.0), &format!("Seite {i} Suchwort"), font, PdfPoints::new(24.0)).unwrap();
            page.objects_mut().create_path_object_rect(PdfRect::new_from_values(40.0, 40.0, 120.0, 200.0), None, None, Some(PdfColor::new(30, 90, 200, 255))).unwrap();
        }
        doc.save_to_bytes().unwrap()
    })
}

/// A worker whose documents are the given byte vectors (key = index), counting loads.
fn worker(e: PdfEngine, docs: Vec<Vec<u8>>, open_docs: usize) -> (PdfWorker, Receiver<TileResult>, Arc<AtomicUsize>) {
    let (tx, rx) = channel();
    let tx = Mutex::new(tx);
    let loads = Arc::new(AtomicUsize::new(0));
    let l = loads.clone();
    let w = PdfWorker::start(
        e,
        move |key| {
            l.fetch_add(1, Ordering::SeqCst);
            key.parse::<usize>().ok().and_then(|i| docs.get(i).cloned()).ok_or_else(|| Error::Invalid(format!("no document {key}")))
        },
        move |r| {
            let _ = tx.lock().unwrap().send(r);
        },
        WorkerOptions { open_docs },
    );
    (w, rx, loads)
}

fn req(doc: &str, page: u32, scale: f32, tile: Tile, priority: u32, view: u32, generation: u64) -> TileRequest {
    TileRequest { doc: doc.into(), page, scale, tile, priority, view, generation }
}

fn pixel(img: &Rgba, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * img.width + x) * 4) as usize;
    img.data[i..i + 4].try_into().unwrap()
}

#[test]
fn tiles_from_the_worker_stitch_to_the_page() {
    let e = engine();
    let bytes = make_pdf(&e, 2);
    let reference = e.open(bytes.clone()).unwrap();
    let (w, rx, _) = worker(e, vec![bytes], 4);
    let scale = 1.5;
    let size = reference.page_size(1).unwrap();
    let full_px = page_pixels(size, scale);
    let grid = tiles(full_px, 256);
    w.request(grid.iter().map(|t| req("0", 1, scale, *t, priority::VISIBLE, 0, 1)));
    let full = reference.render_page(1, full_px.0).unwrap();
    for _ in 0..grid.len() {
        let r = rx.recv_timeout(Duration::from_secs(10)).expect("tile");
        let img = r.image.unwrap();
        let t = r.request.tile;
        for y in 0..t.height {
            let src = &img.data[(y * t.width * 4) as usize..((y + 1) * t.width * 4) as usize];
            let at = (((t.y + y) * full.width + t.x) * 4) as usize;
            assert_eq!(src, &full.data[at..at + src.len()], "tile {t:?} row {y}");
        }
    }
    assert!(rx.recv_timeout(Duration::from_millis(100)).is_err(), "one result per tile");
}

#[test]
fn visible_first_and_stale_generations_dropped() {
    let e = engine();
    let (w, rx, _) = worker(e, vec![make_pdf(&e, 1)], 4);
    // Hold the worker so everything below queues up.
    let (go_tx, go_rx) = channel::<()>();
    let (started_tx, started_rx) = channel::<()>();
    w.spawn(0, move |_| {
        let _ = started_tx.send(());
        let _ = go_rx.recv();
    });
    started_rx.recv().unwrap();
    let t = |x: u32| Tile { x, y: 0, width: 64, height: 64 };
    w.request([req("0", 0, 1.0, t(0), priority::PREFETCH, 0, 1), req("0", 0, 1.0, t(64), priority::PREFETCH, 0, 1)]);
    w.request([req("0", 0, 1.0, t(128), priority::PREFETCH, 1, 1)]); // thumbnails: another view
    w.request([req("0", 0, 1.0, t(192), priority::PREFETCH, 0, 2), req("0", 0, 1.0, t(256), priority::VISIBLE, 0, 2)]); // scrolled
    w.request([req("0", 0, 1.0, t(0), priority::VISIBLE, 0, 1)]); // late request of an old generation
    w.request([req("0", 0, 1.0, t(320), priority::BACKGROUND, 1, 1)]);
    w.cancel(1, 1); // does not drop generation 1 of view 1
    assert_eq!(w.queued(), 4);
    go_tx.send(()).unwrap();
    let got: Vec<(u32, u32)> = (0..4).map(|_| rx.recv_timeout(Duration::from_secs(10)).unwrap()).map(|r| (r.request.tile.x, r.request.view)).collect();
    assert_eq!(got, vec![(256, 0), (128, 1), (192, 0), (320, 1)]);
    assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
    w.cancel(1, 2);
    w.request([req("0", 0, 1.0, t(0), priority::VISIBLE, 1, 1)]);
    assert!(rx.recv_timeout(Duration::from_millis(200)).is_err(), "view 1 now ignores generation 1");
}

#[test]
fn errors_jobs_and_the_document_lru() {
    let e = engine();
    let docs: Vec<Vec<u8>> = (1..=3).map(|n| make_pdf(&e, n)).collect();
    let (w, rx, loads) = worker(e, docs, 2);
    // Unknown document and page out of range come back as errors.
    w.request([req("9", 0, 1.0, Tile { x: 0, y: 0, width: 8, height: 8 }, 0, 0, 0), req("0", 5, 1.0, Tile { x: 0, y: 0, width: 8, height: 8 }, 0, 0, 0)]);
    for _ in 0..2 {
        assert!(rx.recv_timeout(Duration::from_secs(10)).unwrap().image.is_err());
    }
    // "0" is open now (the out-of-range page loaded it).
    let loads_before = loads.load(Ordering::SeqCst);
    let count = |w: &PdfWorker, key: &'static str| w.call(priority::INTERACTIVE, move |d| d.get(key).map(|doc| doc.page_count())).unwrap().unwrap();
    assert_eq!(count(&w, "0"), 1);
    assert_eq!(count(&w, "1"), 2);
    assert_eq!(count(&w, "0"), 1); // still open
    assert_eq!(loads.load(Ordering::SeqCst), loads_before + 1);
    assert_eq!(count(&w, "2"), 3); // evicts "1" (least recently used)
    assert_eq!(w.call(0, |d| d.open_keys()).unwrap(), vec!["0", "2"]);
    assert_eq!(count(&w, "1"), 2);
    assert_eq!(loads.load(Ordering::SeqCst), loads_before + 3);
    // A panicking job does not kill the worker.
    assert!(w.call(0, |_| -> u32 { panic!("job panics") }).is_err());
    assert_eq!(count(&w, "1"), 2);
}

#[test]
fn call_from_the_sink_is_refused_not_deadlocked() {
    let e = engine();
    let bytes = make_pdf(&e, 1);
    let slot: Arc<Mutex<Option<Arc<PdfWorker>>>> = Arc::new(Mutex::new(None));
    let (tx, rx) = channel();
    let s = slot.clone();
    let w = Arc::new(PdfWorker::start(
        e,
        move |_| Ok(bytes.clone()),
        move |_r| {
            let w = s.lock().unwrap().clone().unwrap();
            let _ = tx.send(w.call(0, |_| 1).is_err());
        },
        WorkerOptions::default(),
    ));
    *slot.lock().unwrap() = Some(w.clone());
    w.request([req("x", 0, 1.0, Tile { x: 0, y: 0, width: 8, height: 8 }, 0, 0, 0)]);
    assert!(rx.recv_timeout(Duration::from_secs(10)).unwrap());
    slot.lock().unwrap().take();
}

#[test]
fn export_through_the_worker() {
    let e = engine();
    let (w, _rx, _) = worker(e, vec![make_pdf(&e, 3)], 4);
    let line: Vec<InkPoint> = (0..=40).map(|i| InkPoint { x: 100.0 + i as f32 * 7.5, y: 300.0, pressure: 0.5, t_ms: i * 8 }).collect();
    let black = Brush { width: 6.0, thinning: 0.0, color: 0xFF00_0000, kind: 0 };
    let sheets = vec![
        ExportSheet { source: SheetSource::Pdf { doc: "0".into(), page: 2 }, strokes: vec![(line.clone(), black)], paper: None, texts: vec![], images: vec![], gaps: vec![] },
        ExportSheet { source: SheetSource::Blank { width: 400.0, height: 400.0 }, strokes: vec![(line, black)], paper: None, texts: vec![], images: vec![], gaps: vec![] },
        ExportSheet { source: SheetSource::Pdf { doc: "0".into(), page: 0 }, strokes: vec![], paper: None, texts: vec![], images: vec![], gaps: vec![] },
    ];
    let out = e.open(w.export(priority::INTERACTIVE, &sheets).unwrap()).unwrap();
    assert_eq!(out.page_count(), 3);
    assert!(out.page_text(0).unwrap().text().starts_with("Seite 2"));
    assert!(out.page_text(2).unwrap().text().starts_with("Seite 0"));
    for p in [0, 1] {
        let img = out.render_page(p, out.page_size(p).unwrap().width as u32).unwrap();
        assert_eq!(pixel(&img, 250, 300), [0, 0, 0, 255], "ink on page {p}");
    }
    let missing = [ExportSheet { source: SheetSource::Pdf { doc: "7".into(), page: 0 }, strokes: vec![], paper: None, texts: vec![], images: vec![], gaps: vec![] }];
    assert!(w.export(0, &missing).is_err());
}

#[test]
fn export_draws_paper_ruling_under_no_ink() {
    let e = engine();
    let (w, _rx, _) = worker(e, vec![make_pdf(&e, 1)], 4);
    let sheets = vec![ExportSheet {
        source: SheetSource::Blank { width: 200.0, height: 200.0 },
        strokes: vec![],
        paper: Some(n0te_pdf::Paper { background: Background::Grid, spacing: 20.0 }),
        texts: vec![],
        images: vec![],
        gaps: vec![],
    }];
    let out = e.open(w.export(priority::INTERACTIVE, &sheets).unwrap()).unwrap();
    let img = out.render_page(0, 400).unwrap(); // 2 px per point
    let on_line = pixel(&img, 40, 10); // x = 20 pt: a grid line
    let between = pixel(&img, 50, 50); // x = 25 pt, y = 25 pt: between lines
    assert!(on_line[0] < 250, "grid line drawn: {on_line:?}");
    assert_eq!(between, [255, 255, 255, 255], "paper between the lines stays white");
}

#[test]
fn export_typesets_text_boxes_that_pdf_readers_can_read() {
    let e = engine();
    let (w, _rx, _) = worker(e, vec![make_pdf(&e, 1)], 4);
    let text = "# Satz 4.2\n- Mittelwertsätze gelten für **stetige** Funktionen\n> Frage (an Prof)";
    let sheets = vec![ExportSheet {
        source: SheetSource::Blank { width: 595.0, height: 842.0 },
        strokes: vec![],
        paper: None,
        texts: vec![n0te_pdf::PdfText { x: 60.0, y: 80.0, w: 300.0, size: 14.0, text: text.into() }],
        images: vec![],
        gaps: vec![],
    }];
    let out = e.open(w.export(priority::INTERACTIVE, &sheets).unwrap()).unwrap();
    let read = out.page_text(0).unwrap().text();
    for want in ["Satz 4.2", "Mittelwertsätze", "stetige", "Frage (an Prof)", "•"] {
        assert!(read.contains(want), "{want:?} in {read:?}");
    }
    assert!(!read.contains("**"), "Markdown marks are not printed");
    // Upright text near the box's top-left: the first glyph's box lies below y = 80 from the top.
    let img = out.render_page(0, 595).unwrap();
    let dark = (0..img.width).flat_map(|x| (60..140).map(move |y| (x, y))).any(|(x, y)| pixel(&img, x, y)[0] < 100);
    assert!(dark, "the heading is drawn where the box is");
}

#[test]
fn notebook_import_and_export_round_trip() {
    let e = engine();
    let root = std::env::temp_dir().join(format!("n0te-pdf-nb-{}", n0te_core::ids::new_id(9)));
    let lib = Arc::new(Library::open(&root).unwrap());
    let l = lib.clone();
    let w = PdfWorker::start(e, move |key| Ok(l.blob(key)?), |_| {}, WorkerOptions::default());
    let nb = lib.create_notebook("Folien").unwrap();
    nb.add_sheet(None, 0, &SheetSpec::a4(Background::Lines)).unwrap();
    let g = nb.add_group(None, 9, "Vorlesung 1").unwrap();

    let slides = make_pdf(&e, 3); // pdfium writes a new /ID on every save: make it once
    let imported = import_pdf(&lib, &nb, &w, slides.clone(), Some(&g), 0).unwrap();
    assert_eq!(imported.len(), 3);
    assert!(import_pdf(&lib, &nb, &w, b"%PDF-1.7 garbage".to_vec(), None, 0).is_err());
    let blob = n0te_core::store::blob_id(&slides);
    assert_eq!(lib.blob(&blob).unwrap(), slides, "the PDF is stored as a blob");
    let order = nb.sheet_order();
    assert_eq!(order.len(), 4);
    assert_eq!(order[1..], imported.iter().map(|s| s.sheet_id.clone()).collect::<Vec<_>>()[..]);

    // Ink on the second slide (stored), then export everything.
    let sheet = lib.load_sheet(&nb.id(), &imported[1].sheet_id).unwrap();
    let line: Vec<InkPoint> = (0..=40).map(|i| InkPoint { x: 100.0 + i as f32 * 7.5, y: 300.0, pressure: 0.5, t_ms: i * 8 }).collect();
    sheet.add_stroke(Brush { width: 6.0, thinning: 0.0, color: 0xFFD7_1921, kind: 0 }, &line).unwrap();
    lib.save_sheet(&nb.id(), &imported[1].sheet_id, &sheet).unwrap();
    let pages = export_sheets(&nb, None, |id| Ok(lib.load_sheet(&nb.id(), id)?.strokes())).unwrap();
    let out = e.open(w.export(priority::INTERACTIVE, &pages).unwrap()).unwrap();
    assert_eq!(out.page_count(), 4);
    assert_eq!(out.page_text(0).unwrap().text(), "");
    assert!(out.page_text(2).unwrap().text().starts_with("Seite 1"));
    let img = out.render_page(2, 595).unwrap();
    assert_eq!(pixel(&img, 250, 300), [0xD7, 0x19, 0x21, 255]);

    // Tiles of an imported page through the blob store.
    let (tx, rx) = channel();
    let tx = Mutex::new(tx);
    let l = lib.clone();
    let w2 = PdfWorker::start(e, move |key| Ok(l.blob(key)?), move |r| tx.lock().unwrap().send(r).unwrap(), WorkerOptions::default());
    w2.request([req(&blob, 0, 1.0, Tile { x: 60, y: 742, width: 40, height: 40 }, 0, 0, 0)]);
    let img = rx.recv_timeout(Duration::from_secs(10)).unwrap().image.unwrap();
    assert_eq!(pixel(&img, 20, 20), [30, 90, 200, 255], "the blue rectangle");
    drop((w, w2));
    std::fs::remove_dir_all(root).unwrap();
}

/// A 16 × 8 px red JPEG.
const RED_JPEG: &[u8] = &[255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0, 255, 219, 0, 67, 0, 3, 2, 2, 3, 2, 2, 3, 3, 3, 3, 4, 3, 3, 4, 5, 8, 5, 5, 4, 4, 5, 10, 7, 7, 6, 8, 12, 10, 12, 12, 11, 10, 11, 11, 13, 14, 18, 16, 13, 14, 17, 14, 11, 11, 16, 22, 16, 17, 19, 20, 21, 21, 21, 12, 15, 23, 24, 22, 20, 24, 18, 20, 21, 20, 255, 219, 0, 67, 1, 3, 4, 4, 5, 4, 5, 9, 5, 5, 9, 20, 13, 11, 13, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 20, 255, 192, 0, 17, 8, 0, 8, 0, 16, 3, 1, 34, 0, 2, 17, 1, 3, 17, 1, 255, 196, 0, 31, 0, 0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 255, 196, 0, 181, 16, 0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 125, 1, 2, 3, 0, 4, 17, 5, 18, 33, 49, 65, 6, 19, 81, 97, 7, 34, 113, 20, 50, 129, 145, 161, 8, 35, 66, 177, 193, 21, 82, 209, 240, 36, 51, 98, 114, 130, 9, 10, 22, 23, 24, 25, 26, 37, 38, 39, 40, 41, 42, 52, 53, 54, 55, 56, 57, 58, 67, 68, 69, 70, 71, 72, 73, 74, 83, 84, 85, 86, 87, 88, 89, 90, 99, 100, 101, 102, 103, 104, 105, 106, 115, 116, 117, 118, 119, 120, 121, 122, 131, 132, 133, 134, 135, 136, 137, 138, 146, 147, 148, 149, 150, 151, 152, 153, 154, 162, 163, 164, 165, 166, 167, 168, 169, 170, 178, 179, 180, 181, 182, 183, 184, 185, 186, 194, 195, 196, 197, 198, 199, 200, 201, 202, 210, 211, 212, 213, 214, 215, 216, 217, 218, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 255, 196, 0, 31, 1, 0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 255, 196, 0, 181, 17, 0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 119, 0, 1, 2, 3, 17, 4, 5, 33, 49, 6, 18, 65, 81, 7, 97, 113, 19, 34, 50, 129, 8, 20, 66, 145, 161, 177, 193, 9, 35, 51, 82, 240, 21, 98, 114, 209, 10, 22, 36, 52, 225, 37, 241, 23, 24, 25, 26, 38, 39, 40, 41, 42, 53, 54, 55, 56, 57, 58, 67, 68, 69, 70, 71, 72, 73, 74, 83, 84, 85, 86, 87, 88, 89, 90, 99, 100, 101, 102, 103, 104, 105, 106, 115, 116, 117, 118, 119, 120, 121, 122, 130, 131, 132, 133, 134, 135, 136, 137, 138, 146, 147, 148, 149, 150, 151, 152, 153, 154, 162, 163, 164, 165, 166, 167, 168, 169, 170, 178, 179, 180, 181, 182, 183, 184, 185, 186, 194, 195, 196, 197, 198, 199, 200, 201, 202, 210, 211, 212, 213, 214, 215, 216, 217, 218, 226, 227, 228, 229, 230, 231, 232, 233, 234, 242, 243, 244, 245, 246, 247, 248, 249, 250, 255, 218, 0, 12, 3, 1, 0, 2, 17, 3, 17, 0, 63, 0, 241, 42, 40, 162, 191, 42, 63, 191, 79, 255, 217];

#[test]
fn export_embeds_images_upright_where_they_are_placed() {
    assert_eq!(n0te_pdf::jpeg_info(RED_JPEG), Some((16, 8, 3)));
    let e = engine();
    let (w, _rx, _) = worker(e, vec![make_pdf(&e, 1)], 4);
    let sheets = vec![ExportSheet {
        source: SheetSource::Blank { width: 200.0, height: 200.0 },
        strokes: vec![],
        paper: None,
        texts: vec![],
        images: vec![n0te_pdf::PdfImage { jpeg: RED_JPEG.to_vec(), x: 20.0, y: 30.0, w: 100.0, h: 50.0 }],
        gaps: vec![],
    }];
    let out = e.open(w.export(priority::INTERACTIVE, &sheets).unwrap()).unwrap();
    let img = out.render_page(0, 200).unwrap(); // 1 px per point
    let inside = pixel(&img, 70, 55);
    assert!(inside[0] > 180 && inside[1] < 80 && inside[2] < 80, "image drawn at (20, 30) 100 × 50: {inside:?}");
    assert_eq!(pixel(&img, 70, 90), [255, 255, 255, 255], "below it the page stays white");
    assert_eq!(pixel(&img, 10, 55), [255, 255, 255, 255], "left of it too");
}

/// One A4 page: a full-width blue band at page y 200..400, a red box at y 600..700 (x 100..300),
/// "Oben" near the top, "Unten" near the bottom, and two links (y 50..90 and y 600..700).
fn striped_pdf(e: &PdfEngine) -> Vec<u8> {
    e.with_raw(|pdfium| {
        let mut doc = pdfium.create_new_pdf().unwrap();
        let font = doc.fonts_mut().helvetica();
        let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::from_points(PdfPoints::new(595.0), PdfPoints::new(842.0))).unwrap();
        let h = 842.0;
        // PdfRect::new_from_values(bottom, left, top, right), in PDF user space (y up).
        page.objects_mut().create_path_object_rect(PdfRect::new_from_values(h - 400.0, 0.0, h - 200.0, 595.0), None, None, Some(PdfColor::new(30, 90, 200, 255))).unwrap();
        page.objects_mut().create_path_object_rect(PdfRect::new_from_values(h - 700.0, 100.0, h - 600.0, 300.0), None, None, Some(PdfColor::new(215, 25, 33, 255))).unwrap();
        page.objects_mut().create_text_object(PdfPoints::new(72.0), PdfPoints::new(h - 100.0), "Oben", font, PdfPoints::new(24.0)).unwrap();
        page.objects_mut().create_text_object(PdfPoints::new(72.0), PdfPoints::new(h - 780.0), "Unten", font, PdfPoints::new(24.0)).unwrap();
        for (y0, y1) in [(50.0, 90.0), (600.0, 700.0)] {
            let mut link = page.annotations_mut().create_link_annotation("https://example.org/n0te").unwrap();
            link.set_bounds(PdfRect::new_from_values(h - y1, 350.0, h - y0, 500.0)).unwrap();
        }
        doc.save_to_bytes().unwrap()
    })
}

const BLUE: [u8; 4] = [30, 90, 200, 255];
const RED: [u8; 4] = [215, 25, 33, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

fn pdf_sheet(doc: &str, strokes: Vec<(Vec<InkPoint>, Brush)>, gaps: Vec<(f32, f32)>) -> ExportSheet {
    ExportSheet { source: SheetSource::Pdf { doc: doc.into(), page: 0 }, strokes, paper: None, texts: vec![], images: vec![], gaps }
}

fn hline(y: f32) -> Vec<InkPoint> {
    (0..=40).map(|i| InkPoint { x: 100.0 + i as f32 * 7.5, y, pressure: 0.5, t_ms: i * 8 }).collect()
}

/// Page objects of one type on page `p` of `bytes`.
fn count_objects(e: &PdfEngine, bytes: &[u8], p: i32, kind: PdfPageObjectType) -> usize {
    e.with_raw(|pdfium| pdfium.load_pdf_from_byte_slice(bytes, None).unwrap().pages().get(p).unwrap().objects().iter().filter(|o| o.object_type() == kind).count())
}

#[test]
fn export_splits_a_pdf_page_where_space_was_inserted() {
    let e = engine();
    let (w, _rx, _) = worker(e, vec![striped_pdf(&e)], 4);
    let black = Brush { width: 6.0, thinning: 0.0, color: 0xFF00_0000, kind: 0 };
    // 150 pt inserted at y 300 (through the blue band); ink in the new space at y 380 and on
    // the moved red box (y 650 + 150).
    let sheets = vec![
        pdf_sheet("0", vec![(hline(380.0), black), (hline(800.0), black)], vec![(300.0, 150.0)]),
        pdf_sheet("0", vec![], vec![]),
    ];
    let bytes = w.export(priority::INTERACTIVE, &sheets).unwrap();
    let out = e.open(bytes.clone()).unwrap();
    let size = out.page_size(0).unwrap();
    assert_eq!((size.width, size.height), (595.0, 992.0), "the page got 150 pt longer");
    assert_eq!(out.page_size(1).unwrap(), n0te_pdf::PageSize { width: 595.0, height: 842.0 }, "a page without gaps keeps its size");

    let img = out.render_page(0, 595).unwrap(); // 1 px per point
    assert_eq!((img.width, img.height), (595, 992));
    assert_eq!(pixel(&img, 300, 250), BLUE, "blue band above the line stays");
    assert_eq!(pixel(&img, 300, 296), BLUE);
    assert_eq!(pixel(&img, 300, 304), WHITE, "the inserted space is blank paper");
    assert_eq!(pixel(&img, 300, 360), WHITE, "where the band was before");
    assert_eq!(pixel(&img, 300, 380), [0, 0, 0, 255], "ink in the inserted space");
    assert_eq!(pixel(&img, 50, 380), WHITE);
    assert_eq!(pixel(&img, 300, 446), WHITE);
    assert_eq!(pixel(&img, 300, 454), BLUE, "the rest of the band, 150 pt down");
    assert_eq!(pixel(&img, 300, 546), BLUE);
    assert_eq!(pixel(&img, 300, 556), WHITE, "the band ends 150 pt lower too");
    assert_eq!(pixel(&img, 200, 680), WHITE, "the red box moved away");
    assert_eq!(pixel(&img, 200, 760), RED, "the red box, 150 pt down");
    assert_eq!(pixel(&img, 150, 800), [0, 0, 0, 255], "ink drawn on the moved box");
    assert_eq!(pixel(&img, 590, 250), BLUE, "bands span the whole width");
    assert_eq!(pixel(&img, 590, 500), BLUE);
    // The unsplit copy of the page renders as before.
    let plain = out.render_page(1, 595).unwrap();
    assert_eq!((pixel(&plain, 300, 380), pixel(&plain, 200, 650), pixel(&plain, 300, 470)), (BLUE, RED, WHITE));

    // Still vector: text stays text, the PDF is a form drawn once per band. Text extraction sees
    // each band's whole form, so every text comes once per band: the visible copy where it is
    // drawn, the other one under the other band's clip (invisible, but found by text search).
    let text = out.page_text(0).unwrap();
    let hits = |word: &str| text.find(word).into_iter().map(|h| h[0]).collect::<Vec<_>>();
    let (oben, unten) = (hits("Oben"), hits("Unten"));
    assert_eq!((oben.len(), unten.len()), (2, 2), "{:?}", text.text());
    assert!(oben.iter().any(|r| r.y0 > 70.0 && r.y1 < 110.0), "Oben where it was: {oben:?}");
    assert!(unten.iter().any(|r| r.y0 > 750.0 + 150.0 && r.y1 < 790.0 + 150.0), "Unten 150 pt lower: {unten:?}");
    assert_eq!(count_objects(&e, &bytes, 0, PdfPageObjectType::XObjectForm), 2, "the PDF once per band");
    assert_eq!(count_objects(&e, &bytes, 0, PdfPageObjectType::Image), 0, "nothing rasterized");
    assert_eq!(count_objects(&e, &bytes, 1, PdfPageObjectType::XObjectForm), 0);

    // Links move with their part of the page (user space: y up, the page now reaches y = -150).
    let links: Vec<(f32, f32)> = e.with_raw(|pdfium| {
        let doc = pdfium.load_pdf_from_byte_slice(&bytes, None).unwrap();
        let page = doc.pages().get(0).unwrap();
        page.annotations().iter().map(|a| a.bounds().unwrap()).map(|r| (r.bottom().value, r.top().value)).collect()
    });
    assert_eq!(links.len(), 2, "{links:?}");
    assert!(links.contains(&(752.0, 792.0)), "the link above the line stays: {links:?}");
    assert!(links.contains(&(-8.0, 92.0)), "the link below it moves 150 pt down: {links:?}");

    // The file is well-formed.
    let path = std::env::temp_dir().join(format!("n0te-gaps-{}-{}.pdf", std::process::id(), n0te_core::ids::new_id(6)));
    std::fs::write(&path, &bytes).unwrap();
    match std::process::Command::new("qpdf").arg("--check").arg(&path).output() {
        Ok(o) => assert!(o.status.success(), "qpdf --check:\n{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)),
        Err(_) => eprintln!("qpdf not installed, skipping the qpdf check"),
    }
    std::fs::remove_file(path).unwrap();
}

/// One page of `media` points with a crop box (left, bottom, right, top), a rotation and a blue
/// square near the middle of the crop box.
fn cropped_pdf(e: &PdfEngine, rot: PdfPageRenderRotation) -> Vec<u8> {
    e.with_raw(|pdfium| {
        let mut doc = pdfium.create_new_pdf().unwrap();
        let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::from_points(PdfPoints::new(600.0), PdfPoints::new(800.0))).unwrap();
        page.objects_mut().create_path_object_rect(PdfRect::new_from_values(250.0, 350.0, 300.0, 400.0), None, None, Some(PdfColor::new(30, 90, 200, 255))).unwrap();
        page.boundaries_mut().set_crop(PdfRect::new_from_values(100.0, 50.0, 700.0, 500.0)).unwrap();
        page.set_rotation(rot);
        doc.save_to_bytes().unwrap()
    })
}

/// First and last pixel row with blue in it.
fn blue_rows(img: &Rgba) -> Option<(u32, u32)> {
    let rows: Vec<u32> = (0..img.height).filter(|&y| (0..img.width).any(|x| pixel(img, x, y) == BLUE)).collect();
    Some((*rows.first()?, *rows.last()?))
}

fn row(img: &Rgba, y: u32) -> &[u8] {
    &img.data[(y * img.width * 4) as usize..((y + 1) * img.width * 4) as usize]
}

#[test]
fn split_pages_grow_downwards_on_screen_for_every_rotation_and_crop_box() {
    use PdfPageRenderRotation::*;
    let e = engine();
    let rotations = [None, Degrees90, Degrees180, Degrees270];
    let (w, _rx, _) = worker(e, rotations.iter().map(|&r| cropped_pdf(&e, r)).collect(), 4);
    for (k, rot) in rotations.iter().enumerate() {
        let doc = k.to_string();
        let plain_bytes = w.export(priority::INTERACTIVE, &[pdf_sheet(&doc, vec![], vec![])]).unwrap();
        let plain = e.open(plain_bytes).unwrap();
        let size = plain.page_size(0).unwrap();
        let before = plain.render_page(0, size.width as u32).unwrap();
        let (top, bottom) = blue_rows(&before).unwrap();
        assert!(bottom - top > 40, "{rot:?}: the square is {top}..{bottom}");
        // 100 pt inserted through the middle of the square.
        let cut = (top + bottom) / 2;
        let bytes = w.export(priority::INTERACTIVE, &[pdf_sheet(&doc, vec![], vec![(cut as f32, 100.0)])]).unwrap();
        let split = e.open(bytes.clone()).unwrap();
        let grown = split.page_size(0).unwrap();
        assert_eq!((grown.width, grown.height), (size.width, size.height + 100.0), "{rot:?}");
        let after = split.render_page(0, size.width as u32).unwrap();
        assert_eq!(after.height, before.height + 100, "{rot:?}");
        for y in [top.saturating_sub(5), top + 2, cut - 3] {
            assert_eq!(row(&after, y), row(&before, y), "{rot:?}: row {y} above the cut stays");
        }
        for y in [cut + 3, bottom - 2, bottom + 5] {
            assert_eq!(row(&after, y + 100), row(&before, y), "{rot:?}: row {y} below the cut moves 100 down");
        }
        for y in cut + 3..cut + 97 {
            assert!(row(&after, y).chunks_exact(4).all(|p| p == WHITE), "{rot:?}: row {y} is inserted space");
        }
        assert_eq!(count_objects(&e, &bytes, 0, PdfPageObjectType::XObjectForm), 2, "{rot:?}");
    }
}

/// A handmade one-page A4 PDF whose content is split over two streams, the first one
/// hex-encoded (a filter the writer does not decode): the fill colour, then a full-width band
/// at page y 200..400.
fn two_stream_pdf() -> Vec<u8> {
    let colour: String = b"0.1176 0.3529 0.7843 rg".iter().map(|b| format!("{b:02x}")).collect::<String>() + ">";
    let band = "0 442 595 200 re f";
    let objs = [
        "<</Type/Catalog/Pages 2 0 R>>".to_string(),
        "<</Type/Pages/Kids[3 0 R]/Count 1/MediaBox[0 0 595 842]>>".to_string(),
        "<</Type/Page/Parent 2 0 R/Contents[4 0 R 5 0 R]/Resources<<>>>>".to_string(),
        format!("<</Length {}/Filter/ASCIIHexDecode>>stream\n{colour}\nendstream", colour.len()),
        format!("<</Length {}>>stream\n{band}\nendstream", band.len()),
    ];
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f\r\n", objs.len() + 1).as_bytes());
    for o in offs {
        pdf.extend_from_slice(format!("{o:010} 00000 n\r\n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<</Size {}/Root 1 0 R>>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    pdf
}

#[test]
fn content_in_several_streams_splits_too() {
    let e = engine();
    let black = Brush { width: 6.0, thinning: 0.0, color: 0xFF00_0000, kind: 0 };
    let striped = striped_pdf(&e);
    // A page exported before, with ink: its content is now four streams (q, page, Q, ink).
    let (w0, _rx0, _) = worker(e, vec![striped], 4);
    let inked = w0.export(priority::INTERACTIVE, &[pdf_sheet("0", vec![(hline(650.0), black)], vec![])]).unwrap();
    let (w, _rx, _) = worker(e, vec![inked, two_stream_pdf()], 4);
    let check = |bytes: &[u8], what: &str| {
        let img = e.open(bytes.to_vec()).unwrap().render_page(0, 595).unwrap();
        assert_eq!(img.height, 992, "{what}");
        assert_eq!(pixel(&img, 300, 250), BLUE, "{what}: above the line");
        assert_eq!(pixel(&img, 300, 380), WHITE, "{what}: inserted space");
        assert_eq!(pixel(&img, 300, 500), BLUE, "{what}: below the line, moved");
        img
    };
    // Flate and plain streams are joined into one form.
    let again = w.export(priority::INTERACTIVE, &[pdf_sheet("0", vec![], vec![(300.0, 150.0)])]).unwrap();
    let img = check(&again, "exported before");
    assert_eq!(pixel(&img, 150, 800), [0, 0, 0, 255], "the earlier ink moved with its part of the page");
    assert_eq!(pixel(&img, 150, 650), WHITE);
    assert_eq!(count_objects(&e, &again, 0, PdfPageObjectType::XObjectForm), 2);
    // The earlier ink is still in its layer: hiding n0te ink hides it inside the form too.
    let hidden = e.open(n0te_pdf::hide_ink(again).unwrap()).unwrap().render_page(0, 595).unwrap();
    assert_eq!(pixel(&hidden, 150, 800), RED, "earlier ink hidden, the red box under it shows");
    // A filter the writer does not decode: the streams are drawn again per band.
    let hex = w.export(priority::INTERACTIVE, &[pdf_sheet("1", vec![], vec![(300.0, 150.0)])]).unwrap();
    check(&hex, "hex-encoded stream");
    assert_eq!(count_objects(&e, &hex, 0, PdfPageObjectType::XObjectForm), 0);
}
