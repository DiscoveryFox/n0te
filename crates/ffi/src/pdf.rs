//! PDF for the Android app: the process's PDF worker ([`PdfService`]), tiles through a callback
//! ([`TileSink`]), text search, and import/export on [`NotebookHandle`].
//!
//! Kotlin:
//! ```kotlin
//! val pdf = PdfService.start(library, "${applicationInfo.nativeLibraryDir}/libpdfium.so",
//!                            listOf("/system/fonts"), sink)          // once per process
//! val sheets = notebook.importPdf(pdf, bytes, groupNode, 0u)         // one sheet per page
//! pdf.requestTiles(listOf(FfiTileRequest(blob, page, scale, x, y, w, h, priority, view, gen)))
//! pdf.cancelTiles(view, gen)                                         // or just request with gen+1
//! val bytes = notebook.exportPdf(pdf, null, false)                   // all sheets, reading order
//! ```
//! Page coordinates are points from the top-left of the displayed page; a page rendered at
//! `scale` is `pdfPagePixels(width, height, scale)` pixels and tiles are rectangles of that.

use std::sync::{Arc, Mutex, Weak};

use n0te_pdf::worker::priority;
use n0te_pdf::{PdfEngine, PdfWorker, Tile, TileRequest, TileResult, WorkerOptions};

use crate::{N0teError, NoteLibrary, NotebookHandle, R, lock};

impl From<n0te_pdf::Error> for N0teError {
    fn from(e: n0te_pdf::Error) -> Self {
        N0teError::Failed { msg: e.to_string() }
    }
}

/// One tile to render. `priority`: lower first (0 = on screen, 10 = import/export/search,
/// 20 = prefetch, 30 = thumbnails). Generations count per `view`: a request with a newer
/// generation drops the view's queued tiles of older generations (they get no callback).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiTileRequest {
    /// The PDF's blob id (`FfiSheetSpec.pdf_blob`).
    pub blob: String,
    pub page: u32,
    /// Pixels per point.
    pub scale: f32,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub priority: u32,
    pub view: u32,
    pub generation: u64,
}

/// A rendered tile: `width * height * 4` bytes of RGBA, rows top to bottom, opaque (so it can
/// go straight into an ARGB_8888 bitmap with `copyPixelsFromBuffer`). On failure `rgba` is
/// empty and `error` says why.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiTile {
    /// The newest request for this tile.
    pub request: FfiTileRequest,
    pub rgba: Vec<u8>,
    pub error: Option<String>,
}

/// Receives rendered tiles, on the PDF worker thread (post to the UI thread yourself).
#[uniffi::export(foreign)]
pub trait TileSink: Send + Sync {
    fn on_tile(&self, tile: FfiTile);
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct FfiPageSize {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, uniffi::Record)]
pub struct FfiRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// One search hit: one box per line it spans (page coordinates).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiTextMatch {
    pub boxes: Vec<FfiRect>,
}

/// A sheet created by `importPdf`.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiImportedSheet {
    pub node: String,
    pub sheet_id: String,
    pub page: u32,
    pub width: f32,
    pub height: f32,
}

fn to_ffi_request(r: TileRequest) -> FfiTileRequest {
    FfiTileRequest {
        blob: r.doc,
        page: r.page,
        scale: r.scale,
        x: r.tile.x,
        y: r.tile.y,
        width: r.tile.width,
        height: r.tile.height,
        priority: r.priority,
        view: r.view,
        generation: r.generation,
    }
}

fn from_ffi_request(r: FfiTileRequest) -> TileRequest {
    TileRequest {
        doc: r.blob,
        page: r.page,
        scale: r.scale,
        tile: Tile { x: r.x, y: r.y, width: r.width, height: r.height },
        priority: r.priority,
        view: r.view,
        generation: r.generation,
    }
}

fn to_ffi_tile(r: TileResult) -> FfiTile {
    let request = to_ffi_request(r.request);
    match r.image {
        Ok(img) => FfiTile { request, rgba: img.data, error: None },
        Err(e) => FfiTile { request, rgba: Vec::new(), error: Some(e.to_string()) },
    }
}

