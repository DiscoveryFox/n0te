# Changelog

What changed in n0te. One version number covers the whole suite: the Android tablet app,
n0te Laptop, n0te Tablet and n0te-sync. Each `## [version]` section below becomes the notes of
that GitHub release. The Android app shows its changes in German too (Einstellungen →
Neuigkeiten).

## [0.32.0] - 2026-11-24

The first public release: the Android tablet app, n0te Laptop for Windows and Linux, and a
first preview of n0te Tablet for Windows pen devices. From now on all of them share one
version number.

### Android tablet app

- Handwriting with low-latency ink and palm rejection; fountain pen, ballpoint, pencil, brush
  and marker; eraser, lasso, ruler, undo.
- Shapes, straight lines and arrows snap clean when you hold the pen at the end of a stroke.
- Notebooks with pages, nested groups, side pages left and right, infinite boards, paper
  patterns and page formats; an outline with page pictures and bookmarks.
- PDFs: import, write on them, insert pages, export with the ink as vectors.
- Text pages and text boxes with LaTeX formulas; GeoGebra-style graphs and constructions.
- Search through handwriting (recognised on the tablet), typed text and PDF text.
- Library with folders, tags, pins and a start page; two notebooks side by side.
- Sync with the laptop, live sessions, guests via invite links, version history; updates
  from GitHub releases.

### n0te Laptop (Windows and Linux)

- The same notebooks with keyboard and mouse: library, outline, page view, PDF import and
  export, text boxes, text pages and a notes column, formulas (with a local TeX installation),
  graphs with an algebra view and input line, search, two notebooks side by side, command
  palette, six themes.
- Shows the tablet's ink; drawing ink stays on the tablet.
- Linux: sync, pairing, live sessions, guests and version history through the n0te-sync
  service. Windows: first builds (x64 and ARM64); sync on Windows is still being ported.

### n0te Tablet (Windows, preview)

- A separate program for pen and touch devices, with its own icon and Start menu entry. For
  now it shows the n0te Laptop interface; pen input and the tablet tools follow in the next
  versions.

### Windows packages

- Installer without administrator rights (per user): pick n0te Laptop, n0te Tablet or both;
  Start menu entries, `.n0te` files open in n0te Laptop, `n0te://` invite links go to n0te
  Tablet, n0te-sync starts at sign-in. Uninstalling keeps your notebooks.
- Portable zip with the same programs. Builds for x64 and ARM64 (Snapdragon). Not code-signed
  yet: SmartScreen may warn about an unknown publisher.

### New on the tablet in this version

- Insert space sideways: put the pen down and drag right, everything to the right moves over
  and the page grows wider if needed; dragging left takes the space out again.
- The first direction the pen moves decides (a small cross marks the spot until it is clear).
- Dragging down works with nothing below too: the page grows, so you can keep writing.

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
