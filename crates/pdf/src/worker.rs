//! The PDF worker: one thread that does an app's pdfium work, in priority order.
//!
//! pdfium is not thread-safe, so all PDF work in a process is serialized anyway (see the crate
//! docs). The worker makes that serialization useful: requests wait in one priority queue, so
//! visible tiles are rendered before prefetch tiles and before background work, and requests
//! the user scrolled away from are dropped before they cost anything.
//!
//! # Tiles
//! A [`TileRequest`] names a document (an app-chosen key; n0te uses the blob id), a page, a
//! scale (pixels per point), a tile of the page at that scale ([`crate::tiles`]), a priority
//! (lower runs first: [`priority::VISIBLE`] before [`priority::PREFETCH`]), a view and a
//! generation. Every rendered tile goes to the sink given to [`PdfWorker::start`], on the worker
//! thread.
//!
//! Generations are per view (e.g. the main page view and a thumbnail strip, which must not
//! cancel each other): a request with a newer generation drops every queued tile of the same
//! view with an older one, and requests older than the newest seen are ignored. An app bumps the
//! generation whenever its viewport changes and requests what is still missing; dropped tiles
//! get no callback.
//!
//! Requests for a tile that is already queued replace the queued one (its priority, view and
//! generation are updated); a request for the tile being rendered right now is merged into it
//! (the result carries the newest request). So re-requesting everything that is missing on each
//! viewport change renders each tile once.
//!
//! # Documents
//! Open documents are kept in a small LRU keyed by the document key; a miss loads the bytes
//! through the loader given to [`PdfWorker::start`] (n0te: the blob store).
//!
//! # Other work
//! [`PdfWorker::call`] runs any closure on the worker (text search, page sizes, the pdfium half
//! of an export) with the open documents, ordered by its priority like tiles.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::{JoinHandle, ThreadId};

use n0te_core::ink::{Brush, InkPoint};

use crate::{Error, PageSource, PdfDoc, PdfEngine, Result, Rgba, Stroke, Tile, add_ink};

/// Priorities used by n0te (lower runs first). Any `u32` works.
pub mod priority {
    /// Tiles on screen.
    pub const VISIBLE: u32 = 0;
    /// Work the user is waiting for: import, export, search.
    pub const INTERACTIVE: u32 = 10;
    /// Tiles just outside the viewport.
    pub const PREFETCH: u32 = 20;
    /// Thumbnails, indexing.
    pub const BACKGROUND: u32 = 30;
}

/// One tile to render.
#[derive(Debug, Clone, PartialEq)]
pub struct TileRequest {
    /// Document key (n0te: the PDF's blob id).
    pub doc: String,
    pub page: u32,
    /// Pixels per point; the page is `page_pixels(size, scale)` pixels.
    pub scale: f32,
    pub tile: Tile,
    /// Lower runs first.
    pub priority: u32,
    /// Generations are counted per view.
    pub view: u32,
    pub generation: u64,
}

impl TileRequest {
    fn key(&self) -> (&str, u32, u32, Tile) {
        (&self.doc, self.page, self.scale.to_bits(), self.tile)
    }
}

/// A rendered tile (or why it could not be rendered).
#[derive(Debug)]
pub struct TileResult {
    /// The newest request for this tile.
    pub request: TileRequest,
    pub image: Result<Rgba>,
}

/// Loads the bytes of a document by key.
pub type Loader = dyn Fn(&str) -> Result<Vec<u8>> + Send + Sync;
/// Receives rendered tiles, on the worker thread.
pub type Sink = dyn Fn(TileResult) + Send + Sync;

#[derive(Debug, Clone, Copy)]
pub struct WorkerOptions {
    /// Documents kept open (LRU).
    pub open_docs: usize,
}

impl Default for WorkerOptions {
    fn default() -> Self {
        WorkerOptions { open_docs: 4 }
    }
}

/// The documents open on the worker, available to jobs.
pub struct Docs {
    engine: PdfEngine,
    loader: Box<Loader>,
    /// Least recently used first.
    open: Vec<(String, Arc<PdfDoc>)>,
    capacity: usize,
}

