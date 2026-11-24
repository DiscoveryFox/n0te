//! When a stroke becomes a shape or an arrow, and what the pen does with it while it stays
//! down: the rules around [`crate::shapes`] and [`crate::arrows`] as the Android tablet's
//! NotebookView.kt applies them (`actionMove`, `trySnap`, `moveHeldLine`, `wiggle`,
//! `moveHeldShape`, `commitInk`), for any view to follow:
//!
//! 1. While the pen is down, a [`Rest`] tracks where it last settled. On every move event
//!    (before the sample is appended) call [`Rest::moved`]; each time it returns true, (re)start
//!    two timers: after [`SHAPE_HOLD_MS`] call [`on_hold`] with [`Hold::Shape`], after
//!    [`ARROW_HOLD_MS`] with [`Hold::Arrow`].
//! 2. When [`on_hold`] returns a [`Snap`], the stroke's samples become `snap.shape.points` at
//!    once (light tick), the stroke `snap.delete` (an earlier barb) is deleted, and
//!    [`Held::new`] says what moving the pen does from now on: a line's end follows it
//!    ([`HeldLine`]: 15°/45° snapping, a wiggle frees it), a closed shape's corner follows it
//!    ([`HeldDrag`]: resize), an arrow stays as it is. The timers no longer matter.
//! 3. At pen-up of a stroke that did not snap: [`on_lift`] (the pen held still at the end
//!    still makes a shape or a head). A stroke that snapped is stored as it shows.
//!
//! Nothing snaps while a stroke runs along the ruler or after it crossed onto another page
//! (the view checks that before calling).

use std::collections::VecDeque;

use crate::arrows::{self, LineStroke};
use crate::ink::InkPoint;
use crate::shapes::{self, HeldShape, Shape, ShapeKind, atan2, hypot, round_half_up};

/// How far the pen may stray (screen px) and still rest: [`Rest`], and `still` for the
/// recognisers (`STILL_PX / scale` sheet points).
pub const STILL_PX: f32 = 6.0;
/// The pen resting this long (from where it last moved off) snaps the stroke to a shape.
pub const SHAPE_HOLD_MS: u32 = 500;
/// Arrows want a longer hold than shapes (writing pauses for half a second, not for a whole one).
pub const ARROW_HOLD_MS: u32 = 900;
/// At pen-up: the pen rested longer than this at the end (shapes, triangle heads).
pub const LIFT_HOLD_MS: u32 = 600;
/// Arrowheads at a line's end only for strokes of at most this many samples.
pub const MAX_HEAD_SAMPLES: usize = 600;
/// The view appends a sample only this far (screen px) from the last one (fewer near-duplicates).
pub const MIN_STEP_PX: f32 = 0.4;
/// A held shape follows the pen only once it moved this far (screen px) from where it snapped:
/// the pen shifting as it lifts changes nothing.
pub const HELD_SHAPE_PLAY_PX: f32 = 6.0;
/// A held line follows the pen only this far (screen px) from its start.
pub const HELD_LINE_MIN_PX: f32 = 3.0;
/// A wiggle across a held line: turns back and forth of this many dp (density-independent px)…
pub const WIGGLE_DP: f32 = 6.0;
/// … three of them within this long …
pub const WIGGLE_WINDOW_MS: u32 = 800;
/// … then wiggles are ignored for this long.
pub const WIGGLE_QUIET_MS: u32 = 600;

/// What the user has switched on, and the tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Settings: recognise shapes (on by default). Off: nothing snaps, no arrows either.
    pub shapes: bool,
    /// Settings: recognise arrows (on by default).
    pub arrows: bool,
    /// Writing with the pen tool (not the marker): only then a stroke can be a head at a line's end.
    pub pen: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options { shapes: true, arrows: true, pen: true }
    }
}

/// Where the pen last settled while a stroke is drawn, and since when.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rest {
    since: u32,
    x: f32,
    y: f32,
}

impl Rest {
    /// The pen went down at (x, y) (sheet points) at `t_ms` (ms since the stroke began).
    pub fn new(x: f32, y: f32, t_ms: u32) -> Rest {
        Rest { since: t_ms, x, y }
    }

