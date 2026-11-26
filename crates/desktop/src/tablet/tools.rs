//! The geometry of n0te Tablet's helpers, as on the Android tablet (NotebookView.kt): the
//! ruler, scribbling to delete, the "+" beside the current page row, the writing magnifier's
//! strip and frame, and how inserted space is told. Pure functions, tested here.

use iced::{Point, Rectangle, Size};
use n0te_core::ink::Rect;
use n0te_core::layout::Placed;
use n0te_core::notebook::Background;

use crate::i18n::{tr, trf};
use crate::pages::{Camera, Pages};

// --- the ruler -------------------------------------------------------------------------------

/// The ruler's length and width (page points).
pub const RULER_LEN: f32 = 560.0;
pub const RULER_W: f32 = 56.0;
/// How close (screen px) the pen must go down to an edge for the stroke to follow it.
pub const RULER_EDGE_PX: f32 = 18.0;

/// A ruler lying on the pages: its middle (page points) and its angle (radians).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Ruler {
    pub cx: f32,
    pub cy: f32,
    pub angle: f32,
}

impl Ruler {
    /// A page point in the ruler's own coordinates: (along it, across it).
    pub fn local(&self, x: f32, y: f32) -> (f32, f32) {
        let (dx, dy) = (x - self.cx, y - self.cy);
        let (c, s) = ((-self.angle).cos(), (-self.angle).sin());
        (dx * c - dy * s, dx * s + dy * c)
    }

    /// Whether a page point lies on the ruler.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let (a, b) = self.local(x, y);
        a.abs() <= RULER_LEN / 2.0 && b.abs() <= RULER_W / 2.0
    }

    /// Which edge a pen at a page point touches (within `tol` page points): -1 the top one, 1
    /// the bottom one, 0 none.
    pub fn edge_at(&self, x: f32, y: f32, tol: f32) -> i8 {
        let (a, b) = self.local(x, y);
        if a.abs() > RULER_LEN / 2.0 {
            return 0;
        }
        if (b + RULER_W / 2.0).abs() < tol {
            -1
        } else if (b - RULER_W / 2.0).abs() < tol {
            1
        } else {
            0
        }
    }

    /// A page point moved onto edge `edge`, outside it by half the line `width` (the ink lies
    /// along the ruler, like a pen against a real one).
    pub fn snap(&self, x: f32, y: f32, edge: i8, width: f32) -> (f32, f32) {
        let (a, _) = self.local(x, y);
        let across = edge as f32 * (RULER_W / 2.0 + width / 2.0);
        let (c, s) = (self.angle.cos(), self.angle.sin());
        (self.cx + a * c - across * s, self.cy + a * s + across * c)
    }

    /// The ruler turned to `raw` radians, snapping to the nearest 45° within 2° (as on the
    /// tablet).
    pub fn turned_to(&self, raw: f32) -> Ruler {
        let deg = raw.to_degrees();
        let near = (deg / 45.0).round() * 45.0;
        let angle = if (deg - near).abs() < 2.0 { near.to_radians() } else { raw };
        Ruler { angle, ..*self }
    }

    /// The angle shown on the ruler (0–359°).
    pub fn degrees(&self) -> f32 {
        (self.angle.to_degrees() % 360.0 + 360.0) % 360.0
    }

    /// The ruler's four corners (page points), for drawing.
    pub fn corners(&self) -> [Point; 4] {
        let (c, s) = (self.angle.cos(), self.angle.sin());
        let at = |a: f32, b: f32| Point::new(self.cx + a * c - b * s, self.cy + a * s + b * c);
        let (l, w) = (RULER_LEN / 2.0, RULER_W / 2.0);
        [at(-l, -w), at(l, -w), at(l, w), at(-l, w)]
    }

    /// A point `along` and `across` the ruler (page points).
    pub fn at(&self, along: f32, across: f32) -> Point {
        let (c, s) = (self.angle.cos(), self.angle.sin());
        Point::new(self.cx + along * c - across * s, self.cy + along * s + across * c)
    }
}

// --- the "+" beside the current page row -----------------------------------------------------------

