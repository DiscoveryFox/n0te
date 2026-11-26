//! Drawing instruments on the page, the same on every device: the ruler, the Geodreieck (a
//! set square with a protractor scale, as German schools use it), the protractor and the
//! compass. Pure geometry in page points, true to size (a page point is 1/72 inch): where an
//! instrument lies, which edge the pen is at and where the pen goes along it, the scale's marks
//! and numbers for drawing, the compass's circle and arcs, and the points a pin snaps to.

use crate::ink::InkPoint;

/// Page points per millimetre.
pub const MM: f32 = 72.0 / 25.4;

/// Which instrument.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A straight ruler with a millimetre scale on both edges.
    Ruler,
    /// The Geodreieck: a right isosceles triangle, its long side (16 cm) with a millimetre scale
    /// from the 0 in its middle, a degree scale round that 0.
    SetSquare,
    /// A half-circle protractor (10 cm across), its middle on the base line.
    Protractor,
}

/// The ruler's length and width (page points; as on the tablet before the instruments came).
pub const RULER_LEN: f32 = 560.0;
pub const RULER_W: f32 = 56.0;
/// The Geodreieck's long side.
pub const SET_SQUARE_LEN: f32 = 160.0 * MM;
/// The protractor's radius.
pub const PROTRACTOR_R: f32 = 50.0 * MM;

/// An instrument lying on the pages: its kind, the point it turns about (page points: the
/// ruler's middle, the Geodreieck's 0, the protractor's centre) and its angle (radians).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Instrument {
    pub kind: Kind,
    pub cx: f32,
    pub cy: f32,
    pub angle: f32,
}

/// A mark of a scale (instrument coordinates): a line from `a` to `b`, `level` 0 small, 1
/// middle, 2 long.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mark {
    pub a: (f32, f32),
    pub b: (f32, f32),
    pub level: u8,
}

/// A number on a scale (instrument coordinates): its middle, the text, its turn (radians).
#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub turn: f32,
    /// Font size (page points).
    pub size: f32,
}

impl Instrument {
    pub fn new(kind: Kind, cx: f32, cy: f32) -> Instrument {
        Instrument { kind, cx, cy, angle: 0.0 }
    }

    /// A page point in the instrument's own coordinates (x along its base, y across: negative
    /// towards the triangle's tip and the protractor's arc).
    pub fn local(&self, x: f32, y: f32) -> (f32, f32) {
        let (dx, dy) = (x - self.cx, y - self.cy);
        let (c, s) = (self.angle.cos(), self.angle.sin());
        (dx * c + dy * s, -dx * s + dy * c)
    }

    /// An instrument point on the page.
    pub fn page(&self, lx: f32, ly: f32) -> (f32, f32) {
        let (c, s) = (self.angle.cos(), self.angle.sin());
        (self.cx + lx * c - ly * s, self.cy + lx * s + ly * c)
    }

    /// Its outline (instrument coordinates), a closed polygon.
    pub fn outline(&self) -> Vec<(f32, f32)> {
        match self.kind {
            Kind::Ruler => {
                let (l, w) = (RULER_LEN / 2.0, RULER_W / 2.0);
                vec![(-l, -w), (l, -w), (l, w), (-l, w)]
            }
            Kind::SetSquare => {
                let h = SET_SQUARE_LEN / 2.0;
                vec![(-h, 0.0), (0.0, -h), (h, 0.0)]
            }
            Kind::Protractor => {
                let r = PROTRACTOR_R;
                let mut out: Vec<(f32, f32)> = (0..=60).map(|i| {
                    let a = std::f32::consts::PI * i as f32 / 60.0;
                    (r * a.cos(), -r * a.sin())
                }).collect();
                out.reverse();
                out
            }
        }
    }

    /// Its straight edges a pen can draw along (instrument coordinates).
    pub fn edges(&self) -> Vec<((f32, f32), (f32, f32))> {
        match self.kind {
            Kind::Ruler => {
                let (l, w) = (RULER_LEN / 2.0, RULER_W / 2.0);
                vec![((-l, -w), (l, -w)), ((-l, w), (l, w))]
            }
            Kind::SetSquare => {
                let h = SET_SQUARE_LEN / 2.0;
                vec![((-h, 0.0), (h, 0.0)), ((-h, 0.0), (0.0, -h)), ((0.0, -h), (h, 0.0))]
            }
            Kind::Protractor => vec![((-PROTRACTOR_R, 0.0), (PROTRACTOR_R, 0.0))],
        }
    }