impl Docs {
    pub fn engine(&self) -> PdfEngine {
        self.engine
    }

    /// The open document for `key`, loading it through the loader on a miss.
    pub fn get(&mut self, key: &str) -> Result<Arc<PdfDoc>> {
        if let Some(i) = self.open.iter().position(|(k, _)| k == key) {
            let entry = self.open.remove(i);
            let doc = entry.1.clone();
            self.open.push(entry);
            return Ok(doc);
        }
        let bytes = (self.loader)(key)?;
        self.insert(key, bytes)
    }

    /// Opens `bytes` as document `key` (replacing an open one) and keeps it in the LRU.
    pub fn insert(&mut self, key: &str, bytes: Vec<u8>) -> Result<Arc<PdfDoc>> {
        let doc = Arc::new(self.engine.open(bytes)?);
        self.forget(key);
        self.open.push((key.to_string(), doc.clone()));
        while self.open.len() > self.capacity.max(1) {
            self.open.remove(0);
        }
        Ok(doc)
    }

    /// Closes document `key` if it is open.
    pub fn forget(&mut self, key: &str) {
        self.open.retain(|(k, _)| k != key);
    }

    /// Keys of the open documents, least recently used first.
    pub fn open_keys(&self) -> Vec<String> {
        self.open.iter().map(|(k, _)| k.clone()).collect()
    }
}

type Job = Box<dyn FnOnce(&mut Docs) + Send>;

enum Work {
    Tile(TileRequest),
    Job(Job),
}

struct Queued {
    priority: u32,
    seq: u64,
    work: Work,
}

#[derive(Default)]
struct State {
    queue: Vec<Queued>,
    /// The tile being rendered: the newest request for it.
    in_flight: Option<TileRequest>,
    /// Newest generation seen per view.
    generations: HashMap<u32, u64>,
    seq: u64,
    stop: bool,
}

impl State {
    fn push(&mut self, priority: u32, work: Work) {
        self.seq += 1;
        self.queue.push(Queued { priority, seq: self.seq, work });
    }

    fn add_tile(&mut self, req: TileRequest) {
        let newest = self.generations.entry(req.view).or_insert(req.generation);
        if req.generation < *newest {
            return; // stale
        }
        if req.generation > *newest {
            *newest = req.generation;
            self.drop_older(req.view, req.generation);
        }
        if let Some(cur) = &mut self.in_flight
            && cur.key() == req.key()
        {
            *cur = req;
            return;
        }
        self.queue.retain(|q| !matches!(&q.work, Work::Tile(t) if t.key() == req.key()));
        self.push(req.priority, Work::Tile(req));
    }

    fn drop_older(&mut self, view: u32, generation: u64) {
        self.queue.retain(|q| !matches!(&q.work, Work::Tile(t) if t.view == view && t.generation < generation));
    }

    fn pop(&mut self) -> Option<Work> {
        let i = self.queue.iter().enumerate().min_by_key(|(_, q)| (q.priority, q.seq))?.0;
        let q = self.queue.remove(i);
        if let Work::Tile(t) = &q.work {
            self.in_flight = Some(t.clone());
        }
        Some(q.work)
    }
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// The worker thread and its queue. Dropping it stops the thread (queued work is dropped; a
/// blocked [`PdfWorker::call`] returns an error).
pub struct PdfWorker {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    thread_id: ThreadId,
}

impl PdfWorker {
    /// Starts the worker thread. `loader` provides document bytes by key, `sink` receives every
    /// rendered tile (on the worker thread; it may call back into the worker).
    pub fn start(
        engine: PdfEngine,
        loader: impl Fn(&str) -> Result<Vec<u8>> + Send + Sync + 'static,
        sink: impl Fn(TileResult) + Send + Sync + 'static,
        options: WorkerOptions,
    ) -> PdfWorker {
        let shared = Arc::new(Shared { state: Mutex::new(State::default()), wake: Condvar::new() });
        let docs = Docs { engine, loader: Box::new(loader), open: Vec::new(), capacity: options.open_docs };
        let s = shared.clone();
        let thread = std::thread::Builder::new()
            .name("n0te-pdf".into())
            .spawn(move || run(&s, docs, Box::new(sink)))
            .expect("spawn the PDF worker thread");
        let thread_id = thread.thread().id();
        PdfWorker { shared, thread: Some(thread), thread_id }
    }

