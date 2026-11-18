//! End-to-end UI tests through iced's real event path (headless tiny-skia, no window):
//! key presses go into the focused `text_editor`, our key bindings turn them into block
//! operations, focus moves via widget operations, wheel events pan/zoom the page canvas.

mod harness;

use harness::Harness;
use iced::keyboard::key::Named;
use iced::widget::text_editor::{Cursor, Position};
use iced::{Point, Size};
use n0te_desktop::app::{Message, Mode};
use n0te_desktop::blocks::{Kind, editor_id};
use n0te_desktop::{App, Message as M};

fn harness() -> Harness {
    Harness::new(App::new(), "tiny-skia", Size::new(1600.0, 900.0), 1.0).expect("tiny-skia renderer")
}

/// Opens the text page and focuses block `i` with the cursor at (line, column).
fn focus(h: &mut Harness, i: usize, line: usize, column: usize) -> u64 {
    h.message(Message::SetMode(Mode::Text));
    let id = h.app.doc.blocks[i].id;
    let task = h.app.doc.focus_block(id, Cursor { position: Position { line, column }, selection: None });
    h.task(task.map(M::Blocks));
    id
}

fn text_of(h: &Harness, i: usize) -> String {
    h.app.doc.blocks[i].text()
}

#[test]
fn typing_german_text_and_enter_split_via_key_events() {
    let mut h = harness();
    let n = h.app.doc.blocks.len();
    let last = n - 1; // the demo ends with an empty paragraph
    focus(&mut h, last, 0, 0);
    h.type_text("Übungsblatt für Mäßigkeit: ä ö ü ß – „Zitat“ € @");
    assert_eq!(text_of(&h, last), "Übungsblatt für Mäßigkeit: ä ö ü ß – „Zitat“ € @");
    // Enter splits; typing continues in the new block (focus moved by a widget operation).
    h.press(Named::Enter);
    assert_eq!(h.app.doc.blocks.len(), n + 1);
    h.type_text("Neue Zeile");
    assert_eq!(text_of(&h, last + 1), "Neue Zeile");
    // Backspace at the start merges back.
    for _ in 0.."Neue Zeile".chars().count() {
        h.press(Named::ArrowLeft);
    }
    h.press(Named::Backspace);
    assert_eq!(h.app.doc.blocks.len(), n);
    assert_eq!(text_of(&h, last), "Übungsblatt für Mäßigkeit: ä ö ü ß – „Zitat“ € @Neue Zeile");
}

#[test]
fn markdown_shortcuts_through_the_editor() {
    let mut h = harness();
    let last = h.app.doc.blocks.len() - 1;
    focus(&mut h, last, 0, 0);
    h.type_text("## Zwischentitel");
    assert_eq!(h.app.doc.blocks[last].kind, Kind::Heading(2));
    assert_eq!(text_of(&h, last), "Zwischentitel");
    h.press(Named::Enter);
    h.type_text("```python");
    h.press(Named::Enter);
    assert_eq!(h.app.doc.blocks[last + 1].kind, Kind::Code { lang: "python".into() });
    // Typing continues inside the (re-created, highlighted) code editor.
    h.type_text("def f(x):");
    h.press(Named::Enter); // a line break inside code
    h.type_text("    return x");
    assert_eq!(text_of(&h, last + 1), "def f(x):\n    return x");
}

#[test]
fn arrow_keys_move_between_blocks() {
    let mut h = harness();
    // Block 1 is a long paragraph; block 0 the heading.
    focus(&mut h, 1, 0, 3);
    h.press(Named::ArrowUp);
    let heading = h.app.doc.blocks[0].id;
    assert_eq!(h.app.doc.focus, Some(heading));
    h.type_text("X");
    assert!(text_of(&h, 0).starts_with("MitX") || text_of(&h, 0).contains('X'), "{}", text_of(&h, 0));
    h.press(Named::ArrowDown);
    assert_eq!(h.app.doc.focus, Some(h.app.doc.blocks[1].id));
}

#[test]
fn up_in_a_wrapped_paragraph_stays_in_the_block() {
    let mut h = harness();
    // Block 1 wraps onto two visual lines at this width; its text is one logical line.
    let text = text_of(&h, 1);
    assert!(!text.contains('\n'));
    let end = text.len();
    let id = focus(&mut h, 1, 0, end);
    h.press(Named::ArrowUp);
    assert_eq!(h.app.doc.focus, Some(id), "first Up moves to the first visual line");
    h.press(Named::ArrowUp);
    assert_eq!(h.app.doc.focus, Some(h.app.doc.blocks[0].id), "second Up leaves the block");
}

