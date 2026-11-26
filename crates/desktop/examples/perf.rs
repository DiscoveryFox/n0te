//! Frame times of the page view on very large pages (headless, never opens a window):
//!
//!   cargo run --profile fast -p n0te-desktop --example perf -- [laptop|tablet] [strokes]
//!
//! Builds a throwaway library with a 2000 × 2000 mm page on squared paper full of handwriting
//! (and two coordinate systems), the same size on dotted paper, a big PDF page (when pdfium is
//! there) and a board with ink spread far and wide; opens it as n0te Tablet (default) or n0te
//! Laptop and measures opening, panning zoomed out / at 100 % / at 400 %, zooming in steps,
//! writing a stroke on the dense page (tablet) and moving over the board. Prints median, 95th
//! percentile and worst frame per phase (update + view + draw + render, ms).
//! `N0TE_BACKEND=tiny-skia|wgpu` (default wgpu, falls back to tiny-skia).
//!
//!   cargo run --profile fast -p n0te-desktop --example perf -- look <dir> [words]
//!
//! saves screenshots of the same pages instead (dark and light design, several zooms), to
//! compare the look before and after a change.
//!
//!   cargo run --profile fast -p n0te-desktop --example perf -- library [notebooks]
//!
//! times the library screen instead: reading the list of a library with that many notebooks
//! (default 300, each with a few pages), once and again after one of them changed (what every
//! sync change on the library screen costs).
//!
//!   cargo run --profile fast -p n0te-desktop --example perf -- long [pages]
//!
//! times a long notebook (default 400 pages of handwriting, in groups): opening it, frames with
//! the outline as a list and as pictures, scrolling the pages and the outline, adding a page.

#[path = "../tests/harness/mod.rs"]
mod harness;

use std::time::Instant;

use harness::{FrameTimes, Harness, rss_mib, stats};
use iced::{Point, Rectangle, Size};
use n0te_core::ink::{Brush, InkPoint};
use n0te_core::notebook::{Background, SheetSpec};
use n0te_core::store::Library;
use n0te_desktop::app::{Message, Screen};
use n0te_desktop::demo::{self, Rng};
use n0te_desktop::{App, Mode};