    /// Whether a page point lies on the instrument.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        let (lx, ly) = self.local(x, y);
        let poly = self.outline();
        let mut inside = false;
        let n = poly.len();
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + n - 1) % n]);
            if (a.1 > ly) != (b.1 > ly) && lx < (b.0 - a.0) * (ly - a.1) / (b.1 - a.1) + a.0 {
                inside = !inside;
            }
        }
        inside
    }

    /// The edge a pen at a page point is at (within `tol` page points), if any: the nearest.
    pub fn edge_at(&self, x: f32, y: f32, tol: f32) -> Option<usize> {
        let (lx, ly) = self.local(x, y);
        self.edges()
            .iter()
            .enumerate()
            .filter_map(|(i, &(a, b))| {
                let (t, d) = along(lx, ly, a, b);
                (t >= -0.01 && t <= 1.01 && d <= tol).then_some((i, d))
            })
            .min_by(|p, q| p.1.total_cmp(&q.1))
            .map(|(i, _)| i)
    }

    /// A page point moved onto edge `edge`, outside it by half the line `width` (the ink lies
    /// against the instrument, like a pen against a real one).
    pub fn snap(&self, x: f32, y: f32, edge: usize, width: f32) -> (f32, f32) {
        let edges = self.edges();
        let Some(&(a, b)) = edges.get(edge) else { return (x, y) };
        let (lx, ly) = self.local(x, y);
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = dx.hypot(dy).max(1e-6);
        let (ux, uy) = (dx / len, dy / len);
        let t = (lx - a.0) * ux + (ly - a.1) * uy;
        // Outward: away from the instrument's middle.
        let (mx, my) = self.middle();
        let (mut nx, mut ny) = (-uy, ux);
        if (a.0 - mx) * nx + (a.1 - my) * ny < 0.0 {
            (nx, ny) = (-nx, -ny);
        }
        let off = width / 2.0;
        self.page(a.0 + ux * t + nx * off, a.1 + uy * t + ny * off)
    }

    /// The middle of the outline (instrument coordinates).
    fn middle(&self) -> (f32, f32) {
        let p = self.outline();
        let n = p.len() as f32;
        (p.iter().map(|q| q.0).sum::<f32>() / n, p.iter().map(|q| q.1).sum::<f32>() / n)
    }

    /// Turned to `raw` radians, snapping to the nearest 45° (ruler) or 15° (the others) within 2°.
    pub fn turned_to(&self, raw: f32) -> Instrument {
        let step = if self.kind == Kind::Ruler { 45.0 } else { 15.0 };
        let deg = raw.to_degrees();
        let near = (deg / step).round() * step;
        let angle = if (deg - near).abs() < 2.0 { near.to_radians() } else { raw };
        Instrument { angle, ..*self }
    }

    /// The angle it lies at (0–359°).
    pub fn degrees(&self) -> f32 {
        (self.angle.to_degrees() % 360.0 + 360.0) % 360.0
    }

    /// The angle (0–180°) a page point is at, seen from the 0 of a Geodreieck or the centre of a
    /// protractor along its base: what its degree scale reads there. None for the ruler and
    /// below the base.
    pub fn reads(&self, x: f32, y: f32) -> Option<f32> {
        if self.kind == Kind::Ruler {
            return None;
        }
        let (lx, ly) = self.local(x, y);
        if ly > 0.5 || lx.hypot(ly) < 2.0 {
            return None;
        }
        Some((-ly).atan2(lx).to_degrees().clamp(0.0, 180.0))
    }

    /// The scale's marks and numbers (instrument coordinates).
    pub fn scale(&self) -> (Vec<Mark>, Vec<Label>) {
        let mut marks = Vec::new();
        let mut labels = Vec::new();
        let mm_marks = |marks: &mut Vec<Mark>, labels: &mut Vec<Label>, from: f32, to: f32, zero: f32, y: f32, dir: f32, numbered: bool| {
            let first = ((from - zero) / MM).ceil() as i32;
            let last = ((to - zero) / MM).floor() as i32;
            for k in first..=last {
                let x = zero + k as f32 * MM;
                let (level, len) = if k % 10 == 0 { (2, 12.0) } else if k % 5 == 0 { (1, 8.0) } else { (0, 5.0) };
                marks.push(Mark { a: (x, y), b: (x, y + dir * len), level });
                if numbered && k % 10 == 0 {
                    labels.push(Label { x, y: y + dir * 20.0, text: format!("{}", (k / 10).abs()), turn: 0.0, size: 9.0 });
                }
            }
        };
        match self.kind {
            Kind::Ruler => {
                let (l, w) = (RULER_LEN / 2.0, RULER_W / 2.0);
                mm_marks(&mut marks, &mut labels, -l + 4.0, l - 4.0, -l + 8.0, -w, 1.0, true);
                mm_marks(&mut marks, &mut labels, -l + 4.0, l - 4.0, -l + 8.0, w, -1.0, false);
            }
            Kind::SetSquare => {
                let h = SET_SQUARE_LEN / 2.0;
                // Millimetres both ways from the 0 in the middle of the long side.
                mm_marks(&mut marks, &mut labels, -h + 6.0, h - 6.0, 0.0, 0.0, -1.0, true);
                // Degrees round the 0, inside the triangle.
                let r = 0.3 * SET_SQUARE_LEN;
                degree_marks(&mut marks, &mut labels, r, 6.0);
            }
            Kind::Protractor => degree_marks(&mut marks, &mut labels, PROTRACTOR_R, 10.0),
        }
        (marks, labels)
    }
}

