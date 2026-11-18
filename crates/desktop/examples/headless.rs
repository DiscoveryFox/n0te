//! Headless screenshots and measurements of the n0te desktop spike. Never opens a window.
//!
//!   cargo run --profile fast -p n0te-desktop --example headless -- shots   # PNGs
//!   cargo run --profile fast -p n0te-desktop --example headless -- bench   # numbers
//!
//! `N0TE_BACKEND=tiny-skia|wgpu` picks the renderer (default: both for bench, wgpu for shots,
//! falling back to tiny-skia when no GPU adapter is available).

#[path = "../tests/harness/mod.rs"]
mod harness;

use std::path::PathBuf;
use std::time::Instant;

use harness::{FrameTimes, Harness, rss_mib, stats};
use iced::keyboard::{Modifiers, key::Named};
use iced::widget::text_editor::{Cursor, Position};
use iced::{Point, Size};
use n0te_core::ink;
use n0te_desktop::app::{Message, Mode};
use n0te_desktop::blocks::Doc;
use n0te_desktop::pages::{self, Camera, InkPath};
use n0te_desktop::{App, demo};

const VIEW: Size = Size::new(1600.0, 900.0);
const SCALE: f32 = 1.2; // the laptop: 1920x1080 at 1.2

fn shots_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spikes/desktop-shots")
}

fn harness(backend: &str) -> Harness {
    Harness::new(App::new(), backend, VIEW, SCALE)
        .or_else(|| {
            eprintln!("backend {backend} unavailable, using tiny-skia");
            Harness::new(App::new(), "tiny-skia", VIEW, SCALE)
        })
        .expect("a headless renderer")
}

fn main() {
    let what = std::env::args().nth(1).unwrap_or_else(|| "all".into());
    let backend = std::env::var("N0TE_BACKEND").ok();
    print_adapter();
    if what == "shots" || what == "all" {
        shots(backend.as_deref().unwrap_or("wgpu"));
    }
    if what == "gpu" {
        gpu(backend.as_deref().unwrap_or("wgpu"));
    }
    if what == "bench" || what == "all" {
        match backend.as_deref() {
            Some(b) => bench(b),
            None => {
                bench("wgpu");
                bench("tiny-skia");
            }
        }
    }
}

fn print_adapter() {
    let instance = iced::wgpu::Instance::new(&iced::wgpu::InstanceDescriptor { backends: iced::wgpu::Backends::PRIMARY, ..Default::default() });
    let adapters = instance.enumerate_adapters(iced::wgpu::Backends::PRIMARY);
    for a in adapters {
        let i = a.get_info();
        println!("adapter: {} ({:?}, {:?})", i.name, i.backend, i.device_type);
    }
}

fn mid() -> Point {
    // Centre of the page view (right of the sidebar, below the top bar).
    Point::new(273.0 + (VIEW.width - 273.0) / 2.0, 53.0 + (VIEW.height - 80.0) / 2.0)
}

