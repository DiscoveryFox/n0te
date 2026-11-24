//! Arrows, tidied but still hand-drawn. The head comes out symmetric about the line it sits on,
//! each barb keeping a third of how it differed from the other, the barbs' ends tapering off;
//! a straight shaft comes out straight but keeps a third of the hand's sway and the pen's
//! pressure; a curved shaft stays exactly as drawn. Ways to draw one:
//!  - a line (straight or curved) first, then a "V" — or a small closed triangle — at one of
//!    its ends: that becomes the head ([`head`], [`closed_head`]; found among the page's
//!    strokes by [`head_at_line_ends`]); or the head as two single barbs ([`barb_pair`]);
//!  - in one stroke, shaft then head (one barb, both, back and forth), held at the end like
//!    any shape ([`from_stroke`]).
//!
//! The Android app has the same rules in Kotlin: `n0te/android/.../Arrows.kt` (this is a
//! line-by-line port with the same thresholds and the same float/double arithmetic, see
//! [`crate::shapes`]); [`head_at_line_ends`] and [`barb_pair`] are `arrowHead` and `barbPair`
//! in NotebookView.kt, and [`ends`] is the FFI's `stroke_ends` (`FfiStroke.ends`). When the
//! view calls them is in [`crate::snap`].

use crate::ink::{InkPoint, Rect};
use crate::sheet::Stroke;
use crate::shapes::{self, atan2, avg, cos, fsum, hypot, min_by_first, ms, round_half_up, seg_dist, sin};

/// How far a head drawn on its own sits past the line's end (sheet points), covering its round end.
pub const POINT_PAST: f32 = 0.35;

/// A line's end: where it is, and the way back into the line from there (unit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineEnd {
    pub x: f32,
    pub y: f32,
    pub dx: f32,
    pub dy: f32,
}

/// Where a stroke starts and ends, each with the way back into it, and its length
/// (`FfiStroke.ends`: x0, y0, dx0, dy0, x1, y1, dx1, dy1, length).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ends {
    pub start: LineEnd,
    pub end: LineEnd,
    pub len: f32,
}

impl Ends {
    /// As `FfiStroke.ends` packs it.
    pub fn to_array(&self) -> [f32; 9] {
        let (a, b) = (self.start, self.end);
        [a.x, a.y, a.dx, a.dy, b.x, b.y, b.dx, b.dy, self.len]
    }
}

/// A drawn barb: from the tip, as a vector, and how hard the pen pressed on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Barb {
    pub dx: f32,
    pub dy: f32,
    pub p: f32,
}

/// A piece of the result: points (x, y, pressure) to follow, ends that taper off.
struct Leg {
    pts: Vec<[f32; 3]>,
    taper_start: bool,
    taper_end: bool,
}

impl Leg {
    fn line(x0: f32, y0: f32, x1: f32, y1: f32, p: f32, taper_start: bool, taper_end: bool) -> Leg {
        Leg { pts: vec![[x0, y0, p], [x1, y1, p]], taper_start, taper_end }
    }
}

/// `acos` of a cosine computed in float, clamped, in degrees (as the Kotlin code reads angles).
fn acos_deg(c: f32) -> f64 {
    (c.clamp(-1.0, 1.0) as f64).acos().to_degrees()
}

fn fmin(v: &[f32]) -> f32 {
    v.iter().fold(f32::MAX, |a, &b| a.min(b))
}

fn fmax(v: &[f32]) -> f32 {
    v.iter().fold(-f32::MAX, |a, &b| a.max(b))
}

/// The area a closed path encloses (shoelace, summed in double).
fn area(xs: &[f32], ys: &[f32]) -> f32 {
    let n = xs.len();
    let s = (0..n).fold(0.0f64, |s, i| {
        let j = (i + 1) % n;
        s + (xs[i] * ys[j] - xs[j] * ys[i]) as f64
    });
    (s as f32).abs() / 2.0
}

// --- a head drawn on its own ------------------------------------------------------------------

/// The stroke `s` (trembling trimmed, see [`shapes::trimmed`]) as the head of an arrow at a
/// line's end `end`, the line `shaft` long. Whatever order it was drawn in — a V with its point
/// in the middle, from the point out and back and out again, with hooks, or a small closed
/// triangle — it is a head if it has its point at that end and one straight barb on each side
/// of the line pointing back along it. A round bracket (curved arms) or a small circle (not a
/// triangle) at a line's end is no head: number lines keep their ends. Kotlin `Arrows.head`.
pub fn head(s: &[InkPoint], end: LineEnd, shaft: f32) -> Option<Vec<InkPoint>> {
    let n = s.len();
    if n < 4 {
        return None;
    }
    let xs: Vec<f32> = s.iter().map(|q| q.x).collect();
    let ys: Vec<f32> = s.iter().map(|q| q.y).collect();
    let total = path_len(&xs, &ys, 0, n - 1);
    if total < 8.0 {
        return None;
    }
    let size = (fmax(&xs) - fmin(&xs)).max(fmax(&ys) - fmin(&ys));
    if size > 12f32.max(shaft * 0.7) {
        return None;
    }
    let (t0, t1) = (s[0].t_ms as f32, s[n - 1].t_ms as f32);
    let LineEnd { x: tip_x, y: tip_y, dx: back_x, dy: back_y } = end;
    // Its point: its foremost part, the way the line points. Near the line's end and on
    // the line's way on (a "v" written beside the end is no head).
    let ti = min_by_first(0..n, |i| (xs[i] - tip_x) * back_x + (ys[i] - tip_y) * back_y)?;
    let d_end = hypot(xs[ti] - tip_x, ys[ti] - tip_y);
    let aside = (back_x * (ys[ti] - tip_y) - back_y * (xs[ti] - tip_x)).abs();
    if d_end > 8f32.max(size * 0.6) || aside > 4f32.max(size * 0.35) {
        return None;
    }
    // Its barbs: on each side of the line the farthest part pointing back along it (within
    // 85°: an arm may curl out at its end), seen from its own point (which may overshoot
    // the line's end a little). A part reaching forward is something else (a "<" after the line).
    let (px, py) = (xs[ti], ys[ti]);
    let (mut left, mut right, mut l_best, mut r_best) = (None, None, 0f32, 0f32);
    for i in 0..n {
        let (vx, vy) = (xs[i] - px, ys[i] - py);
        let l = hypot(vx, vy);
        if l < size * 0.3 {
            continue;
        }
        let ang = acos_deg((vx * back_x + vy * back_y) / l);
        if ang > 85.0 {
            if ang > 100.0 && l > size * 0.35 {
                return None;
            }
            continue;
        }
        let side = back_x * vy - back_y * vx;
        if side >= 0.0 && l > l_best {
            l_best = l;
            left = Some(i);
        } else if side < 0.0 && l > r_best {
            r_best = l;
            right = Some(i);
        }
    }
    let (left, right) = (left?, right?);
    let (lx, ly, rx, ry) = (xs[left] - px, ys[left] - py, xs[right] - px, ys[right] - py);
    let (ll, rl) = (hypot(lx, ly), hypot(rx, ry));
    if ll.min(rl) < 5.0 || ll.max(rl) > 3.0 * ll.min(rl) {
        return None;
    }
    let opening = acos_deg((lx * rx + ly * ry) / (ll * rl));
    if !(15.0..=125.0).contains(&opening) {
        return None;
    }
    // Straight barbs (a hook at the far end is fine): the way from the point out to each,
    // up to three quarters of it, stays near the straight line.
    let near_tip: Vec<usize> = (0..n).filter(|&i| hypot(xs[i] - xs[ti], ys[i] - ys[ti]) < 3f32.max(size * 0.2)).collect();
    let straight = |b: usize| -> bool {
        let Some(f) = min_by_first(near_tip.iter().copied(), |i| i.abs_diff(b)) else { return false };
        let (bx, by) = (xs[b], ys[b]);
        let bl = hypot(bx - xs[f], by - ys[f]).max(1e-3);
        for i in f.min(b)..=f.max(b) {
            let along = ((xs[i] - xs[f]) * (bx - xs[f]) + (ys[i] - ys[f]) * (by - ys[f])) / bl;
            if along > bl * 0.75 {
                continue;
            }
            if seg_dist(xs[i], ys[i], xs[f], ys[f], bx, by) > 1.5f32.max(bl * 0.12) {
                return false;
            }
        }
        true
    };
    let closed = hypot(xs[0] - xs[n - 1], ys[0] - ys[n - 1]) < total * 0.3;
    let tri = (lx * ry - ly * rx).abs() / 2.0;
    let area = if closed { area(&xs, &ys) } else { 0.0 };
    let triangle = closed && area > tri * 0.3;
    if triangle && area > tri * 1.6 {
        return None; // round, not a triangle
    }
    if !triangle && !(straight(left) && straight(right)) {
        return None; // curved barbs
    }
    // It must open back along the line (roughly); the clean head then lies exactly about the
    // line's way at its end, whichever way the drawn one leaned.
    let (mx, my) = (lx / ll + rx / rl, ly / ll + ry / rl);
    let ml = hypot(mx, my);
    if ml < 1e-3 {
        return None;
    }
    if acos_deg((mx * back_x + my * back_y) / ml) > 50.0 {
        return None;
    }
    let p = avg(s.iter().map(|q| q.pressure)) as f32;
    let barbs = [Barb { dx: lx, dy: ly, p }, Barb { dx: rx, dy: ry, p }];
    // The point a third of a point past the line's end: it covers the line's round end.
    let legs = head_legs(tip_x - back_x * POINT_PAST, tip_y - back_y * POINT_PAST, back_x, back_y, &barbs, shaft, true);
    if !triangle {
        return Some(sample(&legs, t0, t1));
    }
    // Closed: from the last barb's end back to the first.
    Some(sample(&closed_legs(legs, p), t0, t1))
}

/// The legs of a joined head, not tapering, and closed: from the last barb's end back to the first.
fn closed_legs(legs: Vec<Leg>, p: f32) -> Vec<Leg> {
    let b1 = legs[0].pts[0];
    let b2 = *legs[legs.len() - 1].pts.last().expect("a leg has points");
    let mut out: Vec<Leg> = legs.into_iter().map(|g| Leg { pts: g.pts, taper_start: false, taper_end: false }).collect();
    out.push(Leg::line(b2[0], b2[1], b1[0], b1[1], p, false, false));
    out
}

/// A small closed triangle `s` drawn at a line's end, however it was turned: the head of an
/// arrow pointing exactly the way the line runs there, as big as the drawn one (a third of the
/// line, 30 pt at most). Closed (its end back near its start) and three-cornered, not round:
/// 4π·area/perimeter² between a thin triangle's and a square's (a circle at a line's end stays
/// a circle). Its middle lies within its size of the end, not back along the line. None if it
/// is no such triangle. Kotlin `Arrows.closedHead`.
pub fn closed_head(s: &[InkPoint], end: LineEnd, shaft: f32) -> Option<Vec<InkPoint>> {
    let n = s.len();
    if n < 5 {
        return None;
    }
    let xs: Vec<f32> = s.iter().map(|q| q.x).collect();
    let ys: Vec<f32> = s.iter().map(|q| q.y).collect();
    let total = path_len(&xs, &ys, 0, n - 1);
    if total < 10.0 {
        return None;
    }
    let size = (fmax(&xs) - fmin(&xs)).max(fmax(&ys) - fmin(&ys));
    if size > 12f32.max(shaft * 0.7) {
        return None;
    }
    if hypot(xs[0] - xs[n - 1], ys[0] - ys[n - 1]) > total * 0.3 {
        return None; // not closed
    }
    let a = area(&xs, &ys);
    let compact = (4.0 * std::f64::consts::PI * a as f64 / (total * total) as f64) as f32;
    if !(0.18..=0.74).contains(&compact) {
        return None; // not a triangle
    }
    let LineEnd { x: tip_x, y: tip_y, dx: back_x, dy: back_y } = end;
    let (cx, cy) = (avg(xs.iter().copied()) as f32, avg(ys.iter().copied()) as f32);
    let along = (cx - tip_x) * back_x + (cy - tip_y) * back_y;
    if hypot(cx - tip_x, cy - tip_y) > 8f32.max(size * 0.9) || along > size * 0.8 {
        return None; // not at the end
    }
    let p = avg(s.iter().map(|q| q.pressure)) as f32;
    let l = size * 0.95;
    let a = 24f64.to_radians() as f32;
    let barbs: Vec<Barb> = [a, -a].iter().map(|&t| Barb { dx: (back_x * cos(t) - back_y * sin(t)) * l, dy: (back_x * sin(t) + back_y * cos(t)) * l, p }).collect();
    let legs = head_legs(tip_x - back_x * POINT_PAST, tip_y - back_y * POINT_PAST, back_x, back_y, &barbs, shaft, true);
    Some(sample(&closed_legs(legs, p), s[0].t_ms as f32, s[n - 1].t_ms as f32))
}