/// The live service, so a process never runs two workers.
static RUNNING: Mutex<Weak<PdfService>> = Mutex::new(Weak::new());

/// The process's PDF worker: pdfium, the open documents and the tile queue.
#[derive(uniffi::Object)]
pub struct PdfService {
    worker: PdfWorker,
    lib: Arc<NoteLibrary>,
    /// Pages' lines with their boxes, for search (pdfium's text extraction isn't free).
    lines: std::sync::Mutex<std::collections::HashMap<(String, u32), Vec<(String, n0te_pdf::Rect)>>>,
}

impl PdfService {
    /// A page's lines with their boxes (page points, top-left origin), cached.
    pub(crate) fn page_lines(&self, blob: &str, page: u32) -> Vec<(String, n0te_pdf::Rect)> {
        let key = (blob.to_string(), page);
        if let Some(l) = crate::lock(&self.lines).get(&key) {
            return l.clone();
        }
        let b = blob.to_string();
        let lines = self.worker.call(priority::INTERACTIVE, move |d| d.get(&b)?.page_text(page)).ok().and_then(|r| r.ok()).map(|t| t.lines()).unwrap_or_default();
        crate::lock(&self.lines).insert(key, lines.clone());
        lines
    }
}

#[uniffi::export]
impl PdfService {
    /// Loads pdfium from `pdfium_path` (Android: `"$nativeLibraryDir/libpdfium.so"`) with extra
    /// font directories for fonts a PDF does not embed (Android: `/system/fonts`), and starts the
    /// worker. Documents are read from `library`'s blobs. One per process: while one is alive
    /// (not closed), starting another fails.
    #[uniffi::constructor]
    pub fn start(library: Arc<NoteLibrary>, pdfium_path: String, font_dirs: Vec<String>, sink: Arc<dyn TileSink>) -> R<Arc<Self>> {
        let mut running = lock(&RUNNING);
        if running.upgrade().is_some() {
            return Err(N0teError::Failed { msg: "a PdfService is already running in this process; reuse it or close it first".into() });
        }
        let dirs: Vec<&str> = font_dirs.iter().map(String::as_str).collect();
        let engine = PdfEngine::load_with_font_dirs(&pdfium_path, &dirs)?;
        let lib = library.clone();
        let worker = PdfWorker::start(engine, move |blob| Ok(lib.inner.blob(blob)?), move |r| sink.on_tile(to_ffi_tile(r)), WorkerOptions::default());
        let service = Arc::new(PdfService { worker, lib: library, lines: Default::default() });
        *running = Arc::downgrade(&service);
        Ok(service)
    }

    /// Queues tiles; each rendered one arrives at the sink.
    pub fn request_tiles(&self, requests: Vec<FfiTileRequest>) {
        self.worker.request(requests.into_iter().map(from_ffi_request));
    }

    /// Drops the queued tiles of `view` older than `generation` (and ignores such requests).
    pub fn cancel_tiles(&self, view: u32, generation: u64) {
        self.worker.cancel(view, generation);
    }

    /// Requests and jobs waiting in the queue.
    pub fn queued(&self) -> u32 {
        self.worker.queued() as u32
    }

    /// Page sizes in points, as displayed (after crop box and rotation).
    pub fn page_sizes(&self, blob: String) -> R<Vec<FfiPageSize>> {
        let sizes = self.worker.call(priority::INTERACTIVE, move |d| d.get(&blob)?.page_sizes())??;
        Ok(sizes.into_iter().map(|s| FfiPageSize { width: s.width, height: s.height }).collect())
    }

    /// The text of a page in reading order.
    pub fn page_text(&self, blob: String, page: u32) -> R<String> {
        Ok(self.worker.call(priority::INTERACTIVE, move |d| d.get(&blob)?.page_text(page))??.text())
    }

