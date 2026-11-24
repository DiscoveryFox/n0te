//! Where n0te keeps things. Linux (and every other Unix) follows the XDG base directories
//! exactly as n0te always did; Windows uses the user's known folders: roaming AppData for the
//! library and the settings, local AppData for caches and the service's scratch files.
//!
//! | What | Linux | Windows |
//! |---|---|---|
//! | [`library_root`] | `$N0TE_LIBRARY`, else `$XDG_DATA_HOME/n0te` (`~/.local/share/n0te`) | `$N0TE_LIBRARY`, else `%APPDATA%\n0te` |
//! | [`config_dir`] | `$XDG_CONFIG_HOME/n0te` (`~/.config/n0te`) | `%APPDATA%\n0te\config` |
//! | [`cache_dir`] | `$XDG_CACHE_HOME/n0te` (`~/.cache/n0te`) | `%LOCALAPPDATA%\n0te\cache` |
//! | [`runtime_dir`] | `$XDG_RUNTIME_DIR`, else `<library>/sync` | `%LOCALAPPDATA%\n0te\run` |
//! | [`downloads`] | `~/Downloads` | the Downloads known folder, else `%USERPROFILE%\Downloads` |

use std::path::PathBuf;

fn var(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

/// The user's home folder: `$HOME`; Windows: the profile folder (`%USERPROFILE%`).
pub fn home() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        crate::win::known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Profile).or_else(|| var("USERPROFILE"))
    }
    #[cfg(not(windows))]
    {
        var("HOME")
    }
}

/// The n0te library: `$N0TE_LIBRARY`, else `$XDG_DATA_HOME/n0te` (`~/.local/share/n0te`);
/// Windows: `%APPDATA%\n0te`. The sync service and the apps must agree on it.
pub fn library_root() -> PathBuf {
    if let Some(p) = var("N0TE_LIBRARY") {
        return p;
    }
    #[cfg(windows)]
    {
        roaming().join("n0te")
    }
    #[cfg(not(windows))]
    {
        var("XDG_DATA_HOME").or_else(|| home().map(|h| h.join(".local/share"))).unwrap_or_else(|| PathBuf::from(".")).join("n0te")
    }
}

/// Settings of the apps: `$XDG_CONFIG_HOME/n0te` (`~/.config/n0te`); Windows:
/// `%APPDATA%\n0te\config` (the library ignores folders it doesn't know).
pub fn config_dir() -> PathBuf {
    #[cfg(windows)]
    {
        roaming().join("n0te").join("config")
    }
    #[cfg(not(windows))]
    {
        var("XDG_CONFIG_HOME").or_else(|| home().map(|h| h.join(".config"))).unwrap_or_else(|| PathBuf::from(".")).join("n0te")
    }
}

/// Things that can be made again (rendered formulas …): `$XDG_CACHE_HOME/n0te`
/// (`~/.cache/n0te`); Windows: `%LOCALAPPDATA%\n0te\cache`.
pub fn cache_dir() -> PathBuf {
    #[cfg(windows)]
    {
        local().join("n0te").join("cache")
    }
    #[cfg(not(windows))]
    {
        var("XDG_CACHE_HOME").unwrap_or_else(|| home().unwrap_or_default().join(".cache")).join("n0te")
    }
}

/// The sync service's private scratch folder: its socket (Unix), pictures from devices
/// (`n0te-sync screenshot`), its log (Windows). `$XDG_RUNTIME_DIR`, else `<library>/sync`;
/// Windows: `%LOCALAPPDATA%\n0te\run`. Not created here.
pub fn runtime_dir() -> PathBuf {
    #[cfg(windows)]
    {
        local().join("n0te").join("run")
    }
    #[cfg(not(windows))]
    {
        var("XDG_RUNTIME_DIR").unwrap_or_else(|| library_root().join("sync"))
    }
}

/// The Downloads folder: `~/Downloads`; Windows: the Downloads known folder (wherever the
/// user moved it), else `%USERPROFILE%\Downloads`.
pub fn downloads() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        crate::win::known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Downloads).or_else(|| home().map(|h| h.join("Downloads")))
    }
    #[cfg(not(windows))]
    {
        home().map(|h| h.join("Downloads"))
    }
}

/// Documents: `~/Documents`; Windows: the Documents known folder (often in OneDrive).
pub fn documents() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        crate::win::known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Documents).or_else(|| home().map(|h| h.join("Documents")))
    }
    #[cfg(not(windows))]
    {
        home().map(|h| h.join("Documents"))
    }
}

/// The desktop folder: `~/Desktop`; Windows: the Desktop known folder (often in OneDrive).
pub fn desktop() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        crate::win::known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_Desktop).or_else(|| home().map(|h| h.join("Desktop")))
    }
    #[cfg(not(windows))]
    {
        home().map(|h| h.join("Desktop"))
    }
}

#[cfg(windows)]
fn roaming() -> PathBuf {
    crate::win::known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_RoamingAppData).or_else(|| var("APPDATA")).unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(windows)]
fn local() -> PathBuf {
    crate::win::known_folder(&windows_sys::Win32::UI::Shell::FOLDERID_LocalAppData).or_else(|| var("LOCALAPPDATA")).unwrap_or_else(roaming)
}

#[cfg(all(test, unix))]
mod tests {
    #[test]
    fn linux_folders_are_the_ones_n0te_always_used() {
        // Only reads the environment (the folders aren't touched).
        if std::env::var_os("N0TE_LIBRARY").is_none()
            && std::env::var_os("XDG_DATA_HOME").is_none()
            && let Some(h) = std::env::var_os("HOME")
        {
            assert_eq!(super::library_root(), std::path::Path::new(&h).join(".local/share/n0te"));
        }
        if std::env::var_os("XDG_CONFIG_HOME").is_none()
            && let Some(h) = std::env::var_os("HOME")
        {
            assert_eq!(super::config_dir().join("laptop.conf"), std::path::Path::new(&h).join(".config/n0te/laptop.conf"));
        }
        if let Some(r) = std::env::var_os("XDG_RUNTIME_DIR") {
            assert_eq!(super::runtime_dir(), std::path::Path::new(&r));
        }
    }
}
