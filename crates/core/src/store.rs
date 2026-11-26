//! On-disk library: notebooks, sheets and blobs under one root directory.
//!
//! ```text
//! <root>/device            this device's Loro peer id (never shared between devices)
//! <root>/notebooks/<id>/index.loro
//! <root>/notebooks/<id>/sheets/<sheet id>.loro
//! <root>/blobs/<blake3 hex>  PDFs, images, code files (content-addressed, write-once)
//! ```
//! Every write goes to a temp file, is fsynced and renamed into place, so a crash or a full
//! disk never leaves a half-written document behind.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::ids;
use crate::notebook::Notebook;
use crate::sheet::Sheet;

pub struct Library {
    root: PathBuf,
    peer: u64,
}

impl Library {
    /// Opens (or creates) the library at `root` for the app (the notes UI).
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        Self::open_as(root, "")
    }

    /// Opens the library for another writer on this device, e.g. `"sync"` for the sync
    /// service. Every writer that may edit at the same time as another needs its own Loro
    /// peer id (two writers with one id would produce clashing operations), so each `role`
    /// keeps its own id in `<root>/device.<role>`; `""` is the app's `<root>/device`.
    pub fn open_as(root: impl AsRef<Path>, role: &str) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("notebooks"))?;
        fs::create_dir_all(root.join("blobs"))?;
        if !role.is_empty() {
            check_name(role)?;
        }
        let dev = if role.is_empty() { root.join("device") } else { root.join(format!("device.{role}")) };
        let stored = match fs::read(&dev) {
            Ok(b) => std::str::from_utf8(&b).ok().and_then(|s| s.trim().parse::<u64>().ok()).filter(|&p| p != 0),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        let peer = match stored {
            Some(p) => p,
            // Missing, or damaged (a broken copy of the library): a fresh id is as good as the
            // old one (only reusing one is dangerous), so the library still opens.
            None => {
                let p = ids::new_peer_id();
                write_atomic(&dev, p.to_string().as_bytes())?;
                p
            }
        };
        Ok(Library { root, peer })
    }

    pub fn peer(&self) -> u64 {
        self.peer
    }

    /// Ids of all notebooks on this device.
    pub fn notebook_ids(&self) -> Result<Vec<String>> {
        let mut out = Vec::new();
        for e in fs::read_dir(self.root.join("notebooks"))? {
            let e = e?;
            if e.path().join("index.loro").is_file()
                && let Some(n) = e.file_name().to_str()
            {
                out.push(n.to_string());
            }
        }
        out.sort();
        Ok(out)
    }

    pub fn create_notebook(&self, title: &str) -> Result<Notebook> {
        let mut nb = Notebook::create(self.peer, title)?;
        let (_, lease) = self.peer_for(&self.nb_dir(&nb.id())?.join("index.loro"));
        nb.lease = lease;
        self.save_notebook(&nb)?;
        Ok(nb)
    }

    pub fn load_notebook(&self, id: &str) -> Result<Notebook> {
        let path = self.nb_dir(id)?.join("index.loro");
        let bytes = fs::read(&path).map_err(|e| not_found(e, &format!("notebook {id}")))?;
        let (peer, lease) = self.peer_for(&path);
        let mut nb = Notebook::load(peer, &bytes)?;
        nb.lease = lease;
        Ok(nb)
    }

    /// The peer id a document opened from `path` edits with: this library's own, unless a copy
    /// opened earlier still has it (the editor and the library screen hold one notebook, two
    /// views one page). Then the new copy gets an id of its own: two copies editing under one id
    /// make operations with the same ids and different content, which Loro takes for the same
    /// operation, so one copy's edits would be dropped and devices would end up different.
    fn peer_for(&self, path: &Path) -> (u64, Option<PeerLease>) {
        let key = (path.to_path_buf(), self.peer);
        let mut open = OPEN_WITH_OWN_PEER.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if open.contains(&key) {
            return (ids::new_peer_id(), None);
        }
        open.push(key.clone());
        (self.peer, Some(PeerLease { key }))
    }

    /// Saves a notebook. Other processes (the sync service, the laptop app, the tablet app
    /// with its sync engine) write the same files, so this merges what is on disk into `nb`
    /// first, under the notebook's lock: nobody's edits are lost, and `nb` picks up theirs.
    pub fn save_notebook(&self, nb: &Notebook) -> Result<()> {
        let id = nb.id();
        let dir = self.nb_dir(&id)?;
        fs::create_dir_all(dir.join("sheets"))?;
        let _lock = self.lock(&id)?;
        let path = dir.join("index.loro");
        if let Ok(disk) = fs::read(&path)
            && nb.import(&disk).is_err()
        {
            // A damaged file is replaced by our (complete) copy rather than blocking saves; the
            // damaged one is kept next to it, in case something can be got out of it by hand.
            keep_aside(&path);
        }
        write_atomic(&path, &nb.snapshot()?)?;
        self.touch(&id);
        Ok(())
    }

    /// Loads a sheet; a sheet nobody drew on yet has no file and comes back empty.
    pub fn load_sheet(&self, notebook: &str, sheet: &str) -> Result<Sheet> {
        let path = self.sheet_path(notebook, sheet)?;
        let bytes = match fs::read(&path) {
            Ok(b) => Some(b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => return Err(e.into()),
        };
        let (peer, lease) = self.peer_for(&path);
        let mut s = match bytes {
            Some(b) => Sheet::load(peer, &b)?,
            None => Sheet::create(peer)?,
        };
        s.lease = lease;
        Ok(s)
    }

    /// Saves a sheet, merging what is on disk first (see [`Library::save_notebook`]).
    pub fn save_sheet(&self, notebook: &str, sheet_id: &str, sheet: &Sheet) -> Result<()> {
        let path = self.sheet_path(notebook, sheet_id)?;
        fs::create_dir_all(path.parent().expect("has parent"))?;
        let _lock = self.lock(notebook)?;
        if let Ok(disk) = fs::read(&path)
            && sheet.import(&disk).is_err()
        {
            keep_aside(&path);
        }
        write_atomic(&path, &sheet.snapshot()?)?;
        self.touch(notebook);
        Ok(())
    }

    /// Saves a sheet without it counting as an edit: its file keeps its time (what only the
    /// handwriting index for search changed). Other devices still hear of it.
    pub fn save_sheet_quietly(&self, notebook: &str, sheet_id: &str, sheet: &Sheet) -> Result<()> {
        let path = self.sheet_path(notebook, sheet_id)?;
        let was = fs::metadata(&path).and_then(|m| m.modified()).ok();
        self.save_sheet(notebook, sheet_id, sheet)?;
        if let Some(t) = was
            && let Ok(f) = fs::File::options().write(true).open(&path)
        {
            let _ = f.set_modified(t);
        }
        Ok(())
    }

    /// The pages changed last, across all notebooks (here or merged from another device),
    /// newest first: notebook id, sheet id, when (ms since 1970). At most `limit`.
    pub fn recent_sheets(&self, limit: usize) -> Result<Vec<(String, String, i64)>> {
        let mut out = Vec::new();
        for nb in self.notebook_ids()? {
            let Ok(rd) = fs::read_dir(self.nb_dir(&nb)?.join("sheets")) else { continue };
            for e in rd.flatten() {
                let name = e.file_name();
                let Some(sheet) = name.to_str().and_then(|n| n.strip_suffix(".loro")) else { continue };
                let Ok(t) = e.metadata().and_then(|m| m.modified()) else { continue };
                let ms = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
                out.push((nb.clone(), sheet.to_string(), ms));
            }
        }
        out.sort_by(|a, b| b.2.cmp(&a.2));
        out.truncate(limit);
        Ok(out)
    }

    /// Merges updates (or a snapshot) from another device into the notebook's index on disk,
    /// creating the notebook if it is new here. Returns whether anything changed.
    pub fn merge_notebook(&self, id: &str, bytes: &[u8]) -> Result<bool> {
        let dir = self.nb_dir(id)?;
        fs::create_dir_all(dir.join("sheets"))?;
        let _lock = self.lock(id)?;
        let path = dir.join("index.loro");
        let (nb, damaged) = match fs::read(&path) {
            Ok(disk) => match Notebook::load(self.peer, &disk) {
                Ok(nb) => {
                    let before = nb.version();
                    nb.import(bytes)?;
                    if nb.version() == before {
                        return Ok(false);
                    }
                    (nb, false)
                }
                // Damaged here: the peer heard we have nothing and sent everything, so its
                // updates make the notebook whole again (it never could, were this an error).
                Err(_) => (Notebook::load(self.peer, bytes)?, true),
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Notebook::load(self.peer, bytes)?, false),
            Err(e) => return Err(e.into()),
        };
        if nb.id() != id {
            return Err(Error::Format(format!("updates for notebook {id} carry id {}", nb.id())));
        }
        if damaged {
            keep_aside(&path);
        }
        write_atomic(&path, &nb.snapshot()?)?;
        self.touch(id);
        Ok(true)
    }

    /// Merges updates into a sheet on disk (created if new). Returns whether anything changed.
    pub fn merge_sheet(&self, notebook: &str, sheet_id: &str, bytes: &[u8]) -> Result<bool> {
        let path = self.sheet_path(notebook, sheet_id)?;
        fs::create_dir_all(path.parent().expect("has parent"))?;
        let _lock = self.lock(notebook)?;
        let sheet = match fs::read(&path) {
            Ok(disk) => match Sheet::load(self.peer, &disk) {
                Ok(s) => {
                    let before = s.version();
                    s.import(bytes)?;
                    if s.version() == before {
                        return Ok(false);
                    }
                    s
                }
                // Damaged here: repaired from the peer's updates (see `merge_notebook`).
                Err(_) => {
                    let s = Sheet::load(self.peer, bytes)?;
                    keep_aside(&path);
                    s
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Sheet::load(self.peer, bytes)?,
            Err(e) => return Err(e.into()),
        };
        write_atomic(&path, &sheet.snapshot()?)?;
        self.touch(notebook);
        Ok(true)
    }

    /// Brings an open notebook up to date with its file (another process saved it). Returns
    /// whether anything changed.
    pub fn refresh_notebook(&self, nb: &Notebook) -> Result<bool> {
        let path = self.nb_dir(&nb.id())?.join("index.loro");
        let Ok(disk) = fs::read(&path) else { return Ok(false) };
        let before = nb.version();
        nb.import(&disk)?;
        Ok(nb.version() != before)
    }

    /// Brings an open sheet up to date with its file. Returns whether anything changed.
    pub fn refresh_sheet(&self, notebook: &str, sheet_id: &str, sheet: &Sheet) -> Result<bool> {
        let Ok(disk) = fs::read(self.sheet_path(notebook, sheet_id)?) else { return Ok(false) };
        let before = sheet.version();
        sheet.import(&disk)?;
        Ok(sheet.version() != before)
    }

    /// The sheets of a notebook that have a file (drawn on at least once).
    pub fn sheet_ids_on_disk(&self, notebook: &str) -> Result<Vec<String>> {
        let mut out = Vec::new();
        if let Ok(rd) = fs::read_dir(self.nb_dir(notebook)?.join("sheets")) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if let Some(id) = name.strip_suffix(".loro")
                    && !name.starts_with('.')
                    && check_name(id).is_ok()
                {
                    out.push(id.to_string());
                }
            }
        }
        out.sort();
        Ok(out)
    }

    /// Whether the notebook was moved to the trash on this device (sync must not bring it back).
    pub fn is_trashed(&self, id: &str) -> bool {
        let prefix = format!("{id}-");
        fs::read_dir(self.root.join("trash")).map(|rd| rd.flatten().any(|e| e.file_name().to_string_lossy().starts_with(&prefix))).unwrap_or(false)
    }

    /// A value that changes whenever the notebook is saved or merged by any process on this
    /// device (poll it to notice changes made elsewhere); 0 if never.
    pub fn change_stamp(&self, id: &str) -> u64 {
        fs::read_to_string(self.root.join("changes").join(id)).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0)
    }

    fn touch(&self, id: &str) {
        let dir = self.root.join("changes");
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
        // Strictly increasing even within one clock tick.
        let stamp = now.max(self.change_stamp(id) + 1);
        let _ = fs::create_dir_all(&dir).and_then(|_| fs::write(dir.join(id), stamp.to_string()));
    }

    /// Exclusive lock on a notebook's files, across processes (released when dropped).
    fn lock(&self, id: &str) -> Result<NotebookLock> {
        let dir = self.nb_dir(id)?;
        fs::create_dir_all(&dir)?;
        let f = fs::OpenOptions::new().create(true).truncate(false).read(true).write(true).open(dir.join(".lock"))?;
        f.lock()?;
        Ok(NotebookLock(f))
    }

    /// The library's root directory (the sync engine keeps its files under `<root>/sync`).
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Imports a bundle as a NEW notebook (fresh id, title `title`), leaving any existing copy
    /// alone: restoring an old version from the history. Returns the new id.
    pub fn import_bundle_as_copy(&self, bytes: &[u8], title: &str) -> Result<String> {
        let b = self.check_bundle(bytes, false)?;
        let id = ids::new_id(self.peer);
        b.index.set_id(&id)?;
        b.index.set_title(title)?;
        for (blob, data) in &b.blobs {
            self.put_blob_as(blob, data)?;
        }
        for (sheet_id, data) in &b.sheets {
            self.merge_sheet(&id, sheet_id, data)?;
        }
        // The index last: the notebook shows up in the library only once its pages are there.
        self.save_notebook(&b.index)?;
        Ok(id)
    }

    /// The title of the notebook a bundle holds (without importing it).
    pub fn bundle_title(&self, bytes: &[u8]) -> Result<String> {
        let entries = read_bundle(bytes)?;
        let index = entries.iter().find(|(n, _)| n == "index.loro").map(|(_, d)| *d).ok_or_else(|| Error::Format("bundle without index".into()))?;
        Ok(Notebook::load(self.peer, index)?.title())
    }

    /// Reads and checks a whole bundle before anything of it is written, so a bad entry can't
    /// leave half a notebook behind. `strict`: a blob that doesn't match its name is an error
    /// (else it is left out).
    fn check_bundle<'a>(&self, bytes: &'a [u8], strict: bool) -> Result<CheckedBundle<'a>> {
        let entries = read_bundle(bytes)?;
        let index = entries.iter().find(|(n, _)| n == "index.loro").map(|(_, d)| *d).ok_or_else(|| Error::Format("bundle without index".into()))?;
        let nb = Notebook::load(self.peer, index)?;
        check_name(&nb.id())?;
        let mut out = CheckedBundle { index_bytes: index, index: nb, sheets: Vec::new(), blobs: Vec::new() };
        for (name, data) in entries {
            if let Some(file) = name.strip_prefix("sheets/") {
                let Some(sheet_id) = file.strip_suffix(".loro") else { continue };
                check_name(sheet_id)?;
                Sheet::load(self.peer, data)?;
                out.sheets.push((sheet_id.to_string(), data));
            } else if let Some(blob) = name.strip_prefix("blobs/") {
                check_name(blob)?;
                if blob_id(data) == blob {
                    out.blobs.push((blob.to_string(), data));
                } else if strict {
                    return Err(Error::Format(format!("blob {blob} does not match its content")));
                }
            }
        }
        Ok(out)
    }

    /// A notebook as one file (`.n0te`): its index, every sheet and the PDFs it uses. Meant
    /// for carrying a notebook to another device by hand; [`Library::import_bundle`] merges.
    pub fn export_bundle(&self, id: &str) -> Result<Vec<u8>> {
        self.export(id, true)
    }

    /// A bundle of just the notebook's documents, without its PDFs and pictures: for
    /// checkpoints of this library (it keeps every blob for good, so they needn't be copied).
    pub fn export_bundle_without_blobs(&self, id: &str) -> Result<Vec<u8>> {
        self.export(id, false)
    }

    fn export(&self, id: &str, with_blobs: bool) -> Result<Vec<u8>> {
        let dir = self.nb_dir(id)?;
        let mut out = BUNDLE_MAGIC.to_vec();
        let mut put = |name: &str, data: &[u8]| {
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(&(data.len() as u64).to_le_bytes());
            out.extend_from_slice(data);
        };
        let index = fs::read(dir.join("index.loro")).map_err(|e| not_found(e, &format!("notebook {id}")))?;
        put("index.loro", &index);
        if let Ok(rd) = fs::read_dir(dir.join("sheets")) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.ends_with(".loro") && !name.starts_with('.') {
                    let data = fs::read(e.path())?;
                    // A damaged page stays out: the rest of the notebook still travels (and
                    // the bundle still imports, which checks every page).
                    if intact(&data) {
                        put(&format!("sheets/{name}"), &data);
                    }
                }
            }
        }
        let nb = Notebook::load(self.peer, &index)?;
        if !with_blobs {
            return Ok(out);
        }
        let mut blobs: Vec<String> = nb.pdf_blobs();
        blobs.extend(self.image_blobs(&nb.id()));
        blobs.sort();
        blobs.dedup();
        for b in blobs {
            if let Ok(data) = self.blob(&b) {
                put(&format!("blobs/{b}"), &data);
            }
        }
        Ok(out)
    }

    /// Adds a bundle's notebook to the library, or merges it into the copy already here (the
    /// documents are CRDTs: both sides' edits are kept). Returns the notebook id.
    pub fn import_bundle(&self, bytes: &[u8]) -> Result<String> {
        let b = self.check_bundle(bytes, true)?;
        let id = b.index.id();
        for (blob, data) in &b.blobs {
            self.put_blob_as(blob, data)?;
        }
        for (sheet_id, data) in &b.sheets {
            self.merge_sheet(&id, sheet_id, data)?;
        }
        // The index last (see `import_bundle_as_copy`).
        self.merge_notebook(&id, b.index_bytes)?;
        Ok(id)
    }

    /// Moves a notebook (with its sheets) to `trash/`, out of the library but recoverable.
    pub fn trash_notebook(&self, id: &str) -> Result<()> {
        let dir = self.nb_dir(id)?;
        let trash = self.root.join("trash");
        fs::create_dir_all(&trash)?;
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        fs::rename(&dir, trash.join(format!("{id}-{stamp}")))?;
        Ok(())
    }

    /// The blobs of every image on the notebook's sheets (sync wants them, bundles carry them).
    pub fn image_blobs(&self, notebook: &str) -> Vec<String> {
        let Ok(nb) = self.load_notebook(notebook) else { return Vec::new() };
        let mut out: Vec<String> = nb.sheet_order().iter().filter_map(|s| self.load_sheet(notebook, s).ok()).flat_map(|s| s.images().into_iter().map(|i| i.blob)).collect();
        out.sort();
        out.dedup();
        out
    }

    /// Stores a blob (if new) and returns its BLAKE3 hex id ([`blob_id`]).
    pub fn put_blob(&self, data: &[u8]) -> Result<String> {
        let id = blob_id(data);
        self.put_blob_as(&id, data)?;
        Ok(id)
    }

    /// Stores `data` (whose id the caller checked) unless it is there already. A file of the
    /// wrong size (cut short by a full disk or a broken copy) is written again: otherwise the
    /// blob would stay damaged for good, however often sync brings it.
    fn put_blob_as(&self, id: &str, data: &[u8]) -> Result<()> {
        let path = self.root.join("blobs").join(id);
        if fs::metadata(&path).map(|m| m.len() != data.len() as u64).unwrap_or(true) {
            write_atomic(&path, data)?;
        }
        Ok(())
    }

    /// Reads a blob and checks it against its id.
    pub fn blob(&self, id: &str) -> Result<Vec<u8>> {
        check_name(id)?;
        let data = fs::read(self.root.join("blobs").join(id)).map_err(|e| not_found(e, &format!("blob {id}")))?;
        if blake3::hash(&data).to_hex().as_str() != id {
            return Err(Error::Format(format!("blob {id} is damaged")));
        }
        Ok(data)
    }

    pub fn blob_path(&self, id: &str) -> Result<PathBuf> {
        check_name(id)?;
        Ok(self.root.join("blobs").join(id))
    }

    fn nb_dir(&self, id: &str) -> Result<PathBuf> {
        check_name(id)?;
        Ok(self.root.join("notebooks").join(id))
    }

    fn sheet_path(&self, notebook: &str, sheet: &str) -> Result<PathBuf> {
        check_name(sheet)?;
        Ok(self.nb_dir(notebook)?.join("sheets").join(format!("{sheet}.loro")))
    }
}