/// A single barb drawn on its own at a line's end (the head drawn as two strokes): a short,
/// straight stroke with one end at the tip, pointing back along the line (8–70° off it). Its
/// vector from the tip and pressure, or None. Kotlin `Arrows.barb`.
pub fn barb(s: &[InkPoint], end: LineEnd, shaft: f32) -> Option<Barb> {
    let n = s.len();
    if n < 3 {
        return None;
    }
    let (x0, y0, x1, y1) = (s[0].x, s[0].y, s[n - 1].x, s[n - 1].y);
    let mut len = 0f32;
    for i in 0..n - 1 {
        len += hypot(s[i + 1].x - s[i].x, s[i + 1].y - s[i].y);
    }
    let p = avg(s.iter().map(|q| q.pressure)) as f32;
    barb_from(x0, y0, x1, y1, len, end, shaft).map(|(dx, dy)| Barb { dx, dy, p })
}

/// A straight stroke from (x0, y0) to (x1, y1), `len` long, as a barb at the line's end: its
/// vector from the tip, or None. Kotlin `Arrows.barbFrom`.
pub fn barb_from(x0: f32, y0: f32, x1: f32, y1: f32, len: f32, end: LineEnd, shaft: f32) -> Option<(f32, f32)> {
    let LineEnd { x: tip_x, y: tip_y, dx: back_x, dy: back_y } = end;
    let chord = hypot(x1 - x0, y1 - y0);
    if chord < 5.0 || chord < len * 0.85 || chord > shaft * 0.6 {
        return None;
    }
    let (d0, d1) = (hypot(x0 - tip_x, y0 - tip_y), hypot(x1 - tip_x, y1 - tip_y));
    let (fx, fy) = if d0 <= d1 { (x1, y1) } else { (x0, y0) };
    if d0.min(d1) > 8f32.max(chord * 0.5) {
        return None;
    }
    let (vx, vy) = (fx - tip_x, fy - tip_y);
    let vl = hypot(vx, vy);
    if vl < 3.0 {
        return None;
    }
    if !(8.0..=70.0).contains(&acos_deg((vx * back_x + vy * back_y) / vl)) {
        return None;
    }
    Some((vx, vy))
}

/// A head from two barbs drawn one by one (vectors from the tip, on either side of the line):
/// one V, tidied like any head; `t0`..`t1` its samples' time. None if both lie on the same side.
/// Kotlin `Arrows.headFromBarbs`.
pub fn head_from_barbs(b1: Barb, b2: Barb, end: LineEnd, shaft: f32, t0: f32, t1: f32) -> Option<Vec<InkPoint>> {
    let LineEnd { x: tip_x, y: tip_y, dx: back_x, dy: back_y } = end;
    let s1 = back_x * b1.dy - back_y * b1.dx;
    let s2 = back_x * b2.dy - back_y * b2.dx;
    if s1 * s2 >= 0.0 {
        return None;
    }
    Some(sample(&head_legs(tip_x - back_x * POINT_PAST, tip_y - back_y * POINT_PAST, back_x, back_y, &[b1, b2], shaft, true), t0, t1))
}

// --- an arrow in one stroke -------------------------------------------------------------------

/// One stroke `s` (trembling trimmed) that is an arrow (a head at one end or both), or None.
/// `strict` (no hold, as the pen lifts): only with both barbs drawn and a shaft of 30 pt or
/// more. Kotlin `Arrows.fromStroke`.
pub fn from_stroke(s: &[InkPoint], strict: bool) -> Option<Vec<InkPoint>> {
    let n = s.len();
    if n < 6 {
        return None;
    }
    let xs: Vec<f32> = s.iter().map(|q| q.x).collect();
    let ys: Vec<f32> = s.iter().map(|q| q.y).collect();
    let p_at = |i: usize| s[i].pressure;
    let mut cum = vec![0f32; n];
    for i in 1..n {
        cum[i] = cum[i - 1] + hypot(xs[i] - xs[i - 1], ys[i] - ys[i - 1]);
    }
    let total = cum[n - 1];
    if total < 30.0 {
        return None;
    }
    // How sharply the path turns at sample i, over 6 pt (at least 2 samples) either side (degrees).
    let turn_at = |i: usize| -> f64 {
        let mut j = i;
        while j > 0 && (cum[i] - cum[j] < 6.0 || i - j < 2) {
            j -= 1;
        }
        let mut k = i;
        while k < n - 1 && (cum[k] - cum[i] < 6.0 || k - i < 2) {
            k += 1;
        }
        if j == i || k == i {
            return 0.0;
        }
        let (ax, ay, bx, by) = (xs[i] - xs[j], ys[i] - ys[j], xs[k] - xs[i], ys[k] - ys[i]);
        ((ax * by - ay * bx) as f64).atan2((ax * bx + ay * by) as f64).to_degrees().abs()
    };
    // The sharpest turn in the run of samples around i that turn more than 110°.
    let peak = |i: usize| -> usize {
        let (mut best, mut k) = (i, i);
        while k < n && turn_at(k) > 110.0 {
            if turn_at(k) > turn_at(best) {
                best = k;
            }
            k += 1;
        }
        best
    };
    // The tips: where the path doubles back into a head that takes at most 45 % of it.
    let end_tip = (0..n).find(|&i| cum[i] >= total * 0.55 && total - cum[i] >= 3.0 && turn_at(i) > 110.0).map(peak);
    let start_tip = (0..n).rev().find(|&i| cum[i] <= total * 0.45 && cum[i] >= 3.0 && turn_at(i) > 110.0 && end_tip.is_none_or(|e| i < e)).map(|i| {
        let mut j = i;
        while j > 0 && turn_at(j - 1) > 110.0 {
            j -= 1;
        }
        peak(j)
    });
    let a = start_tip.unwrap_or(0);
    let b = end_tip.unwrap_or(n - 1);
    let shaft_len = cum[b] - cum[a];
    if shaft_len < (if strict { 30.0 } else { 24.0 }) || shaft_len < total * 0.38 {
        return None; // shaft short
    }
    // The shaft: straight or a smooth curve (no sharp turns: not writing).
    let inner: Vec<usize> = simplify(&xs[a..=b], &ys[a..=b], 2.5f32.max(shaft_len * 0.04)).into_iter().map(|i| i + a).collect();
    let mut turning = 0.0f64;
    for k in 1..inner.len().saturating_sub(1) {
        let (i0, i1, i2) = (inner[k - 1], inner[k], inner[k + 1]);
        let (ax, ay, bx, by) = (xs[i1] - xs[i0], ys[i1] - ys[i0], xs[i2] - xs[i1], ys[i2] - ys[i1]);
        let t = ((ax * by - ay * bx) as f64).atan2((ax * bx + ay * by) as f64).to_degrees().abs();
        if t > 50.0 {
            return None; // a sharp turn
        }
        turning += t;
    }
    if turning > 270.0 {
        return None;
    }
    let chord = hypot(xs[b] - xs[a], ys[b] - ys[a]);
    let straight = (a..=b).all(|i| seg_dist(xs[i], ys[i], xs[a], ys[a], xs[b], ys[b]) <= 3f32.max(chord * 0.05));
    let fitted = straight && chord >= 10.0;
    // The shaft's ends (straight: on its fitted line) and its points.
    let (tip_a, tip_b, shaft_pts) = if fitted {
        let (ta, tb) = fit_line(&xs, &ys, a, b);
        (ta, tb, shaft_leg(&xs, &ys, a, b, ta, tb, &p_at))
    } else {
        // Curved: as drawn (the pressure smoothed a little).
        let pts = (a..=b).map(|i| [xs[i], ys[i], avg((i.saturating_sub(2).max(a)..=(i + 2).min(b)).map(p_at)) as f32]).collect();
        ((xs[a], ys[a]), (xs[b], ys[b]), pts)
    };
    // The way into the shaft from its end i (unit), read over its last stretch.
    let into = |i: usize, tip: (f32, f32), inward: isize| -> (f32, f32) {
        let reach = 14f32.min(shaft_len * 0.2);
        let mut j = i as isize;
        while j >= a as isize && j <= b as isize && (cum[j as usize] - cum[i]).abs() < reach {
            j += inward;
        }
        let j = j.clamp(a as isize, b as isize) as usize;
        let (dx, dy) = (xs[j] - tip.0, ys[j] - tip.1);
        let l = hypot(dx, dy).max(1e-3);
        (dx / l, dy / l)
    };
    // A head: the corners beyond the tip, each a barb if it points back along the shaft;
    // the farthest on each side counts. None if something else was drawn there.
    let head_at = |tip: (f32, f32), back: (f32, f32), idx: std::ops::Range<usize>, tip_idx: usize| -> Option<Vec<Barb>> {
        let (mut left, mut right): (Option<Barb>, Option<Barb>) = (None, None);
        let (mut left_l, mut right_l, mut stray) = (0f32, 0f32, 0f32);
        for i in idx {
            let (dx, dy) = (xs[i] - tip.0, ys[i] - tip.1);
            let l = hypot(dx, dy);
            if l < 3f32.max(shaft_len * 0.04) {
                continue;
            }
            let ang = acos_deg((dx * back.0 + dy * back.1) / l);
            if ang > 70.0 || l > shaft_len * 0.6 {
                stray = stray.max(l);
                continue;
            }
            let side = back.0 * dy - back.1 * dx;
            let p = avg((tip_idx.min(i)..=tip_idx.max(i)).map(p_at)) as f32;
            if side >= 0.0 && l > left_l {
                left = Some(Barb { dx, dy, p });
                left_l = l;
            } else if side < 0.0 && l > right_l {
                right = Some(Barb { dx, dy, p });
                right_l = l;
            }
        }
        if stray > shaft_len * 0.12 {
            return None;
        }
        Some(left.into_iter().chain(right).collect())
    };
    // The way back into the shaft at each tip: along the fitted line when straight, else
    // along the curve's last stretch.
    let (back_a, back_b) = if fitted {
        let l = hypot(tip_b.0 - tip_a.0, tip_b.1 - tip_a.1).max(1e-3);
        let (ux, uy) = ((tip_b.0 - tip_a.0) / l, (tip_b.1 - tip_a.1) / l);
        ((ux, uy), (-ux, -uy))
    } else {
        (into(a, tip_a, 1), into(b, tip_b, -1))
    };
    let end_head = if end_tip.is_none() { Vec::new() } else { head_at(tip_b, back_b, b + 1..n, b)? };
    let start_head = if start_tip.is_none() { Vec::new() } else { head_at(tip_a, back_a, 0..a, a)? };
    if end_head.is_empty() && start_head.is_empty() {
        return None;
    }
    if strict && (end_head.len() == 1 || start_head.len() == 1) {
        return None;
    }
    let mut legs = Vec::new();
    if !start_head.is_empty() {
        legs.extend(head_legs(tip_a.0, tip_a.1, back_a.0, back_a.1, &start_head, shaft_len, false));
    }
    legs.push(Leg { pts: shaft_pts, taper_start: start_head.is_empty(), taper_end: end_head.is_empty() });
    if !end_head.is_empty() {
        legs.extend(head_legs(tip_b.0, tip_b.1, back_b.0, back_b.1, &end_head, shaft_len, false));
    }
    Some(sample(&legs, s[0].t_ms as f32, s[n - 1].t_ms as f32))
}

