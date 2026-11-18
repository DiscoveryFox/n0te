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
    /// Opens (or creates) the library at `root`.
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        fs::create_dir_all(root.join("notebooks"))?;
        fs::create_dir_all(root.join("blobs"))?;
        let dev = root.join("device");
        let peer = match fs::read_to_string(&dev) {
            Ok(s) => s.trim().parse::<u64>().map_err(|_| Error::Format("bad device id".into()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let p = ids::new_peer_id();
                write_atomic(&dev, p.to_string().as_bytes())?;
                p
            }
            Err(e) => return Err(e.into()),
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
        let nb = Notebook::create(self.peer, title)?;
        self.save_notebook(&nb)?;
        Ok(nb)
    }

    pub fn load_notebook(&self, id: &str) -> Result<Notebook> {
        let path = self.nb_dir(id)?.join("index.loro");
        let bytes = fs::read(&path).map_err(|e| not_found(e, &format!("notebook {id}")))?;
        Notebook::load(self.peer, &bytes)
    }

    pub fn save_notebook(&self, nb: &Notebook) -> Result<()> {
        let dir = self.nb_dir(&nb.id())?;
        fs::create_dir_all(dir.join("sheets"))?;
        write_atomic(&dir.join("index.loro"), &nb.snapshot()?)
    }

    /// Loads a sheet; a sheet nobody drew on yet has no file and comes back empty.
    pub fn load_sheet(&self, notebook: &str, sheet: &str) -> Result<Sheet> {
        let path = self.sheet_path(notebook, sheet)?;
        match fs::read(&path) {
            Ok(b) => Sheet::load(self.peer, &b),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Sheet::create(self.peer),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save_sheet(&self, notebook: &str, sheet_id: &str, sheet: &Sheet) -> Result<()> {
        let path = self.sheet_path(notebook, sheet_id)?;
        fs::create_dir_all(path.parent().expect("has parent"))?;
        write_atomic(&path, &sheet.snapshot()?)
    }

    /// A notebook as one file (`.n0te`): its index, every sheet and the PDFs it uses. Meant
    /// for carrying a notebook to another device by hand; [`Library::import_bundle`] merges.
    pub fn export_bundle(&self, id: &str) -> Result<Vec<u8>> {
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
                    put(&format!("sheets/{name}"), &fs::read(e.path())?);
                }
            }
        }
        let nb = Notebook::load(self.peer, &index)?;
        let mut blobs: Vec<String> = nb.pdf_blobs();
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
        let entries = read_bundle(bytes)?;
        let index = entries.iter().find(|(n, _)| n == "index.loro").map(|(_, d)| *d).ok_or_else(|| Error::Format("bundle without index".into()))?;
        let incoming = Notebook::load(self.peer, index)?;
        let id = incoming.id();
        let dir = self.nb_dir(&id)?;
        let nb = match fs::read(dir.join("index.loro")) {
            Ok(mine) => {
                let nb = Notebook::load(self.peer, &mine)?;
                nb.import(index)?;
                nb
            }
            Err(_) => incoming,
        };
        self.save_notebook(&nb)?;
        for (name, data) in &entries {
            if let Some(file) = name.strip_prefix("sheets/") {
                let Some(sheet_id) = file.strip_suffix(".loro") else { continue };
                check_name(sheet_id)?;
                let sheet = match fs::read(self.sheet_path(&id, sheet_id)?) {
                    Ok(mine) => {
                        let s = Sheet::load(self.peer, &mine)?;
                        s.import(data)?;
                        s
                    }
                    Err(_) => Sheet::load(self.peer, data)?,
                };
                self.save_sheet(&id, sheet_id, &sheet)?;
            } else if let Some(blob) = name.strip_prefix("blobs/") {
                check_name(blob)?;
                if blob_id(data) != blob {
                    return Err(Error::Format(format!("blob {blob} does not match its content")));
                }
                let path = self.root.join("blobs").join(blob);
                if !path.exists() {
                    write_atomic(&path, data)?;
                }
            }
        }
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

    /// Stores a blob (if new) and returns its BLAKE3 hex id ([`blob_id`]).
    pub fn put_blob(&self, data: &[u8]) -> Result<String> {
        let id = blob_id(data);
        let path = self.root.join("blobs").join(&id);
        if !path.exists() {
            write_atomic(&path, data)?;
        }
        Ok(id)
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

/// Ids become file names: only [0-9a-zA-Z_-], 1..=128 chars (no traversal).
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
}
