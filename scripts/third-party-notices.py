#!/usr/bin/env python3
"""Writes THIRD_PARTY_NOTICES.md: what n0te ships that others made, and under which licenses.

    scripts/third-party-notices.py > THIRD_PARTY_NOTICES.md

The Rust part comes from `cargo metadata`: every crate that ends up in a shipped program
(n0te Laptop and n0te Tablet = n0te-desktop, n0te-sync, the Android library n0te-ffi) for the
shipped targets, normal dependencies only (build tools and test crates are not shipped).
Fonts, pdfium and the Android libraries are listed by hand below; keep them current when a
dependency changes (android/build.gradle.kts, crates/desktop/fonts, scripts/fetch-pdfium.sh).
"""

import collections
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SHIPPED = ["n0te-desktop", "n0te-sync", "n0te-ffi"]
TARGETS = ["x86_64-pc-windows-msvc", "aarch64-pc-windows-msvc", "x86_64-unknown-linux-gnu", "aarch64-linux-android"]

HEAD = """# Third-party notices

n0te itself is proprietary (see [LICENSE](LICENSE): all rights reserved). It is built on, and
ships, software by others. Those components remain under their own licenses, listed here.
Nothing in n0te's license restricts the rights these licenses grant you for the components.

The Windows installer and portable zip carry this file as `THIRD_PARTY_NOTICES.txt` and the
full license texts of pdfium and the fonts in the `licenses` folder.

## Fonts

Bundled in n0te Laptop / n0te Tablet (`crates/desktop/fonts`) and, except Noto Serif, in the
Android app (`android/src/main/res/font`). All under the
[SIL Open Font License 1.1](https://openfontlicense.org); the license texts are next to the
fonts (`*-OFL.txt`, `android/src/main/assets/licenses`) and in `licenses/fonts` of the Windows
packages.

| Font | Copyright |
|---|---|
{fonts}

## pdfium

PDF rendering, text and export on every platform: [PDFium](https://pdfium.googlesource.com/pdfium/)
{pdfium_version}, prebuilt by [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)
(`scripts/fetch-pdfium.sh`, SHA-256 pinned). PDFium is under the BSD 3-Clause License
(Copyright 2014 The PDFium Authors) with parts under Apache License 2.0; it includes code from
abseil (Apache-2.0), AGG 2.3, dragonbox (Apache-2.0 with LLVM exception / Boost), fast_float,
FreeType (FreeType License), HarfBuzz (MIT), Highway (Apache-2.0), ICU (Unicode License),
Little CMS (MIT), libjpeg-turbo (IJG / BSD), OpenJPEG (BSD-2-Clause), libpng (libpng License),
LLVM libc (Apache-2.0 with LLVM exception), simdutf and zlib (zlib License). The complete texts
ship with every build: `licenses/pdfium` in the Windows packages, `LICENSE` and `licenses/` in
each pdfium-binaries archive.

## Android app libraries

| Library | License |
|---|---|
| AndroidX (core-ktx, activity-compose, graphics-core, input-motionprediction) and Jetpack Compose (ui, material3) | Apache License 2.0 |
| Kotlin standard library, kotlinx.coroutines | Apache License 2.0 |
| JNA 5.19.1 (`net.java.dev.jna:jna`) | Apache License 2.0 (dual-licensed with LGPL-2.1-or-later; used under Apache-2.0) |
| Google code scanner (`com.google.android.gms:play-services-code-scanner`) and ML Kit digital ink recognition (`com.google.mlkit:digital-ink-recognition`) | [ML Kit Terms of Service](https://developers.google.com/ml-kit/terms), [Android SDK License](https://developer.android.com/studio/terms) (Google Play services) |
| JLaTeXMath for Android (`ru.noties:jlatexmath-android` 0.2.0) | GNU General Public License v2 (as its Maven POM states; upstream [JLaTeXMath](https://github.com/opencollab/jlatexmath) is GPL-2.0-or-later with a linking exception) |

## Rust crates

{crate_count} crates from [crates.io](https://crates.io) are compiled into n0te Laptop,
n0te Tablet, n0te-sync and the Android app's native library. Each crate's license file and
copyright notice are in its source package on crates.io (and in `~/.cargo/registry` after a
build). Licenses, by how many crates use them:

| License (SPDX) | Crates |
|---|---|
{license_counts}

<details>
<summary>All crates with version and license</summary>

| Crate | Version | License | Authors |
|---|---|---|---|
{crate_rows}

</details>

### License texts

The common license texts (MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, MPL-2.0, Unicode-3.0,
BSL-1.0, CC0-1.0) are at <https://spdx.org/licenses/>. MIT-licensed crates require their
copyright notice to be kept: it is in each crate's `LICENSE-MIT` / `LICENSE` file, under the
names in the Authors column. MPL-2.0 crates (e.g. UniFFI's runtime) are unchanged; their source
is on crates.io.
"""


