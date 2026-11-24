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

/// A one-page PDF of 595 × 842 pt with a full-width blue band at page y 200..400.
fn banded_pdf(app: &App) -> Vec<u8> {
    use n0te_pdf::pdfium_render::prelude::*;
    let pdf = app.pdf.as_ref().expect("pdfium (third_party/pdfium) for the PDF tests");
    pdf.worker
        .call(10, |docs| {
            docs.engine().with_raw(|p| {
                let mut doc = p.create_new_pdf().unwrap();
                let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::from_points(PdfPoints::new(595.0), PdfPoints::new(842.0))).unwrap();
                // (bottom, left, top, right) in PDF space, y up.
                page.objects_mut().create_path_object_rect(PdfRect::new_from_values(842.0 - 400.0, 0.0, 842.0 - 200.0, 595.0), None, None, Some(PdfColor::new(30, 90, 200, 255))).unwrap();
                doc.save_to_bytes().unwrap()
            })
        })
        .unwrap()
}

#[test]
fn a_pdf_page_with_inserted_space_draws_its_pdf_split_there() {
    let root = temp_root("gap");
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(1600.0, 900.0), 1.0).unwrap();
    let pdf = h.app.pdf.clone().expect("pdfium (third_party/pdfium) for the PDF tests");
    let stored = n0te_pdf::notebook::store_pdf(&h.app.lib, &pdf.worker, banded_pdf(&h.app)).unwrap();
    let nb = h.app.lib.create_notebook("Platz").unwrap();
    let sheets = n0te_pdf::notebook::add_pdf_sheets(&nb, &stored, None, 0).unwrap();
    // 150 pt inserted at y 300, through the blue band.
    nb.reshape(&sheets[0].node, 992.0, &[(300.0, 150.0)]).unwrap();
    h.app.lib.save_notebook(&nb).unwrap();
    // A red picture on the moved part of the band: drawn over the PDF.
    let png = root.join("red.png");
    harness::save_png(&png, &[215, 25, 33, 255].repeat(16 * 8), 16, 8);
    let blob = h.app.lib.put_blob(&std::fs::read(&png).unwrap()).unwrap();
    let sheet = h.app.lib.load_sheet(&nb.id(), &sheets[0].sheet_id).unwrap();
    sheet.add_image(&blob, 400.0, 480.0, 100.0, 50.0).unwrap();
    h.app.lib.save_sheet(&nb.id(), &sheets[0].sheet_id, &sheet).unwrap();
    h.message(Message::Open(nb.id()));
    h.events(&[]);
    h.frame();
    let o = open(&h);
    let rect = o.pages.pages[0].rect();
    assert_eq!((rect.width(), rect.height()), (595.0, 992.0), "the page is as long as the PDF plus the space");
    // Zoom 1 (sharp tiles at scale 2), the page's top-left corner 20 px into the view.
    let cam = n0te_desktop::pages::Camera { x: rect.x0 - 20.0, y: rect.y0 - 20.0, zoom: 1.0, ..o.camera };
    assert!(cam.viewport.height > 600.0, "{cam:?}");
    h.message(Message::Camera(cam));
    let blue = [30u8, 90, 200, 255];
    let px = |rgba: &[u8], x: f32, y: f32| -> [u8; 4] {
        let (sx, sy) = ((cam.origin.x + 20.0 + x) as usize, (cam.origin.y + 20.0 + y) as usize);
        let i = (sy * 1600 + sx) * 4;
        rgba[i..i + 4].try_into().unwrap()
    };
    // Tiles render on the PDF worker and show up on a later frame (a debug frame takes ~1.5 s).
    let mut frame = Vec::new();
    for _ in 0..40 {
        h.events(&[]);
        frame = h.frame_rgba().0;
        if px(&frame, 300.0, 250.0) == blue && px(&frame, 300.0, 500.0) == blue && px(&frame, 20.0, 545.0) == blue {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert_eq!(px(&frame, 300.0, 250.0), blue, "the band above the line stays where it is");
    assert_eq!(px(&frame, 300.0, 296.0), blue);
    for y in [304.0, 350.0, 399.0, 446.0] {
        assert_ne!(px(&frame, 300.0, y), blue, "blank paper where the space went in (y {y}), no stretched PDF");
    }
    for y in [454.0, 500.0, 546.0] {
        assert_eq!(px(&frame, 300.0, y), blue, "the rest of the band 150 pt lower (y {y})");
        assert_eq!(px(&frame, 20.0, y), blue);
    }
    assert_ne!(px(&frame, 300.0, 556.0), blue, "and it ends 150 pt lower too");
    assert_ne!(px(&frame, 300.0, 180.0), blue);
    assert_eq!(px(&frame, 450.0, 505.0), [215, 25, 33, 255], "pictures stay over the split PDF");
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
    let sheet = open(&h).text_page.as_ref().expect("text page open").sheet_id.clone();
    assert!(!open(&h).text_page.as_ref().unwrap().docked);
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
    assert_eq!(open(&h).text_page.as_ref().unwrap().doc.blocks.len(), 2);
}

#[test]
fn notes_column_creates_a_side_text_page_and_follows_the_page() {
    let (mut h, root, nb) = blank_notebook("notes");
    h.message(Message::ToggleNotes);
    let t = open(&h).text_page.as_ref().expect("notes open");
    assert!(t.docked);
    let sheet = t.sheet_id.clone();
    let o = open(&h);
    let side = o.pages.pages.iter().find(|p| p.placed.sheet_id == sheet).expect("notes sheet laid out");
    assert_eq!((side.placed.side, side.placed.label()), (1, "1 R1".to_string()));
    h.type_text("Frage: gilt das für komplexe Zahlen?");
    h.message(Message::ToggleNotes);
    assert!(open(&h).text_page.is_none());
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
    assert!(open(&h).text_page.is_some(), "\"Neue Textseite\" ran");
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
    h.app.joined = vec![Joined { notebook: nb.clone(), owner: "dev-owner".into(), owner_name: "Alex".into(), write: false, online: true }];
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

#[test]
fn pinching_zooms_around_the_fingers_and_a_swipe_glides_until_touched() {
    use n0te_desktop::gestures::Gesture;
    let mut h = opened("pinch");
    let c0 = open(&h).camera;
    assert!(c0.origin.x > 0.0, "the view knows where it sits (right of the outline): {c0:?}");
    let fingers = Point::new(c0.origin.x + 300.0, c0.origin.y + 200.0);
    let page_at = |c: n0te_desktop::pages::Camera, p: Point| Point::new(c.x + (p.x - c.origin.x) / c.zoom, c.y + (p.y - c.origin.y) / c.zoom);
    let before = page_at(c0, fingers);
    h.message(Message::Gesture(Gesture::PinchBegin { x: fingers.x, y: fingers.y }));
    h.message(Message::Gesture(Gesture::Pinch { scale: 1.5, dx: 0.0, dy: 0.0 }));
    h.message(Message::Gesture(Gesture::Pinch { scale: 2.0, dx: 0.0, dy: 0.0 }));
    let c1 = open(&h).camera;
    assert!((c1.zoom - c0.zoom * 2.0).abs() < 1e-3, "{} -> {}", c0.zoom, c1.zoom);
    let after = page_at(c1, fingers);
    assert!((before.x - after.x).abs() < 0.5 && (before.y - after.y).abs() < 0.5, "the page stays under the fingers: {before:?} vs {after:?}");
    // Moving both fingers drags the page along.
    h.message(Message::Gesture(Gesture::Pinch { scale: 2.0, dx: 0.0, dy: 40.0 }));
    let c2 = open(&h).camera;
    assert!((c2.y - (c1.y - 40.0 / c1.zoom)).abs() < 0.01, "{c1:?} -> {c2:?}");
    h.message(Message::Gesture(Gesture::PinchEnd));
    // A pinch over the outline does nothing.
    h.message(Message::Gesture(Gesture::PinchBegin { x: 20.0, y: 300.0 }));
    h.message(Message::Gesture(Gesture::Pinch { scale: 3.0, dx: 0.0, dy: 0.0 }));
    assert_eq!(open(&h).camera, c2);
    h.message(Message::Gesture(Gesture::PinchEnd));

    // A swipe that ends in motion glides on, slower and slower; fingers on the pad stop it.
    h.message(Message::Gesture(Gesture::Fling { vx: 0.0, vy: 1200.0 }));
    assert!(h.app.glide.is_some());
    let t0 = std::time::Instant::now();
    h.message(Message::Glide(t0 + std::time::Duration::from_millis(16)));
    h.message(Message::Glide(t0 + std::time::Duration::from_millis(32)));
    let c3 = open(&h).camera;
    assert!(c3.y > c2.y, "glided down: {} -> {}", c2.y, c3.y);
    let (_, _, v, _) = h.app.glide.unwrap();
    assert!(v < 1200.0 && v > 0.0, "slows down: {v}");
    h.message(Message::Gesture(Gesture::Touch));
    assert!(h.app.glide.is_none());
    // Switched off in the settings: no glide.
    h.message(Message::Momentum(false));
    h.message(Message::Gesture(Gesture::Fling { vx: 0.0, vy: 1200.0 }));
    assert!(h.app.glide.is_none());
}

#[test]
fn a_page_turns_takes_a_format_or_its_own_size_and_passes_it_on() {
    let (mut h, root, nb) = blank_notebook("format");
    let spec = |h: &Harness| h.app.page_in_view().unwrap().1;
    assert_eq!((spec(&h).width, spec(&h).height), (595.0, 842.0));
    h.message(Message::RotatePage);
    assert_eq!((spec(&h).width, spec(&h).height), (842.0, 595.0));
    // The page panel: A5 keeps the page lying down, then a size of its own in mm.
    h.message(Message::ShowPage(true));
    h.events(&[]);
    h.message(Message::PageSize(595.0, 420.0));
    assert_eq!(n0te_core::format::describe(spec(&h).width, spec(&h).height), "A5 quer");
    h.message(Message::CustomSize(true));
    assert_eq!(h.app.size_text, Some(("210".into(), "148".into())));
    h.message(Message::SizeWidth("200".into()));
    h.message(Message::SizeHeight("100".into()));
    h.events(&[]);
    h.message(Message::PageSize(n0te_core::format::mm_to_pt(200.0), n0te_core::format::mm_to_pt(100.0)));
    assert_eq!(n0te_core::format::describe(spec(&h).width, spec(&h).height), "200 × 100 mm");
    assert_eq!(h.app.size_text, None, "applying closes the fields");
    // Saved: the library on disk has the new size.
    let lib = Library::open(&root).unwrap();
    let stored = lib.load_notebook(&nb).unwrap().sheet_specs();
    assert!(stored.values().any(|s| (s.width - n0te_core::format::mm_to_pt(200.0)).abs() < 0.01));
    // The notebook gets a second page, then "für alle" gives it this size and paper too.
    h.message(Message::NewTextPage);
    h.message(Message::CloseTextPage);
    // Page 1 is short now: zoom in on it, so it is the page in the middle of the view.
    let o = open(&h);
    let r = o.pages.pages[o.pages.mains()[0]].placed.rect;
    let cam = n0te_desktop::pages::Camera { x: r.x0, y: r.y0, zoom: 4.0, ..o.camera };
    h.message(Message::Camera(cam));
    assert_eq!(n0te_core::format::describe(spec(&h).width, spec(&h).height), "200 × 100 mm");
    h.message(Message::PagePaper(n0te_core::notebook::Background::Dots));
    h.message(Message::PageToAll);
    let o = open(&h);
    assert!(o.pages.pages.iter().all(|p| (p.placed.spec.width - n0te_core::format::mm_to_pt(200.0)).abs() < 0.01), "every page took the size");
    // The text page kept being a text page; the other page took the dots.
    let kinds: Vec<_> = o.pages.pages.iter().map(|p| p.placed.spec.background).collect();
    assert!(kinds.contains(&n0te_core::notebook::Background::Text) && kinds.contains(&n0te_core::notebook::Background::Dots), "{kinds:?}");
}

#[test]
fn turning_a_page_warns_when_content_would_be_cut_off() {
    let (mut h, root, nb) = blank_notebook("cutoff");
    // A stroke low on the portrait page: it would stick out of a landscape one.
    let lib = Library::open(&root).unwrap();
    let book = lib.load_notebook(&nb).unwrap();
    let sheet_id = book.sheet_order()[0].clone();
    let sheet = lib.load_sheet(&nb, &sheet_id).unwrap();
    let p = |x: f32, y: f32| n0te_core::ink::InkPoint { x, y, pressure: 0.5, t_ms: 0 };
    sheet.add_stroke(n0te_core::ink::Brush { width: 2.0, thinning: 0.0, color: 0xFF000000, kind: 0 }, &[p(100.0, 700.0), p(300.0, 760.0)]).unwrap();
    lib.save_sheet(&nb, &sheet_id, &sheet).unwrap();
    let spec = |h: &Harness| h.app.page_in_view().unwrap().1;
    h.message(Message::RotatePage);
    let w = h.app.size_warning.clone().expect("a warning instead of turning");
    assert_eq!((w.what.as_str(), w.pages), ("1 Strich", 1));
    assert_eq!(spec(&h).width, 595.0, "not turned yet");
    h.events(&[]);
    // Leave it: nothing changes. Then turn anyway.
    h.message(Message::ConfirmSize(false));
    assert!(h.app.size_warning.is_none());
    assert_eq!(spec(&h).width, 595.0);
    h.message(Message::RotatePage);
    h.message(Message::ConfirmSize(true));
    assert_eq!((spec(&h).width, spec(&h).height), (842.0, 595.0));
    // Turning back needs no warning: everything fits again.
    h.message(Message::RotatePage);
    assert!(h.app.size_warning.is_none());
    assert_eq!(spec(&h).width, 595.0);
}

// --- search and headings -------------------------------------------------------------------

/// A stroke from (x, y), `w` wide: something the tablet could have read.
fn scribble(sheet: &n0te_core::sheet::Sheet, x: f32, y: f32, w: f32) -> String {
    let pts: Vec<_> = (0..20).map(|i| n0te_core::ink::InkPoint { x: x + i as f32 * w / 19.0, y: y + (i % 3) as f32 * 4.0, pressure: 0.5, t_ms: i * 8 }).collect();
    sheet.add_stroke(n0te_core::ink::Brush { width: 2.0, thinning: 0.0, color: 0xFF111114, kind: 0 }, &pts).unwrap()
}

/// A line of handwriting as the tablet's recogniser read it.
fn read_line(key: &str, text: &str, alts: &[&str], strokes: &[String]) -> n0te_core::sheet::IndexLine {
    n0te_core::sheet::IndexLine {
        key: key.into(),
        text: text.into(),
        alts: alts.iter().map(|a| a.to_string()).collect(),
        strokes: strokes.to_vec(),
        x0: 0.0,
        y0: 0.0,
        x1: 0.0,
        y1: 0.0,
    }
}

struct Searchable {
    h: Harness,
    root: std::path::PathBuf,
    nb: String,
    /// Page 1, the text page beside it, page 2, page 3 (in a folded group).
    sheets: [String; 4],
    /// The headings on page 1 and on page 2.
    headings: [String; 2],
}

/// A notebook to search, opened: page 1 (untitled) with a heading and a line of handwriting
/// the tablet read, a text page beside it; page 2 "Übungen" with a heading and a text box;
/// page 3, in a folded group, with a line read. All made through the core, like the tablet.
fn searchable(tag: &str) -> Searchable {
    use n0te_core::notebook::{Background, SheetSpec, Side};
    let root = temp_root(tag);
    let lib = Library::open(&root).unwrap();
    let nb = lib.create_notebook("Analysis").unwrap();
    let (n1, s1) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Lines)).unwrap();
    let (_, side) = nb.add_side_sheet(&n1, Side::Right, 0, &SheetSpec::a4(Background::Text)).unwrap();
    let (n2, s2) = nb.add_sheet(None, 1, &SheetSpec::a4(Background::Grid)).unwrap();
    nb.set_title_of(&n2, "Übungen").unwrap();
    let g = nb.add_group(None, 2, "Woche 2").unwrap();
    let (_, s3) = nb.add_sheet(Some(&g), 0, &SheetSpec::a4(Background::Lines)).unwrap();
    nb.set_collapsed(&g, true).unwrap();
    lib.save_notebook(&nb).unwrap();
    let id = nb.id();
    let p1 = lib.load_sheet(&id, &s1).unwrap();
    let a = scribble(&p1, 80.0, 120.0, 220.0);
    let b = scribble(&p1, 80.0, 300.0, 320.0);
    let satz = p1.add_heading("Satz 3: Grenzwerte", &[a]).unwrap();
    p1.set_index(&[read_line("l1", "Der Grenzwert der Folge ist eins", &["Der Grenzwerf der Falge ist eins"], &[b])], &[]).unwrap();
    lib.save_sheet(&id, &s1, &p1).unwrap();
    let notes = lib.load_sheet(&id, &side).unwrap();
    notes.set_page_text("Notiz zum Grenzwert").unwrap();
    lib.save_sheet(&id, &side, &notes).unwrap();
    let p2 = lib.load_sheet(&id, &s2).unwrap();
    let c = scribble(&p2, 80.0, 90.0, 150.0);
    let aufgabe = p2.add_heading("Aufgabe 1", &[c]).unwrap();
    p2.add_text_box(56.0, 400.0, 300.0, 12.0, "Übung: Grenzwert berechnen").unwrap();
    lib.save_sheet(&id, &s2, &p2).unwrap();
    let p3 = lib.load_sheet(&id, &s3).unwrap();
    let d = scribble(&p3, 80.0, 500.0, 300.0);
    p3.set_index(&[read_line("l1", "Grenzwerte im Unendlichen", &[], &[d])], &[]).unwrap();
    lib.save_sheet(&id, &s3, &p3).unwrap();
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(1600.0, 900.0), 1.0).unwrap();
    h.message(Message::Open(id.clone()));
    h.events(&[]);
    h.events(&[]);
    Searchable { h, root, nb: id, sheets: [s1, side, s2, s3], headings: [satz, aufgabe] }
}