    /// The pen at (x, y) at `t_ms`, the view showing `scale` screen px per sheet point: true when
    /// it moved off where it rested (more than [`STILL_PX`]); it rests here from now on, so the
    /// hold timers start over.
    pub fn moved(&mut self, x: f32, y: f32, t_ms: u32, scale: f32) -> bool {
        if hypot(x - self.x, y - self.y) > STILL_PX / scale {
            *self = Rest { since: t_ms, x, y };
            true
        } else {
            false
        }
    }

    /// The stroke goes on on another page from (x, y) (in that page's points): the pen rests
    /// there, since when it did.
    pub fn relocate(&mut self, x: f32, y: f32) {
        self.x = x;
        self.y = y;
    }

    /// When the pen last moved off (ms since the stroke began).
    pub fn since(&self) -> u32 {
        self.since
    }

    /// How long the pen has rested at `now_ms`.
    pub fn held_ms(&self, now_ms: u32) -> u32 {
        now_ms.saturating_sub(self.since)
    }
}

/// Which hold timer fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hold {
    /// [`SHAPE_HOLD_MS`]: a shape, or a small triangle at a line's end as its head.
    Shape,
    /// [`ARROW_HOLD_MS`]: a head at a line's end (a V, a triangle, the second of two barbs) or an
    /// arrow drawn in one stroke.
    Arrow,
}

/// What the stroke became: its shape (the samples to show and store) and, for a head drawn as
/// two barbs, the index (in the `lines` passed) of the earlier barb stroke, which goes.
#[derive(Debug, Clone, PartialEq)]
pub struct Snap {
    pub shape: Shape,
    pub delete: Option<usize>,
}

impl Snap {
    fn arrow(points: Vec<InkPoint>, delete: Option<usize>) -> Snap {
        Snap { shape: Shape::arrow(points), delete }
    }
}

/// A hold timer fired while the stroke `points` (all of it so far) is being drawn: what it
/// becomes, or None (it goes on as drawn). `scale`: screen px per sheet point; `lines`: the
/// strokes on its page (without hidden ones) for heads at their ends, see [`LineStroke::of`].
/// A head in the making (a V or a barb at a line's end) is left alone by the shapes: it waits
/// for the arrow hold. NotebookView.kt `trySnap`.
pub fn on_hold(hold: Hold, points: &[InkPoint], scale: f32, lines: &[LineStroke], o: Options) -> Option<Snap> {
    if !o.shapes || points.len() < 5 {
        return None;
    }
    let still = STILL_PX / scale;
    let heads = o.arrows && o.pen && points.len() <= MAX_HEAD_SAMPLES;
    match hold {
        Hold::Shape => {
            // A triangle at a line's end becomes its head at once; a V waits for the longer hold.
            if heads && let Some(h) = arrows::head_at_line_ends(points, still, lines, true) {
                return Some(Snap::arrow(h, None));
            }
            if heads && (arrows::head_at_line_ends(points, still, lines, false).is_some() || arrows::barb_pair(points, still, lines).is_some()) {
                return None;
            }
            shapes::detect(points, still, false).map(|shape| Snap { shape, delete: None })
        }
        Hold::Arrow => {
            if !o.arrows {
                return None;
            }
            if heads {
                if let Some(h) = arrows::head_at_line_ends(points, still, lines, false) {
                    return Some(Snap::arrow(h, None));
                }
                if let Some((h, first)) = arrows::barb_pair(points, still, lines) {
                    return Some(Snap::arrow(h, Some(first)));
                }
            }
            let r = shapes::trimmed(points, still)?;
            arrows::from_stroke(&points[r], false).map(|h| Snap::arrow(h, None))
        }
    }
}

