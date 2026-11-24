//! Headless screenshots of the screens outside the editor in every design (never opens a
//! window): library (the start, all notebooks as a list, a folder with subfolders, a tag, the
//! right-click menus, the move, tags and cover dialogs, "Auswählen", a card dragged onto a
//! folder and onto "Angeheftet"), the outline (list with page pictures, pictures, the marked
//! pages, a page's menu), settings, export, the page panel, a turned page, the outline with
//! headings, search results and a found spot lighting up, a page with coordinate systems
//! (constructions: a triangle with its circumcircle, a function with a tangent and a slider) and
//! the panel for them, also with commands suggested while one is typed (on the GPU renderer if
//! there is one: the software one draws no ink), two notebooks side by side.
//!
//!   cargo run --profile fast -p n0te-desktop --example styles -- <out-dir> [theme ids…]

#[path = "../tests/harness/mod.rs"]
mod harness;

use std::path::PathBuf;

use harness::Harness;
use iced::{Point, Size};
use n0te_core::store::Library;
use n0te_desktop::app::Message;
use n0te_desktop::shelf::{self, Action, Pick, Target};
use n0te_desktop::widgets::Drag;
use n0te_desktop::{App, demo, theme};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().map(PathBuf::from).expect("out dir");
    let wanted: Vec<String> = args.collect();
    std::fs::create_dir_all(&out).unwrap();
    let root = std::env::temp_dir().join(format!("n0te-styles-{}", std::process::id()));
    let lib = Library::open(&root).unwrap();
    let id = demo::save_into(&lib);
    mark_up(&lib, &id);
    let (plot_sheet, plot_id, plot_id2) = plot_page(&lib, &id);
    // Folders (nested), tags and pins, as the tablet files them.
    let mut ids = std::collections::HashMap::new();
    for (title, folder, cover, tags, pinned) in [
        ("Lineare Algebra", "Uni/WS26", "initial", &["Mathe", "Prüfung"][..], true),
        ("Analysis I", "Uni/WS26", "dots", &["Mathe"][..], false),
        ("Physik Praktikum", "Uni/SS25", "blue", &["Labor"][..], false),
        ("Bachelorarbeit", "Uni", "black", &["Prüfung", "Wichtig", "Entwurf", "Literatur"][..], false),
        ("Ideen", "", "stripes", &["Privat"][..], true),
        ("Rezepte", "Privat", "orange", &[][..], false),
    ] {
        let nb = lib.create_notebook(title).unwrap();
        nb.set_cover(cover).unwrap();
        nb.set_folder(folder).unwrap();
        nb.set_tags(&tags.iter().map(|t| t.to_string()).collect::<Vec<_>>()).unwrap();
        nb.set_pinned(pinned).unwrap();
        lib.save_notebook(&nb).unwrap();
        ids.insert(title, nb.id());
    }
    {
        let nb = lib.load_notebook(&id).unwrap();
        nb.set_folder("Uni/WS26").unwrap();
        lib.save_notebook(&nb).unwrap();
    }
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(1440.0, 900.0), 1.0).expect("renderer");
    let mut gpu = Harness::new(App::new(&root), "wgpu", Size::new(1440.0, 900.0), 1.0);
    for t in theme::PALETTES {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == t.id) {
            continue;
        }
        h.message(Message::SetTheme(t.id));
        h.message(Message::Back);
        // Opened lately (the start's first row): these, the last one first.
        for t in ["Ideen", "Physik Praktikum", "Analysis I", "Lineare Algebra"] {
            h.message(Message::Open(ids[t].clone()));
            h.message(Message::Back);
        }
        h.message(Message::Shelf(shelf::Msg::Pick(Pick::Home)));
        h.events(&[]);
        h.screenshot(out.join(format!("{}-1-library.png", t.id)));
        h.message(Message::Shelf(shelf::Msg::ListView(true)));
        h.events(&[]);
        h.screenshot(out.join(format!("{}-1a-start-list.png", t.id)));
        h.message(Message::Shelf(shelf::Msg::Pick(Pick::All)));
        h.events(&[]);
        h.screenshot(out.join(format!("{}-1a2-all-list.png", t.id)));
        h.message(Message::Shelf(shelf::Msg::ListView(false)));
        library_screens(&mut h, &out, t.id, &ids);
        h.message(Message::StartCreate);
        h.message(Message::NewTitle("Analysis II".into()));
        h.events(&[]);
        h.screenshot(out.join(format!("{}-1b-create.png", t.id)));
        h.message(Message::CancelCreate);
        h.message(Message::ShowSettings(true));
        h.events(&[]);
        h.screenshot(out.join(format!("{}-2-settings.png", t.id)));
        h.message(Message::ShowSettings(false));
        h.message(Message::Open(id.clone()));
        h.events(&[]);
        h.message(Message::ShowExport(true));
        h.events(&[]);
        h.screenshot(out.join(format!("{}-3-export.png", t.id)));
        h.message(Message::ShowExport(false));
        // The page panel, with a size of its own being typed; then the page turned.
        h.message(Message::ShowPage(true));
        h.events(&[]);
        h.screenshot(out.join(format!("{}-4-page.png", t.id)));
        h.message(Message::CustomSize(true));
        h.events(&[]);
        h.screenshot(out.join(format!("{}-4b-custom.png", t.id)));
        h.message(Message::ShowPage(false));
        h.message(Message::RotatePage);
        h.events(&[]);
        h.screenshot(out.join(format!("{}-5-turned.png", t.id)));
        h.message(Message::RotatePage);
        // Search: results in the sidebar, then the first hit shown and lit up; the outline with
        // the headings again.
        h.message(Message::SearchQuery("Grenzwert".into()));
        h.events(&[]);
        h.screenshot(out.join(format!("{}-6-search.png", t.id)));
        h.message(Message::SearchSubmit);
        h.events(&[]);
        h.screenshot(out.join(format!("{}-6b-found.png", t.id)));
        h.message(Message::SearchClear);
        h.events(&[]);
        h.screenshot(out.join(format!("{}-7-headings.png", t.id)));
        split_screens(&mut h, &out, t.id);
        // Coordinate systems on squared paper, then the dialog for one.
        let g = match gpu.as_mut() {
            Some(g) => g,
            None => &mut h,
        };
        g.message(Message::SetTheme(t.id));
        g.message(Message::Open(id.clone()));
        g.events(&[]);
        outline_screens(g, &out, t.id);
        g.message(Message::Jump(plot_sheet.clone()));
        g.events(&[]);
        g.screenshot(out.join(format!("{}-8-plots.png", t.id)));
        // Close up: how the core draws them (the triangle and its circle; the function, the
        // slider, the tangent, the marked points).
        if let n0te_desktop::app::Screen::Notebook(o) = &g.app.screen {
            let page = o.pages.pages[o.pages.page_of_sheet(&plot_sheet).unwrap()].rect();
            let zoom = 3.2;
            let cam = |cy: f32| n0te_desktop::pages::Camera { x: page.x0 + 297.0 - o.camera.viewport.width / zoom / 2.0, y: page.y0 + cy - o.camera.viewport.height / zoom / 2.0, zoom, ..o.camera };
            let (near1, near2, before) = (cam(255.0), cam(560.0), o.camera);
            g.message(Message::Camera(near1));
            g.events(&[]);
            g.screenshot(out.join(format!("{}-8f-plot-near.png", t.id)));
            g.message(Message::Camera(near2));
            g.events(&[]);
            g.screenshot(out.join(format!("{}-8g-plot-near-2.png", t.id)));
            g.message(Message::Camera(before));
            g.events(&[]);
        }
        // The panel: the triangle's construction with a row selected (its properties), a
        // mistake in the input line, a row being edited, the question before deleting.
        use n0te_desktop::plots::{Msg, Tab};
        g.message(Message::EditPlot(plot_sheet.clone(), plot_id.clone()));
        g.events(&[]);
        g.screenshot(out.join(format!("{}-8b-plot-panel.png", t.id)));
        let circle = g.app.plot_edit.as_ref().and_then(|e| e.plot.objs.iter().find(|o| o.name() == "c").map(|o| o.id.clone())).unwrap();
        g.message(Message::Plot(Msg::Pick(circle)));
        g.events(&[]);
        g.screenshot(out.join(format!("{}-8c-plot-properties.png", t.id)));
        g.message(Message::Plot(Msg::Input("Gerade(A)".into())));
        g.message(Message::Plot(Msg::InputSubmit));
        g.events(&[]);
        g.screenshot(out.join(format!("{}-8d-plot-mistake.png", t.id)));
        // A command being typed: the commands that fit under the line.
        g.message(Message::Plot(Msg::Input("S = Mi".into())));
        g.events(&[]);
        g.screenshot(out.join(format!("{}-8j-plot-suggest.png", t.id)));
        g.message(Message::Plot(Msg::Input(String::new())));
        let a = g.app.plot_edit.as_ref().map(|e| e.plot.objs[0].id.clone()).unwrap();
        g.message(Message::Plot(Msg::Edit(a.clone())));
        g.message(Message::Plot(Msg::Insert(2)));
        g.events(&[]);
        g.screenshot(out.join(format!("{}-8e-plot-edit-row.png", t.id)));
        g.message(Message::Plot(Msg::Escape));
        g.message(Message::Plot(Msg::AskRemove(a)));
        g.message(Message::Plot(Msg::Tab(Tab::Graphics)));
        g.message(Message::Plot(Msg::Range(1, "8".into())));
        g.events(&[]);
        g.screenshot(out.join(format!("{}-8h-plot-delete-graphics.png", t.id)));
        g.message(Message::Plot(Msg::Cancel));
        // The second one: the function with its slider (selected) and tangent.
        g.message(Message::EditPlot(plot_sheet.clone(), plot_id2.clone()));
        let slider = g.app.plot_edit.as_ref().map(|e| e.plot.objs[0].id.clone()).unwrap();
        g.message(Message::Plot(Msg::Pick(slider)));
        g.events(&[]);
        g.screenshot(out.join(format!("{}-8i-plot-panel-slider.png", t.id)));
        g.message(Message::Plot(Msg::Cancel));
    }
    let _ = std::fs::remove_dir_all(root);
}