fn shots(backend: &str) {
    let dir = shots_dir();
    let mut h = harness(backend);
    println!("shots via {} into {}", h.backend(), dir.display());

    h.screenshot(dir.join("01-pages.png"));

    // Dense page with its diagram, zoomed in.
    let dense = h.app.dense_sheet.clone();
    h.message(Message::Jump(dense));
    h.message(Message::Zoom(2.5));
    let c = h.app.camera;
    h.message(Message::Camera(Camera { y: c.y + 120.0, x: c.x - 60.0, ..c }));
    h.screenshot(dir.join("02-pages-zoom-250.png"));

    // Zoomed out: many pages, side pages left and right.
    let p5 = h.app.pages.pages[h.app.pages.mains[5]].sheet_id.clone();
    h.message(Message::Zoom(0.28));
    h.message(Message::Jump(p5));
    h.screenshot(dir.join("03-pages-zoom-28.png"));

    // Flowing text page with a caret in the first paragraph.
    h.message(Message::SetMode(Mode::Text));
    let id = h.app.doc.blocks[1].id;
    let t = h.app.doc.focus_block(id, Cursor { position: Position { line: 0, column: 6 }, selection: None });
    h.task(t.map(Message::Blocks));
    h.screenshot(dir.join("04-text.png"));

    // Editing: new heading via "## ", a bullet, German text and a selection.
    let last = h.app.doc.blocks.len() - 1;
    let id = h.app.doc.blocks[last].id;
    let t = h.app.doc.focus_block(id, Cursor { position: Position { line: 0, column: 0 }, selection: None });
    h.task(t.map(Message::Blocks));
    h.type_text("## Nächste Woche");
    h.press(Named::Enter);
    h.type_text("- Übungsblatt 9 abgeben (Fälligkeit: Montag, 10 Uhr)");
    h.press(Named::Enter);
    h.type_text("Satz von Taylor wiederholen (Restglied!)");
    h.press(Named::Enter);
    h.press(Named::Enter);
    h.type_text("$$");
    h.type_text("f(x) = \\sum_{k=0}^{n} \\frac{f^{(k)}(a)}{k!}(x-a)^k + R_n(x)");
    h.set_modifiers(Modifiers::SHIFT);
    for _ in 0..8 {
        h.press(Named::ArrowLeft);
    }
    h.set_modifiers(Modifiers::empty());
    // Scroll the page so the new blocks are visible.
    h.scroll(Point::new(900.0, 500.0), 0.0, -900.0);
    h.screenshot(dir.join("05-text-editing.png"));

    // Outline with a collapsed group, pages mode at 100 %.
    h.message(Message::SetMode(Mode::Pages));
    let group = h.app.rows.iter().find_map(|r| match r {
        n0te_core::notebook::OutlineRow::Group { node, title, .. } if title == "Klausurvorbereitung" => Some(node.clone()),
        _ => None,
    });
    if let Some(g) = group {
        h.message(Message::ToggleGroup(g));
    }
    let p2 = h.app.pages.pages[h.app.pages.mains[2]].sheet_id.clone();
    h.message(Message::Zoom(1.0));
    h.message(Message::Jump(p2));
    h.screenshot(dir.join("06-pages-100-side.png"));
    println!("done");
}

fn ms(t: Instant) -> f32 {
    t.elapsed().as_secs_f32() * 1000.0
}

fn report(name: &str, frames: &[FrameTimes]) {
    let mut cpu: Vec<f32> = frames.iter().map(FrameTimes::cpu).collect();
    let mut render: Vec<f32> = frames.iter().map(|f| f.render).collect();
    let mut total: Vec<f32> = frames.iter().map(FrameTimes::total).collect();
    let mut draw: Vec<f32> = frames.iter().map(|f| f.draw).collect();
    let mut view: Vec<f32> = frames.iter().map(|f| f.view + f.update).collect();
    let (c50, c95, cmax) = stats(&mut cpu);
    let (r50, r95, rmax) = stats(&mut render);
    let (t50, t95, tmax) = stats(&mut total);
    let (d50, d95, dmax) = stats(&mut draw);
    let (v50, v95, _) = stats(&mut view);
    println!(
        "  {name:<28} n={:<4} cpu p50 {c50:6.2} p95 {c95:6.2} max {cmax:7.2} | update+view p50 {v50:5.2} p95 {v95:5.2} | draw p50 {d50:5.2} p95 {d95:6.2} max {dmax:7.2} | render p50 {r50:6.2} p95 {r95:6.2} max {rmax:7.2} | total p50 {t50:6.2} p95 {t95:6.2} max {tmax:7.2} ms",
        frames.len()
    );
}

