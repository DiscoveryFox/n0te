//! Shapes: a hand-drawn stroke turned into a clean straight line, triangle, rectangle, quad,
//! polygon or ellipse when the pen is held still at its end, and a snapped shape resized while
//! the pen stays down. Pure geometry on a stroke's samples (sheet points); no UI.
//!
//! The Android app has the same rules in Kotlin: `n0te/android/.../Shapes.kt` (this is a
//! line-by-line port; every threshold is the Kotlin one). The arithmetic follows the Kotlin
//! code too: `Float` math in `f32`, `Double` math in `f64`, trigonometry and `hypot` in double
//! and rounded to float (as `kotlin.math` does for `Float`), so the same samples give the same
//! shape on the tablet, the laptop and Windows. When to call it (how long the pen rests, how
//! far it may tremble) is in [`crate::snap`]; arrows in [`crate::arrows`].

use std::ops::RangeInclusive;

use crate::arrows;
use crate::ink::InkPoint;

// --- Kotlin math ------------------------------------------------------------------------------

/// `kotlin.math.hypot(Float, Float)`: in double, rounded to float.
pub(crate) fn hypot(a: f32, b: f32) -> f32 {
    (a as f64).hypot(b as f64) as f32
}
/// `kotlin.math.atan2(Float, Float)`.
pub(crate) fn atan2(y: f32, x: f32) -> f32 {
    (y as f64).atan2(x as f64) as f32
}
/// `kotlin.math.cos(Float)`.
pub(crate) fn cos(a: f32) -> f32 {
    (a as f64).cos() as f32
}
/// `kotlin.math.sin(Float)`.
pub(crate) fn sin(a: f32) -> f32 {
    (a as f64).sin() as f32
}
/// `Math.round(Double)` (halves round up, toward +∞), as a double.
pub(crate) fn round_half_up(x: f64) -> f64 {
    (x + 0.5).floor()
}
/// `average()` of floats: summed in double.
pub(crate) fn avg(v: impl IntoIterator<Item = f32>) -> f64 {
    let (mut s, mut n) = (0.0f64, 0usize);
    for x in v {
        s += x as f64;
        n += 1;
    }
    if n == 0 { f64::NAN } else { s / n as f64 }
}
/// `sum()` of floats: summed in float, in order.
pub(crate) fn fsum(v: &[f32]) -> f32 {
    v.iter().fold(0.0f32, |a, &b| a + b)
}
/// A sample time computed as a float (as Kotlin keeps it) as stored: whole milliseconds,
/// truncated (as the FFI turns the Android app's samples into [`InkPoint`]s).
pub(crate) fn ms(t: f32) -> u32 {
    t.max(0.0) as u32
}
/// Kotlin's `minBy`: the first item with the smallest key.
pub(crate) fn min_by_first<T: Copy, K: PartialOrd>(items: impl IntoIterator<Item = T>, key: impl Fn(T) -> K) -> Option<T> {
    let mut best: Option<(T, K)> = None;
    for it in items {
        let k = key(it);
        if best.as_ref().is_none_or(|(_, bk)| *bk > k) {
            best = Some((it, k));
        }
    }
    best.map(|b| b.0)
}
/// Kotlin's `maxBy`: the first item with the largest key.
pub(crate) fn max_by_first<T: Copy, K: PartialOrd>(items: impl IntoIterator<Item = T>, key: impl Fn(T) -> K) -> Option<T> {
    let mut best: Option<(T, K)> = None;
    for it in items {
        let k = key(it);
        if best.as_ref().is_none_or(|(_, bk)| *bk < k) {
            best = Some((it, k));
        }
    }
    best.map(|b| b.0)
}

/// Distance from (px, py) to the segment (ax, ay)–(bx, by) (NotebookView.kt `segDist`).
pub fn seg_dist(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-6 { 0.0 } else { (((px - ax) * dx + (py - ay) * dy) / l2).clamp(0.0, 1.0) };
    hypot(px - (ax + t * dx), py - (ay + t * dy))
}

// --- shapes -----------------------------------------------------------------------------------

/// What a stroke was recognised as (Kotlin: the `kind` strings, see [`ShapeKind::name`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShapeKind {
    Line,
    Triangle,
    /// Four corners at about right angles: squared up (level within 10°, a square when its
    /// sides are within 7 %).
    Rectangle,
    /// Four corners, not right angles: as drawn.
    Quad,
    /// Five or six corners.
    Polygon,
    Ellipse,
    /// Shaft and head(s), see [`crate::arrows`].
    Arrow,
}

impl ShapeKind {
    /// The Kotlin name ("line", "triangle", "rectangle", "quad", "polygon", "ellipse", "arrow").
    pub fn name(self) -> &'static str {
        match self {
            ShapeKind::Line => "line",
            ShapeKind::Triangle => "triangle",
            ShapeKind::Rectangle => "rectangle",
            ShapeKind::Quad => "quad",
            ShapeKind::Polygon => "polygon",
            ShapeKind::Ellipse => "ellipse",
            ShapeKind::Arrow => "arrow",
        }
    }
}

/// An ellipse: centre, its first axis (unit), the radii along that axis and across it, and the
/// mean radial error of the points it was fitted to (0 = on the ellipse). Kotlin `Shapes.Fit`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ellipse {
    pub cx: f32,
    pub cy: f32,
    pub ux: f32,
    pub uy: f32,
    pub ra: f32,
    pub rb: f32,
    pub err: f32,
}

/// A recognised shape: its kind, the clean samples that replace the stroke (pressure: the
/// stroke's mean; time: evenly from the stroke's first to its last sample), and what it is made
/// of: the corners (a line's two ends) or the ellipse. Kotlin `Shapes.Shape`.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub kind: ShapeKind,
    pub points: Vec<InkPoint>,
    pub corners: Option<Vec<(f32, f32)>>,
    pub ellipse: Option<Ellipse>,
}

impl Shape {
    pub(crate) fn arrow(points: Vec<InkPoint>) -> Shape {
        Shape { kind: ShapeKind::Arrow, points, corners: None, ellipse: None }
    }
}

/// The clean samples of the shape the stroke is, or None (Kotlin `Shapes.recognize`).
pub fn recognize(points: &[InkPoint], still: f32, arrows: bool) -> Option<Vec<InkPoint>> {
    detect(points, still, arrows).map(|s| s.points)
}

/// The shape a stroke is, or None (Kotlin `Shapes.detect`). `still`: how far (sheet points) the
/// pen may stray and still count as resting, as the view decides when it held still (6 screen
/// px, see [`crate::snap::STILL_PX`]); 0 = the whole stroke counts. What the pen did while
/// resting is left out. `arrows`: an open stroke that is not straight may be an arrow drawn in
/// one stroke ([`arrows::from_stroke`]).
pub fn detect(points: &[InkPoint], still: f32, arrows: bool) -> Option<Shape> {
    let n0 = if still > 0.0 { rest_at(points, still) + 1 } else { points.len() };
    if n0 < 4 {
        return None;
    }
    let mut p = 0f32;
    for q in &points[..n0] {
        p += q.pressure;
    }
    p /= n0 as f32;
    let (t0, t1) = (points[0].t_ms as f32, points[n0 - 1].t_ms as f32);
    // Where the pen lands and where it rests at the end (that is when this runs), it trembles
    // on the spot: tiny back-and-forths that would read as sharp corners. They tell nothing
    // about the shape, so the path starts where the pen sets off and ends where it came to rest.
    let keep = steady(&points[..n0]);
    let n = keep.end() - keep.start() + 1;
    if n < 4 {
        return None;
    }
    let xs: Vec<f32> = points[keep.clone()].iter().map(|q| q.x).collect();
    let ys: Vec<f32> = points[keep.clone()].iter().map(|q| q.y).collect();
    let len = path_length(&xs, &ys);
    if len < 24.0 {
        return None;
    }
    let gap = hypot(xs[0] - xs[n - 1], ys[0] - ys[n - 1]);
    // Closed also when the end stops a bit short of the start (hands rarely meet exactly).
    let closed = gap < len * 0.2;
    if !closed {
        // Open: a line if every point is near the chord.
        let chord = hypot(xs[n - 1] - xs[0], ys[n - 1] - ys[0]);
        let straight = chord >= 24.0 && (1..n - 1).all(|i| seg_dist(xs[i], ys[i], xs[0], ys[0], xs[n - 1], ys[n - 1]) <= chord * 0.06 + 2.0);
        if !straight {
            // Not straight: an arrow (shaft and head in one stroke), or nothing.
            return if arrows { arrows::from_stroke(&points[keep], false).map(Shape::arrow) } else { None };
        }
        let ends = [xs[0], ys[0], xs[n - 1], ys[n - 1]];
        return Some(Shape { kind: ShapeKind::Line, points: poly(&ends, p, t0, t1, false), corners: Some(pairs(&ends)), ellipse: None });
    }
    // The path as drawn, without what ran past the start.
    let cut = overshoot_cut(&xs, &ys);
    let (tx, ty) = (&xs[..=cut], &ys[..=cut]);
    // Straight sides with corners win, unless it is almost perfectly round (a circle drawn
    // with a little wobble can look like it has corners; a rounded square can look round).
    let (ex, ey) = resample_open(tx, ty, 120);
    let fit = fit_ellipse(&ex, &ey);
    let corners = side_corners(tx, ty);
    if let Some(corners) = corners
        && !fit.is_some_and(|f| f.err < 0.045)
    {
        let (kind, c) = match corners.len() {
            3 => (ShapeKind::Triangle, flat(&corners)),
            4 => square_up(&corners),
            _ => (ShapeKind::Polygon, flat(&corners)),
        };
        return Some(Shape { kind, points: poly(&c, p, t0, t1, true), corners: Some(pairs(&c)), ellipse: None });
    }
    fit.filter(|f| f.err < 0.15).map(|f| Shape { kind: ShapeKind::Ellipse, points: ellipse_samples(&f, p, t0, t1), corners: None, ellipse: Some(f) })
}

