#!/usr/bin/env bash
# Downloads the pinned prebuilt pdfium (https://github.com/bblanchon/pdfium-binaries) for the
# platforms n0te ships on and puts the library + licenses into n0te/third_party/pdfium/<platform>/:
# lib/libpdfium.so on Linux and Android, bin/pdfium.dll on Windows (the archives' own layout).
#
#   scripts/fetch-pdfium.sh            # linux-x64 and android-arm64
#   scripts/fetch-pdfium.sh win-x64    # one platform (win-x64, win-arm64, android-x64, ...)
#   FORCE=1 scripts/fetch-pdfium.sh    # re-download even if the pinned version is present
#
# Runs on Linux and under Git Bash on Windows (the GitHub Actions Windows runners).
#
# Every archive is checked against the SHA-256 recorded below before it is opened. Nothing from
# the archive is executed: only the library, LICENSE, licenses/* and VERSION are copied out.
#
# To update: pick a newer release, download the archives, compare `sha256sum` with the digests
# GitHub shows on the release page, then change PDFIUM_TAG and the hashes here.
set -euo pipefail

PDFIUM_TAG="chromium/8165" # PDFium 160.0.8165.0, released 2026-11-16
declare -A SHA256=(
	[linux-x64]="ba7865e16c0271daab75256790be06c26272190afe6e2a70eae0bb39bb99089c"
	[android-arm64]="ce4f17fea8881cdf163950748ab90854cf33f296309fab68de9441a1d41ff6cc"
	# The emulator (x86_64 test builds only; digest from the GitHub release's asset list).
	[android-x64]="e5d8646165eff2f837e5d8fc356eda6b7e624cbfad5915e2500428e244a89b44"
	# n0te Laptop / n0te Tablet on Windows (pdfium.dll next to the exes).
	[win-x64]="ca7338ae7779b8b1b82f4580305c4d762d214f8df1d54b47647fe3cc75989ded"
	[win-arm64]="d65e405c59cffe6fe36c0935b90d40ddf3ce75ef284f3168a28dcf00d0388131"
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
	case "$platform" in
	win-*) lib="bin/pdfium.dll" ;;
	*) lib="lib/libpdfium.so" ;;
	esac
	dest="$dest_root/$platform"
	if [[ -z "${FORCE:-}" && -f "$dest/$lib" && "$(cat "$dest/TAG" 2>/dev/null)" == "$PDFIUM_TAG" ]]; then
		echo "$platform: $PDFIUM_TAG already present"
		continue
	fi

	# Fresh empty directories for the download and for the extraction. The archive is opened by
	# its relative name from inside its directory: GNU tar would read "C:/..." as host:path.
	dl="$(mktemp -d "$work/dl.XXXXXX")"
	ex="$(mktemp -d "$work/ex.XXXXXX")"
	archive="pdfium-$platform.tgz"
	echo "$platform: downloading $PDFIUM_TAG"
	curl -fsSL --proto '=https' --tlsv1.2 --retry 3 -o "$dl/$archive" "$BASE_URL/$archive"

	got="$(sha256sum "$dl/$archive" | cut -d' ' -f1)"
	if [[ "$got" != "$want" ]]; then
		echo "$platform: SHA-256 mismatch: got $got, want $want" >&2
		exit 1
	fi

	(
		cd "$dl"
		# Refuse archives with absolute paths, '..' components or anything but files/directories.
		if tar -tzf "$archive" | grep -Eq '(^/|(^|/)\.\.(/|$))'; then
			echo "$platform: archive contains unsafe paths" >&2
			exit 1
		fi
		if tar -tvzf "$archive" | grep -Eqv '^[-d]'; then
			echo "$platform: archive contains links or special files" >&2
			exit 1
		fi
		tar -xzf "$archive" -C "$ex" --no-same-owner --no-same-permissions "$lib" LICENSE VERSION licenses
	)

	rm -rf "$dest"
	mkdir -p "$dest/$(dirname "$lib")"
	cp "$ex/$lib" "$dest/$lib"
	cp "$ex/LICENSE" "$ex/VERSION" "$dest/"
	cp -r "$ex/licenses" "$dest/licenses"
	chmod 0644 "$dest/$lib" "$dest/LICENSE" "$dest/VERSION"
	chmod -R u=rwX,go=rX "$dest/licenses"
	echo "$PDFIUM_TAG" >"$dest/TAG"
	echo "$platform: $(wc -c <"$dest/$lib" | tr -d ' ') bytes -> ${dest#"$root"/}/$lib"
done
