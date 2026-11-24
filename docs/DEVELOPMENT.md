# Developing n0te

How the code is laid out, how to build and test each part, and how a release is made. For what
n0te is and does, see the [README](../README.md).

## Layout

| Path | What |
|---|---|
| `crates/core` | `n0te-core`: notebook tree (groups, sheets, side sheets left/right, boards), sheet documents (strokes, text boxes, images, plots), ink encoding and stroke outlines, page layout, search, on-disk library. Loro CRDTs, so every device holds a full copy and edits merge. |
| `crates/pdf` | `n0te-pdf`: pdfium (loaded at run time by path) for page sizes, tiles and text; `PdfWorker`, the one thread per app that does pdfium work in priority order (visible tiles first, stale generations dropped, LRU of open documents); export = pdfium assembles the pages, n0te's own writer appends the ink (incremental update, Flate streams, optional content group "n0te ink"). |
| `crates/desktop` | `n0te-desktop` (iced 0.14): two programs from one crate. Binary `n0te` = **n0te Laptop** (keyboard and mouse; `n0te-laptop.exe` on Windows), binary `n0te-tablet` = **n0te Tablet** (pen and touch, the port of the Android app to Windows pen devices, still showing the laptop interface). `Mode` in `src/lib.rs` tells them apart. |
| `crates/sync` | `n0te-sync`: sync engine over iroh (own devices after pairing, guests with signed invite links, live presence and strokes, history checkpoints with thinning and restore-as-copy) and the `n0te-sync` service/CLI. Every library write is merge-on-save under a per-notebook lock; the engine uses its own Loro peer id. |
| `crates/ffi` | `n0te-ffi`: UniFFI bindings of core, pdf and sync for the Android app (Kotlin). Hot paths pass packed little-endian f32 bytes. |
| `crates/uniffi-bindgen` | The bindgen CLI (`cargo run -p uniffi-bindgen -- generate …`). |
| `android/` | The Android app (package `dev.n0bridge.n0te`, Kotlin + Compose). Builds on its own (`cd android && gradle assembleRelease`) or as `:n0te` inside n0bridge's Android build. |
| `packaging/` | Linux desktop entry, MIME type, systemd user unit; `icons/` (SVG sources, `.ico`, `render.sh`); `windows/` (NSIS installer, portable zip, manifest, resource helper for the build scripts). |
| `scripts/` | Build helpers, see below. |
| `docs/` | This file, the README's screenshots. |
| `.github/workflows/` | CI and releases (GitHub Actions). |

## Requirements