/// 2000 mm in page points.
const BIG: f32 = 2000.0 * 72.0 / 25.4;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = if args.iter().any(|a| a == "laptop") { Mode::Laptop } else { Mode::Tablet };
    let words: usize = args.iter().find_map(|a| a.parse().ok()).unwrap_or(12_000);
    let look = args.iter().position(|a| a == "look").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);
    assert!(n0te_desktop::set_mode(mode));
    if args.iter().any(|a| a == "library") {
        library(args.iter().find_map(|a| a.parse().ok()).unwrap_or(300));
        return;
    }
    if args.iter().any(|a| a == "long") {
        long(args.iter().find_map(|a| a.parse().ok()).unwrap_or(400));
        return;
    }
    let root = std::env::temp_dir().join(format!("n0te-perf-{}", std::process::id()));
    let lib = Library::open(&root).unwrap();
    let t = Instant::now();
    let (id, dense) = build(&lib, words);
    println!("mode {mode:?}, {words} words on the big page, library built in {:.1} s", t.elapsed().as_secs_f32());

    let backend = std::env::var("N0TE_BACKEND").unwrap_or_else(|_| "wgpu".into());
    let size = Size::new(1400.0, 900.0);
    let mut h = Harness::new(App::new(&root), &backend, size, 1.0).or_else(|| Harness::new(App::new(&root), "tiny-skia", size, 1.0)).expect("renderer");
    println!("backend: {}", h.backend());
    if let Some(pdf) = h.app.pdf.clone() {
        add_pdf_page(&h, &pdf, &lib, &id);
    }

    if let Some(dir) = look {
        shots(&mut h, &id, &dense, &dir);
        return;
    }

    // The floor: frames of the library screen (rendering and reading the frame back).
    let mut floor: Vec<f32> = (0..10).map(|_| h.step(&[]).total()).collect();
    report("floor (library screen)", &mut floor);

    // Opening: the notebook, its first frames (the big page fills the view's width).
    let t = Instant::now();
    h.message(Message::Open(id.clone()));
    let opening = [h.step(&[]), h.step(&[]), h.step(&[])];
    report("open (3 frames)", &mut opening.iter().map(FrameTimes::total).collect::<Vec<_>>());
    println!("  open total {:.0} ms", t.elapsed().as_secs_f32() * 1000.0);

    let mut idle: Vec<f32> = (0..10).map(|_| h.step(&[]).total()).collect();
    report("idle frames", &mut idle);
    storage(&lib, &id, &dense);

    let mid = Point::new(size.width / 2.0, size.height / 2.0 + 60.0);
    for (label, zoom) in [("pan at the start zoom", None), ("pan at 100 %", Some(1.0)), ("pan at 400 %", Some(4.0))] {
        if let Some(z) = zoom {
            h.message(Message::Zoom(z));
            h.step(&[]);
        }
        let mut frames = Vec::new();
        for i in 0..60 {
            let dy = if i < 30 { -60.0 } else { 60.0 };
            frames.push(h.scroll(mid, if i % 10 < 5 { 25.0 } else { -25.0 }, dy));
        }
        report(label, &mut frames.iter().map(FrameTimes::total).collect::<Vec<_>>());
        split(&frames);
    }

    // Zooming in steps (ctrl + wheel) from far out to 600 %, around the middle.
    h.message(Message::Zoom(0.1));
    h.step(&[]);
    h.set_modifiers(iced::keyboard::Modifiers::CTRL);
    let mut frames = Vec::new();
    for _ in 0..40 {
        frames.push(h.scroll(mid, 0.0, 100.0));
    }
    for _ in 0..20 {
        frames.push(h.scroll(mid, 0.0, -100.0));
    }
    h.set_modifiers(iced::keyboard::Modifiers::default());
    report("zoom steps", &mut frames.iter().map(FrameTimes::total).collect::<Vec<_>>());
    split(&frames);

    if mode == Mode::Tablet {
        // Writing on the dense page at 100 %: every pen move is an event plus a frame; the pen-up
        // stores the stroke (and the page shows it).
        h.message(Message::Zoom(1.0));
        h.step(&[]);
        let start = window_point(&h, &dense, 0.3, 0.2);
        let mut moves = Vec::new();
        h.press_at(start);
        for i in 1..=150 {
            let p = Point::new(start.x + i as f32 * 2.5, start.y + (i as f32 * 0.4).sin() * 18.0);
            h.cursor = iced::mouse::Cursor::Available(p);
            moves.push(h.step(&[iced::Event::Mouse(iced::mouse::Event::CursorMoved { position: p })]));
        }
        let t = Instant::now();
        h.release();
        let up = t.elapsed().as_secs_f32() * 1000.0;
        let first = h.frame();
        // A second stroke right after (the page is open for writing now).
        let start2 = Point::new(start.x, start.y + 30.0);
        h.press_at(start2);
        for i in 1..=60 {
            h.move_to(Point::new(start2.x + i as f32 * 3.0, start2.y + (i as f32 * 0.5).sin() * 10.0));
        }
        // The pen-up itself, as the app's update runs it (its background tasks left out: the
        // real app runs them on other threads).
        let lift = n0te_desktop::tablet::pen::PenEvent { phase: n0te_desktop::tablet::pen::Phase::Up, samples: Vec::new(), eraser: false, barrel: false, real: false };
        let t = Instant::now();
        let _ = h.app.update(Message::Pen(lift));
        let ui = t.elapsed().as_secs_f32() * 1000.0;
        h.events(&[iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left))]);
        let f = h.frame();
        println!("  second pen up (update) {ui:.1} ms, next frame {:.1} ms (view {:.1}, draw {:.1}, render {:.1})", f.total(), f.view, f.draw, f.render);
        report("writing: pen moves", &mut moves.iter().map(FrameTimes::total).collect::<Vec<_>>());
        split(&moves);
        println!("  pen up (store + reload) {:.1} ms, next frame {:.1} ms", up, first.total());
        let mut after = Vec::new();
        for i in 0..20 {
            after.push(h.scroll(mid, 0.0, if i < 10 { -40.0 } else { 40.0 }));
        }
        report("pan after writing", &mut after.iter().map(FrameTimes::total).collect::<Vec<_>>());
    }

    // The board, in its frame in the column.
    if let Some(board) = board_sheet(&h) {
        jump(&mut h, &board);
        h.message(Message::Zoom(2.0));
        h.step(&[]);
        let mut frames = Vec::new();
        for i in 0..40 {
            frames.push(h.scroll(mid, if i < 20 { 30.0 } else { -30.0 }, 0.0));
        }
        report("board frame: pan at 200 %", &mut frames.iter().map(FrameTimes::total).collect::<Vec<_>>());
        split(&frames);
        // n0te Tablet: the board on its own (endless, all its ink in view at first).
        if mode == Mode::Tablet {
            let t = Instant::now();
            h.message(Message::Tablet(n0te_desktop::tablet::Msg::Board(true)));
            let first = h.step(&[]);
            println!("  board on its own: open {:.0} ms + first frame {:.0} ms (zoom {:.2})", t.elapsed().as_secs_f32() * 1000.0 - first.total(), first.total(), open(&h).camera.zoom);
            for (label, zoom) in [("board on its own: all of it", None), ("board on its own: 100 %", Some(1.0))] {
                if let Some(z) = zoom {
                    h.message(Message::Zoom(z));
                    h.step(&[]);
                }
                let mut frames = Vec::new();
                for i in 0..40 {
                    frames.push(h.scroll(mid, if i < 20 { 40.0 } else { -40.0 }, if i % 10 < 5 { 30.0 } else { -30.0 }));
                }
                report(label, &mut frames.iter().map(FrameTimes::total).collect::<Vec<_>>());
                split(&frames);
            }
        }
    }
    let (rss, peak) = rss_mib();
    println!("memory: {rss:.0} MiB now, {peak:.0} MiB peak");
}

