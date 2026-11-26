//! Crash reports. When the program panics, a report goes to `<cache>/crashes/`: version, system,
//! what went wrong and where, and a backtrace. The next start offers to copy it. On Windows the
//! program has no console, so without this a crash vanishes without a trace.
//!
//! A report holds no notes, names or paths of the user, only the program's own state.

use std::path::{Path, PathBuf};

/// Reports kept (the oldest go).
const KEEP: usize = 5;
/// Where the issue tracker is (the "report" button opens a new issue there).
pub const ISSUES: &str = "https://github.com/DiscoveryFox/n0te/issues/new";

/// The folder with the reports.
pub fn dir() -> PathBuf {
    n0te_platform::paths::cache_dir().join("crashes")
}

/// Writes a report for every panic from now on (then the usual panic output follows).
pub fn install(program: &'static str) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let what = match info.payload().downcast_ref::<&str>() {
            Some(s) => s.to_string(),
            None => info.payload().downcast_ref::<String>().cloned().unwrap_or_else(|| "(no message)".into()),
        };
        let at = info.location().map(|l| format!("{}:{}", l.file(), l.line())).unwrap_or_default();
        let thread = std::thread::current().name().unwrap_or("unnamed").to_string();
        let backtrace = std::backtrace::Backtrace::force_capture().to_string();
        // Never panic inside the hook: a failed write is simply lost.
        let _ = save(&dir(), &compose(program, &what, &at, &thread, &backtrace), now_ms());
        previous(info);
    }));
}

fn now_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

/// The report's text.
pub fn compose(program: &str, what: &str, at: &str, thread: &str, backtrace: &str) -> String {
    format!(
        "{program} {} ({} {})\nthread '{thread}' panicked at {at}:\n{what}\n\nbacktrace:\n{backtrace}\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

/// Stores a report as `crash-<ms>.txt` and keeps only the newest [`KEEP`].
pub fn save(dir: &Path, text: &str, at_ms: u128) -> std::io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(format!("crash-{at_ms}.txt"));
    std::fs::write(&path, text)?;
    let mut all = reports(dir);
    while all.len() > KEEP {
        let (old, _) = all.remove(0);
        let _ = std::fs::remove_file(old);
    }
    Ok(path)
}

/// Every report in `dir`, oldest first, with whether it was shown already.
fn reports(dir: &Path) -> Vec<(PathBuf, bool)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<(u128, PathBuf, bool)> = entries
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            let rest = name.strip_prefix("crash-")?;
            let (stamp, seen) = match rest.strip_suffix(".seen.txt") {
                Some(s) => (s, true),
                None => (rest.strip_suffix(".txt")?, false),
            };
            Some((stamp.parse().ok()?, e.path(), seen))
        })
        .collect();
    out.sort_by_key(|(t, _, _)| *t);
    out.into_iter().map(|(_, p, s)| (p, s)).collect()
}

/// The newest report that hasn't been shown yet, with its text.
pub fn unseen(dir: &Path) -> Option<(PathBuf, String)> {
    let (path, _) = reports(dir).into_iter().rev().find(|(_, seen)| !seen)?;
    let text = std::fs::read_to_string(&path).ok()?;
    Some((path, text))
}

/// Marks every report as shown (they stay for a later look in the folder).
pub fn mark_seen(dir: &Path) {
    for (path, seen) in reports(dir) {
        if !seen {
            let _ = std::fs::rename(&path, path.with_extension("seen.txt"));
        }
    }
}

/// The first line of what went wrong (the banner and the issue title).
pub fn summary(report: &str) -> String {
    let what = report.lines().skip_while(|l| !l.starts_with("thread '")).nth(1).unwrap_or("").trim();
    what.chars().take(120).collect()
}

/// A new issue on GitHub with the version and the summary as title (the user adds the rest
/// and pastes the copied report themselves).
pub fn issue_url(report: &str) -> String {
    let first = report.lines().next().unwrap_or("n0te");
    let title = format!("Crash: {} ({first})", summary(report));
    let enc: String = title
        .bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect();
    format!("{ISSUES}?title={enc}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_are_kept_shown_once_and_pruned() {
        let dir = std::env::temp_dir().join(format!("n0te-crash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        assert!(unseen(&dir).is_none(), "no folder, no report");
        for i in 0..7u128 {
            save(&dir, &compose("n0te", &format!("boom {i}"), "src/app.rs:1", "main", "bt"), 1000 + i).unwrap();
        }
        assert_eq!(reports(&dir).len(), KEEP, "only the newest five stay");
        let (_, text) = unseen(&dir).unwrap();
        assert_eq!(summary(&text), "boom 6");
        assert!(text.starts_with(&format!("n0te {}", env!("CARGO_PKG_VERSION"))));
        mark_seen(&dir);
        assert!(unseen(&dir).is_none());
        assert_eq!(reports(&dir).len(), KEEP, "shown reports stay in the folder");
        save(&dir, &compose("n0te", "again", "x", "main", "bt"), 2000).unwrap();
        assert_eq!(summary(&unseen(&dir).unwrap().1), "again");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_issue_link_is_a_safe_url() {
        let r = compose("n0te", "index out of bounds: the len is 0 & ü", "a.rs:3", "main", "");
        let url = issue_url(&r);
        assert!(url.starts_with(ISSUES));
        assert!(url.bytes().all(|b| b.is_ascii_graphic()), "{url}");
        assert!(url.contains("index%20out%20of%20bounds"));
    }
}
