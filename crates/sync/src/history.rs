//! Version history: checkpoints of changed notebooks as bundles in
//! `<root>/history/<notebook>/<unix seconds>.n0te`, thinned over time (all of the last hour,
//! hourly for a day, daily for 30 days, weekly after). Every new checkpoint is verified by
//! importing it into a scratch library (backup check); a broken one is renamed `.bad` and
//! reported. Restoring never touches the live notebook: it becomes a new notebook
//! "Titel (Stand …)".

use std::fs;
use std::path::{Path, PathBuf};

use n0te_core::store::Library;

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq)]
pub struct Checkpoint {
    pub notebook: String,
    /// Unix seconds.
    pub at: u64,
    pub size: u64,
    pub path: PathBuf,
}

/// Outcome of one checkpoint pass.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Pass {
    pub written: Vec<(String, u64)>,
    pub failed: Vec<(String, String)>,
    pub removed: usize,
}

fn dir(root: &Path, notebook: &str) -> PathBuf {
    root.join("history").join(notebook)
}

/// The checkpoints of a notebook, newest first.
pub fn list(root: &Path, notebook: &str) -> Vec<Checkpoint> {
    let mut out: Vec<Checkpoint> = fs::read_dir(dir(root, notebook))
        .map(|rd| {
            rd.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let at = name.strip_suffix(".n0te")?.parse().ok()?;
                    Some(Checkpoint { notebook: notebook.to_string(), at, size: e.metadata().map(|m| m.len()).unwrap_or(0), path: e.path() })
                })
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b.at.cmp(&a.at));
    out
}

/// Which checkpoints (unix seconds) to delete at `now`: keep everything younger than an hour,
/// then the newest per hour (up to a day), per day (up to 30 days), per week (older).
pub fn thin(times: &[u64], now: u64) -> Vec<u64> {
    const H: u64 = 3600;
    const D: u64 = 86_400;
    let mut sorted = times.to_vec();
    sorted.sort_unstable_by(|a, b| b.cmp(a));
    let mut seen = std::collections::HashSet::new();
    let mut delete = Vec::new();
    for (i, &t) in sorted.iter().enumerate() {
        let age = now.saturating_sub(t);
        let bucket = match age {
            a if a < H => (0u8, t),
            a if a < D => (1, t / H),
            a if a < 30 * D => (2, t / D),
            _ => (3, t / (7 * D)),
        };
        if i > 0 && !seen.insert(bucket) {
            delete.push(t);
        } else {
            seen.insert(bucket);
        }
    }
    delete
}

/// Writes a checkpoint of every notebook changed since its newest checkpoint, verifies it,
/// and thins old ones. `now`: unix seconds.
pub fn checkpoint_changed(lib: &Library, now: u64) -> Pass {
    let mut pass = Pass::default();
    let root = lib.root().to_path_buf();
    for id in lib.notebook_ids().unwrap_or_default() {
        let newest = list(&root, &id).first().map(|c| c.at);
        let changed_ns = lib.change_stamp(&id);
        let due = match newest {
            None => true,
            Some(at) => changed_ns > at.saturating_mul(1_000_000_000) && now > at,
        };
        if due {
            match write_checkpoint(lib, &id, now) {
                Ok(()) => pass.written.push((id.clone(), now)),
                Err(e) => pass.failed.push((id.clone(), e.to_string())),
            }
        }
        let times: Vec<u64> = list(&root, &id).iter().map(|c| c.at).collect();
        for t in thin(&times, now) {
            if fs::remove_file(dir(&root, &id).join(format!("{t}.n0te"))).is_ok() {
                pass.removed += 1;
            }
        }
    }
    pass
}

fn write_checkpoint(lib: &Library, id: &str, now: u64) -> Result<()> {
    let bytes = lib.export_bundle(id)?;
    let d = dir(lib.root(), id);
    fs::create_dir_all(&d)?;
    let path = d.join(format!("{now}.n0te"));
    n0te_core::store::write_atomic(&path, &bytes)?;
    if let Err(e) = verify(&bytes, id) {
        let _ = fs::rename(&path, d.join(format!("{now}.n0te.bad")));
        let _ = fs::OpenOptions::new().create(true).append(true).open(d.join("problems.log")).and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{now} checkpoint failed its check: {e}")
        });
        return Err(e);
    }
    Ok(())
}