/// A line through the points a..=b: its ends where the drawn ends lie on it, exactly level,
/// upright or diagonal within 3°.
fn fit_line(xs: &[f32], ys: &[f32], a: usize, b: usize) -> ((f32, f32), (f32, f32)) {
    let (mx, my) = (avg(xs[a..=b].iter().copied()) as f32, avg(ys[a..=b].iter().copied()) as f32);
    let (mut sxx, mut syy, mut sxy) = (0f32, 0f32, 0f32);
    for i in a..=b {
        let (dx, dy) = (xs[i] - mx, ys[i] - my);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    let mut ang = 0.5 * (2.0 * sxy as f64).atan2((sxx - syy) as f64);
    let quarter = std::f64::consts::PI / 4.0;
    let near = round_half_up(ang / quarter) * quarter;
    if (ang - near).abs() < 3f64.to_radians() {
        ang = near;
    }
    let (ux, uy) = (ang.cos() as f32, ang.sin() as f32);
    let on = |i: usize| {
        let t = (xs[i] - mx) * ux + (ys[i] - my) * uy;
        (mx + ux * t, my + uy * t)
    };
    (on(a), on(b))
}

/// The shaft from `from` to `to` (on its fitted line), keeping a third of how the hand swayed
/// off that line (smoothed: the sway, not the tremble; none at the ends, so the head sits
/// right) and the pen's pressure along it (smoothed too).
fn shaft_leg(xs: &[f32], ys: &[f32], a: usize, b: usize, from: (f32, f32), to: (f32, f32), p: &dyn Fn(usize) -> f32) -> Vec<[f32; 3]> {
    let ((ax, ay), (bx, by)) = (from, to);
    let l = hypot(bx - ax, by - ay);
    if l < 1e-3 || b - a < 3 {
        return vec![[ax, ay, p(a)], [bx, by, p(b)]];
    }
    let (ux, uy) = ((bx - ax) / l, (by - ay) / l);
    let t: Vec<f32> = (a..=b).map(|i| ((xs[i] - ax) * ux + (ys[i] - ay) * uy).clamp(0.0, l)).collect();
    let o: Vec<f32> = (a..=b).map(|i| -(xs[i] - ax) * uy + (ys[i] - ay) * ux).collect();
    let pr: Vec<f32> = (a..=b).map(p).collect();
    let m = t.len();
    let win = 2.max(m / 10);
    let smooth = |v: &[f32], i: usize| avg(v[i.saturating_sub(win)..=(i + win).min(v.len() - 1)].iter().copied()) as f32;
    let mut out: Vec<[f32; 3]> = Vec::with_capacity(m);
    for i in 0..m {
        let ti = if i == 0 {
            0.0
        } else if i == m - 1 {
            l
        } else {
            t[i]
        };
        let fade = 1f32.min((ti / (0.12 * l)).min((l - ti) / (0.12 * l))).max(0.0);
        let off = 0.33 * smooth(&o, i) * fade;
        out.push([ax + ux * ti - uy * off, ay + uy * ti + ux * off, smooth(&pr, i)]);
    }
    // Along the line in order (a wobble backwards would fold the stroke).
    let key = |q: &[f32; 3]| (q[0] - ax) * ux + (q[1] - ay) * uy;
    out.sort_by(|q, r| key(q).total_cmp(&key(r)));
    let (p0, p1) = (out[0][2], out[m - 1][2]);
    let mut res = Vec::with_capacity(m);
    res.push([ax, ay, p0]);
    res.extend_from_slice(&out[1..m - 1]);
    res.push([bx, by, p1]);
    res
}

// --- shared -----------------------------------------------------------------------------------

/// The barbs of a head at the tip (tx, ty), the shaft running from it along (bx, by):
/// symmetric about the shaft, each keeping a third of how it differed from the other; one
/// barb drawn gets its mirror image. Angles 16–50° off the shaft, lengths 4 pt up to a third
/// of the shaft (30 pt at most). `joined`: the head alone, as one V (barb, tip, barb); else
/// drawn out from the tip and back for each barb (to follow on from the shaft).
fn head_legs(tx: f32, ty: f32, bx: f32, by: f32, drawn: &[Barb], shaft: f32, joined: bool) -> Vec<Leg> {
    let bl = hypot(bx, by);
    let (ux, uy) = (bx / bl, by / bl);
    let angs: Vec<f32> = drawn.iter().map(|d| atan2(ux * d.dy - uy * d.dx, d.dx * ux + d.dy * uy)).collect();
    let lens: Vec<f32> = drawn.iter().map(|d| hypot(d.dx, d.dy)).collect();
    let mean_a = avg(angs.iter().map(|a| a.abs())) as f32;
    let mean_l = avg(lens.iter().copied()) as f32;
    let angle = |i: usize| mean_a + 0.33 * (angs[i].abs() - mean_a);
    let length = |i: usize| mean_l + 0.33 * (lens[i] - mean_l);
    let (min_a, max_a) = (16f64.to_radians() as f32, 50f64.to_radians() as f32);
    let sign = |a: f32| if a >= 0.0 { 1.0f32 } else { -1.0 };
    let barbs: Vec<(f32, f32, f32)> = if drawn.len() == 1 {
        let sg = sign(angs[0]);
        [sg, -sg].iter().map(|&s| (s * angle(0).clamp(min_a, max_a), length(0), drawn[0].p)).collect()
    } else {
        (0..drawn.len()).map(|i| (sign(angs[i]) * angle(i).clamp(min_a, max_a), length(i), drawn[i].p)).collect()
    };
    let ends: Vec<(f32, f32, f32)> = barbs
        .iter()
        .map(|&(a, l0, p)| {
            // Not bigger than a third of the line (a big drawn head is made smaller), 30 pt at most.
            let l = l0.clamp(4.0, 4f32.max((shaft * 0.3).min(30.0)));
            let (ca, sa) = (cos(a), sin(a));
            (tx + (ux * ca - uy * sa) * l, ty + (ux * sa + uy * ca) * l, p)
        })
        .collect();
    if joined {
        let ((x1, y1, p1), (x2, y2, p2)) = (ends[0], ends[1]);
        vec![Leg::line(x1, y1, tx, ty, p1, true, false), Leg::line(tx, ty, x2, y2, p2, false, true)]
    } else {
        ends.iter().flat_map(|&(x, y, p)| [Leg::line(tx, ty, x, y, p, false, true), Leg::line(x, y, tx, ty, p, true, false)]).collect()
    }
}

/// The legs one after the other as samples every 2 pt, the pressure easing off at tapered ends
/// (the last 4 pt or a third of a free end: down to 55 %). Close together at each leg's ends
/// (0.25, 0.5, 1 pt …): the renderer smooths a stroke (each point pulled halfway toward the
/// next), which would round the head's point inward by about the spacing and let the line poke
/// out through it. Time: evenly from `t0` to `t1`.
fn sample(legs: &[Leg], t0: f32, t1: f32) -> Vec<InkPoint> {
    const NEAR: [f32; 4] = [0.25, 0.5, 1.0, 2.0];
    let mut out: Vec<[f32; 3]> = Vec::new();
    for (li, g) in legs.iter().enumerate() {
        let seg: Vec<f32> = g.pts.windows(2).map(|w| hypot(w[1][0] - w[0][0], w[1][1] - w[0][1])).collect();
        let l = fsum(&seg);
        let ease = 4f32.min(l / 3.0).max(1e-3);
        let mut stops = vec![0.0f32];
        stops.extend(NEAR.iter().copied().filter(|&d| d < l / 2.0));
        stops.extend((1..=1.max((l / 2.0) as i32 - 1)).map(|k| k as f32 * 2.0 + 2.0).filter(|&d| d < l - 2.0));
        stops.extend(NEAR.iter().copied().filter(|&d| d < l / 2.0).rev().map(|d| l - d));
        stops.push(l);
        stops.sort_by(f32::total_cmp);
        stops.dedup_by(|a, b| a.to_bits() == b.to_bits());
        let (mut j, mut acc) = (0usize, 0f32);
        for (k, &d) in stops.iter().enumerate() {
            if k == 0 && li > 0 {
                continue;
            }
            while j + 1 < seg.len() && acc + seg[j] < d {
                acc += seg[j];
                j += 1;
            }
            let f = if seg.is_empty() || seg[j] <= 0.0 { 0.0 } else { ((d - acc) / seg[j]).clamp(0.0, 1.0) };
            let (q0, q1) = (g.pts[j], g.pts[(j + 1).min(g.pts.len() - 1)]);
            let mut p = q0[2] + (q1[2] - q0[2]) * f;
            if g.taper_start && d < ease {
                p *= 0.55 + 0.45 * d / ease;
            }
            if g.taper_end && l - d < ease {
                p *= 0.55 + 0.45 * (l - d) / ease;
            }
            out.push([q0[0] + (q1[0] - q0[0]) * f, q0[1] + (q1[1] - q0[1]) * f, p]);
        }
        // A corner (the head's point, a barb's end): the same point a few times more, so the
        // smoothing settles right on it.
        if li < legs.len() - 1 {
            let e = g.pts[g.pts.len() - 1];
            let p = out.last().map_or(0.0, |q| q[2]);
            for _ in 0..3 {
                out.push([e[0], e[1], p]);
            }
        }
    }
    let span = (out.len().max(2) - 1) as f32;
    out.iter().enumerate().map(|(i, q)| InkPoint { x: q[0], y: q[1], pressure: q[2], t_ms: ms(t0 + (t1 - t0) * i as f32 / span) }).collect()
}

fn path_len(xs: &[f32], ys: &[f32], a: usize, b: usize) -> f32 {
    let mut l = 0f32;
    for i in a..b {
        l += hypot(xs[i + 1] - xs[i], ys[i + 1] - ys[i]);
    }
    l
}

/// Douglas–Peucker: the indices of the corners that keep every point within `tol` of the polyline.
fn simplify(xs: &[f32], ys: &[f32], tol: f32) -> Vec<usize> {
    let n = xs.len();
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0usize, n - 1)];
    while let Some((i, j)) = stack.pop() {
        let (mut best, mut far) = (None, tol);
        for q in i + 1..j {
            let d = seg_dist(xs[q], ys[q], xs[i], ys[i], xs[j], ys[j]);
            if d > far {
                far = d;
                best = Some(q);
            }
        }
        if let Some(b) = best {
            keep[b] = true;
            stack.push((i, b));
            stack.push((b, j));
        }
    }
    (0..n).filter(|&i| keep[i]).collect()
}

// --- line ends --------------------------------------------------------------------------------