/// Ctrl + a key as the keyboard subscription delivers it (the harness doesn't run it).
fn ctrl_key(h: &Harness, c: &str) -> Message {
    match h.key_event(iced::keyboard::Key::Character(c.into()), None)[0].clone() {
        iced::Event::Keyboard(iced::keyboard::Event::KeyPressed { key, modified_key, physical_key, location, text, repeat, .. }) => {
            Message::Key(iced::keyboard::Event::KeyPressed { key, modified_key, physical_key, location, modifiers: iced::keyboard::Modifiers::CTRL, text, repeat })
        }
        _ => unreachable!(),
    }
}

#[test]
fn ctrl_f_searches_the_whole_notebook_and_lists_the_hits_by_page() {
    use n0te_core::search::HitKind;
    use n0te_desktop::sidebar;
    let mut s = searchable("search");
    let h = &mut s.h;
    let m = ctrl_key(h, "f");
    h.message(m);
    let field = h.bounds_of(sidebar::input_id()).expect("a search field in the sidebar");
    assert_eq!(h.find(iced_test::selector::is_focused()), Some(field), "Ctrl+F focuses it");
    h.type_text("Grenzwert");
    let (query, pages) = open(&h).search.found.clone().expect("searched");
    assert_eq!(query, "Grenzwert");
    // Page by page in outline order (side sheets after their page, folded groups too), each
    // page's hits best first, then top to bottom.
    let summary: Vec<(u32, i32, String, Vec<HitKind>)> = pages.iter().map(|p| (p.number, p.side, p.name.clone(), p.hits.iter().map(|h| h.kind).collect())).collect();
    assert_eq!(
        summary,
        vec![
            (1, 0, "Satz 3: Grenzwerte".to_string(), vec![HitKind::Heading, HitKind::Ink]),
            (1, 1, "Notiz zum Grenzwert".to_string(), vec![HitKind::Text]),
            (2, 0, "Übungen".to_string(), vec![HitKind::Text]),
            (3, 0, "Seite".to_string(), vec![HitKind::Ink]),
        ]
    );
    let ink = &pages[0].hits[1];
    assert_eq!(ink.text, "Der Grenzwert der Folge ist eins");
    assert!(ink.x0 < 82.0 && ink.y0 > 290.0 && ink.y1 < 320.0, "where the strokes are: {ink:?}");
    // The sidebar shows them instead of the outline: a row per hit under its page's name.
    assert!(h.bounds_of(sidebar::hit_id(4)).is_some() && h.bounds_of(sidebar::hit_id(5)).is_none());
    assert!(h.find("Übungen").is_some());
    assert!(h.bounds_of(sidebar::heading_id(&s.headings[1])).is_none(), "no outline while searching");
    h.frame();
    // The recogniser's second guess counts too ("Falge" for "Folge" is read as it is).
    h.message(Message::SearchQuery("falge".into()));
    assert_eq!(open(&h).search.hits().map(|(p, _)| p.number).collect::<Vec<_>>(), vec![1]);
    h.message(Message::SearchQuery("Integral".into()));
    assert_eq!(open(&h).search.found.as_ref().map(|(_, p)| p.len()), Some(0), "nothing found");
    h.frame();
    // Escape in the field empties it: the outline again.
    h.press(Named::Escape);
    assert!(!open(&h).search.active() && open(&h).search.found.is_none());
    assert!(h.bounds_of(sidebar::heading_id(&s.headings[1])).is_some());
}

#[test]
fn text_typed_but_not_saved_yet_is_found_too() {
    use n0te_core::search::HitKind;
    let (mut h, root, nb) = blank_notebook("unsaved");
    let sheet = open(&h).pages.pages[0].placed.sheet_id.clone();
    h.message(Message::NewText(sheet.clone(), 60.0, 200.0));
    h.type_text("Konvergenzradius");
    assert!(h.app.editing.as_ref().is_some_and(|e| e.dirty), "typed, not saved yet");
    assert!(stored_texts(&root, &nb).iter().all(|t| t.text.is_empty()));
    h.message(Message::SearchQuery("konvergenzradius".into()));
    let hits: Vec<(String, HitKind, String)> = open(&h).search.hits().map(|(p, h)| (p.sheet_id.clone(), h.kind, h.text.clone())).collect();
    assert_eq!(hits, vec![(sheet, HitKind::Text, "Konvergenzradius".to_string())]);
}

#[test]
fn clicking_a_hit_shows_it_in_the_middle_and_lights_it_up() {
    use n0te_desktop::sidebar;
    let mut s = searchable("reveal");
    let h = &mut s.h;
    let zoom = open(h).camera.zoom;
    h.message(Message::SearchQuery("unendlichen".into()));
    let hit = open(&h).search.hits().next().expect("found").1.clone();
    assert_eq!(open(&h).search.hits().count(), 1);
    assert!(open(h).pages.page_of_sheet(&s.sheets[3]).is_none(), "its group is folded");
    let row = h.bounds_of(sidebar::hit_id(0)).expect("its row");
    h.click(row.center());
    let o = open(h);
    let page = o.pages.pages[o.pages.page_of_sheet(&s.sheets[3]).expect("the group opened for it")].placed.rect;
    let c = o.camera;
    let middle = (c.x + c.viewport.width / c.zoom / 2.0, c.y + c.viewport.height / c.zoom / 2.0);
    let spot = (page.x0 + (hit.x0 + hit.x1) / 2.0, page.y0 + (hit.y0 + hit.y1) / 2.0);
    assert!((middle.0 - spot.0).abs() < 0.5 && (middle.1 - spot.1).abs() < 0.5, "{middle:?} vs {spot:?}");
    assert_eq!(c.zoom, zoom, "the zoom stays");
    let flash = o.pages.flash.clone().expect("it lights up");
    assert_eq!((flash.sheet_id.as_str(), flash.rect.y0), (s.sheets[3].as_str(), hit.y0));
    // In the accent (a warm red in the designs the tests use), around the spot.
    let pad = (4.0 * c.zoom).max(3.0);
    let x = (c.origin.x + (page.x0 + hit.x0 - c.x) * c.zoom - pad).ceil() as usize;
    let y = (c.origin.y + (spot.1 - c.y) * c.zoom) as usize;
    let (rgba, _) = h.frame_rgba();
    let px = &rgba[(y * 1600 + x) * 4..][..4];
    assert!(px[0] as i32 > px[1] as i32 + 50 && px[0] as i32 > px[2] as i32 + 50, "accent frame at ({x}, {y}): {px:?}");
    // It fades: gone after 1.5 s (the frame clock clears it).
    if let Screen::Notebook(o) = &mut h.app.screen {
        o.pages.flash.as_mut().unwrap().at -= std::time::Duration::from_secs(2);
    }
    h.message(Message::Glide(std::time::Instant::now()));
    assert!(open(h).pages.flash.is_none());
    // Enter in the field shows the first hit.
    h.message(Message::Camera(n0te_desktop::pages::Camera { y: 0.0, ..open(h).camera }));
    h.message(Message::SearchSubmit);
    assert_eq!(open(h).pages.flash.as_ref().map(|f| f.sheet_id.clone()), Some(s.sheets[3].clone()));
}

#[test]
fn headings_show_under_their_page_name_untitled_pages_and_follow_the_tablet() {
    use n0te_desktop::live::LiveEvent;
    use n0te_desktop::sidebar;
    let mut s = searchable("headings");
    let h = &mut s.h;
    assert_eq!(h.app.headings.get(&s.sheets[0]).map(Vec::len), Some(1), "read in the background on opening");
    // Page 2 has a title: its heading is listed under it, indented.
    let title = h.find("Übungen").expect("page 2 in the outline");
    let row = h.bounds_of(sidebar::heading_id(&s.headings[1])).expect("its heading under it");
    assert!(row.y > title.y && h.find("Aufgabe 1").is_some_and(|t| t.x > title.x), "below and indented");
    // Page 1 has no title: its first heading names it (and isn't listed again).
    assert!(h.find("Satz 3: Grenzwerte").is_some());
    assert!(h.bounds_of(sidebar::heading_id(&s.headings[0])).is_none());
    // A click shows the heading on its page, lit up.
    h.click(row.center());
    let o = open(h);
    let flash = o.pages.flash.clone().expect("lit up");
    assert_eq!(flash.sheet_id, s.sheets[2]);
    let page = o.pages.pages[o.pages.page_of_sheet(&s.sheets[2]).unwrap()].placed.rect;
    let c = o.camera;
    assert!((c.y + c.viewport.height / c.zoom / 2.0 - (page.y0 + (flash.rect.y0 + flash.rect.y1) / 2.0)).abs() < 0.5);
    // The tablet marks another heading: it shows once the sheet comes in.
    {
        let lib = Library::open(&s.root).unwrap();
        let p2 = lib.load_sheet(&s.nb, &s.sheets[2]).unwrap();
        let e = scribble(&p2, 80.0, 600.0, 120.0);
        p2.add_heading("Lemma 4", &[e]).unwrap();
        lib.save_sheet(&s.nb, &s.sheets[2], &p2).unwrap();
    }
    h.message(Message::Live(LiveEvent::Changed { notebook: s.nb.clone(), index: false, sheets: vec![s.sheets[2].clone()] }));
    assert_eq!(h.app.headings[&s.sheets[2]].iter().map(|h| h.text.as_str()).collect::<Vec<_>>(), vec!["Aufgabe 1", "Lemma 4"]);
    assert!(h.find("Lemma 4").is_some());
    h.frame();
}

#[test]
fn pdf_text_is_found_where_it_lies_on_the_page() {
    use n0te_core::search::HitKind;
    let root = temp_root("pdfsearch");
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(1600.0, 900.0), 1.0).unwrap();
    let pdf = h.app.pdf.clone().expect("pdfium (third_party/pdfium) for the PDF tests");
    let stored = n0te_pdf::notebook::store_pdf(&h.app.lib, &pdf.worker, two_page_pdf(&h.app)).unwrap();
    let nb = h.app.lib.create_notebook("Folien").unwrap();
    let sheets = n0te_pdf::notebook::add_pdf_sheets(&nb, &stored, None, 0).unwrap();
    // 100 pt of space inserted near the top of the second page: its text moved down.
    nb.reshape(&sheets[1].node, 942.0, &[(20.0, 100.0)]).unwrap();
    h.app.lib.save_notebook(&nb).unwrap();
    h.message(Message::Open(nb.id()));
    h.events(&[]);
    h.message(Message::SearchQuery("folie".into()));
    let found = open(&h).search.found.clone().expect("searched").1;
    assert_eq!(found.iter().map(|p| (p.number, p.name.as_str())).collect::<Vec<_>>(), vec![(1, "PDF-Seite 1"), (2, "PDF-Seite 2")]);
    let (a, b) = (&found[0].hits[0], &found[1].hits[0]);
    assert_eq!((a.kind, a.text.as_str(), b.kind, b.text.as_str()), (HitKind::Pdf, "Folie 0", HitKind::Pdf, "Folie 1"));
    assert!(a.y0 > 50.0 && a.y1 < 110.0, "near the top: {a:?}");
    assert!((b.y0 - a.y0 - 100.0).abs() < 0.5, "moved down with the PDF: {a:?} {b:?}");
}

// --- coordinate systems ----------------------------------------------------------------------

/// The green of the test plots' curve.
const CURVE: u32 = 0xFF00_B000;

fn is_curve(p: &[u8]) -> bool {
    p[0] < 70 && p[1] > 130 && p[2] < 70
}

/// f(x) = 2 in thick green, ruler-straight, in a 240 × 160 pt box at (100, 200) with 20 pt to
/// the unit: the curve runs along page y 240 from x 100 to 340.
fn green_plot() -> n0te_core::plot::Plot {
    let mut p = n0te_core::plot::Plot::centered(100.0, 200.0, 240.0, 160.0, 20.0, n0te_desktop::plots::AXES, 4.0);
    p.clean = true;
    let mut p = p.add("f(x) = 2").unwrap();
    p.objs[0].style.color = CURVE;
    p
}

/// A library with one page of `paper` (and a green plot on it), opened at zoom 1 with the
/// page's corner 20 px into the view: (harness, root, notebook, sheet).
fn plot_page(tag: &str, paper: n0te_core::notebook::Background, with_plot: bool) -> (Harness, std::path::PathBuf, String, String) {
    plot_page_on("tiny-skia", tag, paper, with_plot).expect("tiny-skia renderer")
}