/// The library screen with `n` notebooks: reading the list, again unchanged, again after one
/// notebook changed (a sync change while the library shows), and a frame.
fn library(n: usize) {
    let root = std::env::temp_dir().join(format!("n0te-perf-lib-{}", std::process::id()));
    let lib = Library::open(&root).unwrap();
    let t = Instant::now();
    let mut ids = Vec::new();
    for i in 0..n {
        let nb = lib.create_notebook(&format!("Notizbuch {i}")).unwrap();
        for p in 0..6 {
            nb.add_sheet(None, p, &SheetSpec::a4(Background::Grid)).unwrap();
        }
        nb.set_folder(&format!("Ordner {}", i % 7)).unwrap();
        lib.save_notebook(&nb).unwrap();
        ids.push(nb.id());
    }
    println!("{n} notebooks built in {:.1} s", t.elapsed().as_secs_f32());
    let t = Instant::now();
    let backend = std::env::var("N0TE_BACKEND").unwrap_or_else(|_| "wgpu".into());
    let size = Size::new(1400.0, 900.0);
    let mut h = Harness::new(App::new(&root), &backend, size, 1.0).or_else(|| Harness::new(App::new(&root), "tiny-skia", size, 1.0)).expect("renderer");
    println!("  app start (reads the list): {:.0} ms, backend {}", t.elapsed().as_secs_f32() * 1000.0, h.backend());
    let mut again = Vec::new();
    for _ in 0..5 {
        let t = Instant::now();
        h.message(Message::Refresh);
        again.push(t.elapsed().as_secs_f32() * 1000.0);
    }
    report("list read again, nothing changed", &mut again);
    let mut changed = Vec::new();
    for (k, id) in ids.iter().take(5).enumerate() {
        let nb = lib.load_notebook(id).unwrap();
        nb.set_title(&format!("Umbenannt {k}")).unwrap();
        lib.save_notebook(&nb).unwrap();
        let t = Instant::now();
        h.message(Message::Live(n0te_desktop::live::LiveEvent::Changed { notebook: id.clone(), index: true, sheets: Vec::new() }));
        changed.push(t.elapsed().as_secs_f32() * 1000.0);
        assert!(h.app.entries.iter().any(|e| e.title == format!("Umbenannt {k}")), "the new title shows");
    }
    report("a sync change on the library screen", &mut changed);
    let frames: Vec<FrameTimes> = (0..10).map(|_| h.step(&[])).collect();
    report("library frames", &mut frames.iter().map(FrameTimes::total).collect::<Vec<_>>());
    split(&frames);
    h.message(Message::Shelf(n0te_desktop::shelf::Msg::Pick(n0te_desktop::shelf::Pick::All)));
    let frames: Vec<FrameTimes> = (0..10).map(|_| h.step(&[])).collect();
    report("all notebooks as cards", &mut frames.iter().map(FrameTimes::total).collect::<Vec<_>>());
    split(&frames);
    drop(h);
    let _ = std::fs::remove_dir_all(&root);
}