/// A stroke's ends and the way it runs there, for arrowheads: where the rest of the stroke
/// points, not how its last millimetres wobbled as the pen landed or rested. A straight stroke
/// (every point near the chord) runs the chord's way; a curved one the way of a line fitted
/// through its last fifth (8–30 pt in from the end: a curving end still points the way it
/// ends). A little flick as the pen lifted (its last few points turning off sharply) is left
/// out: the end is where it began, the way is the stroke's own. None for a dot.
/// The FFI's `stroke_ends` (`FfiStroke.ends`), which the Android app reads.
pub fn ends(pts: &[InkPoint]) -> Option<Ends> {
    let n = pts.len();
    if n < 2 {
        return None;
    }
    let seg: Vec<f32> = pts.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).collect();
    let len: f32 = seg.iter().sum();
    if len < 1.0 {
        return None;
    }
    // The point `d` along the stroke from one end (interpolated).
    let along = |from_end: bool, d: f32| -> (f32, f32) {
        let mut acc = 0.0;
        for k in 0..seg.len() {
            let i = if from_end { seg.len() - 1 - k } else { k };
            if acc + seg[i] >= d && seg[i] > 0.0 {
                let f = (d - acc) / seg[i];
                let (p, q) = if from_end { (pts[i + 1], pts[i]) } else { (pts[i], pts[i + 1]) };
                return (p.x + (q.x - p.x) * f, p.y + (q.y - p.y) * f);
            }
            acc += seg[i];
        }
        let p = if from_end { pts[0] } else { pts[n - 1] };
        (p.x, p.y)
    };
    let unit = |x: f32, y: f32| {
        let l = x.hypot(y).max(1e-6);
        (x / l, y / l)
    };
    // Straight: every point within 4 % of the length (1.5 pt at least) of the chord.
    let (a, b) = (pts[0], pts[n - 1]);
    let chord = (b.x - a.x).hypot(b.y - a.y);
    let straight = chord > len * 0.9 && {
        let (ux, uy) = unit(b.x - a.x, b.y - a.y);
        let tol = (len * 0.04).max(1.5);
        pts.iter().all(|p| ((p.x - a.x) * uy - (p.y - a.y) * ux).abs() <= tol)
    };
    // The way a curved end runs: the main axis of the points from 1.5 pt to a fifth in.
    let fitted = |from_end: bool| -> (f32, f32) {
        let (near, far) = (1.5f32.min(len / 8.0), (len * 0.2).clamp(8.0, 30.0).min(len / 2.0));
        let mut q: Vec<(f32, f32)> = vec![along(from_end, near), along(from_end, far)];
        let mut acc = 0.0;
        for k in 0..seg.len() {
            let i = if from_end { seg.len() - 1 - k } else { k };
            acc += seg[i];
            if acc > far {
                break;
            }
            if acc > near {
                let p = if from_end { pts[i] } else { pts[i + 1] };
                q.push((p.x, p.y));
            }
        }
        let (mx, my) = (q.iter().map(|p| p.0).sum::<f32>() / q.len() as f32, q.iter().map(|p| p.1).sum::<f32>() / q.len() as f32);
        let (mut cxx, mut cxy, mut cyy) = (0.0f32, 0.0f32, 0.0f32);
        for (x, y) in &q {
            cxx += (x - mx) * (x - mx);
            cxy += (x - mx) * (y - my);
            cyy += (y - my) * (y - my);
        }
        let t = 0.5 * (2.0 * cxy).atan2(cxx - cyy);
        let (dx, dy) = (t.cos(), t.sin());
        // Pointing back into the stroke.
        let (fx, fy) = (q[1].0 - q[0].0, q[1].1 - q[0].1);
        if dx * fx + dy * fy < 0.0 { (-dx, -dy) } else { (dx, dy) }
    };
    let end = |from_end: bool| -> LineEnd {
        let tip = if from_end { pts[n - 1] } else { pts[0] };
        // A flick: the last 4 pt run off at a sharp angle to the 4–12 pt before them.
        let (h0, h1) = (4f32.min(len / 6.0), 12f32.min(len / 2.0));
        let (px, py) = along(from_end, h0);
        let (qx, qy) = along(from_end, h1);
        let (fx, fy) = unit(px - tip.x, py - tip.y);
        let (gx, gy) = unit(qx - px, qy - py);
        let flick = len > 20.0 && fx * gx + fy * gy < 0.5;
        let (dx, dy) = if straight && !flick {
            if from_end { unit(a.x - b.x, a.y - b.y) } else { unit(b.x - a.x, b.y - a.y) }
        } else {
            fitted(from_end)
        };
        if flick { LineEnd { x: px, y: py, dx, dy } } else { LineEnd { x: tip.x, y: tip.y, dx, dy } }
    };
    Some(Ends { start: end(false), end: end(true), len })
}

/// A stroke on the page a head may be attached to: its bounds (as stored, see
/// [`Stroke::bounds`]) and [`ends`] (None for a dot). Leave out hidden strokes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LineStroke {
    pub bounds: Rect,
    pub ends: Option<Ends>,
}

impl LineStroke {
    pub fn of(stroke: &Stroke) -> LineStroke {
        LineStroke { bounds: stroke.bounds, ends: ends(&stroke.points) }
    }
}

/// `android.graphics.RectF.intersects(a, b)` (open intervals).
fn rect_intersects(a: &Rect, b: &Rect) -> bool {
    a.x0 < b.x1 && b.x0 < a.x1 && a.y0 < b.y1 && b.y0 < a.y1
}

/// `android.graphics.RectF.contains(x, y)` (right and bottom edges outside).
fn rect_contains(r: &Rect, x: f32, y: f32) -> bool {
    r.x0 < r.x1 && r.y0 < r.y1 && x >= r.x0 && x < r.x1 && y >= r.y0 && y < r.y1
}

/// The box of `s`, raw (x0, y0, x1, y1).
fn extent(s: &[InkPoint]) -> (f32, f32, f32, f32) {
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, -f32::MAX, -f32::MAX);
    for q in s {
        x0 = x0.min(q.x);
        x1 = x1.max(q.x);
        y0 = y0.min(q.y);
        y1 = y1.max(q.y);
    }
    (x0, y0, x1, y1)
}

/// The stroke being drawn (`points`, all of it so far) as the head of an arrow: a "V" (or a
/// small triangle) with its point at the end of one of `lines` (straight or curved, 30 pt or
/// longer), opening back along it. A closed triangle at a line's end is a head however it was
/// turned (it then points the way the line does). `triangles`: only such triangles (they need
/// no long hold). The head is no bigger than 120 pt and the end within its reach (half its
/// size, 8 pt at least); the nearest end is tried first. `still`: see [`shapes::detect`].
/// NotebookView.kt `arrowHead`.
pub fn head_at_line_ends(points: &[InkPoint], still: f32, lines: &[LineStroke], triangles: bool) -> Option<Vec<InkPoint>> {
    let r = shapes::trimmed(points, still)?;
    let s = &points[r];
    let (x0, y0, x1, y1) = extent(s);
    let big = (x1 - x0).max(y1 - y0);
    if big > 120.0 {
        return None; // a head, not a drawing
    }
    let reach = 8f32.max(0.5 * big);
    let area = Rect { x0: x0 - reach, y0: y0 - reach, x1: x1 + reach, y1: y1 + reach };
    // The nearest line end first (several may be close).
    let mut cands: Vec<(f32, LineEnd, f32)> = Vec::new();
    for l in lines {
        let Some(e) = l.ends else { continue };
        if !rect_intersects(&l.bounds, &area) || e.len < 30.0 {
            continue;
        }
        for end in [e.start, e.end] {
            if rect_contains(&area, end.x, end.y) {
                cands.push((hypot(end.x - (x0 + x1) / 2.0, end.y - (y0 + y1) / 2.0), end, e.len));
            }
        }
    }
    cands.sort_by(|a, b| a.0.total_cmp(&b.0));
    cands.into_iter().find_map(|(_, end, len)| closed_head(s, end, len).or_else(|| if triangles { None } else { head(s, end, len) }))
}

