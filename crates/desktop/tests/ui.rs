//! End-to-end UI tests through iced's real event path (headless tiny-skia, no window): the
//! library, opening a notebook, wheel pan/zoom, outline jumps, folding a group by clicking its
//! title in the page view, keyboard paging, importing a `.n0te` bundle.

mod harness;

use harness::Harness;
use iced::keyboard::key::Named;
use iced::{Point, Size};
use n0te_core::notebook::OutlineRow;
use n0te_core::store::Library;
use n0te_desktop::app::{Message, Screen};
use n0te_desktop::{App, demo};

fn temp_root(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("n0te-ui-{tag}-{}-{}", std::process::id(), n0te_core::ids::new_id(9)))
}

/// A library with the demo notebook, opened.
fn opened(tag: &str) -> Harness {
    let root = temp_root(tag);
    let id = demo::save_into(&Library::open(&root).unwrap());
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(1600.0, 900.0), 1.0).expect("tiny-skia renderer");
    assert_eq!(h.app.entries.len(), 1, "library lists the notebook");
    h.message(Message::Open(id));
    h.events(&[]);
    h.events(&[]);
    h
}

fn open(h: &Harness) -> &n0te_desktop::app::Open {
    match &h.app.screen {
        Screen::Notebook(o) => o,
        Screen::Library => panic!("no notebook open"),
    }
}

#[test]
fn wheel_pans_ctrl_wheel_zooms_and_outline_jumps() {
    let mut h = opened("wheel");
    let c0 = open(&h).camera;
    assert!(c0.viewport.width > 0.0, "page view reported its size");
    let center = Point::new(900.0, 450.0);
    h.scroll(center, 0.0, -120.0);
    let c1 = open(&h).camera;
    assert!((c1.y - c0.y - 120.0 / c0.zoom).abs() < 0.01, "{c0:?} -> {c1:?}");
    // Ctrl + wheel zooms around the cursor: the page point under it stays put.
    let origin = (1600.0 - c1.viewport.width, 900.0 - 26.0 - 1.0 - c1.viewport.height);
    let page_at = |c: n0te_desktop::pages::Camera| Point::new(c.x + (center.x - origin.0) / c.zoom, c.y + (center.y - origin.1) / c.zoom);
    let before = page_at(c1);
    h.set_modifiers(iced::keyboard::Modifiers::CTRL);
    h.scroll(center, 0.0, 100.0);
    h.set_modifiers(iced::keyboard::Modifiers::empty());
    let c2 = open(&h).camera;
    assert!(c2.zoom > c1.zoom);
    let after = page_at(c2);
    assert!((before.x - after.x).abs() < 0.5 && (before.y - after.y).abs() < 0.5, "{before:?} vs {after:?}");
    // Jump to the last main page from the outline.
    let o = open(&h);
    let last = *o.pages.mains().last().unwrap();
    let (sheet, number) = (o.pages.pages[last].placed.sheet_id.clone(), o.pages.pages[last].placed.number);
    h.message(Message::Jump(sheet));
    let o = open(&h);
    assert_eq!(o.pages.current_number(&o.camera), number);
}

#[test]
fn several_wheel_events_in_one_batch_all_count() {
    use iced::mouse::{Event as Mouse, ScrollDelta};
    let mut h = opened("batch");
    let c0 = open(&h).camera;
    h.cursor = iced::mouse::Cursor::Available(Point::new(900.0, 450.0));
    let wheel = |y| iced::Event::Mouse(Mouse::WheelScrolled { delta: ScrollDelta::Pixels { x: 0.0, y } });
    h.events(&[wheel(-40.0), wheel(-40.0), wheel(-40.0)]);
    assert!((open(&h).camera.y - c0.y - 120.0 / c0.zoom).abs() < 0.01);
}

#[test]
fn clicking_a_group_title_folds_it_and_numbers_stay() {
    let mut h = opened("fold");
    let o = open(&h);
    let rows_before = o.rows.len();
    let total = o.pages.total_pages;
    let last_number = o.pages.pages.iter().filter(|p| p.placed.side == 0).last().unwrap().placed.number;
    // Bring the first group title into view and click it.
    let header = o.pages.headers[0].clone();
    let cam = n0te_desktop::pages::Camera { y: header.rect.y0 - 40.0, ..o.camera };
    h.message(Message::Camera(cam));
    let origin = (1600.0 - cam.viewport.width, 900.0 - 26.0 - 1.0 - cam.viewport.height);
    let at = Point::new(origin.0 + (header.rect.x0 + 40.0 - cam.x) * cam.zoom, origin.1 + (header.rect.y0 + 10.0 - cam.y) * cam.zoom);
    h.click(at);
    let o = open(&h);
    assert!(o.rows.len() < rows_before, "group folded: {} -> {}", rows_before, o.rows.len());
    assert_eq!(o.pages.total_pages, total);
    assert_eq!(o.pages.pages.iter().filter(|p| p.placed.side == 0).last().unwrap().placed.number, last_number, "numbers don't change when folding");
    assert!(matches!(o.rows.iter().find(|r| matches!(r, OutlineRow::Group { node, .. } if *node == header.node)), Some(OutlineRow::Group { collapsed: true, .. })));
    // The fold is saved.
    h.message(Message::Fold(header.node.clone()));
    assert_eq!(open(&h).rows.len(), rows_before);
}

