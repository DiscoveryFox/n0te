//! n0te-pdf: PDF pages for n0te — open, render (whole pages and tiles), extract text with
//! positions, and export selected pages plus blank pages with vector ink on top.
//!
//! - [`PdfEngine`] / [`PdfDoc`]: pdfium, loaded from an explicit path.
//! - [`add_ink`] ([`ink_writer`]): n0te's own writer that appends ink to a PDF as an incremental
//!   update (no pdfium involved); [`PdfEngine::export`] = pdfium assembles the pages, then this.
//! - [`PdfWorker`] ([`worker`]): the one thread an app does its pdfium work on — tiles in
//!   priority order with per-view generations, an LRU of open documents, other jobs.
//! - [`notebook`]: import a PDF into a notebook, turn a notebook's sheets into export pages.
//!
//! Built on pdfium through `pdfium-render` (dynamic binding). The apps ship `libpdfium.so`
//! themselves (`scripts/fetch-pdfium.sh`) and pass its path to [`PdfEngine::load`].
//!
//! # Coordinates
//! Page coordinates are PDF points (1/72 inch) with the origin at the top-left corner of the page
//! *as displayed* (crop box, after `/Rotate`), x to the right and y down. That is the space of
//! n0te ink ([`n0te_core::ink`]) and of rendered bitmaps (pixel = point × scale).
//!
//! # Threads
//! pdfium is not thread-safe: it has process-global state (font mapper and caches, page and
//! document caches, the last-error value) and must never run on two threads at once. Two layers
//! keep it safe:
//! 1. `pdfium-render`'s `thread_safe` feature takes a global mutex around every single FFI call.
//! 2. This crate takes its own global lock around every public operation (open, render, text,
//!    export, close), so multi-call sequences (load page → render → close, load → read last
//!    error) never interleave between threads.
//!
//! All types are `Send + Sync`, so an `Arc<PdfDoc>` can be used from any thread, but there is no
//! parallel speed-up: all PDF work in one process is serialized. Apps do their PDF work on one
//! [`PdfWorker`] so it runs in priority order, never on the UI thread. [`add_ink`] does not use
//! pdfium and runs on any thread without the lock.

mod export;
pub mod ink_writer;
mod links;
pub mod notebook;
mod render;
mod syntax;
mod text;
pub mod text_layout;
pub mod worker;

use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock, PoisonError};

pub use n0te_core::ink::Rect;
/// The underlying binding, for features this crate does not wrap yet. Use it only inside
/// [`PdfEngine::with_raw`], which holds the crate lock.
pub use pdfium_render;
use pdfium_render::prelude::{PdfDocument, PdfPage, PdfRenderConfig, Pdfium, PdfiumError, PdfiumLibraryConfig};