/// The library's organising: a folder with its subfolders and the way to it, a tag, the
/// right-click menus (a notebook, a folder), the dialogs (move with a new folder being named,
/// tags, cover), "Auswählen" with two ticked, a card dragged onto a folder in the sidebar.
fn library_screens(h: &mut Harness, out: &std::path::Path, theme: &str, ids: &std::collections::HashMap<&str, String>) {
    let shot = |h: &mut Harness, name: &str| {
        h.events(&[]);
        h.screenshot(out.join(format!("{theme}-1{name}.png")));
    };
    let la = ids["Lineare Algebra"].clone();
    let ba = ids["Bachelorarbeit"].clone();
    h.message(Message::Shelf(shelf::Msg::Pick(Pick::Folder("Uni".into()))));
    shot(h, "c-folder");
    h.message(Message::Shelf(shelf::Msg::Pick(Pick::Tag("Prüfung".into()))));
    shot(h, "d-tag");
    h.message(Message::Shelf(shelf::Msg::Pick(Pick::All)));
    h.message(Message::Shelf(shelf::Msg::Query("prüfung".into())));
    shot(h, "d2-search");
    h.message(Message::Shelf(shelf::Msg::ClearQuery));
    // The menu where a right click on a card would open it; then the one of a folder.
    h.events(&[]);
    let card = h.bounds_of(shelf::card_id(&la)).expect("card");
    h.move_to(Point::new(card.x + 60.0, card.y + 90.0));
    h.message(Message::Shelf(shelf::Msg::Menu(Target::Notebooks(vec![la.clone()]), Point::new(card.x + 60.0, card.y + 90.0))));
    shot(h, "e-menu");
    h.message(Message::Shelf(shelf::Msg::CloseMenu));
    let row = h.bounds_of(shelf::folder_id("Uni/WS26")).expect("folder row");
    h.message(Message::Shelf(shelf::Msg::Menu(Target::Folder("Uni/WS26".into()), Point::new(row.x + 90.0, row.center_y()))));
    shot(h, "f-folder-menu");
    h.message(Message::Shelf(shelf::Msg::CloseMenu));
    h.move_to(Point::new(5.0, 890.0));
    h.message(Message::Shelf(shelf::Msg::Act(Target::Notebooks(vec![ba.clone()]), Action::Move)));
    h.message(Message::Shelf(shelf::Msg::To("Uni/WS26".into())));
    h.message(Message::Shelf(shelf::Msg::NewFolder));
    h.message(Message::Shelf(shelf::Msg::Text("Klausuren".into())));
    shot(h, "g-move");
    h.message(Message::Shelf(shelf::Msg::CloseDialog));
    h.message(Message::Shelf(shelf::Msg::Act(Target::Notebooks(vec![la.clone()]), Action::Tags)));
    h.message(Message::Shelf(shelf::Msg::Text("Klau".into())));
    shot(h, "h-tags");
    h.message(Message::Shelf(shelf::Msg::CloseDialog));
    h.message(Message::Shelf(shelf::Msg::Act(Target::Notebooks(vec![la.clone()]), Action::Cover)));
    shot(h, "i-cover");
    h.message(Message::Shelf(shelf::Msg::CloseDialog));
    h.message(Message::Shelf(shelf::Msg::Select(true)));
    h.message(Message::Shelf(shelf::Msg::Tick(la.clone())));
    h.message(Message::Shelf(shelf::Msg::Tick(ba.clone())));
    shot(h, "j-select");
    // Both ticked cards dragged onto a folder in the sidebar.
    let card = h.bounds_of(shelf::card_id(&ba)).expect("card");
    let row = h.bounds_of(shelf::folder_id("Uni/SS25")).expect("folder row");
    h.message(Message::Shelf(shelf::Msg::Drag(ba.clone(), Drag::Start(card.center()))));
    h.message(Message::Shelf(shelf::Msg::Drag(ba.clone(), Drag::Move(Point::new(row.x + 120.0, row.center_y())))));
    h.message(Message::Shelf(shelf::Msg::Over(shelf::Drop::Folder("Uni/SS25".into()), true)));
    shot(h, "k-drag");
    // Onto "Angeheftet" (shown while dragging), "Ohne Ordner" there too.
    h.message(Message::Shelf(shelf::Msg::Over(shelf::Drop::Folder("Uni/SS25".into()), false)));
    if let Some(pin) = h.bounds_of(shelf::side_id("pinned")) {
        h.message(Message::Shelf(shelf::Msg::Drag(ba.clone(), Drag::Move(Point::new(pin.x + 120.0, pin.center_y())))));
        h.message(Message::Shelf(shelf::Msg::Over(shelf::Drop::Pin, true)));
    }
    shot(h, "l-drag-pin");
    h.app.shelf.drag = None;
    h.message(Message::Shelf(shelf::Msg::Select(false)));
}

