#!/usr/bin/env bash
# The suite's version, as the build files and CHANGELOG.md have it (used by the release workflow):
#
#   scripts/version.sh cargo            # Cargo workspace version
#   scripts/version.sh android          # Android versionName
#   scripts/version.sh code             # Android versionCode
#   scripts/version.sh check v0.32.0    # tag = workspace version = versionName, and CHANGELOG.md has a
#                                       # finished "## [0.32.0]" section; prints the version
#   scripts/version.sh notes 0.32.0     # that CHANGELOG.md section without its heading (release notes)
set -euo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

cargo_version() {
	awk '
		/^\[/ { in_pkg = ($0 == "[workspace.package]") }
		in_pkg && /^version[[:space:]]*=/ { gsub(/.*=[[:space:]]*"|".*/, ""); print; exit }
	' Cargo.toml
}
android_version() { sed -n 's/^[[:space:]]*versionName = "\([^"]*\)".*/\1/p' android/build.gradle.kts | head -1; }
android_code() { sed -n 's/^[[:space:]]*versionCode = \([0-9][0-9]*\).*/\1/p' android/build.gradle.kts | head -1; }

# The body of "## [X.Y.Z] ..." up to the next "## [" heading (or the link list at the end).
notes() {
	awk -v v="$1" '
		/^## \[/ { if (on) exit; on = (index($0, "## [" v "]") == 1); next }
		on && /^\[[^]]+\]: / { exit }
		on { print }
	' CHANGELOG.md | sed -e '/./,$!d' | sed -e ':a' -e '/^\n*$/{$d;N;ba' -e '}'
}

case "${1:-}" in
cargo) cargo_version ;;
android) android_version ;;
code) android_code ;;
notes)
	body="$(notes "${2:?version}")"
	[[ -n "$body" ]] || { echo "CHANGELOG.md has no section [$2]" >&2; exit 1; }
	printf '%s\n' "$body"
	;;
check)
	tag="${2:?tag, e.g. v0.32.0}"
	want="${tag#v}"
	c="$(cargo_version)"
	a="$(android_version)"
	ok=1
	[[ "$tag" == v* ]] || { echo "tag '$tag' does not start with v" >&2; ok=0; }
	[[ "$c" == "$want" ]] || { echo "Cargo workspace version is $c, tag says $want" >&2; ok=0; }
	[[ "$a" == "$want" ]] || { echo "Android versionName is $a, tag says $want" >&2; ok=0; }
	body="$(notes "$want" || true)"
	if [[ -z "$body" ]]; then
		echo "CHANGELOG.md has no '## [$want]' section" >&2
		ok=0
	elif grep -q 'TODO' <<<"$body"; then
		echo "CHANGELOG.md section [$want] still says TODO" >&2
		ok=0
	fi
	[[ $ok == 1 ]] || exit 1
	echo "$want"
	;;
*)
	echo "usage: $0 cargo|android|code|check <tag>|notes <version>" >&2
	exit 2
	;;
esac
