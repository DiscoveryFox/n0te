//! The README's pictures, taken headless (never opens a window): `scripts/screenshots.sh`.
//!
//!   cargo run --profile fast -p n0te-desktop --example screenshots -- docs/screenshots
//!
//! Everything happens in a temporary library built here: the demo notebook (crates/desktop/src
//! /demo.rs) and a few more, with ink as the tablet would have written it (n0te Laptop shows ink
//! but does not draw it), lines of handwriting "read" as the tablet's recogniser stores them,
//! slides made with pdfium. The run gets its own HOME, config, cache and runtime folders, so the
//! real n0te-sync, settings and library are never touched; a stand-in on the run's own socket
//! keeps the app "connected" (it answers nothing), guests and the live tablet are set up in the
//! app's state like the UI tests do. Ink is drawn as GPU meshes, so this needs the wgpu renderer
//! (N0TE_BACKEND=tiny-skia works, but shows no ink).

#[path = "../tests/harness/mod.rs"]
mod harness;

use std::path::{Path, PathBuf};
use std::time::Duration;

use harness::Harness;
use iced::Size;
use n0te_core::ink::{Brush, InkPoint, KIND_MARKER, KIND_PENCIL, Rect};
use n0te_core::notebook::{Background, OutlineRow, SheetSpec};
use n0te_core::sheet::{IndexLine, Sheet};
use n0te_core::store::Library;
use n0te_desktop::app::{LiveSession, Message, Screen};
use n0te_desktop::demo::{self, INK_BLACK, INK_BLUE, INK_RED, Rng};
use n0te_desktop::live::{Device, Guest, Invite, LiveEvent};
use n0te_desktop::pages::Camera;
use n0te_desktop::{App, split};

const W: f32 = 1520.0;
const H: f32 = 950.0;

