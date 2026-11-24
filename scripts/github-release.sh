#!/usr/bin/env bash
# Publishes the current n0te APK as a GitHub release, where the tablet app's updater finds it:
#
#   n0te/scripts/github-release.sh OWNER/REPO [--pre]
#
# Needs gh (logged in: gh auth login). Tag n0te-v<versionName>, the arm64 APK from
# ~/.local/share/n0bridge/releases (run make n0te-apk and copy it there first, as for tablet
# updates), the notes from the newest Changelog.kt entry. --pre marks it as a pre-release.
# The APK must be signed with the same key as the installed app (the laptop's debug key).
set -euo pipefail
repo="${1:?usage: github-release.sh OWNER/REPO [--pre]}"
pre="${2:-}"
root="$(cd "$(dirname "$0")/../.." && pwd)"
version=$(sed -n 's/.*versionName = "\(.*\)".*/\1/p' "$root/n0te/android/build.gradle.kts" | head -1)
apk="$HOME/.local/share/n0bridge/releases/n0te-v$version-release.apk"
[ -f "$apk" ] || { echo "no $apk (make n0te-apk, then copy it there)" >&2; exit 1; }
# Not the emulator build (x86_64 only): the tablet needs arm64.
unzip -l "$apk" | grep -q "lib/arm64-v8a/" || { echo "$apk has no arm64 libraries (an emulator build?)" >&2; exit 1; }
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
python3 - "$root/n0te/android/src/main/java/dev/n0bridge/n0te/Changelog.kt" "$version" > "$tmp/notes.md" <<'PY'
import re, sys
src, version = open(sys.argv[1]).read(), sys.argv[2]
m = re.search(r'Entry\("' + re.escape(version) + r'",\s*"([^"]*)",\s*"([^"]*)",\s*listOf\((.*?)\)\),', src, re.S)
if not m:
    sys.exit(f"no Changelog entry for {version}")
date, title, body = m.groups()
items = re.findall(r'"((?:[^"\\]|\\.)*)"', body)
print(f"# {title}\n\n" + "\n".join(f"- {i}" for i in items) + f"\n\n{date}")
PY
title=$(head -1 "$tmp/notes.md" | sed 's/^# //')
cp "$apk" "$tmp/n0te-v$version-release.apk"
gh release create "n0te-v$version" "$tmp/n0te-v$version-release.apk" --repo "$repo" --title "n0te $version – $title" --notes-file "$tmp/notes.md" ${pre:+--prerelease}
