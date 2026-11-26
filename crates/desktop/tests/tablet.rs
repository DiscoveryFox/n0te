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

/// The strokes stored on a sheet (after the background saves are done).
fn strokes(h: &Harness, nb: &str, sheet: &str) -> Vec<n0te_core::sheet::Stroke> {
    assert!(n0te_desktop::saver::flush(), "pages stored");
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

/// Lassos the strokes in the box (window points) around `a`.
fn lasso_around(h: &mut Harness, a: Point, w: f32) {
    h.message(Message::Tablet(Msg::Tool(Tool::Lasso)));
    let pts = [(-15.0, -15.0), (w + 15.0, -15.0), (w + 15.0, 15.0), (-15.0, 15.0), (-15.0, -14.0)];
    h.press_at(Point::new(a.x + pts[0].0, a.y + pts[0].1));
    for (x, y) in &pts[1..] {
        h.move_to(Point::new(a.x + x, a.y + y));
    }
    h.release();
}

#[test]
fn a_move_the_system_cancels_leaves_the_selection_and_the_next_stroke_is_stored() {
    let mut h = opened("cancelmove");
    let (nb, sheet, a) = first_page(&h, 0.3, 0.8);
    drag(&mut h, a, Point::new(a.x + 60.0, a.y), 6);
    lasso_around(&mut h, a, 60.0);
    assert!(h.app.tablet.selection.is_some());
    let before = strokes(&h, &nb, &sheet);
    // The pen goes down on the selection and moves it, then Windows takes the pen away.
    let ev = |phase, x: f32, y: f32, t: u32| Message::Pen(PenEvent { phase, samples: vec![Sample { x, y, pressure: 0.5, tilt: (0.0, 0.0), t_ms: t }], eraser: false, barrel: false, real: true });
    h.message(ev(Phase::Down, a.x + 30.0, a.y, 100));
    h.message(ev(Phase::Move, a.x + 60.0, a.y + 20.0, 120));
    h.message(Message::Pen(PenEvent { phase: Phase::Cancel, samples: Vec::new(), eraser: false, barrel: false, real: true }));
    assert!(!h.app.tablet.busy(), "nothing under way after the cancel");
    assert!(open(&h).pages.erasing.borrow().is_empty() && open(&h).pages.live.borrow().is_empty(), "the selection shows where it was");
    let unchanged = strokes(&h, &nb, &sheet);
    assert_eq!(unchanged.iter().map(|s| (s.id.clone(), s.bounds.x0)).collect::<Vec<_>>(), before.iter().map(|s| (s.id.clone(), s.bounds.x0)).collect::<Vec<_>>(), "nothing moved");
    // The next stroke (elsewhere) is written as usual.
    h.message(Message::Tablet(Msg::Tool(Tool::Pen)));
    let b = Point::new(a.x, a.y - 120.0);
    drag(&mut h, b, Point::new(b.x + 80.0, b.y), 6);
    assert_eq!(strokes(&h, &nb, &sheet).len(), before.len() + 1, "the new stroke is stored");
}

#[test]
fn undo_and_a_selection_stay_with_their_notebook() {
    let mut h = opened("forget");
    let (nb, sheet, a) = first_page(&h, 0.3, 0.8);
    drag(&mut h, a, Point::new(a.x + 60.0, a.y), 6);
    lasso_around(&mut h, a, 60.0);
    assert!(h.app.tablet.selection.is_some());
    let n = strokes(&h, &nb, &sheet).len();
    // Another notebook: Delete and undo there must not touch the first one.
    let other = {
        let lib = &h.app.lib;
        let o = lib.create_notebook("Zweites").unwrap();
        o.add_sheet(None, 0, &n0te_core::notebook::SheetSpec::a4(n0te_core::notebook::Background::Grid)).unwrap();
        lib.save_notebook(&o).unwrap();
        o.id()
    };
    h.message(Message::Back);
    h.message(Message::Open(other));
    h.events(&[]);
    let delete = match h.key_event(iced::keyboard::Key::Named(iced::keyboard::key::Named::Delete), None)[0].clone() {
        iced::Event::Keyboard(e) => e,
        _ => unreachable!(),
    };
    h.message(Message::Key(delete));
    h.message(Message::Tablet(Msg::Undo));
    h.message(Message::Tablet(Msg::Undo));
    assert_eq!(strokes(&h, &nb, &sheet).len(), n, "the first notebook is as it was");
    assert!(h.app.tablet.selection.is_none() && !h.app.tablet.can_undo());
}

#[test]
fn a_graph_inserted_and_changed_on_the_tablet_is_undone_step_by_step() {
    let mut h = opened("plotundo");
    let (nb, sheet, _) = first_page(&h, 0.5, 0.5);
    let plots = |h: &Harness| {
        assert!(n0te_desktop::saver::flush());
        h.app.lib.load_sheet(&nb, &sheet).unwrap().plots()
    };
    let before = plots(&h).len();
    h.message(Message::InsertPlot);
    assert!(h.app.plot_edit.is_some(), "its panel opened");
    h.message(Message::Plot(n0te_desktop::plots::Msg::Cancel));
    assert_eq!(plots(&h).len(), before + 1, "stored at once");
    // The new graph opens again from the page at once (its file may still be on its way).
    let id = plots(&h).last().unwrap().id.clone();
    h.message(Message::EditPlot(sheet.clone(), id));
    assert!(h.app.plot_edit.is_some());
    h.message(Message::Plot(n0te_desktop::plots::Msg::Cancel));
    h.message(Message::Tablet(Msg::Undo));
    assert_eq!(plots(&h).len(), before, "undo takes the graph away again");
    h.message(Message::Tablet(Msg::Redo));
    assert_eq!(plots(&h).len(), before + 1);
}

#[test]
fn a_selection_shows_its_actions_beside_it_and_they_act() {
    let mut h = opened("selbar");
    let (nb, sheet, a) = first_page(&h, 0.3, 0.6);
    drag(&mut h, a, Point::new(a.x + 60.0, a.y), 6);
    let n = strokes(&h, &nb, &sheet).len();
    lasso_around(&mut h, a, 60.0);
    let (_, bar) = h.app.selection_bar().expect("the selection's bar");
    assert!(bar.y + bar.height < a.y, "above the selection: {bar:?} vs {a:?}");
    let delete = n0te_desktop::theme::cased(n0te_desktop::i18n::tr("Löschen"));
    let at = h.find(delete.as_str()).expect("Löschen in the bar");
    assert!(bar.contains(at.center()), "it is the bar's");
    h.click(at.center());
    assert_eq!(strokes(&h, &nb, &sheet).len(), n - 1, "deleted");
    assert!(h.app.tablet.selection.is_none() && h.app.selection_bar().is_none());
}

#[test]
fn the_tool_bar_fits_a_small_pen_laptop() {
    use n0te_desktop::i18n::tr;
    use n0te_desktop::theme::cased;
    let mut h = opened("narrowbar");
    let width = 1200.0;
    h.size = Size::new(width, 800.0);
    h.events(&[]);
    h.events(&[]);
    for (tool, labels) in [(Tool::Pen, &["Laser", "Lineal", "Lupe"][..]), (Tool::Eraser, &["Laser", "Fläche", "Lineal", "Lupe"][..])] {
        h.message(Message::Tablet(Msg::Tool(tool)));
        h.events(&[]);
        for l in labels {
            let r = h.find(cased(tr(l)).as_str()).unwrap_or_else(|| panic!("{l} shows"));
            assert!(r.x + r.width <= width, "{l} fits: {r:?}");
        }
    }
    // The bar keeps its height whatever the tool: the pages don't jump.
    let top = open(&h).camera.origin.y;
    h.message(Message::Tablet(Msg::Tool(Tool::Pen)));
    h.events(&[]);
    assert_eq!(open(&h).camera.origin.y, top);
}

#[test]
fn n0te_tablet_writes_with_a_peer_id_of_its_own() {
    // n0te Laptop and n0te Tablet may run at once on one machine: one Loro peer id for both
    // would make clashing edits.
    let h = opened("peer");
    let laptop = Library::open(&h.app.root).unwrap().peer();
    assert_ne!(h.app.lib.peer(), laptop);
    assert_eq!(h.app.lib.peer(), Library::open_as(&h.app.root, "tablet").unwrap().peer());
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

#[test]
fn holding_the_pen_still_makes_a_wobbly_line_straight() {
    let mut h = opened("shape");
    let (nb, sheet, a) = first_page(&h, 0.2, 0.7);
    let old: Vec<String> = strokes(&h, &nb, &sheet).into_iter().map(|s| s.id).collect();
    h.press_at(a);
    for i in 1..=30 {
        let wobble = if i % 2 == 0 { 2.5 } else { -2.5 };
        h.move_to(Point::new(a.x + i as f32 * 8.0, a.y + wobble));
    }
    // Rest: after 0.5 s the hold check runs (frames while drawing) and the line snaps.
    std::thread::sleep(std::time::Duration::from_millis(650));
    h.message(Message::Tablet(Msg::Frame));
    h.release();
    let s = strokes(&h, &nb, &sheet).into_iter().find(|s| !old.contains(&s.id)).expect("stored");
    let (y0, y1) = s.points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.y), hi.max(p.y)));
    assert!(y1 - y0 < 1.0, "straight: y spread {} over {} points", y1 - y0, s.points.len());
    let (x0, x1) = s.points.iter().fold((f32::MAX, f32::MIN), |(lo, hi), p| (lo.min(p.x), hi.max(p.x)));
    assert!(x1 - x0 > 150.0 / open(&h).camera.zoom, "as long as drawn");
}