#[test]
fn page_down_pages_and_escape_returns_to_the_library() {
    let mut h = opened("keys");
    let o = open(&h);
    let first = o.pages.current_number(&o.camera);
    // Shortcuts arrive through the keyboard subscription, which the harness doesn't run.
    let key = |h: &Harness, k: Named| match h.key_event(iced::keyboard::Key::Named(k), None)[0].clone() {
        iced::Event::Keyboard(e) => Message::Key(e),
        _ => unreachable!(),
    };
    let m = key(&h, Named::PageDown);
    h.message(m);
    let o = open(&h);
    assert!(o.pages.current_number(&o.camera) > first);
    let m = key(&h, Named::Escape);
    h.message(m);
    assert!(matches!(h.app.screen, Screen::Library));
}

#[test]
fn importing_a_bundle_adds_and_opens_the_notebook() {
    let tablet = Library::open(temp_root("tablet")).unwrap();
    let id = demo::save_into(&tablet);
    let file = temp_root("bundle").with_extension("n0te");
    std::fs::write(&file, tablet.export_bundle(&id).unwrap()).unwrap();
    let mut h = Harness::new(App::new(temp_root("laptop")), "tiny-skia", Size::new(1600.0, 900.0), 1.0).unwrap();
    assert!(h.app.entries.is_empty());
    h.message(Message::Import(file));
    assert_eq!(h.app.entries.len(), 1);
    assert!(matches!(&h.app.screen, Screen::Notebook(o) if o.id == id));
}

/// A two-page PDF made with the app's own pdfium.
fn two_page_pdf(app: &App) -> Vec<u8> {
    use n0te_pdf::pdfium_render::prelude::*;
    let pdf = app.pdf.as_ref().expect("pdfium (third_party/pdfium) for the PDF tests");
    pdf.worker
        .call(10, |docs| {
            docs.engine().with_raw(|p| {
                let mut doc = p.create_new_pdf().unwrap();
                let font = doc.fonts_mut().helvetica();
                for i in 0..2 {
                    let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::a4()).unwrap();
                    page.objects_mut().create_text_object(PdfPoints::new(72.0), PdfPoints::new(742.0), &format!("Folie {i}"), font, PdfPoints::new(24.0)).unwrap();
                }
                doc.save_to_bytes().unwrap()
            })
        })
        .unwrap()
}

#[test]
fn pdf_import_insert_and_export() {
    let root = temp_root("pdf");
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(1600.0, 900.0), 1.0).unwrap();
    let file = root.join("Vorlesung_03.pdf");
    std::fs::write(&file, two_page_pdf(&h.app)).unwrap();
    // In the library a PDF becomes a notebook named after the file.
    h.message(Message::ImportPdf(file.clone()));
    assert_eq!(h.app.entries.len(), 1);
    assert_eq!(h.app.entries[0].title, "Vorlesung 03");
    let pdf_pages = |h: &Harness| open(h).pages.pages.iter().filter(|p| p.placed.spec.pdf.is_some()).count();
    assert_eq!(pdf_pages(&h), 2);
    // Inside the notebook it is inserted after the current page.
    h.message(Message::FileDropped(file));
    assert_eq!(pdf_pages(&h), 4);
    // Tiles render in the background and show up on a later frame.
    h.events(&[]);
    std::thread::sleep(std::time::Duration::from_millis(400));
    h.events(&[]);
    // Export: whole notebook and the picked pages, into the export folder.
    h.app.export_dir = root.join("out");
    h.message(Message::Export(n0te_desktop::app::ExportWhat::All));
    let all = h.app.last_export.clone().expect("exported");
    let bytes = std::fs::read(&all).unwrap();
    assert!(bytes.starts_with(b"%PDF"));
    h.message(Message::ShowExport(true));
    let first = open(&h).pages.pages[0].placed.node.clone();
    h.message(Message::TogglePick(first));
    h.message(Message::Export(n0te_desktop::app::ExportWhat::Picked));
    let picked = h.app.last_export.clone().unwrap();
    assert_ne!(picked, all, "a free name, not overwritten");
    assert!(std::fs::metadata(&picked).unwrap().len() < bytes.len() as u64);
}