/// A notebook of `n` pages with handwriting, in groups of 20: opening, idle frames with the
/// outline as a list and as pictures, scrolling the pages and the outline, adding a page.
fn long(n: usize) {
    let root = std::env::temp_dir().join(format!("n0te-perf-long-{}", std::process::id()));
    let lib = Library::open(&root).unwrap();
    let t = Instant::now();
    let nb = lib.create_notebook("Lang").unwrap();
    let mut rng = Rng::new(7);
    let mut group = None;
    for i in 0..n {
        if i % 20 == 0 {
            group = Some(nb.add_group(None, i / 20, &format!("Woche {}", i / 20 + 1)).unwrap());
        }
        let spec = SheetSpec::a4(Background::Lines);
        let (_, sheet_id) = nb.add_sheet(group.as_deref(), i % 20, &spec).unwrap();
        let sheet = n0te_core::sheet::Sheet::create(1).unwrap();
        for s in demo::page_strokes(&mut rng, &spec, 120, false) {
            sheet.add_stroke(s.brush, &s.points).unwrap();
        }
        lib.save_sheet(&nb.id(), &sheet_id, &sheet).unwrap();
    }
    lib.save_notebook(&nb).unwrap();
    println!("{n} pages built in {:.1} s", t.elapsed().as_secs_f32());
    let size = Size::new(1400.0, 900.0);
    let backend = std::env::var("N0TE_BACKEND").unwrap_or_else(|_| "wgpu".into());
    let mut h = Harness::new(App::new(&root), &backend, size, 1.0).or_else(|| Harness::new(App::new(&root), "tiny-skia", size, 1.0)).expect("renderer");
    println!("backend: {}", h.backend());
    let t = Instant::now();
    h.message(Message::Open(nb.id()));
    let opening = [h.step(&[]), h.step(&[]), h.step(&[])];
    println!("  open: {:.0} ms (3 frames {:.0} ms)", t.elapsed().as_secs_f32() * 1000.0, opening.iter().map(FrameTimes::total).sum::<f32>());
    let mid = Point::new(size.width / 2.0 + 140.0, size.height / 2.0);
    let side = Point::new(120.0, size.height / 2.0);
    // Where the time of a frame goes: building the widgets (App::view) alone, a whole frame with
    // the outline and without it.
    let t = Instant::now();
    for _ in 0..20 {
        drop(h.app.view());
    }
    println!("  App::view alone: {:.1} ms", t.elapsed().as_secs_f32() * 1000.0 / 20.0);
    h.message(Message::ToggleSidebar);
    let mut bare: Vec<FrameTimes> = (0..10).map(|_| h.step(&[])).collect();
    report("idle, outline hidden", &mut bare.iter().map(FrameTimes::total).collect::<Vec<_>>());
    split(&bare);
    bare.clear();
    h.message(Message::ToggleSidebar);
    for pictures in [false, true] {
        h.message(Message::Outline(n0te_desktop::sidebar::Msg::Pictures(pictures)));
        let what = if pictures { "pictures" } else { "list" };
        let mut idle: Vec<FrameTimes> = (0..10).map(|_| h.step(&[])).collect();
        report(&format!("idle, outline as {what}"), &mut idle.iter().map(FrameTimes::total).collect::<Vec<_>>());
        split(&idle);
        idle.clear();
        let mut frames: Vec<FrameTimes> = (0..40).map(|i| h.scroll(mid, 0.0, if i < 20 { -120.0 } else { 120.0 })).collect();
        report(&format!("pages scroll ({what})"), &mut frames.iter().map(FrameTimes::total).collect::<Vec<_>>());
        split(&frames);
        frames = (0..40).map(|i| h.scroll(side, 0.0, if i < 20 { -120.0 } else { 120.0 })).collect();
        report(&format!("outline scroll ({what})"), &mut frames.iter().map(FrameTimes::total).collect::<Vec<_>>());
        split(&frames);
    }
    let t = Instant::now();
    h.message(Message::Outline(n0te_desktop::sidebar::Msg::AddPage));
    let add = t.elapsed().as_secs_f32() * 1000.0;
    let f = h.step(&[]);
    println!("  add a page: {add:.0} ms, next frame {:.0} ms", f.total());
    let (rss, peak) = rss_mib();
    println!("memory: {rss:.0} MiB now, {peak:.0} MiB peak");
    drop(h);
    let _ = std::fs::remove_dir_all(&root);
}