/// The notebook's sheet spec as laid out now.
fn spec_of(h: &Harness, sheet: &str) -> n0te_core::notebook::SheetSpec {
    open(h).pages.pages.iter().find(|p| p.placed.sheet_id == sheet).expect("page").placed.spec.clone()
}

/// The window point at page point (x, y).
fn win(h: &Harness, x: f32, y: f32) -> Point {
    let cam = open(h).camera;
    cam.to_screen(Rectangle::new(cam.origin, cam.viewport), x, y)
}

#[test]
fn the_pen_follows_the_rulers_edge_and_fingers_move_and_turn_it() {
    use iced::touch::{Event as T, Finger};
    let mut h = opened("ruler");
    h.message(Message::Tablet(Msg::Ruler(true)));
    let r = h.app.tablet.ruler.expect("ruler on").1;
    let (nb, sheet, _) = first_page(&h, 0.5, 0.5);
    let old: Vec<String> = strokes(&h, &nb, &sheet).into_iter().map(|s| s.id).collect();
    let page = open(&h).pages.pages.iter().find(|p| p.placed.sheet_id == sheet).unwrap().placed.rect;
    // Down just under the bottom edge, wobbling along: the stroke lies on the edge.
    let edge = r.cy + n0te_desktop::tablet::tools::RULER_W / 2.0;
    h.press_at(win(&h, r.cx - 150.0, edge + 6.0));
    for i in 1..=20 {
        let wob = if i % 2 == 0 { 7.0 } else { 2.0 };
        h.move_to(win(&h, r.cx - 150.0 + i as f32 * 15.0, edge + wob));
    }
    h.release();
    let s = strokes(&h, &nb, &sheet).into_iter().find(|s| !old.contains(&s.id)).expect("stored");
    let want = edge + s.brush.width / 2.0 - page.y0;
    assert!(s.points.iter().all(|p| (p.y - want).abs() < 0.05), "on the edge: {want} vs {:?}", s.points.iter().map(|p| p.y).collect::<Vec<_>>());
    assert!(s.points.last().unwrap().x - s.points[0].x > 250.0, "as long as drawn");
    h.screenshot(shots().join("tablet-03-ruler.png"));

    // One finger on it moves it.
    let zoom = open(&h).camera.zoom;
    let c = win(&h, r.cx, r.cy);
    h.events(&[iced::Event::Touch(T::FingerPressed { id: Finger(1), position: c })]);
    h.events(&[iced::Event::Touch(T::FingerMoved { id: Finger(1), position: Point::new(c.x + 60.0, c.y + 20.0) })]);
    let moved = h.app.tablet.ruler.unwrap().1;
    assert!((moved.cx - (r.cx + 60.0 / zoom)).abs() < 0.01 && (moved.cy - (r.cy + 20.0 / zoom)).abs() < 0.01, "moved: {moved:?}");
    let cam_before = open(&h).camera;
    // A second finger turns it: 44° comes out as 45°.
    let a = Point::new(c.x + 60.0, c.y + 20.0);
    let b0 = Point::new(a.x + 100.0, a.y);
    h.events(&[iced::Event::Touch(T::FingerPressed { id: Finger(2), position: b0 })]);
    let turn = 44.0f32.to_radians();
    h.events(&[iced::Event::Touch(T::FingerMoved { id: Finger(2), position: Point::new(a.x + 100.0 * turn.cos(), a.y + 100.0 * turn.sin()) })]);
    let turned = h.app.tablet.ruler.unwrap().1;
    assert!((turned.angle - 45.0f32.to_radians()).abs() < 1e-4, "snapped to 45°: {}", turned.angle.to_degrees());
    h.events(&[iced::Event::Touch(T::FingerLifted { id: Finger(1), position: a }), iced::Event::Touch(T::FingerLifted { id: Finger(2), position: b0 })]);
    assert_eq!(open(&h).camera, cam_before, "the pages didn't move meanwhile");
    h.message(Message::Tablet(Msg::Ruler(false)));
    assert!(h.app.tablet.ruler.is_none());
}

