#!/usr/bin/env bash
# Driving n0te in the Android emulator (AVD "n0te-tab", Android 14 x86_64, 2560x1800) for tests
# without the tablet. Build the x86_64 APK with `make n0te-apk-emu` (arm64 libraries under the
# emulator's ARM translation hang in iroh's network setup, so it needs native x86_64).
#
#   scripts/emu.sh start               boot the AVD headless (waits for boot)
#   scripts/emu.sh install [apk]       install (default: the emulator build)
#   scripts/emu.sh shot NAME           screenshot -> $N0TE_EMU_OUT/NAME.png
#   scripts/emu.sh tap X Y             tap at screen pixels
#   scripts/emu.sh taptext TEXT        tap the element with this text / content description
#   scripts/emu.sh type TEXT           type (sent in 40-char chunks; adb truncates long strings)
#   scripts/emu.sh key CODE            key event (66 Enter, 4 Back, 67 Delete)
#   scripts/emu.sh stylus X1 Y1 X2 Y2  draw with the emulated stylus
#   scripts/emu.sh text                the focused text field's content
#   scripts/emu.sh field               tap the first text field
#   scripts/emu.sh ui                  the UI tree (uiautomator XML)
#   scripts/emu.sh stop
set -euo pipefail
SDK="${ANDROID_HOME:-$HOME/Android/Sdk}"
[[ -x "$SDK/platform-tools/adb" ]] || SDK="$HOME/Android/Sdk"
ADB="$SDK/platform-tools/adb"
OUT="${N0TE_EMU_OUT:-${TMPDIR:-/tmp}/n0te-emu}"
APK="$(cd "$(dirname "$0")/../.." && pwd)/app-android/n0te/build/outputs/apk/release/n0te-emu.apk"
mkdir -p "$OUT"
ui() { "$ADB" shell uiautomator dump /sdcard/ui.xml >/dev/null 2>&1; "$ADB" shell cat /sdcard/ui.xml; }
case "${1:-}" in
	start)
		nohup "$SDK/emulator/emulator" -avd n0te-tab -no-window -no-audio -gpu swiftshader_indirect -no-snapshot -no-boot-anim >"$OUT/emulator.log" 2>&1 &
		"$ADB" wait-for-device
		until [[ "$("$ADB" shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" == 1 ]]; do sleep 2; done
		echo booted ;;
	install) "$ADB" install -r "${2:-$APK}" | tail -1 ;;
	shot) "$ADB" exec-out screencap -p >"$OUT/$2.png"; echo "$OUT/$2.png" ;;
	tap) "$ADB" shell input tap "$2" "$3" ;;
	taptext)
		xy=$(ui | python3 -I -c '
import re, sys
x, t = sys.stdin.read(), sys.argv[1]
for n in re.findall(r"<node [^>]*>", x):
    if f"text=\"{t}\"" in n or f"content-desc=\"{t}\"" in n:
        b = list(map(int, re.search(r"bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", n).groups()))
        print((b[0] + b[2]) // 2, (b[1] + b[3]) // 2); break' "$2")
		[[ -n "$xy" ]] && "$ADB" shell input tap $xy && echo "tapped $2" || { echo "not found: $2" >&2; exit 1; } ;;
	type)
		t="$2"
		for ((i = 0; i < ${#t}; i += 40)); do
			chunk="${t:$i:40}"
			"$ADB" shell input text "$(printf '%s' "$chunk" | sed -e 's/ /%s/g' -e 's/[$*#^&()<>|;\\'"'"']/\\&/g')"
			sleep 0.3
		done ;;
	key) "$ADB" shell input keyevent "$2" ;;
	ui) ui ;;
	field)
		xy=$(ui | python3 -I -c '
import re, sys
for n in re.findall(r"<node [^>]*class=\"android.widget.EditText\"[^>]*>", sys.stdin.read()):
    b = list(map(int, re.search(r"bounds=\"\[(\d+),(\d+)\]\[(\d+),(\d+)\]\"", n).groups()))
    print((b[0] + b[2]) // 2, (b[1] + b[3]) // 2); break')
		[[ -n "$xy" ]] && "$ADB" shell input tap $xy && echo "field at $xy" || { echo "no text field" >&2; exit 1; } ;;
	stylus) "$ADB" shell input stylus swipe "$2" "$3" "$4" "$5" "${6:-400}" ;;
	text) ui | python3 -I -c '
import html, re, sys
x = sys.stdin.read()
m = re.search(r"<node [^>]*class=\"android.widget.EditText\"[^>]*focused=\"true\"[^>]*>", x) or re.search(r"<node [^>]*class=\"android.widget.EditText\"[^>]*>", x)
print(html.unescape(re.search(r"text=\"([^\"]*)\"", m.group(0)).group(1)) if m else "")' ;;
	stop) "$ADB" emu kill ;;
	*) sed -n '2,19p' "$0"; exit 2 ;;
esac