/// The "+" radius and how far from its middle a touch still hits it (screen px).
pub const HANDLE_R: f32 = 15.0;
pub const HANDLE_HIT: f32 = HANDLE_R * 1.6;

/// Which page a "+" adds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleDir {
    Left,
    Right,
    Below,
}

/// A "+" beside the current page row: where (page points), for which main page, and the page
/// it would add (its outline, page points).
#[derive(Debug, Clone, PartialEq)]
pub struct Handle {
    pub dir: HandleDir,
    pub at: Point,
    pub main_node: String,
    pub ghost: Rect,
}

/// The "+" beside the main page in the middle of the view: left of its row, right of it and
/// below it (none while the page is small on screen, under 160 px wide).
pub fn handles(pages: &Pages, cam: &Camera, side_gap: f32, row_gap: f32) -> Vec<Handle> {
    let Some(cur) = pages.index_of_number(pages.current_number(cam)).map(|i| &pages.pages[i].placed) else { return Vec::new() };
    if cur.rect.width() * cam.zoom < 160.0 {
        return Vec::new();
    }
    let row: Vec<&Placed> = pages.pages.iter().map(|p| &p.placed).filter(|p| p.main_node == cur.node).collect();
    let left = row.iter().map(|p| p.rect.x0).fold(f32::MAX, f32::min);
    let right = row.iter().map(|p| p.rect.x1).fold(f32::MIN, f32::max);
    let bottom = row.iter().map(|p| p.rect.y1).fold(f32::MIN, f32::max);
    let off = (side_gap / 2.0).max(HANDLE_HIT / cam.zoom);
    let (w, h) = (cur.rect.width(), cur.rect.height());
    let cy = (cur.rect.y0 + cur.rect.y1) / 2.0;
    vec![
        Handle { dir: HandleDir::Left, at: Point::new(left - off, cy), main_node: cur.node.clone(), ghost: Rect::new(left - side_gap - w, cur.rect.y0, left - side_gap, cur.rect.y0 + h) },
        Handle { dir: HandleDir::Right, at: Point::new(right + off, cy), main_node: cur.node.clone(), ghost: Rect::new(right + side_gap, cur.rect.y0, right + side_gap + w, cur.rect.y0 + h) },
        Handle { dir: HandleDir::Below, at: Point::new((cur.rect.x0 + cur.rect.x1) / 2.0, bottom + row_gap / 2.0), main_node: cur.node.clone(), ghost: Rect::new(cur.rect.x0, bottom + row_gap, cur.rect.x1, bottom + row_gap + h) },
    ]
}

/// The "+" under a window point.
pub fn handle_at(handles: &[Handle], cam: &Camera, p: Point) -> Option<Handle> {
    let r = Rectangle::new(cam.origin, cam.viewport);
    handles.iter().find(|h| cam.to_screen(r, h.at.x, h.at.y).distance(p) < HANDLE_HIT).cloned()
}

// --- the writing magnifier ---------------------------------------------------------------------

/// Where the line one writes on sits in the frame (share of its height from the top).
pub const LOUPE_BASELINE: f32 = 0.72;
/// The strip's "move on" part (share of its width at the right).
pub const LOUPE_ADVANCE: f32 = 0.22;
/// How wide the frame starts (share of the page).
pub const LOUPE_SHARE: f32 = 0.34;

/// The magnifier's strip in a page view at `view` (window px): along the bottom, 16 px in from
/// the edges, 30 % of the view high (at least 140 px, at most 60 %).
pub fn loupe_strip(view: Rectangle) -> Rectangle {
    let m = 16.0;
    let h = (view.height * 0.3).clamp(140.0, (view.height * 0.6).max(140.0));
    let top = (view.height - m - h).max(110.0f32.min(view.height - h));
    Rectangle::new(Point::new(view.x + m, view.y + top), Size::new((view.width - 2.0 * m).max(1.0), h))
}

/// The paper's rules (offset, spacing; page points), None on blank paper.
pub fn rules(bg: Background, spacing: f32) -> Option<(f32, f32)> {
    let sp = if spacing >= 4.0 { spacing } else { 20.0 };
    match bg {
        Background::Lines | Background::Grid | Background::Dots => Some((0.0, sp)),
        Background::Cornell => Some((90.0, sp)),
        _ => None,
    }
}

