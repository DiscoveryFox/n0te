//! PDF on the laptop: pdfium (shipped with the app), the core's PDF worker, page tiles cached
//! as images (thumbnail + power-of-two levels, like the tablet), import and export.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use iced::Subscription;
use iced::advanced::image::Handle;
use iced::futures::StreamExt;
use iced::futures::channel::mpsc;
use iced::futures::stream::{self, BoxStream};

use n0te_core::notebook::{Notebook, OutlineRow};
use n0te_core::store::Library;
use n0te_pdf::worker::priority;
use n0te_pdf::{PageSize, PdfEngine, PdfWorker, Tile, TileRequest, TileResult, WorkerOptions, page_pixels};

/// Scale of the thumbnail level (always requested, so a fast scroll never shows white).
pub const THUMB: f32 = 0.25;
pub const TILE: u32 = 512;
/// Tile cache budget.
const BUDGET: usize = 256 * 1024 * 1024;
const VIEW: u32 = 1;

/// (blob, page, scale bits, tile x, tile y)
pub type Key = (String, u32, u32, u32, u32);

#[derive(Default)]
pub struct Tiles {
    map: HashMap<Key, (Handle, usize)>,
    order: VecDeque<Key>,
    bytes: usize,
    pending: HashSet<Key>,
}

impl Tiles {
    fn insert(&mut self, k: Key, h: Handle, size: usize) {
        self.pending.remove(&k);
        if self.map.insert(k.clone(), (h, size)).is_none() {
            self.order.push_back(k);
            self.bytes += size;
        }
        while self.bytes > BUDGET {
            let Some(old) = self.order.pop_front() else { break };
            if let Some((_, s)) = self.map.remove(&old) {
                self.bytes -= s;
            }
        }
    }
    pub fn get(&self, k: &Key) -> Option<Handle> {
        self.map.get(k).map(|(h, _)| h.clone())
    }
}

pub struct Pdf {
    pub worker: PdfWorker,
    pub tiles: Arc<Mutex<Tiles>>,
    /// Pages' lines with their boxes, for search (pdfium's text extraction isn't free; blobs
    /// never change, so they stay valid).
    lines: Mutex<HashMap<(String, u32), Vec<(String, n0te_pdf::Rect)>>>,
}

static READY: OnceLock<Mutex<Option<mpsc::UnboundedReceiver<()>>>> = OnceLock::new();

/// pdfium's file name on this system.
const PDFIUM: &str = if cfg!(windows) { "pdfium.dll" } else { "libpdfium.so" };

/// Where pdfium is: `N0TE_PDFIUM_LIB`, next to the installed app, or the repo's copy.
/// Windows: `pdfium.dll` next to the program (the installer puts it there), else
/// `N0TE_PDFIUM_LIB`.
pub fn find_pdfium() -> Option<PathBuf> {
    let mut c: Vec<PathBuf> = Vec::new();
    let exe_dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf));
    #[cfg(windows)]
    {
        if let Some(dir) = &exe_dir {
            c.push(dir.join(PDFIUM));
        }
        if let Some(p) = std::env::var_os("N0TE_PDFIUM_LIB") {
            c.push(p.into());
        }
    }
    #[cfg(not(windows))]
    {
        if let Some(p) = std::env::var_os("N0TE_PDFIUM_LIB") {
            c.push(p.into());
        }
        if let Some(dir) = &exe_dir {
            c.push(dir.join("../lib/n0te/libpdfium.so"));
        }
        if let Some(h) = std::env::var_os("HOME") {
            c.push(PathBuf::from(h).join(".local/lib/n0te/libpdfium.so"));
        }
        c.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../third_party/pdfium/linux-x64/lib/libpdfium.so"));
    }
    c.into_iter().find(|p| p.is_file())
}

