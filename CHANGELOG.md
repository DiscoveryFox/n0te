# Changelog

What changed in n0te. One version number covers the whole suite: the Android tablet app,
n0te Laptop, n0te Tablet and n0te-sync. Each `## [version]` section below becomes the notes of
that GitHub release, so it lists only what changed in that version (what n0te can do overall:
[README](README.md)). The Android app shows its changes in German too (Einstellungen →
Neuigkeiten).

## [0.36.0] - 2026-11-26

### Added

- Laser pointer: a new look, the same on every device: a soft glowing dot and a smooth trail
  that thins and fades towards its end (it used to be a jagged line, and a red dot jumping ten
  times a second on the other devices). Guests now see it, also when they are connected to the
  laptop and the tablet points, and guests can point too, each in their own colour. On n0te
  Laptop hold L and the mouse is a laser pointer.
- Links in PDFs work: tap (tablet, where fingers don't write) or click (laptop) a link to jump
  to the page it points to (back with the back gesture or Alt+←), or to open a web address in
  the browser.
- Importing a PDF turns its table of contents into groups, two levels deep; a chapter of a
  single page becomes that page's title. Can be switched off (Settings › PDF on the laptop,
  Einstellungen › Seiten on the tablet).
- Favourite pens: up to six pens with their type, colour, width and line in the tool bar; "+"
  keeps the pen as it is now, a tap picks one, holding (tablet) or a right click (n0te Tablet)
  removes it; keys 1–6 on n0te Tablet.
- Dashed, dotted and dash-dot lines (pen menu, and "Linie" for a selection). Older versions
  show them as solid lines.

### Fixed

- Scribbling out reacts to what is underneath: colouring in and hatching no longer delete the
  outline they touch (they stay ink), and a quick scribble over a single letter now deletes it.
  Only what the scribble really covers goes; a long line it crosses keeps its ends, pictures
  and graphs go when they are mostly covered. One undo brings it all back.
- What a guest scribbled out sometimes stayed on the owner's screen (and the other way round):
  the scribble itself stayed visible there. It now goes at once, and strokes from elsewhere that
  are never stored don't stay for good.

## [0.35.0] - 2026-11-26

### Added

- Designs: write your own with an AI. Settings › Design copies instructions for ChatGPT, Claude
  or Gemini; paste the AI's answer back (or open/drop its `.json` file). n0te checks it, shows a
  preview to apply or save, and otherwise lists exactly what the AI should fix. The same file
  imports on Android, Windows and Linux; share a design from its card. Format:
  [docs/styles.md](https://github.com/DiscoveryFox/n0te/blob/main/docs/styles.md).
- Eight new designs: Christmas (snowfall and fairy lights), Blueprint (drafting grid),
  Chalkboard, Starry night (twinkling stars), Cherry blossom (falling petals), Autumn (falling
  leaves), Terminal (scanlines) and High contrast.
- Decorations behind the library (only there, never while writing), with switches to turn them
  off or hold them still.
- Crash reports: after a crash, n0te (tablet, laptop, n0te Tablet) offers to copy or share a
  report of what went wrong, or to open a GitHub issue for it. Reports contain no notes. On
  Windows a crash used to vanish without a trace.

### Fixed

Your notes are safer:

- Strokes drawn right before leaving the Android app, and text typed right before closing the
  laptop window or an update restart, are always saved.
- Edits are no longer lost when the same notebook or page is open twice at once (e.g. in the
  library and in the editor, or while its handwriting is being read), and devices no longer
  drift apart afterwards. n0te Laptop and n0te Tablet can run at the same time.
- A damaged notebook or page file no longer blocks the library: it is repaired from a paired
  device and the damaged file is kept beside it. Importing a broken `.n0te` file no longer
  leaves half a notebook; a PDF or picture that arrived incomplete is stored again in full.
- "Undo" after deleting a page undoes only the delete; failed saves on the laptop are reported
  instead of silently lost.
- Version history no longer copies every PDF and picture into each checkpoint (much less disk).

Crashes and freezes:

- Graphs: formulas like `f(x) = f(x) + 1`, very long or deeply nested input, huge derivatives
  and extreme values from another device no longer crash or freeze the app.
- Android: crashes while writing with "Mitwandern" at a page edge, with the magnifier in a short
  split pane, in the new-notebook dialog with a folder named "+", and when a file app refuses
  to tell a PDF's name.
- Broken pen samples, a held line far off the page, absurd page sizes, deeply nested groups and
  impossible PDF tiles from another device no longer crash or hang anything.
- Laptop: a hanging sync service no longer freezes the window; a PDF page that can't be
  rendered no longer keeps the CPU busy; pairing with a code no longer blocks the window.
- A misbehaving paired device can no longer make the app reserve gigabytes of memory.

Android:

- Updates reach beta testers (from a beta to the release or the next beta).
- PDF pages no longer stay white after zooming in while they load.
- Picking several notebooks picks the right ones; invite links pasted from chats work; graph
  edits are kept on Back; changes from the laptop made while n0te was paused show at once.
- Cut no longer deletes a newer selection; boards shown alone show their pictures and graphs;
  pages with several photos no longer flicker; graph numbers, angles and decimal commas are
  read correctly and a slider drag is one undo step.
- Turning the tablet keeps open dialogs and typed text; search hits in folded groups unfold
  them; the last page can't be deleted from any menu.

n0te Laptop and n0te Tablet:

- n0te Tablet: undo and Delete no longer change a notebook that isn't on screen; a stroke right
  after a cancelled move is no longer lost; graphs can be undone; strokes no longer blink when a
  sync change arrives; the tool bar fits small pen laptops.
- Windows: the pen writes in the right place after the screen is rotated or monitors change;
  the touch keyboard no longer stalls the window; export file names are always valid; paths and
  hints are Windows ones.
- Panels of one notebook close when another takes its place; deleting a page closes its notes;
  the import panel lists the PDFs in Downloads; text pages open with the cursor at the end.
- README pictures and UI tests no longer show every button as disabled.

### Changed

- The designs come from one shared set of files, so tablet and laptop look exactly alike; the
  design picker groups them into Classic, Special and Your own. Text on the accent colour
  follows the design (Forest's green buttons now carry dark text).
- Faster: the laptop library with 300 notebooks refreshes in 16 ms instead of 1 s, 400-page
  notebooks scroll 3–8× lighter, pages refresh faster after another device saved, and the tablet
  decodes pictures at most at the size it shows them.
- Laptop: PDF export runs in the background and includes what was just typed; Ctrl+Z brings back
  a deleted page; narrow windows get shorter labels; icon buttons have tooltips; empty notebooks
  offer a first page; the settings show the version.
- Android: long dialogs scroll with their buttons always visible; the library's top bar wraps
  when upright.
- A real test suite: more than 120 new tests (fuzzing of every parser, hostile files and peers,
  user flows on every platform); `scripts/test.sh` runs all of it in one go.

## [0.34.1] - 2026-11-26

### Fixed

- Guests of more than one notebook of the same owner kept connecting and disconnecting: the
  owner saw them come and go, and what they wrote or changed in graphs hardly arrived. Each
  shared notebook now has its own connection.
- Opening the same invite link again no longer makes two connections take turns.
- As a guest, "Gast bei …" now shows whether the owner is actually reachable.
- Android: long "what's new" and update dialogs scroll, and their buttons always stay in view.

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
