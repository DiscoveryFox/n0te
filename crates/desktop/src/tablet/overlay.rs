//! What n0te Tablet draws over the pages: the lasso while it is drawn and the frame around a
//! selection (red, like on the tablet).

use iced::widget::canvas::{self, Path, Stroke};
use iced::{Color, Element, Length, Point, Rectangle, Renderer, Theme, mouse};

use super::Tablet;
use crate::app::{Message, Open};
use crate::pages::Camera;
use crate::theme;

struct Overlay {
    camera: Camera,
    lasso: Option<Vec<Point>>,
    /// The selection's frame in page coordinates.
    frame: Option<n0te_core::ink::Rect>,
    /// The laser point (page coordinates).
    laser: Option<Point>,
}

impl canvas::Program<Message> for Overlay {
    type State = ();

    fn draw(&self, _state: &(), renderer: &Renderer, _theme: &Theme, bounds: Rectangle, _cursor: mouse::Cursor) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let local = Rectangle::new(Point::ORIGIN, bounds.size());
        let accent = theme::p().accent;
        if let Some(pts) = &self.lasso
            && pts.len() > 1
        {
            let path = Path::new(|b| {
                b.move_to(self.camera.to_screen(local, pts[0].x, pts[0].y));
                for p in &pts[1..] {
                    b.line_to(self.camera.to_screen(local, p.x, p.y));
                }
            });
            frame.stroke(&path, Stroke { line_dash: canvas::LineDash { segments: &[6.0, 4.0], offset: 0 }, ..Stroke::default().with_color(accent).with_width(1.5) });
        }
        if let Some(r) = self.frame {
            let a = self.camera.to_screen(local, r.x0, r.y0);
            let b = self.camera.to_screen(local, r.x1, r.y1);
            let rect = Path::rectangle(Point::new(a.x - 6.0, a.y - 6.0), iced::Size::new(b.x - a.x + 12.0, b.y - a.y + 12.0));
            frame.fill(&rect, Color { a: 0.06, ..accent });
            frame.stroke(&rect, Stroke::default().with_color(accent).with_width(1.5));
        }
        if let Some(p) = self.laser {
            let c = self.camera.to_screen(local, p.x, p.y);
            frame.fill(&Path::circle(c, 14.0), Color { a: 0.18, ..accent });
            frame.fill(&Path::circle(c, 6.0), accent);
        }
        vec![frame.into_geometry()]
    }
}

/// The overlay for pane `o`, or None when there is nothing to draw.
pub fn view<'a>(t: &'a Tablet, o: &'a Open) -> Option<Element<'a, Message>> {
    let lasso = t.lasso_outline.as_ref().filter(|(k, _)| *k == o.key).map(|(_, p)| p.clone());
    let frame = t.selection.as_ref().filter(|s| s.notebook == o.id).and_then(|s| {
        let placed = o.pages.pages.iter().find(|p| p.placed.sheet_id == s.sheet)?;
        let r = s.bounds;
        let (x0, y0) = (placed.placed.rect.x0, placed.placed.rect.y0);
        Some(n0te_core::ink::Rect::new(r.x0 + x0, r.y0 + y0, r.x1 + x0, r.y1 + y0))
    });
    let laser = t.laser.filter(|(k, _)| *k == o.key).map(|(_, p)| p);
    if lasso.is_none() && frame.is_none() && laser.is_none() {
        return None;
    }
    Some(canvas::Canvas::new(Overlay { camera: o.camera, lasso, frame, laser }).width(Length::Fill).height(Length::Fill).into())
}