    /// Case-insensitive search in one page's text.
    pub fn find_text(&self, blob: String, page: u32, query: String) -> R<Vec<FfiTextMatch>> {
        let hits = self.worker.call(priority::INTERACTIVE, move |d| d.get(&blob)?.page_text(page).map(|t| t.find(&query)))??;
        Ok(hits
            .into_iter()
            .map(|boxes| FfiTextMatch { boxes: boxes.into_iter().map(|r| FfiRect { x0: r.x0, y0: r.y0, x1: r.x1, y1: r.y1 }).collect() })
            .collect())
    }
}

/// Pixel size `[width, height]` of a page of `width` x `height` points rendered at `scale`;
/// tiles must lie inside it.
#[uniffi::export]
pub fn pdf_page_pixels(width: f32, height: f32, scale: f32) -> Vec<u32> {
    let (w, h) = n0te_pdf::page_pixels(n0te_pdf::PageSize { width, height }, scale);
    vec![w, h]
}

impl NotebookHandle {
    /// Strokes and text boxes of a sheet for export: from an open handle (unsaved edits
    /// included), else stored.
    #[allow(clippy::type_complexity)]
    fn content_for_export(&self, nb_id: &str, sheet_id: &str, width: f32) -> n0te_pdf::Result<(Vec<n0te_core::sheet::Stroke>, Vec<n0te_core::sheet::TextBox>, Vec<n0te_core::sheet::ImageObj>)> {
        // Plots go in as ink under the handwriting (the same look as on screen).
        let ink = |s: &n0te_core::sheet::Sheet| {
            let mut v = s.plot_strokes();
            v.extend(s.strokes());
            v
        };
        if let Some(open) = self.open_sheet_handle(sheet_id) {
            let s = lock(&open.sheet);
            return Ok((ink(&s), s.text_boxes_with_body(width), s.images()));
        }
        let s = self.lib.inner.load_sheet(nb_id, sheet_id)?;
        Ok((ink(&s), s.text_boxes_with_body(width), s.images()))
    }

    fn export_bytes(&self, pdf: &PdfService, sheet_ids: Option<Vec<String>>, sides_at_end: bool) -> R<(Vec<u8>, u32)> {
        let order = if sides_at_end { n0te_pdf::notebook::ExportOrder::SidesAtEnd } else { n0te_pdf::notebook::ExportOrder::Inline };
        let pages = {
            // A copy of the index: reading every sheet takes a while, and the notebook stays
            // free for the UI meanwhile (outline, saves) instead of waiting on its lock.
            let nb = n0te_core::notebook::Notebook::load(0, &lock(&self.nb).snapshot()?)?;
            let id = nb.id();
            let specs = nb.sheet_specs();
            n0te_pdf::notebook::export_sheets_with_images(
                &nb,
                sheet_ids.as_deref(),
                order,
                |sheet| self.content_for_export(&id, sheet, specs.get(sheet).map(|s| s.width).unwrap_or(595.0)),
                |blob| self.lib.inner.blob(blob).ok(),
            )?
        };
        if pages.is_empty() {
            return Err(N0teError::Failed { msg: "nothing to export".into() });
        }
        Ok((pdf.worker.export(priority::INTERACTIVE, &pages)?, pages.len() as u32))
    }
}

/// A link on a PDF page: its box (page points, top-left origin) and where it goes: a `page` of
/// the same PDF (0-based) or a `uri`.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiPdfLink {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub page: Option<u32>,
    pub uri: Option<String>,
}

#[uniffi::export]
impl PdfService {
    /// The links on a page of a stored PDF (empty when it has none or can't be read).
    pub fn page_links(&self, blob: String, page: u32) -> Vec<FfiPdfLink> {
        let links = self.worker.call(priority::INTERACTIVE, move |d| d.get(&blob)?.page_links(page)).ok().and_then(|r| r.ok()).unwrap_or_default();
        links
            .into_iter()
            .map(|l| {
                let (page, uri) = match l.target {
                    n0te_pdf::LinkTarget::Page(p) => (Some(p), None),
                    n0te_pdf::LinkTarget::Uri(u) => (None, Some(u)),
                };
                FfiPdfLink { x0: l.rect.x0, y0: l.rect.y0, x1: l.rect.x1, y1: l.rect.y1, page, uri }
            })
            .collect()
    }
}