fn main() {
    let out = std::path::absolute(PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "docs/screenshots".into()))).unwrap();
    std::fs::create_dir_all(&out).unwrap();
    // A home of its own for the run: no socket, setting or cache of the real n0te is used.
    let scratch = std::env::temp_dir().join(format!("n0te-readme-shots-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    for d in ["run", "config", "cache", "home"] {
        std::fs::create_dir_all(scratch.join(d)).unwrap();
    }
    // SAFETY: still one thread; nothing has read the environment yet.
    unsafe {
        std::env::set_var("XDG_RUNTIME_DIR", scratch.join("run"));
        std::env::set_var("XDG_CONFIG_HOME", scratch.join("config"));
        std::env::set_var("XDG_CACHE_HOME", scratch.join("cache"));
        std::env::set_var("HOME", scratch.join("home"));
    }
    stand_in::start(&scratch);
    let root = scratch.join("library");
    let lib = Library::open(&root).unwrap();
    let ids = build_library(&lib);

    let backend = std::env::var("N0TE_BACKEND").unwrap_or_else(|_| "wgpu".into());
    let mut h = Harness::new(App::new(&root), &backend, Size::new(W, H), 1.0).expect("renderer (wgpu needs a GPU or a software Vulkan)");
    println!("backend: {}", h.backend());
    h.app.sync = n0te_desktop::live::Session::connect().map(std::sync::Arc::new);
    h.app.devices = vec![Device { id: "tab".into(), name: "Tablet".into(), online: true }];
    h.app.settings.split_seen = true;
    let shot = |h: &mut Harness, name: &str| {
        settle(h);
        h.screenshot(out.join(format!("{name}.png")));
        println!("{name}.png");
    };

    // 1. The library's start page.
    shot(&mut h, "library");

    // 2. The demo notebook with the outline as page pictures: a lecture page and its side page.
    h.app.settings.outline_pictures = true;
    open(&mut h, &ids.analysis);
    let v02 = sheet_titled(&lib, &ids.analysis, "V02");
    show_row(&mut h, &v02, 0.92);
    shot(&mut h, "notebook");

    // 3. The whole notebook from afar: groups, side pages left and right.
    h.app.settings.outline_pictures = false;
    h.message(Message::Zoom(0.24));
    shot(&mut h, "overview");

    // 4. Search: handwriting (as the tablet read it), headings, typed text.
    h.message(Message::Zoom(0.9));
    h.message(Message::FocusSearch);
    h.events(&[]);
    h.type_text("Grenzwert");
    let row = h.bounds_of(n0te_desktop::sidebar::hit_id(1)).expect("a handwriting hit");
    h.click(row.center());
    shot(&mut h, "search");
    h.message(Message::SearchClear);

    // 5. Ink and pens, as written on the tablet.
    open(&mut h, &ids.sketches);
    let page = first_sheet(&lib, &ids.sketches);
    show_page(&mut h, &page, 0.98);
    shot(&mut h, "ink");

    // 6. Graphs: a construction on squared paper, its panel with the algebra view.
    open(&mut h, &ids.functions);
    let page = first_sheet(&lib, &ids.functions);
    show_top(&mut h, &page, 1.25);
    shot(&mut h, "graph-page");
    let plot = lib.load_sheet(&ids.functions, &page).unwrap().plots()[0].id.clone();
    h.message(Message::EditPlot(page.clone(), plot));
    shot(&mut h, "graph");
    h.message(Message::Plot(n0te_desktop::plots::Msg::Ok));
    h.app.plot_edit = None;

    // 7. PDF slides: ink from the tablet, a text box and the notes column typed on the laptop.
    h.message(Message::Back);
    let slides = make_slides(&h, &root);
    h.message(Message::ImportPdf(slides));
    wait(&mut h, 6);
    let pdf_nb = match &h.app.screen {
        Screen::Notebook(s) => s.panes[0].id.clone(),
        Screen::Library => panic!("the PDF did not open"),
    };
    let first = first_sheet(&lib, &pdf_nb);
    annotate_slide(&lib, &pdf_nb, &first);
    h.message(Message::Live(LiveEvent::Changed { notebook: pdf_nb.clone(), index: false, sheets: vec![first.clone()] }));
    h.message(Message::NewText(first.clone(), 520.0, 300.0));
    for (i, line) in ["**Merke:** konvergent $\\Rightarrow$ beschränkt", "Umkehrung gilt nicht: $(-1)^n$"].iter().enumerate() {
        if i > 0 {
            h.press(iced::keyboard::key::Named::Enter);
        }
        h.type_text(line);
    }
    h.message(Message::DoneText);
    h.message(Message::ToggleNotes);
    for (i, line) in ["## Notizen zu Folie 1", "Beweisidee: ab einem $N$ liegen alle Glieder in $(a-1, a+1)$.", "- endlich viele davor: Maximum nehmen", "Übung 4 dazu rechnen"].iter().enumerate() {
        if i > 0 {
            h.press(iced::keyboard::key::Named::Enter);
        }
        h.type_text(line);
    }
    show_page(&mut h, &first, 0.98);
    wait(&mut h, 12);
    shot(&mut h, "pdf");

    // 8. A text page: Markdown, a code block, LaTeX formulas (rendered by the local TeX).
    h.message(Message::NewTextPage);
    for (i, line) in ["# Mitschrift · Vorlesung 8", "Heute: **Mittelwertsatz** und *Folgerungen*.", "```rust", "fn slope(f: impl Fn(f64) -> f64, a: f64, b: f64) -> f64 { (f(b) - f(a)) / (b - a) }"].iter().enumerate() {
        if i > 0 {
            h.press(iced::keyboard::key::Named::Enter);
        }
        h.type_text(line);
    }
    h.message(Message::Blocks(n0te_desktop::blocks::Msg::ExitBlock(focused_block(&h))));
    for (i, line) in ["1. Sekante durch $(a, f(a))$ und $(b, f(b))$", "Tangente parallel dazu suchen", "", "[ ] Übung 4 rechnen", "", "Es gibt ein $\\xi \\in (a,b)$ mit", "$$"].iter().enumerate() {
        if i > 0 {
            h.press(iced::keyboard::key::Named::Enter);
        }
        h.type_text(line);
    }
    h.type_text("f'(\\xi) = \\frac{f(b)-f(a)}{b-a}");
    h.message(Message::Blocks(n0te_desktop::blocks::Msg::ExitBlock(focused_block(&h))));
    h.type_text("> Merke: stetig auf [a,b], differenzierbar auf (a,b).");
    wait(&mut h, 12);
    shot(&mut h, "textpage");
    h.message(Message::CloseTextPage);

    // 9. Two notebooks side by side.
    open(&mut h, &ids.analysis);
    let v04 = sheet_titled(&lib, &ids.analysis, "V04");
    h.message(Message::Jump(v04));
    h.message(Message::ToggleSidebar);
    h.message(Message::Split(split::Msg::Beside(pdf_nb.clone(), Some(first.clone()))));
    wait(&mut h, 3);
    shot(&mut h, "split");
    h.message(Message::Split(split::Msg::Alone(None)));
    if let Screen::Notebook(s) = &h.app.screen
        && s.panes.len() > 1
    {
        let other = s.panes[1].key;
        h.message(Message::Split(split::Msg::Close(other)));
    }
    h.message(Message::ToggleSidebar);

    // 10. Live with the tablet: its view as a frame, its laser pointer, a stroke on its way.
    open(&mut h, &ids.analysis);
    let v05 = sheet_titled(&lib, &ids.analysis, "V05");
    show_page(&mut h, &v05, 0.98);
    h.app.live_session = Some(LiveSession { device: "tab".into(), name: "Tablet".into(), follow_them: false, they_follow: true });
    h.message(Message::Live(LiveEvent::Presence { device: "tab".into(), notebook: ids.analysis.clone(), sheet: Some(v05.clone()), viewport: [40.0, 300.0, 560.0, 690.0], zoom: 1.6, laser: Some([330.0, 480.0]) }));
    let samples: Vec<u8> = live_stroke().iter().flat_map(|f| f.to_le_bytes()).collect();
    h.message(Message::Live(LiveEvent::Stroke { device: "tab".into(), notebook: ids.analysis.clone(), sheet: v05.clone(), stroke: 7, brush: (2.2, 0.55, INK_RED, 0), samples, done: false }));
    shot(&mut h, "live");

    // 11. Sharing with guests: who is here, an invite with its QR code, a guest's view.
    h.app.live_session = None;
    h.app.guests = vec![
        Guest { invite: "inv-rw".into(), notebook: ids.analysis.clone(), name: "Mia".into(), device: "dev-mia".into(), write: true },
        Guest { invite: "inv-ro".into(), notebook: ids.analysis.clone(), name: "Ben".into(), device: "dev-ben".into(), write: false },
    ];
    h.app.waiting = vec![Guest { invite: "inv-ro".into(), notebook: ids.analysis.clone(), name: "Cleo".into(), device: "dev-cleo".into(), write: false }];
    h.message(Message::Live(LiveEvent::Presence { device: "tab".into(), notebook: ids.analysis.clone(), sheet: None, viewport: [0.0; 4], zoom: 1.0, laser: None }));
    h.message(Message::Live(LiveEvent::Presence { device: "dev-mia".into(), notebook: ids.analysis.clone(), sheet: Some(v05.clone()), viewport: [60.0, 120.0, 520.0, 420.0], zoom: 1.0, laser: None }));
    let link = "n0te://invite/AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8gISIjJCUmJygpKissLS4vMDEyMzQ1Njc4OTo7PD0-P0BBQkNERUZHSElKS0xNTk9QUVJTVFVWV1hZWltcXV5fYGFiY2RlZmdoaWprbG1ub3BxcnN0dXZ3eHl6e3x9fn8";
    let invite = |id: &str, rw: bool, pin: Option<&str>, online: u64, members: u64| Invite {
        id: id.into(),
        notebook: ids.analysis.clone(),
        rw,
        revoked: false,
        online,
        link: link.into(),
        expires: 0,
        approve: pin.is_some(),
        single: false,
        pin: pin.map(Into::into),
        members,
        banned: vec![],
    };
    h.app.share = Some(vec![invite("inv-rw", true, None, 1, 1), invite("inv-ro", false, Some("4821"), 1, 2)]);
    h.app.invite_open = Some("inv-ro".into());
    h.app.share_qr = iced::widget::qr_code::Data::new(link.as_bytes()).ok();
    shot(&mut h, "share");

    // 12. Pairing: the devices in the settings, a pair code as QR for the tablet to scan.
    h.app.share = None;
    h.app.guests.clear();
    h.app.waiting.clear();
    h.message(Message::Back);
    let code = "n0te-pair:AQIDBAUGBwgJCgsMDQ4PEBESExQVFhcYGRobHB0eHyAhIiMkJSYnKCkqKywtLi8wMTIzNDU2Nzg5Ojs8PT4_QEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl9g";
    h.app.devices = vec![Device { id: "tab".into(), name: "Tablet".into(), online: true }, Device { id: "phone".into(), name: "Handy".into(), online: false }];
    h.app.pair_code = Some(code.into());
    h.app.pair_qr = iced::widget::qr_code::Data::new(code.as_bytes()).ok();
    h.message(Message::ShowSettings(true));
    settle(&mut h);
    h.scroll(iced::Point::new(W / 2.0, H / 2.0), 0.0, -4000.0);
    shot(&mut h, "pairing");

    // 13. Version history of a notebook (what the sync service lists), restore as a copy.
    h.message(Message::ShowSettings(false));
    open(&mut h, &ids.analysis);
    show_page(&mut h, &v05, 0.98);
    let base = 1_795_478_400u64; // 2026-11-24 00:00 UTC
    let mut list: Vec<(u64, String)> = Vec::new();
    for (day, hour, minute, kb) in [(-28, 9, 0, 296), (-21, 9, 0, 318), (-14, 10, 0, 341), (-7, 10, 0, 365), (-3, 11, 0, 377), (-2, 12, 0, 384), (-1, 16, 0, 391), (0, 9, 0, 395), (0, 10, 0, 398), (0, 11, 0, 402), (0, 12, 0, 405), (0, 13, 0, 406), (0, 14, 0, 409), (0, 15, 5, 411), (0, 15, 20, 412), (0, 15, 35, 412), (0, 15, 50, 414)] {
        let at = (base as i64 + day * 86_400 + hour * 3_600 + minute * 60) as u64;
        let date = match day {
            0 => "2026-11-24".to_string(),
            d => format!("2026-{:02}-{:02}", if 24 + d >= 1 { 11 } else { 10 }, if 24 + d >= 1 { 24 + d } else { 31 + 24 + d }),
        };
        list.push((at, format!("{date} {hour:02}:{minute:02}  {kb} KB")));
    }
    h.app.history = Some(list);
    shot(&mut h, "history");

    println!("screenshots in {}", out.display());
    drop(h);
    let _ = std::fs::remove_dir_all(scratch);
}

