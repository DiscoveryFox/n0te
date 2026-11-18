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

    /// Stores a blob (if new) and returns its BLAKE3 hex id.
    pub fn put_blob(&self, data: &[u8]) -> Result<String> {
        let id = blake3::hash(data).to_hex().to_string();
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
}