#[uniffi::export]
impl NotebookHandle {
    /// Imports a PDF: stores it as a blob and adds one sheet per page at `index` under `parent`
    /// (None = main column, else a group node), sized like the page. With `chapters` > 0 its
    /// table of contents becomes groups that many levels deep (a chapter of one page: the page's
    /// title). Save the notebook after.
    pub fn import_pdf(&self, pdf: Arc<PdfService>, bytes: Vec<u8>, parent: Option<String>, index: u32, chapters: u32) -> R<Vec<FfiImportedSheet>> {
        // The pdfium part runs without the notebook lock.
        let stored = n0te_pdf::notebook::store_pdf(&pdf.lib.inner, &pdf.worker, bytes)?;
        let key = stored.blob.clone();
        let outline = if chapters == 0 { Vec::new() } else { pdf.worker.call(priority::INTERACTIVE, move |d| d.get(&key).map(|d| d.outline())).ok().and_then(|r| r.ok()).unwrap_or_default() };
        let parts = n0te_pdf::notebook::chapters(&outline, stored.pages.len() as u32, chapters.min(4) as usize);
        let sheets = n0te_pdf::notebook::add_pdf_parts(&lock(&self.nb), &stored, &parts, parent.as_deref(), index as usize)?;
        Ok(sheets
            .into_iter()
            .map(|s| FfiImportedSheet { node: s.node, sheet_id: s.sheet_id, page: s.page, width: s.size.width, height: s.size.height })
            .collect())
    }

    /// Exports all sheets (`sheet_ids` = null) or the given ones as a PDF, in reading order
    /// (side sheets next to their main sheet: left ones, the sheet, right ones; with
    /// `sides_at_end` all main sheets first, then the side sheets in that order). PDF sheets keep
    /// their page, other sheets become pages of their size, boards a page around their ink.
    /// Open sheets are exported with their unsaved strokes.
    pub fn export_pdf(&self, pdf: Arc<PdfService>, sheet_ids: Option<Vec<String>>, sides_at_end: bool) -> R<Vec<u8>> {
        Ok(self.export_bytes(&pdf, sheet_ids, sides_at_end)?.0)
    }

    /// Like `export_pdf`, written to `path` (atomically). Returns the number of pages.
    pub fn export_pdf_to_file(&self, pdf: Arc<PdfService>, sheet_ids: Option<Vec<String>>, sides_at_end: bool, path: String) -> R<u32> {
        let (bytes, pages) = self.export_bytes(&pdf, sheet_ids, sides_at_end)?;
        n0te_core::store::write_atomic(std::path::Path::new(&path), &bytes)?;
        Ok(pages)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc::{Sender, channel};
    use std::time::Duration;

    use n0te_pdf::pdfium_render::prelude::*;

    use super::*;
    use crate::{FfiBrush, FfiOutlineRow, pack};

    struct ChannelSink(Mutex<Sender<FfiTile>>);
    impl TileSink for ChannelSink {
        fn on_tile(&self, tile: FfiTile) {
            let _ = lock(&self.0).send(tile);
        }
    }

    fn pdfium() -> String {
        std::env::var("N0TE_PDFIUM_LIB").unwrap_or_else(|_| format!("{}/../../third_party/pdfium/linux-x64/lib/libpdfium.so", env!("CARGO_MANIFEST_DIR")))
    }

    fn two_pages(e: &PdfEngine) -> Vec<u8> {
        e.with_raw(|p| {
            let mut doc = p.create_new_pdf().unwrap();
            let font = doc.fonts_mut().helvetica();
            for i in 0..2 {
                let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::a4()).unwrap();
                page.objects_mut().create_text_object(PdfPoints::new(72.0), PdfPoints::new(742.0), &format!("Folie {i} Satz"), font, PdfPoints::new(24.0)).unwrap();
            }
            doc.save_to_bytes().unwrap()
        })
    }