/// Degree marks every degree on the arc of radius `r` about the origin (above the base), a
/// number every 10°.
fn degree_marks(marks: &mut Vec<Mark>, labels: &mut Vec<Label>, r: f32, inset: f32) {
    let r = r - inset;
    for d in 0..=180 {
        let a = (d as f32).to_radians();
        let (c, s) = (a.cos(), -a.sin());
        let (level, len) = if d % 10 == 0 { (2, 12.0) } else if d % 5 == 0 { (1, 8.0) } else { (0, 5.0) };
        marks.push(Mark { a: (r * c, r * s), b: ((r - len) * c, (r - len) * s), level });
        if d % 10 == 0 && d != 0 && d != 180 {
            let rl = r - 22.0;
            labels.push(Label { x: rl * c, y: rl * s, text: d.to_string(), turn: std::f32::consts::FRAC_PI_2 - a, size: 8.0 });
        }
    }
    marks.push(Mark { a: (0.0, 0.0), b: (0.0, -10.0), level: 2 });
}

/// How far along the segment a–b a point lies (0 at a, 1 at b) and how far from its line.
fn along(x: f32, y: f32, a: (f32, f32), b: (f32, f32)) -> (f32, f32) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = (dx * dx + dy * dy).max(1e-9);
    let t = ((x - a.0) * dx + (y - a.1) * dy) / l2;
    let (px, py) = (a.0 + dx * t, a.1 + dy * t);
    (t, (x - px).hypot(y - py))
}

/// The compass: its needle and radius (page points).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Compass {
    pub cx: f32,
    pub cy: f32,
    pub r: f32,
}

impl Compass {
    /// Where the pencil is when the pen is at (x, y): on the circle, in that direction.
    pub fn point(&self, x: f32, y: f32) -> (f32, f32) {
        let a = self.angle_of(x, y);
        (self.cx + self.r * a.cos(), self.cy + self.r * a.sin())
    }

    /// The direction of (x, y) from the needle (radians).
    pub fn angle_of(&self, x: f32, y: f32) -> f32 {
        (y - self.cy).atan2(x - self.cx)
    }

    /// The arc from angle `a0` turning by `sweep` (radians, either way; ±2π is the whole
    /// circle), as samples about two points apart, `pressure` all along, `t0`…`t1` ms.
    pub fn arc(&self, a0: f32, sweep: f32, pressure: f32, t0: u32, t1: u32) -> Vec<InkPoint> {
        let sweep = sweep.clamp(-std::f32::consts::TAU, std::f32::consts::TAU);
        let n = ((self.r * sweep.abs() / 2.0).ceil() as usize).clamp(2, 4000);
        (0..=n)
            .map(|i| {
                let f = i as f32 / n as f32;
                let a = a0 + sweep * f;
                InkPoint { x: self.cx + self.r * a.cos(), y: self.cy + self.r * a.sin(), pressure, t_ms: t0 + ((t1.saturating_sub(t0)) as f32 * f) as u32 }
            })
            .collect()
    }

