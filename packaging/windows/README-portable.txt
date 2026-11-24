n0te @VERSION@ for Windows (@ARCH@), portable version

Nothing to install: unpack this folder anywhere and start a program from it.

  n0te-laptop.exe   n0te Laptop: notebooks with keyboard and mouse
  n0te-tablet.exe   n0te Tablet: for pen and touch devices (preview, the pen
                    tools are still being built)
  n0te-sync.exe     the sync helper: keeps your notebooks in sync with the
                    tablet and with guests while it runs
  pdfium.dll        PDF engine (keep it next to the exes)

The portable version changes nothing in the registry: no Start menu entries,
no .n0te file association, no n0te:// links, and n0te-sync does not start by
itself at sign-in. The installer (n0te-@VERSION@-windows-@ARCH@-setup.exe)
does all of that and installs without administrator rights.

Your notebooks live in %APPDATA%\n0te, not in this folder. Deleting this
folder does not delete them.

Windows may warn that the publisher is unknown (the programs are not signed
yet): "More info" -> "Run anyway".

Deutsch: Einfach entpacken und n0te-laptop.exe oder n0te-tablet.exe starten.
Die Notizbücher liegen in %APPDATA%\n0te. Die portable Version trägt nichts
in Windows ein (kein Startmenü, keine Dateizuordnung, kein Autostart von
n0te-sync); das macht der Installer, ohne Administratorrechte.

More: https://github.com/DiscoveryFox/n0te
Copyright (C) 2026 DiscoveryFox. All rights reserved. See LICENSE.txt and
THIRD_PARTY_NOTICES.txt.