    /// All PDF FFI paths in one test: the service is one per process.
    #[test]
    fn import_tiles_search_export() {
        let dir = std::env::temp_dir().join(format!("n0te-ffi-pdf-{}", n0te_core::ids::new_id(5)));
        let lib = NoteLibrary::open(dir.to_string_lossy().into_owned()).unwrap();
        let (tx, rx) = channel();
        let pdf = PdfService::start(lib.clone(), pdfium(), vec![], Arc::new(ChannelSink(Mutex::new(tx)))).unwrap();
        assert!(PdfService::start(lib.clone(), pdfium(), vec![], Arc::new(ChannelSink(Mutex::new(channel().0)))).is_err(), "one per process");

        let nb = lib.clone().create_notebook("Physik".into()).unwrap();
        let group = nb.add_group(None, 0, "VL 1".into()).unwrap();
        let bytes = two_pages(&PdfEngine::load(pdfium()).unwrap());
        let sheets = nb.import_pdf(pdf.clone(), bytes, Some(group), 0, 2).unwrap();
        assert_eq!(sheets.len(), 2);
        assert_eq!((sheets[0].width.round(), sheets[0].height.round(), sheets[1].page), (595.0, 842.0, 1)); // A4
        let FfiOutlineRow::Sheet { spec, .. } = &nb.outline(true)[1] else { panic!() };
        let blob = spec.pdf_blob.clone().unwrap();
        assert_eq!(pdf.page_sizes(blob.clone()).unwrap().len(), 2);
        assert!(nb.import_pdf(pdf.clone(), b"not a pdf".to_vec(), None, 0, 0).is_err());

        assert_eq!(pdf_page_pixels(595.0, 842.0, 2.0), vec![1190, 1684]);
        let req = FfiTileRequest { blob: blob.clone(), page: 1, scale: 2.0, x: 0, y: 0, width: 256, height: 256, priority: 0, view: 0, generation: 1 };
        pdf.request_tiles(vec![req.clone(), FfiTileRequest { page: 9, ..req.clone() }]);
        let mut tiles: Vec<FfiTile> = (0..2).map(|_| rx.recv_timeout(Duration::from_secs(10)).unwrap()).collect();
        tiles.sort_by_key(|t| t.request.page);
        assert_eq!(tiles[0].rgba.len(), 256 * 256 * 4);
        assert!(tiles[0].error.is_none() && tiles[1].error.is_some() && tiles[1].rgba.is_empty());

        assert_eq!(pdf.page_text(blob.clone(), 1).unwrap(), "Folie 1 Satz");
        let hits = pdf.find_text(blob.clone(), 0, "SATZ".into()).unwrap();
        assert_eq!(hits.len(), 1);
        assert!(hits[0].boxes[0].y0 > 70.0 && hits[0].boxes[0].y1 < 110.0, "{:?}", hits[0].boxes);

        // Ink on slide 2 that is not saved yet: the export still has it.
        let sheet = nb.clone().open_sheet(sheets[1].sheet_id.clone()).unwrap();
        let samples: Vec<f32> = (0..=40).flat_map(|i| [100.0 + i as f32 * 7.5, 300.0, 0.5, i as f32 * 8.0]).collect();
        sheet.add_stroke(FfiBrush { width: 6.0, thinning: 0.0, color: 0xFFD7_1921, kind: 0 }, pack(&samples)).unwrap();
        let out = nb.export_pdf(pdf.clone(), Some(vec![sheets[1].sheet_id.clone()]), false).unwrap();
        let doc = PdfEngine::load(pdfium()).unwrap().open(out).unwrap();
        assert_eq!(doc.page_count(), 1);
        let img = doc.render_page(0, 595).unwrap();
        let i = ((300 * img.width + 250) * 4) as usize;
        assert_eq!(&img.data[i..i + 4], &[0xD7, 0x19, 0x21, 255]);

        let path = dir.join("out.pdf");
        assert_eq!(nb.export_pdf_to_file(pdf.clone(), None, true, path.to_string_lossy().into_owned()).unwrap(), 2);
        assert!(std::fs::read(&path).unwrap().starts_with(b"%PDF"));
        assert!(nb.export_pdf(pdf.clone(), Some(vec!["unknown".into()]), false).is_err());

        drop((pdf, sheet, doc));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
