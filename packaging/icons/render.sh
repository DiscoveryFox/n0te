#!/usr/bin/env bash
# Renders the app icons from their SVGs (the sources) into the files the builds use (committed):
#
#   packaging/icons/render.sh
#
#   n0te-laptop.ico, n0te-tablet.ico, n0te-sync.ico  embedded into the Windows exes (build.rs)
#   n0te.ico                                        the installer (from packaging/n0te.svg)
#   n0te-laptop.png, n0te-tablet.png (128 px)       the window icons at run time (crates/desktop)
#
# Each .ico holds 16, 20, 24, 32, 40, 48, 64, 128 and 256 px, every size rendered from the SVG
# (not scaled down from a big one). Sizes up to 24 px of n0te Tablet use n0te-tablet-small.svg
# (the nib alone, bigger). Needs rsvg-convert (librsvg) and python3.
set -euo pipefail
here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
sizes=(16 20 24 32 40 48 64 128 256)

ico() { # ico <name> <svg> [<svg for <=24 px>]
	local name="$1" svg="$2" small="${3:-$2}" pngs=()
	for s in "${sizes[@]}"; do
		local src="$svg"
		((s <= 24)) && src="$small"
		rsvg-convert -w "$s" -h "$s" "$src" -o "$tmp/$name-$s.png"
		pngs+=("$tmp/$name-$s.png")
	done
	# PNG-compressed entries (read by Windows Vista and later): small files, full alpha.
	python3 -I - "$here/$name.ico" "${pngs[@]}" <<'PY'
import struct, sys
out, pngs = sys.argv[1], [open(p, "rb").read() for p in sys.argv[2:]]
head = struct.pack("<HHH", 0, 1, len(pngs))
offset, entries = 6 + 16 * len(pngs), b""
for png in pngs:
    w, h = struct.unpack(">II", png[16:24])  # IHDR
    entries += struct.pack("<BBBBHHII", w % 256, h % 256, 0, 0, 1, 32, len(png), offset)
    offset += len(png)
open(out, "wb").write(head + entries + b"".join(pngs))
PY
	echo "$name.ico"
}

ico n0te-laptop "$here/n0te-laptop.svg"
ico n0te-tablet "$here/n0te-tablet.svg" "$here/n0te-tablet-small.svg"
ico n0te-sync "$here/n0te-sync.svg"
ico n0te "$here/../n0te.svg"
for n in laptop tablet; do
	rsvg-convert -w 128 -h 128 "$here/n0te-$n.svg" -o "$here/n0te-$n.png"
	echo "n0te-$n.png"
done