/// Backup check: the bundle imports into an empty library and yields the same notebook.
pub fn verify(bytes: &[u8], id: &str) -> Result<()> {
    let scratch = std::env::temp_dir().join(format!("n0te-verify-{}-{}", std::process::id(), n0te_core::ids::new_id(11)));
    let result = (|| {
        let lib = Library::open(&scratch)?;
        let got = lib.import_bundle(bytes)?;
        if got != id {
            return Err(Error::Invalid(format!("checkpoint holds notebook {got}, expected {id}")));
        }
        let nb = lib.load_notebook(&got)?;
        for sheet in lib.sheet_ids_on_disk(&got)? {
            lib.load_sheet(&got, &sheet)?;
        }
        let _ = nb.outline(true);
        Ok(())
    })();
    let _ = fs::remove_dir_all(&scratch);
    result
}

/// Restores checkpoint `at` of `notebook` as a new notebook "Titel (Stand YYYY-MM-DD HH:MM)".
pub fn restore_as_copy(lib: &Library, notebook: &str, at: u64) -> Result<String> {
    let path = dir(lib.root(), notebook).join(format!("{at}.n0te"));
    let bytes = fs::read(&path).map_err(|_| Error::Invalid(format!("kein Stand {at} für {notebook}")))?;
    let title = {
        let scratch = std::env::temp_dir().join(format!("n0te-restore-{}-{}", std::process::id(), n0te_core::ids::new_id(12)));
        let t = Library::open(&scratch).and_then(|l| {
            let id = l.import_bundle(&bytes)?;
            Ok(l.load_notebook(&id)?.title())
        });
        let _ = fs::remove_dir_all(&scratch);
        t?
    };
    Ok(lib.import_bundle_as_copy(&bytes, &format!("{title} (Stand {})", local_time(at)))?)
}

/// "YYYY-MM-DD HH:MM" in local time.
pub use n0te_platform::local_time;

#[cfg(test)]
mod tests {
    use super::*;

    const H: u64 = 3600;
    const D: u64 = 86_400;

    #[test]
    fn thinning_keeps_recent_hourly_daily_weekly() {
        let now = 1000 * D;
        let times = vec![
            now - 60,
            now - 120,
            now - 50 * 60, // last hour: all three stay
            now - 2 * H + 10,
            now - 2 * H + 20, // same hour bucket: only the newer stays
            now - 3 * D + 5,
            now - 3 * D + 50, // same day: the newer stays
            now - 60 * D,
            now - 60 * D - 3600, // old, same week: one stays
        ];
        let del = thin(&times, now);
        assert_eq!(del.len(), 3, "{del:?}");
        assert!(del.contains(&(now - 2 * H + 10)));
        assert!(del.contains(&(now - 3 * D + 5)));
        assert!(!del.contains(&(now - 60)) && !del.contains(&(now - 50 * 60)));
        assert!(thin(&[now - 400 * D], now).is_empty(), "the newest is always kept");
    }

    #[test]
    fn checkpoints_verify_and_restore_as_copy() {
        use n0te_core::ink::{Brush, InkPoint};
        use n0te_core::notebook::{Background, SheetSpec};
        let root = std::env::temp_dir().join(format!("n0te-hist-{}", n0te_core::ids::new_id(13)));
        let lib = Library::open(&root).unwrap();
        let nb = lib.create_notebook("Physik").unwrap();
        let (_, s1) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Grid)).unwrap();
        lib.save_notebook(&nb).unwrap();
        let sheet = lib.load_sheet(&nb.id(), &s1).unwrap();
        let line: Vec<InkPoint> = (0..5).map(|i| InkPoint { x: i as f32, y: 1.0, pressure: 0.5, t_ms: i }).collect();
        sheet.add_stroke(Brush::default(), &line).unwrap();
        lib.save_sheet(&nb.id(), &s1, &sheet).unwrap();

        let t0 = 1_800_000_000;
        let pass = checkpoint_changed(&lib, t0);
        assert_eq!(pass.written, vec![(nb.id(), t0)]);
        assert!(pass.failed.is_empty());
        // Nothing changed: no new checkpoint.
        assert!(checkpoint_changed(&lib, t0 + 10).written.is_empty());

        // Draw more, then restore the old state as a copy.
        sheet.add_stroke(Brush::default(), &line).unwrap();
        lib.save_sheet(&nb.id(), &s1, &sheet).unwrap();
        let copy = restore_as_copy(&lib, &nb.id(), t0).unwrap();
        assert_ne!(copy, nb.id());
        assert!(lib.load_notebook(&copy).unwrap().title().starts_with("Physik (Stand "));
        assert_eq!(lib.load_sheet(&copy, &s1).unwrap().len(), 1, "the old state");
        assert_eq!(lib.load_sheet(&nb.id(), &s1).unwrap().len(), 2, "live notebook untouched");
        assert_eq!(list(&root, &nb.id()).len(), 1);
        assert!(verify(b"garbage", &nb.id()).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