/// [`plot_page`] on a renderer (None: not available here).
fn plot_page_on(backend: &str, tag: &str, paper: n0te_core::notebook::Background, with_plot: bool) -> Option<(Harness, std::path::PathBuf, String, String)> {
    let root = temp_root(tag);
    let lib = Library::open(&root).unwrap();
    let nb = lib.create_notebook("Funktionen").unwrap();
    let spec = n0te_core::notebook::SheetSpec { spacing: 17.0, ..n0te_core::notebook::SheetSpec::a4(paper) };
    let (_, sheet_id) = nb.add_sheet(None, 0, &spec).unwrap();
    lib.save_notebook(&nb).unwrap();
    if with_plot {
        let sheet = lib.load_sheet(&nb.id(), &sheet_id).unwrap();
        sheet.add_plot(&green_plot()).unwrap();
        lib.save_sheet(&nb.id(), &sheet_id, &sheet).unwrap();
    }
    let mut h = Harness::new(App::new(&root), backend, Size::new(1600.0, 900.0), 1.0)?;
    h.message(Message::Open(nb.id()));
    h.events(&[]);
    let o = open(&h);
    let rect = o.pages.pages[0].rect();
    let cam = n0te_desktop::pages::Camera { x: rect.x0 - 20.0, y: rect.y0 - 20.0, zoom: 1.0, ..o.camera };
    h.message(Message::Camera(cam));
    h.events(&[]);
    Some((h, root, nb.id(), sheet_id))
}

/// Where a sheet point is on the screen (the camera of [`plot_page`]).
fn screen_at(h: &Harness, x: f32, y: f32) -> Point {
    let c = open(h).camera;
    Point::new(c.origin.x + 20.0 + x, c.origin.y + 20.0 + y)
}

fn stored_plots(root: &std::path::Path, nb: &str, sheet: &str) -> Vec<n0te_core::plot::Plot> {
    Library::open(root).unwrap().load_sheet(nb, sheet).unwrap().plots()
}

/// The definitions of a plot's objects.
fn defs(p: &n0te_core::plot::Plot) -> Vec<String> {
    p.objs.iter().map(|o| o.def.clone()).collect()
}

/// The open panel.
fn panel(h: &Harness) -> &n0te_desktop::plots::Editor {
    h.app.plot_edit.as_ref().expect("the panel is open")
}

/// Curve-green pixels of a frame: (in the band along page y 236..244, x 96..344; elsewhere).
fn green_pixels(h: &mut Harness) -> (usize, usize) {
    let (rgba, _) = h.frame_rgba();
    let (a, b) = (screen_at(h, 96.0, 235.0), screen_at(h, 344.0, 245.0));
    let (mut on, mut off) = (0, 0);
    for (i, p) in rgba.chunks_exact(4).enumerate() {
        if !is_curve(p) {
            continue;
        }
        let (x, y) = ((i % 1600) as f32, (i / 1600) as f32);
        if x >= a.x && x <= b.x && y >= a.y && y <= b.y {
            on += 1;
        } else {
            off += 1;
        }
    }
    (on, off)
}

#[test]
fn a_plot_on_a_page_is_drawn_also_when_it_comes_from_the_tablet() {
    // Ink (strokes and plots) is drawn as meshes, which only the GPU renderer draws.
    let gpu = plot_page_on("wgpu", "plotdraw", n0te_core::notebook::Background::Blank, false);
    let pixels = gpu.is_some();
    let (mut h, root, nb, sheet) = gpu.unwrap_or_else(|| plot_page("plotdraw", n0te_core::notebook::Background::Blank, false));
    if pixels {
        assert_eq!(green_pixels(&mut h).0, 0);
    }
    // The tablet adds a plot: the change comes in through the sync service.
    {
        let lib = Library::open(&root).unwrap();
        let s = lib.load_sheet(&nb, &sheet).unwrap();
        s.add_plot(&green_plot()).unwrap();
        lib.save_sheet(&nb, &sheet, &s).unwrap();
    }
    h.message(Message::Live(n0te_desktop::live::LiveEvent::Changed { notebook: nb.clone(), index: false, sheets: vec![sheet.clone()] }));
    let plots = open(&h).pages.plots_of(&sheet);
    assert_eq!(plots.len(), 1, "the page has it");
    if !pixels {
        // No GPU here: what would be drawn — the curve's ink lies along y = 2.
        use iced::advanced::graphics::mesh::Mesh;
        let green = iced::advanced::graphics::color::pack(n0te_desktop::pages::argb(CURVE));
        let meshes = n0te_desktop::plots::meshes(&plots, iced::Rectangle::new(Point::ORIGIN, Size::new(595.0, 842.0))).expect("plot ink");
        let at: Vec<[f32; 2]> = meshes
            .iter()
            .flat_map(|m| match m {
                Mesh::Solid { buffers, .. } => buffers.vertices.iter().filter(|v| v.color == green).map(|v| v.position).collect(),
                _ => Vec::new(),
            })
            .collect();
        assert!(at.len() > 20 && at.iter().all(|p| (p[1] - 240.0).abs() < 3.0 && p[0] > 97.0 && p[0] < 343.0), "{at:?}");
        eprintln!("no wgpu: checked the plot's mesh instead of pixels");
        return;
    }
    let (on, off) = green_pixels(&mut h);
    // 240 pt of a 4 pt line at zoom 1.
    assert!(on > 600, "the curve is drawn along y = 2: {on} green pixels there");
    assert!(off < 20, "and nowhere else: {off}");
    // Its axes in their near-black: the x axis along page y 280.
    let (rgba, _) = h.frame_rgba();
    let at = screen_at(&h, 160.0, 280.0);
    let dark = (-3..=3).any(|d| {
        let i = (((at.y as i32 + d) * 1600) + at.x as i32) as usize * 4;
        rgba[i] < 90 && rgba[i + 1] < 90 && rgba[i + 2] < 90
    });
    assert!(dark, "the x axis is drawn");
}

#[test]
fn clicking_a_plot_opens_its_panel_and_the_input_line_adds_named_objects() {
    use iced::keyboard::key::Named;
    let (mut h, root, nb, sheet) = plot_page("plotclick", n0te_core::notebook::Background::Lines, true);
    let id = stored_plots(&root, &nb, &sheet)[0].id.clone();
    // A click into the box (not a double click: no text box either).
    let at = screen_at(&h, 220.0, 300.0);
    h.click(at);
    let e = panel(&h);
    assert_eq!((e.sheet_id.as_str(), e.plot.id.as_str()), (sheet.as_str(), id.as_str()));
    assert!(h.app.editing.is_none());
    assert!(h.find("Koordinatensystem").is_some() || h.find("KOORDINATENSYSTEM").is_some() || h.find("koordinatensystem").is_some());
    // The algebra view lists the function; the preview draws it (as paths: the software
    // renderer draws those, not the page's ink meshes, so the green is the preview's).
    assert!(h.find("f(x) = 2").is_some(), "its row");
    let green = h.frame_rgba().0.chunks_exact(4).filter(|p| is_curve(p)).count();
    assert!(green > 300, "the preview shows the curve: {green} green pixels");
    // Selected, it is drawn bolder there (only there: the plot stays as it is).
    let f = panel(&h).plot.objs[0].id.clone();
    h.message(Message::Plot(n0te_desktop::plots::Msg::Pick(f)));
    let bold = h.frame_rgba().0.chunks_exact(4).filter(|p| is_curve(p)).count();
    assert!(bold as f32 > green as f32 * 1.6, "the selected curve stands out: {green} -> {bold} green pixels");
    assert_eq!(panel(&h).plot, stored_plots(&root, &nb, &sheet)[0]);
    h.message(Message::Plot(n0te_desktop::plots::Msg::Tab(n0te_desktop::plots::Tab::Graphics)));
    h.task(n0te_desktop::plots::focus(n0te_desktop::plots::input_id(), None));
    // The cursor is in the input line: a point, a function, where they meet.
    assert_eq!(h.find(iced_test::selector::is_focused()), h.bounds_of(n0te_desktop::plots::input_id()));
    for s in ["A = (1|2)", "x^2", "Schneide(f, g)"] {
        h.type_text(s);
        h.press(Named::Enter);
    }
    let e = panel(&h);
    assert_eq!(defs(&e.plot), vec!["f(x) = 2", "A = (1|2)", "g = x^2", "B = Schneide(f, g, 1)", "C = Schneide(f, g, 2)"], "two intersections: two points");
    assert_eq!(e.input, "", "the line is empty again");
    assert_eq!(h.find(iced_test::selector::is_focused()), h.bounds_of(n0te_desktop::plots::input_id()), "and keeps the cursor");
    h.frame();
    for row in ["A = (1 | 2)", "g(x) = x²", "B = (−1,41 | 2)", "C = (1,41 | 2)"] {
        assert!(h.find(row).is_some(), "{row} is listed");
    }
    // Under a point's value, how it came about; not under what its value says.
    assert!(h.find("Schneide(f, g, 1)").is_some() && h.find("Schneide(f, g, 2)").is_some());
    assert!(h.find("A = (1|2)").is_none() && h.find("g = x^2").is_none(), "no definitions as typed");
    let ok = h.find("OK").expect("the OK button");
    h.click(ok.center());
    assert!(h.app.plot_edit.is_none(), "OK closes the panel");
    let p = &stored_plots(&root, &nb, &sheet)[0];
    assert_eq!(defs(p), vec!["f(x) = 2", "A = (1|2)", "g = x^2", "B = Schneide(f, g, 1)", "C = Schneide(f, g, 2)"]);
    assert_eq!((p.x, p.y, p.w, p.h), (100.0, 200.0, 240.0, 160.0), "the box stays");
    assert_eq!(open(&h).pages.plots_of(&sheet)[0].objs.len(), 5, "the page shows them");
}

#[test]
fn a_mistake_is_said_rows_edit_and_esc_and_loeschen_work() {
    use iced::keyboard::key::Named;
    use n0te_desktop::plots::{Msg, Tab};
    let (mut h, root, nb, sheet) = plot_page("plotbad", n0te_core::notebook::Background::Blank, true);
    let before = stored_plots(&root, &nb, &sheet)[0].clone();
    h.message(Message::EditPlot(sheet.clone(), before.id.clone()));
    h.type_text("Schneide(f)");
    h.press(Named::Enter);
    assert!(h.find("So: Schneide(f, g)").is_some(), "the mistake shows under the line");
    assert_eq!(panel(&h).plot.objs.len(), 1);
    // OK waits while the line has a mistake.
    let ok = h.find("OK").unwrap();
    h.click(ok.center());
    assert!(h.app.plot_edit.is_some(), "OK doesn't close it");
    assert_eq!(stored_plots(&root, &nb, &sheet)[0], before, "nothing stored");
    h.message(Message::Plot(Msg::Input(String::new())));
    assert!(h.find("×").is_none(), "no × while nothing is selected");
    // A click on a row: its definition to edit, the cursor in it; Enter takes it.
    let row = h.find("f(x) = 2").expect("the row");
    h.click(row.center());
    assert!(h.find("×").is_some(), "the selected row can be deleted");
    let e = panel(&h);
    assert_eq!(e.editing.as_ref().map(|r| r.text.as_str()), Some("f(x) = 2"));
    assert_eq!(h.find(iced_test::selector::is_focused()), h.bounds_of(n0te_desktop::plots::edit_id()));
    h.message(Message::Plot(Msg::EditText("f(x) = x - 1".into())));
    h.press(Named::Enter);
    assert!(panel(&h).editing.is_none());
    assert!(h.find("f(x) = x − 1").is_some(), "listed as GeoGebra lists it");
    // A range that isn't one: said on the Grafik side, OK waits.
    h.message(Message::Plot(Msg::Tab(Tab::Graphics)));
    h.message(Message::Plot(Msg::Range(1, "-7".into())));
    assert!(h.find("x bis muss größer sein als x von").is_some());
    h.message(Message::Plot(Msg::Ok));
    assert!(h.app.plot_edit.is_some());
    // Fixed (German comma): stored in one go.
    h.message(Message::Plot(Msg::Range(1, "7,5".into())));
    h.message(Message::Plot(Msg::Grid(true)));
    h.message(Message::Plot(Msg::Ok));
    assert!(h.app.plot_edit.is_none());
    let p = stored_plots(&root, &nb, &sheet)[0].clone();
    assert_eq!((p.objs[0].def.as_str(), p.x_max, p.grid), ("f(x) = x - 1", 7.5, true));
    assert_eq!(n0te_core::plot::Plot { objs: before.objs.clone(), x_max: before.x_max, grid: before.grid, ..p.clone() }, before, "nothing else changed");
    // Esc from the input line cancels.
    h.message(Message::EditPlot(sheet.clone(), p.id.clone()));
    h.type_text("x^3");
    h.press(Named::Enter);
    assert_eq!(panel(&h).plot.objs.len(), 2);
    h.press(Named::Escape);
    assert!(h.app.plot_edit.is_none(), "Esc closed it");
    assert_eq!(stored_plots(&root, &nb, &sheet)[0], p, "unchanged");
    // Esc in a row leaves the row only.
    h.message(Message::EditPlot(sheet.clone(), p.id.clone()));
    let row = h.find("f(x) = x − 1").unwrap();
    h.click(row.center());
    h.message(Message::Plot(Msg::EditText("f(x) = x".into())));
    h.press(Named::Escape);
    assert!(h.app.plot_edit.is_some() && panel(&h).editing.is_none(), "the row is left, the panel stays");
    assert_eq!(panel(&h).plot.objs[0].def, "f(x) = x - 1", "what was typed in it is forgotten");
    // Löschen removes the plot.
    h.message(Message::Plot(Msg::Delete));
    assert!(h.app.plot_edit.is_none() && stored_plots(&root, &nb, &sheet).is_empty());
}

