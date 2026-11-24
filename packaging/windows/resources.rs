//! Windows resources of the n0te programs: icon, version info (what Explorer and Task Manager
//! show) and the application manifest (packaging/windows/n0te.manifest). Included with `#[path]`
//! by the build scripts of n0te-desktop and n0te-sync.
//!
//! A no-op for every target but Windows. Building for Windows on a non-Windows host (`cargo check`
//! from Linux, scripts/wincheck.sh) works without a resource compiler: it says so in a warning and
//! goes on. On a Windows host a failure stops the build, so no exe ships without its manifest.

use std::path::{Path, PathBuf};

/// One program (a Cargo binary) and how Windows should present it.
pub struct Program<'a> {
    /// The Cargo binary name.
    pub bin: &'a str,
    /// File description: Task Manager shows it as the process name.
    pub description: &'a str,
    /// The exe's name as shipped (installer, zip).
    pub file: &'a str,
    /// The icon, relative to the n0te repo root.
    pub icon: &'a str,
}

/// Compiles a resource script per program and links it into that binary only.
pub fn embed(programs: &[Program]) {
    println!("cargo:rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let crate_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let repo = crate_dir.join("../..");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let version = std::env::var("CARGO_PKG_VERSION").expect("CARGO_PKG_VERSION");
    let manifest = repo.join("packaging/windows/n0te.manifest");
    println!("cargo:rerun-if-changed={}", manifest.display());
    println!("cargo:rerun-if-changed={}", repo.join("packaging/windows/resources.rs").display());
    for p in programs {
        let icon = repo.join(p.icon);
        println!("cargo:rerun-if-changed={}", icon.display());
        let rc = out.join(format!("{}.rc", p.bin));
        std::fs::write(&rc, script(p, &version, &icon, &manifest)).expect("write .rc");
        match embed_resource::compile_for(&rc, [p.bin], embed_resource::NONE) {
            embed_resource::CompilationResult::Ok | embed_resource::CompilationResult::NotWindows => {}
            other if cfg!(windows) => panic!("Windows resources for {}: {other}", p.bin),
            other => println!("cargo:warning=no Windows resources for {} (icon, manifest): {other}", p.bin),
        }
    }
}

/// The resource script. No #include (works without the Windows SDK headers): the numbers are
/// RT_MANIFEST (24), VOS_NT_WINDOWS32 (0x40004) and VFT_APP (1).
fn script(p: &Program, version: &str, icon: &Path, manifest: &Path) -> String {
    let nums: Vec<u32> = version.split(['-', '+']).next().unwrap_or("0").split('.').map(|n| n.parse().unwrap_or(0)).chain([0; 4]).take(4).collect();
    let num = format!("{},{},{},{}", nums[0], nums[1], nums[2], nums[3]);
    let path = |p: &Path| p.display().to_string().replace('\\', "/");
    let internal = p.file.trim_end_matches(".exe");
    format!(
        r#"1 ICON "{icon}"
1 24 "{manifest}"
1 VERSIONINFO
FILEVERSION {num}
PRODUCTVERSION {num}
FILEFLAGSMASK 0x3F
FILEFLAGS 0x0
FILEOS 0x40004
FILETYPE 0x1
FILESUBTYPE 0x0
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "DiscoveryFox"
      VALUE "FileDescription", "{description}"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "{internal}"
      VALUE "LegalCopyright", "Copyright (C) 2026 DiscoveryFox. All rights reserved."
      VALUE "OriginalFilename", "{file}"
      VALUE "ProductName", "n0te"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        icon = path(icon),
        manifest = path(manifest),
        description = p.description,
        file = p.file,
    )
}