fn bench(backend: &str) {
    let (rss0, _) = rss_mib();
    let t = Instant::now();
    let app = App::new();
    let startup = ms(t);
    let (rss1, _) = rss_mib();
    let Some(mut h) = Harness::new(app, backend, VIEW, SCALE) else {
        println!("== {backend}: unavailable");
        return;
    };
    let wgpu = h.backend() == "wgpu";
    println!("\n== backend {} · viewport {}x{} @ {SCALE} ({}x{} px)", h.backend(), VIEW.width, VIEW.height, VIEW.width * SCALE, VIEW.height * SCALE);
    let p = &h.app.pages;
    let samples: usize = p.pages.iter().map(|p| p.samples).sum();
    println!(
        "  notebook: {} pages ({} main), {} strokes, {} samples · demo build (Loro sheets) {:.0} ms, decode into pages {:.0} ms, App::new {:.0} ms",
        p.pages.len(),
        p.mains.len(),
        p.stroke_count(),
        samples,
        h.app.build_ms,
        h.app.decode_ms,
        startup
    );
    println!("  RSS: process start {rss0:.0} MiB, after App::new {rss1:.0} MiB, after renderer init {:.0} MiB", rss_mib().0);

    // --- one dense page -------------------------------------------------------------------
    let dense = h.app.pages.page_of_sheet(&h.app.dense_sheet).expect("dense page");
    let page = &h.app.pages.pages[dense];
    let n = 12;
    let (mut outline_ms, mut path_ms, mut canvas_ms, mut bg_ms, mut mesh_ms) = (vec![], vec![], vec![], vec![], vec![]);
    let (mut verts, mut mesh_verts, mut mesh_tris) = (0usize, 0usize, 0usize);
    for _ in 0..n {
        let t = Instant::now();
        let polys: Vec<Vec<f32>> = page.strokes.iter().map(|s| ink::outline(&s.points, &s.brush)).collect();
        outline_ms.push(ms(t));
        verts = polys.iter().map(|p| p.len() / 2).sum();
        let t = Instant::now();
        let paths: Vec<_> = polys.iter().filter_map(|p| pages::polygon(p)).collect();
        path_ms.push(ms(t));
        drop(paths);
        page.evict();
        canvas_ms.push(page.geometry(&h.renderer, 1.0, true).1.unwrap_or(f32::NAN));
        page.evict();
        bg_ms.push(page.geometry(&h.renderer, 1.0, false).1.unwrap_or(f32::NAN));
        let t = Instant::now();
        let mesh = pages::ink_mesh(&page.spec, &page.strokes, 1);
        mesh_ms.push(ms(t));
        if let iced::advanced::graphics::Mesh::Solid { buffers, .. } = &mesh {
            mesh_verts = buffers.vertices.len();
            mesh_tris = buffers.indices.len() / 3;
        }
    }
    page.evict();
    let (o50, _, _) = stats(&mut outline_ms);
    let (pa50, _, _) = stats(&mut path_ms);
    let (c50, c95, cmax) = stats(&mut canvas_ms);
    let (bg50, _, _) = stats(&mut bg_ms);
    let (m50, m95, _) = stats(&mut mesh_ms);
    println!("  dense page ({}): {} strokes, {} samples", page.spec.background.as_str(), page.strokes.len(), page.samples);
    println!(
        "    canvas path: ink::outline {o50:.2} ms ({verts} vertices) + Path {pa50:.2} ms; whole page via canvas::Frame ({}) p50 {c50:.2} p95 {c95:.2} max {cmax:.2} ms",
        if wgpu { "lyon fill tessellation" } else { "tiny-skia paths, rasterised per frame" }
    );
    println!("    mesh path:   background-only canvas {bg50:.2} ms + ink::outline_mesh → one mesh {m50:.2} ms (p95 {m95:.2}) = {mesh_verts} vertices, {mesh_tris} triangles");

    // --- baseline: render cost of an (almost) empty scene of the same size ---------------
    let mut base = Vec::new();
    h.message(Message::SetMode(Mode::Text));
    let saved = std::mem::replace(&mut h.app.doc, Doc::new(&[("", "")]));
    for _ in 0..20 {
        base.push(h.frame().render);
    }
    h.app.doc = saved;
    h.message(Message::SetMode(Mode::Pages));
    let (base50, _, _) = stats(&mut base);
    println!("  baseline render of a near-empty frame (clear + offscreen{}): p50 {base50:.2} ms", if wgpu { " + GPU read-back" } else { "" });

    let paths: &[InkPath] = if wgpu { &[InkPath::Mesh, InkPath::Canvas] } else { &[InkPath::Canvas] };
    for &path in paths {
        h.app.pages.ink_path.set(path);
        for p in &h.app.pages.pages {
            p.evict();
        }
        let s = &h.app.pages.stats;
        s.builds.set(0);
        s.max_build_ms.set(0.0);
        let first = h.app.pages.pages[0].sheet_id.clone();
        h.message(Message::Zoom(1.0));
        h.message(Message::Jump(first));
        println!("  -- ink path {path:?}");
        pan_and_zoom(&mut h);
    }

    // --- text editing -----------------------------------------------------------------------
    let typing = |h: &mut Harness, label: &str| {
        h.message(Message::SetMode(Mode::Text));
        let last = h.app.doc.blocks.len() - 1;
        let id = h.app.doc.blocks[last].id;
        let t = h.app.doc.focus_block(id, Cursor { position: Position { line: 0, column: 0 }, selection: None });
        h.task(t.map(Message::Blocks));
        h.frame();
        let mut frames = Vec::new();
        for c in "Die Ableitung misst die lokale Änderungsrate; für Mäßigkeit gilt ö ü ß. ".chars().cycle().take(150) {
            let s = c.to_string();
            let ev = h.key_event(iced::keyboard::Key::Character(s.as_str().into()), Some(&s));
            frames.push(h.step(&ev));
        }
        report(label, &frames);
    };
    println!("  -- text page");
    let label = format!("typing, {} blocks", h.app.doc.blocks.len());
    typing(&mut h, &label);
    let mut items = Vec::new();
    let page = demo::text_page();
    for _ in 0..20 {
        items.extend(page.iter().cloned());
    }
    h.app.doc = Doc::new(&items);
    let label = format!("typing, {} blocks", h.app.doc.blocks.len());
    typing(&mut h, &label);
    println!("  RSS end: {:.0} MiB (peak {:.0})", rss_mib().0, rss_mib().1);
}

