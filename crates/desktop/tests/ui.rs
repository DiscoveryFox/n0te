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