/// Screenshots of the big pages, the board and a page with its edges, dark and light.
fn shots(h: &mut Harness, id: &str, dense: &str, dir: &std::path::Path) {
    std::fs::create_dir_all(dir).unwrap();
    for theme in ["nothing", "light"] {
        n0te_desktop::theme::set_theme(theme);
        h.theme = n0te_desktop::theme::theme();
        h.message(Message::Back);
        h.message(Message::Open(id.to_string()));
        h.step(&[]);
        h.step(&[]);
        let sheets: Vec<String> = open(h).pages.pages.iter().map(|p| p.placed.sheet_id.clone()).collect();
        for (i, sheet) in sheets.iter().enumerate() {
            jump(h, sheet);
            for z in [0.12, 0.45, 1.0, 3.3] {
                h.message(Message::Zoom(z));
                h.step(&[]);
                for _ in 0..4 {
                    h.step(&[]);
                    std::thread::sleep(std::time::Duration::from_millis(60));
                }
                h.screenshot(dir.join(format!("{theme}-page{i}-z{z}.png")));
            }
        }
        jump(h, dense);
        h.message(Message::Zoom(2.0));
        h.scroll(Point::new(300.0, 300.0), 300.0, 120.0);
        h.screenshot(dir.join(format!("{theme}-edge.png")));
    }
}

/// What reading and writing the dense sheet costs (the library on its own).
fn storage(lib: &Library, id: &str, sheet: &str) {
    let ms = |t: Instant| t.elapsed().as_secs_f32() * 1000.0;
    let t = Instant::now();
    let s = lib.load_sheet(id, sheet).unwrap();
    let load = ms(t);
    let t = Instant::now();
    let n: usize = s.strokes().iter().map(|s| s.points.len()).sum();
    let decode = ms(t);
    let t = Instant::now();
    let _ = s.strokes().len();
    let again = ms(t);
    let t = Instant::now();
    let other = s.text_boxes().len() + s.images().len() + s.plots().len() + s.page_text().len();
    let rest = ms(t);
    let mark = s.version_mark();
    s.add_stroke(pen(demo::INK_BLACK, 1.6), &demo::word(&mut Rng::new(1), 100.0, 100.0, 40.0, 6.0, 120)).unwrap();
    let t = Instant::now();
    let ch = s.changes_since(&mark).map(|c| c.strokes.len());
    let since = ms(t);
    println!("  texts/images/plots/body ({other}) {rest:.1} ms; changes after one stroke ({ch:?}) {since:.2} ms");
    let t = Instant::now();
    let bytes = s.snapshot().unwrap().len();
    let snap = ms(t);
    let t = Instant::now();
    lib.save_sheet(id, sheet, &s).unwrap();
    let save = ms(t);
    let t = Instant::now();
    lib.refresh_sheet(id, sheet, &s).unwrap();
    let refresh = ms(t);
    println!("  dense sheet: {n} points, {:.1} MB; load {load:.0} ms, strokes() {decode:.0} ms (again {again:.0} ms), snapshot {snap:.0} ms, save {save:.0} ms, refresh {refresh:.0} ms", bytes as f32 / 1e6);
}