/// The stroke being drawn (`points`) as the second barb of a head drawn in two strokes: a short
/// straight stroke (80 pt at most) at the end of one of `lines`, with an earlier one among
/// `lines` at the same end on the other side of the line. The head and the index in `lines` of
/// the earlier barb (it goes: the head replaces both), or None. NotebookView.kt `barbPair`.
pub fn barb_pair(points: &[InkPoint], still: f32, lines: &[LineStroke]) -> Option<(Vec<InkPoint>, usize)> {
    let r = shapes::trimmed(points, still)?;
    let s = &points[r];
    let (x0, y0, x1, y1) = extent(s);
    let big = (x1 - x0).max(y1 - y0);
    if big > 80.0 {
        return None;
    }
    let reach = 10f32.max(big);
    let area = Rect { x0: x0 - reach, y0: y0 - reach, x1: x1 + reach, y1: y1 + reach };
    let near: Vec<(usize, Ends)> = lines.iter().enumerate().filter_map(|(i, l)| l.ends.filter(|_| rect_intersects(&l.bounds, &area)).map(|e| (i, e))).collect();
    let (t0, t1) = (s[0].t_ms as f32, s[s.len() - 1].t_ms as f32);
    for &(li, e) in &near {
        if e.len < 30.0 {
            continue;
        }
        for end in [e.start, e.end] {
            if !rect_contains(&area, end.x, end.y) {
                continue;
            }
            let Some(b2) = barb(s, end, e.len) else { continue };
            // A line, not another barb: well longer than the barbs.
            if e.len < 25f32.max(2.5 * hypot(b2.dx, b2.dy)) {
                continue;
            }
            for &(oi, f) in &near {
                if oi == li {
                    continue;
                }
                let Some((dx, dy)) = barb_from(f.start.x, f.start.y, f.end.x, f.end.y, f.len, end, e.len) else { continue };
                if let Some(head) = head_from_barbs(Barb { dx, dy, p: b2.p }, b2, end, e.len, t0, t1) {
                    return Some((head, oi));
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    //! ArrowsTest.kt of the Android app, ported case by case (same seed, same strokes), and the
    //! FFI's test of the line ends.

    use super::*;
    use crate::ink::{self, Brush};
    use crate::shapes::tests::JavaRandom;
    use crate::shapes::{ShapeKind, detect, max_by_first};
    use std::f64::consts::PI;

    struct Pen {
        rnd: JavaRandom,
    }

    impl Pen {
        fn new() -> Pen {
            Pen { rnd: JavaRandom::new(3) }
        }

        /// A path through `pts`, a sample every ~2 pt with a little wobble, the pressure swelling
        /// in the middle, 6 ms apart.
        fn drawn(&mut self, pts: &[(f32, f32)], wobble: f32) -> Vec<InkPoint> {
            let mut out: Vec<(f32, f32)> = Vec::new();
            for i in 0..pts.len() - 1 {
                let ((ax, ay), (bx, by)) = (pts[i], pts[i + 1]);
                let steps = 1.max((hypot(bx - ax, by - ay) / 2.0) as i32);
                for k in 0..steps {
                    let x = ax + (bx - ax) * k as f32 / steps as f32 + self.rnd.jitter(wobble);
                    let y = ay + (by - ay) * k as f32 / steps as f32 + self.rnd.jitter(wobble);
                    out.push((x, y));
                }
            }
            out.push(pts[pts.len() - 1]);
            let n = out.len();
            out.iter().enumerate().map(|(i, &(x, y))| InkPoint { x, y, pressure: 0.3 + 0.5 * (PI * i as f64 / n as f64).sin() as f32, t_ms: i as u32 * 6 }).collect()
        }

        fn line(&mut self, pts: &[(f32, f32)]) -> Vec<InkPoint> {
            self.drawn(pts, 0.6)
        }

        /// A closed triangle drawn turned by `deg` from pointing along +x, its middle at (cx, cy), `r` to its corners.
        fn triangle(&mut self, cx: f32, cy: f32, r: f32, deg: f64) -> Vec<InkPoint> {
            let corners: Vec<(f32, f32)> = (0..=3)
                .map(|k| {
                    let t = (deg + 120.0 * (k % 3) as f64).to_radians();
                    (cx + r * t.cos() as f32, cy + r * t.sin() as f32)
                })
                .collect();
            self.drawn(&corners, 0.3)
        }
    }

    fn kind(s: &[InkPoint]) -> Option<ShapeKind> {
        detect(s, 0.0, true).map(|s| s.kind)
    }

    fn pts(o: &[InkPoint]) -> Vec<(f32, f32)> {
        o.iter().map(|q| (q.x, q.y)).collect()
    }

    /// A stroke's end (`last`) or start and the way back into it, and its length, as the core
    /// hands it out ([`ends`], `FfiStroke.ends`).
    fn end(s: &[InkPoint], last: bool) -> (LineEnd, f32) {
        let e = ends(s).expect("a line");
        (if last { e.end } else { e.start }, e.len)
    }

    fn head_on(line: &[InkPoint], h: &[InkPoint], last: bool) -> Option<Vec<InkPoint>> {
        let (e, len) = end(line, last);
        head(h, e, len)
    }

    fn close(a: f32, b: f32, tol: f32, what: &str) {
        assert!((a - b).abs() <= tol, "{what}: {a} vs {b} (± {tol})");
    }

    /// Strokes from the tablet (x, y, pressure), as a user drew them; 4 ms apart.
    fn user(name: &str) -> Vec<InkPoint> {
        let s = match name {
            "1224" => "266.0,772.0,0.09 266.1,771.9,0.23 266.3,771.7,0.23 266.7,771.5,0.23 267.3,771.3,0.23 267.9,771.0,0.23 268.7,770.6,0.24 269.6,770.1,0.26 270.6,769.6,0.28 271.8,768.9,0.31 273.1,768.3,0.34 274.4,767.5,0.38 275.8,766.7,0.42 277.2,765.9,0.45 278.7,765.0,0.47 280.3,764.1,0.49 282.0,763.1,0.51 283.6,762.1,0.53 285.4,761.1,0.55 287.2,760.0,0.56 289.1,758.8,0.56 291.1,757.5,0.56 293.2,756.0,0.56 295.3,754.5,0.56 297.6,752.8,0.56 299.9,750.9,0.56 302.1,749.0,0.57 304.2,746.9,0.57 306.3,744.7,0.57 308.4,742.6,0.58 310.2,740.5,0.58 311.9,738.4,0.60 313.4,736.4,0.61 314.8,734.5,0.62 316.0,732.7,0.64 316.9,730.8,0.64 317.6,729.0,0.65 318.0,727.3,0.66 318.2,725.6,0.67 318.2,724.1,0.68 318.0,722.6,0.69 317.6,721.1,0.69 316.9,719.6,0.70 316.1,718.2,0.70 315.2,716.7,0.70 314.0,715.2,0.69 312.7,713.8,0.70 311.3,712.3,0.70 309.7,710.9,0.70 308.1,709.6,0.71 306.3,708.3,0.71 304.4,707.1,0.71 302.5,705.9,0.72 300.6,704.6,0.72 298.7,703.3,0.72 296.8,702.0,0.73 295.1,700.8,0.75 293.6,699.6,0.76 292.4,698.4,0.76 291.3,697.2,0.77 290.6,696.0,0.78 290.1,694.8,0.79 289.8,693.6,0.80 289.7,692.4,0.80 289.7,691.2,0.80 289.7,690.1,0.80 289.9,689.1,0.80 290.1,688.1,0.78 290.1,687.2,0.73 290.2,686.2,0.58 290.1,685.4,0.28",
            "1225" => "284.3,692.8,0.08 284.2,692.8,0.11 284.1,693.0,0.13 284.0,693.0,0.16 283.9,693.1,0.18 283.8,693.2,0.20 283.8,693.2,0.22 283.7,693.3,0.23 283.6,693.3,0.25 283.6,693.4,0.29 283.5,693.5,0.34 283.4,693.4,0.46 283.4,693.3,0.48 283.5,693.1,0.51 283.6,692.9,0.53 283.6,692.8,0.54 283.8,692.6,0.55 283.9,692.3,0.56 284.0,692.1,0.57 284.2,691.8,0.58 284.3,691.5,0.59 284.5,691.2,0.60 284.7,690.8,0.60 284.9,690.4,0.61 285.2,690.0,0.61 285.4,689.6,0.62 285.7,689.2,0.62 285.9,688.8,0.62 286.2,688.4,0.62 286.4,687.9,0.63 286.6,687.4,0.64 286.9,686.9,0.64 287.1,686.5,0.64 287.3,686.0,0.65 287.6,685.6,0.65 287.8,685.2,0.65 288.0,684.8,0.66 288.2,684.4,0.66 288.4,684.0,0.66 288.5,683.6,0.67 288.7,683.3,0.67 288.9,683.0,0.67 289.1,682.7,0.67 289.2,682.4,0.67 289.3,682.2,0.67 289.4,681.9,0.68 289.6,681.7,0.68 289.7,681.5,0.68 289.7,681.4,0.68 289.8,681.2,0.69 289.9,681.1,0.69 290.0,681.0,0.70 290.1,681.0,0.70 290.2,680.9,0.71 290.2,680.9,0.71 290.4,680.8,0.72 290.5,680.8,0.73 290.7,680.8,0.74 290.8,680.9,0.75 291.0,681.0,0.76 291.3,681.2,0.77 291.5,681.4,0.78 291.8,681.6,0.79 292.1,682.0,0.80 292.4,682.4,0.81 292.8,682.8,0.81 293.1,683.3,0.82 293.5,683.8,0.83 293.9,684.3,0.84 294.3,684.8,0.85 294.7,685.4,0.85 295.1,685.9,0.86 295.4,686.4,0.86 295.8,687.0,0.87 296.2,687.6,0.87 296.7,688.1,0.88 297.1,688.6,0.88 297.5,689.1,0.88 297.9,689.5,0.88 298.3,689.9,0.89 298.6,690.2,0.89 299.0,690.5,0.90 299.3,690.8,0.90 299.6,691.0,0.90 299.9,691.2,0.90 300.2,691.4,0.89 300.4,691.5,0.87 300.6,691.5,0.82 300.7,691.6,0.74 300.8,691.6,0.59 301.0,691.7,0.38",
            "1226" => "286.3,696.3,0.05 286.2,696.4,0.06 286.0,696.6,0.09 285.8,696.7,0.11 285.7,696.8,0.14 285.6,696.9,0.19 285.5,696.9,0.44 285.6,696.7,0.51 285.7,696.5,0.56 285.8,696.3,0.60 285.9,695.9,0.63 286.1,695.7,0.65 286.3,695.3,0.66 286.5,694.8,0.68 286.8,694.2,0.68 287.1,693.6,0.69 287.4,692.9,0.69 287.8,692.2,0.70 288.2,691.4,0.70 288.6,690.5,0.70 289.1,689.5,0.69 289.5,688.6,0.69 289.9,687.7,0.69 290.3,686.8,0.69 290.7,686.1,0.69 290.9,685.4,0.70 291.1,684.9,0.70 291.2,684.5,0.70 291.2,684.2,0.71 291.2,684.2,0.72 291.1,684.3,0.72 290.9,684.4,0.72 290.8,684.6,0.72 290.8,684.9,0.72 290.7,685.1,0.72 290.7,685.5,0.72 290.7,685.8,0.72 290.8,686.0,0.72 290.9,686.2,0.72 291.0,686.5,0.72 291.2,686.8,0.72 291.4,687.0,0.72 291.7,687.2,0.72 292.0,687.5,0.72 292.3,687.8,0.72 292.7,688.1,0.72 293.1,688.3,0.72 293.5,688.6,0.73 293.9,688.8,0.73 294.3,689.1,0.73 294.7,689.4,0.72 295.1,689.7,0.72 295.6,690.0,0.72 296.0,690.3,0.73 296.4,690.6,0.73 296.8,690.9,0.73 297.2,691.2,0.73 297.6,691.5,0.73 297.9,691.8,0.74 298.2,692.1,0.74 298.5,692.3,0.74 298.7,692.5,0.74 298.9,692.8,0.74 299.1,692.9,0.74 299.2,693.1,0.75 299.2,693.3,0.78 299.0,693.4,0.80 298.8,693.5,0.82 298.6,693.6,0.83 298.4,693.6,0.83 298.2,693.6,0.82 297.9,693.6,0.71 297.7,693.6,0.31",
            "1227" => "289.1,686.9,0.05 289.1,686.7,0.05 289.1,686.4,0.07 289.1,686.0,0.12 289.2,685.9,0.19 289.3,685.8,0.29 289.4,685.7,0.42 289.4,685.9,0.56 289.3,686.2,0.60 289.0,686.6,0.63 288.7,687.1,0.65 288.4,687.8,0.67 287.9,688.7,0.68 287.4,689.6,0.69 286.9,690.5,0.69 286.4,691.5,0.70 285.9,692.4,0.71 285.4,693.3,0.71 285.0,694.1,0.71 284.7,694.9,0.71 284.4,695.6,0.72 284.2,696.1,0.72 284.2,696.6,0.72 284.2,697.0,0.72 284.2,697.4,0.72 284.4,697.7,0.72 284.7,698.0,0.71 285.1,698.2,0.71 285.6,698.3,0.71 286.2,698.4,0.71 286.8,698.4,0.71 287.5,698.5,0.71 288.2,698.5,0.71 288.9,698.4,0.71 289.6,698.3,0.70 290.4,698.2,0.70 291.1,698.1,0.70 291.8,698.0,0.70 292.6,697.8,0.70 293.3,697.6,0.70 294.0,697.3,0.70 294.7,697.1,0.70 295.2,696.8,0.70 295.8,696.7,0.70 296.2,696.5,0.70 296.7,696.3,0.71 297.0,696.3,0.71 297.3,696.2,0.71 297.5,696.1,0.71 297.6,696.0,0.71 297.7,695.9,0.71 297.7,695.7,0.71 297.6,695.4,0.72 297.4,695.2,0.72 297.3,695.0,0.72 297.0,694.6,0.72 296.7,694.3,0.73 296.4,693.9,0.73 295.9,693.5,0.73 295.5,693.0,0.74 295.0,692.5,0.74 294.5,692.0,0.74 294.0,691.5,0.75 293.5,690.9,0.75 292.9,690.3,0.76 292.4,689.6,0.76 291.9,689.0,0.76 291.5,688.4,0.76 291.1,687.8,0.76 290.8,687.2,0.76 290.4,686.8,0.77 290.3,686.4,0.77 290.1,686.1,0.76 289.9,686.1,0.76 289.8,686.0,0.74 289.6,686.1,0.61 289.5,686.4,0.42 289.4,686.7,0.20",
            "1228" => "282.7,767.5,0.07 282.6,767.5,0.12 282.5,767.6,0.16 282.4,767.6,0.18 282.4,767.7,0.21 282.3,767.7,0.23 282.3,767.8,0.25 282.2,767.8,0.26 282.2,767.9,0.28 282.2,767.9,0.29 282.1,768.0,0.32 282.1,768.1,0.36 282.2,768.0,0.42 282.3,767.9,0.43 282.4,767.8,0.44 282.6,767.6,0.45 282.7,767.5,0.46 282.9,767.2,0.47 283.2,767.0,0.49 283.4,766.8,0.50 283.7,766.5,0.51 284.0,766.1,0.53 284.3,765.8,0.54 284.7,765.4,0.55 285.0,764.9,0.56 285.4,764.5,0.57 285.8,764.0,0.58 286.3,763.5,0.59 286.8,763.0,0.60 287.3,762.4,0.60 287.9,761.7,0.61 288.5,761.0,0.62 289.1,760.3,0.63 289.8,759.5,0.60 290.5,758.7,0.62 291.3,757.9,0.63 292.1,757.0,0.63 293.0,756.0,0.60 294.0,755.1,0.61 295.0,754.1,0.61 296.1,753.0,0.61 297.2,752.0,0.61 298.4,750.9,0.59 299.7,749.7,0.59 301.0,748.6,0.59 302.3,747.4,0.58 303.7,746.2,0.58 305.1,745.0,0.57 306.5,743.7,0.56 307.9,742.3,0.56 309.2,741.0,0.56 310.6,739.6,0.56 312.1,738.1,0.55 313.6,736.7,0.56 315.1,735.2,0.56 316.6,733.7,0.55 318.2,732.2,0.56 319.8,730.8,0.57 321.3,729.4,0.58 322.9,728.0,0.59 324.4,726.8,0.58 326.0,725.5,0.60 327.4,724.2,0.61 328.9,723.1,0.62 330.4,721.9,0.62 331.7,720.9,0.63 333.1,719.9,0.64 334.3,718.9,0.65 335.6,718.1,0.66 336.7,717.2,0.67 337.8,716.5,0.66 338.9,715.8,0.68 339.8,715.2,0.69 340.7,714.6,0.69 341.6,714.1,0.70 342.3,713.6,0.70 343.0,713.1,0.71 343.6,712.7,0.71 344.2,712.3,0.70 344.7,712.0,0.70 345.1,711.7,0.68 345.5,711.4,0.64 345.8,711.1,0.59 346.1,710.9,0.52 346.3,710.6,0.41 346.5,710.4,0.29 346.6,710.1,0.20",
            "b182" => "374.4,222.4,0.11 374.2,222.2,0.12 373.9,222.0,0.16 373.6,221.9,0.18 373.5,221.6,0.20 373.3,221.6,0.22 372.8,221.4,0.28 372.7,221.4,0.30 372.4,221.3,0.33 371.9,221.3,0.37 371.5,221.3,0.42 371.0,221.3,0.46 370.9,221.4,0.48 370.6,221.4,0.50 370.5,221.6,0.52 370.2,221.8,0.54 370.1,221.9,0.55 369.8,222.3,0.57 369.7,222.5,0.59 369.6,222.9,0.60 369.3,223.3,0.62 369.3,223.8,0.63 369.2,224.3,0.64 369.1,224.9,0.66 369.0,225.7,0.64 368.9,226.4,0.66 368.8,227.3,0.67 368.8,228.2,0.68 368.8,229.2,0.68 368.8,230.3,0.68 368.8,231.4,0.69 368.8,232.6,0.69 368.9,233.8,0.69 369.0,235.0,0.69 369.3,236.3,0.69 369.4,237.7,0.69 369.8,239.1,0.70 370.1,240.6,0.68 370.4,242.1,0.68 370.7,243.6,0.68 371.1,245.1,0.68 371.6,246.6,0.67 372.0,248.1,0.68 372.6,249.6,0.68 373.2,251.0,0.67 373.9,252.4,0.67 374.7,253.6,0.66 375.6,254.8,0.67 376.6,255.8,0.67 377.7,256.6,0.68 378.8,257.3,0.68 380.0,257.9,0.67 381.4,258.4,0.67 382.9,258.7,0.68 384.5,258.8,0.66 386.2,258.8,0.67 387.9,258.8,0.66 389.7,258.5,0.66 391.6,258.2,0.66 393.4,257.7,0.67 395.3,257.2,0.67 397.2,256.5,0.67 399.0,255.8,0.65 400.9,255.0,0.66 402.7,254.2,0.66 404.5,253.2,0.65 406.2,252.2,0.66 407.9,251.2,0.66 409.6,250.1,0.65 411.2,248.9,0.66 412.7,247.6,0.67 414.2,246.3,0.65 415.7,244.9,0.66 417.1,243.4,0.64 418.6,241.9,0.65 419.9,240.2,0.65 421.2,238.6,0.66 422.5,236.8,0.66 423.6,234.9,0.64 424.8,233.0,0.65 425.9,231.1,0.65 426.9,228.9,0.65 427.9,226.8,0.64 428.8,224.6,0.64 429.6,222.3,0.66 430.4,220.0,0.64 431.1,217.5,0.62 431.8,215.1,0.64 432.4,212.5,0.64 433.0,210.0,0.62 433.5,207.4,0.61 434.0,204.8,0.63 434.5,202.2,0.62 434.9,199.6,0.63 435.3,196.9,0.64 435.6,194.5,0.62 435.8,192.1,0.63 435.9,189.9,0.64 436.3,187.8,0.66 436.4,185.9,0.66 436.4,184.3,0.66 436.5,182.8,0.68 436.5,181.6,0.69 436.5,180.6,0.70 436.4,179.7,0.71 436.1,179.0,0.71 435.8,178.4,0.72 435.4,178.0,0.73 434.9,177.7,0.73 434.5,177.6,0.72 433.9,177.3,0.71 433.3,177.3,0.69 432.7,177.3,0.66 432.0,177.3,0.60 431.3,177.6,0.53 430.6,177.7,0.43 429.9,178.1,0.31 429.3,178.3,0.20",
            "b189" => "412.1,252.7,0.16 412.0,252.9,0.17 411.9,253.2,0.19 411.6,253.3,0.21 411.6,253.7,0.23 411.4,254.1,0.24 411.4,254.5,0.25 411.5,255.0,0.25 411.8,255.1,0.25 411.9,255.5,0.25 412.2,255.8,0.25 412.5,255.9,0.25 412.9,256.2,0.25 413.2,256.5,0.26 413.7,256.8,0.26 414.2,257.1,0.27 414.7,257.4,0.29 415.2,257.7,0.30 415.9,258.0,0.32 416.6,258.3,0.34 417.3,258.6,0.35 418.1,258.9,0.37 419.0,259.0,0.39 419.9,259.3,0.40 420.8,259.4,0.42 421.7,259.6,0.43 422.7,259.6,0.44 423.7,259.8,0.46 424.7,259.8,0.46 425.8,259.8,0.47 427.0,259.7,0.47 428.1,259.3,0.48 429.2,258.9,0.49 430.4,258.5,0.49 431.5,257.9,0.50 432.7,257.2,0.51 433.9,256.5,0.50 435.1,255.7,0.51 436.2,254.7,0.51 437.4,253.7,0.47 438.5,252.7,0.49 439.6,251.6,0.50 440.7,250.4,0.50 441.8,249.2,0.51 442.8,247.9,0.51 443.7,246.6,0.51 444.6,245.3,0.51 445.5,244.0,0.51 446.3,242.7,0.51 447.1,241.2,0.52 447.8,239.8,0.52 448.5,238.3,0.53 449.1,236.8,0.52 449.6,235.3,0.52 450.1,233.8,0.53 450.5,232.3,0.53 450.8,230.8,0.53 451.1,229.2,0.52 451.2,227.6,0.53 451.5,226.1,0.53 451.5,224.6,0.54 451.5,223.0,0.53 451.5,221.5,0.54 451.4,220.0,0.54 451.1,218.4,0.54 450.8,216.9,0.54 450.5,215.4,0.54 450.1,213.9,0.55 449.6,212.5,0.55 449.1,211.2,0.55 448.4,209.9,0.54 447.8,208.7,0.55 447.1,207.8,0.55 446.3,206.9,0.56 445.6,206.1,0.56 444.7,205.6,0.56 443.9,205.1,0.57 443.0,204.8,0.58 442.0,204.8,0.58 441.0,204.8,0.57 439.9,204.9,0.58 438.8,205.4,0.57 437.7,205.9,0.57 436.5,206.5,0.57 435.4,207.3,0.57 434.3,208.2,0.55 433.1,209.2,0.55 432.0,210.3,0.54 430.8,211.4,0.54 429.7,212.6,0.54 428.7,213.8,0.54 427.7,215.0,0.54 426.7,216.3,0.54 425.8,217.5,0.53 425.1,218.7,0.53 424.4,219.7,0.53 423.8,220.7,0.53 423.3,221.5,0.51 422.9,222.3,0.50 422.7,222.9,0.47 422.6,223.4,0.42 422.6,223.9,0.35 422.6,224.4,0.26 422.6,224.7,0.18",
            "b190" => "422.2,224.0,0.05 422.1,224.1,0.05 421.9,224.2,0.06 421.8,224.2,0.07 421.6,224.3,0.09 421.6,224.2,0.17 421.6,223.9,0.20 421.6,223.7,0.22 421.6,223.4,0.25 421.6,222.9,0.28 421.6,222.4,0.31 421.6,222.2,0.33 421.6,221.9,0.34 421.6,221.6,0.36 421.6,221.2,0.37 421.6,220.8,0.39 421.6,220.3,0.40 421.6,219.8,0.41 421.6,219.3,0.42 421.7,218.7,0.43 421.7,218.1,0.44 421.8,217.4,0.44 421.8,216.7,0.45 421.9,216.0,0.44 422.0,215.3,0.46 422.1,214.6,0.46 422.1,213.9,0.47 422.2,213.1,0.47 422.3,212.4,0.48 422.3,211.6,0.48 422.4,210.8,0.48 422.5,210.1,0.49 422.6,209.2,0.48 422.6,208.5,0.48 422.7,207.7,0.49 422.7,207.0,0.49 422.8,206.3,0.49 422.8,205.7,0.50 422.9,205.1,0.50 423.0,204.5,0.50 423.1,204.0,0.50 423.1,203.6,0.51 423.2,203.2,0.51 423.2,202.9,0.52 423.2,202.6,0.53 423.3,202.4,0.54 423.3,202.6,0.55 423.3,202.9,0.55 423.3,203.2,0.55 423.3,203.6,0.55 423.3,204.1,0.55 423.3,204.7,0.55 423.3,205.3,0.55 423.2,206.1,0.55 423.2,206.8,0.54 423.1,207.7,0.55 423.1,208.6,0.55 423.1,209.4,0.55 423.0,210.3,0.54 422.9,211.2,0.54 422.8,212.1,0.54 422.7,213.0,0.54 422.6,213.9,0.54 422.6,214.8,0.55 422.4,215.7,0.54 422.4,216.6,0.54 422.3,217.4,0.55 422.2,218.2,0.55 422.2,218.9,0.55 422.1,219.5,0.55 422.1,220.1,0.55 422.1,220.6,0.56 422.1,221.0,0.56 422.1,221.4,0.56 422.1,221.8,0.56 422.1,222.1,0.56 422.1,222.5,0.56 422.2,222.7,0.57 422.2,222.9,0.57 422.3,223.2,0.56 422.4,223.4,0.57 422.5,223.6,0.56 422.6,223.7,0.56 422.8,223.8,0.56 422.9,223.9,0.56 423.1,224.1,0.57 423.2,224.1,0.56 423.5,224.2,0.56 423.9,224.2,0.56 424.1,224.2,0.56 424.4,224.2,0.56 424.8,224.2,0.56 425.2,224.2,0.56 425.7,224.2,0.56 426.3,223.9,0.56 427.0,223.9,0.56 427.8,223.5,0.56 428.5,223.2,0.56 429.3,222.9,0.55 430.1,222.5,0.56 431.0,222.2,0.56 431.8,221.8,0.56 432.6,221.4,0.56 433.4,221.1,0.56 434.3,220.7,0.57 435.1,220.3,0.57 435.9,220.0,0.58 436.7,219.7,0.58 437.4,219.4,0.59 438.1,219.3,0.59 438.8,218.9,0.60 439.4,218.8,0.60 440.1,218.5,0.60 440.6,218.4,0.61 441.1,218.1,0.61 441.5,218.0,0.62 441.8,218.0,0.62 442.1,217.9,0.62 442.5,217.8,0.60 442.4,218.0,0.26",
            "c133" => "278.3,200.8,0.10 278.1,201.0,0.11 277.8,201.2,0.12 277.6,201.5,0.12 277.3,201.8,0.14 277.2,202.1,0.14 277.1,202.3,0.15 277.0,202.5,0.16 276.9,202.7,0.17 276.9,202.9,0.18 276.9,203.0,0.20 276.8,203.1,0.22 276.8,203.2,0.27 276.9,203.3,0.33 277.1,203.2,0.35 277.2,203.2,0.37 277.3,203.0,0.38 277.5,202.8,0.40 277.7,202.7,0.41 277.9,202.5,0.42 278.2,202.2,0.44 278.6,201.9,0.46 278.9,201.5,0.47 279.3,201.1,0.48 279.8,200.6,0.49 280.3,200.1,0.49 281.0,199.4,0.50 281.7,198.7,0.51 282.5,198.0,0.51 283.4,197.1,0.51 284.4,196.2,0.52 285.6,195.2,0.51 286.8,194.1,0.52 288.1,193.0,0.53 289.5,191.8,0.53 291.0,190.5,0.53 292.6,189.2,0.51 294.2,187.8,0.52 296.0,186.4,0.52 297.8,185.1,0.51 299.7,183.6,0.51 301.6,182.2,0.51 303.5,180.9,0.51 305.4,179.6,0.51 307.3,178.2,0.51 309.3,176.9,0.50 311.3,175.6,0.51 313.2,174.4,0.51 315.1,173.2,0.51 317.0,172.1,0.51 318.8,170.9,0.52 320.5,169.9,0.51 322.2,169.0,0.51 323.7,168.1,0.51 325.2,167.2,0.52 326.6,166.4,0.52 327.8,165.6,0.53 329.0,165.0,0.53 330.0,164.4,0.54 330.9,163.8,0.53 331.7,163.4,0.53 332.4,163.1,0.50 332.9,162.7,0.46 333.5,162.5,0.41 333.9,162.3,0.34 334.2,162.1,0.26 334.5,162.1,0.17",
            "c134" => "294.7,169.1,0.07 294.9,169.0,0.08 295.1,168.8,0.10 295.4,168.6,0.11 295.7,168.4,0.13 296.0,168.3,0.14 296.4,168.1,0.16 296.9,168.0,0.18 297.4,167.8,0.20 298.1,167.6,0.22 298.7,167.3,0.25 299.4,167.0,0.27 300.2,166.7,0.29 301.0,166.4,0.31 301.9,166.0,0.33 302.8,165.7,0.34 303.8,165.3,0.35 304.8,164.9,0.36 305.9,164.6,0.37 306.9,164.2,0.38 308.0,163.8,0.38 309.1,163.5,0.39 310.3,163.1,0.40 311.5,162.7,0.41 312.7,162.4,0.41 313.9,162.0,0.42 315.2,161.6,0.42 316.5,161.2,0.43 317.8,160.9,0.43 319.1,160.5,0.44 320.4,160.1,0.44 321.8,159.7,0.45 323.2,159.3,0.46 324.5,159.0,0.46 325.9,158.6,0.46 327.2,158.3,0.46 328.5,158.0,0.47 329.8,157.7,0.47 331.0,157.4,0.47 332.2,157.2,0.47 333.4,157.1,0.48 334.5,156.9,0.48 335.6,156.9,0.49 336.7,156.7,0.49 337.6,156.7,0.50 338.6,156.7,0.51 339.4,156.7,0.52 340.3,156.7,0.53 341.0,156.9,0.54 341.7,157.0,0.55 342.3,157.2,0.55 342.9,157.5,0.57 343.3,157.8,0.58 343.7,158.2,0.59 344.0,158.6,0.60 344.2,159.0,0.61 344.4,159.5,0.63 344.4,160.0,0.64 344.4,160.6,0.65 344.4,161.3,0.66 344.1,161.9,0.67 343.8,162.7,0.67 343.5,163.4,0.68 343.0,164.2,0.68 342.5,165.1,0.68 341.9,166.0,0.69 341.2,166.9,0.68 340.5,167.8,0.69 339.8,168.8,0.69 339.0,169.7,0.68 338.2,170.6,0.69 337.4,171.5,0.69 336.7,172.4,0.69 335.9,173.2,0.69 335.2,174.0,0.70 334.5,174.8,0.71 333.8,175.5,0.71 333.2,176.2,0.70 332.6,176.9,0.71 332.0,177.5,0.71 331.6,178.1,0.71 331.1,178.6,0.69 330.7,179.1,0.67 330.3,179.5,0.62 330.0,179.9,0.54 329.7,180.2,0.43 329.4,180.5,0.28",
            "1230" => "340.0,711.6,0.05 339.9,711.6,0.08 339.8,711.6,0.10 339.7,711.5,0.12 339.6,711.5,0.14 339.5,711.4,0.16 339.4,711.3,0.18 339.3,711.3,0.21 339.3,711.2,0.24 339.2,711.2,0.28 339.2,711.1,0.33 339.2,711.0,0.36 339.4,710.9,0.38 339.6,710.9,0.41 339.7,710.8,0.42 339.9,710.8,0.44 340.1,710.6,0.46 340.4,710.6,0.47 340.7,710.4,0.49 341.0,710.3,0.50 341.4,710.2,0.51 341.8,710.1,0.53 342.2,709.9,0.54 342.6,709.8,0.55 343.1,709.6,0.56 343.5,709.5,0.56 344.0,709.3,0.57 344.5,709.2,0.58 345.0,709.1,0.58 345.5,708.9,0.58 345.9,708.8,0.59 346.4,708.6,0.59 346.8,708.5,0.59 347.3,708.4,0.60 347.7,708.2,0.60 348.1,708.2,0.60 348.4,708.0,0.61 348.7,708.0,0.61 349.0,707.9,0.62 349.3,707.9,0.62 349.5,707.9,0.62 349.7,707.8,0.62 349.9,707.8,0.62 350.1,707.8,0.63 350.2,707.8,0.63 350.4,707.9,0.64 350.5,707.9,0.64 350.6,708.0,0.65 350.7,708.1,0.65 350.7,708.3,0.66 350.7,708.4,0.67 350.7,708.6,0.67 350.7,708.8,0.68 350.7,709.0,0.69 350.7,709.3,0.69 350.6,709.6,0.69 350.6,709.9,0.70 350.4,710.3,0.71 350.2,710.7,0.71 350.1,711.1,0.71 349.9,711.6,0.71 349.7,712.1,0.71 349.5,712.6,0.72 349.3,713.2,0.71 349.1,713.8,0.71 348.9,714.4,0.71 348.8,715.0,0.71 348.6,715.6,0.71 348.5,716.2,0.71 348.3,716.7,0.72 348.2,717.2,0.71 348.0,717.6,0.72 347.9,718.1,0.72 347.8,718.5,0.72 347.7,718.8,0.72 347.6,719.2,0.73 347.5,719.5,0.73 347.4,719.8,0.73 347.3,720.0,0.72 347.3,720.2,0.71 347.2,720.3,0.68 347.1,720.4,0.64 347.1,720.4,0.58 346.9,720.5,0.49 346.8,720.5,0.36 346.7,720.5,0.24",
            _ => panic!("no stroke {name}"),
        };
        s.split(' ')
            .enumerate()
            .map(|(i, p)| {
                let v: Vec<f32> = p.split(',').map(|x| x.parse().unwrap()).collect();
                InkPoint { x: v[0], y: v[1], pressure: v[2], t_ms: i as u32 * 4 }
            })
            .collect()
    }

    #[test]
    fn a_triangle_at_a_line_end_points_along_the_line_however_it_was_turned() {
        let mut pen = Pen::new();
        let line = pen.drawn(&[(100.0, 200.0), (300.0, 202.0)], 0.3);
        let (e, len) = end(&line, true);
        for deg in [0.0, 35.0, -50.0, 90.0, 180.0] {
            let tri = pen.triangle(e.x - 4.0, e.y + 1.0, 9.0, deg);
            let head = closed_head(&tri, e, len).unwrap_or_else(|| panic!("turned {deg}°"));
            let p = pts(&head);
            // The point: foremost along the line, at its end; the two other corners mirror each other about the line.
            let tip = max_by_first(p.iter().copied(), |q| q.0 * -e.dx + q.1 * -e.dy).unwrap();
            close(tip.0, e.x, 0.6, "tip x");
            close(tip.1, e.y, 0.6, "tip y");
            let side: Vec<f32> = p.iter().filter(|q| (q.0 - tip.0) * e.dx + (q.1 - tip.1) * e.dy > 5.0).map(|q| (q.0 - tip.0) * -e.dy + (q.1 - tip.1) * e.dx).collect();
            let (lo, hi) = (side.iter().fold(f32::MAX, |a, &b| a.min(b)), side.iter().fold(-f32::MAX, |a, &b| a.max(b)));
            close(hi + lo, 0.0, 1.2, &format!("symmetric about the line ({deg}°)"));
        }
        // A circle at the end stays a circle; a triangle far back along the line is no head.
        let circle: Vec<(f32, f32)> = (0..=30)
            .map(|i| {
                let t = 2.0 * PI * i as f64 / 30.0;
                (e.x - 6.0 + 7.0 * t.cos() as f32, e.y + 7.0 * t.sin() as f32)
            })
            .collect();
        let c = pen.drawn(&circle, 0.2);
        assert_eq!(closed_head(&c, e, len), None, "o");
        let far = pen.triangle(e.x - 40.0, e.y, 9.0, 30.0);
        assert_eq!(closed_head(&far, e, len), None, "behind");
    }

    #[test]
    fn a_wobble_as_the_pen_rests_does_not_turn_the_head() {
        // A straight line whose last few points dip off 30° (the pen landing): the head still lies along the line.
        let mut pen = Pen::new();
        let mut p: Vec<(f32, f32)> = (0..=95).map(|i| (100.0 + i as f32 * 2.0, 200.0)).collect();
        p.extend((1..=4).map(|i| (290.0 + i as f32 * 1.7, 200.0 + i as f32 * 1.0)));
        let line = pen.drawn(&p, 0.1);
        close(end(&line, true).0.dy, 0.0, 0.06, "along the line, not the dip");
    }

    #[test]
    fn heads_on_users_lines() {
        // An S-curve, then a head at its top end: a caret with a hook, another, a small triangle.
        let curve = user("1224");
        for h in ["1225", "1226", "1227"] {
            assert!(head_on(&curve, &user(h), true).is_some(), "head {h} on the curve");
        }
        // A straight-ish line and a V with a hook where the pen landed.
        assert!(head_on(&user("1228"), &user("1230"), true).is_some(), "V with a hook");
        // The triangle stays closed (a filled-looking head), its point on the curve's end.
        let tri = pts(&head_on(&curve, &user("1227"), true).unwrap());
        close(tri[0].0, tri[tri.len() - 1].0, 0.5, "closed");
        let (e, _) = end(&curve, true);
        assert!(tri.iter().any(|q| hypot(q.0 - e.x, q.1 - e.y) < 0.5));
    }

    /// Drawn by the core (which smooths every stroke), the head's point reaches the line's end: the line doesn't poke through it.
    #[test]
    fn the_line_ends_inside_the_point() {
        let mut pen = Pen::new();
        let brush = Brush { width: 1.0, thinning: 0.3, color: 0xFF00_0000, kind: 0 };
        let line = pen.drawn(&[(100.0, 200.0), (300.0, 140.0)], 0.2);
        let (e, _) = end(&line, true);
        let v = pen.drawn(&[(e.x - 14.0, e.y - 2.0), (e.x + 1.0, e.y), (e.x - 6.0, e.y + 13.0)], 0.2);
        let head = head_on(&line, &v, true).unwrap();
        let o = ink::outline(&head, &brush);
        // How far the head's outline reaches forward past the line's end (along the line's way on).
        let reach = o.chunks_exact(2).map(|q| -((q[0] - e.x) * e.dx + (q[1] - e.y) * e.dy)).fold(-f32::MAX, f32::max);
        assert!(reach >= 0.4, "the point reaches {reach} pt past the end (the line's round end reaches 0.5)");
    }

    /// A big, rounded head drawn a bit past a curve's end: tidied, it lies about the curve's way and isn't huge.
    #[test]
    fn a_big_leaning_head_comes_out_along_the_line() {
        let line = user("c133");
        let (e, len) = end(&line, true);
        let p = pts(&head_on(&line, &user("c134"), true).expect("head"));
        let d = |q: (f32, f32), r: (f32, f32)| hypot(q.0 - r.0, q.1 - r.1);
        let tip = shapes::min_by_first(p.iter().copied(), |q| d(q, (e.x, e.y))).unwrap();
        let far = max_by_first(p.iter().copied(), |q| d(q, tip)).unwrap();
        // The two barb ends: the farthest point on each side of the line's way.
        let side = |q: &(f32, f32)| e.dx * (q.1 - e.y) - e.dy * (q.0 - e.x);
        let left = max_by_first(p.iter().copied().filter(|q| side(q) > 0.0), |q| d(q, (e.x, e.y))).unwrap();
        let right = max_by_first(p.iter().copied().filter(|q| side(q) < 0.0), |q| d(q, (e.x, e.y))).unwrap();
        let angle = |q: (f32, f32)| {
            let (vx, vy) = (q.0 - e.x, q.1 - e.y);
            (((vx * e.dx + vy * e.dy) / hypot(vx, vy)) as f64).acos().to_degrees()
        };
        assert!((angle(left) - angle(right)).abs() <= 6.0, "symmetric about the line: {} vs {}", angle(left), angle(right));
        assert!(d(far, tip) <= len * 0.31, "not huge: {} on a line of {len}", d(far, tip));
    }

    #[test]
    fn heads_on_curling_ends() {
        // Curves that curl over at the top and end with a little flick, as drawn on the tablet:
        // their way at the end is the curve's, not the flick's.
        let (e, _) = end(&user("b182"), true);
        assert!(e.dy > 0.6, "182 ends going up: {e:?}"); // back into it = downwards
        // A V right at the end of a curve that loops over (it was not taken before).
        assert!(head_on(&user("b189"), &user("b190"), true).is_some(), "V on the looping curve, end={:?}", end(&user("b189"), true));
    }

    #[test]
    fn a_head_drawn_as_two_barbs() {
        let mut pen = Pen::new();
        let line = pen.drawn(&[(100.0, 200.0), (300.0, 200.0)], 0.3);
        let (e, len) = end(&line, true);
        // One barb up-left from the tip, one down-left, drawn as separate strokes.
        let first = pen.drawn(&[(e.x - 1.0, e.y + 1.0), (e.x - 18.0, e.y - 12.0)], 0.2);
        let second = pen.drawn(&[(e.x + 1.0, e.y), (e.x - 17.0, e.y + 13.0)], 0.2);
        let b1 = barb(&first, e, len).expect("first barb");
        let b2 = barb(&second, e, len).expect("second barb");
        let head = head_from_barbs(b1, b2, e, len, 0.0, 100.0).expect("head");
        let tip = max_by_first(pts(&head), |q| q.0).unwrap();
        close(tip.0, e.x, 0.5, "tip");
        // Both on the same side: no head. A stroke pointing away from the line: no barb.
        assert_eq!(head_from_barbs(b1, b1, e, len, 0.0, 100.0), None);
        let away = pen.drawn(&[(e.x, e.y), (e.x + 18.0, e.y - 12.0)], 0.2);
        assert_eq!(barb(&away, e, len), None);
    }

    #[test]
    fn one_stroke_arrows() {
        let mut pen = Pen::new();
        // Shaft, then out to one barb, back to the tip, out to the other.
        let both = pen.line(&[(100.0, 200.0), (300.0, 200.0), (276.0, 186.0), (300.0, 200.0), (278.0, 216.0)]);
        let a = detect(&both, 0.0, true).unwrap();
        assert_eq!(a.kind, ShapeKind::Arrow);
        let p = pts(&a.points);
        close(p[0].0, 100.0, 2.0, "start");
        assert!(p.iter().take_while(|q| q.0 < 299.0).all(|q| (q.1 - p[0].1).abs() < 1.5), "straight shaft");
        let tip = max_by_first(p.iter().copied(), |q| q.0).unwrap();
        close(tip.0, 300.0, 2.0, "tip");
        let barbs: Vec<(f32, f32)> = p.iter().copied().filter(|q| hypot(q.0 - tip.0, q.1 - tip.1) > 20.0 && q.0 > 250.0).collect();
        let up = shapes::min_by_first(barbs.iter().copied(), |q| q.1).unwrap();
        let down = max_by_first(barbs.iter().copied(), |q| q.1).unwrap();
        let ang_up = ((tip.1 - up.1) as f64).atan2((tip.0 - up.0) as f64).to_degrees();
        let ang_down = ((down.1 - tip.1) as f64).atan2((tip.0 - down.0) as f64).to_degrees();
        assert!((ang_up - ang_down).abs() <= 4.0, "{ang_up} vs {ang_down}");
        let ps: Vec<f32> = a.points.iter().map(|q| q.pressure).collect();
        let (lo, hi) = (ps.iter().fold(f32::MAX, |a, &b| a.min(b)), ps.iter().fold(-f32::MAX, |a, &b| a.max(b)));
        assert!(hi - lo > 0.1, "pressure varies: {lo}..{hi}");
        // As the pen lifts (no hold) too, with both barbs.
        assert!(from_stroke(&both, true).is_some());
        // One barb only: held, the other comes as its mirror image; not as the pen lifts.
        let one = pen.line(&[(100.0, 300.0), (260.0, 220.0), (262.0, 248.0)]);
        assert_eq!(kind(&one), Some(ShapeKind::Arrow));
        assert_eq!(from_stroke(&one, true), None);
        // Drawn from the tip: barb first, then the shaft; both ends.
        let s = pen.line(&[(130.0, 115.0), (100.0, 100.0), (300.0, 100.0)]);
        assert_eq!(kind(&s), Some(ShapeKind::Arrow));
        let s = pen.line(&[(125.0, 85.0), (100.0, 100.0), (125.0, 115.0), (100.0, 100.0), (300.0, 100.0), (275.0, 85.0), (300.0, 100.0), (275.0, 115.0)]);
        assert_eq!(kind(&s), Some(ShapeKind::Arrow));
    }

    #[test]
    fn a_curved_arrow_in_one_stroke_keeps_its_curve() {
        let mut pen = Pen::new();
        // A quarter arc ending upwards, then a head (barbs back down along it).
        let arc: Vec<(f32, f32)> = (0..=40)
            .map(|i| {
                let t = PI / 2.0 * i as f64 / 40.0;
                (100.0 + 150.0 * t.sin() as f32, 300.0 - 150.0 * (1.0 - t.cos()) as f32)
            })
            .collect();
        let (tx, ty) = arc[arc.len() - 1];
        let mut path = arc.clone();
        path.extend([(tx - 11.0, ty + 17.0), (tx, ty), (tx + 10.0, ty + 18.0)]);
        let s = pen.drawn(&path, 0.3);
        let out = from_stroke(&s, true).expect("curved arrow");
        // The arc's middle is still where it was drawn.
        let (mx, my) = arc[20];
        assert!(out.iter().any(|q| hypot(q.x - mx, q.y - my) < 2.0));
    }

    #[test]
    fn other_strokes_are_no_arrows() {
        let mut pen = Pen::new();
        assert_eq!(kind(&pen.line(&[(100.0, 100.0), (100.0, 300.0), (250.0, 300.0)])), None, "an L");
        assert_eq!(kind(&pen.line(&[(100.0, 200.0), (130.0, 240.0), (220.0, 140.0)])), None, "a tick");
        assert_eq!(kind(&pen.line(&[(100.0, 100.0), (150.0, 150.0), (200.0, 100.0), (250.0, 150.0), (300.0, 100.0)])), None, "a zigzag");
        assert_eq!(kind(&pen.line(&[(100.0, 100.0), (300.0, 140.0)])), Some(ShapeKind::Line), "a straight line stays a line");
        // Writing (sharp turns all along) is no shaft, also as the pen lifts.
        let word = pen.line(&[
            (100.0, 200.0),
            (110.0, 170.0),
            (120.0, 200.0),
            (130.0, 170.0),
            (140.0, 200.0),
            (150.0, 170.0),
            (160.0, 200.0),
            (200.0, 200.0),
            (190.0, 192.0),
            (200.0, 200.0),
            (190.0, 208.0),
        ]);
        assert_eq!(from_stroke(&word, true), None);
    }

    #[test]
    fn a_vee_at_the_end_of_a_line_becomes_its_head() {
        let mut pen = Pen::new();
        let line = pen.drawn(&[(100.0, 200.0), (300.0, 202.0)], 0.3);
        let (e, _) = end(&line, true);
        let vee = pen.line(&[(282.0, 186.0), (302.0, 199.0), (279.0, 217.0)]);
        let h = head_on(&line, &vee, true).expect("head");
        let tip = max_by_first(pts(&h), |q| q.0).unwrap();
        close(tip.0, e.x, 0.5, "tip x");
        close(tip.1, e.y, 0.5, "tip y");
        // A "<" after the line (opening away from it) and a "v" below its end stay as they are.
        let lt = pen.line(&[(322.0, 188.0), (303.0, 200.0), (322.0, 213.0)]);
        assert_eq!(head_on(&line, &lt, true), None, "<");
        let v = pen.line(&[(290.0, 210.0), (300.0, 230.0), (310.0, 210.0)]);
        assert_eq!(head_on(&line, &v, true), None, "v");
        // Drawn from the point: out to one barb, back along it, out to the other.
        let retraced = pen.line(&[(e.x, e.y), (e.x - 18.0, e.y - 13.0), (e.x + 0.5, e.y), (e.x - 17.0, e.y + 14.0)]);
        assert!(head_on(&line, &retraced, true).is_some(), "from the point, retraced");
        // Number lines keep their ends: a round bracket or a small circle at a line's end is no head.
        let (start, len) = end(&line, false); // the line's left end, the way into it is to the right
        let paren: Vec<(f32, f32)> = (0..=24)
            .map(|i| {
                let t = PI * (0.5 + i as f64 / 24.0);
                (start.x + 11.0 + 12.0 * t.cos() as f32, start.y + 12.0 * t.sin() as f32)
            })
            .collect();
        let p = pen.drawn(&paren, 0.2);
        let _same_size = pen.drawn(&paren, 0.2); // the Kotlin test draws it twice (once for its size)
        assert_eq!(head(&p, start, len), None, "(");
        let circle: Vec<(f32, f32)> = (0..=30)
            .map(|i| {
                let t = 2.0 * PI * i as f64 / 30.0;
                (start.x - 6.0 + 6.0 * t.cos() as f32, start.y + 6.0 * t.sin() as f32)
            })
            .collect();
        let c = pen.drawn(&circle, 0.2);
        assert_eq!(head(&c, start, len), None, "o");
        // A head as big as the line is no head.
        let short = pen.line(&[(100.0, 200.0), (130.0, 200.0)]);
        let big = pen.line(&[(90.0, 160.0), (132.0, 200.0), (90.0, 240.0)]);
        assert_eq!(head_on(&short, &big, true), None, "big");
    }

    fn pt(x: f32, y: f32) -> InkPoint {
        InkPoint { x, y, pressure: 0.5, t_ms: 0 }
    }

    fn angle_between(a: (f32, f32), b: (f32, f32)) -> f32 {
        ((a.0 * b.0 + a.1 * b.1) / (a.0.hypot(a.1) * b.0.hypot(b.1))).clamp(-1.0, 1.0).acos().to_degrees()
    }

    /// The FFI's test of `stroke_ends`.
    #[test]
    fn line_ends_point_where_the_line_runs_not_where_it_wobbled() {
        // A straight line to the right whose last 5 pt wobble 30° down (the pen landing to rest).
        let mut p: Vec<_> = (0..=95).map(|i| pt(i as f32, 0.0)).collect();
        for k in 1..=5 {
            p.push(pt(95.0 + k as f32 * 0.87, k as f32 * 0.5));
        }
        let e = ends(&p).unwrap();
        assert!(angle_between((e.end.dx, e.end.dy), (-1.0, 0.0)) < 4.0, "back into the line: {:?}", e.end);
        assert!(angle_between((e.start.dx, e.start.dy), (1.0, 0.0)) < 4.0);
        assert_eq!(e.to_array()[8], e.len);
        // A quarter circle (radius 60) from (60, 0) to (0, 60): at its end it runs straight left.
        let arc: Vec<_> = (0..=90)
            .map(|d| {
                let t = (d as f32).to_radians();
                pt(60.0 * t.cos(), 60.0 * t.sin())
            })
            .collect();
        let e = ends(&arc).unwrap();
        // Back into the stroke at the end is down-right; the fitted way lies within a few degrees of the tangent.
        assert!(angle_between((e.end.dx, e.end.dy), (1.0, -0.15)) < 12.0, "{:?}", e.end);
        assert_eq!(ends(&[pt(1.0, 1.0)]), None, "a dot");
    }

    /// The page-level search (NotebookView.kt `arrowHead`, `barbPair`): the line among the page's
    /// strokes, its nearest end, the earlier barb that goes.
    #[test]
    fn heads_found_among_the_page_strokes() {
        let mut pen = Pen::new();
        let line = pen.drawn(&[(100.0, 200.0), (300.0, 202.0)], 0.3);
        let other = pen.drawn(&[(100.0, 400.0), (300.0, 400.0)], 0.3);
        let stroke = |pts: &[InkPoint]| LineStroke { bounds: ink::bounds(pts, &Brush::default()), ends: ends(pts) };
        let lines = [stroke(&other), stroke(&line)];
        let (e, _) = end(&line, true);
        let vee = pen.line(&[(282.0, 186.0), (302.0, 199.0), (279.0, 217.0)]);
        let h = head_at_line_ends(&vee, 0.0, &lines, false).expect("a V at the line's end");
        let tip = max_by_first(pts(&h), |q| q.0).unwrap();
        close(tip.0, e.x, 0.5, "tip");
        // Only triangles: a V waits.
        assert_eq!(head_at_line_ends(&vee, 0.0, &lines, true), None);
        let tri = pen.triangle(e.x - 4.0, e.y + 1.0, 9.0, 35.0);
        assert!(head_at_line_ends(&tri, 0.0, &lines, true).is_some(), "a triangle at once");
        // Far from any line end: nothing.
        let lone = pen.line(&[(182.0, 286.0), (202.0, 299.0), (179.0, 317.0)]);
        assert_eq!(head_at_line_ends(&lone, 0.0, &lines, false), None);
        // A head as two barbs: the second one finds the first, which goes.
        let first = pen.drawn(&[(e.x - 1.0, e.y + 1.0), (e.x - 18.0, e.y - 12.0)], 0.2);
        let with_first = [stroke(&other), stroke(&line), stroke(&first)];
        let second = pen.drawn(&[(e.x + 1.0, e.y), (e.x - 17.0, e.y + 13.0)], 0.2);
        let (h, gone) = barb_pair(&second, 0.0, &with_first).expect("the second barb");
        assert_eq!(gone, 2);
        close(max_by_first(pts(&h), |q| q.0).unwrap().0, e.x, 0.5, "tip");
        assert_eq!(barb_pair(&second, 0.0, &lines), None, "no first barb");
    }
}