    /// The radius as it reads: "3,5 cm" (German decimal comma; `en` for a point).
    pub fn label(&self, en: bool) -> String {
        let cm = self.r / MM / 10.0;
        let s = format!("{cm:.1} cm");
        if en { s } else { s.replace('.', ",") }
    }
}

/// How far the pen turned from angle `from` to `to`, the short way (radians, −π…π): the
/// compass adds these up so an arc can go round more than half a circle.
pub fn turned(from: f32, to: f32) -> f32 {
    let mut d = to - from;
    while d > std::f32::consts::PI {
        d -= std::f32::consts::TAU;
    }
    while d < -std::f32::consts::PI {
        d += std::f32::consts::TAU;
    }
    d
}

/// A compass radius snapped: to whole squares on squared paper (`square`: their size), else to
/// half centimetres, when within a sixth of a step.
pub fn snap_radius(r: f32, square: Option<f32>) -> f32 {
    let step = square.filter(|s| *s > 1.0).unwrap_or(5.0 * MM);
    let near = (r / step).round() * step;
    if near > 0.0 && (r - near).abs() < step / 6.0 { near } else { r }
}

/// Where a pin (the compass needle, the Geodreieck's 0) snaps near (x, y): the nearest end of a
/// stroke or crossing of two straight strokes within `radius` page points.
pub fn snap_point(strokes: &[&[InkPoint]], x: f32, y: f32, radius: f32) -> Option<(f32, f32)> {
    let ends: Vec<((f32, f32), (f32, f32), f32)> = strokes
        .iter()
        .filter_map(|s| {
            let (f, l) = (s.first()?, s.last()?);
            let len: f32 = s.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum();
            Some(((f.x, f.y), (l.x, l.y), len))
        })
        .collect();
    snap_point_ends(&ends, x, y, radius)
}

/// [`snap_point`] from each stroke's ends and length (apps that keep only those).
pub fn snap_point_ends(strokes: &[((f32, f32), (f32, f32), f32)], x: f32, y: f32, radius: f32) -> Option<(f32, f32)> {
    let mut best: Option<((f32, f32), f32)> = None;
    let mut consider = |p: (f32, f32)| {
        let d = (p.0 - x).hypot(p.1 - y);
        if d <= radius && best.is_none_or(|b| d < b.1) {
            best = Some((p, d));
        }
    };
    let mut lines: Vec<((f32, f32), (f32, f32))> = Vec::new();
    for &(f, l, len) in strokes {
        consider(f);
        consider(l);
        let chord = (l.0 - f.0).hypot(l.1 - f.1);
        if len > 4.0 && chord >= 0.92 * len {
            lines.push((f, l));
        }
    }
    for (i, a) in lines.iter().enumerate() {
        for b in &lines[i + 1..] {
            if let Some(p) = crossing(*a, *b) {
                consider(p);
            }
        }
    }
    best.map(|b| b.0)
}