#[test]
fn opening_another_notebook_directly_gets_its_own_camera() {
    let root = temp_root("switch");
    let lib = Library::open(&root).unwrap();
    let a = demo::save_into(&lib);
    let other = lib.create_notebook("B").unwrap();
    other.add_sheet(None, 0, &n0te_core::notebook::SheetSpec::a4(n0te_core::notebook::Background::Grid)).unwrap();
    lib.save_notebook(&other).unwrap();
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(1600.0, 900.0), 1.0).unwrap();
    h.message(Message::Open(a));
    h.events(&[]);
    h.message(Message::Zoom(2.0));
    // Straight to B without passing the library (e.g. a dropped .n0te file).
    h.message(Message::Open(other.id()));
    h.events(&[]);
    assert!(open(&h).camera.viewport.width > 0.0, "B got a camera for the view");
}

/// A library with one empty grid page, opened.
fn blank_notebook(tag: &str) -> (Harness, std::path::PathBuf, String) {
    let root = temp_root(tag);
    let lib = Library::open(&root).unwrap();
    let nb = lib.create_notebook("Texte").unwrap();
    nb.add_sheet(None, 0, &n0te_core::notebook::SheetSpec::a4(n0te_core::notebook::Background::Lines)).unwrap();
    lib.save_notebook(&nb).unwrap();
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(1600.0, 900.0), 1.0).unwrap();
    h.message(Message::Open(nb.id()));
    h.events(&[]);
    h.events(&[]);
    (h, root, nb.id())
}

fn stored_texts(root: &std::path::Path, nb: &str) -> Vec<n0te_core::sheet::TextBox> {
    let lib = Library::open(root).unwrap();
    let n = lib.load_notebook(nb).unwrap();
    let sheet = n.sheet_order()[0].clone();
    lib.load_sheet(nb, &sheet).unwrap().text_boxes()
}

#[test]
fn double_click_creates_a_text_box_typing_saves_it() {
    let (mut h, root, nb) = blank_notebook("text");
    let cam = open(&h).camera;
    let page = open(&h).pages.pages[0].placed.rect;
    let origin = (1600.0 - cam.viewport.width, 900.0 - 26.0 - 1.0 - cam.viewport.height);
    let at = Point::new(origin.0 + (page.x0 + 60.0 - cam.x) * cam.zoom, origin.1 + (page.y0 + 100.0 - cam.y) * cam.zoom);
    h.click(at);
    h.click(at);
    assert!(h.app.editing.is_some(), "double click opened a text box for editing");
    h.type_text("# Satz 4.2 **wichtig**");
    h.message(Message::DoneText);
    let texts = stored_texts(&root, &nb);
    assert_eq!(texts.len(), 1);
    assert_eq!(texts[0].text, "# Satz 4.2 **wichtig**");
    assert!((texts[0].x - 60.0).abs() < 1.0 && (texts[0].y - 100.0).abs() < 1.0, "{:?}", texts[0]);
    // Edit again: move, widen, bigger, then delete.
    let (sheet, id) = (open(&h).pages.pages[0].placed.sheet_id.clone(), texts[0].id.clone());
    h.message(Message::EditText(sheet, id));
    h.message(Message::MoveText(iced::Vector::new(20.0 * cam.zoom, 10.0 * cam.zoom)));
    h.message(Message::ResizeText(iced::Vector::new(30.0 * cam.zoom, 0.0)));
    h.message(Message::PlaceText);
    h.message(Message::TextSize(2.0));
    let t = &stored_texts(&root, &nb)[0];
    assert!((t.x - 80.0).abs() < 1.0 && (t.y - 110.0).abs() < 1.0 && (t.w - texts[0].w - 30.0).abs() < 1.0 && t.size == 16.0, "{t:?}");
    h.message(Message::DeleteText);
    assert!(stored_texts(&root, &nb).is_empty());
}

#[test]
fn an_empty_box_disappears_and_ctrl_t_makes_one() {
    let (mut h, root, nb) = blank_notebook("ctrlt");
    let ev = match h.key_event(iced::keyboard::Key::Character("t".into()), None)[0].clone() {
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modified_key, physical_key, location, text, repeat, .. }) => {
            iced::keyboard::Event::KeyPressed { key, modified_key, physical_key, location, modifiers: iced::keyboard::Modifiers::CTRL, text, repeat }
        }
        _ => unreachable!(),
    };
    h.message(Message::Key(ev));
    assert!(h.app.editing.is_some());
    assert_eq!(stored_texts(&root, &nb).len(), 1);
    h.message(Message::DoneText);
    assert!(stored_texts(&root, &nb).is_empty(), "nothing typed: the box is gone");
}

fn stored_body(root: &std::path::Path, nb: &str, sheet: &str) -> String {
    Library::open(root).unwrap().load_sheet(nb, sheet).unwrap().page_text()
}