fn report(label: &str, v: &mut [f32]) {
    let (p50, p95, max) = stats(v);
    println!("{label:<28} p50 {p50:>7.1}  p95 {p95:>7.1}  max {max:>7.1} ms  ({} frames)", v.len());
}

/// Where the time goes (medians): update, view, draw, render.
fn split(frames: &[FrameTimes]) {
    let med = |f: fn(&FrameTimes) -> f32| stats(&mut frames.iter().map(f).collect::<Vec<_>>()).0;
    println!("  median split: update {:.1}, view {:.1}, draw {:.1}, render {:.1}", med(|t| t.update), med(|t| t.view), med(|t| t.draw), med(|t| t.render));
}

fn open(h: &Harness) -> &n0te_desktop::app::Open {
    match &h.app.screen {
        Screen::Notebook(o) => o,
        Screen::Library => panic!("no notebook open"),
    }
}

fn window_point(h: &Harness, sheet: &str, fx: f32, fy: f32) -> Point {
    let o = open(h);
    let p = o.pages.pages.iter().find(|p| p.placed.sheet_id == sheet).expect("page");
    let r = p.placed.rect;
    let cam = o.camera;
    // Bring that spot into view first (the caller zoomed already).
    let (x, y) = (r.x0 + r.width() * fx, r.y0 + r.height() * fy);
    let at = cam.to_screen(Rectangle::new(cam.origin, cam.viewport), x, y);
    if Rectangle::new(cam.origin, cam.viewport).contains(at) {
        return at;
    }
    Point::new(cam.origin.x + cam.viewport.width * 0.3, cam.origin.y + cam.viewport.height * 0.4)
}

fn board_sheet(h: &Harness) -> Option<String> {
    open(h).pages.pages.iter().find(|p| p.placed.is_board()).map(|p| p.placed.sheet_id.clone())
}

fn jump(h: &mut Harness, sheet: &str) {
    h.message(Message::Jump(sheet.to_string()));
    h.step(&[]);
}

fn pen(color: u32, width: f32) -> Brush {
    Brush { width, thinning: 0.6, color, kind: 0 }
}

/// Handwriting all over a sheet of `w` × `h`: lines `gap` apart, words `words` in all.
fn scrawl(rng: &mut Rng, w: f32, h: f32, gap: f32, words: usize) -> Vec<(Brush, Vec<InkPoint>)> {
    let mut out = Vec::with_capacity(words);
    let lines = ((h - 120.0) / gap).max(1.0) as usize;
    let per_line = words.div_ceil(lines).max(1);
    let step = (w - 120.0) / per_line as f32;
    'all: for l in 0..lines {
        let y = 80.0 + l as f32 * gap;
        for k in 0..per_line {
            if out.len() >= words {
                break 'all;
            }
            let x = 60.0 + k as f32 * step;
            let color = if rng.chance(0.08) { demo::INK_BLUE } else { demo::INK_BLACK };
            out.push((pen(color, 1.6), demo::word(rng, x, y, (step * 0.75).max(12.0), 6.0, 120)));
        }
    }
    out
}