/// Two side by side: the notebook once more beside itself at a later page (the active pane
/// framed, ★ on the first), the ⧉ menu with two open, the divider held near the edge.
fn split_screens(h: &mut Harness, out: &std::path::Path, theme: &str) {
    use n0te_desktop::split::Msg;
    let shot = |h: &mut Harness, name: &str| {
        h.events(&[]);
        h.screenshot(out.join(format!("{theme}-10{name}.png")));
    };
    let sheet = |h: &Harness, n: u32| match &h.app.screen {
        n0te_desktop::app::Screen::Notebook(s) => s.pages.index_of_number(n).map(|i| s.pages.pages[i].placed.sheet_id.clone()),
        _ => None,
    };
    let Some(first) = sheet(h, 2) else { return };
    h.message(Message::Jump(first));
    h.message(Message::Split(Msg::Again));
    h.events(&[]);
    let Some(later) = sheet(h, 6) else { return };
    let second = match &h.app.screen {
        n0te_desktop::app::Screen::Notebook(s) => s.panes[1].key,
        _ => return,
    };
    h.message(Message::Pane(second, Box::new(Message::Jump(later))));
    shot(h, "-split");
    h.message(Message::Split(Msg::Menu(Some(Point::new(730.0, 30.0)))));
    shot(h, "b-split-menu");
    h.message(Message::Split(Msg::Menu(None)));
    h.message(Message::Split(Msg::Drag(0.9)));
    shot(h, "c-split-closing");
    h.message(Message::Split(Msg::Release(0.5)));
    h.message(Message::Split(Msg::Close(second)));
}