/// Stands in for n0te-sync on the run's own socket, so the app shows its connected panels: GUI
/// sessions stay open (nothing is sent), every one-shot request gets "not here".
#[cfg(unix)]
mod stand_in {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::path::Path;

    pub fn start(scratch: &Path) {
        let path = n0te_platform::ipc::Endpoint::service().path().to_path_buf();
        assert!(path.starts_with(scratch), "the socket must be the run's own: {}", path.display());
        let listener = UnixListener::bind(&path).expect("socket");
        std::thread::spawn(move || {
            for conn in listener.incoming().flatten() {
                std::thread::spawn(move || {
                    let Ok(read) = conn.try_clone() else { return };
                    let mut lines = BufReader::new(read).lines();
                    let Some(Ok(first)) = lines.next() else { return };
                    if first.contains("\"session\"") {
                        for _ in lines {}
                        return;
                    }
                    let mut conn = conn;
                    let _ = writeln!(conn, "{{\"ok\":false,\"out\":\"(Demo)\"}}");
                });
            }
        });
    }
}

#[cfg(not(unix))]
mod stand_in {
    pub fn start(_: &std::path::Path) {}
}

struct Ids {
    analysis: String,
    functions: String,
    sketches: String,
}

/// The library: the demo notebook and a few more, in folders, with tags, pins and covers.
fn build_library(lib: &Library) -> Ids {
    let analysis = demo::save_into(lib);
    let nb = lib.load_notebook(&analysis).unwrap();
    nb.set_folder("Uni/WS26").unwrap();
    nb.set_tags(&["Mathe".into()]).unwrap();
    nb.set_cover("initial").unwrap();
    lib.save_notebook(&nb).unwrap();
    read_handwriting(lib, &analysis);

    let mut made = Vec::new();
    for (title, folder, tags, pinned, cover) in [
        ("Lineare Algebra", "Uni/WS26", &["Mathe", "Prüfung"][..], true, "dots"),
        ("Physik II", "Uni/WS26", &[][..], false, "blue"),
        ("Programmieren", "Uni/WS26", &["Info"][..], false, "black"),
        ("Funktionen", "Uni/WS26", &["Mathe"][..], false, "stripes"),
        ("Analysis I · SS26", "Uni/SS26", &["Mathe"][..], false, "red"),
        ("Skizzen", "", &["Privat"][..], true, "orange"),
        ("Ideen", "", &["Privat"][..], false, "green"),
        ("Projekt n0te", "Projekte", &[][..], false, "purple"),
    ] {
        let nb = lib.create_notebook(title).unwrap();
        nb.set_folder(folder).unwrap();
        nb.set_tags(&tags.iter().map(|t| t.to_string()).collect::<Vec<_>>()).unwrap();
        nb.set_pinned(pinned).unwrap();
        nb.set_cover(cover).unwrap();
        lib.save_notebook(&nb).unwrap();
        made.push((title, nb.id()));
    }
    let id = |t: &str| made.iter().find(|(n, _)| *n == t).unwrap().1.clone();
    let functions = id("Funktionen");
    let sketches = id("Skizzen");
    graph_page(lib, &functions);
    pens_page(lib, &sketches);
    Ids { analysis, functions, sketches }
}