/// The pen lifted off the stroke `points`, which did not snap, after resting `held_ms` at its
/// end ([`Rest::held_ms`] at the pen-up time): what it becomes, or None (stored as drawn).
/// Held still at the end (only then: writing never turns into shapes): a head at a line's end
/// (a V or two barbs after [`ARROW_HOLD_MS`], a triangle after [`LIFT_HOLD_MS`]), or a clean
/// line, triangle, rectangle, ellipse, … (an arrow in one stroke after [`ARROW_HOLD_MS`]).
/// NotebookView.kt `commitInk` (a scribble over ink that erases it is checked before this; the
/// fountain pen's thin lift-off ([`crate::pens::Fountain::taper_end`]) is for strokes that
/// stayed as drawn).
pub fn on_lift(points: &[InkPoint], scale: f32, held_ms: u32, lines: &[LineStroke], o: Options) -> Option<Snap> {
    let n = points.len();
    let still = STILL_PX / scale;
    let held = o.shapes && n > 3 && held_ms > LIFT_HOLD_MS;
    let held_long = held && o.arrows && held_ms > ARROW_HOLD_MS;
    let heads = o.pen && n <= MAX_HEAD_SAMPLES;
    if held_long && heads {
        if let Some(h) = arrows::head_at_line_ends(points, still, lines, false) {
            return Some(Snap::arrow(h, None));
        }
        if let Some((h, first)) = arrows::barb_pair(points, still, lines) {
            return Some(Snap::arrow(h, Some(first)));
        }
    } else if held && o.arrows && heads {
        // A triangle at a line's end: a head, not a triangle turned the way it was drawn.
        if let Some(h) = arrows::head_at_line_ends(points, still, lines, true) {
            return Some(Snap::arrow(h, None));
        }
    }
    if held && n > 4 {
        return shapes::detect(points, still, held_long).map(|shape| Snap { shape, delete: None });
    }
    None
}

/// What moving the pen does to a stroke that snapped while the pen stays down.
#[derive(Debug, Clone)]
pub enum Held {
    Line(HeldLine),
    Shape(HeldDrag),
}

/// Moving the held pen changed the stroke.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeldUpdate {
    /// The stroke's new samples (None: it stays as it shows).
    pub points: Option<Vec<InkPoint>>,
    /// A light tick: the line snapped to another angle, the shape became a square or circle.
    pub tick: bool,
    /// A wiggle across the held line switched its angle snapping (a long buzz and a message):
    /// Some(true) free ("Linie frei: rastet nicht mehr ein"), Some(false) snapping again
    /// ("Linie rastet wieder ein").
    pub freed: Option<bool>,
}

impl Held {
    /// `shape` just snapped with the pen at (pen_x, pen_y) (the stroke's last sample before it
    /// snapped): a line's end follows the pen, a closed shape's corner does. None for an arrow
    /// (it stays as it is, moving the pen does nothing until it lifts).
    pub fn new(shape: &Shape, pen_x: f32, pen_y: f32) -> Option<Held> {
        if shape.kind == ShapeKind::Line {
            return HeldLine::new(&shape.points).map(Held::Line);
        }
        if shape.corners.is_some() || shape.ellipse.is_some() {
            return HeldShape::new(shape, pen_x, pen_y).map(|h| Held::Shape(HeldDrag::new(h, pen_x, pen_y)));
        }
        None
    }

    /// The pen at (x, y) (sheet points) at `t_ms` (since the stroke began); `scale`: screen px
    /// per sheet point, `density`: screen px per dp.
    pub fn pen_at(&mut self, x: f32, y: f32, t_ms: u32, scale: f32, density: f32) -> HeldUpdate {
        match self {
            Held::Line(l) => l.pen_at(x, y, t_ms, scale, density),
            Held::Shape(s) => s.pen_at(x, y, t_ms, scale),
        }
    }
}

/// A line that snapped while the pen is still down: its start stays, its end follows the pen
/// and it stays straight, snapping to 45° steps (within 6°) and 15° steps (within 3°), with a
/// tick at each new step. A quick wiggle across it (three turns back and forth of
/// [`WIGGLE_DP`] or more within [`WIGGLE_WINDOW_MS`]) switches the snapping off, another one
/// back on. NotebookView.kt `moveHeldLine` and `wiggle`.
#[derive(Debug, Clone)]
pub struct HeldLine {
    ax: f32,
    ay: f32,
    pressure: f32,
    /// Across the line as snapped (unit): where a wiggle is measured.
    nx: f32,
    ny: f32,
    free: bool,
    snap_deg: Option<i32>,
    wiggle: Wiggle,
}

impl HeldLine {
    /// The snapped line's samples (from [`shapes::detect`]); None if empty.
    pub fn new(line: &[InkPoint]) -> Option<HeldLine> {
        let (a, b) = (line.first()?, line.last()?);
        let (ex, ey) = (b.x - a.x, b.y - a.y);
        let l = hypot(ex, ey).max(1e-3);
        Some(HeldLine { ax: a.x, ay: a.y, pressure: a.pressure, nx: -ey / l, ny: ex / l, free: false, snap_deg: None, wiggle: Wiggle::default() })
    }

