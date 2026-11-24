//! File permissions.

use std::io;
use std::path::Path;

/// Lets only this user read and write `path` (keys, pictures from devices). Unix: mode 0600.
/// Windows: a protected access list with just this user and SYSTEM (nothing inherited).
pub fn make_private(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
    }
    #[cfg(windows)]
    {
        windows::make_private(path)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = path;
        Ok(())
    }
}

#[cfg(windows)]
mod windows {
    use std::io;
    use std::path::Path;
    use std::ptr::null_mut;

    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Authorization::{ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1, SE_FILE_OBJECT, SetNamedSecurityInfoW};
    use windows_sys::Win32::Security::{ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR};

    use crate::win::{current_user, wide};

    pub(super) fn make_private(path: &Path) -> io::Result<()> {
        let me = current_user()?;
        // Full access for this user and SYSTEM; "P": nothing inherited from the folder.
        let sddl = wide(format!("D:P(A;;FA;;;{})(A;;FA;;;SY)", me.text));
        let mut sd: PSECURITY_DESCRIPTOR = null_mut();
        if unsafe { ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl.as_ptr(), SDDL_REVISION_1, &mut sd, null_mut()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        let result = (|| {
            let (mut present, mut defaulted) = (0, 0);
            let mut dacl: *mut ACL = null_mut();
            if unsafe { GetSecurityDescriptorDacl(sd, &mut present, &mut dacl, &mut defaulted) } == 0 {
                return Err(io::Error::last_os_error());
            }
            let name = wide(path);
            let err = unsafe { SetNamedSecurityInfoW(name.as_ptr(), SE_FILE_OBJECT, DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION, null_mut(), null_mut(), dacl, null_mut()) };
            if err != 0 { Err(io::Error::from_raw_os_error(err as i32)) } else { Ok(()) }
        })();
        unsafe { LocalFree(sd) };
        result
    }
}
