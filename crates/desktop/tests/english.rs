//! n0te in English, end to end through the headless harness (its own test binary: the
//! language is app-wide, and `tests/ui.rs` checks the German texts): the library and its
//! dialogs, the demo notebook, the palette, the shortcuts, the settings, the page panel, the
//! export and the graph editor show English, and nothing panics on the way.

mod harness;

use harness::Harness;
use iced::Size;
use iced::keyboard::key::Named;
use n0te_core::store::Library;
use n0te_desktop::app::{Message, Screen};
use n0te_desktop::i18n::{self, Lang, tr};
use n0te_desktop::{App, demo, theme};

fn temp_root(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("n0te-en-{tag}-{}-{}", std::process::id(), n0te_core::ids::new_id(9)))
}

/// A library with the demo notebook, in English; the notebook's id.
fn english_library(tag: &str) -> (Harness, String) {
    i18n::set(Lang::En);
    let root = temp_root(tag);
    let id = demo::save_into(&Library::open(&root).unwrap());
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(1600.0, 900.0), 1.0).expect("tiny-skia renderer");
    h.frame();
    (h, id)
}

/// Whether `label` shows, in whatever case the design writes it (buttons shout in the mono
/// designs, dot headings whisper).
fn shows(h: &mut Harness, label: &str) -> bool {
    [label.to_string(), theme::cased(label), label.to_uppercase(), label.to_lowercase()].iter().any(|l| h.find(l.as_str()).is_some())
}

/// Esc as the keyboard subscription delivers it (the harness doesn't run subscriptions).
fn escape(h: &mut Harness) {
    let iced::Event::Keyboard(e) = h.key_event(iced::keyboard::Key::Named(Named::Escape), None)[0].clone() else { unreachable!() };
    h.message(Message::Key(e));
}

#[track_caller]
fn assert_shows(h: &mut Harness, labels: &[&str]) {
    let missing: Vec<&&str> = labels.iter().filter(|l| !shows(h, l)).collect();
    assert!(missing.is_empty(), "not on screen: {missing:?}");
}

#[track_caller]
fn assert_hidden(h: &mut Harness, labels: &[&str]) {
    let there: Vec<&&str> = labels.iter().filter(|l| shows(h, l)).collect();
    assert!(there.is_empty(), "German on screen: {there:?}");
}

#[test]
fn the_table_answers_in_english() {
    i18n::set(Lang::En);
    assert_eq!(tr("Einstellungen"), "Settings");
    assert_eq!(i18n::trf("{} Seiten", &[&3]), "3 pages");
    assert_eq!(i18n::trc("paper", "Punkte"), "Dots");
    assert_eq!(tr("Punkte"), "Points");
    assert_eq!(i18n::number(8.5, 1), "8.5");
    assert_eq!(n0te_desktop::sidebar::format_name(842.0, 595.0), "A4 landscape");
    assert_eq!(n0te_desktop::plots::command_name("Schneide"), "Intersect");
}

#[test]
fn the_library_and_its_dialogs_speak_english() {
    let (mut h, _) = english_library("library");
    assert_shows(&mut h, &["Settings", "+ New notebook", "Invite", "Home", "All notebooks", "Library"]);
    assert_hidden(&mut h, &["Einstellungen", "+ Neues Notizbuch", "Bibliothek"]);
    // A new notebook: its paper (dots, not points) and format.
    h.message(Message::StartCreate);
    h.frame();
    assert_shows(&mut h, &["New notebook", "Create", "Cancel", "Paper", "Blank", "Lines", "Grid", "Dots", "Portrait", "Landscape", "Square", "Custom …"]);
    assert_hidden(&mut h, &["Anlegen", "Punkte", "Points"]);
    h.message(Message::CancelCreate);
    // Joining with an invite.
    h.message(Message::StartJoin);
    h.frame();
    assert_shows(&mut h, &["Open an invite", "Join"]);
    h.message(Message::CancelJoin);
    // The shortcuts.
    h.message(Message::ShowHelp(true));
    h.frame();
    assert_shows(&mut h, &["Keyboard shortcuts", "Ctrl K", "Search commands, pages, notebooks", "Right-click", "Close"]);
    assert_hidden(&mut h, &["Tastenkürzel", "Strg K"]);
    h.message(Message::ShowHelp(false));
    h.frame();
}

#[test]
fn the_palette_offers_english_commands() {
    let (mut h, id) = english_library("palette");
    h.message(Message::OpenPalette);
    h.frame();
    assert_shows(&mut h, &["New notebook", "Import PDF", "Ctrl N", "Settings", "Keyboard shortcuts"]);
    // Filtered by English words; the themes by their English names.
    h.type_text("theme");
    h.frame();
    assert_shows(&mut h, &["Theme: Light", "Theme: Ocean", "Theme: Forest"]);
    escape(&mut h);
    assert!(h.app.palette.is_none());
    // In a notebook: its commands, found in English, run in English.
    h.message(Message::Open(id));
    h.events(&[]);
    h.message(Message::OpenPalette);
    h.type_text("rotate");
    h.frame();
    assert_shows(&mut h, &["Rotate page", "Ctrl R · portrait ↔ landscape"]);
    escape(&mut h);
    h.message(Message::OpenPalette);
    h.type_text("new text page");
    h.message(Message::PaletteRun(None));
    h.frame();
    assert!(matches!(&h.app.screen, Screen::Notebook(o) if o.text_page.is_some()), "\"New text page\" ran");
    assert_shows(&mut h, &["SAVED", "Close"]);
    h.message(Message::CloseTextPage);
    h.frame();
}

