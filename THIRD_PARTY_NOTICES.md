# Third-party notices

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
| Doto | Copyright 2024 The Doto Project Authors (https://github.com/oliverlalan/Doto) |
| NotoSerif | Copyright 2022 The Noto Project Authors (https://github.com/notofonts/latin-greek-cyrillic) |
| SpaceGrotesk | Copyright 2020 The Space Grotesk Project Authors (https://github.com/floriankarsten/space-grotesk) |
| SpaceMono | Copyright 2016 The Space Mono Project Authors (https://github.com/googlefonts/spacemono) |

## pdfium

PDF rendering, text and export on every platform: [PDFium](https://pdfium.googlesource.com/pdfium/)
(chromium/8165), prebuilt by [bblanchon/pdfium-binaries](https://github.com/bblanchon/pdfium-binaries)
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

678 crates from [crates.io](https://crates.io) are compiled into n0te Laptop,
n0te Tablet, n0te-sync and the Android app's native library. Each crate's license file and
copyright notice are in its source package on crates.io (and in `~/.cargo/registry` after a
build). Licenses, by how many crates use them:

| License (SPDX) | Crates |
|---|---|
| Apache-2.0 OR MIT | 396 |
| MIT | 160 |
| Apache-2.0 OR MIT OR Zlib | 19 |
| Unicode-3.0 | 18 |
| Apache-2.0 | 12 |
| BSD-3-Clause | 10 |
| MIT OR Unlicense | 8 |
| MPL-2.0 | 8 |
| BSD-2-Clause | 7 |
| Zlib | 4 |
| Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | 4 |
| BSL-1.0 | 3 |
| Apache-2.0 OR ISC OR MIT | 3 |
| ISC | 3 |
| MPL-2.0+ | 2 |
| 0BSD | 2 |
| Apache-2.0 OR BSD-3-Clause | 2 |
| Apache-2.0 OR BSD-3-Clause OR MIT | 2 |
| (MIT OR Apache-2.0) AND Unicode-3.0 | 2 |
| Apache-2.0 OR BSD-2-Clause OR MIT | 2 |
| 0BSD OR Apache-2.0 OR MIT | 1 |
| CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | 1 |
| Apache-2.0 OR CC0-1.0 OR MIT-0 | 1 |
| Apache-2.0 AND MIT | 1 |
| CC0-1.0 | 1 |
| Apache-2.0 OR CC0-1.0 | 1 |
| Apache-2.0 AND ISC | 1 |
| (MIT OR Apache-2.0) AND ISC | 1 |
| Apache-2.0 OR BSL-1.0 | 1 |
| Apache-2.0 OR GPL-2.0-only | 1 |
| CDLA-Permissive-2.0 | 1 |

<details>
<summary>All crates with version and license</summary>

| Crate | Version | License | Authors |
|---|---|---|---|
| ab_glyph | 0.2.32 | Apache-2.0 | Alex Butler |
| ab_glyph_rasterizer | 0.1.10 | Apache-2.0 | Alex Butler |
| adler2 | 2.0.1 | 0BSD OR Apache-2.0 OR MIT | Jonas Schievink, oyvindln |
| aead | 0.5.2 | Apache-2.0 OR MIT | RustCrypto Developers |
| aes | 0.8.4 | Apache-2.0 OR MIT | RustCrypto Developers |
| aes-gcm | 0.10.3 | Apache-2.0 OR MIT | RustCrypto Developers |
| ahash | 0.8.12 | Apache-2.0 OR MIT | Tom Kaitchuck |
| aho-corasick | 1.1.5 | MIT OR Unlicense | Andrew Gallant |
| aligned | 0.4.3 | Apache-2.0 OR MIT | – |
| aligned-vec | 0.6.4 | MIT | sarah |
| android-activity | 0.6.1 | Apache-2.0 OR MIT | – |
| android-properties | 0.2.2 | MIT | Mikhail Lappo |
| android_system_properties | 0.1.6 | Apache-2.0 OR MIT | Nicolas Silva |
| anstyle | 1.0.14 | Apache-2.0 OR MIT | – |
| anyhow | 1.0.104 | Apache-2.0 OR MIT | David Tolnay |
| append-only-bytes | 0.1.12 | MIT | zxch3n |
| arbitrary | 1.5.0 | Apache-2.0 OR MIT | The Rust-Fuzz Project Developers, Nick Fitzgerald, Manish Goregaokar, Simonas Kazlauskas, Brian L. Troutwine, Corey Farwell |
| arc-swap | 1.9.5 | Apache-2.0 OR MIT | Michal 'vorner' Vaner |
| archery | 1.2.3 | MIT | Diogo Sousa |
| arg_enum_proc_macro | 0.3.4 | MIT | Luca Barbato |
| arrayref | 0.3.9 | BSD-2-Clause | David Roundy |
| arrayvec | 0.7.8 | Apache-2.0 OR MIT | bluss |
| arref | 0.1.0 | MIT | zxch3n |
| as-raw-xcb-connection | 1.0.1 | Apache-2.0 OR MIT | – |
| as-slice | 0.2.1 | Apache-2.0 OR MIT | Jorge Aparicio, Emil Fresk |
| ash | 0.38.0+1.3.281 | Apache-2.0 OR MIT | Maik Klein, Benjamin Saunders, Marijn Suijten |
| askama | 0.16.1 | Apache-2.0 OR MIT | – |
| askama_derive | 0.16.1 | Apache-2.0 OR MIT | – |
| askama_macros | 0.16.1 | Apache-2.0 OR MIT | – |
| askama_parser | 0.16.1 | Apache-2.0 OR MIT | – |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | Stjepan Glavina, Contributors to futures-rs |
| av-scenechange | 0.14.1 | MIT | Josh Holmer |
| av1-grain | 0.2.5 | BSD-2-Clause | – |
| avif-serialize | 0.8.9 | BSD-3-Clause | Kornel Lesiński |
| backon | 1.6.0 | Apache-2.0 | – |
| base16ct | 1.0.0 | Apache-2.0 OR MIT | RustCrypto Developers |
| base64 | 0.22.1 | Apache-2.0 OR MIT | Marshall Pierce |
| base64 | 0.23.1 | Apache-2.0 OR MIT | Marshall Pierce |
| base64ct | 1.8.3 | Apache-2.0 OR MIT | RustCrypto Developers |
| basic-toml | 0.1.10 | Apache-2.0 OR MIT | Alex Crichton, David Tolnay |
| bincode | 1.3.3 | MIT | Ty Overby, Francesco Mazzoli, David Tolnay, Zoey Riordan |
| bit-set | 0.8.0 | Apache-2.0 OR MIT | Alexis Beingessner |
| bit-vec | 0.8.0 | Apache-2.0 OR MIT | Alexis Beingessner |
| bit_field | 0.10.4 | Apache-2.0 OR MIT | Philipp Oppermann |
| bitflags | 1.3.2 | Apache-2.0 OR MIT | The Rust Project Developers |
| bitflags | 2.13.2 | Apache-2.0 OR MIT | The Rust Project Developers |
| bitstream-io | 4.10.0 | Apache-2.0 OR MIT | Brian Langenberger |
| blake3 | 1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception | Jack O'Connor, Samuel Neves |
| block-buffer | 0.10.4 | Apache-2.0 OR MIT | RustCrypto Developers |
| block-buffer | 0.12.1 | Apache-2.0 OR MIT | RustCrypto Developers |
| bumpalo | 3.20.4 | Apache-2.0 OR MIT | Nick Fitzgerald |
| bytemuck | 1.25.2 | Apache-2.0 OR MIT OR Zlib | Lokathor |
| bytemuck_derive | 1.12.1 | Apache-2.0 OR MIT OR Zlib | Lokathor |
| byteorder | 1.5.0 | MIT OR Unlicense | Andrew Gallant |
| byteorder-lite | 0.1.0 | MIT OR Unlicense | – |
| bytes | 1.12.2 | MIT | Carl Lerche, Sean McArthur |
| calloop | 0.13.0 | MIT | Elinor Berger |
| calloop | 0.14.5 | MIT | Elinor Berger |
| calloop-wayland-source | 0.3.0 | MIT | Kirill Chibisov |
| calloop-wayland-source | 0.4.1 | MIT | Kirill Chibisov |
| camino | 1.2.6 | Apache-2.0 OR MIT | Without Boats, Ashley Williams, Steve Klabnik, Rain |
| cargo-platform | 0.3.3 | Apache-2.0 OR MIT | – |
| cargo_metadata | 0.23.1 | MIT | Oliver Schneider |
| cesu8 | 1.1.0 | Apache-2.0 OR MIT | Eric Kidd |
| cfg-if | 1.0.5 | Apache-2.0 OR MIT | Alex Crichton |
| chacha20 | 0.10.2 | Apache-2.0 OR MIT | RustCrypto Developers |
| chrono | 0.4.45 | Apache-2.0 OR MIT | – |
| cipher | 0.4.4 | Apache-2.0 OR MIT | RustCrypto Developers |
| clap | 4.6.8 | Apache-2.0 OR MIT | – |
| clap_builder | 4.6.7 | Apache-2.0 OR MIT | – |
| clap_derive | 4.6.8 | Apache-2.0 OR MIT | – |
| clap_lex | 1.1.2 | Apache-2.0 OR MIT | – |
| clipboard-win | 5.4.3 | BSL-1.0 | Douman |
| clipboard_wayland | 0.2.2 | Apache-2.0 | Héctor Ramón Jiménez |
| clipboard_x11 | 0.4.3 | MIT | Héctor Ramón Jiménez |
| cmov | 0.5.4 | Apache-2.0 OR MIT | RustCrypto Developers |
| cobs | 0.3.0 | Apache-2.0 OR MIT | Allen Welkie, James Munns |
| codespan-reporting | 0.12.0 | Apache-2.0 | Brendan Zabarauskas |
| color_quant | 1.1.0 | MIT | nwin |
| combine | 4.6.9 | MIT | Markus Westerlind |
| concurrent-queue | 2.5.0 | Apache-2.0 OR MIT | Stjepan Glavina, Taiki Endo, John Nunley |
| const-oid | 0.10.3 | Apache-2.0 OR MIT | RustCrypto Developers |
| constant_time_eq | 0.4.2 | Apache-2.0 OR CC0-1.0 OR MIT-0 | Cesar Eduardo Barros |
| convert_case | 0.10.0 | MIT | rutrum |
| cordyceps | 0.3.5 | MIT | Eliza Weisman |
| core_maths | 0.1.1 | MIT | Robert Bastian |
| cosmic-text | 0.15.0 | Apache-2.0 OR MIT | Jeremy Soller |
| cpufeatures | 0.2.17 | Apache-2.0 OR MIT | RustCrypto Developers |
| cpufeatures | 0.3.1 | Apache-2.0 OR MIT | RustCrypto Developers |
| crc32fast | 1.5.5 | Apache-2.0 OR MIT | Sam Rijs, Alex Crichton |
| crossbeam-deque | 0.8.8 | Apache-2.0 OR MIT | – |
| crossbeam-epoch | 0.9.21 | Apache-2.0 OR MIT | – |
| crossbeam-utils | 0.8.23 | Apache-2.0 OR MIT | – |
| cryoglyph | 0.1.0 | Apache-2.0 OR MIT OR Zlib | Héctor Ramón Jiménez |
| crypto-common | 0.1.7 | Apache-2.0 OR MIT | RustCrypto Developers |
| crypto-common | 0.2.2 | Apache-2.0 OR MIT | RustCrypto Developers |
| ctor | 0.10.1 | Apache-2.0 OR MIT | Matt Mastracci |
| ctr | 0.9.2 | Apache-2.0 OR MIT | RustCrypto Developers |
| ctutils | 0.4.8 | Apache-2.0 OR MIT | RustCrypto Developers |
| cursor-icon | 1.2.0 | Apache-2.0 OR MIT OR Zlib | Kirill Chibisov |
| curve25519-dalek | 5.0.0 | BSD-3-Clause | Isis Lovecruft, Henry de Valence |
| curve25519-dalek-derive | 0.1.1 | Apache-2.0 OR MIT | – |
| darling | 0.20.11 | MIT | Ted Driggs |
| darling_core | 0.20.11 | MIT | Ted Driggs |
| darling_macro | 0.20.11 | MIT | Ted Driggs |
| data-encoding | 2.11.1 | MIT | – |
| data-encoding-macro | 0.1.21 | MIT | – |
| data-encoding-macro-internal | 0.1.19 | MIT | – |
| data-url | 0.3.2 | Apache-2.0 OR MIT | Simon Sapin |
| der | 0.8.3 | Apache-2.0 OR MIT | RustCrypto Developers |
| deranged | 0.5.9 | Apache-2.0 OR MIT | Jacob Pratt |
| derive_arbitrary | 1.5.0 | Apache-2.0 OR MIT | The Rust-Fuzz Project Developers, Nick Fitzgerald, Manish Goregaokar, Andre Bogus, Corey Farwell |
| derive_more | 2.1.1 | MIT | Jelte Fennema |
| derive_more-impl | 2.1.1 | MIT | Jelte Fennema |
| diatomic-waker | 0.2.3 | Apache-2.0 OR MIT | Asynchronics and contributors |
| diff | 0.1.13 | Apache-2.0 OR MIT | Utkarsh Kukreti |
| digest | 0.10.7 | Apache-2.0 OR MIT | RustCrypto Developers |
| digest | 0.11.3 | Apache-2.0 OR MIT | RustCrypto Developers |
| displaydoc | 0.2.7 | Apache-2.0 OR MIT | Jane Lusby |
| dlib | 0.5.3 | MIT | Elinor Berger |
| dlopen2 | 0.8.2 | MIT | Szymon Wieloch, Ahmed Masud, OpenByte |
| document-features | 0.2.12 | Apache-2.0 OR MIT | Slint Developers |
| downcast-rs | 1.2.1 | Apache-2.0 OR MIT | Ashish Myles, Runji Wang |
| dpi | 0.1.2 | Apache-2.0 AND MIT | – |
| dtor | 0.8.1 | Apache-2.0 OR MIT | Matt Mastracci |
| ed25519 | 3.0.0 | Apache-2.0 OR MIT | RustCrypto Developers |
| ed25519-dalek | 3.0.0 | BSD-3-Clause | isis lovecruft, Tony Arcieri, Michael Rosenberg |
| either | 1.19.0 | Apache-2.0 OR MIT | – |
| embedded-io | 0.4.0 | Apache-2.0 OR MIT | – |
| embedded-io | 0.6.1 | Apache-2.0 OR MIT | – |
| ensure-cov | 0.1.0 | Apache-2.0 OR MIT | Zixuan Chen |
| enum-as-inner | 0.5.1 | Apache-2.0 OR MIT | Benjamin Fry |
| enum-as-inner | 0.6.1 | Apache-2.0 OR MIT | Benjamin Fry |
| enum-assoc | 1.4.2 | Apache-2.0 OR MIT | Griffin O'Neill |
| enum_dispatch | 0.3.13 | Apache-2.0 OR MIT | Anton Lazarev |
| equator | 0.4.2 | MIT | sarah |
| equator-macro | 0.4.2 | MIT | sarah |
| equivalent | 1.0.3 | Apache-2.0 OR MIT | – |
| errno | 0.3.15 | Apache-2.0 OR MIT | Chris Wong, Dan Gohman |
| error-code | 3.4.0 | BSL-1.0 | Douman |
| etagere | 0.2.15 | Apache-2.0 OR MIT | Nicolas Silva |
| euclid | 0.22.14 | Apache-2.0 OR MIT | The Servo Project Developers |
| exr | 1.74.3 | BSD-3-Clause | johannesvollmer |
| fancy-regex | 0.16.2 | MIT | Raph Levien, Robin Stocker, Keith Hall |
| fastrand | 2.5.0 | Apache-2.0 OR MIT | Stjepan Glavina |
| fax | 0.2.7 | MIT | Sebastian K |
| fdeflate | 0.3.7 | Apache-2.0 OR MIT | The image-rs Developers |
| fixedbitset | 0.5.9 | Apache-2.0 OR MIT | bluss |
| flate2 | 1.1.11 | Apache-2.0 OR MIT | Alex Crichton, Josh Triplett |
| float-cmp | 0.9.0 | MIT | Mike Dilger |
| float_next_after | 1.0.0 | MIT | Bronson Brown-deVost |
| fnv | 1.0.7 | Apache-2.0 OR MIT | Alex Crichton |
| foldhash | 0.1.5 | Zlib | Orson Peters |
| foldhash | 0.2.0 | Zlib | Orson Peters |
| font-types | 0.10.1 | Apache-2.0 OR MIT | – |
| font-types | 0.12.7 | Apache-2.0 OR MIT | – |
| fontconfig-parser | 0.5.8 | MIT | – |
| fontdb | 0.23.0 | MIT | Yevhenii Reizner |
| form_urlencoded | 1.2.2 | Apache-2.0 OR MIT | The rust-url developers |
| fs-err | 3.3.2 | Apache-2.0 OR MIT | Andrew Hickman |
| futures | 0.3.34 | Apache-2.0 OR MIT | – |
| futures-buffered | 0.2.14 | MIT | Conrad Ludgate |
| futures-channel | 0.3.34 | Apache-2.0 OR MIT | – |
| futures-core | 0.3.34 | Apache-2.0 OR MIT | – |
| futures-executor | 0.3.34 | Apache-2.0 OR MIT | – |
| futures-io | 0.3.34 | Apache-2.0 OR MIT | – |
| futures-lite | 2.6.1 | Apache-2.0 OR MIT | Stjepan Glavina, Contributors to futures-rs |
| futures-macro | 0.3.34 | Apache-2.0 OR MIT | – |
| futures-sink | 0.3.34 | Apache-2.0 OR MIT | – |
| futures-task | 0.3.34 | Apache-2.0 OR MIT | – |
| futures-util | 0.3.34 | Apache-2.0 OR MIT | – |
| g2gen | 1.2.2 | Apache-2.0 OR MIT | WanzenBug |
| g2p | 1.2.2 | Apache-2.0 OR MIT | WanzenBug |
| g2poly | 1.2.2 | Apache-2.0 OR MIT | WanzenBug |
| generic-array | 0.14.7 | MIT | Bartłomiej Kamiński, Aaron Trent |
| generic-btree | 0.10.8 | MIT | zxch3n |
| gethostname | 1.1.0 | Apache-2.0 | Sebastian Wiesner |
| getrandom | 0.2.17 | Apache-2.0 OR MIT | The Rand Project Developers |
| getrandom | 0.3.4 | Apache-2.0 OR MIT | The Rand Project Developers |
| getrandom | 0.4.3 | Apache-2.0 OR MIT | The Rand Project Developers |
| ghash | 0.5.1 | Apache-2.0 OR MIT | RustCrypto Developers |
| gif | 0.13.3 | Apache-2.0 OR MIT | The image-rs Developers |
| gif | 0.14.2 | Apache-2.0 OR MIT | The image-rs Developers |
| glam | 0.25.0 | Apache-2.0 OR MIT | Cameron Hart |
| glob | 0.3.4 | Apache-2.0 OR MIT | The Rust Project Developers |
| glow | 0.16.0 | Apache-2.0 OR MIT OR Zlib | Joshua Groves, Dzmitry Malyshau |
| glutin_wgl_sys | 0.6.1 | Apache-2.0 | Kirill Chibisov |
| goblin | 0.8.2 | MIT | m4b, seu, Will Glynn, Philip Craig, Lzu Tao |
| gpu-alloc | 0.6.2 | Apache-2.0 OR MIT | Zakarum |
| gpu-alloc-types | 0.3.1 | Apache-2.0 OR MIT | Zakarum |
| gpu-allocator | 0.27.0 | Apache-2.0 OR MIT | Traverse Research |
| gpu-descriptor | 0.3.2 | Apache-2.0 OR MIT | Zakarum |
| gpu-descriptor-types | 0.2.0 | Apache-2.0 OR MIT | Zakarum |
| guillotiere | 0.6.2 | Apache-2.0 OR MIT | Nicolas Silva |
| h2 | 0.4.20 | MIT | Carl Lerche, Sean McArthur |
| half | 2.7.1 | Apache-2.0 OR MIT | Kathryn Long |
| harfrust | 0.3.2 | MIT | – |
| hash32 | 0.2.1 | Apache-2.0 OR MIT | Jorge Aparicio |
| hash32 | 0.3.1 | Apache-2.0 OR MIT | Jorge Aparicio |
| hashbrown | 0.15.5 | Apache-2.0 OR MIT | Amanieu d'Antras |
| hashbrown | 0.16.1 | Apache-2.0 OR MIT | Amanieu d'Antras |
| hashbrown | 0.17.1 | Apache-2.0 OR MIT | – |
| heapless | 0.7.17 | Apache-2.0 OR MIT | Jorge Aparicio, Per Lindgren, Emil Fresk |
| heapless | 0.8.0 | Apache-2.0 OR MIT | Jorge Aparicio, Per Lindgren, Emil Fresk |
| heapless | 0.9.4 | Apache-2.0 OR MIT | Jorge Aparicio, Per Lindgren, Emil Fresk |
| heck | 0.4.1 | Apache-2.0 OR MIT | Without Boats |
| heck | 0.5.0 | Apache-2.0 OR MIT | – |
| hexf-parse | 0.2.1 | CC0-1.0 | Kang Seonghoon |
| http | 1.5.0 | Apache-2.0 OR MIT | Alex Crichton, Carl Lerche, Sean McArthur |
| http-body | 1.1.0 | MIT | Carl Lerche, Lucio Franco, Sean McArthur |
| http-body-util | 0.1.5 | MIT | Carl Lerche, Lucio Franco, Sean McArthur |
| httparse | 1.10.1 | Apache-2.0 OR MIT | Sean McArthur |
| httpdate | 1.0.3 | Apache-2.0 OR MIT | Pyfisch |
| hybrid-array | 0.4.15 | Apache-2.0 OR MIT | RustCrypto Developers |
| hyper | 1.12.0 | MIT | Sean McArthur |
| hyper-rustls | 0.27.10 | Apache-2.0 OR ISC OR MIT | – |
| hyper-util | 0.1.21 | MIT | Sean McArthur |
| iana-time-zone | 0.1.65 | Apache-2.0 OR MIT | Andrew Straw, René Kijewski, Ryan Lopopolo |
| iced | 0.14.0 | MIT | Héctor Ramón Jiménez |
| iced_core | 0.14.0 | MIT | Héctor Ramón Jiménez |
| iced_debug | 0.14.0 | MIT | Héctor Ramón Jiménez |
| iced_futures | 0.14.0 | MIT | Héctor Ramón Jiménez |
| iced_graphics | 0.14.0 | MIT | Héctor Ramón Jiménez |
| iced_highlighter | 0.14.0 | MIT | Héctor Ramón Jiménez |
| iced_program | 0.14.0 | MIT | Héctor Ramón Jiménez |
| iced_renderer | 0.14.0 | MIT | Héctor Ramón Jiménez |
| iced_runtime | 0.14.0 | MIT | Héctor Ramón Jiménez |
| iced_tiny_skia | 0.14.1 | MIT | Héctor Ramón Jiménez |
| iced_wgpu | 0.14.0 | MIT | Héctor Ramón Jiménez |
| iced_widget | 0.14.2 | MIT | Héctor Ramón Jiménez |
| iced_winit | 0.14.1 | MIT | Héctor Ramón Jiménez |
| icu_collections | 2.4.0 | Unicode-3.0 | The ICU4X Project Developers |
| icu_locale_core | 2.4.0 | Unicode-3.0 | The ICU4X Project Developers |
| icu_normalizer | 2.4.0 | Unicode-3.0 | The ICU4X Project Developers |
| icu_normalizer_data | 2.4.0 | Unicode-3.0 | The ICU4X Project Developers |
| icu_properties | 2.4.0 | Unicode-3.0 | The ICU4X Project Developers |
| icu_properties_data | 2.4.0 | Unicode-3.0 | The ICU4X Project Developers |
| icu_provider | 2.4.0 | Unicode-3.0 | The ICU4X Project Developers |
| ident_case | 1.0.1 | Apache-2.0 OR MIT | Ted Driggs |
| identity-hash | 0.1.0 | Apache-2.0 OR MIT | – |
| idna | 1.1.0 | Apache-2.0 OR MIT | The rust-url developers |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT | The rust-url developers |
| igd-next | 0.18.0 | MIT | Darius C, Simon Bernier St-Pierre |
| image | 0.25.10 | Apache-2.0 OR MIT | The image-rs Developers |
| image-webp | 0.2.4 | Apache-2.0 OR MIT | – |
| imagesize | 0.13.0 | MIT | Maid Dog |
| imbl | 7.0.2 | MPL-2.0+ | Bodil Stokke, Joe Neeman, Arthur Silva |
| imbl-sized-chunks | 0.2.0 | MPL-2.0+ | Bodil Stokke, Joe Neeman, Arthur Silva |
| imgref | 1.12.3 | Apache-2.0 OR CC0-1.0 | Kornel Lesiński |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | – |
| inout | 0.1.4 | Apache-2.0 OR MIT | RustCrypto Developers |
| ipconfig | 0.3.4 | Apache-2.0 OR MIT | Liran Ringel |
| ipnet | 2.12.2 | Apache-2.0 OR MIT | Kris Price |
| iroh | 1.4.2 | Apache-2.0 OR MIT | dignifiedquire, n0 team |
| iroh-base | 1.4.2 | Apache-2.0 OR MIT | n0 team |
| iroh-dns | 1.4.2 | Apache-2.0 OR MIT | n0 team |
| iroh-metrics | 1.0.3 | Apache-2.0 OR MIT | arqu, n0 team |
| iroh-metrics-derive | 1.0.1 | Apache-2.0 OR MIT | Frando, n0 team |
| iroh-relay | 1.4.2 | Apache-2.0 OR MIT | n0 team |
| itertools | 0.11.0 | Apache-2.0 OR MIT | bluss |
| itertools | 0.12.1 | Apache-2.0 OR MIT | bluss |
| itertools | 0.14.0 | Apache-2.0 OR MIT | bluss |
| itoa | 1.0.19 | Apache-2.0 OR MIT | David Tolnay |
| jni | 0.21.1 | Apache-2.0 OR MIT | Josh Chase |
| jni | 0.22.4 | Apache-2.0 OR MIT | jni team |
| jni-macros | 0.22.4 | Apache-2.0 OR MIT | – |
| jni-sys | 0.3.1 | Apache-2.0 OR MIT | Steven Fackler |
| jni-sys | 0.4.1 | Apache-2.0 OR MIT | Steven Fackler, Robert Bragg |
| jni-sys-macros | 0.4.1 | Apache-2.0 OR MIT | Robert Bragg |
| kamadak-exif | 0.6.2 | BSD-2-Clause | KAMADA Ken'ichi |
| khronos-egl | 6.0.0 | Apache-2.0 OR MIT | Timothée Haudebourg, Sean Kerr |
| kurbo | 0.10.4 | Apache-2.0 OR MIT | Raph Levien |
| kurbo | 0.11.3 | Apache-2.0 OR MIT | Raph Levien |
| leb128 | 0.2.7 | Apache-2.0 OR MIT | Nick Fitzgerald, Philip Craig |
| lebe | 0.5.3 | BSD-3-Clause | johannesvollmer |
| libc | 0.2.191 | Apache-2.0 OR MIT | – |
| libloading | 0.8.9 | ISC | Simonas Kazlauskas |
| libm | 0.2.16 | MIT | Alex Crichton, Amanieu d'Antras, Jorge Aparicio, Trevor Gross |
| lilt | 0.8.2 | MIT | cyypherus |
| linebender_resource_handle | 0.1.1 | Apache-2.0 OR MIT | – |
| linked-hash-map | 0.5.6 | Apache-2.0 OR MIT | Stepan Koltsov, Andrew Paseltiner |
| linux-raw-sys | 0.12.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Dan Gohman |
| linux-raw-sys | 0.4.15 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Dan Gohman |
| linuxvideo | 0.3.5 | 0BSD | – |
| litemap | 0.8.3 | Unicode-3.0 | The ICU4X Project Developers |
| litrs | 1.0.1 | Apache-2.0 OR MIT | Lukas Kalbertodt |
| lock_api | 0.4.14 | Apache-2.0 OR MIT | Amanieu d'Antras |
| log | 0.4.35 | Apache-2.0 OR MIT | The Rust Project Developers |
| loop9 | 0.1.5 | MIT | Kornel |
| loro | 1.18.0 | MIT | Zixuan Chen, Liang Zhao |
| loro-common | 1.18.0 | MIT | Zixuan Chen, Liang Zhao |
| loro-delta | 1.18.0 | MIT | Zixuan Chen, Liang Zhao |
| loro-internal | 1.18.0 | MIT | Zixuan Chen, Liang Zhao |
| loro-kv-store | 1.18.0 | MIT | Liang Zhao, Zixuan Chen |
| loro-rle | 1.6.0 | MIT | Zixuan Chen, Liang Zhao |
| loro-thunderdome | 0.6.2 | Apache-2.0 OR MIT | Lucien Greathouse |
| loro_fractional_index | 1.18.0 | MIT | Liang Zhao, Zixuan Chen |
| lru | 0.16.4 | MIT | Jerome Froelich |
| lru | 0.18.5 | MIT | Jerome Froelich |
| lru-slab | 0.1.3 | Apache-2.0 OR MIT OR Zlib | Benjamin Saunders |
| lyon | 1.0.19 | Apache-2.0 OR MIT | Nicolas Silva |
| lyon_algorithms | 1.0.22 | Apache-2.0 OR MIT | Nicolas Silva |
| lyon_geom | 1.0.20 | Apache-2.0 OR MIT | Nicolas Silva |
| lyon_path | 1.0.19 | Apache-2.0 OR MIT | Nicolas Silva |
| lyon_tessellation | 1.0.23 | Apache-2.0 OR MIT | Nicolas Silva |
| lz4_flex | 0.11.7 | MIT | Pascal Seitz, Arthur Silva, ticki |
| mac-addr | 0.3.0 | MIT | shellrow |
| maybe-owned | 0.3.4 | Apache-2.0 OR MIT | Philipp korber, darkstalker, daboross, Zac Burns (That3Percent) |
| maybe-rayon | 0.1.1 | MIT | – |
| md5 | 0.7.0 | Apache-2.0 OR MIT | Ivan Ukhov, Kamal Ahmad, Konstantin Stepanov, Lukas Kalbertodt, Nathan Musoke, Scott Mabin, Tony Arcieri, Wim de With, Yosef Dinerstein |
| memchr | 2.8.3 | MIT OR Unlicense | Andrew Gallant, bluss |
| memmap2 | 0.9.12 | Apache-2.0 OR MIT | Dan Burkert, Yevhenii Reizner, The Contributors |
| minimal-lexical | 0.2.1 | Apache-2.0 OR MIT | Alex Huszagh |
| miniz_oxide | 0.8.9 | Apache-2.0 OR MIT OR Zlib | Frommi, oyvindln, Rich Geldreich richgel99@gmail.com |
| miniz_oxide | 0.9.1 | Apache-2.0 OR MIT OR Zlib | Frommi, oyvindln, Rich Geldreich richgel99@gmail.com |
| mio | 1.2.5 | MIT | Carl Lerche, Thomas de Zeeuw, Tokio Contributors |
| moxcms | 0.8.1 | Apache-2.0 OR BSD-3-Clause | Radzivon Bartoshyk |
| multiversion | 0.8.1 | Apache-2.0 OR MIT | Caleb Zulawski |
| multiversion-macros | 0.8.1 | Apache-2.0 OR MIT | Caleb Zulawski |
| mutate_once | 0.1.2 | BSD-2-Clause | KAMADA Ken'ichi |
| n0-dns-resolver | 0.1.1 | Apache-2.0 OR MIT | n0 team |
| n0-error | 1.0.3 | Apache-2.0 OR MIT | Frando, n0 team |
| n0-error-macros | 1.0.3 | Apache-2.0 OR MIT | Frando, n0 team |
| n0-future | 0.3.2 | Apache-2.0 OR MIT | Philipp Krüger, n0 team |
| n0-watcher | 1.0.1 | Apache-2.0 OR MIT | dignifiedquire, n0 team |
| naga | 27.0.3 | Apache-2.0 OR MIT | gfx-rs developers |
| ndk | 0.9.0 | Apache-2.0 OR MIT | The Rust Mobile contributors |
| ndk-context | 0.1.1 | Apache-2.0 OR MIT | The Rust Windowing contributors |
| ndk-sys | 0.6.0+11769913 | Apache-2.0 OR MIT | The Rust Windowing contributors |
| netdev | 0.47.0 | MIT | shellrow |
| netlink-packet-core | 0.9.0 | MIT | Corentin Henry |
| netlink-packet-route | 0.33.0 | MIT | Corentin Henry |
| netlink-proto | 0.13.0 | MIT | Corentin Henry |
| netlink-sys | 0.9.0 | MIT | Corentin Henry |
| netwatch | 0.20.2 | Apache-2.0 OR MIT | n0 team |
| new_debug_unreachable | 1.0.6 | MIT | Matt Brubeck, Jonathan Reem |
| no_std_io2 | 0.9.4 | Apache-2.0 OR MIT | – |
| nom | 7.1.3 | MIT | contact@geoffroycouprie.com |
| nom | 8.0.0 | MIT | contact@geoffroycouprie.com |
| nonmax | 0.5.5 | Apache-2.0 OR MIT | Lucien Greathouse |
| noop_proc_macro | 0.3.0 | MIT | Luca Barbato |
| noq | 1.4.0 | Apache-2.0 OR MIT | – |
| noq-proto | 1.4.0 | Apache-2.0 OR MIT | – |
| noq-udp | 1.4.1 | Apache-2.0 OR MIT | – |
| num | 0.4.3 | Apache-2.0 OR MIT | The Rust Project Developers |
| num-bigint | 0.4.8 | Apache-2.0 OR MIT | The Rust Project Developers |
| num-complex | 0.4.6 | Apache-2.0 OR MIT | The Rust Project Developers |
| num-conv | 0.2.2 | Apache-2.0 OR MIT | Jacob Pratt |
| num-derive | 0.4.2 | Apache-2.0 OR MIT | The Rust Project Developers |
| num-integer | 0.1.47 | Apache-2.0 OR MIT | The Rust Project Developers |
| num-iter | 0.1.46 | Apache-2.0 OR MIT | The Rust Project Developers |
| num-rational | 0.4.2 | Apache-2.0 OR MIT | The Rust Project Developers |
| num-traits | 0.2.19 | Apache-2.0 OR MIT | The Rust Project Developers |
| num_enum | 0.7.6 | Apache-2.0 OR BSD-3-Clause OR MIT | Daniel Wagner-Hall, Daniel Henry-Mantilla, Vincent Esche |
| num_enum_derive | 0.7.6 | Apache-2.0 OR BSD-3-Clause OR MIT | Daniel Wagner-Hall, Daniel Henry-Mantilla, Vincent Esche |
| once_cell | 1.21.4 | Apache-2.0 OR MIT | Aleksey Kladov |
| opaque-debug | 0.3.1 | Apache-2.0 OR MIT | RustCrypto Developers |
| openssl-probe | 0.2.2 | Apache-2.0 OR MIT | Alex Crichton |
| ordered-float | 5.5.1 | MIT | Jonathan Reem, Matt Brubeck |
| owned_ttf_parser | 0.25.1 | Apache-2.0 | Alex Butler |
| papaya | 0.2.5 | MIT | Ibraheem Ahmed |
| parking | 2.2.1 | Apache-2.0 OR MIT | Stjepan Glavina, The Rust Project Developers |
| parking_lot | 0.12.5 | Apache-2.0 OR MIT | Amanieu d'Antras |
| parking_lot_core | 0.9.12 | Apache-2.0 OR MIT | Amanieu d'Antras |
| paste | 1.0.15 | Apache-2.0 OR MIT | David Tolnay |
| pastey | 0.1.1 | Apache-2.0 OR MIT | Aditya Kumar, David Tolnay |
| pdfium-render | 0.9.5 | Apache-2.0 OR MIT | Alastair Carey |
| pem-rfc7468 | 1.0.0 | Apache-2.0 OR MIT | RustCrypto Developers |
| percent-encoding | 2.3.2 | Apache-2.0 OR MIT | The rust-url developers |
| pico-args | 0.5.0 | MIT | Yevhenii Reizner |
| pin-project | 1.1.13 | Apache-2.0 OR MIT | – |
| pin-project-internal | 1.1.13 | Apache-2.0 OR MIT | – |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | – |
| piston-float | 1.0.1 | MIT | bvssvni |
| pkcs8 | 0.11.0 | Apache-2.0 OR MIT | RustCrypto Developers |
| plain | 0.2.3 | Apache-2.0 OR MIT | jzr |
| plist | 1.10.4 | MIT | Ed Barnard |
| png | 0.17.16 | Apache-2.0 OR MIT | The image-rs Developers |
| png | 0.18.1 | Apache-2.0 OR MIT | The image-rs Developers |
| polling | 3.11.0 | Apache-2.0 OR MIT | Stjepan Glavina, John Nunley |
| polyval | 0.6.2 | Apache-2.0 OR MIT | RustCrypto Developers |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT | – |
| portmapper | 0.20.2 | Apache-2.0 OR MIT | n0 team |
| postcard | 1.1.3 | Apache-2.0 OR MIT | James Munns |
| postcard-derive | 0.2.2 | Apache-2.0 OR MIT | Lachlan Sneff, James Munns |
| potential_utf | 0.1.6 | Unicode-3.0 | The ICU4X Project Developers |
| powerfmt | 0.2.1 | Apache-2.0 OR MIT | Jacob Pratt |
| ppv-lite86 | 0.2.21 | Apache-2.0 OR MIT | The CryptoCorrosion Contributors |
| presser | 0.3.1 | Apache-2.0 OR MIT | Embark, Gray Olson |
| pretty_assertions | 1.4.1 | Apache-2.0 OR MIT | Colin Kiegel, Florent Fayolle, Tom Milligan |
| proc-macro-crate | 3.5.0 | Apache-2.0 OR MIT | Bastian Köcher |
| proc-macro2 | 1.0.107 | Apache-2.0 OR MIT | David Tolnay, Alex Crichton |
| profiling | 1.0.18 | Apache-2.0 OR MIT | Philip Degarmo |
| profiling-procmacros | 1.0.18 | Apache-2.0 OR MIT | Philip Degarmo |
| pxfm | 0.1.30 | Apache-2.0 OR BSD-3-Clause | Radzivon Bartoshyk |
| qoi | 0.4.1 | Apache-2.0 OR MIT | Ivan Smirnov |
| qrcode | 0.13.0 | Apache-2.0 OR MIT | kennytm |
| quick-error | 2.0.1 | Apache-2.0 OR MIT | Paul Colomiets, Colin Kiegel |
| quick-xml | 0.41.0 | MIT | – |
| quick-xml | 0.42.0 | MIT | – |
| quick_cache | 0.6.24 | MIT | Arthur Silva |
| quote | 1.0.47 | Apache-2.0 OR MIT | David Tolnay |
| rand | 0.10.4 | Apache-2.0 OR MIT | The Rand Project Developers, The Rust Project Developers |
| rand | 0.8.8 | Apache-2.0 OR MIT | The Rand Project Developers, The Rust Project Developers |
| rand | 0.9.5 | Apache-2.0 OR MIT | The Rand Project Developers, The Rust Project Developers |
| rand_chacha | 0.3.1 | Apache-2.0 OR MIT | The Rand Project Developers, The Rust Project Developers, The CryptoCorrosion Contributors |
| rand_chacha | 0.9.0 | Apache-2.0 OR MIT | The Rand Project Developers, The Rust Project Developers, The CryptoCorrosion Contributors |
| rand_core | 0.10.1 | Apache-2.0 OR MIT | The Rand Project Developers |
| rand_core | 0.6.4 | Apache-2.0 OR MIT | The Rand Project Developers, The Rust Project Developers |
| rand_core | 0.9.5 | Apache-2.0 OR MIT | The Rand Project Developers, The Rust Project Developers |
| rand_pcg | 0.10.3 | Apache-2.0 OR MIT | The Rand Project Developers |
| rand_xoshiro | 0.7.0 | Apache-2.0 OR MIT | The Rand Project Developers |
| range-alloc | 0.1.5 | Apache-2.0 OR MIT | the gfx-rs Developers |
| rangemap | 1.8.0 | Apache-2.0 OR MIT | Jeff Parsons |
| rav1e | 0.8.1 | BSD-2-Clause | Thomas Daede |
| ravif | 0.13.0 | BSD-3-Clause | Kornel Lesiński |
| raw-window-handle | 0.6.2 | Apache-2.0 OR MIT OR Zlib | Osspial |
| rayon | 1.12.0 | Apache-2.0 OR MIT | – |
| rayon-core | 1.13.0 | Apache-2.0 OR MIT | – |
| read-fonts | 0.35.0 | Apache-2.0 OR MIT | – |
| read-fonts | 0.41.0 | Apache-2.0 OR MIT | – |
| regex-automata | 0.4.19 | Apache-2.0 OR MIT | The Rust Project Developers, Andrew Gallant |
| regex-syntax | 0.8.12 | Apache-2.0 OR MIT | The Rust Project Developers, Andrew Gallant |
| renderdoc-sys | 1.1.0 | Apache-2.0 OR MIT | Eyal Kalderon |
| reqwest | 0.13.5 | Apache-2.0 OR MIT | Sean McArthur |
| resvg | 0.45.1 | Apache-2.0 OR MIT | – |
| rgb | 0.8.53 | MIT | Kornel Lesiński, James Forster |
| ring | 0.17.14 | Apache-2.0 AND ISC | – |
| roxmltree | 0.20.0 | Apache-2.0 OR MIT | Yevhenii Reizner |
| rqrr | 0.11.0 | (MIT OR Apache-2.0) AND ISC | WanzenBug |
| rustc-hash | 1.1.0 | Apache-2.0 OR MIT | The Rust Project Developers |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT | The Rust Project Developers |
| rustix | 0.38.44 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Dan Gohman, Jakub Konka |
| rustix | 1.1.6 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | Dan Gohman, Jakub Konka |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT | – |
| rustls-native-certs | 0.8.5 | Apache-2.0 OR ISC OR MIT | – |
| rustls-pki-types | 1.16.0 | Apache-2.0 OR MIT | – |
| rustls-platform-verifier | 0.7.1 | Apache-2.0 OR MIT | – |
| rustls-platform-verifier-android | 0.2.0 | Apache-2.0 OR MIT | – |
| rustls-webpki | 0.103.17 | ISC | – |
| rustversion | 1.0.23 | Apache-2.0 OR MIT | David Tolnay |
| rustybuzz | 0.20.1 | MIT | Caleb Maclennan, Laurenz Stampfl, Yevhenii Reizner, خالد حسني (Khaled Hosny) |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | David Tolnay |
| safe_arch | 0.7.4 | Apache-2.0 OR MIT OR Zlib | Lokathor |
| same-file | 1.0.6 | MIT OR Unlicense | Andrew Gallant |
| schannel | 0.1.30 | MIT | Steven Fackler, Steffen Butzer |
| scoped-tls | 1.0.1 | Apache-2.0 OR MIT | Alex Crichton |
| scopeguard | 1.2.0 | Apache-2.0 OR MIT | bluss |
| scroll | 0.12.0 | MIT | m4b, Ted Mielczarek |
| scroll_derive | 0.12.1 | MIT | m4b, Ted Mielczarek, Systemcluster |
| sctk-adwaita | 0.10.1 | MIT | Poly |
| seize | 0.5.1 | MIT | Ibraheem Ahmed |
| self_cell | 1.3.0 | Apache-2.0 OR GPL-2.0-only | Lukas Bergdoll |
| semver | 1.0.28 | Apache-2.0 OR MIT | David Tolnay |
| serde | 1.0.229 | Apache-2.0 OR MIT | Erick Tryzelaar, David Tolnay |
| serde_bytes | 0.11.19 | Apache-2.0 OR MIT | David Tolnay |
| serde_columnar | 0.3.14 | Apache-2.0 OR MIT | leonzhao, zxch3n |
| serde_columnar_derive | 0.3.7 | Apache-2.0 OR MIT | leonzhao, zxch3n |
| serde_core | 1.0.229 | Apache-2.0 OR MIT | Erick Tryzelaar, David Tolnay |
| serde_derive | 1.0.229 | Apache-2.0 OR MIT | Erick Tryzelaar, David Tolnay |
| serde_json | 1.0.154 | Apache-2.0 OR MIT | – |
| serde_spanned | 1.1.3 | Apache-2.0 OR MIT | – |
| serdect | 0.4.3 | Apache-2.0 OR MIT | RustCrypto Developers |
| sha1_smol | 1.0.1 | BSD-3-Clause | Armin Ronacher |
| sha2 | 0.10.9 | Apache-2.0 OR MIT | RustCrypto Developers |
| sha2 | 0.11.1 | Apache-2.0 OR MIT | RustCrypto Developers |
| signal-hook-registry | 1.4.8 | Apache-2.0 OR MIT | Michal 'vorner' Vaner, Masaki Hara |
| signature | 3.0.0 | Apache-2.0 OR MIT | RustCrypto Developers |
| simd-adler32 | 0.3.10 | MIT | Marvin Countryman |
| simd_cesu8 | 1.2.0 | Apache-2.0 OR MIT | Sean C. Roach |
| simd_helpers | 0.1.0 | MIT | Luca Barbato |
| simdutf8 | 0.1.5 | Apache-2.0 OR MIT | Hans Kratz |
| simple-dns | 0.12.0 | MIT | Ilson Roberto Balliego Junior |
| simple-dns | 0.13.1 | MIT | Ilson Roberto Balliego Junior |
| simplecss | 0.2.2 | Apache-2.0 OR MIT | – |
| siphasher | 1.0.4 | Apache-2.0 OR MIT | Frank Denis |
| skrifa | 0.37.0 | Apache-2.0 OR MIT | – |
| skrifa | 0.44.0 | Apache-2.0 OR MIT | – |
| slab | 0.4.12 | MIT | Carl Lerche |
| slotmap | 1.1.1 | Zlib | Orson Peters |
| smallvec | 1.16.6 | Apache-2.0 OR MIT | The Servo Project Developers |
| smawk | 0.3.3 | MIT | Martin Geisler |
| smithay-client-toolkit | 0.19.2 | MIT | Elinor Berger, i509VCB, Ashley Wulber |
| smithay-client-toolkit | 0.20.0 | MIT | Elinor Berger, i509VCB, Ashley Wulber |
| smithay-clipboard | 0.7.3 | MIT | Kirill Chibisov, Elinor Berger |
| smol_str | 0.2.2 | Apache-2.0 OR MIT | Aleksey Kladov |
| socket2 | 0.6.5 | Apache-2.0 OR MIT | Alex Crichton, Thomas de Zeeuw |
| softbuffer | 0.4.8 | Apache-2.0 OR MIT | – |
| sorted-index-buffer | 0.2.1 | Apache-2.0 OR MIT | rklaehn, n0 team |
| spez | 0.1.2 | BSD-2-Clause | Mara Bos |
| spin | 0.10.1 | MIT | Mathijs van de Nes, John Ericson, Joshua Barretto |
| spin | 0.9.9 | MIT | Mathijs van de Nes, John Ericson, Joshua Barretto |
| spirv | 0.3.0+sdk-1.3.268.0 | Apache-2.0 | Lei Zhang |
| spki | 0.8.1 | Apache-2.0 OR MIT | RustCrypto Developers |
| stable_deref_trait | 1.2.1 | Apache-2.0 OR MIT | Robert Grosse |
| static_assertions | 1.1.0 | Apache-2.0 OR MIT | Nikolai Vazquez |
| strict-num | 0.1.1 | MIT | Yevhenii Reizner |
| strsim | 0.11.1 | MIT | Danny Guo, maxbachmann |
| strum | 0.28.0 | MIT | Peter Glotfelty |
| strum_macros | 0.28.0 | MIT | Peter Glotfelty |
| subtle | 2.6.1 | BSD-3-Clause | Isis Lovecruft, Henry de Valence |
| svg_fmt | 0.4.5 | Apache-2.0 OR MIT | Nicolas Silva |
| svgtypes | 0.15.3 | Apache-2.0 OR MIT | – |
| swash | 0.2.10 | Apache-2.0 OR MIT | Chad Brokaw |
| syn | 1.0.109 | Apache-2.0 OR MIT | David Tolnay |
| syn | 2.0.119 | Apache-2.0 OR MIT | David Tolnay |
| syn | 3.0.10 | Apache-2.0 OR MIT | David Tolnay |
| sync_wrapper | 1.0.2 | Apache-2.0 | Actyx AG |
| synstructure | 0.14.0 | MIT | Nika Layzell |
| syntect | 5.3.0 | MIT | Tristan Hume |
| sys-locale | 0.3.2 | Apache-2.0 OR MIT | 1Password |
| target-features | 0.1.6 | Apache-2.0 OR MIT | Caleb Zulawski |
| tempfile | 3.27.0 | Apache-2.0 OR MIT | Steven Allen, The Rust Project Developers, Ashley Mannix, Jason White |
| termcolor | 1.4.1 | MIT OR Unlicense | Andrew Gallant |
| textwrap | 0.16.4 | MIT | – |
| thiserror | 1.0.69 | Apache-2.0 OR MIT | David Tolnay |
| thiserror | 2.0.21 | Apache-2.0 OR MIT | David Tolnay |
| thiserror-impl | 1.0.69 | Apache-2.0 OR MIT | David Tolnay |
| thiserror-impl | 2.0.21 | Apache-2.0 OR MIT | David Tolnay |
| thread_local | 1.1.10 | Apache-2.0 OR MIT | Amanieu d'Antras |
| tiff | 0.11.3 | MIT | The image-rs Developers |
| time | 0.3.55 | Apache-2.0 OR MIT | Jacob Pratt, Time contributors |
| time-core | 0.1.9 | Apache-2.0 OR MIT | Jacob Pratt, Time contributors |
| time-macros | 0.2.32 | Apache-2.0 OR MIT | Jacob Pratt, Time contributors |
| tiny-skia | 0.11.4 | BSD-3-Clause | Yevhenii Reizner |
| tiny-skia-path | 0.11.4 | BSD-3-Clause | Yevhenii Reizner |
| tiny-xlib | 0.2.5 | Apache-2.0 OR MIT OR Zlib | John Nunley |
| tinystr | 0.8.5 | Unicode-3.0 | The ICU4X Project Developers |
| tinyvec | 1.13.3 | Apache-2.0 OR MIT OR Zlib | Lokathor |
| tokio | 1.54.1 | MIT | Tokio Contributors |
| tokio-macros | 2.7.2 | MIT | Tokio Contributors |
| tokio-rustls | 0.26.6 | Apache-2.0 OR MIT | – |
| tokio-stream | 0.1.19 | MIT | Tokio Contributors |
| tokio-util | 0.7.20 | MIT | Tokio Contributors |
| tokio-websockets | 0.13.3 | MIT | – |
| toml | 1.2.1+spec-1.1.0 | Apache-2.0 OR MIT | – |
| toml_datetime | 1.1.3+spec-1.1.0 | Apache-2.0 OR MIT | – |
| toml_edit | 0.25.19+spec-1.1.0 | Apache-2.0 OR MIT | – |
| toml_parser | 1.1.5+spec-1.1.0 | Apache-2.0 OR MIT | – |
| toml_writer | 1.2.0+spec-1.1.0 | Apache-2.0 OR MIT | – |
| tower | 0.5.3 | MIT | Tower Maintainers |
| tower-http | 0.6.11 | MIT | Tower Maintainers |
| tower-layer | 0.3.3 | MIT | Tower Maintainers |
| tower-service | 0.3.3 | MIT | Tower Maintainers |
| tracing | 0.1.44 | MIT | Eliza Weisman, Tokio Contributors |
| tracing-attributes | 0.1.31 | MIT | Tokio Contributors, Eliza Weisman, David Barsky |
| tracing-core | 0.1.36 | MIT | Tokio Contributors |
| try-lock | 0.2.5 | MIT | Sean McArthur |
| ttf-parser | 0.25.1 | Apache-2.0 OR MIT | Caleb Maclennan, Laurenz Stampfl, Yevhenii Reizner, خالد حسني (Khaled Hosny) |
| two-face | 0.4.5 | Apache-2.0 OR MIT | – |
| twox-hash | 2.1.5 | MIT | Jake Goulding |
| typenum | 1.20.1 | Apache-2.0 OR MIT | – |
| unicode-bidi | 0.3.18 | Apache-2.0 OR MIT | The Servo Project Developers |
| unicode-bidi-mirroring | 0.4.0 | Apache-2.0 OR MIT | Yevhenii Reizner |
| unicode-ccc | 0.4.0 | Apache-2.0 OR MIT | Yevhenii Reizner |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | David Tolnay |
| unicode-linebreak | 0.1.5 | Apache-2.0 | Axel Forsman |
| unicode-properties | 0.1.5 | (MIT OR Apache-2.0) AND Unicode-3.0 | Charles Lew, Manish Goregaokar |
| unicode-script | 0.5.9 | Apache-2.0 OR MIT | Manish Goregaokar |
| unicode-segmentation | 1.13.3 | Apache-2.0 OR MIT | kwantam, Manish Goregaokar |
| unicode-vo | 0.1.0 | Apache-2.0 OR MIT | Evgeniy Reizner |
| unicode-width | 0.2.3 | Apache-2.0 OR MIT | kwantam, Manish Goregaokar |
| unicode-xid | 0.2.6 | Apache-2.0 OR MIT | erick.tryzelaar, kwantam, Manish Goregaokar |
| uniffi | 0.32.2 | MPL-2.0 | – |
| uniffi_bindgen | 0.32.2 | MPL-2.0 | – |
| uniffi_core | 0.32.2 | MPL-2.0 | – |
| uniffi_internal_macros | 0.32.2 | MPL-2.0 | – |
| uniffi_macros | 0.32.2 | MPL-2.0 | – |
| uniffi_meta | 0.32.2 | MPL-2.0 | – |
| uniffi_pipeline | 0.32.2 | MPL-2.0 | – |
| uniffi_udl | 0.32.2 | MPL-2.0 | – |
| universal-hash | 0.5.1 | Apache-2.0 OR MIT | RustCrypto Developers |
| untrusted | 0.9.0 | ISC | Brian Smith |
| uoctl | 1.0.1 | 0BSD | – |
| url | 2.5.8 | Apache-2.0 OR MIT | The rust-url developers |
| usvg | 0.45.1 | Apache-2.0 OR MIT | – |
| utf16string | 0.2.0 | Apache-2.0 OR MIT | Floris Bruynooghe, Sentry |
| utf8_iter | 1.1.0 | Apache-2.0 OR MIT | Henri Sivonen |
| utf_types | 1.0.0 | Apache-2.0 OR MIT | Henri Sivonen |
| v_frame | 0.3.9 | BSD-2-Clause | Luca Barbato |
| vecmath | 1.0.0 | MIT | bvssvni |
| walkdir | 2.5.0 | MIT OR Unlicense | Andrew Gallant |
| want | 0.3.2 | MIT | Sean McArthur |
| wasm-bindgen | 0.2.130 | Apache-2.0 OR MIT | The wasm-bindgen Developers |
| wasm-bindgen-macro | 0.2.130 | Apache-2.0 OR MIT | The wasm-bindgen Developers |
| wasm-bindgen-macro-support | 0.2.130 | Apache-2.0 OR MIT | The wasm-bindgen Developers |
| wasm-bindgen-shared | 0.2.130 | Apache-2.0 OR MIT | The wasm-bindgen Developers |
| wayland-backend | 0.3.17 | MIT | Elinor Berger |
| wayland-client | 0.31.15 | MIT | Elinor Berger |
| wayland-csd-frame | 0.3.0 | MIT | Kirill Chibisov |
| wayland-cursor | 0.31.14 | MIT | Elinor Berger |
| wayland-protocols | 0.32.13 | MIT | Elinor Berger |
| wayland-protocols-experimental | 20250721.0.1 | MIT | Elinor Berger |
| wayland-protocols-misc | 0.3.12 | MIT | Elinor Berger |
| wayland-protocols-plasma | 0.3.12 | MIT | Elinor Berger |
| wayland-protocols-wlr | 0.3.12 | MIT | Elinor Berger |
| wayland-scanner | 0.31.11 | MIT | Elinor Berger |
| wayland-sys | 0.31.11 | MIT | Elinor Berger |
| web-time | 1.1.0 | Apache-2.0 OR MIT | – |
| webpki-roots | 1.0.12 | CDLA-Permissive-2.0 | – |
| weedle2 | 5.0.0 | MIT | Sharad Chand, Jan-Erik Rediger |
| weezl | 0.1.12 | Apache-2.0 OR MIT | The image-rs Developers |
| wgpu | 27.0.1 | Apache-2.0 OR MIT | gfx-rs developers |
| wgpu-core | 27.0.3 | Apache-2.0 OR MIT | gfx-rs developers |
| wgpu-core-deps-windows-linux-android | 27.0.0 | Apache-2.0 OR MIT | gfx-rs developers |
| wgpu-hal | 27.0.4 | Apache-2.0 OR MIT | gfx-rs developers |
| wgpu-types | 27.0.1 | Apache-2.0 OR MIT | gfx-rs developers |
| wide | 0.7.33 | Apache-2.0 OR MIT OR Zlib | Lokathor |
| widestring | 1.2.1 | Apache-2.0 OR MIT | – |
| winapi | 0.3.9 | Apache-2.0 OR MIT | Peter Atashian |
| winapi-util | 0.1.11 | MIT OR Unlicense | Andrew Gallant |
| window_clipboard | 0.5.1 | MIT | Héctor Ramón Jiménez |
| windows | 0.58.0 | Apache-2.0 OR MIT | Microsoft |
| windows | 0.62.2 | Apache-2.0 OR MIT | – |
| windows-collections | 0.3.2 | Apache-2.0 OR MIT | – |
| windows-core | 0.58.0 | Apache-2.0 OR MIT | Microsoft |
| windows-core | 0.62.2 | Apache-2.0 OR MIT | – |
| windows-future | 0.3.2 | Apache-2.0 OR MIT | – |
| windows-implement | 0.58.0 | Apache-2.0 OR MIT | Microsoft |
| windows-implement | 0.60.2 | Apache-2.0 OR MIT | – |
| windows-interface | 0.58.0 | Apache-2.0 OR MIT | Microsoft |
| windows-interface | 0.59.3 | Apache-2.0 OR MIT | – |
| windows-link | 0.2.1 | Apache-2.0 OR MIT | – |
| windows-numerics | 0.3.1 | Apache-2.0 OR MIT | – |
| windows-registry | 0.6.1 | Apache-2.0 OR MIT | – |
| windows-result | 0.2.0 | Apache-2.0 OR MIT | Microsoft |
| windows-result | 0.4.1 | Apache-2.0 OR MIT | – |
| windows-strings | 0.1.0 | Apache-2.0 OR MIT | Microsoft |
| windows-strings | 0.5.1 | Apache-2.0 OR MIT | – |
| windows-sys | 0.45.0 | Apache-2.0 OR MIT | Microsoft |
| windows-sys | 0.52.0 | Apache-2.0 OR MIT | Microsoft |
| windows-sys | 0.59.0 | Apache-2.0 OR MIT | Microsoft |
| windows-sys | 0.61.2 | Apache-2.0 OR MIT | – |
| windows-targets | 0.42.2 | Apache-2.0 OR MIT | Microsoft |
| windows-targets | 0.52.6 | Apache-2.0 OR MIT | Microsoft |
| windows-threading | 0.2.1 | Apache-2.0 OR MIT | – |
| windows_aarch64_msvc | 0.42.2 | Apache-2.0 OR MIT | Microsoft |
| windows_aarch64_msvc | 0.52.6 | Apache-2.0 OR MIT | Microsoft |
| windows_x86_64_gnu | 0.52.6 | Apache-2.0 OR MIT | Microsoft |
| windows_x86_64_msvc | 0.42.2 | Apache-2.0 OR MIT | Microsoft |
| windows_x86_64_msvc | 0.52.6 | Apache-2.0 OR MIT | Microsoft |
| winit | 0.30.13 | Apache-2.0 | The winit contributors, Pierre Krieger |
| winnow | 1.0.5 | MIT | – |
| wmi | 0.18.4 | Apache-2.0 OR MIT | Ohad Ravid |
| writeable | 0.6.4 | Unicode-3.0 | The ICU4X Project Developers |
| x11-dl | 2.21.0 | MIT | daggerbot, Erle Pereira, AltF02 |
| x11rb | 0.13.2 | Apache-2.0 OR MIT | Uli Schlachter, Eduardo Sánchez Muñoz, notgull |
| x11rb-protocol | 0.13.2 | Apache-2.0 OR MIT | Uli Schlachter, Eduardo Sánchez Muñoz, notgull |
| xcursor | 0.3.11 | MIT | Samuele Esposito |
| xkbcommon-dl | 0.4.2 | MIT | Francesca Frangipane |
| xkeysym | 0.2.1 | Apache-2.0 OR MIT OR Zlib | John Nunley |
| xml-rs | 0.8.29 | MIT | Vladimir Matveev |
| xmltree | 0.10.3 | MIT | Andrew Chin |
| xmlwriter | 0.1.0 | MIT | Evgeniy Reizner |
| xxhash-rust | 0.8.21 | BSL-1.0 | Douman |
| y4m | 0.8.0 | MIT | Kagami Hiiragi |
| yaml-rust | 0.4.5 | Apache-2.0 OR MIT | Yuheng Chen |
| yansi | 1.0.1 | Apache-2.0 OR MIT | Sergio Benitez |
| yazi | 0.2.2 | Apache-2.0 OR MIT | Chad Brokaw |
| yoke | 0.8.4 | Unicode-3.0 | Manish Goregaokar |
| yoke-derive | 0.8.4 | Unicode-3.0 | Manish Goregaokar |
| zeno | 0.3.3 | Apache-2.0 OR MIT | Chad Brokaw |
| zerocopy | 0.8.63 | Apache-2.0 OR BSD-2-Clause OR MIT | – |
| zerocopy-derive | 0.8.63 | Apache-2.0 OR BSD-2-Clause OR MIT | – |
| zerofrom | 0.1.8 | Unicode-3.0 | The ICU4X Project Developers |
| zerofrom-derive | 0.1.8 | Unicode-3.0 | Manish Goregaokar |
| zeroize | 1.9.2 | Apache-2.0 OR MIT | The RustCrypto Project Developers |
| zeroize_derive | 1.5.0 | Apache-2.0 OR MIT | The RustCrypto Project Developers |
| zerotrie | 0.2.5 | Unicode-3.0 | The ICU4X Project Developers |
| zerovec | 0.11.9 | Unicode-3.0 | The ICU4X Project Developers |
| zerovec-derive | 0.11.7 | Unicode-3.0 | Manish Goregaokar |
| zlib-rs | 0.6.8 | Zlib | – |
| zmij | 1.0.23 | MIT | David Tolnay |
| zune-core | 0.4.12 | Apache-2.0 OR MIT OR Zlib | – |
| zune-core | 0.5.3 | Apache-2.0 OR MIT OR Zlib | – |
| zune-inflate | 0.2.54 | Apache-2.0 OR MIT OR Zlib | – |
| zune-jpeg | 0.4.21 | Apache-2.0 OR MIT OR Zlib | caleb |
| zune-jpeg | 0.5.15 | Apache-2.0 OR MIT OR Zlib | caleb |

</details>

### License texts

The common license texts (MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, MPL-2.0, Unicode-3.0,
BSL-1.0, CC0-1.0) are at <https://spdx.org/licenses/>. MIT-licensed crates require their
copyright notice to be kept: it is in each crate's `LICENSE-MIT` / `LICENSE` file, under the
names in the Authors column. MPL-2.0 crates (e.g. UniFFI's runtime) are unchanged; their source
is on crates.io.
