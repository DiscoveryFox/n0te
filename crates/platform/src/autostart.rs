//! Programs started at login. Windows: values under
//! `HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Run` (no admin rights needed).
//! Elsewhere these fail with [`io::ErrorKind::Unsupported`]: systemd user units (the
//! n0te-sync service) or the desktop's autostart folder do that there.

use std::ffi::OsStr;
use std::io;

/// Starts `command_line` (program in quotes, then its arguments) at every login as `name`.
pub fn enable(name: &str, command_line: &OsStr) -> io::Result<()> {
    #[cfg(windows)]
    {
        windows::enable(name, command_line)
    }
    #[cfg(not(windows))]
    {
        let _ = (name, command_line);
        Err(unsupported())
    }
}

/// No longer starts `name` at login (fine when it didn't).
pub fn disable(name: &str) -> io::Result<()> {
    #[cfg(windows)]
    {
        windows::disable(name)
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        Err(unsupported())
    }
}

/// The command line `name` starts at login, if it does.
pub fn get(name: &str) -> io::Result<Option<std::ffi::OsString>> {
    #[cfg(windows)]
    {
        windows::get(name)
    }
    #[cfg(not(windows))]
    {
        let _ = name;
        Err(unsupported())
    }
}

#[cfg(not(windows))]
fn unsupported() -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, "autostart: only on Windows")
}

#[cfg(windows)]
mod windows {
    use std::ffi::{OsStr, OsString};
    use std::io;
    use std::os::windows::ffi::OsStringExt;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegGetValueW, RegOpenKeyExW, RegSetValueExW,
    };

    use crate::win::wide;

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

    fn check(err: u32) -> io::Result<()> {
        if err == 0 { Ok(()) } else { Err(io::Error::from_raw_os_error(err as i32)) }
    }

    struct Key(HKEY);
    impl Drop for Key {
        fn drop(&mut self) {
            unsafe { RegCloseKey(self.0) };
        }
    }

    pub(super) fn enable(name: &str, command_line: &OsStr) -> io::Result<()> {
        let mut key: HKEY = null_mut();
        check(unsafe { RegCreateKeyExW(HKEY_CURRENT_USER, wide(RUN).as_ptr(), 0, null(), REG_OPTION_NON_VOLATILE, KEY_SET_VALUE, null(), &mut key, null_mut()) })?;
        let key = Key(key);
        let data = wide(command_line);
        check(unsafe { RegSetValueExW(key.0, wide(name).as_ptr(), 0, REG_SZ, data.as_ptr().cast(), (data.len() * 2) as u32) })
    }

    pub(super) fn disable(name: &str) -> io::Result<()> {
        let mut key: HKEY = null_mut();
        match unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, wide(RUN).as_ptr(), 0, KEY_SET_VALUE, &mut key) } {
            0 => {}
            ERROR_FILE_NOT_FOUND => return Ok(()),
            err => return check(err),
        }
        let key = Key(key);
        match unsafe { RegDeleteValueW(key.0, wide(name).as_ptr()) } {
            0 | ERROR_FILE_NOT_FOUND => Ok(()),
            err => check(err),
        }
    }

    pub(super) fn get(name: &str) -> io::Result<Option<OsString>> {
        let (run, name) = (wide(RUN), wide(name));
        let mut size = 0u32;
        match unsafe { RegGetValueW(HKEY_CURRENT_USER, run.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, null_mut(), null_mut(), &mut size) } {
            0 => {}
            ERROR_FILE_NOT_FOUND => return Ok(None),
            err => return check(err).map(|_| None),
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2)];
        check(unsafe { RegGetValueW(HKEY_CURRENT_USER, run.as_ptr(), name.as_ptr(), RRF_RT_REG_SZ, null_mut(), buf.as_mut_ptr().cast(), &mut size) })?;
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Ok(Some(OsString::from_wide(&buf[..len])))
    }
}