#[test]
fn text_page_with_markdown_shortcuts_saves_its_body() {
    let (mut h, root, nb) = blank_notebook("textpage");
    h.message(Message::NewTextPage);
    let sheet = h.app.text_page.as_ref().expect("text page open").sheet_id.clone();
    assert!(!h.app.text_page.as_ref().unwrap().docked);
    h.type_text("# Mitschrift");
    h.press(Named::Enter);
    h.type_text("Der Mittelwertsatz.");
    h.message(Message::CloseTextPage);
    assert_eq!(stored_body(&root, &nb, &sheet), "# Mitschrift\n\nDer Mittelwertsatz.");
    // It sits after the first page and shows in the outline as a text page.
    let o = open(&h);
    let p = o.pages.pages.iter().find(|p| p.placed.sheet_id == sheet).unwrap();
    assert_eq!((p.placed.number, p.placed.spec.background), (2, n0te_core::notebook::Background::Text));
    // Opening it again from the canvas shows the stored body as blocks.
    h.message(Message::OpenTextPage(sheet.clone(), false));
    assert_eq!(h.app.text_page.as_ref().unwrap().doc.blocks.len(), 2);
}

#[test]
fn notes_column_creates_a_side_text_page_and_follows_the_page() {
    let (mut h, root, nb) = blank_notebook("notes");
    h.message(Message::ToggleNotes);
    let t = h.app.text_page.as_ref().expect("notes open");
    assert!(t.docked);
    let sheet = t.sheet_id.clone();
    let o = open(&h);
    let side = o.pages.pages.iter().find(|p| p.placed.sheet_id == sheet).expect("notes sheet laid out");
    assert_eq!((side.placed.side, side.placed.label()), (1, "1 R1".to_string()));
    h.type_text("Frage: gilt das für komplexe Zahlen?");
    h.message(Message::ToggleNotes);
    assert!(h.app.text_page.is_none());
    assert_eq!(stored_body(&root, &nb, &sheet), "Frage: gilt das für komplexe Zahlen?");
}

#[test]
fn palette_finds_and_runs_commands_and_pages() {
    let (mut h, _root, _nb) = blank_notebook("palette");
    h.message(Message::OpenPalette);
    assert!(h.app.palette.is_some());
    h.type_text("textseite");
    h.message(Message::PaletteRun(None));
    assert!(h.app.palette.is_none());
    assert!(h.app.text_page.is_some(), "\"Neue Textseite\" ran");
    h.message(Message::CloseTextPage);
    // Pages by number: go to page 2 (the new text page).
    h.message(Message::OpenPalette);
    h.type_text("seite 2");
    h.message(Message::PaletteRun(None));
    let o = open(&h);
    assert_eq!(o.pages.current_number(&o.camera), 2);
}

#[test]
fn theme_and_gaps_change_and_the_view_follows() {
    let (mut h, _root, _nb) = blank_notebook("theme");
    h.message(Message::NewTextPage);
    h.message(Message::CloseTextPage);
    let gap = |h: &Harness| {
        let o = open(h);
        o.pages.pages[1].placed.rect.y0 - o.pages.pages[0].placed.rect.y1
    };
    let before = gap(&h);
    h.message(Message::SetRowGap(100.0));
    assert!((gap(&h) - 100.0).abs() < 0.01 && before != 100.0);
    h.message(Message::SetTheme("sepia"));
    assert_eq!(n0te_desktop::theme::p().id, "sepia");
    assert!(matches!(h.app.screen, Screen::Notebook(_)), "still in the notebook");
    h.message(Message::SetTheme("nothing"));
}