    /// Whether a wiggle switched the angle snapping off.
    pub fn free(&self) -> bool {
        self.free
    }

    /// The pen at (x, y) at `t_ms`: the line from its start to there (see [`Held::pen_at`]).
    pub fn pen_at(&mut self, x: f32, y: f32, t_ms: u32, scale: f32, density: f32) -> HeldUpdate {
        let mut up = HeldUpdate::default();
        let (mut dx, mut dy) = (x - self.ax, y - self.ay);
        if self.wiggle.feed((dx * self.nx + dy * self.ny) * scale, t_ms, WIGGLE_DP * density) {
            self.free = !self.free;
            self.snap_deg = None;
            up.freed = Some(self.free);
        }
        let len = hypot(dx, dy);
        if len < HELD_LINE_MIN_PX / scale {
            return up;
        }
        let deg = (atan2(dy, dx) as f64).to_degrees();
        let (s45, s15) = (round_half_up(deg / 45.0) * 45.0, round_half_up(deg / 15.0) * 15.0);
        let snap = if self.free {
            None
        } else if (deg - s45).abs() < 6.0 {
            Some(s45)
        } else if (deg - s15).abs() < 3.0 {
            Some(s15)
        } else {
            None
        };
        if let Some(s) = snap {
            dx = (len as f64 * s.to_radians().cos()) as f32;
            dy = (len as f64 * s.to_radians().sin()) as f32;
        }
        let step = snap.map(|s| round_half_up(s) as i32);
        if step != self.snap_deg {
            up.tick = step.is_some();
            self.snap_deg = step;
        }
        up.points = Some(shapes::line(self.ax, self.ay, self.ax + dx, self.ay + dy, self.pressure, 0.0, t_ms as f32));
        up
    }
}

/// Turns of the pen across a held line (screen px), for a wiggle.
#[derive(Debug, Clone, Default)]
struct Wiggle {
    /// 0 not started, 2 started (no way yet), 1 going up, -1 going down.
    dir: i8,
    hi: f32,
    lo: f32,
    turns: VecDeque<u32>,
    quiet_until: u32,
}

impl Wiggle {
    /// The pen `p` px across the line at `t`, turns of `amp` px or more: true when this
    /// completes a wiggle.
    fn feed(&mut self, p: f32, t: u32, amp: f32) -> bool {
        if t < self.quiet_until {
            return false;
        }
        match self.dir {
            0 => {
                (self.hi, self.lo, self.dir) = (p, p, 2);
            }
            2 => {
                self.hi = self.hi.max(p);
                self.lo = self.lo.min(p);
                if self.hi - p > amp {
                    (self.dir, self.lo) = (-1, p);
                } else if p - self.lo > amp {
                    (self.dir, self.hi) = (1, p);
                }
            }
            1 => {
                if p > self.hi {
                    self.hi = p;
                } else if self.hi - p > amp {
                    (self.dir, self.lo) = (-1, p);
                    self.turns.push_back(t);
                }
            }
            _ => {
                if p < self.lo {
                    self.lo = p;
                } else if p - self.lo > amp {
                    (self.dir, self.hi) = (1, p);
                    self.turns.push_back(t);
                }
            }
        }
        while self.turns.front().is_some_and(|&f| t.saturating_sub(f) > WIGGLE_WINDOW_MS) {
            self.turns.pop_front();
        }
        if self.turns.len() >= 3 {
            self.turns.clear();
            self.dir = 0;
            self.quiet_until = t.saturating_add(WIGGLE_QUIET_MS);
            return true;
        }
        false
    }
}

/// A closed shape that snapped while the pen is still down: once the pen has moved
/// [`HELD_SHAPE_PLAY_PX`] from where it snapped, the corner nearest it follows the pen
/// ([`HeldShape::drag`]), with a tick when a rectangle or ellipse becomes a square or circle.
/// NotebookView.kt `moveHeldShape`.
#[derive(Debug, Clone)]
pub struct HeldDrag {
    shape: HeldShape,
    x: f32,
    y: f32,
    dragging: bool,
    square: bool,
}