#[test]
fn inserting_from_the_palette_puts_an_empty_construction_on_the_squares() {
    use iced::keyboard::key::Named;
    let (mut h, root, nb, sheet) = plot_page("plotinsert", n0te_core::notebook::Background::Grid, false);
    h.message(Message::OpenPalette);
    h.type_text("koordinaten");
    h.message(Message::PaletteRun(None));
    let plots = stored_plots(&root, &nb, &sheet);
    assert_eq!(plots.len(), 1, "stored at once");
    let p = &plots[0];
    assert_eq!((p.w, p.h), (12.0 * 17.0, 8.0 * 17.0), "12 × 8 squares");
    assert_eq!((p.x % 17.0, p.y % 17.0), (0.0, 0.0), "on the squares: {p:?}");
    assert!(p.x >= 0.0 && p.y >= 0.0 && p.x + p.w <= 595.0 && p.y + p.h <= 842.0);
    assert_eq!((p.x_min, p.x_max, p.y_min, p.y_max), (-6.0, 6.0, -4.0, 4.0), "a square is the unit");
    assert_eq!((p.color, p.width), (0xFF20_2428, 1.2));
    assert!(p.objs.is_empty(), "an empty construction");
    // In the middle of the view, as far as the page goes (the view shows more than the page).
    let (c, page) = (open(&h).camera, open(&h).pages.pages[0].rect());
    let mid = (c.x + c.viewport.width / c.zoom / 2.0 - page.x0, c.y + c.viewport.height / c.zoom / 2.0 - page.y0);
    let want = ((mid.0 - p.w / 2.0).clamp(0.0, 595.0 - p.w), (mid.1 - p.h / 2.0).clamp(0.0, 842.0 - p.h));
    assert!((p.x - want.0).abs() <= 17.0 && (p.y - want.1).abs() <= 17.0, "{p:?} around {mid:?}");
    // Its panel is open with the cursor in the input line.
    assert!(panel(&h).plot.objs.is_empty());
    assert_eq!(h.find(iced_test::selector::is_focused()), h.bounds_of(n0te_desktop::plots::input_id()));
    assert!(h.find("Noch leer. Unten eingeben, z. B. f(x) = x^2 oder A = (1|2), und Enter.").is_some());
    h.type_text("sin(x)");
    h.press(Named::Enter);
    h.press(Named::Enter);
    assert_eq!(defs(&panel(&h).plot), vec!["f = sin(x)"], "Enter on an empty line adds nothing");
    // A command button: the command in the line, the cursor between its brackets.
    let tangent = h.find(n0te_desktop::theme::cased("Tangente").as_str()).expect("the Tangente button");
    h.click(tangent.center());
    assert_eq!(panel(&h).input, "Tangente()");
    assert_eq!(panel(&h).hint, Some(n0te_desktop::plots::QUICK[6].2), "the help line says how it goes");
    h.type_text("0, f");
    h.press(Named::Enter);
    assert_eq!(defs(&panel(&h).plot), vec!["f = sin(x)", "g = Tangente(0, f)"]);
    h.frame();
    assert!(h.find("g: y = x").is_some());
    let ok = h.find("OK").unwrap();
    h.click(ok.center());
    assert!(h.app.plot_edit.is_none());
    assert_eq!(defs(&stored_plots(&root, &nb, &sheet)[0]), vec!["f = sin(x)", "g = Tangente(0, f)"]);
}

#[test]
fn typing_a_command_suggests_it_tab_takes_it_and_esc_hides_it() {
    use iced::keyboard::key::Named;
    let (mut h, root, nb, sheet) = plot_page("plotsuggest", n0te_core::notebook::Background::Blank, true);
    let id = stored_plots(&root, &nb, &sheet)[0].id.clone();
    h.message(Message::EditPlot(sheet.clone(), id));
    let names = |h: &Harness| panel(h).suggestions().iter().map(|s| s.name).collect::<Vec<_>>();
    let input = |h: &mut Harness| h.bounds_of(n0te_desktop::plots::input_id());
    h.type_text("x^2");
    h.press(Named::Enter);
    // Two letters of a command: the commands that fit, under the line in place of the buttons.
    h.type_text("S = Sch");
    assert_eq!(names(&h), vec!["Schneide", "Schieberegler"]);
    assert!(h.find("Schneide").is_some() && h.find("die Schnittpunkte").is_some(), "shown with what they make");
    assert!(h.find(n0te_desktop::theme::cased("Punkt").as_str()).is_none(), "the buttons make room");
    // Tab takes the first: its usage with the example arguments selected, typing replaces them.
    h.press(Named::Tab);
    assert_eq!(panel(&h).input, "S = Schneide(f, g)");
    assert_eq!(h.find(iced_test::selector::is_focused()), input(&mut h), "the cursor stays in the line");
    h.type_text("g, f");
    assert_eq!(panel(&h).input, "S = Schneide(g, f)");
    assert!(names(&h).is_empty());
    h.press(Named::Enter);
    let d = defs(&panel(&h).plot);
    assert_eq!(&d[2..], ["S_1 = Schneide(g, f, 1)", "S_2 = Schneide(g, f, 2)"], "Enter adds it");
    // In the middle of the line: the word at the cursor; a click takes a suggestion.
    h.type_text("Tangente()");
    h.press(Named::ArrowLeft);
    h.type_text("Ab");
    assert_eq!((panel(&h).input.as_str(), names(&h)), ("Tangente(Ab)", vec!["Ableitung", "Abstand"]));
    let derivative = h.find("Ableitung").expect("the suggestion");
    h.click(derivative.center());
    assert_eq!(panel(&h).input, "Tangente(Ableitung(f))");
    h.type_text("g");
    assert_eq!(panel(&h).input, "Tangente(Ableitung(g))", "the cursor is back in the line, f selected");
    // Esc hides them first (the line keeps the cursor and what it says), then closes the panel.
    h.message(Message::Plot(n0te_desktop::plots::Msg::Input(String::new())));
    h.type_text("Abs");
    assert_eq!(names(&h), vec!["Abstand"]);
    h.press(Named::Escape);
    assert!(h.app.plot_edit.is_some() && names(&h).is_empty(), "Esc hides the suggestions");
    assert_eq!(panel(&h).input, "Abs");
    assert_eq!(h.find(iced_test::selector::is_focused()), input(&mut h));
    assert!(h.find(n0te_desktop::theme::cased("Punkt").as_str()).is_some(), "the buttons are back");
    h.press(Named::Escape);
    assert!(h.app.plot_edit.is_none(), "then Esc closes the panel");
    assert_eq!(stored_plots(&root, &nb, &sheet)[0].objs.len(), 1, "nothing stored");
}

#[test]
fn ansicht_zuruecksetzen_brings_the_ranges_back() {
    use n0te_desktop::plots::{Msg, Tab};
    let (mut h, root, nb, sheet) = plot_page("plotreset", n0te_core::notebook::Background::Blank, true);
    let before = stored_plots(&root, &nb, &sheet)[0].clone();
    h.message(Message::EditPlot(sheet.clone(), before.id.clone()));
    h.message(Message::Plot(Msg::Tab(Tab::Graphics)));
    let label = n0te_desktop::theme::cased("Ansicht zurücksetzen");
    assert!(h.find(label.as_str()).is_none(), "nothing to reset yet");
    h.message(Message::Plot(Msg::Range(0, "-20".into())));
    h.message(Message::Plot(Msg::EqualUnits));
    let p = &panel(&h).plot;
    assert!(p.x_min == -20.0 && p.y_max != before.y_max, "{p:?}");
    let reset = h.find(label.as_str()).expect("the button, now that the view changed");
    h.click(reset.center());
    let p = &panel(&h).plot;
    assert_eq!((p.x_min, p.x_max, p.y_min, p.y_max), (before.x_min, before.x_max, before.y_min, before.y_max));
    assert_eq!(panel(&h).ranges.iter().map(|f| f.text.as_str()).collect::<Vec<_>>(), vec!["-6", "6", "-4", "4"]);
    assert!(h.find(label.as_str()).is_none(), "and goes");
    h.message(Message::Plot(Msg::Ok));
    assert!(h.app.plot_edit.is_none());
    assert_eq!(stored_plots(&root, &nb, &sheet)[0], before, "nothing changed");
}

#[test]
fn the_pdf_export_has_the_plot_as_ink() {
    let (mut h, root, _nb, _sheet) = plot_page("plotpdf", n0te_core::notebook::Background::Blank, true);
    let pdf = h.app.pdf.clone().expect("pdfium (third_party/pdfium) for the PDF tests");
    h.app.export_dir = root.join("out");
    h.message(Message::Export(n0te_desktop::app::ExportWhat::Current));
    let bytes = std::fs::read(h.app.last_export.clone().expect("exported")).unwrap();
    // The page rendered 1 px per point: the curve along y 240, from x 100 to 340.
    let page = pdf.worker.call(10, move |docs| docs.engine().open(bytes).and_then(|d| d.render_page(0, 595))).unwrap().unwrap();
    let px = |x: u32, y: u32| {
        let i = ((y * page.width + x) * 4) as usize;
        &page.data[i..i + 4]
    };
    let band = (120..320).flat_map(|x| (236..245).map(move |y| (x, y))).filter(|&(x, y)| is_curve(px(x, y))).count();
    assert!(band > 400, "the curve is in the PDF: {band} green pixels");
    assert!(!is_curve(px(200, 300)) && !is_curve(px(450, 240)));
}

/// Stores `p` on the sheet as the tablet would and lets the app know.
fn store_plot_from_tablet(h: &mut Harness, root: &std::path::Path, nb: &str, sheet: &str, p: &n0te_core::plot::Plot) {
    let lib = Library::open(root).unwrap();
    let s = lib.load_sheet(nb, sheet).unwrap();
    s.set_plot(p).unwrap();
    lib.save_sheet(nb, sheet, &s).unwrap();
    h.message(Message::Live(n0te_desktop::live::LiveEvent::Changed { notebook: nb.to_string(), index: false, sheets: vec![sheet.to_string()] }));
}

/// A construction as the tablet makes it (with the core's `add`): a slider, a function with a
/// point on it and its tangent there, a triangle with its circumcircle, styles set by hand.
fn tablet_construction(mut p: n0te_core::plot::Plot) -> n0te_core::plot::Plot {
    use n0te_core::plot::{LabelMode, LineStyle, MARK_EXTREMA};
    p.objs.clear();
    for s in ["a = Schieberegler(-1, 2, 0.1)", "f(x) = 0.5x^2 - 2", "T = Punkt(f, a)", "Tangente(T, f)", "A = (-2, -1)", "B = (3, -1)", "C = (0.5, 2.5)", "Vieleck(A, B, C)", "Kreis(A, B, C)", "M = Mittelpunkt(c)"] {
        p = p.add(s).unwrap();
    }
    let pi = std::f64::consts::PI;
    (p.x_min, p.x_max) = (-pi, 1.5 * pi);
    p.pi = true;
    p.x_name = "t".into();
    p.objs[1].style.special = MARK_EXTREMA;
    p.objs[1].style.coords = true;
    p.objs[3].style.line = LineStyle::Dashed;
    p.objs[8].style.label = LabelMode::Caption;
    p.objs[8].style.caption = "Umkreis".into();
    p.objs[9].style.aux = true;
    p
}

#[test]
fn ok_alone_keeps_a_construction_from_the_tablet_and_what_the_panel_changes_is_stored() {
    use n0te_core::plot::PALETTE;
    use n0te_desktop::plots::Msg;
    let (mut h, root, nb, sheet) = plot_page("plotkeep", n0te_core::notebook::Background::Blank, true);
    let p = tablet_construction(stored_plots(&root, &nb, &sheet)[0].clone());
    store_plot_from_tablet(&mut h, &root, &nb, &sheet, &p);
    assert_eq!(stored_plots(&root, &nb, &sheet)[0], p);
    // Opened (every row drawn, helpers last) and OK: the same plot.
    h.message(Message::EditPlot(sheet.clone(), p.id.clone()));
    h.frame();
    for row in ["a = 1", "f(x) = 0,5x² − 2", "T = (1 | −1,5)", "g: y = x − 2,5", "vieleck1 = 8,75", "c: (x − 0,5)² + (y + 0,14)² = 6,98", "M = (0,5 | −0,14)"] {
        assert!(h.find(row).is_some(), "{row} is listed");
    }
    assert!(h.find(n0te_desktop::theme::cased("Hilfsobjekte").as_str()).is_some());
    let ok = h.find("OK").expect("the OK button");
    h.click(ok.center());
    assert!(h.app.plot_edit.is_none());
    assert_eq!(stored_plots(&root, &nb, &sheet)[0], p, "nothing is lost");
    // The slider dragged, f hidden by its dot, the tangent recoloured: stored in one go.
    h.message(Message::EditPlot(sheet.clone(), p.id.clone()));
    h.frame();
    let (a, f, g) = (p.objs[0].id.clone(), p.objs[1].id.clone(), p.objs[3].id.clone());
    let track = h.bounds_of(n0te_desktop::plots::slider_id(&a)).expect("a's slider");
    h.click(Point::new(track.x + track.width * 0.9, track.center_y()));
    let now = panel(&h).plot.objs[0].def.clone();
    assert!(now != "a = 1" && now.trim_start_matches("a = ").parse::<f64>().unwrap() > 1.5, "dragged to the right: {now}");
    let dot = h.bounds_of(n0te_desktop::plots::dot_id(&f)).expect("f's dot");
    h.click(dot.center());
    assert!(!panel(&h).plot.objs[1].style.visible, "hidden");
    h.message(Message::Plot(Msg::Pick(g.clone())));
    h.message(Message::Plot(Msg::Color(PALETTE[4])));
    let ok = h.find("OK").unwrap();
    h.click(ok.center());
    assert!(h.app.plot_edit.is_none());
    let got = stored_plots(&root, &nb, &sheet)[0].clone();
    let mut want = p.clone();
    want.objs[0].def = now;
    want.objs[1].style.visible = false;
    want.objs[3].style.color = PALETTE[4];
    assert_eq!(got, want);
    // Abbrechen keeps what is stored.
    h.message(Message::EditPlot(sheet.clone(), p.id.clone()));
    h.message(Message::Plot(Msg::Remove(p.objs[4].id.clone())));
    h.message(Message::Plot(Msg::Cancel));
    assert!(h.app.plot_edit.is_none());
    assert_eq!(stored_plots(&root, &nb, &sheet)[0], want);
}

#[test]
fn a_function_of_a_plot_is_found_listed_with_the_plot_icon_and_shown() {
    use n0te_core::search::HitKind;
    use n0te_desktop::plots::Msg;
    use n0te_desktop::sidebar;
    let (mut h, root, nb, sheet) = plot_page("plotsearch", n0te_core::notebook::Background::Blank, true);
    let id = stored_plots(&root, &nb, &sheet)[0].id.clone();
    h.message(Message::EditPlot(sheet.clone(), id));
    let f = h.app.plot_edit.as_ref().unwrap().plot.objs[0].id.clone();
    h.message(Message::Plot(Msg::Edit(f)));
    h.message(Message::Plot(Msg::EditText("f(x) = 2sin x".into())));
    h.message(Message::Plot(Msg::Ok));
    assert!(h.app.plot_edit.is_none());
    // Zoomed in on the page's corner: showing the plot moves the view.
    let c = open(&h).camera;
    let page = open(&h).pages.pages[0].rect();
    h.message(Message::Camera(n0te_desktop::pages::Camera { x: page.x0, y: page.y0 + 500.0, zoom: 3.0, ..c }));
    h.message(Message::SearchQuery("sin".into()));
    let hits: Vec<(HitKind, String, [f32; 4])> = open(&h).search.hits().map(|(_, h)| (h.kind, h.text.clone(), [h.x0, h.y0, h.x1, h.y1])).collect();
    assert_eq!(hits, vec![(HitKind::Plot, "f(x) = 2sin x".to_string(), [100.0, 200.0, 340.0, 360.0])], "the plot's box");
    assert_eq!(sidebar::glyph_of(HitKind::Plot), sidebar::PLOT);
    assert!([HitKind::Ink, HitKind::Text, HitKind::Heading, HitKind::Pdf].iter().all(|&k| sidebar::glyph_of(k) != sidebar::PLOT));
    // Its row: the icon is the pair of axes (an L: down the left, along the bottom) with a curve.
    let row = h.bounds_of(sidebar::hit_id(0)).expect("its row");
    let (rgba, _) = h.frame_rgba();
    let px = |x: f32, y: f32| {
        let i = ((y as usize) * 1600 + x as usize) * 4;
        [rgba[i] as i32, rgba[i + 1] as i32, rgba[i + 2] as i32]
    };
    let bg = px(row.x + 30.0, row.y + 4.0);
    let (ix, iy) = (row.x + 38.0, row.y + 7.0);
    let inked = |x: f32, y: f32| px(ix + x, iy + y).iter().zip(bg).map(|(a, b)| (a - b).abs()).sum::<i32>() > 60;
    let left = (2..12).filter(|&y| (1..4).any(|x| inked(x as f32, y as f32))).count();
    let bottom = (4..12).filter(|&x| (10..13).any(|y| inked(x as f32, y as f32))).count();
    let corner = (8..13).flat_map(|x| (1..5).map(move |y| (x, y))).filter(|&(x, y)| inked(x as f32, y as f32)).count();
    assert!(left >= 7 && bottom >= 6, "an L of axes: {left} {bottom}");
    assert!(corner >= 2, "the curve up to the right: {corner}");
    // A click shows the plot in the middle, lit up.
    h.click(row.center());
    let o = open(&h);
    let flash = o.pages.flash.clone().expect("lit up");
    assert_eq!((flash.sheet_id.as_str(), [flash.rect.x0, flash.rect.y0, flash.rect.x1, flash.rect.y1]), (sheet.as_str(), [100.0, 200.0, 340.0, 360.0]));
    let c = o.camera;
    let middle = (c.x + c.viewport.width / c.zoom / 2.0 - page.x0, c.y + c.viewport.height / c.zoom / 2.0 - page.y0);
    assert!((middle.0 - 220.0).abs() < 0.5 && (middle.1 - 280.0).abs() < 0.5, "{middle:?}");
}