#[test]
fn the_demo_notebook_speaks_english() {
    let (mut h, id) = english_library("notebook");
    h.message(Message::Open(id));
    h.events(&[]);
    h.events(&[]);
    h.frame();
    assert!(matches!(h.app.screen, Screen::Notebook(_)));
    assert_shows(&mut h, &["‹ Library", "+ Text page", "+ Graph", "Notes", "Insert PDF", "Share", "Outline", "+ Page", "+ Group", "Ctrl K: commands · F1: shortcuts"]);
    assert_hidden(&mut h, &["‹ Bibliothek", "+ Textseite", "Gliederung", "Teilen"]);
    // The page in view: its paper and format.
    h.message(Message::ShowPage(true));
    h.frame();
    assert_shows(&mut h, &["Close"]);
    assert!(shows(&mut h, "Apply to all pages") || shows(&mut h, "A board is endless, with a dot grid.") || shows(&mut h, "Paper"), "the page panel in English");
    h.message(Message::ShowPage(false));
    // Export.
    h.message(Message::ShowExport(true));
    h.frame();
    assert_shows(&mut h, &["Export PDF", "Range", "Whole notebook", "This page", "Selection", "Export", "Cancel"]);
    assert_hidden(&mut h, &["Ganzes Notizbuch", "Exportieren"]);
    h.message(Message::ShowExport(false));
    // Back to the library, which now knows it was opened.
    h.message(Message::Back);
    h.frame();
    assert!(matches!(h.app.screen, Screen::Library));
    assert_shows(&mut h, &["Settings"]);
}

#[test]
fn the_settings_speak_english() {
    let (mut h, _) = english_library("settings");
    h.message(Message::ShowSettings(true));
    h.frame();
    assert_shows(&mut h, &["Settings", "Theme", "Spacing", "Between pages", "Text", "Language", "Automatic", "Deutsch", "English", "Devices", "Light", "Ocean", "Forest", "Close"]);
    let footer = format!("{} {} · Saved in {}", n0te_desktop::app_name(), n0te_desktop::updater::VERSION, n0te_desktop::settings::path().display());
    assert_shows(&mut h, &[footer.as_str(), "Soft and calm: surfaces, not lines"]);
    assert_hidden(&mut h, &["Abstände", "Sprache", "Geräte", "Hell", "Ozean"]);
    h.message(Message::ShowSettings(false));
    h.frame();
}

#[test]
fn the_graph_editor_speaks_english_and_takes_english_commands() {
    i18n::set(Lang::En);
    let root = temp_root("graph");
    let lib = Library::open(&root).unwrap();
    let nb = lib.create_notebook("Functions").unwrap();
    nb.add_sheet(None, 0, &n0te_core::notebook::SheetSpec { spacing: 17.0, ..n0te_core::notebook::SheetSpec::a4(n0te_core::notebook::Background::Grid) }).unwrap();
    lib.save_notebook(&nb).unwrap();
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(1600.0, 900.0), 1.0).expect("tiny-skia renderer");
    h.message(Message::Open(nb.id()));
    h.events(&[]);
    h.events(&[]);
    h.message(Message::InsertPlot);
    h.frame();
    assert!(h.app.plot_edit.is_some(), "the graph's panel is open");
    assert_shows(&mut h, &["Graph", "Properties", "Graphics", "Delete graph", "Cancel", "Point", "Line", "Circle", "Intersect", "Tangent", "Slider"]);
    assert_shows(&mut h, &["Empty. Type below, e.g. f(x) = x^2 or A = (1|2), then Enter.", "For example f(x) = x^2 − 3, A = (1|2) or a = 2. The line suggests commands as you type."]);
    assert_hidden(&mut h, &["Koordinatensystem", "Schneide", "Tangente"]);
    // A function, then a command button: it writes GeoGebra's English name, which the core takes.
    h.type_text("x^2");
    h.press(Named::Enter);
    let tangent = h.find(theme::cased("Tangent").as_str()).expect("the Tangent button");
    h.click(tangent.center());
    assert_eq!(h.app.plot_edit.as_ref().unwrap().input, "Tangent()");
    h.frame();
    assert_shows(&mut h, &["Tangent(1, f): the tangent to f at x = 1 · Tangent(A, f) · Tangent(A, circle)"]);
    h.type_text("1, f");
    h.press(Named::Enter);
    let e = h.app.plot_edit.as_ref().unwrap();
    assert_eq!(e.plot.objs.len(), 2, "f and its tangent");
    assert!(e.plot.errors().iter().all(Option::is_none), "{:?}", e.plot.errors());
    // Suggestions while typing: the English names first.
    h.type_text("inter");
    let names: Vec<&str> = h.app.plot_edit.as_ref().unwrap().suggestions().iter().map(|s| s.name).collect();
    assert_eq!(names.first(), Some(&"Intersect"), "{names:?}");
    h.frame();
    assert_shows(&mut h, &["the intersections"]);
    h.press(Named::Escape);
    h.frame();
}