/// The samples that say something about the shape: up to where the pen came to rest (`still`,
/// see [`detect`]), trembling trimmed. Kotlin `Shapes.trimmed`.
pub fn trimmed(points: &[InkPoint], still: f32) -> Option<RangeInclusive<usize>> {
    let n0 = if still > 0.0 { rest_at(points, still) + 1 } else { points.len() };
    if n0 < 2 {
        return None;
    }
    Some(steady(&points[..n0]))
}

/// The sample where the pen came to rest: the last one more than `still` from the rest before
/// it (as the view tracks it). Kotlin `Shapes.restAt`.
pub fn rest_at(points: &[InkPoint], still: f32) -> usize {
    let mut a = 0;
    for i in 1..points.len() {
        if hypot(points[i].x - points[a].x, points[i].y - points[a].y) > still {
            a = i;
        }
    }
    a
}

/// The samples from where the pen set off to where it came to rest (trembling on the spot trimmed).
fn steady(s: &[InkPoint]) -> RangeInclusive<usize> {
    let n = s.len();
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, -f32::MAX, -f32::MAX);
    for q in s {
        x0 = x0.min(q.x);
        x1 = x1.max(q.x);
        y0 = y0.min(q.y);
        y1 = y1.max(q.y);
    }
    let r = 1.5f32.max(hypot(x1 - x0, y1 - y0) * 0.03);
    let d = |i: usize, j: usize| hypot(s[i].x - s[j].x, s[i].y - s[j].y);
    let path = |i: usize, j: usize| (i..j).map(|k| d(k, k + 1) as f64).fold(0.0f64, |a, b| a + b) as f32;
    // Only where the pen wandered back and forth on the spot (its path much longer than the
    // way it got): a pen that sets off cleanly keeps its first and last points.
    let mut e = n - 1;
    while e > 0 && d(e - 1, n - 1) < r {
        e -= 1;
    }
    if path(e, n - 1) < 2.0 * d(e, n - 1) + 1.0 {
        e = n - 1;
    }
    let mut b = 0;
    while b < e && d(b + 1, 0) < r {
        b += 1;
    }
    if path(0, b) < 2.0 * d(0, b) + 1.0 {
        b = 0;
    }
    b..=e
}

/// Where a closed stroke that ran past its start should end: where it comes back closest to the start.
fn overshoot_cut(xs: &[f32], ys: &[f32]) -> usize {
    let n = xs.len();
    let mut end = n - 1;
    let mut best = hypot(xs[end] - xs[0], ys[end] - ys[0]);
    for j in (n as f32 * 0.75) as usize..n {
        let d = hypot(xs[j] - xs[0], ys[j] - ys[0]);
        if d < best {
            best = d;
            end = j;
        }
    }
    end
}

/// Removes the first `v` (Kotlin `MutableList<Int>.remove(element)`).
fn remove_value(v: &mut Vec<i32>, x: i32) {
    if let Some(i) = v.iter().position(|&c| c == x) {
        v.remove(i);
    }
}

/// Corners of a closed stroke drawn as straight sides, also with rounded corners, a hook, the
/// end short of the start or past it. Read round as a loop: corners where it turns sharply over
/// a window; the gap where it closes is no side, a corner inside it comes from the directions
/// at the start and the end. Corners that barely turn are bends; a straight line is fitted
/// through each side (nearly level or upright: exactly so) and the corners are where
/// neighbouring sides meet. None if the sides aren't straight.
fn side_corners(xs: &[f32], ys: &[f32]) -> Option<Vec<(f32, f32)>> {
    const M: i32 = 120;
    let m = M as usize;
    let k0 = xs.len();
    let seg: Vec<f32> = (0..k0)
        .map(|i| {
            let j = (i + 1) % k0;
            hypot(xs[j] - xs[i], ys[j] - ys[i])
        })
        .collect();
    let per = fsum(&seg).max(1e-3);
    let drawn = per - seg[k0 - 1]; // without the closing gap
    let (mut rx, mut ry) = (vec![0f32; m], vec![0f32; m]);
    {
        let (mut i, mut acc) = (0usize, 0f32);
        for q in 0..m {
            let d = per * q as f32 / M as f32;
            while i < k0 - 1 && acc + seg[i] < d {
                acc += seg[i];
                i += 1;
            }
            let f = if seg[i] > 0.0 { ((d - acc) / seg[i]).clamp(0.0, 1.0) } else { 0.0 };
            let j = (i + 1) % k0;
            rx[q] = xs[i] + (xs[j] - xs[i]) * f;
            ry[q] = ys[i] + (ys[j] - ys[i]) * f;
        }
    }
    // Samples on the closing gap (only when there is a real gap).
    let gap_from = if per - drawn > per * 0.03 { (drawn / per * M as f32).ceil() as i32 } else { M };
    let in_gap = |q: i32| q >= gap_from;
    let w = 5i32;
    let at = |q: i32| (((q % M) + M) % M) as usize;
    let has_gap = gap_from < M;
    let last = gap_from - 1; // the last sample drawn
    // How sharply it turns at each sample, over a window either side. With a gap, the window
    // stays on what was drawn (the gap's straight line would cut a corner there in two).
    let turn: Vec<f32> = (0..M)
        .map(|q| {
            let u = q as usize;
            if !has_gap {
                angle_between(rx[u] - rx[at(q - w)], ry[u] - ry[at(q - w)], rx[at(q + w)] - rx[u], ry[at(q + w)] - ry[u]).abs()
            } else if q > last {
                0.0
            } else {
                let (a, b) = ((q - w).max(0) as usize, last.min(q + w) as usize);
                if u - a < 2 || b - u < 2 { 0.0 } else { angle_between(rx[u] - rx[a], ry[u] - ry[a], rx[b] - rx[u], ry[b] - ry[u]).abs() }
            }
        })
        .collect();
    // Runs of sharp turning (outside the gap), one corner each.
    let sharp: Vec<bool> = (0..M).map(|q| !in_gap(q) && turn[q as usize] > 40.0).collect();
    let mut cand: Vec<i32> = Vec::new();
    let start = (0..M).find(|&q| !sharp[q as usize])?; // all sharp: none
    let mut i = 0;
    while i < M {
        let q = at(start + i);
        if sharp[q] {
            let mut best = q;
            while i < M && sharp[at(start + i)] {
                let r = at(start + i);
                if turn[r] > turn[best] {
                    best = r;
                }
                i += 1;
            }
            cand.push(best as i32);
        } else {
            i += 1;
        }
    }
    cand.sort();
    // How much each corner turns (a corner found in the gap or by splitting a side gets its own).
    let mut turns = turn.clone();
    // The gap: one corner in it if the way the path ends and the way it set off differ (each
    // read up to a corner drawn right there, if any).
    if has_gap {
        let first = cand.first().copied().filter(|&c| c < w).unwrap_or(w);
        let last_c = cand.last().copied().filter(|&c| c > last - w).unwrap_or(last - w);
        if first >= 2 && last_c >= 0 && last - last_c >= 2 {
            let (l, lc, f) = (last as usize, last_c as usize, first as usize);
            let across = angle_between(rx[l] - rx[lc], ry[l] - ry[lc], rx[f] - rx[0], ry[f] - ry[0]).abs();
            if across > 40.0 {
                let g = (gap_from + M) / 2;
                cand.push(g);
                turns[g as usize] = across;
            }
        }
        cand.sort();
    }
    // Corners closer than 8 % of the way are one rounded corner.
    let mut merged = true;
    while merged && cand.len() > 3 {
        merged = false;
        for c in 0..cand.len() {
            let (a, b) = (cand[c], cand[(c + 1) % cand.len()]);
            if (at(b - a) as f32) < M as f32 * 0.08 {
                if turns[a as usize] >= turns[b as usize] {
                    let k = (c + 1) % cand.len();
                    cand.remove(k);
                } else {
                    cand.remove(c);
                }
                merged = true;
                break;
            }
        }
    }
    // Corners that barely turn are bends in a side.
    while cand.len() > 3 {
        let weakest = min_by_first(cand.iter().copied(), |c| turns[c as usize])?;
        if turns[weakest as usize] >= 52.0 {
            break;
        }
        remove_value(&mut cand, weakest);
    }
    if cand.len() < 2 {
        return None;
    }
    // Samples of side `c` without `share` of its length at each end (where a corner rounds off).
    let side_points = |cand: &[i32], c: usize, share: f32| -> Vec<usize> {
        let (a, b) = (cand[c], cand[(c + 1) % cand.len()]);
        let span = at(b - a) as i32;
        let inset = ((span as f32 * share) as i32).max(2).min(span / 2 - 1).max(0);
        (inset..=span - inset).map(|k| at(a + k)).filter(|&q| !in_gap(q as i32)).collect()
    };
    let max = |v: &[f32]| v.iter().fold(-f32::MAX, |a, &b| a.max(b));
    let min = |v: &[f32]| v.iter().fold(f32::MAX, |a, &b| a.min(b));
    let diag = hypot(max(&rx) - min(&rx), max(&ry) - min(&ry));
    // The line of a side comes from its middle: the ends bend into rounded corners.
    let fit_sides = |cand: &[i32]| -> Option<Vec<[f32; 4]>> {
        let mut out = Vec::with_capacity(cand.len());
        for c in 0..cand.len() {
            // A side left out (the pen stopped short of it) runs along the gap.
            let mut pts = side_points(cand, c, 0.2);
            if pts.len() < 2 {
                pts = (0..=at(cand[(c + 1) % cand.len()] - cand[c]) as i32).map(|k| at(cand[c] + k)).collect();
            }
            if pts.len() < 2 {
                return None;
            }
            let px: Vec<f32> = pts.iter().map(|&q| rx[q]).collect();
            let py: Vec<f32> = pts.iter().map(|&q| ry[q]).collect();
            out.push(fit_line(&px, &py));
        }
        Some(out)
    };
    // Points of side `c` off its line (a few, from a hook or wobble, are fine; its ends may round off).
    let crooked = |cand: &[i32], c: usize, l: &[f32; 4]| -> bool {
        let pts = side_points(cand, c, 0.15);
        if pts.is_empty() {
            return false;
        }
        let off = pts.iter().filter(|&&q| ((rx[q] - l[0]) * l[3] - (ry[q] - l[1]) * l[2]).abs() > diag * 0.06).count();
        off as f32 > pts.len() as f32 * 0.12
    };
    // How much of the way round side `c` takes (0..1).
    let share = |cand: &[i32], c: usize| at(cand[(c + 1) % cand.len()] - cand[c]) as f32 / M as f32;
    // A side that isn't straight hides a corner drawn too round to notice: split it where it
    // lies farthest from the chord between its ends, and look again.
    let mut lines = fit_sides(&cand)?;
    for _ in 0..4 {
        let Some(bent) = (0..cand.len()).find(|&c| crooked(&cand, c, &lines[c])) else { break };
        if cand.len() >= 6 {
            break;
        }
        let (a, b) = (cand[bent] as usize, cand[(bent + 1) % cand.len()] as usize);
        let pts = side_points(&cand, bent, 0.15);
        let Some(q) = max_by_first(pts.iter().copied(), |q| seg_dist(rx[q], ry[q], rx[a], ry[a], rx[b], ry[b])) else { break };
        cand.push(q as i32);
        turns[q] = 90.0;
        cand.sort();
        lines = fit_sides(&cand)?;
    }
    // Two neighbouring sides running nearly the same way are one side (a bend, not a corner).
    while cand.len() > 3 {
        let k = cand.len();
        let Some(c) = (0..k).find(|&c| {
            let (l1, l2) = (&lines[(c + k - 1) % k], &lines[c]);
            (l1[2] * l2[3] - l1[3] * l2[2]).abs() < 0.45 && (l1[2] * l2[2] + l1[3] * l2[3]) > 0.0 // under ~27°, same direction
        }) else {
            break;
        };
        cand.remove(c);
        lines = fit_sides(&cand)?;
    }
    // A short side between two corners is a corner drawn round (cut off), not a side of its own:
    // it goes, with the weaker of its corners.
    while cand.len() > 4 {
        let k = min_by_first(0..cand.len(), |c| share(&cand, c))?;
        if share(&cand, k) >= 0.11 {
            break;
        }
        let (a, b) = (cand[k], cand[(k + 1) % cand.len()]);
        remove_value(&mut cand, if turns[a as usize] >= turns[b as usize] { b } else { a });
        lines = fit_sides(&cand)?;
    }
    if !(3..=6).contains(&cand.len()) {
        return None;
    }
    if (0..cand.len()).any(|c| crooked(&cand, c, &lines[c])) {
        return None;
    }
    let k = cand.len();
    let corners: Vec<(f32, f32)> =
        (0..k).map(|c| intersect(&lines[(c + k - 1) % k], &lines[c]).unwrap_or((rx[cand[c] as usize], ry[cand[c] as usize]))).collect();
    // Corners far outside what was drawn: two sides nearly parallel, no real shape.
    let (min_x, max_x) = (min(&rx) - diag * 0.25, max(&rx) + diag * 0.25);
    let (min_y, max_y) = (min(&ry) - diag * 0.25, max(&ry) + diag * 0.25);
    if corners.iter().any(|&(x, y)| x < min_x || x > max_x || y < min_y || y > max_y) {
        return None;
    }
    Some(corners)
}