// --- organising the library ------------------------------------------------------------------

/// A library filed like the tablet files it: two notebooks in Uni/WS26 (one pinned), one in
/// Uni/SS25, one at the top; tags. Returns the harness on the library and the ids by title.
fn organised(tag: &str) -> (Harness, std::path::PathBuf, std::collections::HashMap<&'static str, String>) {
    let (mut h, root, ids) = organised_start(tag);
    h.message(Message::Shelf(n0te_desktop::shelf::Msg::Pick(n0te_desktop::shelf::Pick::All)));
    (h, root, ids)
}

/// The same library, on the start.
fn organised_start(tag: &str) -> (Harness, std::path::PathBuf, std::collections::HashMap<&'static str, String>) {
    let root = temp_root(tag);
    let lib = Library::open(&root).unwrap();
    let mut ids = std::collections::HashMap::new();
    for (title, folder, tags, pinned) in [
        ("Analysis", "Uni/WS26", &["Mathe"][..], false),
        ("Algebra", "Uni/WS26", &["Mathe", "Prüfung"][..], true),
        ("Physik", "Uni/SS25", &[][..], false),
        ("Ideen", "", &["Privat"][..], false),
    ] {
        let nb = lib.create_notebook(title).unwrap();
        nb.set_folder(folder).unwrap();
        nb.set_tags(&tags.iter().map(|t| t.to_string()).collect::<Vec<_>>()).unwrap();
        nb.set_pinned(pinned).unwrap();
        lib.save_notebook(&nb).unwrap();
        ids.insert(title, nb.id());
    }
    let h = Harness::new(App::new(&root), "tiny-skia", Size::new(1600.0, 900.0), 1.0).expect("tiny-skia renderer");
    assert_eq!(h.app.entries.len(), 4);
    assert_eq!(h.app.shelf.pick, n0te_desktop::shelf::Pick::Home, "the library starts at the start");
    (h, root, ids)
}

/// A notebook as stored: (title, folder, tags, pinned).
fn filed(root: &std::path::Path, id: &str) -> (String, String, Vec<String>, bool) {
    let nb = Library::open(root).unwrap().load_notebook(id).unwrap();
    (nb.title(), nb.folder(), nb.tags(), nb.pinned())
}

/// Clicks the middle of the widget with `id`.
fn click_on(h: &mut Harness, id: iced::widget::Id) {
    let b = h.bounds_of(id.clone()).unwrap_or_else(|| panic!("{id:?} shown"));
    h.click(b.center());
}

fn key(h: &Harness, k: Named) -> Message {
    match h.key_event(iced::keyboard::Key::Named(k), None)[0].clone() {
        iced::Event::Keyboard(e) => Message::Key(e),
        _ => unreachable!(),
    }
}

#[test]
fn the_sidebar_picks_a_folder_a_tag_or_the_pinned_ones_and_the_search_filters() {
    use n0te_desktop::shelf::{self, Pick};
    let (mut h, _root, ids) = organised("sidebar");
    let card = |h: &mut Harness, t: &str| h.bounds_of(shelf::card_id(&ids[t]));
    for t in ["Analysis", "Algebra", "Physik", "Ideen"] {
        assert!(card(&mut h, t).is_some(), "all notebooks at first: {t}");
    }
    // Pinned first.
    assert!(card(&mut h, "Algebra").unwrap().x < card(&mut h, "Analysis").unwrap().x);
    // A folder: its subfolders as tiles, only what lies right in it as cards.
    let uni = h.bounds_of(shelf::folder_id("Uni")).expect("Uni in the sidebar");
    h.click(uni.center());
    assert_eq!(h.app.shelf.pick, Pick::Folder("Uni".into()));
    assert!(h.bounds_of(shelf::tile_id("Uni/WS26")).is_some() && h.bounds_of(shelf::tile_id("Uni/SS25")).is_some());
    assert!(card(&mut h, "Ideen").is_none() && card(&mut h, "Analysis").is_none());
    assert!(h.bounds_of(shelf::crumb_id("")).is_some(), "the way back: Start › Uni");
    let tile = h.bounds_of(shelf::tile_id("Uni/WS26")).unwrap();
    h.click(tile.center());
    assert_eq!(h.app.shelf.pick, Pick::Folder("Uni/WS26".into()));
    assert!(card(&mut h, "Analysis").is_some() && card(&mut h, "Algebra").is_some() && card(&mut h, "Physik").is_none());
    // Back by the way at the top: Start › Uni › WS26.
    click_on(&mut h, shelf::crumb_id("Uni"));
    assert_eq!(h.app.shelf.pick, Pick::Folder("Uni".into()));
    click_on(&mut h, shelf::crumb_id(""));
    assert_eq!(h.app.shelf.pick, Pick::Home);
    // A tag (its colour is the tablet's), the pinned ones.
    let mathe = h.bounds_of(shelf::tag_id("Mathe")).expect("the tag in the sidebar");
    h.click(mathe.center());
    assert_eq!(h.app.shelf.pick, Pick::Tag("Mathe".into()));
    assert!(card(&mut h, "Analysis").is_some() && card(&mut h, "Ideen").is_none());
    let pinned = h.find("Angeheftet").expect("Angeheftet, as one is pinned");
    h.click(pinned.center());
    assert!(card(&mut h, "Algebra").is_some() && card(&mut h, "Analysis").is_none());
    h.message(Message::Shelf(shelf::Msg::Pick(Pick::All)));
    // The search: title, folder and tags, ignoring case; Esc empties it.
    let field = h.bounds_of(shelf::search_id()).expect("the search field");
    h.click(field.center());
    h.type_text("SS25");
    assert_eq!(h.app.shelf.query, "SS25");
    assert!(card(&mut h, "Physik").is_some() && card(&mut h, "Analysis").is_none() && card(&mut h, "Ideen").is_none());
    h.press(Named::Escape);
    assert_eq!(h.app.shelf.query, "");
    assert!(card(&mut h, "Ideen").is_some());
    // Ctrl+F goes to the field.
    let m = ctrl_key(&h, "f");
    h.message(m);
    assert_eq!(h.find(iced_test::selector::is_focused()), Some(field));
    // The arrow before a folder folds it.
    let uni = h.bounds_of(shelf::folder_id("Uni")).unwrap();
    h.click(Point::new(uni.x + 17.0, uni.center_y()));
    assert!(h.app.shelf.folded.contains("Uni"));
    assert!(h.bounds_of(shelf::folder_id("Uni/WS26")).is_none(), "folded away");
    assert_eq!(h.app.shelf.pick, Pick::All, "the arrow doesn't open the folder");
}

#[test]
fn a_right_click_opens_a_small_menu_at_the_pointer_and_its_rows_act() {
    use n0te_desktop::shelf::{self, Action, Target};
    let (mut h, root, ids) = organised("menu");
    let ideen = ids["Ideen"].clone();
    let card = h.bounds_of(shelf::card_id(&ideen)).unwrap();
    let was = card.x;
    let at = Point::new(card.x + 60.0, card.y + 80.0);
    h.right_click(at);
    let menu = h.app.shelf.menu.clone().expect("a menu");
    assert_eq!(menu.target, Target::Notebooks(vec![ideen.clone()]));
    let first = h.bounds_of(shelf::menu_id(Action::Open)).expect("Öffnen");
    assert!((first.x - at.x).abs() < 12.0 && (first.y - at.y).abs() < 12.0, "at the pointer: {first:?} {at:?}");
    for a in [Action::Rename, Action::Move, Action::Tags, Action::Pin, Action::Cover, Action::Trash] {
        assert!(h.bounds_of(shelf::menu_id(a)).is_some(), "{a:?}");
    }
    assert!(h.find("Öffnen").is_some() && h.find("In den Papierkorb").is_some(), "short labels, no more");
    // Anheften.
    let pin = h.bounds_of(shelf::menu_id(Action::Pin)).unwrap();
    h.click(pin.center());
    assert!(h.app.shelf.menu.is_none());
    assert!(filed(&root, &ideen).3, "pinned in its document");
    // Pinned ones come first: the card moved.
    let card = h.bounds_of(shelf::card_id(&ideen)).unwrap();
    assert!(card.x < was, "moved to the front");
    let at = Point::new(card.x + 60.0, card.y + 80.0);
    // Umbenennen: the title selected in the field, Enter keeps what is typed.
    h.right_click(at);
    let rename = h.bounds_of(shelf::menu_id(Action::Rename)).unwrap();
    h.click(rename.center());
    assert_eq!(h.find(iced_test::selector::is_focused()), h.bounds_of(shelf::field_id()), "the field has the focus");
    h.type_text("Einfälle");
    h.press(Named::Enter);
    assert!(h.app.shelf.dialog.is_none());
    assert_eq!(filed(&root, &ideen).0, "Einfälle");
    // A click beside the menu only closes it (nothing opens); so does Esc.
    let card = h.bounds_of(shelf::card_id(&ideen)).unwrap();
    let at = Point::new(card.x + 60.0, card.y + 80.0);
    h.right_click(at);
    h.click(Point::new(1500.0, 820.0));
    assert!(h.app.shelf.menu.is_none() && matches!(h.app.screen, Screen::Library));
    h.right_click(at);
    let m = key(&h, Named::Escape);
    h.message(m);
    assert!(h.app.shelf.menu.is_none());
    // Right-clicking another card while a menu is open opens that one's at once.
    h.right_click(at);
    let other = h.bounds_of(shelf::card_id(&ids["Physik"])).unwrap();
    h.right_click(other.center());
    assert_eq!(h.app.shelf.menu.as_ref().map(|m| m.target.clone()), Some(Target::Notebooks(vec![ids["Physik"].clone()])));
    h.message(Message::Shelf(shelf::Msg::CloseMenu));
    // ⋯ on the cover (under the pointer) opens the same menu, without opening the notebook.
    h.move_to(Point::new(card.x + 80.0, card.y + 100.0));
    h.click(Point::new(card.x + 10.0 + 164.0 - 7.0 - 14.0, card.y + 10.0 + 7.0 + 10.0));
    assert_eq!(h.app.shelf.menu.as_ref().map(|m| m.target.clone()), Some(Target::Notebooks(vec![ideen.clone()])));
    assert!(matches!(h.app.screen, Screen::Library));
    // In den Papierkorb: the first click asks, the second does it.
    let trash = h.bounds_of(shelf::menu_id(Action::Trash)).unwrap();
    h.click(trash.center());
    assert!(h.app.shelf.menu.is_some() && h.find("Wirklich in den Papierkorb?").is_some(), "asks first");
    h.click(trash.center());
    assert!(h.app.shelf.menu.is_none());
    assert!(Library::open(&root).unwrap().is_trashed(&ideen));
    assert!(h.app.entries.iter().all(|e| e.id != ideen));
    assert!(h.app.status.as_deref().is_some_and(|s| s.contains("Papierkorb")));
    // Öffnen.
    let card = h.bounds_of(shelf::card_id(&ids["Physik"])).unwrap();
    h.right_click(card.center());
    let open_row = h.bounds_of(shelf::menu_id(Action::Open)).unwrap();
    h.click(open_row.center());
    assert!(matches!(&h.app.screen, Screen::Notebook(o) if o.id == ids["Physik"]));
}

#[test]
fn renaming_a_folder_a_new_subfolder_and_dissolving_carry_the_notebooks_along() {
    use n0te_desktop::shelf::{self, Action, Pick};
    let (mut h, root, ids) = organised("folders");
    h.message(Message::Shelf(shelf::Msg::Pick(Pick::Folder("Uni/WS26".into()))));
    // Umbenennen: "Uni" becomes "Studium", everything inside goes along (the folder in view too).
    let uni = h.bounds_of(shelf::folder_id("Uni")).unwrap();
    h.right_click(uni.center());
    assert!(h.bounds_of(shelf::menu_id(Action::Dissolve)).is_some() && h.bounds_of(shelf::menu_id(Action::Open)).is_none(), "the folder menu");
    let rename = h.bounds_of(shelf::menu_id(Action::Rename)).unwrap();
    h.click(rename.center());
    h.type_text("Studium");
    h.press(Named::Enter);
    assert_eq!(filed(&root, &ids["Analysis"]).1, "Studium/WS26");
    assert_eq!(filed(&root, &ids["Physik"]).1, "Studium/SS25");
    assert_eq!(filed(&root, &ids["Ideen"]).1, "");
    assert_eq!(h.app.shelf.pick, Pick::Folder("Studium/WS26".into()));
    assert!(h.bounds_of(shelf::folder_id("Uni")).is_none() && h.bounds_of(shelf::folder_id("Studium")).is_some());
    // Neuer Unterordner: empty, so only for now (nothing on disk).
    let ws = h.bounds_of(shelf::folder_id("Studium/WS26")).unwrap();
    h.right_click(ws.center());
    let new = h.bounds_of(shelf::menu_id(Action::NewFolder)).unwrap();
    h.click(new.center());
    h.type_text("Klausuren/2");
    h.press(Named::Enter);
    assert!(h.app.shelf.empty.contains("Studium/WS26/Klausuren 2"), "a / in a name doesn't nest: {:?}", h.app.shelf.empty);
    assert!(h.bounds_of(shelf::folder_id("Studium/WS26/Klausuren 2")).is_some(), "listed in the sidebar");
    assert_eq!(h.app.shelf.pick, Pick::Folder("Studium/WS26/Klausuren 2".into()), "and opened");
    assert!(h.find("Noch leer: zieh ein Notizbuch hierher oder nimm „Verschieben nach …“.").is_some());
    // Auflösen asks first; then what was in it moves one level up, the empty subfolder too.
    let ws = h.bounds_of(shelf::folder_id("Studium/WS26")).unwrap();
    h.right_click(ws.center());
    let dissolve = h.bounds_of(shelf::menu_id(Action::Dissolve)).unwrap();
    h.click(dissolve.center());
    assert!(h.find("Wirklich auflösen?").is_some());
    assert_eq!(filed(&root, &ids["Analysis"]).1, "Studium/WS26", "not yet");
    h.click(dissolve.center());
    assert_eq!(filed(&root, &ids["Analysis"]).1, "Studium");
    assert_eq!(filed(&root, &ids["Algebra"]).1, "Studium");
    assert_eq!(filed(&root, &ids["Physik"]).1, "Studium/SS25");
    assert!(h.app.shelf.empty.contains("Studium/Klausuren 2") && !h.app.shelf.empty.iter().any(|f| f.contains("WS26")));
    assert_eq!(h.app.shelf.pick, Pick::Folder("Studium/Klausuren 2".into()), "the folder in view went along");
    // A new folder at the top ("+" by "Ordner" or "Alle Notizbücher"'s menu).
    let all = h.bounds_of(shelf::folder_id("")).unwrap();
    h.right_click(all.center());
    let new = h.bounds_of(shelf::menu_id(Action::NewFolder)).unwrap();
    h.click(new.center());
    h.type_text("Arbeit");
    h.press(Named::Enter);
    assert!(h.bounds_of(shelf::folder_id("Arbeit")).is_some());
    assert_eq!(h.app.shelf.pick, Pick::Folder("Arbeit".into()));
}

