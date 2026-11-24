//! Updates for the Windows programs, from n0te's GitHub releases: install once, then every new
//! version arrives by itself. A while after start (and every few hours) the app looks for a
//! newer release, downloads the installer for this PC (x64 / ARM64) in the background and checks
//! its SHA-256 against the release's. Then it installs at the next start, or at once with
//! "Neu starten": the installer runs silently (`/S /UPDATE`), keeps the programs that were
//! installed, and starts the app again (`/RELAUNCH=`). A portable copy (not where the installer
//! put n0te) is never changed; it only says that a new version is out.
//!
//! The repo is [`DEFAULT_REPO`] unless the settings name another; updates can be switched off.
//! GitHub only shows releases of a public repo without logging in.

use serde_json::Value;

/// Where n0te's releases are.
pub const DEFAULT_REPO: &str = "DiscoveryFox/n0te";
/// This program's version (the suite version).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// What the updater is doing / found.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// Nothing yet (or switched off / not Windows).
    Idle,
    Checking,
    UpToDate,
    /// The installer of this version is downloaded and checked: ready to install.
    Ready(String),
    /// A portable copy: a newer version is out (release page).
    Available(String, String),
    Failed(String),
}

/// A release that fits: its version, the installer's name and URL, its SHA-256 (if GitHub gave
/// one), and the release page.
#[derive(Debug, Clone, PartialEq)]
pub struct Release {
    pub version: String,
    pub asset: String,
    pub url: String,
    pub sha256: Option<String>,
    pub page: String,
}

/// Whether the updater runs on this system at all.
pub fn supported() -> bool {
    cfg!(windows)
}

/// This PC's installer architecture.
pub fn arch() -> &'static str {
    if cfg!(target_arch = "aarch64") { "arm64" } else { "x64" }
}

fn version_parts(s: &str) -> Option<Vec<u64>> {
    let s = s.trim().trim_start_matches(|c: char| !c.is_ascii_digit());
    let core = s.split(['-', '+']).next()?;
    let parts: Vec<u64> = core.split('.').map(|p| p.parse().ok()).collect::<Option<_>>()?;
    (!parts.is_empty()).then_some(parts)
}

/// Whether version `a` is newer than `b` ("0.33.0" > "0.32.1"; "v0.32.1" works too).
pub fn newer(a: &str, b: &str) -> bool {
    let (Some(x), Some(y)) = (version_parts(a), version_parts(b)) else { return false };
    for i in 0..x.len().max(y.len()) {
        let (p, q) = (x.get(i).copied().unwrap_or(0), y.get(i).copied().unwrap_or(0));
        if p != q {
            return p > q;
        }
    }
    false
}

/// The newest release newer than `current` with an installer for `arch`, from the GitHub API's
/// list of releases. Drafts and pre-releases never.
pub fn pick(releases: &Value, current: &str, arch: &str) -> Option<Release> {
    let mut best: Option<Release> = None;
    for r in releases.as_array()? {
        if r["draft"].as_bool().unwrap_or(false) || r["prerelease"].as_bool().unwrap_or(false) {
            continue;
        }
        let tag = r["tag_name"].as_str().unwrap_or_default();
        let Some(version) = version_parts(tag).map(|v| v.iter().map(u64::to_string).collect::<Vec<_>>().join(".")) else { continue };
        if !newer(&version, current) || best.as_ref().is_some_and(|b| !newer(&version, &b.version)) {
            continue;
        }
        let want = format!("n0te-{version}-windows-{arch}-setup.exe");
        let Some(a) = r["assets"].as_array().and_then(|a| a.iter().find(|a| a["name"].as_str() == Some(want.as_str()))) else { continue };
        let Some(url) = a["browser_download_url"].as_str() else { continue };
        let sha256 = a["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).map(str::to_lowercase);
        best = Some(Release { version, asset: want, url: url.to_string(), sha256, page: r["html_url"].as_str().unwrap_or_default().to_string() });
    }
    best
}

/// The SHA-256 of `file` in a SHA256SUMS.txt text ("<hex>  <name>" lines).
pub fn sum_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, name) = l.trim().split_once(char::is_whitespace)?;
        (name.trim().trim_start_matches('*') == file).then(|| hash.to_lowercase())
    })
}