/// Where two segments cross, if they do (a little past their ends counts: lines drawn to meet
/// often stop just short).
fn crossing(a: ((f32, f32), (f32, f32)), b: ((f32, f32), (f32, f32))) -> Option<(f32, f32)> {
    let (p, r) = (a.0, (a.1.0 - a.0.0, a.1.1 - a.0.1));
    let (q, s) = (b.0, (b.1.0 - b.0.0, b.1.1 - b.0.1));
    let den = r.0 * s.1 - r.1 * s.0;
    if den.abs() < 1e-6 {
        return None;
    }
    let t = ((q.0 - p.0) * s.1 - (q.1 - p.1) * s.0) / den;
    let u = ((q.0 - p.0) * r.1 - (q.1 - p.1) * r.0) / den;
    let slack = |len: f32| 4.0 / len.max(1.0);
    let (lr, ls) = (r.0.hypot(r.1), s.0.hypot(s.1));
    ((-slack(lr)..=1.0 + slack(lr)).contains(&t) && (-slack(ls)..=1.0 + slack(ls)).contains(&u)).then(|| (p.0 + r.0 * t, p.1 + r.1 * t))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f32, y: f32) -> InkPoint {
        InkPoint { x, y, pressure: 0.5, t_ms: 0 }
    }

    #[test]
    fn the_geodreieck_is_true_to_size_and_the_pen_follows_each_edge() {
        let g = Instrument::new(Kind::SetSquare, 300.0, 400.0);
        assert!((SET_SQUARE_LEN - 453.54).abs() < 0.1, "16 cm");
        assert_eq!(g.edges().len(), 3);
        // Just below the long side (outside): on it, half a line width away.
        assert_eq!(g.edge_at(310.0, 403.0, 6.0), Some(0));
        let (x, y) = g.snap(310.0, 403.0, 0, 2.0);
        assert!((x - 310.0).abs() < 1e-3 && (y - 401.0).abs() < 1e-3, "({x}, {y})");
        // Inside, near the tip: not at an edge; on it.
        assert!(g.contains(300.0, 300.0) && !g.contains(300.0, 450.0));
        // At the left leg (45°): the pen goes along it, outside.
        let at = g.page(-100.0, -SET_SQUARE_LEN / 2.0 + 100.0 - 2.0);
        let e = g.edge_at(at.0, at.1, 6.0);
        assert_eq!(e, Some(1));
        // The degree scale reads 90° straight up from the 0, 45° along the right leg.
        assert!((g.reads(300.0, 300.0).unwrap() - 90.0).abs() < 0.01);
        assert!((g.reads(400.0, 300.0).unwrap() - 45.0).abs() < 0.01);
        let (marks, labels) = g.scale();
        assert!(marks.len() > 300 && labels.iter().any(|l| l.text == "90") && labels.iter().any(|l| l.text == "7"), "{} marks", marks.len());
    }

    #[test]
    fn turned_it_snaps_to_fifteen_degrees_and_its_edges_turn_with_it() {
        let g = Instrument::new(Kind::SetSquare, 0.0, 0.0).turned_to(29.0f32.to_radians());
        assert!((g.degrees() - 30.0).abs() < 1e-3);
        assert!((Instrument::new(Kind::Ruler, 0.0, 0.0).turned_to(29.0f32.to_radians()).degrees() - 29.0).abs() < 1e-3, "the ruler snaps to 45° only");
        // The long side now runs at 30°: a point on it projects onto it.
        let on = g.page(50.0, 0.0);
        let (x, y) = g.snap(on.0 + 1.0, on.1 + 1.0, 0, 0.0);
        let (lx, ly) = g.local(x, y);
        assert!(ly.abs() < 1e-3 && (lx - 50.0).abs() < 1.5);
    }

    #[test]
    fn the_compass_draws_arcs_round_its_needle_and_its_radius_reads_in_cm() {
        let c = Compass { cx: 100.0, cy: 100.0, r: 5.0 * 10.0 * MM };
        assert_eq!(c.label(false), "5,0 cm");
        let (x, y) = c.point(300.0, 100.0);
        assert!((x - (100.0 + c.r)).abs() < 1e-3 && (y - 100.0).abs() < 1e-3);
        let full = c.arc(0.0, std::f32::consts::TAU, 0.5, 0, 900);
        assert!(full.iter().all(|p| ((p.x - 100.0).hypot(p.y - 100.0) - c.r).abs() < 0.01));
        let (a, b) = (full[0], full[full.len() - 1]);
        assert!((a.x - b.x).abs() < 0.01 && (a.y - b.y).abs() < 0.01, "closed");
        // Going round past half a circle adds up.
        let mut total = 0.0;
        let mut prev = 0.0f32;
        for k in 1..=10 {
            let a = (k as f32 * 36.0).to_radians();
            total += turned(prev, (a.sin()).atan2(a.cos()));
            prev = (a.sin()).atan2(a.cos());
        }
        assert!((total - std::f32::consts::TAU).abs() < 1e-3);
        assert_eq!(snap_radius(5.0 * MM * 3.02, None), 15.0 * MM);
        assert_eq!(snap_radius(20.0 * 3.1, Some(20.0)), 60.0, "whole squares");
    }

    #[test]
    fn pins_snap_to_stroke_ends_and_crossings() {
        let h: Vec<InkPoint> = (0..=20).map(|i| pt(i as f32 * 10.0, 100.0)).collect();
        let v: Vec<InkPoint> = (0..=20).map(|i| pt(80.0, i as f32 * 10.0)).collect();
        let strokes: Vec<&[InkPoint]> = vec![&h, &v];
        assert_eq!(snap_point(&strokes, 83.0, 97.0, 8.0), Some((80.0, 100.0)), "the crossing");
        assert_eq!(snap_point(&strokes, 198.0, 103.0, 8.0), Some((200.0, 100.0)), "an end");
        assert_eq!(snap_point(&strokes, 140.0, 140.0, 8.0), None);
    }
}