impl Pdf {
    /// Loads pdfium and starts the worker; documents are the library's blobs.
    pub fn start(library_root: &Path) -> Result<Arc<Pdf>, String> {
        let lib_path = find_pdfium().ok_or(format!("{PDFIUM} nicht gefunden"))?;
        // Fonts a PDF doesn't embed: pdfium on Windows asks the system itself.
        let font_dirs: &[&str] = if cfg!(windows) { &[] } else { &["/usr/share/fonts"] };
        let engine = PdfEngine::load_with_font_dirs(&lib_path, font_dirs).map_err(|e| e.to_string())?;
        let lib = Library::open(library_root).map_err(|e| e.to_string())?;
        let tiles = Arc::new(Mutex::new(Tiles::default()));
        let (tx, rx) = mpsc::unbounded();
        let _ = READY.get_or_init(|| Mutex::new(None)).lock().map(|mut r| *r = Some(rx));
        let sink_tiles = tiles.clone();
        let worker = PdfWorker::start(
            engine,
            move |blob| Ok(lib.blob(blob)?),
            move |r: TileResult| {
                let q = &r.request;
                let k = (q.doc.clone(), q.page, q.scale.to_bits(), q.tile.x / TILE, q.tile.y / TILE);
                let mut t = sink_tiles.lock().unwrap_or_else(|p| p.into_inner());
                match r.image {
                    Ok(img) => {
                        let size = img.data.len();
                        t.insert(k, Handle::from_rgba(img.width, img.height, img.data), size);
                    }
                    Err(_) => {
                        t.pending.remove(&k);
                    }
                }
                drop(t);
                let _ = tx.unbounded_send(());
            },
            WorkerOptions::default(),
        );
        Ok(Arc::new(Pdf { worker, tiles, lines: Mutex::default() }))
    }

    /// A page's lines with their boxes (page points of the PDF as printed, top-left origin),
    /// cached. Blocks until the worker has read them: call it off the UI thread.
    pub fn page_lines(&self, blob: &str, page: u32) -> Vec<(String, n0te_pdf::Rect)> {
        let key = (blob.to_string(), page);
        if let Some(l) = self.lines.lock().unwrap_or_else(|p| p.into_inner()).get(&key) {
            return l.clone();
        }
        let b = blob.to_string();
        // Kept only once read (a page that failed is tried again next time).
        let Some(lines) = self.worker.call(priority::INTERACTIVE, move |d| d.get(&b)?.page_text(page)).ok().and_then(|r| r.ok()).map(|t| t.lines()) else { return Vec::new() };
        self.lines.lock().unwrap_or_else(|p| p.into_inner()).insert(key, lines.clone());
        lines
    }

    /// Fires whenever a tile arrived (the UI redraws).
    pub fn subscription() -> Subscription<()> {
        Subscription::run(ready_stream)
    }

    /// The tiles of `blob` page `page` (`size` points) at `scale` that intersect `part` (page
    /// points, page-relative): (tile in page points, image if cached). Missing ones are requested.
    pub fn tiles_for(&self, blob: &str, page: u32, size: PageSize, scale: f32, part: n0te_core::ink::Rect, prio: u32) -> Vec<(n0te_core::ink::Rect, Option<Handle>)> {
        let (pw, ph) = page_pixels(size, scale);
        let x0 = ((part.x0 * scale).max(0.0) as u32).min(pw - 1) / TILE;
        let y0 = ((part.y0 * scale).max(0.0) as u32).min(ph - 1) / TILE;
        let x1 = (((part.x1 * scale).ceil().max(1.0) as u32) - 1).min(pw - 1) / TILE;
        let y1 = (((part.y1 * scale).ceil().max(1.0) as u32) - 1).min(ph - 1) / TILE;
        let mut out = Vec::new();
        let mut missing = Vec::new();
        let mut t = self.tiles.lock().unwrap_or_else(|p| p.into_inner());
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                let (x, y) = (tx * TILE, ty * TILE);
                let (w, h) = (TILE.min(pw - x), TILE.min(ph - y));
                let k = (blob.to_string(), page, scale.to_bits(), tx, ty);
                let rect = n0te_core::ink::Rect::new(x as f32 / scale, y as f32 / scale, (x + w) as f32 / scale, (y + h) as f32 / scale);
                let img = t.get(&k);
                if img.is_none() && t.pending.insert(k) {
                    missing.push(TileRequest { doc: blob.to_string(), page, scale, tile: Tile { x, y, width: w, height: h }, priority: prio, view: VIEW, generation: 0 });
                }
                out.push((rect, img));
            }
        }
        drop(t);
        if !missing.is_empty() {
            self.worker.request(missing);
        }
        out
    }

    /// Imports a PDF into `nb` (one sheet per page) at `index` under `parent`; saves the notebook.
    pub fn import(&self, lib: &Library, nb: &Notebook, bytes: Vec<u8>, parent: Option<&str>, index: usize) -> Result<usize, String> {
        let sheets = n0te_pdf::notebook::import_pdf(lib, nb, &self.worker, bytes, parent, index).map_err(|e| e.to_string())?;
        lib.save_notebook(nb).map_err(|e| e.to_string())?;
        Ok(sheets.len())
    }

    /// Exports all sheets (`None`) or the given ones as a PDF; returns bytes and page count.
    pub fn export(&self, lib: &Library, nb: &Notebook, selected: Option<&[String]>) -> Result<(Vec<u8>, usize), String> {
        let id = nb.id();
        let specs = nb.sheet_specs();
        let pages = n0te_pdf::notebook::export_sheets_with_images(
            nb,
            selected,
            |sheet| {
                let s = lib.load_sheet(&id, sheet)?;
                // Coordinate systems go in as ink under the handwriting (the same look as on screen).
                let mut ink = s.plot_strokes();
                ink.extend(s.strokes());
                Ok((ink, s.text_boxes_with_body(specs.get(sheet).map(|s| s.width).unwrap_or(595.0)), s.images()))
            },
            |blob| lib.blob(blob).ok(),
        )
        .map_err(|e| e.to_string())?;
        if pages.is_empty() {
            return Err("nichts zu exportieren".into());
        }
        let bytes = self.worker.export(priority::INTERACTIVE, &pages).map_err(|e| e.to_string())?;
        Ok((bytes, pages.len()))
    }
}

