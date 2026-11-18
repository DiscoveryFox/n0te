#!/usr/bin/env bash
# Downloads the pinned prebuilt pdfium (https://github.com/bblanchon/pdfium-binaries) for the
# platforms n0te ships on and puts libpdfium.so + licenses into n0te/third_party/pdfium/<platform>/.
#
#   scripts/fetch-pdfium.sh            # all platforms
#   scripts/fetch-pdfium.sh linux-x64  # one platform
#   FORCE=1 scripts/fetch-pdfium.sh    # re-download even if the pinned version is present
#
# Every archive is checked against the SHA-256 recorded below before it is opened. Nothing from
# the archive is executed: only lib/libpdfium.so, LICENSE, licenses/* and VERSION are copied out.
#
# To update: pick a newer release, download both archives, compare `sha256sum` with the digests
# GitHub shows on the release page, then change PDFIUM_TAG and the hashes here.
set -euo pipefail

PDFIUM_TAG="chromium/8165" # PDFium 160.0.8165.0, released 2026-11-16
declare -A SHA256=(
	[linux-x64]="ba7865e16c0271daab75256790be06c26272190afe6e2a70eae0bb39bb99089c"
	[android-arm64]="ce4f17fea8881cdf163950748ab90854cf33f296309fab68de9441a1d41ff6cc"
)
BASE_URL="https://github.com/bblanchon/pdfium-binaries/releases/download/${PDFIUM_TAG}"

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dest_root="$root/third_party/pdfium"

if [[ $# -gt 0 ]]; then platforms=("$@"); else platforms=(linux-x64 android-arm64); fi

work="$(mktemp -d "${TMPDIR:-/tmp}/n0te-pdfium.XXXXXX")"
trap 'rm -rf "$work"' EXIT

for platform in "${platforms[@]}"; do
	want="${SHA256[$platform]:-}"
	if [[ -z "$want" ]]; then
		echo "unknown platform '$platform' (known: ${!SHA256[*]})" >&2
		exit 2
	fi
	dest="$dest_root/$platform"
	if [[ -z "${FORCE:-}" && -f "$dest/lib/libpdfium.so" && "$(cat "$dest/TAG" 2>/dev/null)" == "$PDFIUM_TAG" ]]; then
		echo "$platform: $PDFIUM_TAG already present"
		continue
	fi

	# Fresh empty directories for the download and for the extraction.
	dl="$(mktemp -d "$work/dl.XXXXXX")"
	ex="$(mktemp -d "$work/ex.XXXXXX")"
	archive="$dl/pdfium-$platform.tgz"
	echo "$platform: downloading $PDFIUM_TAG"
	curl -fsSL --proto '=https' --tlsv1.2 -o "$archive" "$BASE_URL/pdfium-$platform.tgz"

	got="$(sha256sum "$archive" | cut -d' ' -f1)"
	if [[ "$got" != "$want" ]]; then
		echo "$platform: SHA-256 mismatch: got $got, want $want" >&2
		exit 1
	fi

	# Refuse archives with absolute paths, '..' components or anything but files/directories.
	if tar -tzf "$archive" | grep -Eq '(^/|(^|/)\.\.(/|$))'; then
		echo "$platform: archive contains unsafe paths" >&2
		exit 1
	fi
	if tar -tvzf "$archive" | grep -Eqv '^[-d]'; then
		echo "$platform: archive contains links or special files" >&2
		exit 1
	fi
	tar -xzf "$archive" -C "$ex" --no-same-owner --no-same-permissions \
		lib/libpdfium.so LICENSE VERSION licenses

	rm -rf "$dest"
	mkdir -p "$dest/lib"
	install -m 0644 "$ex/lib/libpdfium.so" "$dest/lib/libpdfium.so"
	install -m 0644 "$ex/LICENSE" "$ex/VERSION" "$dest/"
	cp -r "$ex/licenses" "$dest/licenses"
	chmod -R u=rwX,go=rX "$dest/licenses"
	echo "$PDFIUM_TAG" >"$dest/TAG"
	echo "$platform: $(stat -c %s "$dest/lib/libpdfium.so") bytes -> ${dest#"$root"/}/lib/libpdfium.so"
done