#[test]
fn space_goes_in_downwards_and_sideways_and_undo_takes_it_out() {
    let mut h = opened("space");
    let (nb, sheet, mid) = first_page(&h, 0.5, 0.2);
    let page = open(&h).pages.pages.iter().find(|p| p.placed.sheet_id == sheet).unwrap().placed.rect;
    let cam = open(&h).camera;
    let line = (mid.y - cam.origin.y) / cam.zoom + cam.y - page.y0;
    let before = strokes(&h, &nb, &sheet);
    let below: Vec<&n0te_core::sheet::Stroke> = before.iter().filter(|s| (s.bounds.y0 + s.bounds.y1) / 2.0 >= line).collect();
    assert!(!below.is_empty(), "something below the line");
    h.message(Message::Tablet(Msg::Tool(Tool::Space)));
    // Down 80 px: everything below moves down by about that (whole millimetres on blank paper).
    h.press_at(mid);
    for i in 1..=8 {
        h.move_to(Point::new(mid.x + (i % 2) as f32, mid.y + i as f32 * 10.0));
    }
    assert!(open(&h).pages.space.borrow().as_ref().is_some_and(|p| p.d > 70.0 / cam.zoom), "the preview shows it");
    h.screenshot(shots().join("tablet-04-space.png"));
    h.release();
    assert!(open(&h).pages.space.borrow().is_none());
    let after = strokes(&h, &nb, &sheet);
    let y_of = |id: &str, s: &[n0te_core::sheet::Stroke]| s.iter().find(|x| x.id == id).unwrap().bounds.y0;
    let d = y_of(&below[0].id, &after) - below[0].bounds.y0;
    assert!((d - 80.0 / cam.zoom).abs() < 3.0, "moved down {d}");
    for s in &below {
        assert!((y_of(&s.id, &after) - s.bounds.y0 - d).abs() < 0.05, "all below by the same: {} vs {d}, middle {} line {line}", y_of(&s.id, &after) - s.bounds.y0, (s.bounds.y0 + s.bounds.y1) / 2.0);
    }
    let above = before.iter().find(|s| (s.bounds.y0 + s.bounds.y1) / 2.0 < line - 5.0).expect("above");
    assert_eq!(y_of(&above.id, &after), above.bounds.y0, "above stays");
    assert!(spec_of(&h, &sheet).height >= 842.0, "the page as long or longer");
    // One undo takes it all back.
    h.message(Message::Tablet(Msg::Undo));
    assert!((y_of(&below[0].id, &strokes(&h, &nb, &sheet)) - below[0].bounds.y0).abs() < 0.05, "undone");
    assert_eq!(spec_of(&h, &sheet).height, 842.0);

    // Sideways: what lies right of the line moves right, the page gets wider if pushed.
    let start = win(&h, page.x0 + 300.0, page.y0 + 400.0);
    let right: Vec<n0te_core::sheet::Stroke> = strokes(&h, &nb, &sheet).into_iter().filter(|s| (s.bounds.x0 + s.bounds.x1) / 2.0 >= 300.0).collect();
    drag(&mut h, start, Point::new(start.x + 120.0, start.y + 2.0), 10);
    let now = strokes(&h, &nb, &sheet);
    let r0 = right.iter().max_by(|a, b| a.bounds.x1.total_cmp(&b.bounds.x1)).expect("something right");
    let dx = now.iter().find(|s| s.id == r0.id).unwrap().bounds.x0 - r0.bounds.x0;
    assert!((dx - 120.0 / cam.zoom).abs() < 3.0, "moved right {dx}");
    let w = spec_of(&h, &sheet).width;
    assert!(w >= r0.bounds.x1 + dx, "the page holds it: {w}");
    h.message(Message::Tablet(Msg::Undo));
    assert_eq!(spec_of(&h, &sheet).width, 595.0, "undo: as wide as before");

    // Nothing below: the page itself gets longer.
    let last = strokes(&h, &nb, &sheet).iter().map(|s| s.bounds.y1).fold(0.0f32, f32::max);
    let bottom = win(&h, page.x0 + 300.0, page.y0 + last + 30.0);
    drag(&mut h, bottom, Point::new(bottom.x, bottom.y + 60.0), 6);
    let grown = spec_of(&h, &sheet).height;
    assert!(grown > 842.0 + 50.0 / cam.zoom, "longer: {grown}");
    let status = h.app.status.clone().unwrap_or_default();
    assert!(status.starts_with("Platz eingefügt"), "told: {status}");
}