pub use export::{ExportPage, PageSource, Stroke};
pub use ink_writer::{INK_LAYER_NAME, Paper, PdfImage, add_ink, add_ink_and_paper, add_ink_paper_text, add_ink_paper_text_images, add_ink_paper_text_images_gaps, hide_ink, jpeg_info};
pub use text_layout::PdfText;
pub use render::{Rgba, Tile, page_pixels, tiles};
pub use links::{LinkTarget, OutlineEntry, PdfLink};
pub use text::{PageText, TextChar};
pub use worker::{Docs, ExportSheet, PdfWorker, SheetSource, TileRequest, TileResult, WorkerOptions};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("pdfium: {0}")]
    Pdfium(#[from] PdfiumError),
    #[error("pdfium is already loaded from {0}")]
    AlreadyLoaded(PathBuf),
    #[error("page {page} out of range (document has {count})")]
    PageOutOfRange { page: u32, count: u32 },
    #[error("{0}")]
    Invalid(String),
    #[error("PDF worker: {0}")]
    Worker(String),
    #[error(transparent)]
    Core(#[from] n0te_core::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Width and height of a page in points, as displayed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageSize {
    pub width: f32,
    pub height: f32,
}

struct Engine {
    pdfium: Pdfium,
    library: PathBuf,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();
/// Serializes every public operation; see the crate docs.
static LOCK: Mutex<()> = Mutex::new(());

fn lock() -> MutexGuard<'static, ()> {
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The loaded pdfium library. pdfium can be bound once per process; later calls to
/// [`PdfEngine::load`] with the same path return the same engine.
#[derive(Clone, Copy)]
pub struct PdfEngine {
    e: &'static Engine,
}

impl PdfEngine {
    /// Loads `libpdfium.so` from an explicit path.
    pub fn load(library: impl AsRef<Path>) -> Result<Self> {
        Self::load_with_font_dirs(library, &[])
    }

    /// Like [`PdfEngine::load`], with extra directories pdfium searches for fonts a PDF does not
    /// embed (on Android: `/system/fonts`). Only the first load in a process applies them.
    pub fn load_with_font_dirs(library: impl AsRef<Path>, font_dirs: &[&str]) -> Result<Self> {
        let library = library.as_ref();
        let _g = lock();
        if let Some(e) = ENGINE.get() {
            return if same_file(&e.library, library) { Ok(PdfEngine { e }) } else { Err(Error::AlreadyLoaded(e.library.clone())) };
        }
        let bindings = Pdfium::bind_to_library(library)?;
        // pdfium-render's config defaults to the Skia renderer; the prebuilt library uses AGG.
        let mut config = PdfiumLibraryConfig::new().set_renderer_anti_grain_geometry();
        if !font_dirs.is_empty() {
            config = config.set_user_font_paths(font_dirs)?;
        }
        let pdfium = Pdfium::new_with_config(bindings, config);
        let _ = ENGINE.set(Engine { pdfium, library: library.to_path_buf() });
        Ok(PdfEngine { e: ENGINE.get().expect("engine was just set") })
    }

    /// Opens a PDF from memory. The document keeps the bytes.
    pub fn open(&self, bytes: Vec<u8>) -> Result<PdfDoc> {
        self.open_with_password(bytes, None)
    }

    pub fn open_with_password(&self, bytes: Vec<u8>, password: Option<&str>) -> Result<PdfDoc> {
        let _g = lock();
        let doc = self.e.pdfium.load_pdf_from_byte_vec(bytes, password)?;
        Ok(PdfDoc { cache: Mutex::new(Vec::new()), doc: ManuallyDrop::new(doc), engine: *self })
    }

    /// Runs `f` with the raw `pdfium-render` handle while holding the crate lock. `f` must not
    /// call into this crate (or drop a [`PdfDoc`]): the lock is not re-entrant.
    pub fn with_raw<R>(&self, f: impl FnOnce(&'static Pdfium) -> R) -> R {
        let _g = lock();
        f(&self.e.pdfium)
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(x), Ok(y)) if x == y)
}

/// Pages kept loaded per document, so the tiles of one page parse its content only once.
const PAGE_CACHE: usize = 4;

/// An open PDF document.
pub struct PdfDoc {
    // Field order matters: cached pages must be closed before the document.
    cache: Mutex<Vec<(u32, PdfPage<'static>)>>,
    doc: ManuallyDrop<PdfDocument<'static>>,
    engine: PdfEngine,
}

impl Drop for PdfDoc {
    fn drop(&mut self) {
        let _g = lock();
        self.cache.get_mut().unwrap_or_else(PoisonError::into_inner).clear();
        // SAFETY: dropped exactly once, here; nothing uses `doc` afterwards.
        unsafe { ManuallyDrop::drop(&mut self.doc) };
    }
}

impl PdfDoc {
    pub fn engine(&self) -> PdfEngine {
        self.engine
    }

    pub fn page_count(&self) -> u32 {
        let _g = lock();
        self.doc.pages().len() as u32
    }

    /// Size of one page in points (does not load the page's content).
    pub fn page_size(&self, page: u32) -> Result<PageSize> {
        let _g = lock();
        self.size_locked(page)
    }

    /// Sizes of all pages in points.
    pub fn page_sizes(&self) -> Result<Vec<PageSize>> {
        let _g = lock();
        (0..self.doc.pages().len() as u32).map(|i| self.size_locked(i)).collect()
    }

    fn size_locked(&self, page: u32) -> Result<PageSize> {
        self.check_page(page)?;
        let r = self.doc.pages().page_size(page as i32)?;
        Ok(PageSize { width: r.width().value, height: r.height().value })
    }

    fn check_page(&self, page: u32) -> Result<()> {
        let count = self.doc.pages().len() as u32;
        if page < count { Ok(()) } else { Err(Error::PageOutOfRange { page, count }) }
    }

    /// Runs `f` on a loaded page, loading it into the small per-document cache if needed.
    /// The caller holds the crate lock.
    fn with_page<R>(&self, page: u32, f: impl FnOnce(&PdfPage<'static>) -> Result<R>) -> Result<R> {
        self.check_page(page)?;
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        let entry = match cache.iter().position(|(i, _)| *i == page) {
            Some(pos) => cache.remove(pos),
            None => (page, self.doc.pages().get(page as i32)?),
        };
        cache.push(entry);
        if cache.len() > PAGE_CACHE {
            cache.remove(0);
        }
        f(&cache.last().expect("just pushed").1)
    }

    /// Closes the cached pages (frees their parsed content).
    pub fn trim(&self) {
        let _g = lock();
        self.cache.lock().unwrap_or_else(PoisonError::into_inner).clear();
    }
}

/// Affine map `(x, y) -> (a·x + c·y + e, b·x + d·y + f)`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Affine {
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
}

impl Affine {
    pub(crate) fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (self.a * x + self.c * y + self.e, self.b * x + self.d * y + self.f)
    }

    pub(crate) fn invert(&self) -> Affine {
        let det = self.a * self.d - self.b * self.c;
        let (a, b, c, d) = (self.d / det, -self.b / det, -self.c / det, self.a / det);
        Affine { a, b, c, d, e: -(a * self.e + c * self.f), f: -(b * self.e + d * self.f) }
    }

    /// Bounding box of a rectangle after mapping its corners.
    pub(crate) fn map_rect(&self, x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
        [(x0, y0), (x1, y0), (x0, y1), (x1, y1)]
            .into_iter()
            .map(|(x, y)| self.apply(x, y))
            .fold(Rect::EMPTY, |r, (x, y)| r.include(x as f32, y as f32))
    }
}

/// Map from page coordinates (see crate docs) to PDF user space, taken from pdfium's own
/// display matrix so it matches rendering exactly (crop box offset, `/Rotate`).
pub(crate) fn display_to_user(page: &PdfPage) -> Result<Affine> {
    const D: i32 = 1 << 20; // large device size: integer device coordinates lose no precision
    let (w, h) = (page.width().value as f64, page.height().value as f64);
    let cfg = PdfRenderConfig::new().set_fixed_size(D, D);
    let p = |x, y| -> Result<(f64, f64)> {
        let (px, py) = page.pixels_to_points(x, y, &cfg)?;
        Ok((px.value as f64, py.value as f64))
    };
    let (p0, p1, p2) = (p(0, 0)?, p(D, 0)?, p(0, D)?);
    Ok(Affine { a: (p1.0 - p0.0) / w, b: (p1.1 - p0.1) / w, c: (p2.0 - p0.0) / h, d: (p2.1 - p0.1) / h, e: p0.0, f: p0.1 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn affine_inverse_round_trips() {
        let m = Affine { a: 0.0, b: -1.0, c: 1.0, d: 0.0, e: 10.0, f: 800.0 };
        let (x, y) = m.apply(12.5, 40.0);
        let (bx, by) = m.invert().apply(x, y);
        assert!((bx - 12.5).abs() < 1e-9 && (by - 40.0).abs() < 1e-9);
    }

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn handles_are_send_and_sync() {
        assert_send_sync::<PdfEngine>();
        assert_send_sync::<PdfDoc>();
    }
}