/// The id a blob is stored under: the BLAKE3 hash of its bytes, as hex.
pub fn blob_id(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}

/// Documents open in this process under their library's own peer id: (file, peer).
static OPEN_WITH_OWN_PEER: std::sync::Mutex<Vec<(PathBuf, u64)>> = std::sync::Mutex::new(Vec::new());

/// A document's claim on its library's peer id (see `Library::peer_for`), given up when the
/// document is dropped.
pub(crate) struct PeerLease {
    key: (PathBuf, u64),
}

impl Drop for PeerLease {
    fn drop(&mut self) {
        let mut open = OPEN_WITH_OWN_PEER.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(i) = open.iter().position(|k| *k == self.key) {
            open.swap_remove(i);
        }
    }
}

/// A bundle read and checked by `Library::check_bundle`.
struct CheckedBundle<'a> {
    index_bytes: &'a [u8],
    index: Notebook,
    sheets: Vec<(String, &'a [u8])>,
    blobs: Vec<(String, &'a [u8])>,
}

/// Moves a damaged document out of the way (`<name>.damaged-<unix seconds>`, next to it),
/// before it is replaced by a good copy. Not a document any more by its name, so nothing reads
/// or syncs it; it stays for recovery by hand.
fn keep_aside(path: &Path) {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let _ = fs::rename(path, path.with_file_name(format!("{name}.damaged-{secs}")));
}

/// Whether a stored document is whole (Loro's checksum over it holds), without decoding it.
fn intact(bytes: &[u8]) -> bool {
    loro::LoroDoc::decode_import_blob_meta(bytes, true).is_ok()
}

/// Held while a notebook's files are read-merged-written; the OS lock goes with the file.
struct NotebookLock(fs::File);

impl Drop for NotebookLock {
    fn drop(&mut self) {
        let _ = self.0.unlock();
    }
}

const BUNDLE_MAGIC: &[u8] = b"N0TEBNDL\x01";

/// (name, data) entries of a bundle.
fn read_bundle(b: &[u8]) -> Result<Vec<(String, &[u8])>> {
    let bad = || Error::Format("not a n0te bundle".into());
    let mut rest = b.strip_prefix(BUNDLE_MAGIC).ok_or_else(bad)?;
    let mut out = Vec::new();
    while !rest.is_empty() {
        if rest.len() < 2 { return Err(bad()); }
        let n = u16::from_le_bytes([rest[0], rest[1]]) as usize;
        rest = &rest[2..];
        if rest.len() < n + 8 { return Err(bad()); }
        let name = std::str::from_utf8(&rest[..n]).map_err(|_| bad())?.to_string();
        let len = u64::from_le_bytes(rest[n..n + 8].try_into().expect("8 bytes")) as usize;
        rest = &rest[n + 8..];
        if rest.len() < len { return Err(bad()); }
        out.push((name, &rest[..len]));
        rest = &rest[len..];
    }
    Ok(out)
}

/// Ids become file names: only [0-9a-zA-Z_-], 1..=128 chars (no traversal).
fn check_name(s: &str) -> Result<()> {
    if s.is_empty() || s.len() > 128 || !s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
        return Err(Error::Invalid(format!("bad id {s:?}")));
    }
    Ok(())
}