- Rust via [rustup](https://rustup.rs) (stable). The scripts use `~/.cargo/bin/cargo`.
- pdfium for the PDF tests and every build that ships: `scripts/fetch-pdfium.sh` downloads the
  pinned, hash-checked prebuilt libraries into `third_party/pdfium/` (gitignored).
- Linux builds of n0te Laptop: `pkg-config`, Wayland and xkbcommon headers (`libwayland-dev
  libxkbcommon-dev` on Debian/Ubuntu).
- Formulas in n0te Laptop: a local TeX with `latex` and `dvisvgm` (TeX Live; Windows untested).
- Android: JDK 17 or newer, Gradle 9.9 (no wrapper in the repo), Android SDK 35, NDK
  27.2.12479018, `rustup target add aarch64-linux-android`.

## Build and test

```sh
scripts/fetch-pdfium.sh                       # linux-x64 + android-arm64 (win-x64, win-arm64 on request)
cargo test --workspace                        # all Rust tests (UI tests run headless, no window)
cargo run -p n0te-desktop --bin n0te          # n0te Laptop on your library (~/.local/share/n0te)
cargo run -p n0te-desktop --bin n0te-tablet   # n0te Tablet
cargo build --profile fast -p n0te-desktop    # optimised without fat LTO, for quick iterations
cargo run --release -p n0te-core --example bench
cargo run --release -p n0te-pdf --example pdf_bench -- third_party/pdfium/linux-x64/lib/libpdfium.so slides.pdf
```

Careful: `cargo run` of the apps works on your real library and talks to a running `n0te-sync`.
The UI tests and the screenshot example use temporary libraries.

`scripts/wincheck.sh` type-checks the workspace for Windows on Linux (`cargo check
--target x86_64-pc-windows-msvc` with stub C headers; needs `clang-cl`, `llvm-lib`, optionally
`llvm-rc` for the exe resources). Real Windows binaries come from the GitHub Actions runners.

Inside the n0bridge repository the Makefile has shortcuts: `make n0te-test`, `make n0te-apk`,
`make n0te-desktop` (installs n0te Laptop into `~/.local`), `make n0te-sync` (binary + systemd
user unit).

### Android

```sh
cd android && gradle assembleRelease          # → android/build/outputs/apk/release/n0te-release.apk
```

Gradle runs `scripts/build-android.sh` first: it builds `libn0te_ffi.so` for arm64 (NDK from
`$ANDROID_NDK_HOME`, default `~/Android/Sdk/ndk/27.2.12479018`), copies the pinned arm64
`libpdfium.so` next to it and generates the Kotlin bindings into `android/build/rust/`. The
release profile keeps symbols because uniffi-bindgen reads its metadata from them; the copy in
the APK is stripped. Emulator builds add x86_64: `gradle assembleRelease -Pn0te.abis="arm64-v8a x86_64"`.

Signing: local builds are signed with the laptop's debug key (`~/.android/debug.keystore`), so
they install over the app on the tablet. With `N0TE_SIGNING_STORE_FILE`,
`N0TE_SIGNING_STORE_PASSWORD`, `N0TE_SIGNING_KEY_ALIAS` and `N0TE_SIGNING_KEY_PASSWORD` set
(the release workflow does that from the repository's secrets) that key is used instead.

JVM binding tests (`gradle testReleaseUnitTest`) load a laptop build of the core: run
`cargo build --release -p n0te-ffi` first; they use `third_party/pdfium/linux-x64`.

### Windows

The two programs and the sync helper:

```sh
cargo build --release --target x86_64-pc-windows-msvc -p n0te-desktop -p n0te-sync --bins
```

on Windows (Visual Studio Build Tools; for ARM64 also LLVM's `clang` on `PATH`, ring needs it).
The build scripts (`crates/desktop/build.rs`, `crates/sync/build.rs`, shared code in
`packaging/windows/resources.rs`) embed each exe's icon, version info and manifest (per-monitor
DPI, UTF-8 code page, long paths). pdfium: `scripts/fetch-pdfium.sh win-x64` (or `win-arm64`);
the apps look for `pdfium.dll` next to their exe. The library lives in `%APPDATA%\n0te`.

Installer and portable zip, from a folder with the three exes (on Linux, with `nsis` installed):

```sh
packaging/windows/package.sh 0.32.0 x64 <exe dir> dist
```

→ `dist/n0te-0.32.0-windows-x64-setup.exe` and `dist/n0te-0.32.0-windows-x64-portable.zip`. The
installer (`packaging/windows/n0te.nsi`) installs per user without administrator rights into
`%LOCALAPPDATA%\Programs\n0te`: n0te Laptop and/or n0te Tablet, n0te-sync, Start menu entries,
`.n0te` files (open with n0te Laptop), `n0te://` links (n0te Tablet if installed), n0te-sync at
sign-in (`HKCU\…\Run`). The uninstaller removes all of that and leaves `%APPDATA%\n0te` alone.

## Pictures

- `scripts/screenshots.sh` regenerates `docs/screenshots/*.png` headless from
  `crates/desktop/examples/screenshots.rs` (temporary library, own HOME/config/cache/runtime
  folders, a stand-in for n0te-sync on its own socket). Needs wgpu (GPU or software Vulkan), a
  local TeX, python3 with Pillow for the size reduction.
- `packaging/icons/render.sh` renders the `.ico` files and window icons from the SVGs.

## Versions and releases

One version for everything: Android `versionName` = Cargo workspace version = the release tag
without `v`. `CHANGELOG.md` has a `## [X.Y.Z] - date` section per release; that section becomes
the GitHub release notes. The Android app keeps its own German list in
`android/src/main/java/dev/n0bridge/n0te/Changelog.kt` (shown once after each update).

### Cutting a release

1. `scripts/bump-version.sh 0.33.0` sets the workspace version, Android versionName, the next
   versionCode, updates `Cargo.lock` and adds a skeleton section to `CHANGELOG.md`.
2. Write the `CHANGELOG.md` section (no `TODO` left) and the `Changelog.kt` entry.
3. Commit, then tag and push:

   ```sh
   git commit -am "release: 0.33.0"
   git tag v0.33.0
   git push origin main v0.33.0
   ```

   (From n0bridge, where n0te lives in `n0te/`: publish the subtree first, then push the tag to
   the n0te repository, e.g. `git push n0te <subtree commit>:refs/tags/v0.33.0`.)
4. `release.yml` checks `scripts/version.sh check v0.33.0` (tag = Cargo = Android, finished
   changelog section), builds Windows x64 and ARM64, makes the installers and zips, builds the
   APK if the signing secrets are set, and publishes the release with `SHA256SUMS.txt`.

A version with a suffix (`0.33.0-beta.1`) becomes a pre-release; the tablet's updater skips
pre-releases unless they are switched on in its settings.

### Repository settings the workflows need

- Actions enabled; workflow permissions may stay "read" (the publish job asks for
  `contents: write` itself).
- Secrets for the APK: `ANDROID_KEYSTORE_B64` (`base64 -w0 keystore`), `ANDROID_KEYSTORE_PASSWORD`,
  `ANDROID_KEY_ALIAS`, `ANDROID_KEY_PASSWORD`. The tablet's updater only installs updates signed
  with the key of the installed app, so this has to be that key. Without the secrets releases
  have no APK; a locally built one can be added with `gh release upload v0.33.0 n0te-0.33.0-android.apk`.
- Optional variable `ANDROID_CERT_SHA256`: the signer certificate's SHA-256; the release fails
  if the APK is signed with another key (`apksigner verify --print-certs` shows it).

### What CI does

`ci.yml` on pushes to `main` and pull requests: Linux clippy and `cargo test --workspace`;
Windows x64 (`windows-latest`) and ARM64 (`windows-11-arm`) release builds, tests on x64
(non-blocking while the Windows port is in progress); installer + zip from those builds; the
APK. Build outputs are kept as workflow artifacts for 7 days. There is no `cargo fmt` check:
the code is not rustfmt-formatted.

## Third-party notices

`THIRD_PARTY_NOTICES.md` is generated: `scripts/third-party-notices.py > THIRD_PARTY_NOTICES.md`
(Rust crates from `cargo metadata`; fonts, pdfium and Android libraries listed in the script).
Run it after dependency changes. The Windows packages carry it with the pdfium and font license
texts.