#[test]
fn live_events_show_the_tablet_and_changes_from_it() {
    use n0te_desktop::live::LiveEvent;
    let (mut h, root, nb) = blank_notebook("live");
    let sheet = open(&h).pages.pages[0].placed.sheet_id.clone();
    // The tablet's view and laser pointer appear; following moves the camera there.
    h.app.live_session = Some(n0te_desktop::app::LiveSession { device: "tab".into(), name: "Tablet".into(), follow_them: true, they_follow: false });
    let before = open(&h).camera;
    h.message(Message::Live(LiveEvent::Presence { device: "tab".into(), notebook: nb.clone(), sheet: Some(sheet.clone()), viewport: [100.0, 500.0, 400.0, 700.0], zoom: 2.0, laser: Some([150.0, 550.0]) }));
    let o = open(&h);
    assert_eq!(o.pages.remote.borrow().len(), 1);
    assert_eq!(o.pages.remote.borrow()[0].laser, Some((150.0, 550.0)));
    assert!(o.camera != before && (o.camera.zoom - o.camera.viewport.width / 300.0).abs() < 0.01, "followed: {:?}", o.camera);
    // A stroke being drawn shows live, until the stored one arrives.
    let samples: Vec<u8> = [100.0f32, 600.0, 0.5, 0.0, 140.0, 610.0, 0.5, 8.0].iter().flat_map(|f| f.to_le_bytes()).collect();
    h.message(Message::Live(LiveEvent::Stroke { device: "tab".into(), notebook: nb.clone(), sheet: sheet.clone(), stroke: 1, brush: (2.0, 0.5, 0xFF111111, 0), samples, done: true }));
    assert_eq!(open(&h).pages.live.borrow().values().next().unwrap().points.len(), 2);
    h.events(&[]);
    // A save from just before the stroke (only a new page): the live stroke stays (it used to
    // vanish here and come back with the next save).
    {
        let lib = Library::open(&root).unwrap();
        let n = lib.load_notebook(&nb).unwrap();
        n.add_sheet(None, 1, &n0te_core::notebook::SheetSpec::a4(n0te_core::notebook::Background::Grid)).unwrap();
        lib.save_notebook(&n).unwrap();
    }
    h.message(Message::Live(LiveEvent::Changed { notebook: nb.clone(), index: true, sheets: vec![sheet.clone()] }));
    let o = open(&h);
    assert_eq!(o.pages.live.borrow().len(), 1, "not stored yet: still shown live");
    assert_eq!(o.pages.total_pages, 2, "the tablet's new page shows up");
    // The stored stroke arrives: the live copy goes.
    {
        let lib = Library::open(&root).unwrap();
        let sh = lib.load_sheet(&nb, &sheet).unwrap();
        let pts = [(100.0, 600.0, 0u32), (140.0, 610.0, 8)].map(|(x, y, t)| n0te_core::ink::InkPoint { x, y, pressure: 0.5, t_ms: t });
        sh.add_stroke(n0te_core::ink::Brush { width: 2.0, thinning: 0.5, color: 0xFF111111, kind: 0 }, &pts).unwrap();
        lib.save_sheet(&nb, &sheet, &sh).unwrap();
    }
    h.message(Message::Live(LiveEvent::Changed { notebook: nb.clone(), index: false, sheets: vec![sheet.clone()] }));
    assert!(open(&h).pages.live.borrow().is_empty(), "stored now: the live copy is gone");
}

#[test]
fn erasing_on_the_tablet_shows_at_once_and_the_frame_can_be_hidden() {
    use n0te_desktop::live::LiveEvent;
    let (mut h, root, nb) = blank_notebook("erase");
    let sheet = open(&h).pages.pages[0].placed.sheet_id.clone();
    let brush = n0te_core::ink::Brush { width: 2.0, thinning: 0.5, color: 0xFF111111, kind: 0 };
    let line = |y: f32| (0..30).map(|i| n0te_core::ink::InkPoint { x: 100.0 + i as f32 * 5.0, y, pressure: 0.5, t_ms: i * 8 }).collect::<Vec<_>>();
    let (a, b) = {
        let lib = Library::open(&root).unwrap();
        let sh = lib.load_sheet(&nb, &sheet).unwrap();
        let a = sh.add_stroke(brush, &line(200.0)).unwrap();
        let b = sh.add_stroke(brush, &line(300.0)).unwrap();
        lib.save_sheet(&nb, &sheet, &sh).unwrap();
        (a, b)
    };
    h.message(Message::Live(LiveEvent::Changed { notebook: nb.clone(), index: false, sheets: vec![sheet.clone()] }));
    // Stroke eraser on a, area eraser through the middle of b: shown before any save.
    h.message(Message::Live(LiveEvent::Erase { notebook: nb.clone(), sheet: sheet.clone(), ids: vec![a.clone()], dabs: vec![[175.0, 300.0, 8.0]] }));
    h.events(&[]);
    {
        let o = open(&h);
        let erasing = o.pages.erasing.borrow();
        let e = erasing.get(&sheet).expect("erasing shown");
        let content: Vec<n0te_core::sheet::Stroke> = Library::open(&root).unwrap().load_sheet(&nb, &sheet).unwrap().strokes();
        let shown = e.apply(&content);
        assert!(shown.iter().all(|s| s.id != a), "the hidden stroke is gone");
        assert_eq!(shown.iter().filter(|s| s.id.starts_with(&format!("{b}#"))).count(), 2, "b is cut in two");
        // A stroke drawn afterwards in the erased spot stays whole.
        let later = n0te_core::sheet::Stroke { id: "later".into(), brush, bounds: n0te_core::ink::Rect::new(160.0, 295.0, 190.0, 305.0), points: line(300.0) };
        assert!(e.apply(&[later]).iter().any(|s| s.id == "later"));
    }
    // The stored result arrives: the live overlay goes.
    {
        let lib = Library::open(&root).unwrap();
        let sh = lib.load_sheet(&nb, &sheet).unwrap();
        sh.remove(&[a.clone()]).unwrap();
        sh.erase_area(&[(175.0, 300.0, 8.0)]).unwrap();
        lib.save_sheet(&nb, &sheet, &sh).unwrap();
    }
    h.message(Message::Live(LiveEvent::Changed { notebook: nb.clone(), index: false, sheets: vec![sheet.clone()] }));
    assert!(open(&h).pages.erasing.borrow().is_empty(), "stored now: overlay dropped");
    // The red frame of the other device can be switched off.
    h.message(Message::RemoteFrame(false));
    assert!(!open(&h).pages.show_remote_frame.get() && !h.app.settings.remote_frame);
}

