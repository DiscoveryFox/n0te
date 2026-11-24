#!/usr/bin/env bash
# Type-checks the workspace for Windows on Linux (no Windows SDK needed):
#
#   scripts/wincheck.sh                    # all crates, x86_64
#   scripts/wincheck.sh -p n0te-desktop    # one crate
#   TARGET=aarch64-pc-windows-msvc scripts/wincheck.sh
#
# `cargo check` only, never a build: C code of dependencies (ring, blake3) is compiled by clang-cl
# against stub C headers just far enough for their build scripts to finish; nothing is linked.
# Real Windows binaries come from the GitHub Actions Windows runners (.github/workflows).
# Needs clang-cl + llvm-lib (Arch: clang, llvm) and `rustup target add x86_64-pc-windows-msvc`.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
target="${TARGET:-x86_64-pc-windows-msvc}"
stub="$here/target/wincheck/stub-include"
mkdir -p "$stub"
printf '#pragma once\n#define assert(x) ((void)0)\n' > "$stub/assert.h"
printf '#pragma once\n#include <stddef.h>\nvoid *memcpy(void*,const void*,size_t);void *memset(void*,int,size_t);int memcmp(const void*,const void*,size_t);void *memmove(void*,const void*,size_t);size_t strlen(const char*);\n' > "$stub/string.h"
printf '#pragma once\n#include <stddef.h>\nvoid abort(void);void *malloc(size_t);void free(void*);\n' > "$stub/stdlib.h"
printf '#pragma once\n' > "$stub/stdio.h"
printf '#pragma once\n' > "$stub/errno.h"
t="${target//-/_}"
export "CC_$t=clang-cl" "AR_$t=llvm-lib" "CFLAGS_$t=/imsvc$stub"
export CARGO_TARGET_DIR="$here/target/wincheck"
cd "$here"
args=("$@")
[[ ${#args[@]} -eq 0 ]] && args=(-p n0te-core -p n0te-pdf -p n0te-sync -p n0te-desktop)
exec "${CARGO:-$HOME/.cargo/bin/cargo}" check --target "$target" "${args[@]}"