/// How far the frame's next line is: one rule on lined paper, whole boxes for what was
/// written on squared and dotted paper, else a line's height (`ink`: the height written on the
/// line, `frame_h`: the frame's height).
pub fn line_step(bg: Background, spacing: f32, ink: f32, frame_h: f32) -> f32 {
    let sp = if spacing >= 4.0 { spacing } else { 20.0 };
    match bg {
        Background::Lines | Background::Cornell => sp * (ink / sp + 0.35).floor().max(1.0),
        Background::Grid | Background::Dots => sp * ((ink * 1.15).max(frame_h * 0.4) / sp).ceil().max(1.0),
        _ => (ink * 1.35).max(frame_h * 0.45),
    }
}

// --- inserted space ----------------------------------------------------------------------------

/// One millimetre in page points.
pub const MM: f32 = 72.0 / 25.4;

/// The steps space goes in on a page: whole lines on lined, squared and dotted paper (so
/// writing stays on them), millimetres elsewhere; sideways whole boxes on squared and dotted
/// paper. (down, sideways)
pub fn space_steps(p: &Placed) -> (f32, f32) {
    let s = &p.spec;
    let sp = if s.spacing >= 4.0 { s.spacing } else { 20.0 };
    let ruled = s.pdf.is_none() && !p.is_board() && matches!(s.background, Background::Lines | Background::Grid | Background::Dots | Background::Cornell);
    let columns = s.pdf.is_none() && matches!(s.background, Background::Grid | Background::Dots);
    (if ruled { sp } else { MM }, if columns { sp } else { MM })
}