def cargo_metadata():
    cargo = os.environ.get("CARGO") or os.path.expanduser("~/.cargo/bin/cargo")
    if not os.path.exists(cargo):
        cargo = "cargo"
    args = [cargo, "metadata", "--format-version", "1", "--locked", "--manifest-path", os.path.join(ROOT, "Cargo.toml")]
    for t in TARGETS:
        args += ["--filter-platform", t]
    return json.loads(subprocess.check_output(args, cwd=ROOT))


def shipped_crates(meta):
    pkgs = {p["id"]: p for p in meta["packages"]}
    nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
    by_name = {p["name"]: p["id"] for p in meta["packages"] if p["source"] is None}
    todo = [by_name[n] for n in SHIPPED]
    seen = set()
    while todo:
        i = todo.pop()
        if i in seen:
            continue
        seen.add(i)
        for d in nodes[i]["deps"]:
            if any(k["kind"] is None for k in d["dep_kinds"]):
                todo.append(d["pkg"])
    return sorted((pkgs[i] for i in seen if pkgs[i]["source"] is not None), key=lambda p: (p["name"], p["version"]))


def spdx(expr):
    """One spelling per license expression: "A / B" and "B OR A" both become "A OR B"."""
    e = " ".join((expr or "see crate").replace("/", " OR ").split())
    if "(" not in e and " AND " not in e and " WITH " not in e:
        e = " OR ".join(sorted(e.split(" OR ")))
    return e


def cell(s):
    return (s or "").replace("|", "\\|").replace("\n", " ").strip()


def main():
    meta = cargo_metadata()
    crates = shipped_crates(meta)
    counts = collections.Counter(spdx(p.get("license")) for p in crates)
    rows = []
    for p in crates:
        authors = ", ".join(a.split("<")[0].strip() for a in p.get("authors") or [])
        rows.append(f"| {cell(p['name'])} | {p['version']} | {cell(spdx(p.get('license')))} | {cell(authors) or '–'} |")
    fonts = []
    fdir = os.path.join(ROOT, "crates/desktop/fonts")
    for f in sorted(os.listdir(fdir)):
        if f.endswith("-OFL.txt"):
            first = open(os.path.join(fdir, f), encoding="utf-8").readline().strip()
            fonts.append(f"| {f[: -len('-OFL.txt')]} | {cell(first)} |")
    version = "chromium/8165"
    vfile = os.path.join(ROOT, "scripts/fetch-pdfium.sh")
    for line in open(vfile, encoding="utf-8"):
        if line.startswith("PDFIUM_TAG="):
            version = line.split('"')[1]
    sys.stdout.write(
        HEAD.replace("{fonts}", "\n".join(fonts))
        .replace("{pdfium_version}", f"({version})")
        .replace("{crate_count}", str(len(crates)))
        .replace("{license_counts}", "\n".join(f"| {cell(k)} | {v} |" for k, v in counts.most_common()))
        .replace("{crate_rows}", "\n".join(rows))
    )


if __name__ == "__main__":
    main()