/// Lines of the demo's fake handwriting "read" by the tablet's recogniser (the laptop searches
/// what the tablet stores), and the first line of a page as a handwritten heading.
fn read_handwriting(lib: &Library, nb: &str) {
    let lines: [(&str, &str, &[&str]); 3] = [
        ("V04", "Folgen und Grenzwerte", &["Eine Folge konvergiert gegen ihren Grenzwert a", "Der Grenzwert ist eindeutig, falls er existiert", "Jede konvergente Folge ist beschränkt"]),
        ("V05", "Reihen", &["Eine Reihe konvergiert, wenn ihre Partialsummen einen Grenzwert haben", "Geometrische Reihe: Grenzwert 1/(1-q) für |q| < 1"]),
        ("Probeklausur 2", "Probeklausur 2", &["Aufgabe 1: Grenzwert der Folge bestimmen"]),
    ];
    for (page, heading, read) in lines {
        let sid = sheet_titled(lib, nb, page);
        let sheet = lib.load_sheet(nb, &sid).unwrap();
        let rows = text_rows(&sheet);
        if let Some(first) = rows.first() {
            sheet.add_heading(heading, first).unwrap();
        }
        let index: Vec<IndexLine> = read
            .iter()
            .enumerate()
            .filter_map(|(i, text)| {
                let strokes = rows.get(i + 1)?.clone();
                Some(IndexLine { key: format!("l{i}"), text: text.to_string(), alts: vec![], strokes, x0: 0.0, y0: 0.0, x1: 0.0, y1: 0.0 })
            })
            .collect();
        sheet.set_index(&index, &[]).unwrap();
        lib.save_sheet(nb, &sid, &sheet).unwrap();
    }
}