/// `n` points evenly along the path as drawn (first and last point kept).
fn resample_open(xs: &[f32], ys: &[f32], n: usize) -> (Vec<f32>, Vec<f32>) {
    let k = xs.len();
    let seg: Vec<f32> = (0..k - 1).map(|i| hypot(xs[i + 1] - xs[i], ys[i + 1] - ys[i])).collect();
    let per = fsum(&seg).max(1e-3);
    let (mut ox, mut oy) = (vec![0f32; n], vec![0f32; n]);
    let (mut i, mut acc) = (0usize, 0f32);
    for q in 0..n {
        let d = per * q as f32 / (n - 1) as f32;
        while i < k - 2 && acc + seg[i] < d {
            acc += seg[i];
            i += 1;
        }
        let f = if seg[i] > 0.0 { ((d - acc) / seg[i]).clamp(0.0, 1.0) } else { 0.0 };
        ox[q] = xs[i] + (xs[i + 1] - xs[i]) * f;
        oy[q] = ys[i] + (ys[i + 1] - ys[i]) * f;
    }
    (ox, oy)
}

fn angle_between(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let mut d = ((atan2(by, bx) - atan2(ay, ax)) as f64).to_degrees() as f32;
    while d > 180.0 {
        d -= 360.0;
    }
    while d < -180.0 {
        d += 360.0;
    }
    d
}