/// GPU-side cost: static frames (nothing changes, cached meshes are not re-uploaded) vs 1 px
/// pans (every visible cached mesh is re-uploaded because its transformation changed).
fn gpu(backend: &str) {
    let mut h = harness(backend);
    println!("\n== gpu cost via {} (render time minus an empty frame)", h.backend());
    h.message(Message::SetMode(Mode::Text));
    let saved = std::mem::replace(&mut h.app.doc, Doc::new(&[("", "")]));
    let mut base: Vec<f32> = (0..30).map(|_| h.frame().render).collect();
    h.app.doc = saved;
    h.message(Message::SetMode(Mode::Pages));
    let (b, _, _) = stats(&mut base);
    println!("  empty frame render p50 {b:.2} ms");
    for zoom in [1.0, 0.3] {
        for path in [InkPath::Canvas, InkPath::Mesh] {
            h.app.pages.ink_path.set(path);
            let p = h.app.pages.pages[h.app.pages.mains[8]].sheet_id.clone();
            h.message(Message::Zoom(zoom));
            h.message(Message::Jump(p));
            for _ in 0..3 {
                h.frame();
            }
            let mut still: Vec<f32> = (0..30).map(|_| h.frame().render).collect();
            let mut moving: Vec<f32> = (0..30).map(|i| h.scroll(mid(), 0.0, if i % 2 == 0 { 1.0 } else { -1.0 }).render).collect();
            let (s, _, _) = stats(&mut still);
            let (m, _, _) = stats(&mut moving);
            println!(
                "  zoom {:>3.0} % {path:?}: {} pages / {} strokes visible · still +{:.2} ms · panning +{:.2} ms",
                zoom * 100.0,
                h.app.pages.stats.visible_pages.get(),
                h.app.pages.stats.visible_strokes.get(),
                s - b,
                m - b
            );
        }
    }
}