#[test]
fn vertical_motion_keeps_glyph_index_not_x_position() {
    // Documents a cosmic-text 0.15 quirk: Up/Down keep the *glyph index*, not the x position,
    // so with a proportional font the caret jumps sideways between lines of different widths.
    let mut h = harness();
    h.message(Message::SetMode(Mode::Text));
    h.app.doc = n0te_desktop::blocks::Doc::new(&[("", "WWWWWWWWWWWW\niiiiiiiiiiii")]);
    let id = focus(&mut h, 0, 1, 10);
    h.press(Named::ArrowUp);
    let pos = h.app.doc.blocks[0].content.cursor().position;
    assert_eq!(h.app.doc.focus, Some(id));
    assert_eq!(pos, Position { line: 0, column: 10 }, "x-position semantics would land near column 3");
}

#[test]
fn clipboard_select_all_copy_paste_and_undo() {
    let mut h = harness();
    let last = h.app.doc.blocks.len() - 1;
    focus(&mut h, 2, 0, 0); // "Satz (Mittelwertsatz)"
    h.ctrl('a');
    h.ctrl('c');
    assert_eq!(h.clipboard.standard.as_deref(), Some("Satz (Mittelwertsatz)"));
    focus(&mut h, last, 0, 0);
    h.ctrl('v');
    assert_eq!(text_of(&h, last), "Satz (Mittelwertsatz)");
    h.ctrl('z');
    assert_eq!(text_of(&h, last), "");
    h.ctrl('y');
    assert_eq!(text_of(&h, last), "Satz (Mittelwertsatz)");
}

#[test]
fn ime_commit_and_click_focus() {
    let mut h = harness();
    let last = h.app.doc.blocks.len() - 1;
    h.message(Message::SetMode(Mode::Text));
    // Click into block 3 (a paragraph) through the real mouse path.
    let id3 = h.app.doc.blocks[3].id;
    let b = h.bounds_of(editor_id(id3)).expect("block 3 on screen");
    h.click(Point::new(b.x + 4.0, b.y + b.height / 2.0));
    assert_eq!(h.app.doc.focus, Some(id3));
    // IME: pre-edit is shown by the shell, only the commit reaches the text.
    focus(&mut h, last, 0, 0);
    h.ime(&["ka", "かん"], "漢字");
    assert_eq!(text_of(&h, last), "漢字");
}

#[test]
fn wheel_pans_ctrl_wheel_zooms_and_sidebar_jumps() {
    let mut h = harness();
    let c0 = h.app.camera;
    assert!(c0.viewport.width > 0.0, "page view reported its size");
    let center = Point::new(900.0, 450.0);
    h.scroll(center, 0.0, -120.0);
    let c1 = h.app.camera;
    assert!((c1.y - c0.y - 120.0 / c0.zoom).abs() < 0.01, "{c0:?} -> {c1:?}");
    // Ctrl + wheel zooms around the cursor: the page point under it stays put.
    h.set_modifiers(iced::keyboard::Modifiers::CTRL);
    let before = Point::new(c1.x + (center.x - 273.0) / c1.zoom, c1.y + (center.y - 53.0) / c1.zoom);
    h.scroll(center, 0.0, 100.0);
    h.set_modifiers(iced::keyboard::Modifiers::empty());
    let c2 = h.app.camera;
    assert!(c2.zoom > c1.zoom);
    let after = Point::new(c2.x + (center.x - 273.0) / c2.zoom, c2.y + (center.y - 53.0) / c2.zoom);
    assert!((before.x - after.x).abs() < 0.5 && (before.y - after.y).abs() < 0.5, "{before:?} vs {after:?}");
    // Jump to the last main page from the outline.
    let last_sheet = h.app.pages.pages[*h.app.pages.mains.last().unwrap()].sheet_id.clone();
    h.message(Message::Jump(last_sheet));
    assert_eq!(h.app.pages.current_number(&h.app.camera), h.app.pages.mains.len());
}

#[test]
fn several_wheel_events_in_one_batch_all_count() {
    use iced::mouse::{Event as Mouse, ScrollDelta};
    let mut h = harness();
    let c0 = h.app.camera;
    h.cursor = iced::mouse::Cursor::Available(Point::new(900.0, 450.0));
    let wheel = |y| iced::Event::Mouse(Mouse::WheelScrolled { delta: ScrollDelta::Pixels { x: 0.0, y } });
    h.events(&[wheel(-40.0), wheel(-40.0), wheel(-40.0)]);
    assert!((h.app.camera.y - c0.y - 120.0 / c0.zoom).abs() < 0.01, "{c0:?} -> {:?}", h.app.camera);
}

#[test]
fn outline_groups_collapse_and_expand() {
    let mut h = harness();
    let before = h.app.rows.len();
    let group = h
        .app
        .rows
        .iter()
        .find_map(|r| match r {
            n0te_core::notebook::OutlineRow::Group { node, title, .. } if title == "Vorlesungen" => Some(node.clone()),
            _ => None,
        })
        .expect("group");
    h.message(Message::ToggleGroup(group.clone()));
    assert_eq!(h.app.rows.len(), before - 12);
    h.message(Message::ToggleGroup(group));
    assert_eq!(h.app.rows.len(), before);
}