#[test]
fn a_scribble_over_ink_deletes_it() {
    let mut h = opened("scribble");
    let (nb, sheet, a) = first_page(&h, 0.3, 0.8);
    let before = strokes(&h, &nb, &sheet).len();
    drag(&mut h, a, Point::new(a.x + 120.0, a.y), 8);
    assert_eq!(strokes(&h, &nb, &sheet).len(), before + 1);
    // A quick zigzag over all of it.
    h.press_at(Point::new(a.x - 10.0, a.y - 12.0));
    for i in 1..=34 {
        let x = a.x - 10.0 + i as f32 * 4.0;
        let y = if i % 2 == 0 { a.y - 12.0 } else { a.y + 12.0 };
        h.move_to(Point::new(x, y));
    }
    h.release();
    assert_eq!(strokes(&h, &nb, &sheet).len(), before, "the line and the scribble are gone");
    assert_eq!(h.app.status.as_deref(), Some("1 Strich weggekritzelt"));
    // Undo brings the line back.
    h.message(Message::Tablet(Msg::Undo));
    assert_eq!(strokes(&h, &nb, &sheet).len(), before + 1);
    // Colouring in next to it with the same zigzag, touching the line only at its ends, keeps
    // the line and keeps the colouring as ink.
    h.press_at(Point::new(a.x + 10.0, a.y + 2.0));
    for i in 1..=24 {
        let x = a.x + 10.0 + i as f32 * 4.0;
        let y = if i % 2 == 0 { a.y + 2.0 } else { a.y + 40.0 };
        h.move_to(Point::new(x, y));
    }
    h.release();
    assert_eq!(strokes(&h, &nb, &sheet).len(), before + 2, "the line stays, the colouring is ink");
}