/// A line through points: a point on it and its direction (unit); nearly level/upright (within
/// 6°): exactly so.
fn fit_line(px: &[f32], py: &[f32]) -> [f32; 4] {
    let (mx, my) = (avg(px.iter().copied()) as f32, avg(py.iter().copied()) as f32);
    let (mut sxx, mut syy, mut sxy) = (0f32, 0f32, 0f32);
    for i in 0..px.len() {
        let (dx, dy) = (px[i] - mx, py[i] - my);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    let mut ang = 0.5 * atan2(2.0 * sxy, sxx - syy);
    let deg = (ang as f64).to_degrees();
    let axis = round_half_up(deg / 90.0) * 90.0;
    if (deg - axis).abs() < 6.0 {
        ang = axis.to_radians() as f32;
    }
    [mx, my, cos(ang), sin(ang)]
}

/// Where two lines (point, direction) meet; None if nearly parallel (under ~10°).
fn intersect(a: &[f32; 4], b: &[f32; 4]) -> Option<(f32, f32)> {
    let den = a[2] * b[3] - a[3] * b[2];
    if den.abs() < 0.17 {
        return None;
    }
    let t = ((b[0] - a[0]) * b[3] - (b[1] - a[1]) * b[2]) / den;
    Some((a[0] + a[2] * t, a[1] + a[3] * t))
}

fn path_length(xs: &[f32], ys: &[f32]) -> f32 {
    let mut l = 0f32;
    for i in 1..xs.len() {
        l += hypot(xs[i] - xs[i - 1], ys[i] - ys[i - 1]);
    }
    l
}

/// The angle at `b` from `a` to `c` (degrees, -180..180).
fn angle(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    let (a1, a2) = (atan2(a.1 - b.1, a.0 - b.0), atan2(c.1 - b.1, c.0 - b.0));
    let mut d = ((a1 - a2) as f64).to_degrees() as f32;
    while d > 180.0 {
        d -= 360.0;
    }
    while d < -180.0 {
        d += 360.0;
    }
    d
}

fn flat(c: &[(f32, f32)]) -> Vec<f32> {
    c.iter().flat_map(|&(x, y)| [x, y]).collect()
}

fn pairs(c: &[f32]) -> Vec<(f32, f32)> {
    c.chunks_exact(2).map(|p| (p[0], p[1])).collect()
}

/// A four-cornered shape: a clean rectangle if the corners are within 22° of right angles, else
/// the quad as drawn.
fn square_up(c: &[(f32, f32)]) -> (ShapeKind, Vec<f32>) {
    let right = (0..4).all(|i| (angle(c[(i + 3) % 4], c[i], c[(i + 1) % 4]).abs() - 90.0).abs() < 22.0);
    if !right {
        return (ShapeKind::Quad, flat(c));
    }
    // Fit a rotated rectangle: centre, the way all four sides lie (a quarter turn is the
    // same way, so their angles are averaged four times over, weighted by length), half sides.
    let cx = c.iter().fold(0.0f64, |s, p| s + p.0 as f64) as f32 / 4.0;
    let cy = c.iter().fold(0.0f64, |s, p| s + p.1 as f64) as f32 / 4.0;
    let (mut sx, mut sy) = (0.0f64, 0.0f64);
    for i in 0..4 {
        let (dx, dy) = ((c[(i + 1) % 4].0 - c[i].0) as f64, (c[(i + 1) % 4].1 - c[i].1) as f64);
        let (a4, l) = (4.0 * dy.atan2(dx), dx.hypot(dy));
        sx += l * a4.cos();
        sy += l * a4.sin();
    }
    let mut ang = (sy.atan2(sx) / 4.0) as f32;
    // Drawn about level (within 10°): level.
    if (ang as f64).to_degrees().abs() < 10.0 {
        ang = 0.0;
    }
    let (ux, uy) = (cos(ang), sin(ang));
    let (mut hw, mut hh) = (0f32, 0f32);
    for &(x, y) in c {
        let (dx, dy) = (x - cx, y - cy);
        hw += (dx * ux + dy * uy).abs();
        hh += (-dx * uy + dy * ux).abs();
    }
    hw /= 4.0;
    hh /= 4.0;
    // Sides within 7 % of each other: a square (as a nearly round ellipse is a circle).
    if (hw - hh).abs() < 0.07 * hw.max(hh) {
        let h = (hw + hh) / 2.0;
        hw = h;
        hh = h;
    }
    let signs = [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    let mut out = vec![0f32; 8];
    for (i, (sa, sb)) in signs.into_iter().enumerate() {
        let (a, b) = (sa * hw, sb * hh);
        out[2 * i] = cx + a * ux - b * uy;
        out[2 * i + 1] = cy + a * uy + b * ux;
    }
    (ShapeKind::Rectangle, out)
}

/// A straight line from (x0, y0) to (x1, y1) as samples (dragging a snapped line's end).
/// Kotlin `Shapes.line`.
pub fn line(x0: f32, y0: f32, x1: f32, y1: f32, p: f32, t0: f32, t1: f32) -> Vec<InkPoint> {
    poly(&[x0, y0, x1, y1], p, t0, t1, false)
}

/// The smallest side a held shape can be pulled to (sheet points).
pub const MIN_SIDE: f32 = 3.0;
/// A held rectangle or ellipse is a square or circle while its sides differ by less than this share.
pub const SQUARE_SNAP: f32 = 0.04;

/// A closed shape that just snapped while the pen is still down: dragging the pen changes its
/// size. The corner of the shape's box nearest the pen follows the pen, the opposite corner
/// stays where it is, so pulling sideways changes only the width. A rectangle or ellipse keeps
/// its angle (its box lies along its own sides), and turns into a square or circle while its
/// sides nearly match ([`HeldShape::square`]); other shapes stretch in a level box.
/// Kotlin `Shapes.HeldShape`.
#[derive(Debug, Clone)]
pub struct HeldShape {
    kind: ShapeKind,
    ellipse: Option<Ellipse>,
    p: f32,
    t0: f32,
    // The frame the box lies in: an origin and its x axis (unit).
    ox: f32,
    oy: f32,
    ux: f32,
    uy: f32,
    /// The corners in the frame (x, y pairs; empty for an ellipse).
    local: Vec<f32>,
    /// The held corner and the one opposite it, in the frame.
    hx: f32,
    hy: f32,
    ax: f32,
    ay: f32,
    square: bool,
}

impl HeldShape {
    /// `shape` held with the pen at (pen_x, pen_y) (where it rested when the shape snapped).
    /// None for a shape without corners or ellipse (an arrow).
    pub fn new(shape: &Shape, pen_x: f32, pen_y: f32) -> Option<HeldShape> {
        let (p, t0) = (shape.points.first()?.pressure, shape.points[0].t_ms as f32);
        let (ox, oy, ux, uy, local, x0, x1, y0, y1);
        if let Some(e) = shape.ellipse {
            (ox, oy, ux, uy) = (e.cx, e.cy, e.ux, e.uy);
            local = Vec::new();
            (x0, x1, y0, y1) = (-e.ra, e.ra, -e.rb, e.rb);
        } else {
            let c = flat(shape.corners.as_deref()?);
            let m = c.len() / 2;
            if m == 0 {
                return None;
            }
            ox = (0..m).fold(0.0f64, |s, i| s + c[2 * i] as f64) as f32 / m as f32;
            oy = (0..m).fold(0.0f64, |s, i| s + c[2 * i + 1] as f64) as f32 / m as f32;
            if shape.kind == ShapeKind::Rectangle {
                let l = hypot(c[2] - c[0], c[3] - c[1]).max(1e-3);
                (ux, uy) = ((c[2] - c[0]) / l, (c[3] - c[1]) / l);
            } else {
                (ux, uy) = (1.0, 0.0);
            }
            let mut l = vec![0f32; c.len()];
            for i in 0..m {
                let (dx, dy) = (c[2 * i] - ox, c[2 * i + 1] - oy);
                l[2 * i] = dx * ux + dy * uy;
                l[2 * i + 1] = -dx * uy + dy * ux;
            }
            x0 = (0..m).map(|i| l[2 * i]).fold(f32::MAX, f32::min);
            x1 = (0..m).map(|i| l[2 * i]).fold(-f32::MAX, f32::max);
            y0 = (0..m).map(|i| l[2 * i + 1]).fold(f32::MAX, f32::min);
            y1 = (0..m).map(|i| l[2 * i + 1]).fold(-f32::MAX, f32::max);
            local = l;
        }
        let (dx, dy) = (pen_x - ox, pen_y - oy);
        let (px, py) = (dx * ux + dy * uy, -dx * uy + dy * ux);
        let (hx, ax) = if (px - x0).abs() < (px - x1).abs() { (x0, x1) } else { (x1, x0) };
        let (hy, ay) = if (py - y0).abs() < (py - y1).abs() { (y0, y1) } else { (y1, y0) };
        Some(HeldShape { kind: shape.kind, ellipse: shape.ellipse, p, t0, ox, oy, ux, uy, local, hx, hy, ax, ay, square: false })
    }

    /// Whether the last drag made it a square or a circle (the view ticks when it becomes one).
    pub fn square(&self) -> bool {
        self.square
    }

    /// The shape with the held corner moved by (dx, dy) sheet points from where it snapped;
    /// `t1`: its time (ms since the stroke began).
    pub fn drag(&mut self, dx: f32, dy: f32, t1: f32) -> Vec<InkPoint> {
        let (ux, uy) = (self.ux, self.uy);
        let (ldx, ldy) = (dx * ux + dy * uy, -dx * uy + dy * ux);
        // Sizes from the corner that stays (signed: dragging past it mirrors the shape).
        let keep = |v: f32, was: f32| {
            if v.abs() >= MIN_SIDE {
                v
            } else if v != 0.0 {
                MIN_SIDE.copysign(v)
            } else {
                MIN_SIDE.copysign(was)
            }
        };
        let (hx, hy, ax, ay) = (self.hx, self.hy, self.ax, self.ay);
        let mut w = keep(hx + ldx - ax, hx - ax);
        let mut h = keep(hy + ldy - ay, hy - ay);
        // Sides nearly alike: a square (a circle); the side being pulled more gives way.
        self.square = matches!(self.kind, ShapeKind::Rectangle | ShapeKind::Ellipse) && (w.abs() - h.abs()).abs() < SQUARE_SNAP * w.abs().max(h.abs());
        if self.square {
            if ldx.abs() >= ldy.abs() {
                w = h.abs().copysign(w);
            } else {
                h = w.abs().copysign(h);
            }
        }
        let page = |a: f32, b: f32| (self.ox + a * ux - b * uy, self.oy + a * uy + b * ux);
        if self.ellipse.is_some() {
            let c = page(ax + w / 2.0, ay + h / 2.0);
            return ellipse_samples(&Ellipse { cx: c.0, cy: c.1, ux, uy, ra: w.abs() / 2.0, rb: h.abs() / 2.0, err: 0.0 }, self.p, self.t0, t1);
        }
        let (kx, ky) = (w / (hx - ax), h / (hy - ay));
        let mut out = vec![0f32; self.local.len()];
        for i in 0..self.local.len() / 2 {
            let q = page(ax + (self.local[2 * i] - ax) * kx, ay + (self.local[2 * i + 1] - ay) * ky);
            out[2 * i] = q.0;
            out[2 * i + 1] = q.1;
        }
        poly(&out, self.p, self.t0, t1, true)
    }
}

/// An ellipse through the points (principal axes; radii from the mean extent, robust to
/// wobble). Radii within 12 % of each other: a circle (level); otherwise within 10° of level
/// or upright: exactly so. None if a radius is under 8 pt.
fn fit_ellipse(xs: &[f32], ys: &[f32]) -> Option<Ellipse> {
    let (cx, cy) = (avg(xs.iter().copied()) as f32, avg(ys.iter().copied()) as f32);
    let (mut sxx, mut syy, mut sxy) = (0f32, 0f32, 0f32);
    for i in 0..xs.len() {
        let (dx, dy) = (xs[i] - cx, ys[i] - cy);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    let ang = 0.5 * atan2(2.0 * sxy, sxx - syy);
    let (ux, uy) = (cos(ang), sin(ang));
    // For points evenly around an ellipse, mean(a²) = ra²/2.
    let (mut saa, mut sbb) = (0f32, 0f32);
    for i in 0..xs.len() {
        let (dx, dy) = (xs[i] - cx, ys[i] - cy);
        let (a, b) = (dx * ux + dy * uy, -dx * uy + dy * ux);
        saa += a * a;
        sbb += b * b;
    }
    let mut ra = (2.0 * saa / xs.len() as f32).sqrt();
    let mut rb = (2.0 * sbb / xs.len() as f32).sqrt();
    if ra < 8.0 || rb < 8.0 {
        return None;
    }
    let mut err = 0f32;
    for i in 0..xs.len() {
        let (dx, dy) = (xs[i] - cx, ys[i] - cy);
        let (a, b) = ((dx * ux + dy * uy) / ra, (-dx * uy + dy * ux) / rb);
        err += ((a * a + b * b).sqrt() - 1.0).abs();
    }
    let (mut fx, mut fy) = (ux, uy);
    if (ra - rb).abs() / ra.max(rb) < 0.12 {
        // A circle (level, for stretching).
        let r = (ra + rb) / 2.0;
        ra = r;
        rb = r;
        (fx, fy) = (1.0, 0.0);
    } else {
        // Drawn about level or upright (within 10°): exactly so, like a rectangle.
        let deg = (ang as f64).to_degrees();
        let axis = round_half_up(deg / 90.0) * 90.0;
        if (deg - axis).abs() < 10.0 {
            (fx, fy) = (axis.to_radians().cos() as f32, axis.to_radians().sin() as f32);
        }
    }
    Some(Ellipse { cx, cy, ux: fx, uy: fy, ra, rb, err: err / xs.len() as f32 })
}

fn ellipse_samples(f: &Ellipse, p: f32, t0: f32, t1: f32) -> Vec<InkPoint> {
    let steps = 72;
    (0..=steps)
        .map(|k| {
            let th = 2.0 * std::f32::consts::PI * k as f32 / steps as f32;
            let (a, b) = (f.ra * cos(th), f.rb * sin(th));
            InkPoint { x: f.cx + a * f.ux - b * f.uy, y: f.cy + a * f.uy + b * f.ux, pressure: p, t_ms: ms(t0 + (t1 - t0) * k as f32 / steps as f32) }
        })
        .collect()
}

/// Polyline through the corners `c` (x, y pairs; closed: back to the first). Samples lie close
/// together next to each corner: the renderer smooths a stroke (each point is pulled halfway
/// toward the next), which rounds a corner off by about the spacing there, so 6 pt apart would
/// cut every corner but the first visibly.
fn poly(c: &[f32], p: f32, t0: f32, t1: f32, close: bool) -> Vec<InkPoint> {
    let m = c.len() / 2;
    let segs = if close { m } else { m - 1 };
    let mut pts: Vec<(f32, f32)> = Vec::new();
    for i in 0..segs {
        let (ax, ay) = (c[2 * i], c[2 * i + 1]);
        let (bx, by) = (c[2 * ((i + 1) % m)], c[2 * ((i + 1) % m) + 1]);
        let len = hypot(bx - ax, by - ay);
        if len < 1e-3 {
            continue;
        }
        for d in side_stops(len) {
            pts.push((ax + (bx - ax) * d / len, ay + (by - ay) * d / len));
        }
    }
    pts.push(if close { (c[0], c[1]) } else { (c[2 * (m - 1)], c[2 * (m - 1) + 1]) });
    let n = pts.len();
    let span = (n - 1).max(1) as f32;
    pts.into_iter().enumerate().map(|(i, (x, y))| InkPoint { x, y, pressure: p, t_ms: ms(t0 + (t1 - t0) * i as f32 / span) }).collect()
}

/// Where samples go along a side `len` long, from 0 (the corner) up to but not including the far corner.
fn side_stops(len: f32) -> Vec<f32> {
    const NEAR: [f32; 6] = [0.0, 0.25, 0.5, 1.0, 2.0, 4.0];
    let mut out: Vec<f32> = NEAR.iter().copied().filter(|&d| d < len / 2.0).collect();
    // Every 6 pt from 9 on. A side far longer than any page (a pen position gone wrong, even
    // infinite) gets no stops in between: `d += 6.0` stops moving at 2^27 and looped forever.
    if len < 1e6 {
        let mut d = 9.0f32;
        while d < len - 9.0 {
            out.push(d);
            d += 6.0;
        }
    }
    for &d in NEAR.iter().rev() {
        if d > 0.0 && len - d > len / 2.0 {
            out.push(len - d);
        }
    }
    out
}

#[cfg(test)]
// The random shapes use the Kotlin tests' own constants (6.28 …), so they stay the same strokes.
#[allow(clippy::approx_constant)]
pub(crate) mod tests {
    //! The JVM tests of the Android app, ported case by case (ShapesTest.kt, ShapesFuzzTest.kt,
    //! ShapesIntentTest.kt). The strokes come from [`JavaRandom`] with the Kotlin seeds and the
    //! same float arithmetic, so they are the very strokes the Kotlin tests draw.

    use super::*;
    use crate::ink::{self, Brush};
    use std::f32::consts::PI;

    /// `java.util.Random`: the same numbers for the same seed as on the JVM.
    pub(crate) struct JavaRandom(u64);

    impl JavaRandom {
        const MUL: u64 = 0x5_DEEC_E66D;
        const MASK: u64 = (1 << 48) - 1;

        pub(crate) fn new(seed: i64) -> JavaRandom {
            JavaRandom((seed as u64 ^ Self::MUL) & Self::MASK)
        }
        fn next(&mut self, bits: u32) -> i32 {
            self.0 = self.0.wrapping_mul(Self::MUL).wrapping_add(0xB) & Self::MASK;
            (self.0 >> (48 - bits)) as i32
        }
        pub(crate) fn next_float(&mut self) -> f32 {
            self.next(24) as f32 / (1 << 24) as f32
        }
        pub(crate) fn next_boolean(&mut self) -> bool {
            self.next(1) != 0
        }
        pub(crate) fn next_int(&mut self, bound: i32) -> i32 {
            let mut r = self.next(31);
            let m = bound - 1;
            if bound & m == 0 {
                return ((bound as i64 * r as i64) >> 31) as i32;
            }
            let mut u = r;
            loop {
                r = u % bound;
                if u.wrapping_sub(r).wrapping_add(m) >= 0 {
                    return r;
                }
                u = self.next(31);
            }
        }
        /// `(nextFloat() - 0.5f) * 2 * a`
        pub(crate) fn jitter(&mut self, a: f32) -> f32 {
            (self.next_float() - 0.5) * 2.0 * a
        }
        /// `a + nextFloat() * (b - a)`
        pub(crate) fn r(&mut self, a: f32, b: f32) -> f32 {
            a + self.next_float() * (b - a)
        }
    }

    #[test]
    fn java_random_matches_the_jvm() {
        let mut r = JavaRandom::new(42);
        assert_eq!(r.next(32), -1170105035);
        assert_eq!(r.next(32), 234785527);
        let mut r = JavaRandom::new(42);
        assert_eq!(r.next_int(10), 0);
        assert_eq!(r.next_int(10), 3);
    }

    /// Samples as the Kotlin tests make them: pressure 0.5, `dt` ms apart.
    pub(crate) fn samples(pts: &[(f32, f32)], dt: u32) -> Vec<InkPoint> {
        pts.iter().enumerate().map(|(i, &(x, y))| InkPoint { x, y, pressure: 0.5, t_ms: i as u32 * dt }).collect()
    }

    fn kind(pts: &[(f32, f32)]) -> Option<ShapeKind> {
        detect(&samples(pts, 8), 0.0, true).map(|s| s.kind)
    }

    fn xs(o: &[InkPoint]) -> Vec<f32> {
        o.iter().map(|q| q.x).collect()
    }
    fn ys(o: &[InkPoint]) -> Vec<f32> {
        o.iter().map(|q| q.y).collect()
    }
    fn lo(v: &[f32]) -> f32 {
        v.iter().fold(f32::MAX, |a, &b| a.min(b))
    }
    fn hi(v: &[f32]) -> f32 {
        v.iter().fold(-f32::MAX, |a, &b| a.max(b))
    }
    fn close(a: f32, b: f32, tol: f32, what: &str) {
        assert!((a - b).abs() <= tol, "{what}: {a} vs {b} (± {tol})");
    }
    /// Averages of a window, as the Kotlin tests round corners.
    fn window_avg(w: &[(f32, f32)]) -> (f32, f32) {
        (avg(w.iter().map(|p| p.0)) as f32, avg(w.iter().map(|p| p.1)) as f32)
    }

    // --- ShapesTest.kt: hand-drawn-like strokes (overshoot, wobble, rounded corners, hooks) ---

    struct Hand {
        rnd: JavaRandom,
    }

    impl Hand {
        fn new() -> Hand {
            Hand { rnd: JavaRandom::new(7) }
        }

        /// An ellipse drawn by hand: start angle, sweep (> 2π overshoots), wobble.
        fn ellipse(&mut self, rx: f32, ry: f32, start: f32, sweep: f32, wobble: f32) -> Vec<(f32, f32)> {
            let n = 90;
            (0..n)
                .map(|i| {
                    let t = start + sweep * i as f32 / (n - 1) as f32;
                    let x = 200.0 + rx * cos(t) + self.rnd.jitter(wobble);
                    let y = 200.0 + ry * sin(t) + self.rnd.jitter(wobble);
                    (x, y)
                })
                .collect()
        }

        /// A polygon traced from `start` (fraction of the perimeter), `extra` of the perimeter past
        /// the end, rounded by `round`, in steps of 4.
        fn polygon(&mut self, corners: &[(f32, f32)], start: f32, extra: f32, wobble: f32, round: f32) -> Vec<(f32, f32)> {
            let step = 4.0f32;
            let k = corners.len();
            let segs: Vec<((f32, f32), (f32, f32))> = (0..k).map(|i| (corners[i], corners[(i + 1) % k])).collect();
            let lens: Vec<f32> = segs.iter().map(|(a, b)| hypot(b.0 - a.0, b.1 - a.1)).collect();
            let per = fsum(&lens);
            let at = |d0: f32| {
                let mut d = ((d0 % per) + per) % per;
                for (i, &l) in lens.iter().enumerate() {
                    if d <= l {
                        let (a, b) = segs[i];
                        let f = d / l;
                        return (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
                    }
                    d -= l;
                }
                corners[0]
            };
            let total = per * (1.0 + extra);
            let raw: Vec<(f32, f32)> = (0..=(total / step) as i32).map(|i| at(per * start + i as f32 * step)).collect();
            let k = (round / step) as i32;
            let smooth: Vec<(f32, f32)> = if k <= 0 {
                raw
            } else {
                let k = k as usize;
                (0..raw.len()).map(|i| window_avg(&raw[i.saturating_sub(k)..(i + k + 1).min(raw.len())])).collect()
            };
            smooth
                .into_iter()
                .map(|(x, y)| {
                    let x = x + self.rnd.jitter(wobble);
                    (x, y + self.rnd.jitter(wobble))
                })
                .collect()
        }

        /// A path through `pts` (corners as drawn), filled in every 3 units, with a little wobble.
        fn traced(&mut self, pts: &[(f32, f32)], scale: f32) -> Vec<(f32, f32)> {
            let mut out = Vec::new();
            for i in 0..pts.len() - 1 {
                let ((ax, ay), (bx, by)) = (pts[i], pts[i + 1]);
                let steps = 1.max((hypot(bx - ax, by - ay) * scale / 3.0) as i32);
                for k in 0..steps {
                    let x = (ax + (bx - ax) * k as f32 / steps as f32) * scale + self.rnd.jitter(0.8);
                    let y = (ay + (by - ay) * k as f32 / steps as f32) * scale + self.rnd.jitter(0.8);
                    out.push((x, y));
                }
            }
            let last = pts[pts.len() - 1];
            out.push((last.0 * scale, last.1 * scale));
            out
        }
    }

    const RECT: [(f32, f32); 4] = [(100.0, 100.0), (300.0, 100.0), (300.0, 220.0), (100.0, 220.0)];
    const TRI: [(f32, f32); 3] = [(100.0, 300.0), (220.0, 100.0), (340.0, 300.0)];

    fn rotated(c: &[(f32, f32)], deg: f32) -> Vec<(f32, f32)> {
        let a = deg * PI / 180.0;
        let (cx, cy) = window_avg(c);
        c.iter().map(|&(x, y)| (cx + (x - cx) * cos(a) - (y - cy) * sin(a), cy + (x - cx) * sin(a) + (y - cy) * cos(a))).collect()
    }

    #[test]
    fn circles() {
        let mut h = Hand::new();
        use ShapeKind::Ellipse;
        assert_eq!(kind(&h.ellipse(70.0, 70.0, 0.3, 2.0 * PI, 2.0)), Some(Ellipse), "exact");
        assert_eq!(kind(&h.ellipse(70.0, 70.0, 1.0, 2.2 * PI, 2.0)), Some(Ellipse), "overshoot 10 %");
        assert_eq!(kind(&h.ellipse(70.0, 70.0, 2.0, 1.88 * PI, 2.0)), Some(Ellipse), "short of closing 6 %");
        assert_eq!(kind(&h.ellipse(70.0, 64.0, 0.0, 2.05 * PI, 4.0)), Some(Ellipse), "wobbly");
        assert_eq!(kind(&h.ellipse(110.0, 45.0, 0.5, 2.1 * PI, 2.0)), Some(Ellipse), "flat ellipse");
    }

    #[test]
    fn rectangles() {
        let mut h = Hand::new();
        use ShapeKind::Rectangle;
        assert_eq!(kind(&h.polygon(&RECT, 0.0, 0.0, 1.5, 0.0)), Some(Rectangle), "from a corner");
        assert_eq!(kind(&h.polygon(&RECT, 0.12, 0.0, 1.5, 0.0)), Some(Rectangle), "from the middle of a side");
        assert_eq!(kind(&h.polygon(&RECT, 0.05, 0.12, 1.5, 0.0)), Some(Rectangle), "overshoot");
        assert_eq!(kind(&h.polygon(&RECT, 0.0, 0.05, 1.5, 12.0)), Some(Rectangle), "rounded corners");
        assert_eq!(kind(&h.polygon(&RECT, 0.0, 0.04, 4.0, 0.0)), Some(Rectangle), "wobbly");
        assert_eq!(kind(&h.polygon(&rotated(&RECT, 20.0), 0.3, 0.06, 1.5, 0.0)), Some(Rectangle), "tilted 20°");
    }

    #[test]
    fn triangles() {
        let mut h = Hand::new();
        use ShapeKind::Triangle;
        assert_eq!(kind(&h.polygon(&TRI, 0.0, 0.0, 1.5, 0.0)), Some(Triangle), "plain");
        assert_eq!(kind(&h.polygon(&TRI, 0.2, 0.12, 1.5, 0.0)), Some(Triangle), "overshoot, mid-side start");
        assert_eq!(kind(&h.polygon(&TRI, 0.0, 0.05, 3.0, 10.0)), Some(Triangle), "rounded, wobbly");
    }

    /// The shapes from a user's screenshot: a loose square with round corners, a triangle with a curl.
    #[test]
    fn loose_shapes_from_the_tablet() {
        let mut h = Hand::new();
        let square = h.traced(
            &[
                (75.0, 48.0),
                (78.0, 43.0),
                (86.0, 40.0),
                (120.0, 40.0),
                (193.0, 44.0),
                (190.0, 80.0),
                (186.0, 110.0),
                (180.0, 117.0),
                (172.0, 121.0),
                (150.0, 125.0),
                (120.0, 123.0),
                (85.0, 120.0),
                (78.0, 112.0),
                (74.0, 100.0),
                (72.0, 85.0),
                (73.0, 60.0),
                (75.0, 50.0),
            ],
            2.0,
        );
        assert_eq!(kind(&square), Some(ShapeKind::Rectangle), "loose square");
        let triangle = h.traced(
            &[
                (44.0, 318.0),
                (85.0, 280.0),
                (125.0, 243.0),
                (160.0, 280.0),
                (186.0, 305.0),
                (193.0, 315.0),
                (194.0, 322.0),
                (190.0, 328.0),
                (140.0, 328.0),
                (90.0, 327.0),
                (52.0, 325.0),
                (44.0, 320.0),
            ],
            2.0,
        );
        assert_eq!(kind(&triangle), Some(ShapeKind::Triangle), "triangle with a curl");
        // The result is clean: three corners joined by straight sides (no curl left).
        let clean = detect(&samples(&triangle, 8), 0.0, true).unwrap().points;
        close(hi(&ys(&clean)), 656.0, 8.0, "the base is level");
    }

    /// A user's rectangle traced from their screenshot (round corners, one side bowed, starting at
    /// the sharp corner top right), the pen trembling where it rests: a level rectangle where it was drawn.
    #[test]
    fn round_rectangle_from_the_tablet() {
        let drawn: Vec<(f32, f32)> = "176,29 178,35 178,42 177,48 177,54 177,60 176,65 177,70 177,75 178,81 178,87 176,92 178,98 178,103 178,108 177,114 178,120 174,124 170,128 165,131 160,133 154,134 149,136 144,136 139,137 134,137 130,137 126,138 121,137 117,137 113,137 107,138 101,138 97,138 92,139 88,140 84,139 78,139 74,139 68,140 62,138 54,139 46,138 42,134 38,130 36,125 34,119 33,113 33,107 31,102 32,96 30,91 30,84 31,77 28,70 30,65 30,59 30,53 30,46 34,42 40,38 45,36 52,34 58,33 65,33 70,31 74,30 80,31 84,30 88,30 92,30 96,30 100,31 104,28 110,30 116,28 120,30 123,30 127,30 132,29 136,29 142,29 146,30 153,29 160,29 168,29"
            .split(' ')
            .map(|p| {
                let (x, y) = p.split_once(',').unwrap();
                (x.parse().unwrap(), y.parse().unwrap())
            })
            .collect();
        let mut h = Hand::new();
        for scale in [1.0f32, 2.5] {
            for reverse in [false, true] {
                let path: Vec<(f32, f32)> = if reverse { std::iter::once(drawn[0]).chain(drawn[1..].iter().rev().copied()).collect() } else { drawn.clone() };
                let path: Vec<(f32, f32)> = path.into_iter().map(|(x, y)| (x * scale, y * scale)).collect();
                let mut pts = h.traced(&path, 1.0);
                let (ex, ey) = pts[pts.len() - 1];
                let (mut jx, mut jy) = (0f32, 0f32);
                for _ in 0..80 {
                    jx = (jx + h.rnd.jitter(0.7)).clamp(-2.0, 2.0);
                    jy = (jy + h.rnd.jitter(0.7)).clamp(-2.0, 2.0);
                    pts.push((ex + jx, ey + jy));
                }
                let s = detect(&samples(&pts, 8), 2.4, true).unwrap();
                assert_eq!(s.kind, ShapeKind::Rectangle, "scale {scale} reverse {reverse}");
                let xs: Vec<f32> = s.points.iter().map(|q| q.x / scale).collect();
                let ys: Vec<f32> = s.points.iter().map(|q| q.y / scale).collect();
                close(lo(&xs), 31.0, 3.0, "left");
                close(hi(&xs), 177.0, 3.0, "right");
                close(lo(&ys), 30.0, 3.0, "top");
                close(hi(&ys), 138.0, 3.0, "bottom");
                // Level: every point lies on one of the four sides.
                let (x0, x1, y0, y1) = (lo(&xs), hi(&xs), lo(&ys), hi(&ys));
                assert!((0..xs.len()).all(|i| (xs[i] - x0).abs().min((xs[i] - x1).abs()).min((ys[i] - y0).abs()).min((ys[i] - y1).abs()) < 0.01));
            }
        }
    }

    #[test]
    fn nearly_square_is_square() {
        let mut h = Hand::new();
        let sq = h.polygon(&[(100.0, 100.0), (300.0, 100.0), (300.0, 290.0), (100.0, 290.0)], 0.0, 0.03, 1.0, 0.0);
        let res = detect(&samples(&sq, 8), 0.0, true).unwrap().points;
        let (xs, ys) = (xs(&res), ys(&res));
        close(hi(&xs) - lo(&xs), hi(&ys) - lo(&ys), 0.5, "square");
    }

    /// Drawn by the core (which smooths every stroke), a clean rectangle keeps all four corners sharp, not just the first.
    #[test]
    fn corners_stay_sharp_on_the_page() {
        let mut h = Hand::new();
        let drawn = h.polygon(&RECT, 0.3, 0.03, 1.5, 0.0);
        let res = detect(&samples(&drawn, 8), 0.0, true).unwrap().points;
        let (xs, ys) = (xs(&res), ys(&res));
        let outline = ink::outline(&res, &Brush { width: 2.0, thinning: 0.0, color: 0xFF00_0000, kind: 0 });
        for cx in [lo(&xs), hi(&xs)] {
            for cy in [lo(&ys), hi(&ys)] {
                let near = outline.chunks_exact(2).map(|p| hypot(p[0] - cx, p[1] - cy)).fold(f32::MAX, f32::min);
                assert!(near < 1.8, "corner ({cx}, {cy}) rounded off by {near}");
            }
        }
    }

    /// Left, top, right, bottom of a result.
    fn bbox(o: &[InkPoint]) -> [f32; 4] {
        let (xs, ys) = (xs(o), ys(o));
        [lo(&xs), lo(&ys), hi(&xs), hi(&ys)]
    }

    fn close_all(a: &[f32], b: &[f32], tol: f32, what: &str) {
        assert!(a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tol), "{what}: {a:?} vs {b:?} (± {tol})");
    }

    /// A snapped shape held under the pen: the corner by the pen follows it, the opposite corner stays.
    #[test]
    fn held_shapes_change_size() {
        let mut h = Hand::new();
        // A level rectangle 100..300 × 100..220, the pen resting at its top-left corner.
        let drawn = h.polygon(&RECT, 0.0, 0.02, 1.5, 0.0);
        let mut held = HeldShape::new(&detect(&samples(&drawn, 8), 0.0, true).unwrap(), 100.0, 100.0).unwrap();
        close_all(&bbox(&held.drag(-20.0, -10.0, 0.0)), &[80.0, 90.0, 300.0, 220.0], 1.5, "grown");
        // Only sideways: only the width changes.
        close_all(&bbox(&held.drag(50.0, 0.0, 0.0)), &[150.0, 100.0, 300.0, 220.0], 1.5, "narrower");
        assert!(!held.square());
        // Sides nearly alike: a square (the side being pulled gives way), flagged for the tick.
        let b = bbox(&held.drag(76.0, 0.0, 0.0));
        assert!(held.square());
        close(b[2] - b[0], b[3] - b[1], 0.01, "square");
        close_all(&[b[2], b[3]], &[300.0, 220.0], 1.5, "the far corner stays");

        // A circle (radius 70 round 200, 200), the pen at its start lower right: pulled right, an ellipse.
        let circle = h.ellipse(70.0, 70.0, 0.3, 2.0 * PI, 2.0);
        let c = detect(&samples(&circle, 8), 0.0, true).unwrap();
        assert_eq!(c.kind, ShapeKind::Ellipse);
        let (px, py) = circle[circle.len() - 1];
        close_all(&bbox(&HeldShape::new(&c, px, py).unwrap().drag(30.0, 0.0, 0.0)), &[130.0, 130.0, 300.0, 270.0], 4.0, "ellipse");

        // A rectangle drawn at 20° keeps its angle while it grows.
        let tilted = h.polygon(&rotated(&RECT, 20.0), 0.3, 0.06, 1.5, 0.0);
        let t = detect(&samples(&tilted, 8), 0.0, true).unwrap();
        assert_eq!(t.kind, ShapeKind::Rectangle);
        let k = t.corners.clone().unwrap();
        let out = HeldShape::new(&t, k[0].0, k[0].1).unwrap().drag(-15.0, 10.0, 0.0);
        let before = ((k[1].1 - k[0].1) as f64).atan2((k[1].0 - k[0].0) as f64).to_degrees();
        let after = ((out[1].y - out[0].y) as f64).atan2((out[1].x - out[0].x) as f64).to_degrees();
        assert!((before - after).abs() <= 0.5, "{before}° vs {after}°");
    }

    #[test]
    fn lines() {
        let mut h = Hand::new();
        let straight: Vec<(f32, f32)> = (0..=60)
            .map(|i| {
                let x = 100.0 + i as f32 * 4.0 + h.rnd.jitter(1.5);
                (x, 200.0 + i as f32 * 1.2 + h.rnd.jitter(1.5))
            })
            .collect();
        assert_eq!(kind(&straight), Some(ShapeKind::Line), "straight");
        let sag: Vec<(f32, f32)> = (0..=60)
            .map(|i| {
                let f = i as f32 / 60.0;
                (100.0 + i as f32 * 4.0, 200.0 + 10.0 * sin(PI * f) + h.rnd.jitter(1.0))
            })
            .collect();
        assert_eq!(kind(&sag), Some(ShapeKind::Line), "slight sag");
        let mut hook: Vec<(f32, f32)> = (0..=60).map(|i| (100.0 + i as f32 * 4.0, 200.0 + h.rnd.jitter(1.0))).collect();
        hook.extend((1..=4).map(|i| (340.0 + i as f32 * 2.0, 200.0 + i as f32 * 4.0)));
        assert_eq!(kind(&hook), Some(ShapeKind::Line), "hook at the end");
    }

    #[test]
    fn scribbles_stay_as_they_are() {
        let wave: Vec<(f32, f32)> = (0..=120).map(|i| (100.0 + i as f32 * 3.0, 200.0 + 30.0 * sin(i as f32 / 6.0))).collect();
        assert_eq!(kind(&wave), None);
        let spiral: Vec<(f32, f32)> = (0..=150)
            .map(|i| {
                let t = i as f32 / 10.0;
                (200.0 + t * 6.0 * cos(t), 200.0 + t * 6.0 * sin(t))
            })
            .collect();
        assert_eq!(kind(&spiral), None);
    }

    #[test]
    fn helpers_follow_kotlin() {
        // Kotlin's minBy / maxBy keep the first of equals (Rust's max_by keeps the last).
        assert_eq!(min_by_first([3, 1, 2, 1], |v| v), Some(1));
        assert_eq!(max_by_first([(0, 5), (1, 7), (2, 7)], |v| v.1), Some((1, 7)));
        // Math.round: halves up.
        assert_eq!((round_half_up(-2.5), round_half_up(2.5), round_half_up(-2.6)), (-2.0, 3.0, -3.0));
        // A line snapped and dragged keeps its samples' time, ms truncated.
        let l = line(0.0, 0.0, 30.0, 0.0, 0.4, 0.0, 99.9);
        assert_eq!((l[0].t_ms, l[l.len() - 1].t_ms, l[0].pressure), (0, 99, 0.4));
        assert!(l.windows(2).all(|w| w[1].x > w[0].x));
    }

    // --- ShapesFuzzTest.kt --------------------------------------------------------------------

    /// Many shapes drawn the way a hand draws them: corners moved a little, rounded by a random
    /// radius, uneven speed, a slow wobble, a hook where the pen lands, the end short of the start
    /// or past it, and the pen resting at the end (that is when recognition runs).
    struct Fuzz {
        rnd: JavaRandom,
    }

    type Outline = Box<dyn Fn(f32) -> (f32, f32)>;

    impl Fuzz {
        /// A closed outline as a function of the way round (0..1).
        fn hand(&mut self, outline: Outline, size: f32, round: f32) -> Vec<(f32, f32)> {
            let start = self.rnd.r(0.0, 1.0);
            let end = self.rnd.r(-0.12, 0.18); // short of closing … past the start
            let total = 1.0 + end;
            let mut out = Vec::new();
            // Uneven speed: steps from 1 % to 3 % of the size.
            let mut f = 0f32;
            let (ph1, ph2) = (self.rnd.r(0.0, 6.0), self.rnd.r(0.0, 6.0));
            let amp = self.rnd.r(0.3, 1.4) * size / 100.0;
            while f < total {
                let (x, y) = outline((start + f) % 1.0);
                out.push((x + amp * sin(f * 9.0 + ph1), y + amp * cos(f * 7.0 + ph2)));
                f += self.rnd.r(0.004, 0.012);
            }
            // Rounded corners: average over a window of the path.
            let k = (round * out.len() as f32) as usize;
            let mut res: Vec<(f32, f32)> = if k < 1 { out } else { (0..out.len()).map(|i| window_avg(&out[i.saturating_sub(k)..(i + k + 1).min(out.len())])).collect() };
            // A hook where the pen landed.
            if self.rnd.next_float() < 0.4 {
                let (x0, y0) = res[0];
                let a = self.rnd.r(0.0, 6.28);
                let l = self.rnd.r(2.0, 6.0);
                res.insert(0, (x0 + l * cos(a), y0 + l * sin(a)));
            }
            // The pen resting at the end: a small cluster.
            let (xe, ye) = res[res.len() - 1];
            for _ in 0..15 {
                let x = xe + self.rnd.r(-0.6, 0.6);
                res.push((x, ye + self.rnd.r(-0.6, 0.6)));
            }
            res
        }

        fn place(&mut self, c: &[(f32, f32)], size: f32, rot: f32, jit: f32) -> Vec<(f32, f32)> {
            c.iter()
                .map(|&(x, y)| {
                    let (xr, yr) = (x * cos(rot) - y * sin(rot), x * sin(rot) + y * cos(rot));
                    let px = 300.0 + xr * size + self.rnd.r(-jit, jit) * size;
                    (px, 300.0 + yr * size + self.rnd.r(-jit, jit) * size)
                })
                .collect()
        }

        /// A placed polygon, rotated by up to `rot`, corners moved by up to `jit`, rounded by up to `round`.
        fn polygon(&mut self, c: &[(f32, f32)], size: f32, rot: f32, jit: f32, round: f32) -> Vec<(f32, f32)> {
            let rot = self.rnd.r(-rot, rot);
            let placed = self.place(c, size, rot, jit);
            let round = self.rnd.r(0.0, round);
            self.hand(polygon_outline(placed), size, round)
        }
    }

    fn polygon_outline(c: Vec<(f32, f32)>) -> Outline {
        let k = c.len();
        let lens: Vec<f32> = (0..k).map(|i| hypot(c[(i + 1) % k].0 - c[i].0, c[(i + 1) % k].1 - c[i].1)).collect();
        let per = fsum(&lens);
        Box::new(move |t| {
            let (mut d, mut i) = (t * per, 0);
            while i < k - 1 && d > lens[i] {
                d -= lens[i];
                i += 1;
            }
            let (a, b) = (c[i], c[(i + 1) % k]);
            let q = (d / lens[i]).clamp(0.0, 1.0);
            (a.0 + (b.0 - a.0) * q, a.1 + (b.1 - a.1) * q)
        })
    }

    type Tally = std::collections::BTreeMap<String, usize>;

    fn tally(v: &[(&str, usize)]) -> Tally {
        v.iter().map(|&(k, n)| (k.to_string(), n)).collect()
    }

    /// The share of `n` strokes from `make` recognised as `expect` (the pen resting at the end),
    /// and what they all came out as.
    fn rate<G>(g: &mut G, name: &str, expect: ShapeKind, n: usize, make: impl Fn(&mut G) -> Vec<(f32, f32)>) -> (f32, Tally) {
        let mut ok = 0;
        let mut got = Tally::new();
        for _ in 0..n {
            let pts = make(g);
            let k = detect(&samples(&pts, 8), 2.4, true).map(|s| s.kind);
            *got.entry(k.map_or("none", |k| k.name()).to_string()).or_default() += 1;
            if k == Some(expect) {
                ok += 1;
            }
        }
        let share = ok as f32 / n as f32;
        println!("{name}: {} %  {got:?}", (share * 100.0) as i32);
        (share, got)
    }

    /// Each kind must mostly come out as itself, and squares must not turn into circles (nor
    /// circles into squares). The same strokes give exactly what the Kotlin test gives on the
    /// JVM (ShapesFuzzTest.kt prints "square: 100 % {rectangle=200}", "rectangle: 99 %
    /// {rectangle=199, triangle=1}", "triangle: 100 % {triangle=200}", "circle: 99 % {quad=1,
    /// ellipse=198, rectangle=1}"): the port decides like the tablet, case by case.
    #[test]
    fn hand_drawn_shapes() {
        let mut f = Fuzz { rnd: JavaRandom::new(42) };
        let (square, square_got) = rate(&mut f, "square", ShapeKind::Rectangle, 200, |f| {
            let s = f.rnd.r(50.0, 220.0);
            f.polygon(&[(-0.5, -0.5), (0.5, -0.5), (0.5, 0.5), (-0.5, 0.5)], s, 0.08, 0.05, 0.035)
        });
        let (rect, rect_got) = rate(&mut f, "rectangle", ShapeKind::Rectangle, 200, |f| {
            let s = f.rnd.r(50.0, 200.0);
            let ar = f.rnd.r(0.4, 0.75);
            f.polygon(&[(-0.5, -ar / 2.0), (0.5, -ar / 2.0), (0.5, ar / 2.0), (-0.5, ar / 2.0)], s, 0.6, 0.04, 0.03)
        });
        let (tri, tri_got) = rate(&mut f, "triangle", ShapeKind::Triangle, 200, |f| {
            let s = f.rnd.r(50.0, 220.0);
            f.polygon(&[(0.0, -0.55), (0.55, 0.4), (-0.55, 0.4)], s, 0.4, 0.07, 0.03)
        });
        let (circle, circle_got) = rate(&mut f, "circle", ShapeKind::Ellipse, 200, |f| {
            let s = f.rnd.r(50.0, 220.0);
            let e = f.rnd.r(0.75, 1.0);
            f.hand(Box::new(move |t| (300.0 + s / 2.0 * cos(t * 2.0 * PI), 300.0 + s / 2.0 * e * sin(t * 2.0 * PI))), s, 0.0)
        });
        assert!(square >= 0.97, "squares {square}");
        assert!(rect >= 0.97, "rectangles {rect}");
        assert!(tri >= 0.97, "triangles {tri}");
        assert!(circle >= 0.97, "circles {circle}");
        // As on the JVM.
        assert_eq!(square_got, tally(&[("rectangle", 200)]));
        assert_eq!(rect_got, tally(&[("rectangle", 199), ("triangle", 1)]));
        assert_eq!(tri_got, tally(&[("triangle", 200)]));
        assert_eq!(circle_got, tally(&[("ellipse", 198), ("quad", 1), ("rectangle", 1)]));
    }

    // --- ShapesIntentTest.kt ------------------------------------------------------------------

    /// A sloppy rectangle: its samples, the corners it was drawn round, its tilt, its size.
    struct Drawn {
        pts: Vec<(f32, f32)>,
        corners: Vec<(f32, f32)>,
        tilt: f32,
        size: f32,
    }

    /// A rectangle w × h at `tilt` (radians) drawn sloppily: very round corners, bowed sides, a
    /// hook, the pen trembling while it rests.
    fn sloppy(rnd: &mut JavaRandom, w: f32, h: f32, tilt: f32, max_round: f32, bow: f32) -> Drawn {
        let base = [(-w / 2.0, -h / 2.0), (w / 2.0, -h / 2.0), (w / 2.0, h / 2.0), (-w / 2.0, h / 2.0)];
        let c: Vec<(f32, f32)> = base
            .iter()
            .map(|&(x, y)| {
                let jx = x + rnd.r(-0.025, 0.025) * w;
                let jy = y + rnd.r(-0.025, 0.025) * w;
                (300.0 + jx * cos(tilt) - jy * sin(tilt), 300.0 + jx * sin(tilt) + jy * cos(tilt))
            })
            .collect();
        // The outline: bowed sides, corners rounded by a quadratic curve cut back d on both sides.
        let mut outline: Vec<(f32, f32)> = Vec::new();
        let cut: Vec<f32> = (0..4).map(|_| rnd.r(0.0, max_round) * w.min(h)).collect();
        for i in 0..4 {
            let (a, b) = (c[i], c[(i + 1) % 4]);
            let len = hypot(b.0 - a.0, b.1 - a.1);
            let (ux, uy) = ((b.0 - a.0) / len, (b.1 - a.1) / len);
            let sag = rnd.r(-bow, bow) * len;
            let (d0, d1) = (cut[i], cut[(i + 1) % 4]);
            // The side from a + d0 to b - d1, bowed.
            let steps = 60;
            for k in 0..=steps {
                let u = k as f32 / steps as f32;
                let dist = d0 + (len - d0 - d1) * u;
                let off = sag * sin(PI * dist / len);
                outline.push((a.0 + ux * dist - uy * off, a.1 + uy * dist + ux * off));
            }
            // The corner at b, rounded.
            let n = c[(i + 2) % 4];
            let l2 = hypot(n.0 - b.0, n.1 - b.1);
            let (vx, vy) = ((n.0 - b.0) / l2, (n.1 - b.1) / l2);
            let (p0, p2) = ((b.0 - ux * d1, b.1 - uy * d1), (b.0 + vx * d1, b.1 + vy * d1));
            for k in 1..12 {
                let t = k as f32 / 12.0;
                let x = (1.0 - t) * (1.0 - t) * p0.0 + 2.0 * (1.0 - t) * t * b.0 + t * t * p2.0;
                let y = (1.0 - t) * (1.0 - t) * p0.1 + 2.0 * (1.0 - t) * t * b.1 + t * t * p2.1;
                outline.push((x, y));
            }
        }
        // Walk it by length from a random start, uneven speed, short of or past the start.
        let m = outline.len();
        let seg: Vec<f32> = (0..m).map(|i| hypot(outline[(i + 1) % m].0 - outline[i].0, outline[(i + 1) % m].1 - outline[i].1)).collect();
        let per = fsum(&seg);
        let at = |d0: f32| {
            let mut d = ((d0 % per) + per) % per;
            for i in 0..m {
                if d <= seg[i] {
                    let (a, b) = (outline[i], outline[(i + 1) % m]);
                    let f = d / seg[i].max(1e-3);
                    return (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f);
                }
                d -= seg[i];
            }
            outline[0]
        };
        let start = rnd.r(0.0, per);
        let end = per * (1.0 + rnd.r(-0.08, 0.12));
        let mut pts = Vec::new();
        let size = w.max(h);
        // Landing: a few trembles and maybe a hook.
        let s0 = at(start);
        for _ in 0..rnd.next_int(20) {
            let x = s0.0 + rnd.r(-1.0, 1.0);
            pts.push((x, s0.1 + rnd.r(-1.0, 1.0)));
        }
        if rnd.next_float() < 0.4 {
            let a = rnd.r(0.0, 6.28);
            let l = rnd.r(2.0, 6.0);
            pts.push((s0.0 + l * cos(a), s0.1 + l * sin(a)));
        }
        let mut d = 0f32;
        let wob = rnd.r(0.2, 1.2) * size / 100.0;
        let ph = rnd.r(0.0, 6.0);
        while d < end {
            let (x, y) = at(start + d);
            pts.push((x + wob * sin(d / 30.0 + ph), y + wob * cos(d / 23.0 + ph)));
            d += rnd.r(0.6, 3.5);
        }
        // Resting: the pen trembles about the end (a slow random walk).
        let e = pts[pts.len() - 1];
        let (mut jx, mut jy) = (0f32, 0f32);
        let amp = rnd.r(0.5, 2.5);
        for _ in 0..30 + rnd.next_int(100) {
            jx = (jx + rnd.r(-0.7, 0.7)).clamp(-amp, amp);
            jy = (jy + rnd.r(-0.7, 0.7)).clamp(-amp, amp);
            pts.push((e.0 + jx, e.1 + jy));
        }
        Drawn { pts, corners: c, tilt, size }
    }

    /// The corners of a result (where its direction turns).
    fn corners_of(o: &[InkPoint]) -> Vec<(f32, f32)> {
        let m = o.len();
        (0..m - 1)
            .filter(|&i| {
                let p = if i == 0 { m - 2 } else { i - 1 };
                let (ax, ay) = (o[i].x - o[p].x, o[i].y - o[p].y);
                let (bx, by) = (o[i + 1].x - o[i].x, o[i + 1].y - o[i].y);
                (ax * by - ay * bx).abs() > 0.2 * hypot(ax, ay) * hypot(bx, by)
            })
            .map(|i| (o[i].x, o[i].y))
            .collect()
    }

    /// The share of `n` sloppy rectangles that come out as rectangles, level (`level`) or at
    /// the drawn angle within 6°, each corner within 9 % of the size of a drawn one.
    fn rate_intent(rnd: &mut JavaRandom, name: &str, n: usize, level: bool, make: impl Fn(&mut JavaRandom) -> Drawn) -> (f32, Tally) {
        let mut ok = 0;
        let mut why = Tally::new();
        for _ in 0..n {
            let dr = make(rnd);
            let res = detect(&samples(&dr.pts, 4), 2.4, true);
            let bad = match res {
                None => Some("none".to_string()),
                Some(r) if r.kind != ShapeKind::Rectangle => Some(r.kind.name().to_string()),
                Some(r) => {
                    let cs = corners_of(&r.points);
                    if cs.len() != 4 {
                        Some(format!("corners {}", cs.len()))
                    } else {
                        let ang = atan2(cs[1].1 - cs[0].1, cs[1].0 - cs[0].0);
                        let off = ((((ang - if level { 0.0 } else { dr.tilt }) as f64).to_degrees() % 90.0) + 135.0) % 90.0 - 45.0;
                        let far = dr.corners.iter().map(|t| cs.iter().map(|c| hypot(c.0 - t.0, c.1 - t.1)).fold(f32::MAX, f32::min)).fold(-f32::MAX, f32::max) / dr.size;
                        if level && off.abs() > 0.01 {
                            Some("not level".into())
                        } else if !level && off.abs() > 6.0 {
                            Some("angle off".into())
                        } else if far > 0.09 {
                            Some("corner off".into())
                        } else {
                            None
                        }
                    }
                }
            };
            match bad {
                None => ok += 1,
                Some(b) => *why.entry(b).or_default() += 1,
            }
        }
        println!("{name}: {} %  {why:?}", ok * 100 / n);
        (ok as f32 / n as f32, why)
    }

    /// What a hand means, not just how many corners it drew: a sloppy rectangle must come out as
    /// a rectangle that is level when it was drawn about level, at the angle it was drawn
    /// otherwise, with its corners where the drawn ones were. Exactly as on the JVM
    /// (ShapesIntentTest.kt prints "level, round corners: 97 % {corner off=3, polygon=2,
    /// ellipse=1, triangle=2}", "tilted 15–35°: 99 % {polygon=1}").
    #[test]
    fn sloppy_rectangles_come_out_clean() {
        let mut rnd = JavaRandom::new(11);
        let (level, level_why) = rate_intent(&mut rnd, "level, round corners", 300, true, |rnd| {
            let w = rnd.r(60.0, 220.0);
            let h = w * rnd.r(0.45, 1.0);
            let tilt = (rnd.r(-5.0, 5.0) as f64).to_radians() as f32;
            sloppy(rnd, w, h, tilt, 0.22, 0.03)
        });
        let (tilted, tilted_why) = rate_intent(&mut rnd, "tilted 15–35°", 200, false, |rnd| {
            let w = rnd.r(60.0, 220.0);
            let s = if rnd.next_boolean() { 1.0 } else { -1.0 };
            let h = w * rnd.r(0.45, 1.0);
            let tilt = s * (rnd.r(15.0, 35.0) as f64).to_radians() as f32;
            sloppy(rnd, w, h, tilt, 0.15, 0.025)
        });
        assert!(level >= 0.95, "level {level}");
        assert!(tilted >= 0.9, "tilted {tilted}");
        // As on the JVM.
        assert_eq!(level_why, tally(&[("corner off", 3), ("ellipse", 1), ("polygon", 2), ("triangle", 2)]));
        assert_eq!(tilted_why, tally(&[("polygon", 1)]));
    }
}
