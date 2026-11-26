//! What n0te Tablet draws over the pages: the lasso while it is drawn and the frame around a
//! selection (red, like on the tablet), the ruler, the "+" beside the current row (and the page a
//! dragged one adds), space going into a page, the writing magnifier (its frame on the page, its
//! strip along the bottom with its buttons).

use iced::widget::canvas::{self, Path, Stroke};
use iced::widget::{button, container, pin, row};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Size, Theme, mouse};
use n0te_core::ink::Rect;

use super::tools::{self, HandleDir, LOUPE_ADVANCE, RULER_LEN, RULER_W};
use super::{Msg, Tablet};
use crate::app::{App, Message, Open};
use crate::i18n::tr;
use crate::pages::{Camera, Slot};
use crate::theme;

/// The magnifier's frame on its page (page points) and where its lines begin (page x).
struct Frame {
    rect: Rect,
    line_x: f32,
}

/// Space going into a page: the page (page points), the line (page points), the way (None:
/// unclear yet), how far what lies beyond moves, the label.
struct SpaceMarks {
    page: Rect,
    at: Point,
    sideways: Option<bool>,
    d: f32,
    label: String,
}

struct Overlay {
    camera: Camera,
    lasso: Option<Vec<Point>>,
    /// The selection's frame in page coordinates.
    frame: Option<Rect>,
    /// The laser point (page coordinates).
    laser: Option<Point>,
    /// The pen hovering: where (page coordinates) and the ring's radius in px.
    hover: Option<(Point, f32)>,
    ruler: Option<tools::Ruler>,
    /// The "+" (page points), the one held, and the page it adds (with how sure: 0.25–1).
    handles: Vec<(HandleDir, Point)>,
    held: Option<HandleDir>,
    ghost: Option<(Rect, f32)>,
    space: Option<SpaceMarks>,
    loupe: Option<Frame>,
    /// Mitwandern's band (pane-local px) and its glyph: "→" slides on, "↵" goes to the next line;
    /// the line's middle (page y) if known.
    flow: Option<(Rectangle, &'static str, Option<f32>)>,
}

fn dashed(color: Color, width: f32, on: &'static [f32]) -> Stroke<'static> {
    Stroke { line_dash: canvas::LineDash { segments: on, offset: 0 }, ..Stroke::default().with_color(color).with_width(width) }
}

impl canvas::Program<Message> for Overlay {
    type State = ();

    fn draw(&self, _state: &(), renderer: &Renderer, _theme: &Theme, bounds: Rectangle, _cursor: mouse::Cursor) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let local = Rectangle::new(Point::ORIGIN, bounds.size());
        let cam = self.camera;
        let at = |x: f32, y: f32| cam.to_screen(local, x, y);
        let accent = theme::p().accent;
        if let Some(r) = self.ruler {
            draw_ruler(&mut frame, &r, &at, cam.zoom);
        }
        if let Some(l) = &self.loupe {
            let (a, b) = (at(l.rect.x0, l.rect.y0), at(l.rect.x1, l.rect.y1));
            let rect = Path::rectangle(a, Size::new(b.x - a.x, b.y - a.y));
            frame.fill(&rect, Color { a: 0.08, ..accent });
            frame.stroke(&rect, Stroke::default().with_color(accent).with_width(2.0));
            // Where new lines begin: a short bar left of the line.
            let mx = at(l.line_x, l.rect.y0).x - 5.0;
            let h = b.y - a.y;
            frame.stroke(&Path::line(Point::new(mx, a.y + h * 0.2), Point::new(mx, b.y - h * 0.1)), Stroke::default().with_color(Color { a: 0.56, ..accent }).with_width(3.0));
            // The corner: drag it to write larger or smaller.
            frame.fill(&Path::circle(b, 11.0), Color { a: 0.25, ..accent });
            frame.fill(&Path::circle(b, 6.5), accent);
        }
        if let Some(sp) = &self.space {
            draw_space(&mut frame, sp, &at, cam.zoom);
        }
        if let Some((band, glyph, line)) = self.flow {
            frame.fill(&Path::rectangle(band.position(), band.size()), Color { a: 0.07, ..accent });
            frame.stroke(&Path::line(band.position(), Point::new(band.x, band.y + band.height)), dashed(Color { a: 0.31, ..accent }, 1.5, &[6.0, 5.0]));
            let y = line.map_or(band.y + band.height / 2.0, |y| at(0.0, y).y).clamp(band.y + 30.0, (band.y + band.height - 10.0).max(band.y + 30.0));
            frame.fill_text(canvas::Text {
                content: glyph.into(),
                position: Point::new(band.x + band.width / 2.0, y),
                color: Color { a: 0.7, ..accent },
                size: 22.0.into(),
                align_x: iced::advanced::text::Alignment::Center,
                align_y: iced::alignment::Vertical::Center,
                ..canvas::Text::default()
            });
        }
        if let Some((g, a)) = self.ghost {
            let (p, q) = (at(g.x0, g.y0), at(g.x1, g.y1));
            let rect = Path::rectangle(p, Size::new(q.x - p.x, q.y - p.y));
            frame.fill(&rect, Color { a: 0.2 * a, ..Color::WHITE });
            frame.stroke(&rect, dashed(Color { a, ..accent }, 1.5, &[10.0, 7.0]));
        }
        for (dir, p) in &self.handles {
            let c = at(p.x, p.y);
            let r = tools::HANDLE_R;
            let active = self.held == Some(*dir);
            frame.fill(&Path::circle(c, r), if active { accent } else { theme::p().surface });
            frame.stroke(&Path::circle(c, r), Stroke::default().with_color(theme::p().border).with_width(1.5));
            let k = r * 0.42;
            let plus = Path::new(|b| {
                b.move_to(Point::new(c.x - k, c.y));
                b.line_to(Point::new(c.x + k, c.y));
                b.move_to(Point::new(c.x, c.y - k));
                b.line_to(Point::new(c.x, c.y + k));
            });
            frame.stroke(&plus, Stroke::default().with_color(if active { Color::WHITE } else { theme::p().muted }).with_width(2.0).with_line_cap(canvas::LineCap::Round));
        }
        if let Some(pts) = &self.lasso
            && pts.len() > 1
        {
            let path = Path::new(|b| {
                b.move_to(at(pts[0].x, pts[0].y));
                for p in &pts[1..] {
                    b.line_to(at(p.x, p.y));
                }
            });
            frame.stroke(&path, dashed(accent, 1.5, &[6.0, 4.0]));
        }
        if let Some(r) = self.frame {
            let a = at(r.x0, r.y0);
            let b = at(r.x1, r.y1);
            let rect = Path::rectangle(Point::new(a.x - 6.0, a.y - 6.0), iced::Size::new(b.x - a.x + 12.0, b.y - a.y + 12.0));
            frame.fill(&rect, Color { a: 0.06, ..accent });
            frame.stroke(&rect, Stroke::default().with_color(accent).with_width(1.5));
        }
        if let Some((p, r)) = self.hover {
            let c = at(p.x, p.y);
            frame.stroke(&Path::circle(c, r), Stroke::default().with_color(Color { a: 0.7, ..theme::INK }).with_width(1.0));
        }
        if let Some(p) = self.laser {
            let c = at(p.x, p.y);
            frame.fill(&Path::circle(c, 14.0), Color { a: 0.18, ..accent });
            frame.fill(&Path::circle(c, 6.0), accent);
        }
        vec![frame.into_geometry()]
    }
}

/// The ruler: dark and see-through, millimetre ticks on both edges (longer at 5 mm and 1 cm),
/// its angle in the middle (as on the tablet).
fn draw_ruler(frame: &mut canvas::Frame, r: &tools::Ruler, at: &dyn Fn(f32, f32) -> Point, zoom: f32) {
    let corners = r.corners();
    let body = Path::new(|b| {
        let c: Vec<Point> = corners.iter().map(|p| at(p.x, p.y)).collect();
        b.move_to(c[0]);
        for p in &c[1..] {
            b.line_to(*p);
        }
        b.close();
    });
    frame.fill(&body, Color::from_rgba8(0x1E, 0x1E, 0x22, 0.36));
    frame.stroke(&body, Stroke::default().with_color(Color::from_rgba8(0x1E, 0x1E, 0x22, 0.8)).with_width(1.5));
    let mm = tools::MM;
    let tick = Color::from_rgba8(0xF2, 0xF2, 0xF4, 0.9);
    let ticks = Path::new(|b| {
        let n = (RULER_LEN / mm) as i32;
        for i in 0..=n {
            if mm * zoom < 3.0 && i % 10 != 0 {
                continue;
            }
            let len = if i % 10 == 0 { 14.0 } else if i % 5 == 0 { 9.0 } else { 5.0 };
            let a = -RULER_LEN / 2.0 + i as f32 * mm;
            for (edge, inward) in [(-RULER_W / 2.0, len), (RULER_W / 2.0, -len)] {
                let (p, q) = (r.at(a, edge), r.at(a, edge + inward));
                b.move_to(at(p.x, p.y));
                b.line_to(at(q.x, q.y));
            }
        }
    });
    frame.stroke(&ticks, Stroke::default().with_color(tick).with_width(1.0));
    let c = at(r.cx, r.cy);
    frame.fill_text(canvas::Text {
        content: format!("{:.0}°", r.degrees()),
        position: c,
        color: tick,
        size: (9.0 * zoom).clamp(9.0, 14.0).into(),
        font: theme::MONO,
        align_x: iced::advanced::text::Alignment::Center,
        align_y: iced::alignment::Vertical::Center,
        ..canvas::Text::default()
    });
}

/// Space going into a page: the line (a small cross while it isn't clear which way), the new
/// strip tinted, its size told in a label.
fn draw_space(frame: &mut canvas::Frame, sp: &SpaceMarks, at: &dyn Fn(f32, f32) -> Point, zoom: f32) {
    let accent = theme::p().accent;
    let line = dashed(accent, 1.5, &[8.0, 6.0]);
    let (a, b) = (at(sp.page.x0, sp.page.y0), at(sp.page.x1, sp.page.y1));
    let p = at(sp.at.x, sp.at.y);
    let d = sp.d * zoom;
    match sp.sideways {
        None => {
            let arm = 26.0;
            let cross = Path::new(|bd| {
                bd.move_to(Point::new(p.x - arm, p.y));
                bd.line_to(Point::new(p.x + arm, p.y));
                bd.move_to(Point::new(p.x, p.y - arm));
                bd.line_to(Point::new(p.x, p.y + arm));
            });
            frame.stroke(&cross, dashed(accent, 1.5, &[6.0, 5.0]));
            return;
        }
        Some(false) => {
            if d > 0.0 {
                frame.fill(&Path::rectangle(Point::new(a.x, p.y), Size::new(b.x - a.x, d)), Color { a: 0.1, ..accent });
            }
            for y in [Some(p.y), (d > 0.0).then_some(p.y + d)].into_iter().flatten() {
                frame.stroke(&Path::line(Point::new(a.x - 14.0, y), Point::new(b.x + 14.0, y)), line.clone());
            }
        }
        Some(true) => {
            if d > 0.0 {
                frame.fill(&Path::rectangle(Point::new(p.x, a.y), Size::new(d, b.y - a.y)), Color { a: 0.1, ..accent });
            }
            for x in [Some(p.x), (d > 0.0).then_some(p.x + d)].into_iter().flatten() {
                frame.stroke(&Path::line(Point::new(x, a.y - 14.0), Point::new(x, b.y + 14.0)), line.clone());
            }
        }
    }
    if sp.label.is_empty() {
        return;
    }
    let size = 13.0;
    let w = sp.label.chars().count() as f32 * size * 0.62 + 12.0;
    let h = size * 1.6;
    let (x, y) = match sp.sideways {
        Some(true) => (p.x + d.max(0.0) / 2.0 - w / 2.0, (p.y - 40.0 - h).clamp(a.y + 6.0, (b.y - h - 6.0).max(a.y + 6.0))),
        _ => (b.x - w - 6.0, if d > 0.0 { p.y + d / 2.0 - h / 2.0 } else { p.y - h - 4.0 }),
    };
    frame.fill(&Path::rounded_rectangle(Point::new(x, y), Size::new(w, h), 6.0.into()), accent);
    frame.fill_text(canvas::Text {
        content: sp.label.clone(),
        position: Point::new(x + w / 2.0, y + h / 2.0),
        color: Color::WHITE,
        size: size.into(),
        font: theme::MONO,
        align_x: iced::advanced::text::Alignment::Center,
        align_y: iced::alignment::Vertical::Center,
        ..canvas::Text::default()
    });
}

/// The overlay for pane `o`, or None when there is nothing to draw.
fn view<'a>(app: &'a App, t: &'a Tablet, o: &'a Open) -> Option<Element<'a, Message>> {
    let lasso = t.lasso_outline.as_ref().filter(|(k, _)| *k == o.key).map(|(_, p)| p.clone());
    let frame = t.selection.as_ref().filter(|s| s.notebook == o.id).and_then(|s| {
        let placed = o.pages.pages.iter().find(|p| p.placed.sheet_id == s.sheet)?;
        let r = s.bounds;
        let (x0, y0) = (placed.placed.rect.x0, placed.placed.rect.y0);
        Some(Rect::new(r.x0 + x0, r.y0 + y0, r.x1 + x0, r.y1 + y0))
    });
    let laser = t.laser.filter(|(k, _)| *k == o.key).map(|(_, p)| p);
    // Hovering: a ring as big as the eraser (eraser) or the line (pens), not while writing.
    let hover = t.hover.filter(|(k, _)| *k == o.key && !t.busy()).map(|(_, p)| {
        let r = match t.tool {
            super::Tool::Eraser => t.eraser_size,
            _ => (t.brush().width * o.camera.zoom / 2.0).max(2.0),
        };
        (p, r)
    });
    let ruler = t.ruler.filter(|(k, _)| *k == o.key).map(|(_, r)| r);
    let held = t.handle.as_ref().filter(|h| h.pane == o.key);
    let handles: Vec<(HandleDir, Point)> = app.handles_in(o.key).into_iter().map(|h| (h.dir, h.at)).collect();
    let ghost = held.map(|h| (h.handle.ghost, (h.drag / 60.0).clamp(0.25, 1.0)));
    let space = t.space.as_ref().filter(|s| s.pane() == o.key).and_then(|sd| {
        let (page, at, sideways, d) = sd.marks();
        let preview = o.pages.space.borrow().clone()?;
        let own = preview.own_shift(page);
        let r = page.rect;
        let (w, h) = preview.size;
        Some(SpaceMarks { page: Rect::new(r.x0 + own, r.y0, r.x0 + own + w, r.y0 + h), at: Point::new(r.x0 + own + at.0, r.y0 + at.1), sideways, d, label: sd.label() })
    });
    let loupe = t.loupe.as_ref().filter(|l| l.pane == o.key).and_then(|l| {
        let (_, strip, _) = app.loupe_view()?;
        let page = o.pages.pages.iter().find(|p| p.placed.sheet_id == l.sheet)?.placed.rect;
        let fh = l.w * strip.height / strip.width.max(1.0);
        Some(Frame { rect: Rect::new(page.x0 + l.x, page.y0 + l.y, page.x0 + l.x + l.w, page.y0 + l.y + fh), line_x: page.x0 + l.line_x })
    });
    // Mitwandern's band, while the pen is near (hovering) or writing.
    let near = t.hover.is_some_and(|(k, _)| k == o.key) || t.busy();
    let flow = app.flow_page(o.key).filter(|_| near).and_then(|p| {
        let band = app.flow_band(o.key, &p)?;
        let more = p.rect.x1 > o.camera.x + o.camera.viewport.width / o.camera.zoom + 2.0 / o.camera.zoom;
        let lined = matches!(p.spec.background, n0te_core::notebook::Background::Lines | n0te_core::notebook::Background::Cornell | n0te_core::notebook::Background::Grid | n0te_core::notebook::Background::Dots);
        let glyph = if more { "→" } else if lined { "↵" } else { return None };
        let line = t.flow.as_ref().filter(|f| f.sheet == p.sheet_id).map(|f| (f.top + f.bottom) / 2.0);
        Some((Rectangle::new(Point::new(band.x - o.camera.origin.x, band.y - o.camera.origin.y), band.size()), glyph, line))
    });
    if lasso.is_none() && frame.is_none() && laser.is_none() && hover.is_none() && ruler.is_none() && handles.is_empty() && ghost.is_none() && space.is_none() && loupe.is_none() && flow.is_none() {
        return None;
    }
    let held = held.map(|h| h.handle.dir);
    Some(canvas::Canvas::new(Overlay { camera: o.camera, lasso, frame, laser, hover, ruler, handles, held, ghost, space, loupe, flow }).width(Length::Fill).height(Length::Fill).into())
}