/// The test notebook: (id, the dense page's sheet id).
fn build(lib: &Library, words: usize) -> (String, String) {
    let nb = lib.create_notebook("Große Seiten").unwrap();
    let mut rng = Rng::new(0xB16);
    let grid = SheetSpec { width: BIG, height: BIG, background: Background::Grid, spacing: 20.0, pdf: None, gaps: Vec::new() };
    let (_, dense) = nb.add_sheet(None, 0, &grid).unwrap();
    let dots = SheetSpec { background: Background::Dots, ..grid.clone() };
    let (_, dotted) = nb.add_sheet(None, 1, &dots).unwrap();
    let (_, board) = nb.add_sheet(None, 2, &SheetSpec::board()).unwrap();
    lib.save_notebook(&nb).unwrap();
    let id = nb.id();

    let s = lib.load_sheet(&id, &dense).unwrap();
    s.add_strokes(&scrawl(&mut rng, BIG, BIG, 28.0, words)).unwrap();
    for (x, y) in [(400.0, 300.0), (3000.0, 2500.0)] {
        let mut p = n0te_desktop::plots::fresh(x, y, BIG, BIG, Background::Grid, 20.0);
        for e in ["f(x) = sin(x) * x", "g(x) = 0.2x^2 - 3"] {
            p = p.add(e).unwrap();
        }
        s.add_plot(&p).unwrap();
    }
    lib.save_sheet(&id, &dense, &s).unwrap();

    let s = lib.load_sheet(&id, &dotted).unwrap();
    s.add_strokes(&scrawl(&mut rng, BIG, BIG, 60.0, words / 4)).unwrap();
    lib.save_sheet(&id, &dotted, &s).unwrap();

    // The board: ink far beyond its frame, 12 000 pt either way.
    let s = lib.load_sheet(&id, &board).unwrap();
    let mut ink = Vec::new();
    for _ in 0..(words / 3) {
        let (x, y) = (rng.range(-6000.0, 6000.0), rng.range(-6000.0, 6000.0));
        ink.push((pen(demo::INK_BLACK, 1.8), demo::word(&mut rng, x, y, 40.0, 6.0, 120)));
    }
    s.add_strokes(&ink).unwrap();
    lib.save_sheet(&id, &board, &s).unwrap();
    (id, dense)
}

/// A big PDF page (1400 × 1400 mm, as far as a PDF goes) after the others.
fn add_pdf_page(h: &Harness, pdf: &std::sync::Arc<n0te_desktop::pdf::Pdf>, lib: &Library, id: &str) {
    use n0te_pdf::pdfium_render::prelude::*;
    let bytes = pdf
        .worker
        .call(10, |docs| {
            docs.engine().with_raw(|p| {
                let mut doc = p.create_new_pdf().unwrap();
                let font = doc.fonts_mut().helvetica_bold();
                let side = 3960.0;
                let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::new_custom(PdfPoints::new(side), PdfPoints::new(side))).unwrap();
                for i in 0..40 {
                    let y = side - 80.0 - i as f32 * 95.0;
                    page.objects_mut().create_text_object(PdfPoints::new(60.0), PdfPoints::new(y), &format!("Zeile {i}: Folgen, Reihen und Grenzwerte"), font, PdfPoints::new(48.0)).unwrap();
                }
                page.objects_mut().create_path_object_rect(PdfRect::new_from_values(40.0, 40.0, 400.0, side - 40.0), None, None, Some(PdfColor::new(215, 25, 33, 255))).unwrap();
                doc.save_to_bytes().unwrap()
            })
        })
        .unwrap();
    let blob = lib.put_blob(&bytes).unwrap();
    let nb = lib.load_notebook(id).unwrap();
    let spec = SheetSpec { width: 3960.0, height: 3960.0, background: Background::Blank, spacing: 20.0, pdf: Some((blob, 0)), gaps: Vec::new() };
    let (_, end) = n0te_desktop::pdf::position_after(&nb, None);
    nb.add_sheet(None, end, &spec).unwrap();
    lib.save_notebook(&nb).unwrap();
    let _ = h;
}