fn pan_and_zoom(h: &mut Harness) {
    let at = mid();
    let cam0 = h.app.camera;
    h.frame();
    let builds0 = h.app.pages.stats.builds.get();
    let mut cold = Vec::new();
    for _ in 0..160 {
        cold.push(h.scroll(at, 0.0, -36.0));
    }
    let builds_cold = h.app.pages.stats.builds.get() - builds0;
    let pages_seen = h.app.pages.current_number(&h.app.camera);
    let mut warm = Vec::new();
    for _ in 0..160 {
        warm.push(h.scroll(at, 0.0, 36.0));
    }
    let builds_warm = h.app.pages.stats.builds.get() - builds0 - builds_cold;
    println!("  pan at zoom {:.0} %: 160 frames down by 36 px (to page {pages_seen}), then back up", cam0.zoom * 100.0);
    report("pan down (pages enter: cold)", &cold);
    report("pan back up (cached: warm)", &warm);
    println!("    page builds during pan: down {builds_cold}, up {builds_warm}; max single build {:.2} ms", h.app.pages.stats.max_build_ms.get());
    println!("  RSS after pan: {:.0} MiB (cached pages: {})", rss_mib().0, h.app.pages.cached_pages());

    // Pan through the whole notebook fast (cache eviction, memory bound).
    let t = Instant::now();
    let mut all = Vec::new();
    for _ in 0..400 {
        all.push(h.scroll(at, 0.0, -120.0));
    }
    println!(
        "  fast pan through all pages: 400 frames in {:.0} ms, now page {}, cached {}, evictions {}, RSS {:.0} MiB (peak {:.0})",
        ms(t),
        h.app.pages.current_number(&h.app.camera),
        h.app.pages.cached_pages(),
        h.app.pages.stats.evictions.get(),
        rss_mib().0,
        rss_mib().1
    );
    report("fast pan (120 px/frame)", &all);

    // --- zoom -------------------------------------------------------------------------------
    let dense_sheet = h.app.dense_sheet.clone();
    h.message(Message::Zoom(1.0));
    h.message(Message::Jump(dense_sheet));
    h.frame();
    h.set_modifiers(Modifiers::CTRL);
    let mut zoom_in = Vec::new();
    let b0 = h.app.pages.stats.builds.get();
    while h.app.camera.zoom < 4.0 {
        zoom_in.push(h.scroll(at, 0.0, 24.0)); // ~10 % per step
    }
    let zb = h.app.pages.stats.builds.get() - b0;
    let mut zoom_out = Vec::new();
    while h.app.camera.zoom > 0.3 {
        zoom_out.push(h.scroll(at, 0.0, -24.0));
    }
    h.set_modifiers(Modifiers::empty());
    let zb2 = h.app.pages.stats.builds.get() - b0 - zb;
    println!("  zoom (ctrl+scroll ~10 %/step): 100 % -> 400 % ({} steps, {zb} page rebuilds), 400 % -> 30 % ({} steps, {zb2} rebuilds)", zoom_in.len(), zoom_out.len());
    report("zoom in", &zoom_in);
    report("zoom out", &zoom_out);
    let mut panz = Vec::new();
    for _ in 0..60 {
        panz.push(h.scroll(at, 0.0, -36.0));
    }
    report("pan at 30 % (many pages)", &panz);
    println!("    visible pages at 30 %: {}, strokes {}", h.app.pages.stats.visible_pages.get(), h.app.pages.stats.visible_strokes.get());
}
