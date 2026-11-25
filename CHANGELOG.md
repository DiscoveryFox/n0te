# Changelog

What changed in n0te. One version number covers the whole suite: the Android tablet app,
n0te Laptop, n0te Tablet and n0te-sync. Each `## [version]` section below becomes the notes of
that GitHub release, so it lists only what changed in that version (what n0te can do overall:
[README](README.md)). The Android app shows its changes in German too (Einstellungen →
Neuigkeiten).

## [0.34.0] - 2026-11-25

### Added

- Export (tablet and laptop): pick single pages, even a single side page ("03 R1").
- Graphs (Android): stretch an interactive graph's x or y axis right on the page: drag the axis
  with the pen (or a finger where fingers write); on squared paper it snaps to the squares.
- n0te Tablet: a ruler. Move it with one finger, turn it with two (it snaps at 45°); the pen
  follows its edge.
- n0te Tablet: insert space. Put the pen down and drag down or sideways: everything below or to
  the right moves along and the page grows; dragging back takes the space out again.
- n0te Tablet: writing magnifier ("Schreiblupe"). Write large in the strip at the bottom and the
  frame on the page moves along; also "Mitwandern beim Schreiben" (a setting).
- n0te Tablet: scribble quickly over strokes to delete them.
- n0te Tablet: the "+" at the page edges adds a page below or a side page (tap or drag); boards
  open on their own, endless, with a small overview map.
- n0te Laptop and n0te Tablet: pair and accept invites with the camera. "Mit der Kamera scannen"
  reads the other device's QR code with the webcam (Windows: front or back camera; Linux: V4L2).

### Changed

- Very large pages (2 × 2 m) and big boards stay smooth on Android, n0te Laptop and n0te
  Tablet: only what is in view is drawn. Android draws dense ink from tiles and lets go of pages
  not seen for a while. On Windows and Linux only what changed is rebuilt after a stroke, and it
  is saved in the background.
- Your own page sizes can be up to 5 m a side (was 1.4 m).
- Graphs: editing a row to `a = 3b` makes the curve a(b) = 3b, as typing it does. A row that
  other objects use keeps its sliders.

## [0.33.0] - 2026-11-25

### Added

- Graphs: domains as at school: `f(x) = x², 0 ≤ x ≤ 2`, `… für 0 < x < 2`, `x² (x > 0)`; the
  ends show as a dot (included) or a ring (not included), can be dragged on the graph and set
  under "Definitionsbereich von / bis".
- Graphs: stretch one axis on its own: drag the x or y axis to change only that axis (for
  example y in hundreds and x in ones); on squared paper each axis snaps to its own units.
- Graphs: implicit curves (`x² + y² = 4`, `x·y = 1`), parametric curves (`(cos t, sin t)`,
  `Kurve(…)`), polar curves (`r = 1 + cos θ`) and functions of other letters (`a(b) = 3b`).
- Graphs: an equation of two letters that name nothing yet (`a = 3b`) is drawn as a curve; a
  switch in its row turns it into sliders instead.
- Graphs: handwriting next to a graph goes into it: select it and tap "In den Graphen".
- Android: open PDFs with n0te from other apps ("Open with"): as a new notebook named after the
  file, or inserted into a notebook after the page it was last open at.
- PDF export (tablet and laptop): side pages right after their page or all at the end.

### Changed

- Graphs: division always shows as a fraction (rows, the preview while typing, labels on the
  graph), and exact values like ⅓ as fractions instead of 0.33.
- Holding a straight line selects only that line, even right next to writing; its end grips
  change the length and keep the angle, a clear pull sideways turns it (45° and 15° steps).
- Big selections are much faster: a lasso over a full page, moving, putting down and pasting
  thousands of strokes; dragging a selection stays smooth.

### Fixed

- "An den Laptop" cut off the end of big selections (several pages, wide boards).
- Typing in the graph editor made the view jump to the graph.

## [0.32.1] - 2026-11-24

### Added

- Windows (n0te Laptop and n0te Tablet): automatic updates from this repo's releases. A while
  after start and every few hours n0te looks for a new version, downloads the installer for
  your PC (x64 or ARM64), checks its SHA-256 and installs it at the next start; "Restart" in
  the status bar installs it right away. The installer keeps the programs you chose and starts
  n0te again. Settings → Updates switches it off. A portable copy only says a new version is out.
- Android: updates come from this repo by default (Settings → Updates), no setup needed.

### Changed

- Faster builds: CI checks build in a quick profile, release programs use thin LTO.

## [0.32.0] - 2026-11-24

### Added

- n0te for Windows, x64 and ARM64: **n0te Laptop** (the laptop app, keyboard and mouse) and
  **n0te Tablet** (new, for pen and touch devices), with an installer (no administrator rights,
  pick either or both) and a portable zip.
- n0te Tablet: the Windows pen (pressure, tilt, eraser end, side button, hover; press-and-hold
  right click off on the page; resting palm ignored), the tablet's pen types and palettes,
  eraser (strokes or area), lasso (move, scale, recolour, copy, cut, paste), text tool, laser,
  undo and redo across pages, writing with a finger (setting).
- n0te Tablet: shapes and arrowheads snap when the pen rests (the Android app's recognizers,
  ported to the shared core).
- n0te Tablet: handwriting search with Windows' own recognizer; the touch keyboard opens when
  typing starts.
- Windows: the n0te-sync helper starts at sign-in and keeps notebooks in sync with the apps
  closed; pairing by showing a code / QR code or by pasting another device's code; `n0te://`
  invite links open the join dialog.
