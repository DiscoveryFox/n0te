#!/usr/bin/env bash
# The whole test suite in one go, as fast as it gets (Linux; Windows-only code is tested by CI):
#
#   scripts/test.sh            Rust (every crate) + the Android JVM tests
#   scripts/test.sh rust       Rust only
#   scripts/test.sh android    Android JVM tests only
#
# Rust runs in the "fast" profile (optimised, incremental): the fuzz, sync and UI tests run
# several times quicker than in a debug build, and a rebuild after a change stays short.
# The Android JVM tests load the laptop build of the same Rust library and use Kotlin bindings
# generated from it, so they need no Android NDK build (the APK build makes its own bindings).
# Extra arguments go to `cargo test` (e.g. `scripts/test.sh rust -- design`).
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
cargo="${CARGO:-$HOME/.cargo/bin/cargo}"
command -v "$cargo" >/dev/null 2>&1 || cargo=cargo
what="${1:-all}"
[[ $# -gt 0 ]] && shift
started=$SECONDS

scripts/fetch-pdfium.sh linux-x64 >/dev/null
export N0TE_PDFIUM_LIB="${N0TE_PDFIUM_LIB:-$root/third_party/pdfium/linux-x64/lib/libpdfium.so}"

if [[ "$what" == all || "$what" == rust ]]; then
	echo "== Rust"
	"$cargo" test --profile fast --workspace "$@"
fi

if [[ "$what" == all || "$what" == android ]]; then
	echo "== Android (JVM)"
	# The "fast" profile (no LTO) builds in seconds after a change; the APK keeps fat LTO.
	"$cargo" build --profile fast -p n0te-ffi
	lib="target/fast/libn0te_ffi.so"
	out="android/build/rust/kotlin"
	rm -rf "$out" && mkdir -p "$out"
	"$cargo" run -q -p uniffi-bindgen -- generate --library "$lib" --language kotlin --no-format --out-dir "$out"
	# The monorepo builds the app from app-android/; the published repo from android/ itself.
	if [[ -f ../app-android/settings.gradle.kts ]]; then
		(cd ../app-android && gradle --no-daemon -q :n0te:testReleaseUnitTest -x rustCore -Pn0te.ffiDir=target/fast)
	else
		(cd android && gradle --no-daemon -q testReleaseUnitTest -x rustCore -Pn0te.ffiDir=target/fast)
	fi
	echo "JVM test reports: android/build/reports/tests/testReleaseUnitTest/index.html"
fi

echo "== all green in $((SECONDS - started)) s"