/// Before the window opens: a downloaded, newer update installs now (the installer starts this
/// program again afterwards). True: the installer runs, the caller ends the program.
pub fn apply_pending(relaunch: &str) -> bool {
    #[cfg(windows)]
    {
        win::apply_pending(relaunch)
    }
    #[cfg(not(windows))]
    {
        let _ = relaunch;
        false
    }
}

/// "Neu starten": installs the ready update now and starts `relaunch` (laptop | tablet) again.
/// True: the installer runs, the caller ends the program.
pub fn install_now(relaunch: &str) -> bool {
    apply_pending(relaunch)
}

/// Looks for an update and downloads it (blocking: run it on a worker thread).
pub fn check(repo: &str) -> Status {
    #[cfg(windows)]
    {
        win::check(repo)
    }
    #[cfg(not(windows))]
    {
        let _ = repo;
        Status::Idle
    }
}

#[cfg(windows)]
mod win {
    use super::{Release, Status, VERSION, arch, newer, pick, sum_for};
    use sha2::{Digest, Sha256};
    use std::io::Read;
    use std::path::{Path, PathBuf};

    fn dir() -> PathBuf {
        n0te_platform::paths::cache_dir().with_file_name("updates")
    }

    fn pending_file() -> PathBuf {
        dir().join("pending.txt")
    }