/// The writing magnifier's strip over its pane: the frame's part of the page, large (its own
/// page view: a finger slides the paper, two pinch it), the part where a lift moves on, and the
/// buttons on its top edge. Positions relative to the pane's page view at `origin`.
fn loupe_layers<'a>(app: &'a App, o: &'a Open) -> Vec<Element<'a, Message>> {
    let Some(l) = app.tablet.loupe.as_ref().filter(|l| l.pane == o.key) else { return Vec::new() };
    let Some((_, strip, cam)) = app.loupe_view() else { return Vec::new() };
    let origin = o.camera.origin;
    let view = crate::pages::page_view(&o.pages, cam, |c| Message::Tablet(Msg::LoupeCamera(c)), Message::Fold).slot(Slot::Loupe).fixed().on_pen(Message::Pen);
    let border = theme::p().border;
    let strip_view = container(view).width(strip.width).height(strip.height).style(move |_: &Theme| container::Style {
        border: iced::Border { color: border, width: 1.0, radius: 14.0.into() },
        ..Default::default()
    });
    let at_end = app.loupe_page_width().is_some_and(|pw| l.x + l.w >= pw * 0.97 - 1.0);
    let past = l.last_x.is_some_and(|x| x > l.x + l.w * 0.5);
    let marks = canvas::Canvas::new(StripMarks { line: (l.line_x - l.x) * cam.zoom, at_end, past }).width(strip.width).height(strip.height);
    let mut out: Vec<Element<'a, Message>> = vec![pin(strip_view).x(strip.x - origin.x).y(strip.y - origin.y).into(), pin(marks).x(strip.x - origin.x).y(strip.y - origin.y).into()];
    if let Some(c) = app.loupe_controls() {
        let b = |label: &'static str, m: Msg| button(theme::btn_small(tr(label))).padding([6, 10]).on_press(Message::Tablet(m)).style(theme::ghost);
        let bar = row![b("Zurück", Msg::LoupeStep(false)), b("Weiter", Msg::LoupeStep(true)), b("Neue Zeile", Msg::LoupeNewLine), b("Schließen", Msg::Loupe(false))].spacing(2).align_y(iced::Alignment::Center);
        let bar = container(bar).padding([2, 6]).width(c.width).height(c.height).align_x(iced::alignment::Horizontal::Right).align_y(iced::alignment::Vertical::Center).style(theme::menu);
        out.push(pin(bar).x(c.x - origin.x).y(c.y - origin.y).into());
    }
    out
}

