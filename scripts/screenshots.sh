#!/usr/bin/env bash
# The README's pictures: docs/screenshots/*.png, taken headless (no window) by
# crates/desktop/examples/screenshots.rs from a temporary library it builds itself.
#
#   scripts/screenshots.sh
#
# Never touches your library, settings or the running n0te-sync: the example gives itself its own
# HOME, config, cache and runtime folders (and a stand-in on its own socket). Needs the wgpu
# renderer (a GPU, or Mesa's software Vulkan) for the ink, a local TeX (latex + dvisvgm) for the
# formulas, and python3 with Pillow to shrink the PNGs (256-colour palettes; skipped without).
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
cargo="${CARGO:-$HOME/.cargo/bin/cargo}"
command -v "$cargo" >/dev/null 2>&1 || cargo=cargo
out="$root/docs/screenshots"

scripts/fetch-pdfium.sh linux-x64
export N0TE_PDFIUM_LIB="$root/third_party/pdfium/linux-x64/lib/libpdfium.so"

mkdir -p "$out"
"$cargo" run --locked --profile fast -p n0te-desktop --example screenshots -- "$out"
# n0te Tablet (its own process: the app runs as n0te Tablet, the mouse stands in for the pen).
"$cargo" run --locked --profile fast -p n0te-desktop --example tablet_shots -- "$out"

if python3 -c 'import PIL' 2>/dev/null; then
	python3 -I - "$out"/*.png <<'PY'
import os, sys
from PIL import Image
for path in sys.argv[1:]:
    before = os.path.getsize(path)
    im = Image.open(path).convert("RGB")
    im.quantize(colors=256, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE).save(path, optimize=True)
    print(f"{os.path.basename(path)}: {before // 1024} KiB -> {os.path.getsize(path) // 1024} KiB")
PY
else
	echo "no Pillow: PNGs left as they are" >&2
fi