/// The strokes of a page grouped into rows of writing, top to bottom (only black and blue words).
fn text_rows(sheet: &Sheet) -> Vec<Vec<String>> {
    let mut words: Vec<(f32, String)> = sheet
        .strokes()
        .into_iter()
        .filter(|s| s.brush.color != INK_RED && s.bounds.width() > 8.0 && s.bounds.height() < 60.0)
        .map(|s| (s.bounds.y1, s.id))
        .collect();
    words.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut rows: Vec<(f32, Vec<String>)> = Vec::new();
    for (y, id) in words {
        match rows.last_mut() {
            Some((ry, ids)) if (y - *ry).abs() < 9.0 => ids.push(id),
            _ => rows.push((y, vec![id])),
        }
    }
    rows.into_iter().map(|(_, ids)| ids).collect()
}

/// "Funktionen": squared paper, a task written by hand, a construction (two functions, their
/// intersections, a slider for the line's slope).
fn graph_page(lib: &Library, nb_id: &str) {
    let nb = lib.load_notebook(nb_id).unwrap();
    let spec = SheetSpec { spacing: 17.0, ..SheetSpec::a4(Background::Grid) };
    let (node, sid) = nb.add_sheet(None, 0, &spec).unwrap();
    nb.set_title_of(&node, "Aufgabe 3").unwrap();
    lib.save_notebook(&nb).unwrap();
    let sheet = lib.load_sheet(nb_id, &sid).unwrap();
    let mut rng = Rng::new(0xF00D);
    let mut x = 51.0;
    for w in [70.0, 96.0, 40.0, 120.0] {
        sheet.add_stroke(pen(INK_BLACK, 2.2), &demo::word(&mut rng, x, 85.0, w, 8.0, 120)).unwrap();
        x += w + 16.0;
    }
    let mut x = 51.0;
    for w in [60.0, 30.0, 88.0, 52.0, 110.0] {
        sheet.add_stroke(pen(INK_BLUE, 1.8), &demo::word(&mut rng, x, 119.0, w, 6.0, 120)).unwrap();
        x += w + 12.0;
    }
    let mut p = n0te_core::plot::Plot::centered(85.0, 153.0, 425.0, 340.0, 34.0, n0te_desktop::plots::AXES, 2.2);
    p.grid = false;
    for input in ["f(x) = 0.5x^2 - 2", "g(x) = m x + 1", "Schneide(f, g)"] {
        p = p.add(input).unwrap_or_else(|e| panic!("{input}: {e}"));
    }
    sheet.add_plot(&p).unwrap();
    lib.save_sheet(nb_id, &sid, &sheet).unwrap();
}

