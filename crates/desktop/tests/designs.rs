//! Designs, in a test binary (a process) of their own: the design is app-wide, and switching it
//! while the UI tests run beside it moved their buttons between finding and clicking them.

mod harness;

use harness::Harness;
use iced::Size;
use n0te_core::store::Library;
use n0te_desktop::App;
use n0te_desktop::app::{Message, Screen};

fn temp_root(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("n0te-designs-{tag}-{}-{}", std::process::id(), n0te_core::ids::new_id(9)))
}

fn open(h: &Harness) -> &n0te_desktop::app::Open {
    match &h.app.screen {
        Screen::Notebook(o) => o,
        Screen::Library => panic!("no notebook open"),
    }
}

/// A library with one empty lined page, opened.
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

/// The design tests take turns among themselves too.
static THEME: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn theme_and_gaps_change_and_the_view_follows() {
    let _turn = THEME.lock().unwrap_or_else(|e| e.into_inner());
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
fn a_design_from_an_ai_is_previewed_saved_applied_and_deleted() {
    use n0te_desktop::app::DesignRead;
    use n0te_desktop::theme;
    let _turn = THEME.lock().unwrap_or_else(|e| e.into_inner());
    let (mut h, root, _nb) = blank_notebook("design");
    h.app.designs_dir = root.join("designs");
    h.message(Message::Back);
    h.message(Message::ShowSettings(true));
    // Junk lists its problems; the AI's whole answer (prose, fences, a trailing comma) reads.
    h.message(Message::DesignText(Some("das ist kein Design".into())));
    assert!(matches!(&h.app.design_read, Some(DesignRead::Bad { errors }) if !errors.is_empty()));
    h.frame();
    h.message(Message::DesignDiscard);
    let answer = "Hier:\n```json\n{\"name\":\"Minze\",\"colors\":{\"background\":\"#0b1512\",\"surface\":\"#11201b\",\"text\":\"#e8f5ef\",\"accent\":\"#2dd4a0\"},\"decorations\":[\"stars\"],}\n```";
    h.message(Message::DesignText(Some(answer.into())));
    assert!(matches!(&h.app.design_read, Some(DesignRead::Ok { design, .. }) if design.name == "Minze"));
    h.frame();
    h.message(Message::DesignSave(true));
    assert!(h.app.design_read.is_none());
    assert_eq!(theme::p().id, "minze");
    assert_eq!(h.app.settings.theme, "minze");
    assert!(root.join("designs/minze.json").is_file());
    assert!(theme::imported().iter().any(|t| t.id == "minze" && !t.builtin));
    h.frame();
    // Deleting the design in use goes back to Nothing.
    h.message(Message::DesignDelete(Some("minze".into())));
    h.frame();
    h.message(Message::DesignDeleteConfirmed("minze".into()));
    assert_eq!(theme::p().id, "nothing");
    assert!(!root.join("designs/minze.json").exists());
    assert!(theme::imported().is_empty());
    // A dropped file that isn't a design says so.
    std::fs::write(root.join("big.json"), "x".repeat(n0te_core::design::MAX_BYTES + 1)).unwrap();
    h.message(Message::FileDropped(root.join("big.json")));
    assert!(h.app.design_read.is_none());
}

#[test]
fn every_design_draws_the_library_and_the_settings() {
    use n0te_desktop::theme;
    let _turn = THEME.lock().unwrap_or_else(|e| e.into_inner());
    let (mut h, _root, _nb) = blank_notebook("designs");
    h.message(Message::Back);
    for t in theme::builtins() {
        h.message(Message::SetTheme(t.id.as_str()));
        h.frame();
        h.message(Message::ShowSettings(true));
        h.frame();
        h.message(Message::ShowSettings(false));
    }
    h.message(Message::SetTheme("nothing"));
}
