#!/usr/bin/env bash
# Builds libn0te_ffi.so for the tablet/phone (arm64) and generates the Kotlin bindings.
# Output: app-android/n0te/build/rust/{jniLibs/arm64-v8a/{libn0te_ffi.so,libpdfium.so},kotlin/...}
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
repo="$(cd "$here/.." && pwd)"
cargo="${CARGO:-$HOME/.cargo/bin/cargo}"
ndk="${ANDROID_NDK_HOME:-$HOME/Android/Sdk/ndk/27.2.12479018}"
tc="$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin"
profile="${1:-release}"
[[ -x "$tc/aarch64-linux-android29-clang" ]] || { echo "NDK not found at $ndk" >&2; exit 1; }

export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$tc/aarch64-linux-android29-clang"
export CC_aarch64_linux_android="$tc/aarch64-linux-android29-clang"
export AR_aarch64_linux_android="$tc/llvm-ar"

cd "$here"
flag=(--release); dir=release
[[ "$profile" == debug ]] && { flag=(); dir=debug; }
"$cargo" build "${flag[@]}" -p n0te-ffi --target aarch64-linux-android

out="$repo/app-android/n0te/build/rust"
rm -rf "$out/kotlin"
mkdir -p "$out/jniLibs/arm64-v8a" "$out/kotlin"
"$tc/llvm-strip" --strip-all -o "$out/jniLibs/arm64-v8a/libn0te_ffi.so" "target/aarch64-linux-android/$dir/libn0te_ffi.so"
# pdfium (pinned, hash-checked, already stripped): shipped next to libn0te_ffi.so and loaded by
# path, PdfService.start(lib, "$nativeLibraryDir/libpdfium.so", ...). The fetch is a no-op when
# the pinned version is present.
"$here/scripts/fetch-pdfium.sh" android-arm64
install -m 0644 "$here/third_party/pdfium/android-arm64/lib/libpdfium.so" "$out/jniLibs/arm64-v8a/libpdfium.so"
"$cargo" run -q -p uniffi-bindgen -- generate \
  --library "target/aarch64-linux-android/$dir/libn0te_ffi.so" \
  --language kotlin --no-format --out-dir "$out/kotlin"
ls -la "$out/jniLibs/arm64-v8a/"
find "$out/kotlin" -name '*.kt' | head