fn ready_stream() -> BoxStream<'static, ()> {
    match READY.get().and_then(|m| m.lock().ok().and_then(|mut r| r.take())) {
        Some(rx) => rx.boxed(),
        None => stream::pending().boxed(),
    }
}

/// Smallest power of two at or above `zoom` (pixels per point, a bit oversampled for HiDPI).
pub fn bucket(zoom: f32) -> f32 {
    let want = zoom * 1.25;
    let mut b = THUMB;
    while b < want && b < 8.0 {
        b *= 2.0;
    }
    b
}

/// Where a new page goes after main sheet `node`: (parent group, index), like the tablet.
pub fn position_after(nb: &Notebook, node: Option<&str>) -> (Option<String>, usize) {
    let rows = nb.outline(true);
    let Some(node) = node else {
        let n = rows.iter().filter(|r| depth(r) == 0).count();
        return (None, n);
    };
    let mut stack: Vec<String> = Vec::new();
    let mut counts: HashMap<Option<String>, usize> = HashMap::new();
    for r in &rows {
        let d = depth(r) as usize;
        stack.truncate(d);
        let parent = stack.last().cloned();
        let i = *counts.entry(parent.clone()).or_default();
        counts.insert(parent.clone(), i + 1);
        match r {
            OutlineRow::Sheet { node: n, .. } if n == node => return (parent, i + 1),
            OutlineRow::Group { node: g, .. } => stack.push(g.clone()),
            _ => {}
        }
    }
    (None, rows.iter().filter(|r| depth(r) == 0).count())
}

fn depth(r: &OutlineRow) -> u32 {
    match r {
        OutlineRow::Group { depth, .. } | OutlineRow::Sheet { depth, .. } => *depth,
    }
}

/// Recent PDFs in Downloads and Documents (newest first), for the import list.
pub fn recent_pdfs() -> Vec<(PathBuf, std::time::SystemTime)> {
    use n0te_platform::paths;
    #[cfg(not(windows))]
    let dirs: Vec<PathBuf> = {
        let home = paths::home().unwrap_or_default();
        ["Downloads", "Downloads/n0bridge", "Documents", "Dokumente", "Desktop"].iter().map(|d| home.join(d)).collect()
    };
    // Windows: the known folders (Documents and Desktop often live in OneDrive).
    #[cfg(windows)]
    let dirs: Vec<PathBuf> = [paths::downloads(), paths::documents(), paths::desktop()].into_iter().flatten().collect();
    let mut out = Vec::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("pdf"))
                && let Ok(m) = e.metadata().and_then(|m| m.modified())
            {
                out.push((p, m));
            }
        }
    }
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out.truncate(40);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use n0te_core::notebook::{Background, SheetSpec};

    #[test]
    fn buckets_are_powers_of_two() {
        assert_eq!(bucket(0.1), THUMB);
        assert_eq!(bucket(1.0), 2.0);
        assert_eq!(bucket(0.7), 1.0);
        assert_eq!(bucket(100.0), 8.0);
    }

    #[test]
    fn new_pages_go_right_after_the_current_one_inside_its_group() {
        let nb = Notebook::create(1, "p").unwrap();
        let (a, _) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Blank)).unwrap();
        let g = nb.add_group(None, 1, "G").unwrap();
        let (b, _) = nb.add_sheet(Some(&g), 0, &SheetSpec::a4(Background::Blank)).unwrap();
        nb.add_sheet(Some(&g), 1, &SheetSpec::a4(Background::Blank)).unwrap();
        assert_eq!(position_after(&nb, Some(&a)), (None, 1));
        assert_eq!(position_after(&nb, Some(&b)), (Some(g.clone()), 1));
        assert_eq!(position_after(&nb, None), (None, 2));
    }
}