#[test]
fn following_goes_one_way_and_ends_when_the_tablet_reconnects() {
    use n0te_desktop::live::LiveEvent;
    let (mut h, _root, _nb) = blank_notebook("follow");
    let tab = || n0te_desktop::app::LiveSession { device: "tab".into(), name: "Tablet".into(), follow_them: true, they_follow: false };
    h.app.live_session = Some(tab());
    // "Tablet folgt mir" ends "Ansicht folgt dem Tablet" (both at once chased each other).
    h.message(Message::TheyFollow(true));
    let l = h.app.live_session.clone().unwrap();
    assert!(l.they_follow && !l.follow_them);
    // The tablet asks us to follow it: it leads, so it no longer follows us.
    h.message(Message::Live(LiveEvent::Follow { device: "tab".into(), on: true }));
    let l = h.app.live_session.clone().unwrap();
    assert!(l.follow_them && !l.they_follow);
    // It reconnects (e.g. restarted for an update): no stale follow is left.
    h.message(Message::Live(LiveEvent::Online { id: "tab".into(), name: "Tablet".into() }));
    let l = h.app.live_session.clone().unwrap();
    assert!(!l.follow_them && !l.they_follow);
}

#[test]
fn an_open_request_from_the_tablet_opens_the_notebook_live() {
    use n0te_desktop::live::LiveEvent;
    let (mut h, _root, nb) = blank_notebook("openreq");
    h.message(Message::Back);
    h.message(Message::Live(LiveEvent::Open { device: "tab".into(), notebook: nb.clone() }));
    assert!(matches!(&h.app.screen, Screen::Notebook(o) if o.id == nb));
    assert_eq!(h.app.live_session.as_ref().map(|l| l.device.as_str()), Some("tab"));
}

/// A 32 × 16 px green JPEG.
const GREEN_JPEG: &[u8] = &[255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0, 255, 219, 0, 67, 0, 3, 2, 2, 2, 2, 2, 3, 2, 2, 2, 3, 3, 3, 3, 4, 6, 4, 4, 4, 4, 4, 8, 6, 6, 5, 6, 9, 8, 10, 10, 9, 8, 9, 9, 10, 12, 15, 12, 10, 11, 14, 11, 9, 9, 13, 17, 13, 14, 15, 16, 16, 17, 16, 10, 12, 18, 19, 18, 16, 19, 15, 16, 16, 16, 255, 219, 0, 67, 1, 3, 3, 3, 4, 3, 4, 8, 4, 4, 8, 16, 11, 9, 11, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 16, 255, 192, 0, 17, 8, 0, 16, 0, 32, 3, 1, 34, 0, 2, 17, 1, 3, 17, 1, 255, 196, 0, 31, 0, 0, 1, 5, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 255, 196, 0, 181, 16, 0, 2, 1, 3, 3, 2, 4, 3, 5, 5, 4, 4, 0, 0, 1, 125, 1, 2, 3, 0, 4, 17, 5, 18, 33, 49, 65, 6, 19, 81, 97, 7, 34, 113, 20, 50, 129, 145, 161, 8, 35, 66, 177, 193, 21, 82, 209, 240, 36, 51, 98, 114, 130, 9, 10, 22, 23, 24, 25, 26, 37, 38, 39, 40, 41, 42, 52, 53, 54, 55, 56, 57, 58, 67, 68, 69, 70, 71, 72, 73, 74, 83, 84, 85, 86, 87, 88, 89, 90, 99, 100, 101, 102, 103, 104, 105, 106, 115, 116, 117, 118, 119, 120, 121, 122, 131, 132, 133, 134, 135, 136, 137, 138, 146, 147, 148, 149, 150, 151, 152, 153, 154, 162, 163, 164, 165, 166, 167, 168, 169, 170, 178, 179, 180, 181, 182, 183, 184, 185, 186, 194, 195, 196, 197, 198, 199, 200, 201, 202, 210, 211, 212, 213, 214, 215, 216, 217, 218, 225, 226, 227, 228, 229, 230, 231, 232, 233, 234, 241, 242, 243, 244, 245, 246, 247, 248, 249, 250, 255, 196, 0, 31, 1, 0, 3, 1, 1, 1, 1, 1, 1, 1, 1, 1, 0, 0, 0, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 255, 196, 0, 181, 17, 0, 2, 1, 2, 4, 4, 3, 4, 7, 5, 4, 4, 0, 1, 2, 119, 0, 1, 2, 3, 17, 4, 5, 33, 49, 6, 18, 65, 81, 7, 97, 113, 19, 34, 50, 129, 8, 20, 66, 145, 161, 177, 193, 9, 35, 51, 82, 240, 21, 98, 114, 209, 10, 22, 36, 52, 225, 37, 241, 23, 24, 25, 26, 38, 39, 40, 41, 42, 53, 54, 55, 56, 57, 58, 67, 68, 69, 70, 71, 72, 73, 74, 83, 84, 85, 86, 87, 88, 89, 90, 99, 100, 101, 102, 103, 104, 105, 106, 115, 116, 117, 118, 119, 120, 121, 122, 130, 131, 132, 133, 134, 135, 136, 137, 138, 146, 147, 148, 149, 150, 151, 152, 153, 154, 162, 163, 164, 165, 166, 167, 168, 169, 170, 178, 179, 180, 181, 182, 183, 184, 185, 186, 194, 195, 196, 197, 198, 199, 200, 201, 202, 210, 211, 212, 213, 214, 215, 216, 217, 218, 226, 227, 228, 229, 230, 231, 232, 233, 234, 242, 243, 244, 245, 246, 247, 248, 249, 250, 255, 218, 0, 12, 3, 1, 0, 2, 17, 3, 17, 0, 63, 0, 187, 69, 20, 87, 226, 71, 241, 208, 81, 69, 20, 1, 255, 217];