/// The outline: the list with page pictures and marked pages (the page in view further down),
/// the pictures, only the marked pages, a page's right-click menu.
fn outline_screens(g: &mut Harness, out: &std::path::Path, theme: &str) {
    use n0te_desktop::sidebar::{self, Msg};
    let shot = |g: &mut Harness, name: &str| {
        g.events(&[]);
        g.screenshot(out.join(format!("{theme}-9{name}.png")));
    };
    let Some(sheet) = (match &g.app.screen {
        n0te_desktop::app::Screen::Notebook(o) => o.pages.index_of_number(9).map(|i| o.pages.pages[i].placed.sheet_id.clone()),
        _ => None,
    }) else {
        return;
    };
    g.message(Message::Jump(sheet));
    g.events(&[]);
    let reveal = sidebar::reveal_current(&g.app);
    g.task(reveal);
    shot(g, "-outline-list");
    g.message(Message::Outline(Msg::Pictures(true)));
    shot(g, "b-outline-pictures");
    g.message(Message::Outline(Msg::OnlyMarked(true)));
    shot(g, "c-outline-marked");
    g.message(Message::Outline(Msg::OnlyMarked(false)));
    g.message(Message::Outline(Msg::Pictures(false)));
    let node = match &g.app.screen {
        n0te_desktop::app::Screen::Notebook(o) => o.pages.index_of_number(9).map(|i| o.pages.pages[i].placed.node.clone()),
        _ => None,
    };
    if let Some(node) = node {
        g.events(&[]);
        let reveal = sidebar::reveal_current(&g.app);
        g.task(reveal);
        // Where a right click on the page's row would be: the row sits a third down the list.
        if let Some(list) = g.bounds_of(sidebar::list_id()) {
            g.message(Message::Outline(Msg::Menu(node, Point::new(list.x + 150.0, list.y + list.height / 3.0 + 14.0))));
            shot(g, "d-page-menu");
            g.message(Message::Outline(Msg::CloseMenu));
        }
    }
}