/// "Skizzen": what the tablet's pens leave behind: fountain pen and ballpoint words, a pencil
/// sketch with hatching, marker over a line, colours.
fn pens_page(lib: &Library, nb_id: &str) {
    let nb = lib.load_notebook(nb_id).unwrap();
    let (node, sid) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Dots)).unwrap();
    nb.set_title_of(&node, "Stifte").unwrap();
    lib.save_notebook(&nb).unwrap();
    let sheet = lib.load_sheet(nb_id, &sid).unwrap();
    let mut rng = Rng::new(0xC0FFEE);
    // A title in a broad, pressure-sensitive nib, underlined in red.
    let mut x = 60.0;
    for w in [80.0, 120.0] {
        sheet.add_stroke(Brush { width: 3.4, thinning: 0.85, color: INK_BLACK, kind: 0 }, &demo::word(&mut rng, x, 90.0, w, 12.0, 140)).unwrap();
        x += w + 22.0;
    }
    sheet.add_stroke(pen(INK_RED, 2.4), &line(56.0, 102.0, x - 14.0, 100.0, 60)).unwrap();
    // Lines of writing in ballpoint (black, blue), one with marker over it.
    for (row, color) in [(0, INK_BLACK), (1, INK_BLUE), (2, INK_BLACK), (3, INK_BLACK)] {
        let y = 150.0 + row as f32 * 30.0;
        let mut x = 60.0;
        while x < 500.0 {
            let w = rng.range(30.0, 80.0);
            sheet.add_stroke(pen(color, 1.7), &demo::word(&mut rng, x, y, w, 6.5, 120)).unwrap();
            x += w + rng.range(9.0, 14.0);
        }
        if row == 2 {
            sheet.add_stroke(Brush { width: 14.0, thinning: 0.0, color: 0x66FF_D600, kind: KIND_MARKER }, &line(56.0, y - 4.0, 380.0, y - 5.0, 60)).unwrap();
        }
    }
    // A pencil sketch: a box in perspective, hatched.
    let pencil = Brush { width: 2.6, thinning: 0.3, color: 0xFF33_3338, kind: KIND_PENCIL };
    let (bx, by) = (90.0, 330.0);
    let front = [(bx, by + 60.0), (bx + 160.0, by + 60.0), (bx + 160.0, by + 200.0), (bx, by + 200.0)];
    let back = front.map(|(x, y)| (x + 70.0, y - 55.0));
    for i in 0..4 {
        let (a, b) = (front[i], front[(i + 1) % 4]);
        sheet.add_stroke(pencil, &line(a.0, a.1, b.0, b.1, 50)).unwrap();
        let (a2, b2) = (back[i], back[(i + 1) % 4]);
        if i < 2 {
            sheet.add_stroke(pencil, &line(a2.0, a2.1, b2.0, b2.1, 50)).unwrap();
        }
        if i != 3 {
            sheet.add_stroke(pencil, &line(front[i].0, front[i].1, back[i].0, back[i].1, 40)).unwrap();
        }
    }
    let shade = Brush { width: 1.6, thinning: 0.2, color: 0xFF55_5560, kind: KIND_PENCIL };
    for k in 0..14 {
        let t = k as f32 / 13.0;
        let (x0, y0) = (bx + 160.0 + 70.0 * t, by + 60.0 - 55.0 * t + 6.0);
        sheet.add_stroke(shade, &line(x0, y0, x0, y0 + 128.0, 30)).unwrap();
    }
    // Colours: a few strokes of each pen colour next to the sketch.
    for (i, color) in [INK_BLACK, INK_BLUE, INK_RED, 0xFF2E_7D32, 0xFFEF_6C00, 0xFF6A_1B9A].into_iter().enumerate() {
        let y = 360.0 + i as f32 * 34.0;
        sheet.add_stroke(Brush { width: 2.6, thinning: 0.6, color, kind: 0 }, &demo::word(&mut rng, 380.0, y, 150.0, 7.0, 120)).unwrap();
    }
    lib.save_sheet(nb_id, &sid, &sheet).unwrap();
}

/// Ink on the first slide as the tablet would write it: a word circled in red, a note in blue.
fn annotate_slide(lib: &Library, nb: &str, sid: &str) {
    // Slide text baselines (from the top): 144, 178, 212, 246, 280; 17 pt Helvetica from x 48.
    let sheet = lib.load_sheet(nb, sid).unwrap();
    let mut rng = Rng::new(0xBEEF);
    let ring: Vec<InkPoint> = (0..140)
        .map(|i| {
            let t = i as f32 / 139.0 * std::f32::consts::TAU * 1.08 + 0.4;
            InkPoint { x: 222.0 + 56.0 * t.cos(), y: 139.0 + 17.0 * t.sin(), pressure: 0.6, t_ms: i * 7 }
        })
        .collect();
    sheet.add_stroke(pen(INK_RED, 2.2), &ring).unwrap();
    sheet.add_stroke(pen(INK_RED, 2.0), &line(46.0, 251.0, 400.0, 250.0, 60)).unwrap();
    // A note in blue above the line, an arrow down to the circled word.
    let mut x = 318.0;
    for w in [56.0, 40.0, 90.0] {
        sheet.add_stroke(pen(INK_BLUE, 1.9), &demo::word(&mut rng, x, 108.0, w, 7.0, 120)).unwrap();
        x += w + 12.0;
    }
    let (from, to) = ((310.0, 104.0), (270.0, 124.0));
    sheet.add_stroke(pen(INK_BLUE, 1.9), &line(from.0, from.1, to.0, to.1, 30)).unwrap();
    let (dx, dy) = (from.0 - to.0, from.1 - to.1);
    let len = (dx * dx + dy * dy).sqrt();
    for turn in [0.45f32, -0.45] {
        let (c, s) = (turn.cos(), turn.sin());
        let (hx, hy) = ((dx * c - dy * s) / len * 11.0, (dx * s + dy * c) / len * 11.0);
        sheet.add_stroke(pen(INK_BLUE, 1.9), &line(to.0 + hx, to.1 + hy, to.0, to.1, 10)).unwrap();
    }
    lib.save_sheet(nb, sid, &sheet).unwrap();
}

