# n0te

Notes app for tablet, laptop and phone, part of the n0bridge ecosystem.
Plan and decisions: `~/.claude/plans/n0te-notes-app.md`.

## Layout

| Path | What |
|---|---|
| `crates/core` | `n0te-core`: notebook tree (groups, sheets, side sheets left/right), sheet documents (strokes), ink encoding + stroke outline, on-disk library. Loro CRDTs, so every device holds a full copy and edits merge. |
| `crates/pdf` | `n0te-pdf`: pdfium (loaded by path) for page sizes, tiles and text; `PdfWorker`, the one thread per app that does pdfium work in priority order (visible tiles first, stale generations dropped, LRU of open documents); export = pdfium assembles the pages, n0te's own writer appends the ink (`ink_writer`: incremental update, Flate streams, optional content group "n0te ink"); notebook import/export helpers. |
| `crates/ffi` | `n0te-ffi`: UniFFI bindings of the core for Android (Kotlin). Hot paths pass packed little-endian f32 bytes. PDF: `PdfService` (worker + `TileSink` callback), `NotebookHandle.importPdf/exportPdf`. |
| `crates/uniffi-bindgen` | The bindgen CLI (`cargo run -p uniffi-bindgen -- generate …`). |
| `scripts/build-android.sh` | Builds `libn0te_ffi.so` for arm64, copies the pinned arm64 `libpdfium.so` next to it and generates the Kotlin bindings into `app-android/n0te/build/rust/`. Run by Gradle before `:n0te` builds. |
| `scripts/fetch-pdfium.sh` | Downloads the pinned, hash-checked prebuilt pdfium for linux-x64 and android-arm64 into `third_party/pdfium/` (gitignored). Needed by the n0te-pdf tests and the Android build. |
| `../app-android/n0te` | The Android app (package `dev.n0bridge.n0te`). |

## Build and test

```sh
make n0te-test     # cargo test --workspace (needs ~/.cargo/bin/cargo, rustup)
make n0te-apk      # Android APK (needs the aarch64-linux-android rustup target + NDK 27)
cargo run --release -p n0te-core --example bench   # speed check on the laptop
scripts/fetch-pdfium.sh                             # pdfium for the PDF tests and the APK
cargo run --release -p n0te-pdf --example pdf_bench -- third_party/pdfium/linux-x64/lib/libpdfium.so slides.pdf [more.pdf]
```

JVM binding tests (`:n0te:testReleaseUnitTest`) load a laptop build of the core: run
`cargo build --release -p n0te-ffi` first; they use `third_party/pdfium/linux-x64` for pdfium.

The system Rust (Arch package) can't build for Android; the user-local rustup in `~/.cargo/bin`
has the `aarch64-linux-android` target. The release profile keeps symbols because
uniffi-bindgen reads its metadata from them; the copy packed into the APK is stripped.