/// A page of squared paper at the end with two coordinate systems, made as the tablet makes
/// them (the core's `add`): (sheet, the first plot, the second). The first is a triangle with
/// its circumcircle and centre (a hatched polygon, the perpendicular bisectors as dotted
/// helpers, an angle); the second a function with a point on it moved by a slider and the
/// tangent there, its zeros and lows marked, the x axis in π and named.
fn plot_page(lib: &Library, id: &str) -> (String, String, String) {
    use n0te_core::notebook::{Background, SheetSpec};
    use n0te_core::plot::{LabelMode, LineStyle, MARK_EXTREMA, MARK_ZEROS};
    use n0te_desktop::plots;
    let nb = lib.load_notebook(id).unwrap();
    let (_, end) = n0te_desktop::pdf::position_after(&nb, None);
    let (node, sheet_id) = nb.add_sheet(None, end, &SheetSpec { spacing: 17.0, ..SheetSpec::a4(Background::Grid) }).unwrap();
    nb.set_title_of(&node, "Funktionen").unwrap();
    lib.save_notebook(&nb).unwrap();
    let sheet = lib.load_sheet(id, &sheet_id).unwrap();
    let mut p = plots::fresh(297.0, 250.0, 595.0, 842.0, Background::Grid, 17.0);
    for s in ["A = (-3, -2)", "B = (4, -2)", "C = (1, 3)", "Vieleck(A, B, C)", "Kreis(A, B, C)", "M = Mittelpunkt(c)", "Mittelsenkrechte(A, B)", "Mittelsenkrechte(B, C)", "Winkel(B, A, C)"] {
        p = p.add(s).unwrap();
    }
    for o in &mut p.objs[6..8] {
        o.style.aux = true;
        o.style.line = LineStyle::Dotted;
    }
    p.objs[4].style.label = LabelMode::Caption;
    p.objs[4].style.caption = "Umkreis".into();
    let first = sheet.add_plot(&p).unwrap();
    let mut q = plots::fresh(297.0, 560.0, 595.0, 842.0, Background::Blank, 0.0);
    q.grid = false;
    q.clean = true;
    for s in ["a = Schieberegler(-2, 3, 0.1)", "f(x) = 0.5x^2 - 2", "T = Punkt(f, a)", "Tangente(T, f)"] {
        q = q.add(s).unwrap();
    }
    q = q.set_slider(&q.objs[0].id.clone(), 1.5).unwrap();
    q.objs[1].style.special = MARK_ZEROS | MARK_EXTREMA;
    q.objs[1].style.coords = true;
    q.objs[1].style.label = LabelMode::NameValue;
    q.objs[3].style.line = LineStyle::Dashed;
    q.x_name = "x".into();
    q.y_name = "y".into();
    let second = sheet.add_plot(&q).unwrap();
    lib.save_sheet(id, &sheet_id, &sheet).unwrap();
    (sheet_id, first, second)
}