/// Lecture slides (16:9) made with pdfium: a title bar, a definition, a few points.
fn make_slides(h: &Harness, root: &Path) -> PathBuf {
    use n0te_pdf::pdfium_render::prelude::*;
    let pdf = h.app.pdf.clone().expect("pdfium (scripts/fetch-pdfium.sh linux-x64)");
    let bytes = pdf
        .worker
        .call(10, |docs| {
            docs.engine().with_raw(|p| {
                let mut doc = p.create_new_pdf().unwrap();
                let bold = doc.fonts_mut().helvetica_bold();
                let regular = doc.fonts_mut().helvetica();
                let slides: [(&str, &[&str]); 3] = [
                    ("Folgen und Grenzwerte", &["Definition: (a_n) konvergiert gegen a, wenn", "zu jedem e > 0 ein N existiert mit |a_n - a| < e für n >= N.", "", "Satz: Jede konvergente Folge ist beschraenkt.", "Satz: Der Grenzwert ist eindeutig."]),
                    ("Rechenregeln", &["lim (a_n + b_n) = lim a_n + lim b_n", "lim (a_n * b_n) = lim a_n * lim b_n", "lim (a_n / b_n) = lim a_n / lim b_n, falls lim b_n != 0"]),
                    ("Monotone Folgen", &["Monoton und beschraenkt  =>  konvergent", "Beispiel: a_n = (1 + 1/n)^n  ->  e"]),
                ];
                for (i, (title, lines)) in slides.iter().enumerate() {
                    let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::new_custom(PdfPoints::new(842.0), PdfPoints::new(474.0))).unwrap();
                    page.objects_mut().create_path_object_rect(PdfRect::new_from_values(410.0, 0.0, 474.0, 842.0), None, None, Some(PdfColor::new(238, 238, 241, 255))).unwrap();
                    page.objects_mut().create_text_object(PdfPoints::new(40.0), PdfPoints::new(432.0), &format!("Kapitel 4.{}  {title}", i + 1), bold, PdfPoints::new(24.0)).unwrap();
                    page.objects_mut().create_path_object_rect(PdfRect::new_from_values(398.0, 40.0, 402.0, 120.0), None, None, Some(PdfColor::new(215, 25, 33, 255))).unwrap();
                    for (k, l) in lines.iter().enumerate() {
                        page.objects_mut().create_text_object(PdfPoints::new(48.0), PdfPoints::new(330.0 - 34.0 * k as f32), l, regular, PdfPoints::new(17.0)).unwrap();
                    }
                    page.objects_mut().create_text_object(PdfPoints::new(760.0), PdfPoints::new(20.0), &format!("{}/3", i + 1), regular, PdfPoints::new(11.0)).unwrap();
                }
                doc.save_to_bytes().unwrap()
            })
        })
        .unwrap();
    let file = root.join("Analysis_Kapitel_4.pdf");
    std::fs::write(&file, bytes).unwrap();
    file
}

fn pen(color: u32, width: f32) -> Brush {
    Brush { width, thinning: 0.55, color, kind: 0 }
}

/// A hand-drawn straight line (a slight wobble).
fn line(x0: f32, y0: f32, x1: f32, y1: f32, n: usize) -> Vec<InkPoint> {
    (0..n)
        .map(|i| {
            let t = i as f32 / (n - 1) as f32;
            let wob = (t * std::f32::consts::PI * 2.0).sin() * 0.5;
            InkPoint { x: x0 + (x1 - x0) * t, y: y0 + (y1 - y0) * t + wob, pressure: 0.5 + 0.15 * (t * std::f32::consts::PI).sin(), t_ms: i as u32 * 6 }
        })
        .collect()
}

/// A tick being written on the tablet right now (packed x, y, pressure, t).
fn live_stroke() -> Vec<f32> {
    let mut v = Vec::new();
    for i in 0..40 {
        let t = i as f32 / 39.0;
        let (x, y) = if t < 0.35 { (300.0 + 40.0 * t, 470.0 + 50.0 * t) } else { (314.0 + 90.0 * (t - 0.35), 487.5 - 120.0 * (t - 0.35)) };
        v.extend([x, y, 0.55, i as f32 * 8.0]);
    }
    v
}

