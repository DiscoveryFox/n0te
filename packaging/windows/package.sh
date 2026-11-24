#!/usr/bin/env bash
# Packs the Windows build of n0te into the installer and the portable zip:
#
#   packaging/windows/package.sh <version> <x64|arm64> <dir with the exes> <out dir>
#
# The exes come from `cargo build --release --target <x86_64|aarch64>-pc-windows-msvc
# -p n0te-desktop -p n0te-sync` (CI: the windows jobs): n0te-laptop.exe (the `n0te` binary,
# renamed; n0te.exe is taken as well), n0te-tablet.exe, n0te-sync.exe. pdfium.dll is fetched here
# (scripts/fetch-pdfium.sh, hash-pinned), the licenses come from the repo.
#
# Out: n0te-<version>-windows-<arch>-setup.exe (needs makensis, NSIS 3.08+: `apt install nsis`
# on Linux, preinstalled on the GitHub Windows runners) and n0te-<version>-windows-<arch>-portable.zip
# (needs zip). Without makensis only the zip is made (and a warning printed), unless
# REQUIRE_INSTALLER=1 (CI), which makes it an error.
set -euo pipefail

version="${1:?usage: package.sh <version> <x64|arm64> <exe dir> <out dir>}"
arch="${2:?arch: x64 or arm64}"
bins="$(cd "${3:?exe dir}" && pwd)"
out="${4:?out dir}"
case "$arch" in x64 | arm64) ;; *) echo "arch must be x64 or arm64, not $arch" >&2; exit 2 ;; esac

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$here/../.." && pwd)"
mkdir -p "$out"
out="$(cd "$out" && pwd)"
name="n0te-$version-windows-$arch"
stage="$out/$name"
rm -rf "$stage"
mkdir -p "$stage/licenses/pdfium" "$stage/licenses/fonts"

# Text files for Windows: CRLF line ends.
crlf() { sed -e 's/\r$//' -e 's/$/\r/' "$1" >"$2"; }

laptop="$bins/n0te-laptop.exe"
[[ -f "$laptop" ]] || laptop="$bins/n0te.exe"
for f in "$laptop" "$bins/n0te-tablet.exe" "$bins/n0te-sync.exe"; do
	[[ -f "$f" ]] || { echo "missing $f" >&2; exit 1; }
done
cp "$laptop" "$stage/n0te-laptop.exe"
cp "$bins/n0te-tablet.exe" "$bins/n0te-sync.exe" "$stage/"

"$root/scripts/fetch-pdfium.sh" "win-$arch" >&2
pdfium="$root/third_party/pdfium/win-$arch"
cp "$pdfium/bin/pdfium.dll" "$stage/pdfium.dll"
crlf "$pdfium/LICENSE" "$stage/licenses/pdfium/LICENSE.txt"
for f in "$pdfium"/licenses/*; do
	n="$(basename "$f")"
	[[ "$n" == *.txt ]] || n="$n.txt"
	crlf "$f" "$stage/licenses/pdfium/$n"
done
for f in "$root"/crates/desktop/fonts/*-OFL.txt; do
	crlf "$f" "$stage/licenses/fonts/$(basename "$f")"
done
crlf "$root/LICENSE" "$stage/LICENSE.txt"
crlf "$root/THIRD_PARTY_NOTICES.md" "$stage/THIRD_PARTY_NOTICES.txt"

# The portable zip: the same files plus README.txt, in one folder.
sed -e "s/@VERSION@/$version/g" -e "s/@ARCH@/$arch/g" "$here/README-portable.txt" >"$stage/README.txt.lf"
crlf "$stage/README.txt.lf" "$stage/README.txt"
rm "$stage/README.txt.lf"
zip_file="$out/$name-portable.zip"
rm -f "$zip_file"
(cd "$out" && zip -q -r -X -9 "$zip_file" "$name")
echo "$zip_file"

# The installer (README.txt is only for the zip).
mv "$stage/README.txt" "$out/README-$arch.txt"
setup="$out/$name-setup.exe"
makensis="$(command -v makensis || true)"
if [[ -z "$makensis" ]]; then
	if [[ "${REQUIRE_INSTALLER:-}" == 1 ]]; then
		echo "makensis not found (apt install nsis)" >&2
		exit 1
	fi
	echo "warning: makensis not found, no installer made (apt install nsis)" >&2
else
	# Four plain numbers for the installer's own version info: 0.32.0-beta.1 -> 0.32.0.0.
	base="${version%%[-+]*}"
	"$makensis" -V2 -INPUTCHARSET UTF8 -DVERSION="$version" -DVERSION4="$base.0" -DARCH="$arch" \
		-DSTAGE="$stage" -DOUTFILE="$setup" "$here/n0te.nsi" >&2
	echo "$setup"
fi
mv "$out/README-$arch.txt" "$stage/README.txt"