    /// Queues tiles (see the module docs for priorities, generations and merging).
    pub fn request(&self, requests: impl IntoIterator<Item = TileRequest>) {
        let mut st = self.shared.lock();
        for r in requests {
            st.add_tile(r);
        }
        drop(st);
        self.shared.wake.notify_one();
    }

    /// Drops the queued tiles of `view` with a generation below `generation`, and ignores such
    /// requests from now on.
    pub fn cancel(&self, view: u32, generation: u64) {
        let mut st = self.shared.lock();
        let newest = st.generations.entry(view).or_insert(generation);
        *newest = (*newest).max(generation);
        st.drop_older(view, generation);
    }

    /// Number of queued requests and jobs (not counting the one running).
    pub fn queued(&self) -> usize {
        self.shared.lock().queue.len()
    }

    /// Queues `job` and returns at once.
    pub fn spawn(&self, priority: u32, job: impl FnOnce(&mut Docs) + Send + 'static) {
        self.shared.lock().push(priority, Work::Job(Box::new(job)));
        self.shared.wake.notify_one();
    }

    /// Runs `job` on the worker (after everything with a lower priority number) and waits for
    /// its result. Must not be called from the worker thread itself (e.g. from the tile sink).
    pub fn call<R: Send + 'static>(&self, priority: u32, job: impl FnOnce(&mut Docs) -> R + Send + 'static) -> Result<R> {
        if std::thread::current().id() == self.thread_id {
            return Err(Error::Worker("PdfWorker::call from the worker thread would wait for itself".into()));
        }
        let (tx, rx) = sync_channel(1);
        self.spawn(priority, move |docs| {
            let _ = tx.send(job(docs));
        });
        rx.recv().map_err(|_| Error::Worker("the job failed or the worker stopped".into()))
    }

    /// Builds a PDF from owned page descriptions: pdfium assembles the pages on the worker (the
    /// only part that needs pdfium), then the ink is added on the calling thread.
    pub fn export(&self, priority: u32, sheets: &[ExportSheet]) -> Result<Vec<u8>> {
        let sources: Vec<SheetSource> = sheets.iter().map(|s| s.source.clone()).collect();
        let bytes = self.call(priority, move |docs| -> Result<Vec<u8>> {
            // Keep every source open for the whole assembly, even beyond the LRU's capacity.
            let mut open: Vec<(&str, Arc<PdfDoc>)> = Vec::new();
            for s in &sources {
                if let SheetSource::Pdf { doc, .. } = s
                    && !open.iter().any(|(k, _)| k == doc)
                {
                    open.push((doc, docs.get(doc)?));
                }
            }
            let pages: Vec<PageSource> = sources
                .iter()
                .map(|s| match s {
                    SheetSource::Pdf { doc, page } => {
                        PageSource::Pdf { doc: &open.iter().find(|(k, _)| k == doc).expect("opened above").1, page: *page }
                    }
                    SheetSource::Blank { width, height } => PageSource::Blank { width: *width, height: *height },
                })
                .collect();
            docs.engine().assemble(&pages)
        })??;
        let strokes: Vec<Vec<Stroke>> = sheets.iter().map(|s| s.strokes.iter().map(|(p, b)| Stroke { points: p, brush: *b }).collect()).collect();
        let refs: Vec<&[Stroke]> = strokes.iter().map(Vec::as_slice).collect();
        add_ink(bytes, &refs)
    }
}

impl Drop for PdfWorker {
    fn drop(&mut self) {
        self.shared.lock().stop = true;
        self.shared.wake.notify_all();
        if let Some(t) = self.thread.take()
            && std::thread::current().id() != self.thread_id
        {
            let _ = t.join();
        }
    }
}