#[test]
fn images_on_a_page_are_drawn() {
    let root = temp_root("images");
    let lib = Library::open(&root).unwrap();
    let nb = lib.create_notebook("Bilder").unwrap();
    let (_, sheet_id) = nb.add_sheet(None, 0, &n0te_core::notebook::SheetSpec::a4(n0te_core::notebook::Background::Blank)).unwrap();
    lib.save_notebook(&nb).unwrap();
    let blob = lib.put_blob(GREEN_JPEG).unwrap();
    let sheet = lib.load_sheet(&nb.id(), &sheet_id).unwrap();
    sheet.add_image(&blob, 100.0, 100.0, 300.0, 150.0).unwrap();
    lib.save_sheet(&nb.id(), &sheet_id, &sheet).unwrap();
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(1600.0, 900.0), 1.0).unwrap();
    h.message(Message::Open(nb.id()));
    h.events(&[]);
    h.frame();
    let (rgba, _) = h.frame_rgba();
    let green = rgba.chunks_exact(4).filter(|p| p[0] < 80 && p[1] > 160 && p[2] < 110).count();
    // 300 × 150 pt at the opening zoom: thousands of pixels.
    assert!(green > 5_000, "the image is drawn: {green} green pixels");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn guests_are_named_their_frames_can_be_hidden_and_the_share_panel_lists_them() {
    use n0te_desktop::live::{Guest, Invite, LiveEvent};
    let mut h = opened("guests");
    let (nb, sheet) = {
        let o = open(&h);
        (o.id.clone(), o.pages.pages[0].placed.sheet_id.clone())
    };
    h.app.guests = vec![Guest { invite: "inv1".into(), notebook: nb.clone(), name: "Mia".into(), device: "dev-mia".into(), write: false }];
    let presence = |sheet: Option<String>| LiveEvent::Presence { device: "dev-mia".into(), notebook: nb.clone(), sheet, viewport: [10.0, 10.0, 200.0, 150.0], zoom: 1.0, laser: None };
    let marks = |h: &Harness| open(h).pages.remote.borrow().iter().map(|m| m.name.clone()).collect::<Vec<_>>();

    // A guest's view shows with the guest's name (it said "Tablet" before).
    h.message(Message::Live(presence(Some(sheet.clone()))));
    assert_eq!(marks(&h), vec!["Mia".to_string()]);
    // Looking nowhere (the guest's owner-side view sharing stopped / it left the page): gone.
    h.message(Message::Live(presence(None)));
    assert!(marks(&h).is_empty());

    // "Ansicht der Gäste zeigen" off: the frame goes and new ones don't come.
    h.message(Message::Live(presence(Some(sheet.clone()))));
    h.message(Message::GuestFrames(false));
    assert!(marks(&h).is_empty());
    h.message(Message::Live(presence(Some(sheet.clone()))));
    assert!(marks(&h).is_empty(), "no guest frames while switched off");
    h.message(Message::GuestFrames(true));
    h.message(Message::Live(presence(Some(sheet))));
    assert_eq!(marks(&h), vec!["Mia".to_string()]);

    // The share panel with who's here and the invite's rights draws (no service needed).
    // ... with someone at the door, a removed guest, approval and a PIN.
    h.app.waiting = vec![Guest { invite: "inv1".into(), notebook: nb.clone(), name: "Cleo".into(), device: "dev-cleo".into(), write: false }];
    h.app.share = Some(vec![Invite {
        id: "inv1".into(),
        notebook: nb,
        rw: false,
        revoked: false,
        online: 1,
        link: "n0te://invite/x".into(),
        expires: 0,
        approve: true,
        single: false,
        pin: Some("4821".into()),
        members: 2,
        banned: vec![("dev-ben".into(), "Ben".into())],
    }]);
    h.events(&[]);
    // "Entfernen" asks first: one click only arms it (nothing sent yet).
    h.message(Message::RemoveGuest("dev-mia".into()));
    assert_eq!(h.app.removing.as_deref(), Some("dev-mia"));
    h.events(&[]);
    if let Some(dir) = std::env::var_os("N0TE_UI_SHOTS") {
        h.screenshot(std::path::Path::new(&dir).join("share-guests.png"));
    }
}

