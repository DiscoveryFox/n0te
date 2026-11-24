//! Small Win32 helpers shared by the Windows parts of this crate.

use std::ffi::{OsStr, OsString};
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;

use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE, LocalFree, SYSTEMTIME};
use windows_sys::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows_sys::Win32::Security::{GetLengthSid, GetTokenInformation, TOKEN_QUERY, TOKEN_USER, TokenUser};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_OUTPUT_HANDLE};
use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION};
use windows_sys::Win32::System::Time::{FileTimeToSystemTime, SystemTimeToTzSpecificLocalTime};
use windows_sys::Win32::UI::Shell::{KF_FLAG_DEFAULT, SHGetKnownFolderPath, ShellExecuteW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows_sys::core::{GUID, PWSTR};

/// `s` as a NUL-terminated UTF-16 string.
pub(crate) fn wide(s: impl AsRef<OsStr>) -> Vec<u16> {
    s.as_ref().encode_wide().chain(Some(0)).collect()
}

/// A NUL-terminated UTF-16 string at `p`.
///
/// # Safety
/// `p` points to a NUL-terminated UTF-16 string.
pub(crate) unsafe fn from_wide(p: *const u16) -> OsString {
    let mut n = 0;
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    OsString::from_wide(unsafe { std::slice::from_raw_parts(p, n) })
}

/// A kernel handle, closed when dropped.
pub(crate) struct Handle(pub(crate) HANDLE);

impl Handle {
    /// `None` for the null and the invalid handle.
    pub(crate) fn new(h: HANDLE) -> Option<Handle> {
        (!h.is_null() && h != INVALID_HANDLE_VALUE).then_some(Handle(h))
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// A folder the shell knows (`FOLDERID_…`), where the user has it now.
pub(crate) fn known_folder(id: &GUID) -> Option<PathBuf> {
    let mut p: PWSTR = null_mut();
    let hr = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DEFAULT as u32, null_mut(), &mut p) };
    let path = (hr >= 0 && !p.is_null()).then(|| PathBuf::from(unsafe { from_wide(p) }));
    // Freed even when the call failed (the documentation asks for it).
    unsafe { CoTaskMemFree(p as *const _) };
    path.filter(|p| !p.as_os_str().is_empty())
}

/// The shell's "open" for a file, folder or URL.
pub(crate) fn shell_open(target: &OsStr) -> io::Result<()> {
    let (verb, file) = (wide("open"), wide(target));
    let r = unsafe { ShellExecuteW(null_mut(), verb.as_ptr(), file.as_ptr(), null(), null(), SW_SHOWNORMAL) };
    // Values above 32 mean success.
    if r as usize > 32 { Ok(()) } else { Err(io::Error::other(format!("ShellExecute: Fehler {}", r as usize))) }
}

/// A user account: its SID as bytes (to compare) and as text (`S-1-5-21-…`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct User {
    pub(crate) sid: Vec<u8>,
    pub(crate) text: String,
}

/// The user this process runs as.
pub(crate) fn current_user() -> io::Result<&'static User> {
    static ME: OnceLock<User> = OnceLock::new();
    if let Some(u) = ME.get() {
        return Ok(u);
    }
    let mut token: HANDLE = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let token = Handle(token);
    let user = token_user(&token)?;
    Ok(ME.get_or_init(|| user))
}

/// The user process `pid` runs as.
pub(crate) fn process_user(pid: u32) -> io::Result<User> {
    let process = Handle::new(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) }).ok_or_else(io::Error::last_os_error)?;
    let mut token: HANDLE = null_mut();
    if unsafe { OpenProcessToken(process.0, TOKEN_QUERY, &mut token) } == 0 {
        return Err(io::Error::last_os_error());
    }
    token_user(&Handle(token))
}

fn token_user(token: &Handle) -> io::Result<User> {
    let mut len = 0u32;
    // The first call only asks for the size.
    unsafe { GetTokenInformation(token.0, TokenUser, null_mut(), 0, &mut len) };
    if len == 0 {
        return Err(io::Error::last_os_error());
    }
    // u64s keep the TOKEN_USER and the SID behind it aligned.
    let mut buf = vec![0u64; (len as usize).div_ceil(8)];
    if unsafe { GetTokenInformation(token.0, TokenUser, buf.as_mut_ptr().cast(), len, &mut len) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let sid = unsafe { (*buf.as_ptr().cast::<TOKEN_USER>()).User.Sid };
    let n = unsafe { GetLengthSid(sid) } as usize;
    let bytes = unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), n) }.to_vec();
    let mut s: PWSTR = null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut s) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let text = unsafe { from_wide(s) }.to_string_lossy().into_owned();
    unsafe { LocalFree(s.cast()) };
    Ok(User { sid: bytes, text })
}

/// Unix seconds → local (year, month, day, hour, minute), with the daylight saving rules of
/// that date.
pub(crate) fn local_time(unix: u64) -> Option<(u16, u16, u16, u16, u16)> {
    // FILETIME: 100 ns steps since 1601-01-01.
    let ticks = unix.checked_add(11_644_473_600)?.checked_mul(10_000_000)?;
    let ft = FILETIME { dwLowDateTime: ticks as u32, dwHighDateTime: (ticks >> 32) as u32 };
    let mut utc: SYSTEMTIME = unsafe { std::mem::zeroed() };
    let mut local: SYSTEMTIME = unsafe { std::mem::zeroed() };
    if unsafe { FileTimeToSystemTime(&ft, &mut utc) } == 0 || unsafe { SystemTimeToTzSpecificLocalTime(null(), &utc, &mut local) } == 0 {
        return None;
    }
    Some((local.wYear, local.wMonth, local.wDay, local.wHour, local.wMinute))
}

/// Borrows the parent's console when standard output goes nowhere (see
/// [`crate::attach_parent_console`]).
pub(crate) fn attach_parent_console() {
    let out = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if out.is_null() || out == INVALID_HANDLE_VALUE {
        unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
    }
}