/// Where an exported page comes from, by document key.
#[derive(Debug, Clone, PartialEq)]
pub enum SheetSource {
    Pdf { doc: String, page: u32 },
    Blank { width: f32, height: f32 },
}

/// One page of [`PdfWorker::export`]: its source and its strokes in page coordinates.
#[derive(Debug, Clone)]
pub struct ExportSheet {
    pub source: SheetSource,
    pub strokes: Vec<(Vec<InkPoint>, Brush)>,
}

fn run(shared: &Shared, mut docs: Docs, sink: Box<Sink>) {
    loop {
        let work = {
            let mut st = shared.lock();
            loop {
                if st.stop {
                    st.queue.clear();
                    return;
                }
                if let Some(w) = st.pop() {
                    break w;
                }
                st = shared.wake.wait(st).unwrap_or_else(PoisonError::into_inner);
            }
        };
        match work {
            Work::Tile(req) => {
                let image = catch_unwind(AssertUnwindSafe(|| docs.get(&req.doc)?.render_tile(req.page, req.scale, req.tile)))
                    .unwrap_or_else(|_| Err(Error::Worker("panic while rendering".into())));
                let request = shared.lock().in_flight.take().unwrap_or(req);
                let _ = catch_unwind(AssertUnwindSafe(|| sink(TileResult { request, image })));
            }
            Work::Job(job) => {
                let _ = catch_unwind(AssertUnwindSafe(|| job(&mut docs)));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(doc: &str, x: u32, priority: u32, view: u32, generation: u64) -> TileRequest {
        TileRequest { doc: doc.into(), page: 0, scale: 1.0, tile: Tile { x, y: 0, width: 8, height: 8 }, priority, view, generation }
    }

    fn order(st: &mut State) -> Vec<(u32, u64)> {
        let mut out = Vec::new();
        while let Some(w) = st.pop() {
            if let Work::Tile(t) = w {
                out.push((t.tile.x, t.generation));
            }
            st.in_flight = None;
        }
        out
    }

    #[test]
    fn priority_then_fifo() {
        let mut st = State::default();
        st.add_tile(req("a", 1, priority::PREFETCH, 0, 0));
        st.add_tile(req("a", 2, priority::VISIBLE, 0, 0));
        st.add_tile(req("a", 3, priority::PREFETCH, 0, 0));
        st.add_tile(req("a", 4, priority::VISIBLE, 0, 0));
        assert_eq!(order(&mut st), vec![(2, 0), (4, 0), (1, 0), (3, 0)]);
    }

    #[test]
    fn newer_generation_drops_older_tiles_of_its_view_only() {
        let mut st = State::default();
        st.add_tile(req("a", 1, 0, 0, 1));
        st.add_tile(req("a", 2, 0, 1, 1)); // other view
        st.add_tile(req("a", 3, 0, 0, 2)); // drops x=1
        st.add_tile(req("a", 4, 0, 0, 1)); // stale: ignored
        assert_eq!(order(&mut st), vec![(2, 1), (3, 2)]);
    }

    #[test]
    fn duplicates_merge_into_queued_and_in_flight_tiles() {
        let mut st = State::default();
        st.add_tile(req("a", 1, priority::PREFETCH, 0, 1));
        st.add_tile(req("a", 2, priority::PREFETCH, 0, 1));
        st.add_tile(req("a", 2, priority::VISIBLE, 0, 1)); // replaces the queued one
        assert_eq!(st.queue.len(), 2);
        let Some(Work::Tile(t)) = st.pop() else { panic!() };
        assert_eq!((t.tile.x, t.priority), (2, priority::VISIBLE));
        // Same tile again while it renders: merged, not queued.
        st.add_tile(req("a", 2, priority::VISIBLE, 0, 3));
        assert_eq!(st.queue.len(), 0, "generation 3 also dropped the queued x=1 of generation 1");
        assert_eq!(st.in_flight.as_ref().unwrap().generation, 3);
        // A different document is a different tile.
        st.add_tile(req("b", 2, priority::VISIBLE, 0, 3));
        assert_eq!(st.queue.len(), 1);
    }
}