#[test]
fn as_a_guest_jump_follow_and_get_summoned_and_the_panels_draw() {
    use n0te_desktop::live::{Guest, Invite, Joined, LiveEvent};
    let mut h = opened("guestview");
    let (nb, sheet) = {
        let o = open(&h);
        (o.id.clone(), o.pages.pages[3].placed.sheet_id.clone())
    };
    h.app.joined = vec![Joined { notebook: nb.clone(), owner: "dev-owner".into(), owner_name: "Flinn".into(), write: false, online: true }];
    let cam = |h: &Harness| open(h).camera;
    let presence = |r: [f32; 4]| LiveEvent::Presence { device: "dev-owner".into(), notebook: nb.clone(), sheet: Some(sheet.clone()), viewport: r, zoom: 1.0, laser: None };

    // The owner looks somewhere: jump there once.
    let before = cam(&h);
    h.message(Message::Live(presence([20.0, 30.0, 320.0, 230.0])));
    assert_eq!(cam(&h).y, before.y, "showing a view doesn't move this one");
    h.message(Message::JumpTo("dev-owner".into()));
    let jumped = cam(&h);
    assert!(jumped.y != before.y, "jumped to the owner's page");

    // Follow: the next view moves along; moving by hand ends it.
    h.message(Message::FollowHost(true));
    assert_eq!(h.app.follow_host.as_deref(), Some("dev-owner"));
    h.message(Message::Live(presence([20.0, 400.0, 320.0, 600.0])));
    let followed = cam(&h);
    assert!(followed.y > jumped.y, "followed further down");
    h.message(Message::Camera(followed));
    assert!(h.app.follow_host.is_none(), "moving by hand stops following");
    h.message(Message::Live(presence([20.0, 30.0, 320.0, 230.0])));
    assert_eq!(cam(&h).y, followed.y, "not following any more");

    // Summoned by the owner: brought to its view.
    h.message(Message::Live(LiveEvent::Summoned { device: "dev-owner".into(), notebook: nb.clone(), sheet: sheet.clone(), viewport: [20.0, 700.0, 320.0, 800.0] }));
    assert!(cam(&h).y > followed.y, "summoned there");

    // The guest's panel, then the owner's (calm: one line per invite).
    h.message(Message::ShowShare(true));
    h.events(&[]);
    if let Some(dir) = std::env::var_os("N0TE_UI_SHOTS") {
        h.screenshot(std::path::Path::new(&dir).join("share-guest-side.png"));
    }
    h.app.joined.clear();
    h.app.guests = vec![
        Guest { invite: "inv1".into(), notebook: nb.clone(), name: "Mia".into(), device: "dev-mia".into(), write: false },
        Guest { invite: "inv1".into(), notebook: nb.clone(), name: "Ben".into(), device: "dev-ben".into(), write: true },
    ];
    let inv = |id: &str, rw: bool, pin: Option<&str>| Invite { id: id.into(), notebook: nb.clone(), rw, revoked: false, online: 1, link: format!("n0te://invite/{id}"), expires: 0, approve: pin.is_some(), single: false, pin: pin.map(Into::into), members: 1, banned: vec![] };
    h.app.share = Some(vec![inv("inv1", false, None), inv("inv2", true, Some("4821"))]);
    h.app.invite_open = Some("inv2".into());
    h.events(&[]);
    if let Some(dir) = std::env::var_os("N0TE_UI_SHOTS") {
        h.screenshot(std::path::Path::new(&dir).join("share-owner-side.png"));
    }
}