#[test]
fn a_tag_is_renamed_or_taken_off_every_notebook_from_its_menu() {
    use n0te_desktop::shelf::{self, Action, Pick};
    let (mut h, root, ids) = organised("tags");
    let mathe = h.bounds_of(shelf::tag_id("Mathe")).unwrap();
    h.click(mathe.center());
    h.right_click(mathe.center());
    assert!(h.bounds_of(shelf::menu_id(Action::Remove)).is_some() && h.bounds_of(shelf::menu_id(Action::Move)).is_none(), "the tag menu");
    click_on(&mut h, shelf::menu_id(Action::Rename));
    h.type_text("Mathematik");
    h.press(Named::Enter);
    assert_eq!(filed(&root, &ids["Analysis"]).2, vec!["Mathematik".to_string()]);
    assert_eq!(filed(&root, &ids["Algebra"]).2, vec!["Mathematik".to_string(), "Prüfung".to_string()]);
    assert_eq!(h.app.shelf.pick, Pick::Tag("Mathematik".into()), "the tag in view went along");
    // Off every notebook: asks first.
    let row = h.bounds_of(shelf::tag_id("Mathematik")).unwrap();
    h.right_click(row.center());
    click_on(&mut h, shelf::menu_id(Action::Remove));
    assert!(filed(&root, &ids["Analysis"]).2.len() == 1, "not yet");
    click_on(&mut h, shelf::menu_id(Action::Remove));
    assert!(filed(&root, &ids["Analysis"]).2.is_empty());
    assert_eq!(filed(&root, &ids["Algebra"]).2, vec!["Prüfung".to_string()]);
    assert_eq!(h.app.shelf.pick, Pick::Home, "nothing has the tag any more: back to the start");
    assert!(h.bounds_of(shelf::tag_id("Mathematik")).is_none());
}

#[test]
fn the_move_dialog_takes_a_folder_or_a_new_one_and_the_tags_dialog_toggles_and_adds() {
    use n0te_desktop::shelf::{self, Action, Dialog, Msg};
    let (mut h, root, ids) = organised("dialogs");
    let ideen = ids["Ideen"].clone();
    let card = h.bounds_of(shelf::card_id(&ideen)).unwrap();
    h.right_click(card.center());
    let mv = h.bounds_of(shelf::menu_id(Action::Move)).unwrap();
    h.click(mv.center());
    assert!(matches!(&h.app.shelf.dialog, Some(Dialog::Move { to, .. }) if to.is_empty()), "starts where it lies");
    let ss25 = h.bounds_of(shelf::pick_id("Uni/SS25")).expect("the folder tree");
    h.click(ss25.center());
    assert!(matches!(&h.app.shelf.dialog, Some(Dialog::Move { to, .. }) if to == "Uni/SS25"));
    // "Neuer Ordner" inside the one picked.
    h.message(Message::Shelf(Msg::NewFolder));
    assert_eq!(h.find(iced_test::selector::is_focused()), h.bounds_of(shelf::field_id()));
    h.type_text("Labor");
    h.press(Named::Enter);
    assert!(matches!(&h.app.shelf.dialog, Some(Dialog::Move { to, new_folder: None, .. }) if to == "Uni/SS25/Labor"));
    assert!(h.bounds_of(shelf::pick_id("Uni/SS25/Labor")).is_some(), "listed, picked");
    h.message(Message::Shelf(Msg::Confirm));
    assert!(h.app.shelf.dialog.is_none());
    assert_eq!(filed(&root, &ideen).1, "Uni/SS25/Labor");
    assert!(h.app.status.as_deref().is_some_and(|s| s.contains("Uni/SS25/Labor")), "{:?}", h.app.status);
    // A click beside a dialog closes it.
    h.right_click(card.center());
    click_on(&mut h, shelf::menu_id(Action::Move));
    h.click(Point::new(1580.0, 880.0));
    assert!(h.app.shelf.dialog.is_none());
    // Tags: the library's tags as switches, a new one typed.
    let physik = ids["Physik"].clone();
    let card = h.bounds_of(shelf::card_id(&physik)).unwrap();
    h.right_click(card.center());
    click_on(&mut h, shelf::menu_id(Action::Tags));
    assert_eq!(h.find(iced_test::selector::is_focused()), h.bounds_of(shelf::field_id()));
    let mathe = h.bounds_of(shelf::tag_switch_id("mathe")).expect("Mathe to switch on");
    h.click(mathe.center());
    assert_eq!(filed(&root, &physik).2, vec!["Mathe".to_string()]);
    h.click(mathe.center());
    assert!(filed(&root, &physik).2.is_empty(), "off again");
    click_on(&mut h, shelf::field_id());
    h.type_text("Klausur");
    h.press(Named::Enter);
    assert_eq!(filed(&root, &physik).2, vec!["Klausur".to_string()]);
    assert!(h.app.shelf.dialog.is_some(), "stays open for more");
    h.press(Named::Escape);
    assert!(h.app.shelf.dialog.is_none(), "Esc in the field closes it");
    assert!(h.bounds_of(shelf::tag_id("Klausur")).is_some(), "a new tag in the sidebar");
    // Cover.
    h.right_click(card.center());
    click_on(&mut h, shelf::menu_id(Action::Cover));
    h.message(Message::Shelf(Msg::Cover("purple".into())));
    assert!(h.app.shelf.dialog.is_none());
    assert_eq!(Library::open(&root).unwrap().load_notebook(&physik).unwrap().cover(), "purple");
}

#[test]
fn auswaehlen_ticks_cards_and_the_bar_acts_on_all_of_them() {
    use n0te_desktop::shelf::{self, Action, Msg, Target};
    let (mut h, root, ids) = organised("select");
    let (a, p) = (ids["Analysis"].clone(), ids["Physik"].clone());
    h.message(Message::Shelf(Msg::Select(true)));
    for id in [&a, &p] {
        let card = h.bounds_of(shelf::card_id(id)).unwrap();
        h.click(card.center());
    }
    assert!(matches!(h.app.screen, Screen::Library), "a click ticks, doesn't open");
    let picked: Vec<String> = h.app.shelf.selected.clone().unwrap().into_iter().collect();
    assert_eq!(picked.len(), 2);
    // The menu of a ticked card is about all of them.
    let card = h.bounds_of(shelf::card_id(&a)).unwrap();
    h.right_click(card.center());
    assert!(matches!(&h.app.shelf.menu, Some(m) if matches!(&m.target, Target::Notebooks(v) if v.len() == 2)));
    assert!(h.bounds_of(shelf::menu_id(Action::Rename)).is_none(), "nothing for one alone");
    h.message(Message::Shelf(Msg::CloseMenu));
    let both = Target::Notebooks(h.app.entries.iter().filter(|e| e.id == a || e.id == p).map(|e| e.id.clone()).collect());
    h.message(Message::Shelf(Msg::Act(both.clone(), Action::Pin)));
    assert!(filed(&root, &a).3 && filed(&root, &p).3);
    h.message(Message::Shelf(Msg::Act(both.clone(), Action::Tags)));
    h.message(Message::Shelf(Msg::ToggleTag("Prüfung".into())));
    h.message(Message::Shelf(Msg::CloseDialog));
    assert_eq!(filed(&root, &a).2, vec!["Mathe".to_string(), "Prüfung".to_string()]);
    assert_eq!(filed(&root, &p).2, vec!["Prüfung".to_string()]);
    h.message(Message::Shelf(Msg::Act(both.clone(), Action::Move)));
    h.message(Message::Shelf(Msg::To("Uni".into())));
    h.message(Message::Shelf(Msg::Confirm));
    assert_eq!((filed(&root, &a).1, filed(&root, &p).1), ("Uni".to_string(), "Uni".to_string()));
    assert!(h.app.shelf.selected.is_none(), "done selecting after a move");
    assert!(h.app.shelf.empty.contains("Uni/SS25"), "the folder they left stays for now");
    // Papierkorb for all: asks first.
    h.message(Message::Shelf(Msg::Select(true)));
    h.message(Message::Shelf(Msg::SelectAll));
    assert_eq!(h.app.shelf.selected.as_ref().map(|s| s.len()), Some(4));
    h.message(Message::Shelf(Msg::SelectAll));
    assert_eq!(h.app.shelf.selected.as_ref().map(|s| s.len()), Some(0), "all again: none");
    let ideen = ids["Ideen"].clone();
    h.message(Message::Shelf(Msg::Tick(ideen.clone())));
    h.message(Message::Shelf(Msg::Act(Target::Notebooks(vec![ideen.clone()]), Action::Trash)));
    assert!(!Library::open(&root).unwrap().is_trashed(&ideen));
    h.message(Message::Shelf(Msg::Act(Target::Notebooks(vec![ideen.clone()]), Action::Trash)));
    assert!(Library::open(&root).unwrap().is_trashed(&ideen));
    assert_eq!(h.app.entries.len(), 3);
    // Esc leaves "Auswählen".
    let m = key(&h, Named::Escape);
    h.message(m);
    assert!(h.app.shelf.selected.is_none());
}

#[test]
fn dragging_a_card_onto_a_folder_in_the_sidebar_moves_it_there() {
    use n0te_desktop::shelf;
    let (mut h, root, ids) = organised("drag");
    let ideen = ids["Ideen"].clone();
    let card = h.bounds_of(shelf::card_id(&ideen)).unwrap();
    h.press_at(card.center());
    h.move_to(Point::new(card.center_x() - 30.0, card.center_y()));
    assert!(h.app.shelf.drag.as_ref().is_some_and(|d| d.ids == vec![ideen.clone()]), "a drag past a few pixels");
    let ss25 = h.bounds_of(shelf::folder_id("Uni/SS25")).unwrap();
    h.move_to(Point::new(ss25.x + 50.0, ss25.center_y()));
    assert_eq!(h.app.shelf.drag.as_ref().and_then(|d| d.over.clone()), Some(shelf::Drop::Folder("Uni/SS25".into())));
    h.release();
    assert!(h.app.shelf.drag.is_none());
    assert_eq!(filed(&root, &ideen).1, "Uni/SS25");
    assert!(matches!(h.app.screen, Screen::Library), "dropping doesn't open it");
    // Onto "Alle Notizbücher": back to the top.
    let card = h.bounds_of(shelf::card_id(&ideen)).unwrap();
    h.press_at(card.center());
    h.move_to(Point::new(card.center_x() + 20.0, card.center_y() + 20.0));
    let all = h.bounds_of(shelf::folder_id("")).unwrap();
    h.move_to(all.center());
    h.release();
    assert_eq!(filed(&root, &ideen).1, "");
    // Let go over nothing: nothing moves; a short wiggle is still a click.
    let card = h.bounds_of(shelf::card_id(&ideen)).unwrap();
    h.press_at(card.center());
    h.move_to(Point::new(card.center_x() + 40.0, card.center_y()));
    h.move_to(Point::new(1500.0, 820.0));
    h.release();
    assert_eq!(filed(&root, &ideen).1, "");
    assert!(matches!(h.app.screen, Screen::Library));
    h.press_at(card.center());
    h.move_to(Point::new(card.center_x() + 2.0, card.center_y()));
    h.release();
    assert!(matches!(&h.app.screen, Screen::Notebook(o) if o.id == ideen));
}