/// Over the strip: its move-on part (right) shaded, stronger once a stroke ended past the
/// middle, "→" (or "↵" at the page's end), where lines begin.
struct StripMarks {
    /// Where the frame's lines begin, px from the strip's left.
    line: f32,
    at_end: bool,
    past: bool,
}

impl canvas::Program<Message> for StripMarks {
    type State = ();

    fn draw(&self, _state: &(), renderer: &Renderer, _theme: &Theme, bounds: Rectangle, _cursor: mouse::Cursor) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let (w, h) = (bounds.width, bounds.height);
        let accent = theme::p().accent;
        let zone = w * (1.0 - LOUPE_ADVANCE);
        frame.fill(&Path::rectangle(Point::new(zone, 0.0), Size::new(w - zone, h)), Color { a: if self.past { 0.14 } else { 0.055 }, ..accent });
        frame.stroke(&Path::line(Point::new(zone, 10.0), Point::new(zone, h - 10.0)), dashed(Color { a: 0.38, ..accent }, 1.5, &[6.0, 5.0]));
        frame.fill_text(canvas::Text {
            content: if self.at_end { "↵".into() } else { "→".into() },
            position: Point::new(zone + (w - zone) / 2.0, 40.0),
            color: Color { a: 0.7, ..accent },
            size: 24.0.into(),
            align_x: iced::advanced::text::Alignment::Center,
            align_y: iced::alignment::Vertical::Center,
            ..canvas::Text::default()
        });
        let mx = self.line - 6.0;
        if mx > 0.0 {
            frame.stroke(&Path::line(Point::new(mx, h * 0.2), Point::new(mx, h * 0.88)), Stroke::default().with_color(Color { a: 0.5, ..accent }).with_width(3.0));
        }
        vec![frame.into_geometry()]
    }
}

/// Everything n0te Tablet draws over pane `o` (bottom first).
pub fn layers<'a>(app: &'a App, o: &'a Open) -> Vec<Element<'a, Message>> {
    let mut out: Vec<Element<'a, Message>> = view(app, &app.tablet, o).into_iter().collect();
    out.extend(loupe_layers(app, o));
    // What the selection can do, floating by it (not while the pen moves it).
    if let Some((key, r)) = app.selection_bar().filter(|_| !app.tablet.busy())
        && key == o.key
        && let Some(bar) = super::toolbar::selection_bar(&app.tablet)
    {
        out.push(pin(bar).x(r.x - o.camera.origin.x).y(r.y - o.camera.origin.y).into());
    }
    out
}