#[test]
fn the_plus_adds_a_page_below_and_side_pages() {
    use n0te_desktop::tablet::tools::HandleDir;
    let mut h = opened("handles");
    // The whole page in view (its "+" below too).
    h.message(Message::Zoom(0.5));
    h.events(&[]);
    let count = |h: &Harness| open(h).pages.pages.len();
    let handles = |h: &Harness| {
        let o = open(h);
        let g = h.app.settings.layout();
        n0te_desktop::tablet::tools::handles(&o.pages, &o.camera, g.side_gap, g.row_gap)
    };
    let hs = handles(&h);
    assert_eq!(hs.len(), 3, "left, right, below");
    h.screenshot(shots().join("tablet-05-plus.png"));
    let n = count(&h);
    let main = hs[0].main_node.clone();
    // A tap on the "+" below: a page after this one.
    let below = hs.iter().find(|x| x.dir == HandleDir::Below).unwrap().at;
    h.click(win(&h, below.x, below.y));
    assert_eq!(count(&h), n + 1, "a page added");
    let o = open(&h);
    let i = o.pages.pages.iter().position(|p| p.placed.node == main).unwrap();
    let next = o.pages.pages[i + 1..].iter().find(|p| p.placed.side == 0).unwrap();
    assert!(!next.placed.is_board() && next.placed.spec.pdf.is_none());
    // Back to the first page; drag the right "+" outwards: a side page on the right.
    let first = o.pages.pages[i].placed.sheet_id.clone();
    h.message(Message::Jump(first.clone()));
    h.events(&[]);
    let right = handles(&h).into_iter().find(|x| x.dir == HandleDir::Right).unwrap();
    let p = win(&h, right.at.x, right.at.y);
    drag(&mut h, p, Point::new(p.x + 80.0, p.y), 4);
    assert_eq!(count(&h), n + 2);
    assert!(open(&h).pages.pages.iter().any(|q| q.placed.main_node == right.main_node && q.placed.side == 1), "a right side page");
    // Dragged back: nothing.
    h.message(Message::Jump(first));
    h.events(&[]);
    let left = handles(&h).into_iter().find(|x| x.dir == HandleDir::Left).unwrap();
    let p = win(&h, left.at.x, left.at.y);
    drag(&mut h, p, Point::new(p.x + 70.0, p.y), 4);
    assert_eq!(count(&h), n + 2, "dragged back: no page");
    // A finger tap on a "+" adds its page too (fingers elsewhere move the pages).
    use iced::touch::{Event as T, Finger};
    let below = handles(&h).into_iter().find(|x| x.dir == HandleDir::Below).unwrap();
    let p = win(&h, below.at.x, below.at.y);
    h.events(&[iced::Event::Touch(T::FingerPressed { id: Finger(7), position: p })]);
    h.events(&[iced::Event::Touch(T::FingerLifted { id: Finger(7), position: p })]);
    assert_eq!(count(&h), n + 3, "a finger on the +");
}