/// What the space being dragged does, told on the page: "+3 Zeilen · Seite +12 mm", "−8 mm",
/// sideways "+2 Kästchen · Seite +10 mm breiter"; "nichts zu entfernen" when there is nothing
/// to take out. `d`: how far what lies beyond moves; `step`: its step; `grow`: how much longer
/// (wider) the page gets; `asked`: what the pen asked for.
pub fn space_label(bg: Background, sideways: bool, d: f32, step: f32, grow: f32, asked: f32) -> String {
    if d == 0.0 {
        return if asked < 0.0 { tr("nichts zu entfernen").into() } else { String::new() };
    }
    let sign = if d > 0.0 { "+" } else { "−" };
    let amount = if step != MM {
        let n = (d.abs() / step).round() as i64;
        match (bg, sideways) {
            (Background::Grid, _) => trf("{} Kästchen", &[&n]),
            (Background::Dots, false) if n == 1 => tr("1 Reihe").into(),
            (Background::Dots, false) => trf("{} Reihen", &[&n]),
            (_, true) if n == 1 => tr("1 Spalte").into(),
            (_, true) => trf("{} Spalten", &[&n]),
            _ if n == 1 => tr("1 Zeile").into(),
            _ => trf("{} Zeilen", &[&n]),
        }
    } else {
        format!("{:.0} mm", d.abs() / MM)
    };
    let page = match (sideways, grow) {
        (true, g) if g > 0.5 => trf(" · Seite +{} mm breiter", &[&format!("{:.0}", g / MM)]),
        (false, g) if g > 0.5 => trf(" · Seite +{} mm", &[&format!("{:.0}", g / MM)]),
        (false, g) if g < -0.5 => trf(" · Seite −{} mm", &[&format!("{:.0}", -g / MM)]),
        _ => String::new(),
    };
    format!("{sign}{amount}{page}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ruler_takes_the_pen_along_its_edge() {
        let r = Ruler { cx: 100.0, cy: 100.0, angle: 0.0 };
        assert!(r.contains(100.0 + RULER_LEN / 2.0 - 1.0, 100.0 + RULER_W / 2.0 - 1.0));
        assert!(!r.contains(100.0, 100.0 + RULER_W / 2.0 + 1.0));
        // Near the bottom edge: it; near the top one: that; in the middle: none.
        assert_eq!(r.edge_at(150.0, 100.0 + RULER_W / 2.0 + 5.0, 18.0), 1);
        assert_eq!(r.edge_at(150.0, 100.0 - RULER_W / 2.0 - 5.0, 18.0), -1);
        assert_eq!(r.edge_at(150.0, 100.0, 18.0), 0);
        assert_eq!(r.edge_at(100.0 + RULER_LEN, 100.0 + RULER_W / 2.0, 18.0), 0, "past its end");
        // Snapped: on the line under the edge, half the line width out.
        let (x, y) = r.snap(150.0, 140.0, 1, 2.0);
        assert!((x - 150.0).abs() < 1e-4 && (y - (100.0 + RULER_W / 2.0 + 1.0)).abs() < 1e-4);
        // Turned 90°: the bottom edge is now on the left.
        let t = r.turned_to(std::f32::consts::FRAC_PI_2);
        let (x, y) = t.snap(80.0, 130.0, 1, 0.0);
        assert!((x - (100.0 - RULER_W / 2.0)).abs() < 1e-3 && (y - 130.0).abs() < 1e-3, "{x} {y}");
    }

    #[test]
    fn the_ruler_snaps_to_45_degrees() {
        let r = Ruler::default();
        assert_eq!(r.turned_to(44.0f32.to_radians()).angle, 45.0f32.to_radians());
        assert_eq!(r.turned_to(91.5f32.to_radians()).angle, 90.0f32.to_radians());
        let free = r.turned_to(40.0f32.to_radians()).angle;
        assert!((free - 40.0f32.to_radians()).abs() < 1e-6);
        assert!((r.turned_to(-90.0f32.to_radians()).degrees() - 270.0).abs() < 1e-3);
    }

    #[test]
    fn the_strip_lies_along_the_bottom() {
        let s = loupe_strip(Rectangle::new(Point::new(100.0, 50.0), Size::new(1000.0, 800.0)));
        assert_eq!((s.x, s.y, s.width), (116.0, 50.0 + 800.0 - 16.0 - 240.0, 968.0));
        assert!((s.height - 240.0).abs() < 1e-3);
        // A low view: at least 140 px high.
        assert_eq!(loupe_strip(Rectangle::new(Point::ORIGIN, Size::new(600.0, 300.0))).height, 140.0);
    }

    #[test]
    fn the_next_line_follows_the_paper() {
        assert_eq!(line_step(Background::Lines, 20.0, 12.0, 60.0), 20.0);
        assert_eq!(line_step(Background::Lines, 20.0, 34.0, 60.0), 40.0, "two lines written: two down");
        assert_eq!(line_step(Background::Grid, 20.0, 12.0, 60.0), 40.0, "whole boxes, at least 40 % of the frame");
        assert!((line_step(Background::Blank, 20.0, 20.0, 60.0) - 27.0).abs() < 1e-4);
        assert_eq!(rules(Background::Cornell, 25.0), Some((90.0, 25.0)));
        assert_eq!(rules(Background::Blank, 25.0), None);
    }

    #[test]
    fn inserted_space_is_told_in_lines_boxes_or_millimetres() {
        assert_eq!(space_label(Background::Lines, false, 60.0, 20.0, 12.0 * MM, 60.0), "+3 Zeilen · Seite +12 mm");
        assert_eq!(space_label(Background::Lines, false, 20.0, 20.0, 0.0, 20.0), "+1 Zeile");
        assert_eq!(space_label(Background::Grid, true, 40.0, 20.0, 10.0 * MM, 40.0), "+2 Kästchen · Seite +10 mm breiter");
        assert_eq!(space_label(Background::Blank, false, -8.0 * MM, MM, -8.0 * MM, -9.0 * MM), "−8 mm · Seite −8 mm");
        assert_eq!(space_label(Background::Dots, false, 24.0, 24.0, 0.0, 24.0), "+1 Reihe");
        assert_eq!(space_label(Background::Lines, false, 0.0, 20.0, 0.0, -20.0), "nichts zu entfernen");
        assert_eq!(space_label(Background::Lines, false, 0.0, 20.0, 0.0, 0.0), "");
    }
}
