#!/usr/bin/env bash
# Builds libn0te_ffi.so for the tablet/phone (arm64) and generates the Kotlin bindings.
# Output: n0te/android/build/rust/{jniLibs/arm64-v8a/{libn0te_ffi.so,libpdfium.so},kotlin/...}
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cargo="${CARGO:-$HOME/.cargo/bin/cargo}"
ndk="${ANDROID_NDK_HOME:-$HOME/Android/Sdk/ndk/27.2.12479018}"
tc="$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin"
profile="${1:-release}"
[[ -x "$tc/aarch64-linux-android29-clang" ]] || { echo "NDK not found at $ndk" >&2; exit 1; }

# ABIs: arm64-v8a (the tablet; default) and, for emulator tests, x86_64 (N0TE_ABIS="arm64-v8a x86_64").
abis="${N0TE_ABIS:-arm64-v8a}"
cd "$here"
flag=(--release); dir=release
[[ "$profile" == debug ]] && { flag=(); dir=debug; }
out="$here/android/build/rust"
rm -rf "$out/kotlin" "$out/jniLibs"
mkdir -p "$out/kotlin"
lib_for_bindings=""
for abi in $abis; do
	case "$abi" in
		arm64-v8a) triple=aarch64-linux-android; pdf=android-arm64 ;;
		x86_64) triple=x86_64-linux-android; pdf=android-x64 ;;
		*) echo "unknown ABI $abi" >&2; exit 2 ;;
	esac
	up="$(echo "$triple" | tr 'a-z-' 'A-Z_')"
	export "CARGO_TARGET_${up}_LINKER=$tc/${triple}29-clang"
	export "CC_${triple//-/_}=$tc/${triple}29-clang"
	export "AR_${triple//-/_}=$tc/llvm-ar"
	"$cargo" build "${flag[@]}" -p n0te-ffi --target "$triple"
	mkdir -p "$out/jniLibs/$abi"
	"$tc/llvm-strip" --strip-all -o "$out/jniLibs/$abi/libn0te_ffi.so" "target/$triple/$dir/libn0te_ffi.so"
	# pdfium (pinned, hash-checked, already stripped): shipped next to libn0te_ffi.so and loaded
	# by path, PdfService.start(lib, "$nativeLibraryDir/libpdfium.so", ...). The fetch is a no-op
	# when the pinned version is present.
	"$here/scripts/fetch-pdfium.sh" "$pdf"
	install -m 0644 "$here/third_party/pdfium/$pdf/lib/libpdfium.so" "$out/jniLibs/$abi/libpdfium.so"
	[[ -z "$lib_for_bindings" ]] && lib_for_bindings="target/$triple/$dir/libn0te_ffi.so"
done
"$cargo" run -q -p uniffi-bindgen -- generate \
  --library "$lib_for_bindings" \
  --language kotlin --no-format --out-dir "$out/kotlin"
ls -la "$out"/jniLibs/*/
find "$out/kotlin" -name '*.kt' | head
