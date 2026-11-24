//! n0te Tablet end to end (headless, no window): the mouse and pen events write strokes that
//! are stored in the library, undo / redo, the eraser, the lasso, finger pan and pinch.
//! This test binary runs the app as n0te Tablet (its own process, so the mode is set once).

mod harness;

use harness::Harness;
use iced::{Point, Rectangle, Size};
use n0te_core::store::Library;
use n0te_desktop::app::{Message, Screen};
use n0te_desktop::tablet::pen::{PenEvent, Phase, Sample};
use n0te_desktop::tablet::{Msg, Tool};
use n0te_desktop::{App, Mode, demo};

fn temp_root(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("n0te-tablet-{tag}-{}-{}", std::process::id(), n0te_core::ids::new_id(9)))
}

fn opened(tag: &str) -> Harness {
    assert!(n0te_desktop::set_mode(Mode::Tablet));
    let root = temp_root(tag);
    let id = demo::save_into(&Library::open(&root).unwrap());
    // wgpu draws ink meshes (tiny-skia doesn't); without a GPU the logic still runs on tiny-skia.
    let size = Size::new(1400.0, 900.0);
    let mut h = Harness::new(App::new(&root), "wgpu", size, 1.0).or_else(|| Harness::new(App::new(&root), "tiny-skia", size, 1.0)).expect("a renderer");
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

/// The first main page: (notebook, sheet id, window point at page point (fx, fy) of its size).
fn first_page(h: &Harness, fx: f32, fy: f32) -> (String, String, Point) {
    let o = open(h);
    let p = o.pages.pages.iter().find(|p| p.placed.side == 0 && !p.placed.is_board()).expect("a page");
    let r = p.placed.rect;
    let cam = o.camera;
    let at = cam.to_screen(Rectangle::new(cam.origin, cam.viewport), r.x0 + r.width() * fx, r.y0 + r.height() * fy);
    (o.id.clone(), p.placed.sheet_id.clone(), at)
}

fn strokes(h: &Harness, nb: &str, sheet: &str) -> Vec<n0te_core::sheet::Stroke> {
    h.app.lib.load_sheet(nb, sheet).unwrap().strokes()
}

fn drag(h: &mut Harness, from: Point, to: Point, steps: usize) {
    h.press_at(from);
    for i in 1..=steps {
        let t = i as f32 / steps as f32;
        h.move_to(Point::new(from.x + (to.x - from.x) * t, from.y + (to.y - from.y) * t));
    }
    h.release();
}

fn shots() -> std::path::PathBuf {
    let d = std::path::PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("tablet-shots");
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn a_mouse_drag_writes_a_stroke_undo_and_redo() {
    let mut h = opened("ink");
    let (nb, sheet, a) = first_page(&h, 0.3, 0.6);
    let before = strokes(&h, &nb, &sheet).len();
    drag(&mut h, a, Point::new(a.x + 160.0, a.y + 30.0), 12);
    let after = strokes(&h, &nb, &sheet);
    assert_eq!(after.len(), before + 1, "one stroke stored");
    let s = after.iter().max_by_key(|s| s.points.len()).unwrap();
    assert!(s.points.len() >= 8, "samples kept: {}", s.points.len());
    h.screenshot(shots().join("tablet-01-ink.png"));
    h.message(Message::Tablet(Msg::Undo));
    assert_eq!(strokes(&h, &nb, &sheet).len(), before, "undo takes it back");
    h.message(Message::Tablet(Msg::Redo));
    assert_eq!(strokes(&h, &nb, &sheet).len(), before + 1, "redo brings it again");
}

#[test]
fn pen_events_carry_pressure_and_the_eraser_end_erases() {
    let mut h = opened("pen");
    let (nb, sheet, a) = first_page(&h, 0.5, 0.75);
    let old: Vec<String> = strokes(&h, &nb, &sheet).into_iter().map(|s| s.id).collect();
    let before = old.len();
    let sample = |x: f32, p: f32, t: u32| Sample { x: a.x + x, y: a.y, pressure: p, tilt: (0.0, 0.0), t_ms: t };
    let ev = |phase, samples: Vec<Sample>, eraser| PenEvent { phase, samples, eraser, barrel: false, real: true };
    // A ballpoint, so the stored pressure is the pen's (the fountain pen reshapes it).
    h.message(Message::Tablet(Msg::Pen(n0te_core::pens::PenType::Ballpoint)));
    h.message(Message::Pen(ev(Phase::Down, vec![sample(0.0, 0.2, 1000)], false)));
    h.message(Message::Pen(ev(Phase::Move, (1..20).map(|i| sample(i as f32 * 6.0, 0.2 + i as f32 * 0.04, 1000 + i * 8)).collect(), false)));
    h.message(Message::Pen(ev(Phase::Up, vec![sample(120.0, 0.95, 1170)], false)));
    let after = strokes(&h, &nb, &sheet);
    assert_eq!(after.len(), before + 1);
    let s = after.iter().find(|s| !old.contains(&s.id)).expect("the new stroke");
    let (p0, p1) = (s.points.first().unwrap().pressure, s.points.last().unwrap().pressure);
    assert!(p0 < 0.3 && p1 > 0.8, "pressure follows the pen: {p0} → {p1}");
    assert!(s.points[5].t_ms > s.points[1].t_ms, "times count up");
    // The eraser end across it takes the whole stroke.
    h.message(Message::Pen(ev(Phase::Down, vec![sample(60.0, 0.5, 2000)], true)));
    h.message(Message::Pen(ev(Phase::Move, vec![sample(61.0, 0.5, 2010)], true)));
    h.message(Message::Pen(ev(Phase::Up, vec![sample(62.0, 0.5, 2020)], true)));
    assert_eq!(strokes(&h, &nb, &sheet).len(), before, "erased");
}

#[test]
fn the_lasso_selects_and_delete_removes() {
    let mut h = opened("lasso");
    let (nb, sheet, a) = first_page(&h, 0.4, 0.85);
    let before = strokes(&h, &nb, &sheet).len();
    drag(&mut h, a, Point::new(a.x + 80.0, a.y), 6);
    assert_eq!(strokes(&h, &nb, &sheet).len(), before + 1);
    h.message(Message::Tablet(Msg::Tool(Tool::Lasso)));
    // A loop around the new stroke.
    let pts = [(-20.0, -20.0), (100.0, -20.0), (100.0, 20.0), (-20.0, 20.0), (-20.0, -19.0)];
    h.press_at(Point::new(a.x + pts[0].0, a.y + pts[0].1));
    for (x, y) in &pts[1..] {
        h.move_to(Point::new(a.x + x, a.y + y));
    }
    h.release();
    let sel = h.app.tablet.selection.clone().expect("something selected");
    assert_eq!(sel.ids.len(), 1, "just the new stroke");
    h.screenshot(shots().join("tablet-02-lasso.png"));
    h.message(Message::Tablet(Msg::DeleteSelection));
    assert_eq!(strokes(&h, &nb, &sheet).len(), before, "deleted");
}

#[test]
fn one_finger_pans_two_fingers_zoom() {
    use iced::touch::{Event as T, Finger};
    let mut h = opened("touch");
    let c0 = open(&h).camera;
    let mid = Point::new(c0.origin.x + c0.viewport.width / 2.0, c0.origin.y + c0.viewport.height / 2.0);
    h.events(&[iced::Event::Touch(T::FingerPressed { id: Finger(1), position: mid })]);
    h.events(&[iced::Event::Touch(T::FingerMoved { id: Finger(1), position: Point::new(mid.x, mid.y - 120.0) })]);
    h.events(&[iced::Event::Touch(T::FingerLifted { id: Finger(1), position: Point::new(mid.x, mid.y - 120.0) })]);
    let c1 = open(&h).camera;
    assert!((c1.y - c0.y) * c0.zoom > 100.0, "panned down the page: {} → {}", c0.y, c1.y);
    let (a, b) = (Point::new(mid.x - 50.0, mid.y), Point::new(mid.x + 50.0, mid.y));
    h.events(&[iced::Event::Touch(T::FingerPressed { id: Finger(2), position: a }), iced::Event::Touch(T::FingerPressed { id: Finger(3), position: b })]);
    h.events(&[iced::Event::Touch(T::FingerMoved { id: Finger(3), position: Point::new(mid.x + 150.0, mid.y) })]);
    h.events(&[iced::Event::Touch(T::FingerLifted { id: Finger(2), position: a }), iced::Event::Touch(T::FingerLifted { id: Finger(3), position: b })]);
    let c2 = open(&h).camera;
    assert!(c2.zoom > c1.zoom * 1.5, "zoomed in: {} → {}", c1.zoom, c2.zoom);
}

#[test]
fn a_selection_moves_copies_and_pastes() {
    let mut h = opened("move");
    let (nb, sheet, a) = first_page(&h, 0.3, 0.8);
    let old: Vec<String> = strokes(&h, &nb, &sheet).into_iter().map(|s| s.id).collect();
    drag(&mut h, a, Point::new(a.x + 60.0, a.y), 6);
    let new_id = strokes(&h, &nb, &sheet).into_iter().find(|s| !old.contains(&s.id)).unwrap().id;
    h.message(Message::Tablet(Msg::Tool(Tool::Lasso)));
    let pts = [(-15.0, -15.0), (75.0, -15.0), (75.0, 15.0), (-15.0, 15.0), (-15.0, -14.0)];
    h.press_at(Point::new(a.x + pts[0].0, a.y + pts[0].1));
    for (x, y) in &pts[1..] {
        h.move_to(Point::new(a.x + x, a.y + y));
    }
    h.release();
    assert!(h.app.tablet.selection.is_some());
    let x_before = strokes(&h, &nb, &sheet).into_iter().find(|s| s.id == new_id).unwrap().bounds.x0;
    // Drag from inside the selection: it moves 50 px right (zoom 1 here: 50 points, roughly).
    drag(&mut h, Point::new(a.x + 30.0, a.y), Point::new(a.x + 80.0, a.y), 5);
    let zoom = open(&h).camera.zoom;
    let x_after = strokes(&h, &nb, &sheet).into_iter().find(|s| s.id == new_id).unwrap().bounds.x0;
    assert!(((x_after - x_before) - 50.0 / zoom).abs() < 2.0, "moved by 50 px: {x_before} → {x_after} (zoom {zoom})");
    let n = strokes(&h, &nb, &sheet).len();
    h.message(Message::Tablet(Msg::Copy));
    h.message(Message::Tablet(Msg::Paste));
    let total: usize = open(&h).pages.pages.iter().map(|p| strokes(&h, &nb, &p.placed.sheet_id).len()).sum();
    assert!(total > n, "pasted somewhere in view");
    // Undo the paste, the move: back to where it was.
    h.message(Message::Tablet(Msg::Undo));
    h.message(Message::Tablet(Msg::Undo));
    let x_back = strokes(&h, &nb, &sheet).into_iter().find(|s| s.id == new_id).unwrap().bounds.x0;
    assert!((x_back - x_before).abs() < 0.5, "undo moves it back: {x_back} vs {x_before}");
}

#[test]
fn the_text_tool_makes_a_text_box_where_tapped() {
    let mut h = opened("text");
    let (_, _, a) = first_page(&h, 0.5, 0.5);
    h.message(Message::Tablet(Msg::Tool(Tool::Text)));
    h.click(a);
    h.events(&[]);
    assert!(h.app.editing.is_some(), "a text box is being edited");
}