/// What the tablet adds that the demo lacks: headings and handwriting it read (on the demo's
/// strokes), a text box, and an untitled cover named by its heading.
fn mark_up(lib: &Library, id: &str) {
    use n0te_core::notebook::OutlineRow;
    use n0te_core::sheet::IndexLine;
    let nb = lib.load_notebook(id).unwrap();
    let pages: Vec<(String, String, String)> = nb
        .outline(true)
        .into_iter()
        .filter_map(|r| match r {
            OutlineRow::Sheet { node, sheet_id, title, .. } => Some((node, sheet_id, title)),
            OutlineRow::Group { .. } => None,
        })
        .collect();
    let mark = |title: &str, heads: &[&str], lines: &[&str], text: Option<&str>| {
        let Some((node, sheet_id, _)) = pages.iter().find(|p| p.2.starts_with(title)) else { return };
        let sheet = lib.load_sheet(id, sheet_id).unwrap();
        let mut strokes = sheet.stroke_boxes();
        strokes.sort_by(|a, b| a.1.y0.total_cmp(&b.1.y0));
        let mut rest = strokes.into_iter().map(|s| s.0);
        for h in heads {
            let ids: Vec<String> = rest.by_ref().take(3).collect();
            sheet.add_heading(h, &ids).unwrap();
            let _ = rest.by_ref().take(20).count();
        }
        let read: Vec<IndexLine> = lines
            .iter()
            .enumerate()
            .map(|(i, l)| IndexLine { key: format!("l{i}"), text: l.to_string(), alts: Vec::new(), strokes: rest.by_ref().take(6).collect(), x0: 0.0, y0: 0.0, x1: 0.0, y1: 0.0 })
            .collect();
        sheet.set_index(&read, &[]).unwrap();
        if let Some(t) = text {
            sheet.add_text_box(320.0, 120.0, 220.0, 12.0, t).unwrap();
        }
        lib.save_sheet(id, sheet_id, &sheet).unwrap();
        if title == "Deckblatt" {
            nb.set_title_of(node, "").unwrap();
        }
    };
    mark("Deckblatt", &["Analysis I"], &[], None);
    mark("V01", &["Abbildungen", "Injektiv und surjektiv"], &["Eine Abbildung ordnet jedem Element genau eines zu"], None);
    mark("V04", &["Satz 4.1: Grenzwerte sind eindeutig", "Beweis"], &["Der Grenzwert einer Folge ist eindeutig", "Grenzwert und Teilfolgen"], Some("Übung: Grenzwert von (1 + 1/n)^n berechnen"));
    // Pages marked to find again.
    for (node, _, title) in &pages {
        if ["V04", "V07", "Probeklausur 2"].iter().any(|t| title.starts_with(t)) {
            nb.set_marked(node, true).unwrap();
        }
    }
    lib.save_notebook(&nb).unwrap();
}