    /// Where the installer put n0te (HKCU\Software\n0te InstallDir), if it did.
    fn install_dir() -> Option<PathBuf> {
        use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_SZ, RegGetValueW};
        let key: Vec<u16> = "Software\\n0te".encode_utf16().chain([0]).collect();
        let val: Vec<u16> = "InstallDir".encode_utf16().chain([0]).collect();
        let mut buf = vec![0u16; 1024];
        let mut len = (buf.len() * 2) as u32;
        // SAFETY: buffers and lengths are ours.
        let r = unsafe { RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), val.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr().cast(), &mut len) };
        if r != 0 {
            return None;
        }
        let n = (len as usize / 2).saturating_sub(1);
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
    }

    /// This program runs from the installed copy (not a portable one).
    fn installed() -> bool {
        let (Some(dir), Ok(exe)) = (install_dir(), std::env::current_exe()) else { return false };
        let Some(here) = exe.parent() else { return false };
        let norm = |p: &Path| p.to_string_lossy().trim_end_matches('\\').to_lowercase();
        norm(here) == norm(&dir)
    }

    fn client() -> Result<reqwest::blocking::Client, String> {
        let _ = rustls::crypto::ring::default_provider().install_default();
        reqwest::blocking::Client::builder()
            .user_agent(concat!("n0te-updater/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .map_err(|e| e.to_string())
    }

    fn sha256_of(path: &Path) -> Result<String, String> {
        let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 16];
        loop {
            let n = f.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }

    pub fn check(repo: &str) -> Status {
        match run(repo) {
            Ok(s) => s,
            Err(e) => Status::Failed(e),
        }
    }

    fn run(repo: &str) -> Result<Status, String> {
        let c = client()?;
        let url = format!("https://api.github.com/repos/{repo}/releases?per_page=20");
        let resp = c.get(&url).header("Accept", "application/vnd.github+json").send().map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("GitHub: {}", resp.status()));
        }
        let json: serde_json::Value = serde_json::from_str(&resp.text().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        let Some(rel) = pick(&json, VERSION, arch()) else { return Ok(Status::UpToDate) };
        if !installed() {
            return Ok(Status::Available(rel.version, rel.page));
        }
        // Already downloaded?
        if let Some((v, path)) = read_pending()
            && v == rel.version
            && path.is_file()
        {
            return Ok(Status::Ready(v));
        }
        let sha = match &rel.sha256 {
            Some(s) => s.clone(),
            None => sums(&c, &json, &rel)?.ok_or("keine Prüfsumme in der Release")?,
        };
        std::fs::create_dir_all(dir()).map_err(|e| e.to_string())?;
        let dest = dir().join(&rel.asset);
        let part = dest.with_extension("part");
        let mut resp = c.get(&rel.url).send().map_err(|e| e.to_string())?;
        if !resp.status().is_success() {
            return Err(format!("Download: {}", resp.status()));
        }
        let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
        resp.copy_to(&mut out).map_err(|e| e.to_string())?;
        drop(out);
        let got = sha256_of(&part)?;
        if got != sha {
            let _ = std::fs::remove_file(&part);
            return Err("Prüfsumme stimmt nicht; Update verworfen".into());
        }
        std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
        std::fs::write(pending_file(), format!("{}\n{}\n", rel.version, dest.display())).map_err(|e| e.to_string())?;
        Ok(Status::Ready(rel.version))
    }

    /// The installer's SHA-256 from the release's SHA256SUMS.txt.
    fn sums(c: &reqwest::blocking::Client, json: &serde_json::Value, rel: &Release) -> Result<Option<String>, String> {
        let Some(r) = json.as_array().and_then(|a| a.iter().find(|r| r["assets"].as_array().is_some_and(|a| a.iter().any(|a| a["name"].as_str() == Some(rel.asset.as_str()))))) else { return Ok(None) };
        let Some(url) = r["assets"].as_array().and_then(|a| a.iter().find(|a| a["name"].as_str() == Some("SHA256SUMS.txt"))).and_then(|a| a["browser_download_url"].as_str()) else { return Ok(None) };
        let text = c.get(url).send().and_then(|r| r.text()).map_err(|e| e.to_string())?;
        Ok(sum_for(&text, &rel.asset))
    }

    fn read_pending() -> Option<(String, PathBuf)> {
        let text = std::fs::read_to_string(pending_file()).ok()?;
        let mut l = text.lines();
        Some((l.next()?.trim().to_string(), PathBuf::from(l.next()?.trim())))
    }

    pub fn apply_pending(relaunch: &str) -> bool {
        let Some((v, path)) = read_pending() else { return false };
        // Gone either way: a failed install must not start over at every launch.
        let _ = std::fs::remove_file(pending_file());
        if !newer(&v, VERSION) || !path.is_file() || !installed() {
            let _ = std::fs::remove_file(&path);
            return false;
        }
        let relaunch = format!("/RELAUNCH={relaunch}");
        n0te_platform::spawn_detached(&path, &["/S", "/UPDATE", &relaunch]).is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare() {
        assert!(newer("0.33.0", "0.32.9"));
        assert!(newer("v0.32.1", "0.32.0"));
        assert!(newer("1.0", "0.99.99"));
        assert!(!newer("0.32.0", "0.32.0"));
        assert!(!newer("0.32.0", "0.32.1"));
        assert!(!newer("x", "0.1"));
    }

    #[test]
    fn picks_the_newest_stable_installer_for_this_arch() {
        let json = serde_json::json!([
            { "tag_name": "v0.34.0", "draft": false, "prerelease": true, "html_url": "p34", "assets": [{ "name": "n0te-0.34.0-windows-x64-setup.exe", "browser_download_url": "u34" }] },
            { "tag_name": "v0.33.1", "draft": false, "prerelease": false, "html_url": "p331", "assets": [{ "name": "n0te-0.33.1-android.apk", "browser_download_url": "a" }] },
            { "tag_name": "v0.33.0", "draft": false, "prerelease": false, "html_url": "p33", "assets": [
                { "name": "n0te-0.33.0-windows-arm64-setup.exe", "browser_download_url": "arm", "digest": "sha256:ABCD" },
                { "name": "n0te-0.33.0-windows-x64-setup.exe", "browser_download_url": "x64", "digest": "sha256:EF01" }
            ] },
            { "tag_name": "v0.32.0", "draft": false, "prerelease": false, "html_url": "p32", "assets": [{ "name": "n0te-0.32.0-windows-x64-setup.exe", "browser_download_url": "old" }] }
        ]);
        let r = pick(&json, "0.32.0", "x64").expect("0.33.0");
        assert_eq!((r.version.as_str(), r.url.as_str(), r.sha256.as_deref(), r.page.as_str()), ("0.33.0", "x64", Some("ef01"), "p33"));
        assert_eq!(pick(&json, "0.32.0", "arm64").unwrap().url, "arm");
        assert!(pick(&json, "0.33.0", "x64").is_none(), "nothing newer (pre-releases never)");
    }

    #[test]
    fn checksum_lines() {
        let sums = "aa11  n0te-0.33.0-windows-x64-setup.exe\nBB22 *n0te-0.33.0-windows-arm64-setup.exe\n";
        assert_eq!(sum_for(sums, "n0te-0.33.0-windows-arm64-setup.exe").as_deref(), Some("bb22"));
        assert_eq!(sum_for(sums, "nope"), None);
    }
}
