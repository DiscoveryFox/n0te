#!/usr/bin/env bash
# Sets the one version of the whole n0te suite (Android app, n0te Laptop, n0te Tablet, n0te-sync):
#
#   scripts/bump-version.sh 0.33.0
#
# - Cargo workspace version (Cargo.toml [workspace.package]) and the workspace crates in Cargo.lock
# - Android versionName = the same, versionCode + 1 (android/build.gradle.kts)
# - a skeleton "## [0.33.0] - <today>" section at the top of CHANGELOG.md if there is none yet
#   (the release workflow refuses a section that still says TODO)
#
# It does not commit, tag or push. Afterwards: write the CHANGELOG.md section (and the German
# entry in android/src/main/java/dev/n0bridge/n0te/Changelog.kt), commit, then tag v0.33.0
# (see docs/DEVELOPMENT.md, "Cutting a release").
set -euo pipefail

new="${1:-}"
if [[ ! "$new" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.]+)?$ ]]; then
	echo "usage: $0 X.Y.Z   (got '${new}')" >&2
	exit 2
fi
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
gradle_file=android/build.gradle.kts

old="$(scripts/version.sh cargo)"
old_name="$(scripts/version.sh android)"
old_code="$(sed -n 's/^[[:space:]]*versionCode = \([0-9][0-9]*\).*/\1/p' "$gradle_file" | head -1)"
[[ -n "$old_code" ]] || { echo "no versionCode in $gradle_file" >&2; exit 1; }
new_code=$((old_code + 1))

# Cargo.toml: only the version line inside [workspace.package].
awk -v v="$new" '
	/^\[/ { in_pkg = ($0 == "[workspace.package]") }
	in_pkg && /^version[[:space:]]*=/ && !done { print "version = \"" v "\""; done = 1; next }
	{ print }
	END { if (!done) exit 1 }
' Cargo.toml >Cargo.toml.new || { rm -f Cargo.toml.new; echo "no version in [workspace.package]" >&2; exit 1; }
mv Cargo.toml.new Cargo.toml

sed -i.bak -e "s/^\([[:space:]]*versionCode = \)[0-9][0-9]*/\1$new_code/" \
	-e "s/^\([[:space:]]*versionName = \)\"[^\"]*\"/\1\"$new\"/" "$gradle_file"
rm -f "$gradle_file.bak"

# The workspace crates' entries in Cargo.lock (nothing else changes).
cargo="${CARGO:-$HOME/.cargo/bin/cargo}"
command -v "$cargo" >/dev/null 2>&1 || cargo=cargo
"$cargo" update --workspace --offline -q 2>/dev/null || "$cargo" update --workspace -q

if [[ -f CHANGELOG.md ]] && ! grep -q "^## \[$new\]" CHANGELOG.md; then
	today="$(date +%Y-%m-%d)"
	awk -v v="$new" -v d="$today" '
		!done && /^## \[/ { print "## [" v "] - " d "\n\n### Added\n\n- TODO\n\n### Changed\n\n- TODO\n"; done = 1 }
		{ print }
	' CHANGELOG.md >CHANGELOG.md.new && mv CHANGELOG.md.new CHANGELOG.md
	echo "CHANGELOG.md: added a skeleton section for $new (fill in the TODOs)"
fi

echo "version: $old -> $new (Android $old_name -> $new, versionCode $old_code -> $new_code)"
echo "next: CHANGELOG.md + Changelog.kt, commit, git tag v$new, push the tag"
