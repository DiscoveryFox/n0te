//! Storing the sheets n0te Tablet keeps open, off the UI thread: a big page full of ink is a
//! file of many megabytes, and merging what is on disk, exporting and writing it took the pen a
//! noticeable moment after every stroke. The page view shows the edit from the sheet in memory
//! at once; the file follows here, one save at a time, in the order asked (a sheet asked for
//! again while it waits is stored once: the save takes it as it is then).
//!
//! The library's own lock keeps these saves and the UI thread's apart; [`flush`] waits for what
//! is left (the program does before it ends, tests before they read the files).

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, OnceLock};

use iced::Subscription;
use iced::futures::StreamExt;
use iced::futures::channel::mpsc;
use iced::futures::stream::BoxStream;
use n0te_core::sheet::Sheet;
use n0te_core::store::Library;

/// A sheet stored: (notebook, sheet, an error if it failed).
#[derive(Debug, Clone)]
pub struct Saved {
    pub notebook: String,
    pub sheet: String,
    pub error: Option<String>,
}

struct Job {
    root: PathBuf,
    notebook: String,
    sheet_id: String,
    sheet: Arc<Sheet>,
}

#[derive(Default)]
struct Queue {
    jobs: VecDeque<Job>,
    /// A save is running.
    busy: bool,
    /// Each notebook's change stamp right after our last save of it.
    own: HashMap<String, u64>,
    tx: Option<mpsc::UnboundedSender<Saved>>,
}

struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
}

fn shared() -> &'static Shared {
    static S: OnceLock<Shared> = OnceLock::new();
    S.get_or_init(|| {
        let s = Shared { queue: Mutex::new(Queue::default()), wake: Condvar::new() };
        std::thread::Builder::new().name("n0te-saver".into()).spawn(run).expect("saver thread");
        s
    })
}

fn lock() -> std::sync::MutexGuard<'static, Queue> {
    shared().queue.lock().unwrap_or_else(|p| p.into_inner())
}

/// Stores `sheet` of `notebook` in the library at `root` soon.
pub fn save(root: PathBuf, notebook: &str, sheet_id: &str, sheet: Arc<Sheet>) {
    let mut q = lock();
    if q.jobs.iter().any(|j| j.root == root && j.notebook == notebook && j.sheet_id == sheet_id) {
        return;
    }
    q.jobs.push_back(Job { root, notebook: notebook.to_string(), sheet_id: sheet_id.to_string(), sheet });
    shared().wake.notify_all();
}

/// Waits until everything asked for is stored (at most `limit`); whether it all is.
pub fn flush_for(limit: std::time::Duration) -> bool {
    let until = std::time::Instant::now() + limit;
    let mut q = lock();
    while q.busy || !q.jobs.is_empty() {
        let now = std::time::Instant::now();
        if now >= until {
            return false;
        }
        q = shared().wake.wait_timeout(q, until - now).unwrap_or_else(|p| p.into_inner()).0;
    }
    true
}

/// Waits until everything asked for is stored.
pub fn flush() -> bool {
    flush_for(std::time::Duration::from_secs(30))
}

/// Whether nothing waits to be stored.
pub fn idle() -> bool {
    let q = lock();
    !q.busy && q.jobs.is_empty()
}

/// Notebook `nb`'s change stamp right after our last save of it here (a stamp that is still
/// this one means nobody else saved since).
pub fn own_stamp(nb: &str) -> Option<u64> {
    lock().own.get(nb).copied()
}

/// Fires when a sheet is stored.
pub fn subscription() -> Subscription<Saved> {
    Subscription::run(saved_stream)
}

fn saved_stream() -> BoxStream<'static, Saved> {
    let (tx, rx) = mpsc::unbounded();
    lock().tx = Some(tx);
    rx.boxed()
}

fn run() {
    let mut libs: HashMap<PathBuf, Library> = HashMap::new();
    loop {
        let job = {
            let s = shared();
            let mut q = s.queue.lock().unwrap_or_else(|p| p.into_inner());
            q.busy = false;
            s.wake.notify_all();
            loop {
                if let Some(j) = q.jobs.pop_front() {
                    q.busy = true;
                    break j;
                }
                q = s.wake.wait(q).unwrap_or_else(|p| p.into_inner());
            }
        };
        let lib = match libs.get(&job.root) {
            Some(l) => Ok(l),
            None => Library::open(&job.root).map(|l| &*libs.entry(job.root.clone()).or_insert(l)),
        };
        let error = match lib {
            Ok(lib) => match lib.save_sheet(&job.notebook, &job.sheet_id, &job.sheet) {
                Ok(()) => {
                    let stamp = lib.change_stamp(&job.notebook);
                    lock().own.insert(job.notebook.clone(), stamp);
                    None
                }
                Err(e) => Some(e.to_string()),
            },
            Err(e) => Some(e.to_string()),
        };
        let tx = lock().tx.clone();
        if let Some(tx) = tx {
            let _ = tx.unbounded_send(Saved { notebook: job.notebook, sheet: job.sheet_id, error });
        }
    }
}