#[test]
fn a_board_opens_on_its_own_endless_and_back() {
    assert!(n0te_desktop::set_mode(Mode::Tablet));
    let root = temp_root("board");
    let lib = Library::open(&root).unwrap();
    let id = demo::save_into(&lib);
    let nb = lib.load_notebook(&id).unwrap();
    let (_, end) = n0te_desktop::pdf::position_after(&nb, None);
    let (_, board) = nb.add_sheet(None, end, &n0te_core::notebook::SheetSpec::board()).unwrap();
    lib.save_notebook(&nb).unwrap();
    let size = Size::new(1400.0, 900.0);
    let mut h = Harness::new(App::new(&root), "wgpu", size, 1.0).or_else(|| Harness::new(App::new(&root), "tiny-skia", size, 1.0)).expect("a renderer");
    h.message(Message::Open(id.clone()));
    h.events(&[]);
    h.message(Message::Jump(board.clone()));
    h.events(&[]);
    let column = open(&h).camera;
    h.message(Message::Tablet(Msg::Board(true)));
    h.events(&[]);
    assert!(open(&h).pages.board.is_some(), "open on its own");
    // Endless: the view goes far beyond the frame, the pen writes there.
    for _ in 0..4 {
        h.scroll(Point::new(700.0, 500.0), 0.0, 1500.0);
    }
    let cam = open(&h).camera;
    let frame = open(&h).pages.pages.iter().find(|p| p.placed.sheet_id == board).unwrap().placed.rect;
    assert!(cam.y + cam.viewport.height / cam.zoom < frame.y0, "far above the frame: {} vs {}", cam.y, frame.y0);
    let a = Point::new(600.0, 500.0);
    drag(&mut h, a, Point::new(a.x + 100.0, a.y + 30.0), 8);
    let s = strokes(&h, &id, &board);
    assert_eq!(s.len(), 1);
    assert!(s[0].bounds.y1 < 0.0, "written above the frame: {:?}", s[0].bounds);
    h.screenshot(shots().join("tablet-06-board.png"));
    h.message(Message::Tablet(Msg::Board(false)));
    assert!(open(&h).pages.board.is_none());
    assert_eq!((open(&h).camera.x, open(&h).camera.y, open(&h).camera.zoom), (column.x, column.y, column.zoom), "back where it was");
}