#[test]
fn the_start_shows_opened_last_pinned_folders_and_what_lies_in_none() {
    use n0te_desktop::shelf::{self, Pick};
    let (mut h, root, ids) = organised_start("start");
    for t in ["Zuletzt geöffnet", "Angeheftet", "Ordner", "Ohne Ordner"] {
        assert!(h.bounds_of(shelf::section_id(t)).is_some(), "section {t}");
    }
    let top = |h: &mut Harness, id: iced::widget::Id| h.bounds_of(id).map(|b| b.y);
    let (recent, pinned, folders, loose) = (top(&mut h, shelf::section_id("Zuletzt geöffnet")).unwrap(), top(&mut h, shelf::section_id("Angeheftet")).unwrap(), top(&mut h, shelf::section_id("Ordner")).unwrap(), top(&mut h, shelf::section_id("Ohne Ordner")).unwrap());
    assert!(recent < pinned && pinned < folders && folders < loose, "in this order");
    // Every notebook in "Zuletzt geöffnet"; pinned ones and the ones in no folder as cards.
    for t in ["Analysis", "Algebra", "Physik", "Ideen"] {
        assert!(h.bounds_of(shelf::recent_id(&ids[t])).is_some(), "{t} opened lately");
    }
    assert!(h.bounds_of(shelf::card_id(&ids["Algebra"])).is_some_and(|b| b.y > pinned && b.y < folders), "pinned");
    assert!(h.bounds_of(shelf::card_id(&ids["Ideen"])).is_some_and(|b| b.y > loose), "in no folder");
    assert!(h.bounds_of(shelf::card_id(&ids["Analysis"])).is_none(), "in a folder: a click on it away");
    assert!(h.bounds_of(shelf::tile_id("Uni")).is_some() && h.bounds_of(shelf::tile_id("Uni/WS26")).is_none(), "the top folders");
    // Opened last comes first (the settings keep when).
    for t in ["Physik", "Analysis"] {
        h.message(Message::Open(ids[t].clone()));
        h.message(Message::Back);
    }
    assert_eq!(h.app.shelf.pick, Pick::Home, "back at the start");
    let order: Vec<String> = n0te_desktop::library::recent(&h.app).iter().map(|e| e.id.clone()).collect();
    assert_eq!(&order[..2], &[ids["Analysis"].clone(), ids["Physik"].clone()]);
    assert_eq!(h.app.settings.lib_opened.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(), vec![ids["Analysis"].clone(), ids["Physik"].clone()]);
    let x = |h: &mut Harness, t: &str| h.bounds_of(shelf::recent_id(&ids[t])).unwrap().x;
    assert!(x(&mut h, "Analysis") < x(&mut h, "Physik") && x(&mut h, "Physik") < x(&mut h, "Ideen"));
    // "Neuer Ordner" (the tile after the folders): opens it; the settings keep it while it is
    // empty.
    assert!(h.bounds_of(shelf::new_folder_id()).is_some());
    h.message(Message::Shelf(shelf::Msg::Act(shelf::Target::Folder(String::new()), shelf::Action::NewFolder)));
    h.type_text("Projekte");
    h.press(Named::Enter);
    assert_eq!(h.app.shelf.pick, Pick::Folder("Projekte".into()));
    assert_eq!(h.app.settings.lib_folders, vec!["Projekte".to_string()]);
    h.message(Message::Shelf(shelf::Msg::Act(shelf::Target::Notebooks(vec![ids["Ideen"].clone()]), shelf::Action::Move)));
    h.message(Message::Shelf(shelf::Msg::To("Projekte".into())));
    h.message(Message::Shelf(shelf::Msg::Confirm));
    assert_eq!(filed(&root, &ids["Ideen"]).1, "Projekte");
    assert!(h.app.settings.lib_folders.is_empty(), "a notebook keeps it now");
    // Kept folders come back with the settings.
    let settings = n0te_desktop::settings::Settings { lib_folders: vec!["Archiv/2025".into()], ..Default::default() };
    let mut again = Harness::new(App::with_settings(&root, settings), "tiny-skia", Size::new(1600.0, 900.0), 1.0).unwrap();
    assert!(again.bounds_of(shelf::folder_id("Archiv")).is_some() && again.bounds_of(shelf::tile_id("Archiv")).is_some());
    // Three notebooks or fewer: no "Zuletzt geöffnet".
    let small = temp_root("start-small");
    let lib = Library::open(&small).unwrap();
    for t in ["Eins", "Zwei", "Drei"] {
        lib.create_notebook(t).unwrap();
    }
    let mut few = Harness::new(App::new(&small), "tiny-skia", Size::new(1600.0, 900.0), 1.0).unwrap();
    assert!(few.bounds_of(shelf::section_id("Zuletzt geöffnet")).is_none());
    assert!(few.bounds_of(shelf::section_id("Ohne Ordner")).is_some() && few.bounds_of(shelf::section_id("Angeheftet")).is_none());
}

#[test]
fn list_rows_open_tick_show_the_menu_and_drag_like_cards() {
    use n0te_desktop::shelf::{self, Target};
    let (mut h, root, ids) = organised("list");
    click_on(&mut h, iced::widget::Id::new("list"));
    assert!(h.app.settings.lib_list, "the switch");
    let row = h.bounds_of(shelf::card_id(&ids["Physik"])).expect("a row");
    assert!(row.height < 90.0 && row.width > 800.0, "a row across: {row:?}");
    // A right click: its menu.
    h.right_click(row.center());
    assert_eq!(h.app.shelf.menu.as_ref().map(|m| m.target.clone()), Some(Target::Notebooks(vec![ids["Physik"].clone()])));
    h.message(Message::Shelf(shelf::Msg::CloseMenu));
    // ⋯ at its end, too.
    h.click(Point::new(row.x + row.width - 26.0, row.center_y()));
    assert!(h.app.shelf.menu.is_some() && matches!(h.app.screen, Screen::Library));
    h.message(Message::Shelf(shelf::Msg::CloseMenu));
    // Dragged onto a folder.
    let row = h.bounds_of(shelf::card_id(&ids["Ideen"])).unwrap();
    h.press_at(row.center());
    h.move_to(Point::new(row.center_x() - 40.0, row.center_y()));
    let ws = h.bounds_of(shelf::folder_id("Uni/WS26")).unwrap();
    h.move_to(ws.center());
    h.release();
    assert_eq!(filed(&root, &ids["Ideen"]).1, "Uni/WS26");
    // Ticked in "Auswählen", opened otherwise.
    h.message(Message::Shelf(shelf::Msg::Select(true)));
    let row = h.bounds_of(shelf::card_id(&ids["Physik"])).unwrap();
    h.click(row.center());
    assert!(h.app.shelf.selected.as_ref().is_some_and(|s| s.contains(&ids["Physik"])) && matches!(h.app.screen, Screen::Library));
    h.message(Message::Shelf(shelf::Msg::Select(false)));
    let row = h.bounds_of(shelf::card_id(&ids["Physik"])).unwrap();
    h.click(row.center());
    assert!(matches!(&h.app.screen, Screen::Notebook(o) if o.id == ids["Physik"]));
    h.message(Message::Back);
    click_on(&mut h, iced::widget::Id::new("grid"));
    assert!(!h.app.settings.lib_list);
}

#[test]
fn notebooks_dropped_on_angeheftet_ohne_ordner_or_a_tag() {
    use n0te_desktop::shelf::{self, Action, Msg, Target};
    let (mut h, root, ids) = organised("drops");
    // Nothing pinned: "Angeheftet" shows only while dragging.
    h.message(Message::Shelf(Msg::Act(Target::Notebooks(vec![ids["Algebra"].clone()]), Action::Pin)));
    assert!(!filed(&root, &ids["Algebra"]).3);
    assert!(h.bounds_of(shelf::side_id("pinned")).is_none());
    let drag = |h: &mut Harness, id: &str, onto: iced::widget::Id, check: &dyn Fn(&mut Harness)| {
        let card = h.bounds_of(shelf::card_id(id)).unwrap();
        h.press_at(card.center());
        h.move_to(Point::new(card.center_x() - 30.0, card.center_y()));
        check(h);
        let target = h.bounds_of(onto).expect("the target shows while dragging");
        h.move_to(target.center());
        h.release();
    };
    // In no folder: no "Ohne Ordner" row; onto "Angeheftet": pinned.
    drag(&mut h, &ids["Ideen"], shelf::side_id("pinned"), &|h| assert!(h.bounds_of(shelf::side_id("top")).is_none(), "nothing to take out of a folder"));
    assert!(filed(&root, &ids["Ideen"]).3, "pinned");
    // In a folder: onto "Ohne Ordner": to the top.
    drag(&mut h, &ids["Physik"], shelf::side_id("top"), &|_| {});
    assert_eq!(filed(&root, &ids["Physik"]).1, "");
    assert!(h.bounds_of(shelf::side_id("top")).is_none(), "gone after the drag");
    // Onto a tag: it is added.
    drag(&mut h, &ids["Analysis"], shelf::tag_id("Privat"), &|_| {});
    assert_eq!(filed(&root, &ids["Analysis"]).2, vec!["Mathe".to_string(), "Privat".to_string()]);
}

/// The demo notebook open, with its library root.
fn demo_open(tag: &str) -> (Harness, std::path::PathBuf, String) {
    let root = temp_root(tag);
    let id = demo::save_into(&Library::open(&root).unwrap());
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(1600.0, 900.0), 1.0).expect("tiny-skia renderer");
    h.message(Message::Open(id.clone()));
    h.events(&[]);
    h.events(&[]);
    (h, root, id)
}

fn sheet_nodes(h: &Harness) -> Vec<(String, String, String)> {
    open(h).rows.iter().filter_map(|r| if let OutlineRow::Sheet { node, sheet_id, title, .. } = r { Some((node.clone(), sheet_id.clone(), title.clone())) } else { None }).collect()
}

#[test]
fn outline_pages_are_marked_filtered_renamed_and_deleted_from_their_menu() {
    use n0te_desktop::sidebar::{self, Msg};
    let (mut h, root, nb) = demo_open("outline-menu");
    assert!(h.find("Gliederung").is_some() || h.find("GLIEDERUNG").is_some());
    let pages = sheet_nodes(&h);
    let (first, second) = (pages[0].clone(), pages[1].clone());
    // Merken from the page's right-click menu: a ribbon, the filter.
    let row = h.bounds_of(sidebar::page_id(&first.0)).expect("its row");
    h.right_click(row.center());
    assert!(open(&h).outline.menu.is_some());
    assert!(h.find("Merken").is_some() && h.find("Löschen").is_some());
    click_on(&mut h, sidebar::menu_id("mark"));
    assert_eq!(open(&h).marked, vec![first.0.clone()]);
    assert_eq!(Library::open(&root).unwrap().load_notebook(&nb).unwrap().marked(), vec![first.0.clone()], "saved");
    h.message(Message::Outline(Msg::OnlyMarked(true)));
    assert!(h.bounds_of(sidebar::page_id(&first.0)).is_some() && h.bounds_of(sidebar::page_id(&second.0)).is_none(), "only the marked page");
    h.message(Message::Outline(Msg::Mark(first.0.clone(), false)));
    assert!(open(&h).marked.is_empty() && !open(&h).outline.only_marked, "nothing marked: all again");
    // Umbenennen: the name selected, Enter keeps what is typed.
    let row = h.bounds_of(sidebar::page_id(&second.0)).unwrap();
    h.right_click(row.center());
    click_on(&mut h, sidebar::menu_id("rename"));
    assert_eq!(h.find(iced_test::selector::is_focused()), h.bounds_of(sidebar::rename_id()));
    h.type_text("Einleitung");
    h.press(Named::Enter);
    assert!(open(&h).outline.renaming.is_none());
    assert_eq!(sheet_nodes(&h)[1].2, "Einleitung");
    assert!(h.find("Einleitung").is_some());
    // Löschen asks once, then deletes; "Rückgängig" brings it back.
    let count = pages.len();
    let row = h.bounds_of(sidebar::page_id(&second.0)).unwrap();
    h.right_click(row.center());
    click_on(&mut h, sidebar::menu_id("delete"));
    assert!(h.find("Wirklich löschen?").is_some() && sheet_nodes(&h).len() == count, "asks first");
    click_on(&mut h, sidebar::menu_id("delete"));
    assert_eq!(sheet_nodes(&h).len(), count - 1);
    assert!(h.app.undo_delete.is_some() && h.app.status.as_deref() == Some("Seite gelöscht"));
    h.message(Message::Outline(Msg::Undo));
    assert_eq!(sheet_nodes(&h).len(), count);
    // A group's menu: rename, fold, delete (no marking).
    let group = open(&h).rows.iter().find_map(|r| if let OutlineRow::Group { node, .. } = r { Some(node.clone()) } else { None }).unwrap();
    let row = h.bounds_of(sidebar::page_id(&group)).unwrap();
    h.right_click(row.center());
    assert!(h.bounds_of(sidebar::menu_id("fold")).is_some() && h.bounds_of(sidebar::menu_id("mark")).is_none());
    let m = key(&h, Named::Escape);
    h.message(m);
    assert!(open(&h).outline.menu.is_none(), "Esc closes it");
}

#[test]
fn the_outline_footer_adds_pages_and_groups_and_folds_every_group() {
    let (mut h, _root, _nb) = demo_open("outline-foot");
    let mains = |h: &Harness| open(h).pages.mains().len();
    let before = mains(&h);
    click_on(&mut h, iced::widget::Id::new("add-page"));
    assert_eq!(mains(&h), before + 1);
    let o = open(&h);
    assert_eq!(o.pages.current_number(&o.camera), 2, "after the page in view, and shown");
    // A group, named right away.
    click_on(&mut h, iced::widget::Id::new("add-group"));
    assert!(open(&h).outline.renaming.as_ref().is_some_and(|(_, t)| t == "Neue Gruppe"));
    h.type_text("Woche 5");
    h.press(Named::Enter);
    assert!(open(&h).rows.iter().any(|r| matches!(r, OutlineRow::Group { title, .. } if title == "Woche 5")));
    // Every group folded, then open again.
    let groups = |h: &Harness| open(h).rows.iter().filter_map(|r| if let OutlineRow::Group { collapsed, .. } = r { Some(*collapsed) } else { None }).collect::<Vec<_>>();
    click_on(&mut h, iced::widget::Id::new("fold-all"));
    assert!(groups(&h).iter().all(|c| *c), "all folded: {:?}", groups(&h));
    // Open again: the groups shown (inner ones stay folded until they show and are opened).
    let shown: Vec<String> = open(&h).rows.iter().filter_map(|r| if let OutlineRow::Group { node, .. } = r { Some(node.clone()) } else { None }).collect();
    click_on(&mut h, iced::widget::Id::new("fold-all"));
    for g in &shown {
        assert!(open(&h).rows.iter().any(|r| matches!(r, OutlineRow::Group { node, collapsed: false, .. } if node == g)), "{g} open again");
    }
}

#[test]
fn page_pictures_and_the_outline_follows_the_page_in_view() {
    use n0te_desktop::sidebar;
    let (mut h, _root, _nb) = demo_open("outline-pics");
    click_on(&mut h, iced::widget::Id::new("pictures"));
    assert!(h.app.settings.outline_pictures);
    let pages = sheet_nodes(&h);
    // (The cover, then a group: its pages two to a row.)
    let pic = h.bounds_of(sidebar::page_id(&pages[1].0)).expect("a picture");
    assert!(pic.width < 130.0 && pic.height > 120.0, "two to a row, page-shaped: {pic:?}");
    let next = h.bounds_of(sidebar::page_id(&pages[2].0)).unwrap();
    assert!((next.y - pic.y).abs() < 1.0 && next.x > pic.x, "side by side");
    let cover = h.bounds_of(sidebar::page_id(&pages[0].0)).unwrap();
    assert!(cover.y < pic.y - pic.height, "a group's heading starts a new row");
    // Further down the notebook: switching the view brings its page into the list's view; a
    // click a third down the list lands on it.
    let o = open(&h);
    let last = *o.pages.mains().last().unwrap();
    let (sheet, number) = (o.pages.pages[last].placed.sheet_id.clone(), o.pages.pages[last].placed.number);
    let back = o.pages.mains()[11];
    let (back_sheet, back_number) = (o.pages.pages[back].placed.sheet_id.clone(), o.pages.pages[back].placed.number);
    h.message(Message::Jump(back_sheet));
    click_on(&mut h, iced::widget::Id::new("outline-list"));
    assert!(!h.app.settings.outline_pictures);
    let list = h.bounds_of(sidebar::list_id()).expect("the list");
    h.click(Point::new(list.x + 120.0, list.y + list.height / 3.0 + 12.0));
    let o = open(&h);
    assert_eq!(o.pages.current_number(&o.camera), back_number, "the row there is the page in view");
    h.message(Message::Jump(sheet));
    let o = open(&h);
    assert_eq!(o.pages.current_number(&o.camera), number);
}

// --- two notebooks side by side ------------------------------------------------------------------

fn split_of(h: &Harness) -> &n0te_desktop::split::Split {
    match &h.app.screen {
        Screen::Notebook(s) => s,
        Screen::Library => panic!("no notebook open"),
    }
}

/// A message for one pane, as its view sends it.
fn in_pane(key: n0te_desktop::split::PaneKey, m: Message) -> Message {
    Message::Pane(key, Box::new(m))
}

