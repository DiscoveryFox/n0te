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
    let black = Brush { width: 6.0, thinning: 0.0, color: 0xFF00_0000 };
    let sheets = vec![
        ExportSheet { source: SheetSource::Pdf { doc: "0".into(), page: 2 }, strokes: vec![(line.clone(), black)] },
        ExportSheet { source: SheetSource::Blank { width: 400.0, height: 400.0 }, strokes: vec![(line, black)] },
        ExportSheet { source: SheetSource::Pdf { doc: "0".into(), page: 0 }, strokes: vec![] },
    ];
    let out = e.open(w.export(priority::INTERACTIVE, &sheets).unwrap()).unwrap();
    assert_eq!(out.page_count(), 3);
    assert!(out.page_text(0).unwrap().text().starts_with("Seite 2"));
    assert!(out.page_text(2).unwrap().text().starts_with("Seite 0"));
    for p in [0, 1] {
        let img = out.render_page(p, out.page_size(p).unwrap().width as u32).unwrap();
        assert_eq!(pixel(&img, 250, 300), [0, 0, 0, 255], "ink on page {p}");
    }
    let missing = [ExportSheet { source: SheetSource::Pdf { doc: "7".into(), page: 0 }, strokes: vec![] }];
    assert!(w.export(0, &missing).is_err());
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
    sheet.add_stroke(Brush { width: 6.0, thinning: 0.0, color: 0xFFD7_1921 }, &line).unwrap();
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