fn not_found(e: std::io::Error, what: &str) -> Error {
    if e.kind() == std::io::ErrorKind::NotFound { Error::NotFound(what.to_string()) } else { e.into() }
}

/// Writes `data` to `path` via a temp file in the same directory + fsync + rename.
pub fn write_atomic(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path.parent().ok_or_else(|| Error::Invalid("no parent dir".into()))?;
    let tmp = dir.join(format!(".{}.tmp-{}", path.file_name().and_then(|n| n.to_str()).unwrap_or("f"), ids::new_id(0)));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    if let Err(e) = fs::rename(&tmp, path) {
        let _ = fs::remove_file(&tmp);
        return Err(e.into());
    }
    if let Ok(d) = fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {

    #[test]
    fn recent_sheets_are_newest_first_and_quiet_saves_dont_count() {
        let root = std::env::temp_dir().join(format!("n0te-recent-{}", crate::ids::new_id(9)));
        let lib = Library::open(&root).unwrap();
        let nb = lib.create_notebook("A").unwrap();
        let (_, s1) = nb.add_sheet(None, 0, &crate::notebook::SheetSpec::a4(crate::notebook::Background::Grid)).unwrap();
        let (_, s2) = nb.add_sheet(None, 1, &crate::notebook::SheetSpec::a4(crate::notebook::Background::Grid)).unwrap();
        lib.save_notebook(&nb).unwrap();
        let a = lib.load_sheet(&nb.id(), &s1).unwrap();
        lib.save_sheet(&nb.id(), &s1, &a).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        let b = lib.load_sheet(&nb.id(), &s2).unwrap();
        lib.save_sheet(&nb.id(), &s2, &b).unwrap();
        let r = lib.recent_sheets(10).unwrap();
        assert_eq!(r.iter().map(|x| x.1.clone()).collect::<Vec<_>>(), vec![s2.clone(), s1.clone()]);
        // Saving the first one quietly (the search index) keeps it second.
        std::thread::sleep(std::time::Duration::from_millis(30));
        lib.save_sheet_quietly(&nb.id(), &s1, &a).unwrap();
        assert_eq!(lib.recent_sheets(1).unwrap()[0].1, s2);
        std::fs::remove_dir_all(root).unwrap();
    }

    use super::*;
    use crate::ink::{Brush, InkPoint};
    use crate::notebook::{Background, SheetSpec};

    fn tmpdir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("n0te-test-{tag}-{}", ids::new_id(1)));
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn library_round_trip() {
        let root = tmpdir("lib");
        let lib = Library::open(&root).unwrap();
        let peer = lib.peer();
        let nb = lib.create_notebook("Analysis").unwrap();
        let (_, sid) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Grid)).unwrap();
        lib.save_notebook(&nb).unwrap();
        let sheet = lib.load_sheet(&nb.id(), &sid).unwrap();
        assert!(sheet.is_empty());
        sheet.add_stroke(Brush::default(), &[InkPoint { x: 1.0, y: 2.0, pressure: 0.5, t_ms: 0 }]).unwrap();
        lib.save_sheet(&nb.id(), &sid, &sheet).unwrap();

        let lib2 = Library::open(&root).unwrap();
        assert_eq!(lib2.peer(), peer, "the device id must persist");
        assert_eq!(lib2.notebook_ids().unwrap(), vec![nb.id()]);
        let nb2 = lib2.load_notebook(&nb.id()).unwrap();
        assert_eq!(nb2.title(), "Analysis");
        assert_eq!(lib2.load_sheet(&nb.id(), &sid).unwrap().len(), 1);

        let blob = lib2.put_blob(b"%PDF-1.7 fake").unwrap();
        assert_eq!(lib2.blob(&blob).unwrap(), b"%PDF-1.7 fake");
        assert!(lib2.blob("../../etc/passwd").is_err());
        assert!(lib2.load_notebook("..").is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn trashed_notebooks_leave_the_library_but_stay_on_disk() {
        let root = std::env::temp_dir().join(format!("n0te-trash-{}", ids::new_id(2)));
        let lib = Library::open(&root).unwrap();
        let nb = lib.create_notebook("weg").unwrap();
        nb.set_cover("dots-red").unwrap();
        lib.save_notebook(&nb).unwrap();
        let id = nb.id();
        assert_eq!(lib.load_notebook(&id).unwrap().cover(), "dots-red");
        lib.trash_notebook(&id).unwrap();
        assert!(!lib.notebook_ids().unwrap().contains(&id));
        assert_eq!(std::fs::read_dir(root.join("trash")).unwrap().count(), 1);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn bundles_carry_a_notebook_and_merge_into_an_existing_copy() {
        use crate::ink::{Brush, InkPoint};
        use crate::notebook::{Background, SheetSpec};
        let tag = ids::new_id(3);
        let tablet = Library::open(std::env::temp_dir().join(format!("n0te-bt-{tag}"))).unwrap();
        let laptop = Library::open(std::env::temp_dir().join(format!("n0te-bl-{tag}"))).unwrap();
        let nb = tablet.create_notebook("Analysis").unwrap();
        let pdf = tablet.put_blob(b"%PDF-fake").unwrap();
        let (_, s1) = nb.add_sheet(None, 0, &SheetSpec { pdf: Some((pdf.clone(), 0)), ..SheetSpec::a4(Background::Blank) }).unwrap();
        tablet.save_notebook(&nb).unwrap();
        let sheet = tablet.load_sheet(&nb.id(), &s1).unwrap();
        let line: Vec<InkPoint> = (0..5).map(|i| InkPoint { x: i as f32, y: 1.0, pressure: 0.5, t_ms: i }).collect();
        sheet.add_stroke(Brush::default(), &line).unwrap();
        tablet.save_sheet(&nb.id(), &s1, &sheet).unwrap();

        // To the laptop: everything arrives.
        let id = laptop.import_bundle(&tablet.export_bundle(&nb.id()).unwrap()).unwrap();
        assert_eq!(id, nb.id());
        assert_eq!(laptop.load_notebook(&id).unwrap().title(), "Analysis");
        assert_eq!(laptop.load_sheet(&id, &s1).unwrap().len(), 1);
        assert_eq!(laptop.blob(&pdf).unwrap(), b"%PDF-fake");

        // Both draw; the laptop's copy goes back: the tablet keeps both strokes.
        let ls = laptop.load_sheet(&id, &s1).unwrap();
        ls.add_stroke(Brush::default(), &line).unwrap();
        laptop.save_sheet(&id, &s1, &ls).unwrap();
        let ts = tablet.load_sheet(&id, &s1).unwrap();
        ts.add_stroke(Brush::default(), &line).unwrap();
        tablet.save_sheet(&id, &s1, &ts).unwrap();
        tablet.import_bundle(&laptop.export_bundle(&id).unwrap()).unwrap();
        assert_eq!(tablet.load_sheet(&id, &s1).unwrap().len(), 3, "original + both new strokes");
        assert!(tablet.import_bundle(b"nope").is_err());
    }

    #[test]
    fn two_writers_on_one_library_keep_each_others_edits() {
        use crate::ink::{Brush, InkPoint};
        use crate::notebook::{Background, SheetSpec};
        let root = std::env::temp_dir().join(format!("n0te-two-writers-{}", ids::new_id(4)));
        // Two writers = the laptop app and the sync service (separate processes in real life),
        // each with its own peer id.
        let app = Library::open(&root).unwrap();
        let service = Library::open_as(&root, "sync").unwrap();
        assert_ne!(app.peer(), service.peer());
        assert_eq!(Library::open_as(&root, "sync").unwrap().peer(), service.peer(), "persistent per role");
        let nb = app.create_notebook("Gemeinsam").unwrap();
        let id = nb.id();
        let (_, s1) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Grid)).unwrap();
        app.save_notebook(&nb).unwrap();
        let stamp0 = app.change_stamp(&id);
        assert!(stamp0 > 0);

        // Both hold the notebook in memory; each adds a page and saves.
        let theirs = service.load_notebook(&id).unwrap();
        theirs.add_sheet(None, 9, &SheetSpec::a4(Background::Dots)).unwrap();
        service.save_notebook(&theirs).unwrap();
        nb.add_sheet(None, 9, &SheetSpec::a4(Background::Lines)).unwrap();
        app.save_notebook(&nb).unwrap(); // must not drop the service's page
        assert_eq!(app.load_notebook(&id).unwrap().sheet_count(), 3);
        assert_eq!(nb.sheet_count(), 3, "the app's copy picked up the other page while saving");
        assert!(app.change_stamp(&id) > stamp0);

        // Same for strokes on one sheet.
        let line: Vec<InkPoint> = (0..4).map(|i| InkPoint { x: i as f32, y: 2.0, pressure: 0.5, t_ms: i }).collect();
        let a = app.load_sheet(&id, &s1).unwrap();
        let b = service.load_sheet(&id, &s1).unwrap();
        a.add_stroke(Brush::default(), &line).unwrap();
        b.add_stroke(Brush::default(), &line).unwrap();
        app.save_sheet(&id, &s1, &a).unwrap();
        service.save_sheet(&id, &s1, &b).unwrap();
        assert_eq!(app.load_sheet(&id, &s1).unwrap().len(), 2);

        // refresh pulls a file change into an open copy; merge_* applies remote updates.
        assert!(app.refresh_sheet(&id, &s1, &a).unwrap());
        assert_eq!(a.len(), 2);
        let c = Sheet::create(77).unwrap();
        c.add_stroke(Brush::default(), &line).unwrap();
        assert!(service.merge_sheet(&id, &s1, &c.updates_since(&b.version()).unwrap()).unwrap());
        assert!(!service.merge_sheet(&id, &s1, &c.updates_since(&b.version()).unwrap()).unwrap(), "idempotent");
        assert_eq!(app.load_sheet(&id, &s1).unwrap().len(), 3);
        assert_eq!(app.sheet_ids_on_disk(&id).unwrap(), vec![s1.clone()]);

        // Restore-as-copy: a new notebook with the old content, the original untouched.
        let bundle = app.export_bundle(&id).unwrap();
        let copy = app.import_bundle_as_copy(&bundle, "Gemeinsam (Stand gestern)").unwrap();
        assert_ne!(copy, id);
        assert_eq!(app.load_notebook(&copy).unwrap().title(), "Gemeinsam (Stand gestern)");
        assert_eq!(app.load_sheet(&copy, &s1).unwrap().len(), 3);
        assert_eq!(app.load_notebook(&id).unwrap().title(), "Gemeinsam");
        app.trash_notebook(&copy).unwrap();
        assert!(app.is_trashed(&copy) && !app.is_trashed(&id));
        std::fs::remove_dir_all(root).unwrap();
    }
}