#[test]
fn the_writing_magnifier_writes_into_its_frame_and_moves_on() {
    let mut h = opened("loupe");
    h.message(Message::Tablet(Msg::Loupe(true)));
    let l = h.app.tablet.loupe.clone().expect("open");
    let (_, strip, cam) = h.app.loupe_view().expect("strip");
    h.events(&[]);
    h.screenshot(shots().join("tablet-07-loupe.png"));
    let nb = open(&h).id.clone();
    let old: Vec<String> = strokes(&h, &nb, &l.sheet).into_iter().map(|s| s.id).collect();
    // Writing in the strip lands in the frame, small.
    let a = Point::new(strip.x + strip.width * 0.1, strip.y + strip.height * 0.6);
    drag(&mut h, a, Point::new(a.x + strip.width * 0.3, a.y + 5.0), 12);
    let s = strokes(&h, &nb, &l.sheet).into_iter().find(|s| !old.contains(&s.id)).expect("written");
    let len = s.points.last().unwrap().x - s.points[0].x;
    assert!((len - strip.width * 0.3 / cam.zoom).abs() < 2.0, "as long as in the strip / its zoom: {len}");
    assert!((s.points[0].x - (l.x + strip.width * 0.1 / cam.zoom)).abs() < 2.0, "at the frame's place: {} vs {}", s.points[0].x, l.x);
    // A tap in the strip's right part moves the frame on (and writes no dot).
    let n = strokes(&h, &nb, &l.sheet).len();
    h.click(Point::new(strip.x + strip.width * 0.92, strip.y + strip.height * 0.5));
    let moved = h.app.tablet.loupe.clone().unwrap();
    assert!(moved.x > l.x + l.w * 0.5, "moved on: {} → {}", l.x, moved.x);
    assert_eq!(strokes(&h, &nb, &l.sheet).len(), n, "no dot");
    // The next line: down, back to where lines begin.
    h.message(Message::Tablet(Msg::LoupeNewLine));
    let next = h.app.tablet.loupe.clone().unwrap();
    assert!(next.y > moved.y && next.x == next.line_x, "next line: {:?}", (next.x, next.y));
    h.message(Message::Tablet(Msg::Loupe(false)));
    assert!(h.app.tablet.loupe.is_none());
}

#[test]
fn mitwandern_moves_the_page_on_after_a_lift_in_the_band() {
    let mut h = opened("flow");
    h.message(Message::Tablet(Msg::WriteFlow(true)));
    // A squared page, zoomed in past its width.
    let grid = open(&h).pages.pages.iter().find(|p| p.placed.side == 0 && p.placed.spec.background == n0te_core::notebook::Background::Grid).unwrap().placed.clone();
    h.message(Message::Jump(grid.sheet_id.clone()));
    h.message(Message::Zoom(2.5));
    h.events(&[]);
    let cam = open(&h).camera;
    assert!(h.app.flow_page(open(&h).key).is_some(), "Mitwandern acts here");
    let band = h.app.flow_band(open(&h).key, &grid).expect("a band");
    // A word ending in the band: a moment later the page slides left under the pen.
    let end = Point::new(band.x + band.width / 2.0, band.y + band.height * 0.5);
    drag(&mut h, Point::new(end.x - 80.0, end.y), end, 8);
    assert_eq!(open(&h).camera, cam, "not at once");
    std::thread::sleep(std::time::Duration::from_millis(450));
    h.message(Message::Tablet(Msg::Frame));
    let slid = open(&h).camera;
    assert!(slid.x > cam.x + 50.0 / cam.zoom, "slid on: {} → {}", cam.x, slid.x);
    // At the page's right edge: a tap in the band goes a line down, back to its left edge.
    for _ in 0..40 {
        let c = open(&h).camera;
        if grid.rect.x1 < c.x + c.viewport.width / c.zoom - 40.0 / c.zoom {
            break;
        }
        h.scroll(Point::new(700.0, 500.0), -60.0, 0.0);
    }
    let at_edge = open(&h).camera;
    let band = h.app.flow_band(open(&h).key, &grid).expect("a band");
    h.click(Point::new(band.x + band.width / 2.0, band.y + band.height * 0.4));
    let next = open(&h).camera;
    assert!(next.y > at_edge.y && next.x < at_edge.x, "next line: {:?} → {:?}", (at_edge.x, at_edge.y), (next.x, next.y));
    assert!((next.x - (grid.rect.x0 - 12.0 / next.zoom)).abs() < 1.0, "back to the left edge");
}