impl HeldDrag {
    /// `shape` held, the pen where it snapped at (x, y).
    pub fn new(shape: HeldShape, x: f32, y: f32) -> HeldDrag {
        HeldDrag { shape, x, y, dragging: false, square: false }
    }

    /// The pen at (x, y) at `t_ms` (see [`Held::pen_at`]).
    pub fn pen_at(&mut self, x: f32, y: f32, t_ms: u32, scale: f32) -> HeldUpdate {
        let (dx, dy) = (x - self.x, y - self.y);
        if !self.dragging {
            if hypot(dx, dy) * scale < HELD_SHAPE_PLAY_PX {
                return HeldUpdate::default();
            }
            self.dragging = true;
        }
        let points = self.shape.drag(dx, dy, t_ms as f32);
        let square = self.shape.square();
        let tick = square && !self.square;
        self.square = square;
        HeldUpdate { points: Some(points), tick, freed: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ink::{self, Brush};

    /// A path through `pts`, a sample every 2 pt, 8 ms apart, then the pen resting at its end.
    fn path(pts: &[(f32, f32)], rest: usize) -> Vec<InkPoint> {
        let mut out = Vec::new();
        for w in pts.windows(2) {
            let ((ax, ay), (bx, by)) = (w[0], w[1]);
            let steps = 1.max((hypot(bx - ax, by - ay) / 2.0) as usize);
            for k in 0..steps {
                let f = k as f32 / steps as f32;
                out.push((ax + (bx - ax) * f, ay + (by - ay) * f));
            }
        }
        let last = pts[pts.len() - 1];
        out.extend(std::iter::repeat_n(last, rest + 1));
        out.iter().enumerate().map(|(i, &(x, y))| InkPoint { x, y, pressure: 0.5, t_ms: i as u32 * 8 }).collect()
    }

    fn line_stroke(pts: &[InkPoint]) -> LineStroke {
        LineStroke { bounds: ink::bounds(pts, &Brush::default()), ends: arrows::ends(pts) }
    }

    fn end_of(s: &Shape) -> (f32, f32) {
        let q = s.points[s.points.len() - 1];
        (q.x, q.y)
    }

    const RECT: [(f32, f32); 5] = [(100.0, 100.0), (300.0, 100.0), (300.0, 220.0), (100.0, 220.0), (100.0, 100.0)];

    #[test]
    fn the_pen_rests_until_it_moves_off() {
        let mut r = Rest::new(0.0, 0.0, 0);
        assert!(!r.moved(5.9, 0.0, 10, 1.0));
        assert!(r.moved(6.1, 0.0, 20, 1.0));
        assert_eq!((r.since(), r.held_ms(620)), (20, 600));
        // Zoomed in twice: 6 screen px are 3 sheet points.
        assert!(r.moved(9.2, 0.0, 30, 2.0));
        r.relocate(0.0, 0.0);
        assert_eq!(r.since(), 30);
        assert!(!r.moved(2.9, 0.0, 40, 2.0));
    }

    #[test]
    fn holds_snap_shapes_and_leave_heads_for_the_long_hold() {
        let o = Options::default();
        let rect = path(&RECT, 10);
        let snap = on_hold(Hold::Shape, &rect, 1.0, &[], o).expect("a rectangle");
        assert_eq!((snap.shape.kind, snap.delete), (ShapeKind::Rectangle, None));
        assert!(matches!(Held::new(&snap.shape, 100.0, 100.0), Some(Held::Shape(_))));
        assert_eq!(on_hold(Hold::Shape, &rect, 1.0, &[], Options { shapes: false, ..o }), None, "shapes off");
        assert_eq!(on_hold(Hold::Shape, &rect[..4], 1.0, &[], o), None, "too few samples");

        // A V at a line's end: no shape at the short hold (a head in the making), a head at the long one.
        let line = path(&[(100.0, 200.0), (300.0, 202.0)], 0);
        let lines = [line_stroke(&line)];
        let vee = path(&[(282.0, 186.0), (302.0, 199.0), (279.0, 217.0)], 0);
        assert_eq!(on_hold(Hold::Shape, &vee, 1.0, &lines, o), None);
        let head = on_hold(Hold::Arrow, &vee, 1.0, &lines, o).expect("a head");
        assert_eq!(head.shape.kind, ShapeKind::Arrow);
        assert!(Held::new(&head.shape, 279.0, 217.0).is_none(), "an arrow stays as it is");
        assert_eq!(on_hold(Hold::Arrow, &vee, 1.0, &lines, Options { arrows: false, ..o }), None, "arrows off");
        assert_eq!(on_hold(Hold::Arrow, &vee, 1.0, &lines, Options { pen: false, ..o }), None, "the marker draws no heads");

        // A small triangle at a line's end: its head at once.
        let e = arrows::ends(&line).unwrap().end;
        let tri: Vec<(f32, f32)> = (0..=3)
            .map(|k| {
                let t = (35.0 + 120.0 * (k % 3) as f64).to_radians();
                (e.x - 4.0 + 9.0 * t.cos() as f32, e.y + 1.0 + 9.0 * t.sin() as f32)
            })
            .collect();
        let tri = path(&tri, 0);
        assert_eq!(on_hold(Hold::Shape, &tri, 1.0, &lines, o).map(|s| s.shape.kind), Some(ShapeKind::Arrow));

        // An arrow in one stroke: only at the long hold.
        let both = path(&[(100.0, 200.0), (300.0, 200.0), (276.0, 186.0), (300.0, 200.0), (278.0, 216.0)], 0);
        assert_eq!(on_hold(Hold::Shape, &both, 1.0, &[], o), None);
        assert_eq!(on_hold(Hold::Arrow, &both, 1.0, &[], o).map(|s| s.shape.kind), Some(ShapeKind::Arrow));

        // The second of two barbs: waits at the short hold, then the head, and the first barb goes.
        let first = path(&[(e.x - 1.0, e.y + 1.0), (e.x - 18.0, e.y - 12.0)], 0);
        let with_first = [line_stroke(&line), line_stroke(&first)];
        let second = path(&[(e.x + 1.0, e.y), (e.x - 17.0, e.y + 13.0)], 0);
        assert_eq!(on_hold(Hold::Shape, &second, 1.0, &with_first, o), None);
        let pair = on_hold(Hold::Arrow, &second, 1.0, &with_first, o).expect("a head from two barbs");
        assert_eq!((pair.shape.kind, pair.delete), (ShapeKind::Arrow, Some(1)));
    }

    #[test]
    fn lifting_after_a_rest_still_makes_shapes_and_heads() {
        let o = Options::default();
        let rect = path(&RECT, 10);
        assert_eq!(on_lift(&rect, 1.0, 300, &[], o), None, "no rest: as drawn");
        assert_eq!(on_lift(&rect, 1.0, 700, &[], o).map(|s| s.shape.kind), Some(ShapeKind::Rectangle));
        let line = path(&[(100.0, 200.0), (300.0, 202.0)], 0);
        let lines = [line_stroke(&line)];
        let vee = path(&[(282.0, 186.0), (302.0, 199.0), (279.0, 217.0)], 0);
        assert_eq!(on_lift(&vee, 1.0, 700, &lines, o), None, "a V needs the long rest");
        assert_eq!(on_lift(&vee, 1.0, 950, &lines, o).map(|s| s.shape.kind), Some(ShapeKind::Arrow));
        let both = path(&[(100.0, 200.0), (300.0, 200.0), (276.0, 186.0), (300.0, 200.0), (278.0, 216.0)], 0);
        assert_eq!(on_lift(&both, 1.0, 700, &[], o), None);
        assert_eq!(on_lift(&both, 1.0, 950, &[], o).map(|s| s.shape.kind), Some(ShapeKind::Arrow));
        assert_eq!(on_lift(&both, 1.0, 950, &[], Options { arrows: false, ..o }), None);
    }

    #[test]
    fn a_held_line_snaps_to_angles_and_a_wiggle_frees_it() {
        let o = Options::default();
        let raw = path(&[(100.0, 100.0), (300.0, 100.0)], 10);
        let snap = on_hold(Hold::Shape, &raw, 1.0, &[], o).expect("a line");
        assert_eq!(snap.shape.kind, ShapeKind::Line);
        let Some(Held::Line(mut held)) = Held::new(&snap.shape, 300.0, 100.0) else { panic!("a held line") };
        let at = |deg: f32, len: f32| (100.0 + len * deg.to_radians().cos(), 100.0 + len * deg.to_radians().sin());
        // 44°: on the 45° step, with a tick; again there: no tick.
        let (x, y) = at(44.0, 200.0);
        let up = held.pen_at(x, y, 1000, 1.0, 1.0);
        let pts = up.points.expect("the line follows");
        let (ex, ey) = (pts[pts.len() - 1].x - 100.0, pts[pts.len() - 1].y - 100.0);
        assert!((ex - ey).abs() < 1e-3 && up.tick, "{ex}, {ey}");
        assert_eq!((pts[0].x, pts[0].y, pts[0].pressure), (100.0, 100.0, 0.5));
        assert_eq!(pts[pts.len() - 1].t_ms, 1000);
        let (x, y) = at(44.5, 210.0);
        assert!(!held.pen_at(x, y, 1010, 1.0, 1.0).tick);
        // 37°: no step near (45° is 8° off, 30° 7°): where the pen is.
        let (x, y) = at(37.0, 200.0);
        let up = held.pen_at(x, y, 1020, 1.0, 1.0);
        let q = *up.points.as_ref().and_then(|p| p.last()).unwrap();
        assert!(!up.tick &&(q.x - x).abs() < 1e-3 && (q.y - y).abs() < 1e-3);
        // Within 3 px of the start: it stays as it shows.
        assert_eq!(held.pen_at(101.0, 101.0, 1030, 1.0, 1.0).points, None);

        // A wiggle across it (three turns of 6 dp within 0.8 s): free, then snapping again.
        let Some(Held::Line(mut held)) = Held::new(&snap.shape, 300.0, 100.0) else { panic!() };
        let mut freed = Vec::new();
        for (i, dy) in [10.0, -10.0, 10.0, -10.0, 10.0].into_iter().enumerate() {
            freed.push(held.pen_at(300.0, 100.0 + dy, 2000 + i as u32 * 50, 1.0, 1.0).freed);
        }
        assert_eq!(freed, [None, None, None, None, Some(true)]);
        assert!(held.free());
        let (x, y) = at(44.0, 200.0);
        let pts = held.pen_at(x, y, 2900, 1.0, 1.0).points.unwrap();
        assert!((pts[pts.len() - 1].x - x).abs() < 1e-3, "free: no snapping");
        // Turns too slow (three over more than 0.8 s) are no wiggle.
        let Some(Held::Line(mut held)) = Held::new(&snap.shape, 300.0, 100.0) else { panic!() };
        for (i, dy) in [10.0, -10.0, 10.0, -10.0, 10.0].into_iter().enumerate() {
            assert_eq!(held.pen_at(300.0, 100.0 + dy, 3000 + i as u32 * 450, 1.0, 1.0).freed, None);
        }
    }

    #[test]
    fn a_held_shape_follows_the_pen_after_a_little_play() {
        let o = Options::default();
        let snap = on_hold(Hold::Shape, &path(&RECT, 10), 1.0, &[], o).expect("a rectangle");
        let Some(mut held) = Held::new(&snap.shape, 100.0, 100.0) else { panic!("held") };
        // The pen shifting a little (as it lifts) changes nothing.
        assert_eq!(held.pen_at(102.0, 101.0, 900, 1.0, 1.0), HeldUpdate::default());
        // Then the corner by the pen follows it, the opposite one stays.
        let pts = held.pen_at(80.0, 90.0, 950, 1.0, 1.0).points.unwrap();
        let (x0, y0) = pts.iter().fold((f32::MAX, f32::MAX), |a, q| (a.0.min(q.x), a.1.min(q.y)));
        let (x1, y1) = pts.iter().fold((-f32::MAX, -f32::MAX), |a, q| (a.0.max(q.x), a.1.max(q.y)));
        assert!((x0 - 80.0).abs() < 0.5 && (y0 - 90.0).abs() < 0.5 && (x1 - 300.0).abs() < 0.5 && (y1 - 220.0).abs() < 0.5, "{x0} {y0} {x1} {y1}");
        // Nearly square: a square, with a tick (once).
        let up = held.pen_at(176.0, 100.0, 1000, 1.0, 1.0);
        assert!(up.tick);
        assert!(!held.pen_at(177.0, 100.0, 1010, 1.0, 1.0).tick);
        assert_eq!(end_of(&snap.shape), (100.0, 100.0));
    }
}