fn open(h: &mut Harness, id: &str) {
    if matches!(h.app.screen, Screen::Notebook(_)) {
        h.message(Message::Back);
    }
    h.message(Message::Open(id.to_string()));
    h.events(&[]);
    h.events(&[]);
}

fn settle(h: &mut Harness) {
    for _ in 0..3 {
        h.events(&[]);
        std::thread::sleep(Duration::from_millis(120));
    }
}

/// Lets background work land (PDF tiles, formulas): `n` × ~half a second.
fn wait(h: &mut Harness, n: usize) {
    for _ in 0..n {
        h.events(&[]);
        std::thread::sleep(Duration::from_millis(450));
    }
}

fn sheet_titled(lib: &Library, nb: &str, prefix: &str) -> String {
    let nb = lib.load_notebook(nb).unwrap();
    nb.outline(true)
        .into_iter()
        .find_map(|r| match r {
            OutlineRow::Sheet { sheet_id, title, .. } if title.starts_with(prefix) => Some(sheet_id),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no page {prefix}"))
}

fn first_sheet(lib: &Library, nb: &str) -> String {
    let nb = lib.load_notebook(nb).unwrap();
    nb.outline(true)
        .into_iter()
        .find_map(|r| match r {
            OutlineRow::Sheet { sheet_id, .. } => Some(sheet_id),
            _ => None,
        })
        .expect("a page")
}

/// The open pane's page rectangle of a sheet (layout points) and the camera.
fn page_rect(h: &Harness, sheet: &str) -> (Rect, Camera) {
    let Screen::Notebook(s) = &h.app.screen else { panic!("no notebook open") };
    let o = s.panes.iter().find(|p| p.key == s.active).unwrap_or(&s.panes[0]);
    let i = o.pages.page_of_sheet(sheet).expect("page laid out");
    (o.pages.pages[i].rect(), o.camera)
}

fn look(h: &mut Harness, r: Rect, fill: f32) {
    let c = camera(h);
    let zoom = (c.viewport.width / r.width()).min(c.viewport.height / r.height()) * fill;
    let x = (r.x0 + r.x1) / 2.0 - c.viewport.width / zoom / 2.0;
    let y = (r.y0 + r.y1) / 2.0 - c.viewport.height / zoom / 2.0;
    h.message(Message::Camera(Camera { x, y, zoom, ..c }));
    h.events(&[]);
}

/// The camera of the pane in use.
fn camera(h: &Harness) -> Camera {
    let Screen::Notebook(s) = &h.app.screen else { panic!("no notebook open") };
    s.panes.iter().find(|p| p.key == s.active).unwrap_or(&s.panes[0]).camera
}

/// A whole page in view.
fn show_page(h: &mut Harness, sheet: &str, fill: f32) {
    h.message(Message::Jump(sheet.to_string()));
    h.events(&[]);
    let (r, _) = page_rect(h, sheet);
    look(h, r, fill);
}

/// A page with its side pages (the row it sits in) in view.
fn show_row(h: &mut Harness, sheet: &str, fill: f32) {
    h.message(Message::Jump(sheet.to_string()));
    h.events(&[]);
    let (r, _) = page_rect(h, sheet);
    let Screen::Notebook(s) = &h.app.screen else { return };
    let o = s.panes.iter().find(|p| p.key == s.active).unwrap_or(&s.panes[0]);
    let mut row = r;
    for p in &o.pages.pages {
        let q = p.rect();
        if (q.y0 - r.y0).abs() < 1.0 {
            row = Rect { x0: row.x0.min(q.x0), y0: row.y0.min(q.y0), x1: row.x1.max(q.x1), y1: row.y1.max(q.y1) };
        }
    }
    look(h, row, fill);
}

/// The top part of a page, `zoom` times the fit-to-width size.
fn show_top(h: &mut Harness, sheet: &str, zoom: f32) {
    h.message(Message::Jump(sheet.to_string()));
    h.events(&[]);
    let (r, c) = page_rect(h, sheet);
    let z = c.viewport.height / r.height() * zoom;
    let x = (r.x0 + r.x1) / 2.0 - c.viewport.width / z / 2.0;
    h.message(Message::Camera(Camera { x, y: r.y0 - 12.0 / z, zoom: z, ..c }));
    h.events(&[]);
}

/// The block being written in the open text page (0: none).
fn focused_block(h: &Harness) -> u64 {
    match &h.app.screen {
        Screen::Notebook(s) => s.text_page.as_ref().and_then(|t| t.doc.focus).unwrap_or(0),
        Screen::Library => 0,
    }
}