/// A library with the demo notebook and a one-page "Übungen"; the demo open.
fn two_notebooks(tag: &str) -> (Harness, std::path::PathBuf, String, String) {
    let root = temp_root(tag);
    let lib = Library::open(&root).unwrap();
    let demo = demo::save_into(&lib);
    let nb = lib.create_notebook("Übungen").unwrap();
    nb.add_sheet(None, 0, &n0te_core::notebook::SheetSpec::a4(n0te_core::notebook::Background::Grid)).unwrap();
    lib.save_notebook(&nb).unwrap();
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(1600.0, 900.0), 1.0).unwrap();
    h.message(Message::Open(demo.clone()));
    h.events(&[]);
    h.events(&[]);
    (h, root, demo, nb.id())
}

#[test]
fn the_same_notebook_twice_shares_one_document_and_one_undo() {
    use n0te_desktop::sidebar::Msg as Outline;
    use n0te_desktop::split;
    let (mut h, _root, demo, _) = two_notebooks("split-same");
    let o = open(&h);
    let last = *o.pages.mains().last().unwrap();
    let (last_sheet, last_number) = (o.pages.pages[last].placed.sheet_id.clone(), o.pages.pages[last].placed.number);
    h.message(Message::Jump(last_sheet));
    // ⧉ in the toolbar, then "Dieses Notizbuch nochmal": beside it, at the page in view.
    click_on(&mut h, split::button_id());
    assert!(split_of(&h).menu.is_some(), "the menu opened");
    click_on(&mut h, split::menu_id("again"));
    h.events(&[]);
    h.events(&[]);
    let s = split_of(&h);
    assert_eq!(s.panes.len(), 2);
    let (a, b) = (s.panes[0].key, s.panes[1].key);
    assert!(std::rc::Rc::ptr_eq(&s.panes[0].nb, &s.panes[1].nb), "one document");
    assert_eq!((s.active, s.main, s.menu.is_none()), (a, a, true), "the first stays active and ★");
    assert!(s.panes[1].camera.viewport.width > 0.0 && !s.panes[1].sidebar);
    assert_eq!(s.panes[1].pages.current_number(&s.panes[1].camera), last_number, "opened at the page in view");
    // A page added in A shows in B at once.
    let before = s.panes[1].pages.mains().len();
    h.message(Message::Outline(Outline::AddPage));
    let s = split_of(&h);
    assert_eq!((s.panes[0].pages.mains().len(), s.panes[1].pages.mains().len()), (before + 1, before + 1));
    // Deleted from B's outline (its own messages), brought back from the status bar: one
    // history for the notebook.
    let node = s.panes[1].rows.iter().find_map(|r| if let OutlineRow::Sheet { node, .. } = r { Some(node.clone()) } else { None }).unwrap();
    let pages = |h: &Harness| split_of(h).panes.iter().map(|p| p.rows.iter().filter(|r| matches!(r, OutlineRow::Sheet { .. })).count()).collect::<Vec<_>>();
    let n = pages(&h)[0];
    h.message(in_pane(b, Message::Outline(Outline::Delete(node.clone()))));
    h.message(in_pane(b, Message::Outline(Outline::Delete(node.clone()))));
    assert_eq!(pages(&h), vec![n - 1, n - 1], "gone in both panes");
    assert_eq!(h.app.undo_delete.as_deref(), Some(demo.as_str()));
    h.message(Message::Outline(Outline::Undo));
    assert_eq!(pages(&h), vec![n, n], "back in both");
    // A text box written in A shows in B.
    let first = split_of(&h).panes[0].pages.pages[0].placed.sheet_id.clone();
    h.message(Message::NewText(first.clone(), 60.0, 100.0));
    assert!(h.app.editing.as_ref().is_some_and(|e| e.pane == a));
    h.type_text("Satz 4.2");
    h.message(Message::DoneText);
    let p = &split_of(&h).panes[1];
    let r = p.pages.pages[p.pages.page_of_sheet(&first).unwrap()].placed.rect;
    let cam = n0te_desktop::pages::Camera { x: r.x0, y: r.y0, zoom: 1.0, viewport: Size::new(r.width(), r.height()), origin: Point::ORIGIN };
    assert!(p.pages.visible_texts(&cam).iter().any(|(_, _, t)| t.text == "Satz 4.2"), "the box shows in B");
    // Each pane searches on its own.
    h.message(in_pane(b, Message::SearchQuery("Grenzwert".into())));
    let s = split_of(&h);
    assert!(s.panes[1].search.active() && !s.panes[0].search.active());
}

#[test]
fn a_click_makes_a_pane_active_and_the_star_moves_only_when_clicked() {
    use n0te_desktop::split;
    let (mut h, _root, demo, other) = two_notebooks("split-active");
    h.message(Message::Split(split::Msg::Beside(other.clone(), None)));
    h.events(&[]);
    h.events(&[]);
    let s = split_of(&h);
    let (a, b) = (s.panes[0].key, s.panes[1].key);
    assert_eq!((s.panes[0].id.as_str(), s.panes[1].id.as_str()), (demo.as_str(), other.as_str()));
    // Side by side, half each: B's pages start right of the middle and reach the window's edge.
    let cb = s.panes[1].camera;
    assert!(cb.origin.x > 800.0 && (cb.origin.x + cb.viewport.width - 1600.0).abs() < 1.0, "{cb:?}");
    let za = s.panes[0].camera.zoom;
    // A click on B's page makes it the active one: the toolbar works on it.
    h.click(Point::new(cb.origin.x + cb.viewport.width / 2.0, cb.origin.y + 200.0));
    assert_eq!(split_of(&h).active, b);
    h.message(Message::Zoom(2.0));
    let s = split_of(&h);
    assert_eq!((s.panes[0].camera.zoom, s.panes[1].camera.zoom), (za, 2.0));
    // Page down pages B.
    let m = key(&h, Named::PageDown);
    h.message(m);
    let s = split_of(&h);
    assert_eq!(s.panes[0].pages.current_number(&s.panes[0].camera), 1);
    // ★ stays with A until B's star is clicked.
    assert_eq!(s.main, a);
    click_on(&mut h, split::head_id("main", 1));
    assert_eq!(split_of(&h).main, b);
    // Swapped: each pane keeps its ★ and its being active.
    h.message(Message::Split(split::Msg::Swap));
    let s = split_of(&h);
    assert_eq!((s.panes[0].key, s.panes[1].key, s.main, s.active), (b, a, b, b));
    // B alone (⤢); Esc shows both again, then closes the one beside ★, then the library.
    click_on(&mut h, split::head_id("alone", 0));
    assert_eq!(split_of(&h).maxed, Some(b));
    let esc = key(&h, Named::Escape);
    h.message(esc.clone());
    assert_eq!(split_of(&h).maxed, None);
    h.message(esc.clone());
    let s = split_of(&h);
    assert_eq!((s.panes.len(), s.panes[0].key, s.main, s.active), (1, b, b, b));
    h.message(esc);
    assert!(matches!(h.app.screen, Screen::Library));
}

#[test]
fn the_divider_snaps_and_dragged_to_an_edge_closes_that_pane() {
    use n0te_desktop::split;
    let (mut h, _root, _demo, other) = two_notebooks("split-divider");
    h.message(Message::Split(split::Msg::Beside(other.clone(), None)));
    h.events(&[]);
    let a = split_of(&h).panes[0].key;
    // Dragged a bit left of the middle, let go: it rests at a third.
    h.press_at(Point::new(800.0, 450.0));
    h.move_to(Point::new(560.0, 450.0));
    assert!(split_of(&h).dragging.is_some());
    h.release();
    let s = split_of(&h);
    assert_eq!((s.dragging, s.fraction), (None, 1.0 / 3.0));
    h.events(&[]);
    let cb = split_of(&h).panes[1].camera;
    assert!((cb.origin.x - 536.0).abs() < 1.0, "{cb:?}");
    // ✕ on the main pane: the other stays, now ★ and active.
    click_on(&mut h, split::head_id("close", 0));
    let s = split_of(&h);
    let b = s.panes[0].key;
    assert_eq!((s.panes.len(), s.main, s.active), (1, b, b));
    assert!(b != a);
    // Again beside it; dragged to the right edge it says it will close, and that pane goes.
    h.message(Message::Split(split::Msg::Beside(other.clone(), None)));
    h.events(&[]);
    let cut = (1600.0 * split_of(&h).fraction).floor();
    assert_eq!(cut, 533.0, "the divider stays where it was left");
    h.press_at(Point::new(cut, 450.0));
    h.move_to(Point::new(1560.0, 450.0));
    assert!(h.find("Loslassen schließt").is_some() || h.find("LOSLASSEN SCHLIESST").is_some(), "the pane says it closes");
    h.release();
    let s = split_of(&h);
    assert_eq!((s.panes.len(), s.panes[0].key, s.main, s.active), (1, b, b, b));
}

#[test]
fn live_events_reach_every_pane_of_their_notebook_and_the_main_pane_leads() {
    use n0te_desktop::live::LiveEvent;
    use n0te_desktop::split;
    let (mut h, root, demo, other) = two_notebooks("split-live");
    h.message(Message::Split(split::Msg::Again));
    h.events(&[]);
    h.events(&[]);
    let s = split_of(&h);
    let (a, b) = (s.panes[0].key, s.panes[1].key);
    let first = s.panes[0].pages.pages[0].placed.sheet_id.clone();
    let last = *s.panes[1].pages.mains().last().unwrap();
    let last_sheet = s.panes[1].pages.pages[last].placed.sheet_id.clone();
    h.message(in_pane(b, Message::Jump(last_sheet.clone())));
    let before_b = split_of(&h).panes[1].camera;
    // A stroke from the tablet and its frame show in both panes; following moves ★'s only.
    h.app.live_session = Some(n0te_desktop::app::LiveSession { device: "tab".into(), name: "Tablet".into(), follow_them: true, they_follow: false });
    let samples: Vec<u8> = [100.0f32, 600.0, 0.5, 0.0, 140.0, 610.0, 0.5, 8.0].iter().flat_map(|f| f.to_le_bytes()).collect();
    h.message(Message::Live(LiveEvent::Stroke { device: "tab".into(), notebook: demo.clone(), sheet: first.clone(), stroke: 1, brush: (2.0, 0.5, 0xFF111111, 0), samples, done: false }));
    h.message(Message::Live(LiveEvent::Presence { device: "tab".into(), notebook: demo.clone(), sheet: Some(first.clone()), viewport: [100.0, 500.0, 400.0, 700.0], zoom: 2.0, laser: None }));
    let s = split_of(&h);
    for p in &s.panes {
        assert_eq!((p.pages.live.borrow().len(), p.pages.remote.borrow().len()), (1, 1));
    }
    let ca = s.panes[0].camera;
    assert!((ca.zoom - ca.viewport.width / 300.0).abs() < 0.01, "A (★) follows: {ca:?}");
    assert_eq!(s.panes[1].camera, before_b, "B stays");
    // Where I look in the notebook: one frame, the main pane's; ★ on B, B's.
    assert_eq!(h.app.presence_args(&demo).unwrap()[2], first);
    h.message(Message::Split(split::Msg::Main(b)));
    assert_eq!(h.app.presence_args(&demo).unwrap()[2], last_sheet);
    // Summoned to the first page: the main pane goes there, A stays.
    let ca = split_of(&h).panes[0].camera;
    h.message(Message::Live(LiveEvent::Summoned { device: "owner".into(), notebook: demo.clone(), sheet: first.clone(), viewport: [0.0, 0.0, 300.0, 200.0] }));
    let s = split_of(&h);
    assert_eq!((s.panes[1].pages.current_number(&s.panes[1].camera), s.panes[0].camera), (1, ca));
    // The other notebook into B (from A's ⧉): ★ stays in that place, A speaks for the demo now.
    h.message(Message::Split(split::Msg::Beside(other.clone(), None)));
    let s = split_of(&h);
    let b2 = s.panes[1].key;
    assert_eq!((s.panes[1].id.as_str(), s.main, s.active), (other.as_str(), b2, a));
    assert_eq!(h.app.pane_showing(&demo), Some(a));
    // A change to the other notebook reaches its pane only.
    {
        let lib = Library::open(&root).unwrap();
        let n = lib.load_notebook(&other).unwrap();
        n.add_sheet(None, 1, &n0te_core::notebook::SheetSpec::a4(n0te_core::notebook::Background::Grid)).unwrap();
        lib.save_notebook(&n).unwrap();
    }
    let demo_pages = split_of(&h).panes[0].pages.total_pages;
    h.message(Message::Live(LiveEvent::Changed { notebook: other.clone(), index: true, sheets: vec![] }));
    let s = split_of(&h);
    assert_eq!((s.panes[0].pages.total_pages, s.panes[1].pages.total_pages), (demo_pages, 2));
    // The tablet opens the demo live: it comes into the main pane, sharing A's document.
    h.message(Message::Live(LiveEvent::Open { device: "tab".into(), notebook: demo.clone() }));
    let s = split_of(&h);
    assert_eq!((s.panes[0].id.as_str(), s.panes[1].id.as_str(), s.main), (demo.as_str(), demo.as_str(), s.panes[1].key));
    assert!(std::rc::Rc::ptr_eq(&s.panes[0].nb, &s.panes[1].nb));
}

#[test]
fn a_page_dragged_out_of_the_outline_opens_beside_at_that_page() {
    use n0te_desktop::sidebar;
    let (mut h, _root, demo, _) = two_notebooks("split-drag");
    let o = open(&h);
    let i = o.pages.mains()[2];
    let (node, number) = (o.pages.pages[i].placed.node.clone(), o.pages.pages[i].placed.number);
    let row = h.bounds_of(sidebar::page_id(&node)).expect("its row");
    h.press_at(row.center());
    h.move_to(Point::new(row.center().x + 40.0, row.center().y));
    h.move_to(Point::new(1400.0, 450.0));
    assert!(split_of(&h).drag.is_some(), "dragging");
    assert!(h.find("Daneben öffnen").is_some() || h.find("DANEBEN ÖFFNEN").is_some(), "where it would open");
    h.release();
    h.events(&[]);
    h.events(&[]);
    let s = split_of(&h);
    assert!(s.drag.is_none() && s.panes.len() == 2);
    assert_eq!(s.panes[1].id, demo);
    assert_eq!(s.panes[1].pages.current_number(&s.panes[1].camera), number, "at the page dragged");
    assert_eq!(s.panes[0].pages.current_number(&s.panes[0].camera), 1, "the first pane stays where it was");
}

#[test]
fn an_upright_window_stacks_the_panes() {
    let root = temp_root("split-upright");
    let demo = demo::save_into(&Library::open(&root).unwrap());
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(900.0, 1400.0), 1.0).unwrap();
    h.message(Message::Open(demo));
    h.events(&[]);
    h.message(Message::Split(n0te_desktop::split::Msg::Again));
    h.events(&[]);
    h.events(&[]);
    let s = split_of(&h);
    let (a, b) = (s.panes[0].camera, s.panes[1].camera);
    assert!(b.origin.y >= a.origin.y + a.viewport.height, "one above the other: {a:?} {b:?}");
    assert!((b.viewport.width - 900.0).abs() < 1.0 && b.viewport.height > 500.0, "the lower one gets the whole width: {b:?}");
}
