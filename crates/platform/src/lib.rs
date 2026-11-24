//! What differs between Linux (and Android) and Windows for n0te, so the other crates stay
//! free of `cfg`s:
//!
//! - [`paths`]: where the library, settings, caches and the service's scratch files live;
//! - [`ipc`]: the local connection between the n0te-sync service and the apps (Unix socket,
//!   Windows named pipe);
//! - [`open_path`]: a file, folder or link opened the way the desktop does it;
//! - [`local_time`]: "YYYY-MM-DD HH:MM" in local time;
//! - [`spawn_detached`] and [`no_window`]: programs started from the apps;
//! - [`fs::make_private`]: a file only this user can read;
//! - [`autostart`] and [`attach_parent_console`]: the service at login (Windows).
//!
//! Linux behaviour is exactly what n0te did before this crate existed.

pub mod autostart;
pub mod fs;
pub mod ipc;
pub mod paths;
#[cfg(windows)]
mod win;

use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

/// Opens a file, folder or URL with the program the desktop picks for it (Linux `xdg-open`,
/// Windows the shell's "open"). Returns once it was handed over, not when the program exits.
pub fn open_path(target: impl AsRef<OsStr>) -> io::Result<()> {
    #[cfg(windows)]
    {
        win::shell_open(target.as_ref())
    }
    #[cfg(not(windows))]
    {
        let opener = if cfg!(target_os = "macos") { "open" } else { "xdg-open" };
        let mut child = Command::new(opener).arg(target.as_ref()).spawn()?;
        // Reaped in the background, so no zombie stays behind.
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
}

/// Starts `program args…` in the background, on its own: no console window, not stopped
/// with this process or its terminal (Windows: detached, own process group, outside this
/// process's job where the job allows it). Its working folder is the program's folder.
pub fn spawn_detached(program: &Path, args: &[&str]) -> io::Result<()> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(dir) = program.parent().filter(|d| !d.as_os_str().is_empty()) {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows_sys::Win32::System::Threading::{CREATE_BREAKAWAY_FROM_JOB, CREATE_NEW_PROCESS_GROUP, DETACHED_PROCESS};
        let flags = DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
        // Leaving the job fails when the job forbids it: then start inside it.
        cmd.creation_flags(flags | CREATE_BREAKAWAY_FROM_JOB);
        if cmd.spawn().is_ok() {
            return Ok(());
        }
        cmd.creation_flags(flags);
        cmd.spawn().map(drop)
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
        let mut child = cmd.spawn()?;
        std::thread::spawn(move || {
            let _ = child.wait();
        });
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    {
        cmd.spawn().map(drop)
    }
}

/// A console program started from a windowed app (Windows: without flashing a console
/// window, `CREATE_NO_WINDOW`; elsewhere unchanged).
pub fn no_window(cmd: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);
    }
    cmd
}

/// Windows: a program built without a console (`windows_subsystem = "windows"`) that runs as a
/// command in a terminal writes its output there: it borrows the terminal's console unless
/// its output already goes somewhere (redirected). Elsewhere nothing to do.
pub fn attach_parent_console() {
    #[cfg(windows)]
    win::attach_parent_console();
}

/// "YYYY-MM-DD HH:MM" in local time for unix seconds (UTC with " UTC" when the local time
/// zone can't be read).
pub fn local_time(unix: u64) -> String {
    #[cfg(unix)]
    unsafe {
        let t = unix as libc::time_t;
        let mut tm: libc::tm = std::mem::zeroed();
        if !libc::localtime_r(&t, &mut tm).is_null() {
            return format!("{:04}-{:02}-{:02} {:02}:{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min);
        }
    }
    #[cfg(windows)]
    if let Some((y, mo, d, h, mi)) = win::local_time(unix) {
        return format!("{y:04}-{mo:02}-{d:02} {h:02}:{mi:02}");
    }
    // Fallback: UTC.
    let days = unix / 86_400;
    let (h, m) = ((unix % 86_400) / 3600, (unix % 3600) / 60);
    let (y, mo, d) = civil(days as i64);
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02} UTC")
}

/// Days since 1970-01-01 → (year, month, day) (H. Hinnant's algorithm).
fn civil(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_dates() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_411), (2025, 11, 19));
    }

    #[test]
    fn local_time_has_the_date_and_minutes() {
        // 2025-11-19 12:00 UTC: whatever the zone, still that year and month, as "YYYY-MM-DD HH:MM".
        let s = local_time(20_411 * 86_400 + 12 * 3600);
        assert!(s.starts_with("2025-11-"), "{s}");
        assert_eq!(&s[10..11], " ");
        assert_eq!(s[11..16].chars().filter(|c| *c == ':').count(), 1, "{s}");
    }
}
