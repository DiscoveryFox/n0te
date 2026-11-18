#!/usr/bin/env bash
# Runs cargo for aarch64-linux-android (Galaxy Tab S7 FE) with the NDK linker and C toolchain set
# through environment variables, so host builds are never affected.
#
#   scripts/cargo-android.sh build -p n0te-pdf --release
#   scripts/cargo-android.sh build -p n0te-pdf --release --example bench
#
# NDK: $ANDROID_NDK_HOME, else $ANDROID_NDK_ROOT, else ~/Android/Sdk/ndk/27.2.12479018.
# Minimum API level: $ANDROID_API (default 29).
set -euo pipefail

ndk="${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-$HOME/Android/Sdk/ndk/27.2.12479018}}"
api="${ANDROID_API:-29}"
bin="$ndk/toolchains/llvm/prebuilt/linux-x86_64/bin"
clang="$bin/aarch64-linux-android$api-clang"
if [[ ! -x "$clang" ]]; then
	echo "no NDK clang at $clang (set ANDROID_NDK_HOME / ANDROID_API)" >&2
	exit 1
fi
if [[ $# -lt 1 ]]; then
	echo "usage: $0 <cargo subcommand> [args...]" >&2
	exit 2
fi

export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$clang"
export CC_aarch64_linux_android="$clang"
export CXX_aarch64_linux_android="$clang++"
export AR_aarch64_linux_android="$bin/llvm-ar"

cargo="$HOME/.cargo/bin/cargo"
[[ -x "$cargo" ]] || cargo="cargo"
cd "$(dirname "${BASH_SOURCE[0]}")/.."
sub="$1"
shift
exec "$cargo" "$sub" --target aarch64-linux-android "$@"