- n0te Laptop and n0te Tablet in English as well as German (follows the Windows language;
  Settings → Sprache / Language).
- Android: insert space sideways: put the pen down and drag right, everything to the right
  moves over and the page grows wider if needed; dragging left takes the space out again. The
  first direction the pen moves decides (a small cross marks the spot until it is clear).
- Android: dragging down to insert space works with nothing below too: the page grows, so you
  can keep writing.

### Changed

- One version number for the Android app, n0te Laptop, n0te Tablet and n0te-sync; releases
  are published here on GitHub.

### Known issues

- The Windows programs are new and not code-signed yet (SmartScreen may warn); not yet in n0te
  Tablet: ruler, insert space, the writing magnifier, scribble to delete.

## History before the public release

The Android app's versions up to 0.31.2, condensed (the app was in private use before 0.32.0;
the full German notes are in the app).

### 0.31 · 24 Nov 2026 · Graphs like GeoGebra, keyboard, folders

- "Big" graph view laid out like GeoGebra (algebra panel left, tools on top), the formula is
  typeset while you type, LaTeX input (`\frac`, `\sqrt`, `\pi`), a hardware keyboard types into
  the input line.
- Unknown letters become sliders automatically (`y = m x + c`); the big view shows the paper's
  squares and snaps the graph to them.
- Nicer outline (notebook on top, bigger page pictures, groups with counts, the page in view
  framed red); "⧉ open beside" picks a page by its picture or another notebook.
- Folders as tiles when creating a notebook.

### 0.30 · 24 Nov 2026 · Two notebooks side by side

- Split screen: a second notebook beside the first, or the same one twice; drag a page from
  the outline to the edge to open it beside. The divider snaps at a third, half, two thirds.
- One toolbar for the area touched last; ★ marks the main area for the laptop, laser and
  summoning. The same notebook twice shares every edit and one undo.

### 0.28 – 0.29 · 24 Nov 2026 · An organised library

- Library sidebar with nested folders, coloured tags, pinned notebooks, search, sorting and
  multi-select (move, tag, pin, delete).
- Start page: recently opened, pinned, folders, notebooks in no folder; tiles or list; drag
  notebooks onto folders and tags. Recently edited pages, also from the laptop.
- Outline with page pictures and bookmarks.

### 0.27 · 23 Nov 2026 · Updates from GitHub

- n0te looks for new versions in a GitHub repository's releases, downloads them in the
  background, checks them and asks before installing.

### 0.22 – 0.26 · 23 Nov 2026 · Graphs

- Coordinate systems with function graphs on the page: typed functions (`x^2 - 1`, `2sin x`,
  `|x|`, `√x`), snapping to the paper's squares, a hand-drawn look or ruler-straight.
- Constructions like GeoGebra: points, lines, segments, vectors, circles, polygons, angles,
  conics, sliders, functions with If(…); the GeoGebra tools, an algebra view, commands in
  German and English (Schneide/Intersect, Ableitung/Derivative …), object properties, dragging
  points on the page.
- Roots, extrema, intersections and shaded areas; step-by-step hints while constructing; a math
  keyboard like GeoGebra's. Graphs stay ink: the same on the laptop and in the PDF.
- PDF export keeps every page size (custom formats, lengthened pages, boards).

### 0.21 · 21 Nov 2026 · Find your handwriting

- Search finds handwriting (recognised on the tablet with Google ML Kit, offline after a
  one-time ~20 MB download), typed text, headings and PDF text, tolerant of small reading
  errors, case and umlauts. A hit jumps there and lights up.
- Handwritten headings (lasso → heading) show in the outline.

### 0.17 – 0.20 · 20–21 Nov 2026 · Shapes, arrows, space

- Hold the pen at the end of a stroke: it becomes a clean straight line (snapping at 15° and
  45°), rectangle, square, triangle, polygon, circle or ellipse, even with shaky ends or round
  corners (99–100 % of 200 hand-drawn test shapes each). Keep holding and drag to resize.
- Arrows: a line plus a V at its end becomes a clean arrowhead (hold at the tip).
- Insert space: pen down, drag down, everything below moves and the page grows; PDF pages split
  at the line. Handles to stretch and rotate shapes.

### 0.14 – 0.16 · 20 Nov 2026 · Formats and the writing pad

- Page formats (A3–A6, Letter, square, 16:9 slide, custom size), portrait or landscape, for one
  page or all.
- Writing pad: write large at the bottom of the screen, it lands small in a frame on the page
  and moves along the lines.

### 0.10 – 0.13 · 19–20 Nov 2026 · Images and guests

- Images: insert, move, scale, drag and drop or share from other apps; a page region as image.
- Guests: see them live and where they look, change their rights at any time, remove single
  guests, approval and PIN for new guests, expiry dates; guests can follow the owner, the owner
  can bring guests to their view.

### 0.7 – 0.9 · 19 Nov 2026 · Text, themes, pens

- Text pages and text boxes on the tablet (bold, italic, headings) with LaTeX formulas.
- Pairing with the laptop by QR code; six themes with their own shapes and fonts (Nothing,
  Light, OLED, Ocean, Forest, Sepia); fountain pen options; laser pointer with a trail;
  notebooks reopen where you left off.

### 0.2 – 0.6 · 18–19 Nov 2026 · The beginning

- Library with folders; notebooks with a main column, groups, side pages and boards; paper
  patterns; low-latency ink with palm rejection; pens, eraser, lasso, ruler, undo; PDF import,
  annotation and export.
- Sync with the laptop over iroh (peer to peer, end-to-end encrypted), live sessions, guest
  invites and version history.
