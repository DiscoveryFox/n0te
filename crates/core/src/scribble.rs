//! Scribbling out: a quick back-and-forth over ink deletes what it covers, and only that. One
//! implementation for the Android app (through the FFI, with the stroke outlines it keeps) and
//! n0te Tablet (with the strokes' samples).
//!
//! Two questions, in this order:
//!
//! 1. [`motion`]: does the gesture *move* like a scribble? Back and forth or round and round,
//!    much longer than the area it spans, done quickly. Turns are counted along the gesture's
//!    main axes with some slack, so the round turns of a quick little Kringel over one letter
//!    count as much as the sharp turns of a big zigzag.
//! 2. [`judge`]: what does it *cover*? The covered area is the gesture's path, widened just
//!    enough to close the gaps between its passes. Ink counts as covered only where it lies
//!    well *inside* that area. Colouring in and hatching stay on one side of an outline and
//!    only reach it, so the outline is never inside: nothing is covered and the gesture stays
//!    ink. A letter under a scribble is inside, so it goes.
//!
//! No target, no scribble: a gesture that covers nothing is kept as ordinary ink. And a big
//! area coloured over a little ink inside it is colouring, not scribbling out.

use crate::ink::{InkPoint, Rect};

/// Something on the page a scribble may cover.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub id: String,
    /// Points along it, at most [`STEP`] apart.
    pub pts: Vec<(f32, f32)>,
    /// Ink width around `pts` (0 for an outline: its points already lie at the ink's edge).
    pub width: f32,
    /// A box (a picture, a graph): only ever deleted whole.
    pub solid: bool,
    /// Length of ink per point of `pts` (STEP for samples, STEP / 2 for an outline, which runs
    /// along both sides of the stroke).
    per_pt: f32,
    /// Diagonal of its box: how far inside it must lie scales with it.
    size: f32,
    /// Handwriting (it turns back and forth a lot), not a line or a curve: writing across it
    /// is rare, so a scribble may cut a letter out of it more readily.
    writing: bool,
}

/// How far apart a target's points are (page points).
pub const STEP: f32 = 0.75;

impl Target {
    /// A stroke by its samples and brush width.
    pub fn stroke(id: impl Into<String>, points: &[InkPoint], width: f32) -> Target {
        let xy: Vec<(f32, f32)> = points.iter().map(|p| (p.x, p.y)).collect();
        let pts = along(&xy, false);
        let size = diag_of(&pts);
        let writing = axis_turns(&pts).map_or(false, |a| a.0.max(a.1) >= 6);
        Target { id: id.into(), pts, width: width.max(0.0), solid: false, per_pt: STEP, size, writing }
    }

    /// A stroke by its outline: x, y pairs of the filled polygon the app draws.
    pub fn outline(id: impl Into<String>, xy: &[f32]) -> Target {
        let pts: Vec<(f32, f32)> = xy.chunks_exact(2).map(|c| (c[0], c[1])).collect();
        let pts = along(&pts, true);
        let size = diag_of(&pts);
        // An outline runs along both sides of its stroke: twice the turns.
        let writing = axis_turns(&pts).map_or(false, |a| a.0.max(a.1) >= 12);
        Target { id: id.into(), pts, width: 0.0, solid: false, per_pt: STEP / 2.0, size, writing }
    }

    /// A box on the page (a picture, a graph): only ever deleted whole.
    pub fn boxed(id: impl Into<String>, r: Rect) -> Target {
        let mut pts = Vec::with_capacity(49);
        if !r.is_empty() && r.width().is_finite() && r.height().is_finite() {
            for i in 0..7 {
                for j in 0..7 {
                    pts.push((r.x0 + r.width() * (i as f32 + 0.5) / 7.0, r.y0 + r.height() * (j as f32 + 0.5) / 7.0));
                }
            }
        }
        let per_pt = if pts.is_empty() { 0.0 } else { (r.width() * r.height() / 49.0).sqrt() };
        let size = if pts.is_empty() { 0.0 } else { r.width().hypot(r.height()) };
        Target { id: id.into(), pts, width: 0.0, solid: true, per_pt, size, writing: false }
    }

    /// How deep inside the covered area (from the gesture's middle line) this target's points
    /// must lie, for a scribble `diag` across: a twentieth of the target's size, so the scribble
    /// may end just past a small letter, while colouring that reaches a big outline
    /// (overshooting it by a couple of points at most) doesn't cover the outline; but no more
    /// than the scribble's own size allows (a small scribble over a long joined-up word).
    fn margin(&self, diag: f32) -> f32 {
        (0.05 * self.size).min(0.08 * diag).clamp(0.25, 3.5) + self.width * 0.25
    }
}

/// Whether two strokes touch (their ink meets, give or take a point).
fn touch(a: &Target, b: &Target) -> bool {
    let r = (a.width + b.width) * 0.5 + 1.0;
    let bb = |t: &Target| t.pts.iter().fold(Rect::EMPTY, |r, &(x, y)| r.include(x, y));
    if !bb(a).inflate(r).intersects(&bb(b)) {
        return false;
    }
    a.pts.iter().any(|&(x, y)| b.pts.iter().any(|&(u, v)| (x - u).hypot(y - v) <= r))
}

fn diag_of(pts: &[(f32, f32)]) -> f32 {
    let bb = pts.iter().fold(Rect::EMPTY, |a, &(x, y)| a.include(x, y));
    if bb.is_empty() { 0.0 } else { bb.width().hypot(bb.height()) }
}

/// Points at most [`STEP`] apart along a polyline (closed: back to its start too).
fn along(pts: &[(f32, f32)], closed: bool) -> Vec<(f32, f32)> {
    let pts: Vec<(f32, f32)> = pts.iter().copied().filter(|p| p.0.is_finite() && p.1.is_finite()).collect();
    let mut out = Vec::new();
    let Some(&first) = pts.first() else { return out };
    out.push(first);
    let n = pts.len();
    let segs = if closed && n > 2 { n } else { n - 1 };
    for i in 0..segs {
        let (a, b) = (pts[i], pts[(i + 1) % n]);
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        let k = ((len / STEP).ceil() as usize).clamp(1, 4096);
        for j in 1..=k {
            let t = j as f32 / k as f32;
            out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t));
        }
    }
    out
}

/// What [`judge`] decided: what goes. Empty: the gesture is ink.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Verdict {
    /// Removed entirely.
    pub delete: Vec<String>,
    /// Longer strokes the scribble covers only in part: just the part under `path` goes.
    pub cut: Vec<String>,
    /// The eraser path for `cut`: (x, y, radius) along the gesture.
    pub path: Vec<(f32, f32, f32)>,
}

impl Verdict {
    pub fn is_empty(&self) -> bool {
        self.delete.is_empty() && self.cut.is_empty()
    }
}

/// How a gesture moves (see [`motion`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    /// Diagonal of its bounding box (page points).
    pub diag: f32,
    /// Length of its path.
    pub length: f32,
    /// Turns back along its main axes.
    pub turns: u32,
    /// About how far apart its passes are: the area it spans over its length.
    pub spacing: f32,
    /// How far it moves on per pass, over how long its passes are: small when it goes back
    /// and forth on the spot (scribbling), larger when it moves on like writing.
    pub advance: f32,
    pub ms: u32,
}

/// The longest a scribble takes.
const MAX_MS: u32 = 3000;
/// The smallest a scribble is (diagonal, page points): one small letter.
const MIN_DIAG: f32 = 2.5;
/// Turns a scribble has at least (four passes).
const MIN_TURNS: u32 = 3;
/// Share of a target that must lie inside the covered area to go whole …
const WHOLE: f32 = 0.55;
/// … and of one about as big as the scribble.
const WHOLE_SMALL: f32 = 0.35;
/// … and of one that touches a stroke going whole (the rest of a letter of several strokes).
const WHOLE_TOUCHING: f32 = 0.2;
/// A covered area this many times larger than the ink it covers is colouring in.
const COLOURING: f32 = 5.0;

impl Motion {
    /// Enough of a scribble to cut a piece out of a longer stroke: many passes. Out of a line
    /// or a curve only when they go back and forth on the spot: writing moves on further per
    /// stroke, and joined-up writing across a line must never cut the line.
    fn cuts(&self, t: &Target) -> bool {
        self.turns >= 5 && self.length >= 4.0 * self.diag && (t.writing || self.advance <= 0.13)
    }
}

/// How the gesture moves, or None when it doesn't move like a scribble.
pub fn motion(pts: &[InkPoint]) -> Option<Motion> {
    let pts: Vec<&InkPoint> = pts.iter().filter(|p| p.x.is_finite() && p.y.is_finite()).collect();
    let n = pts.len();
    if n < 8 {
        return None;
    }
    let ms = pts[n - 1].t_ms.saturating_sub(pts[0].t_ms);
    if ms > MAX_MS {
        return None; // slow: drawing, not scribbling
    }
    let mut bb = Rect::EMPTY;
    let mut length = 0.0f32;
    for (i, p) in pts.iter().enumerate() {
        bb = bb.include(p.x, p.y);
        if i > 0 {
            length += (p.x - pts[i - 1].x).hypot(p.y - pts[i - 1].y);
        }
    }
    let diag = bb.width().hypot(bb.height());
    if !(diag >= MIN_DIAG) || length < 2.5 * diag {
        return None;
    }
    let xy: Vec<(f32, f32)> = pts.iter().map(|p| (p.x, p.y)).collect();
    let (tu, tv, eu, ev) = axis_turns(&xy)?;
    let turns = tu.max(tv);
    if turns < MIN_TURNS {
        return None;
    }
    let spacing = (eu.max(0.5) * ev.max(0.5) / length).clamp(0.3, diag / 2.0);
    // Passes run along the axis it turns on most; it moves on along the other one.
    let (pass, on) = if tu > tv { (eu, ev) } else { (ev, eu) };
    let advance = on / (turns + 1) as f32 / pass.max(0.5);
    Some(Motion { diag, length, turns, spacing, advance, ms })
}

/// Turns back along the main axes (principal components) of a path, each with 30 % of the
/// path's extent along that axis as slack, and the two extents.
fn axis_turns(pts: &[(f32, f32)]) -> Option<(u32, u32, f32, f32)> {
    let n = pts.len();
    if n < 3 {
        return None;
    }
    let (mx, my) = (pts.iter().map(|p| p.0).sum::<f32>() / n as f32, pts.iter().map(|p| p.1).sum::<f32>() / n as f32);
    let (mut sxx, mut syy, mut sxy) = (0.0f32, 0.0f32, 0.0f32);
    for p in pts {
        let (dx, dy) = (p.0 - mx, p.1 - my);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    let a = 0.5 * (2.0 * sxy).atan2(sxx - syy);
    let (c, s) = (a.cos(), a.sin());
    let pu: Vec<f32> = pts.iter().map(|p| (p.0 - mx) * c + (p.1 - my) * s).collect();
    let pv: Vec<f32> = pts.iter().map(|p| (p.1 - my) * c - (p.0 - mx) * s).collect();
    let (eu, ev) = (extent(&pu), extent(&pv));
    Some((turns(&pu, (0.3 * eu).max(0.5)), turns(&pv, (0.3 * ev).max(0.5)), eu, ev))
}

fn extent(v: &[f32]) -> f32 {
    v.iter().fold(f32::MIN, |a, &b| a.max(b)) - v.iter().fold(f32::MAX, |a, &b| a.min(b))
}

/// How often the values turn back by at least `slack`.
fn turns(q: &[f32], slack: f32) -> u32 {
    let mut n = 0;
    let mut dir = 0i8;
    let (mut hi, mut lo) = (q[0], q[0]);
    for &x in &q[1..] {
        match dir {
            1 => {
                if x > hi {
                    hi = x;
                } else if hi - x >= slack {
                    n += 1;
                    dir = -1;
                    lo = x;
                }
            }
            -1 => {
                if x < lo {
                    lo = x;
                } else if x - lo >= slack {
                    n += 1;
                    dir = 1;
                    hi = x;
                }
            }
            _ => {
                hi = hi.max(x);
                lo = lo.min(x);
                if hi - x >= slack {
                    dir = -1;
                    lo = x;
                } else if x - lo >= slack {
                    dir = 1;
                    hi = x;
                }
            }
        }
    }
    n
}

/// What a gesture drawn `width` wide deletes of `targets` (see the module docs).
pub fn judge(pts: &[InkPoint], width: f32, targets: &[Target]) -> Verdict {
    analyse(pts, width, targets).map(|a| a.verdict).unwrap_or_default()
}

/// How [`judge`] came to its verdict (for tests and tuning).
#[derive(Debug, Clone)]
#[cfg_attr(not(test), allow(dead_code))]
struct Analysis {
    motion: Motion,
    /// Per target that is covered at all: its id, the share inside, the covered length.
    covered: Vec<(String, f32, f32)>,
    area: f32,
    ink: f32,
    verdict: Verdict,
}

fn analyse(pts: &[InkPoint], width: f32, targets: &[Target]) -> Option<Analysis> {
    let m = motion(pts)?;
    let path: Vec<(f32, f32)> = pts.iter().filter(|p| p.x.is_finite() && p.y.is_finite()).map(|p| (p.x, p.y)).collect();
    let pen = (width * 0.5).clamp(0.2, 20.0);
    // Widening that closes the gaps between passes, and how far inside ink must lie.
    let close = (1.1 * m.spacing).clamp(0.4, m.diag / 4.0);
    let cover = Cover::new(&path, pen, close)?;

    let mut v = Verdict::default();
    let mut covered = Vec::new();
    let mut ink = 0.0f32; // covered ink, as area
    for t in targets {
        if t.pts.is_empty() {
            continue;
        }
        let need = pen + t.margin(m.diag);
        let inside = t.pts.iter().filter(|&&(x, y)| cover.depth(x, y) >= need).count();
        if inside == 0 {
            continue;
        }
        let frac = inside as f32 / t.pts.len() as f32;
        let length = inside as f32 * t.per_pt;
        covered.push((t.id.clone(), frac, length));
        // Something about as big as the scribble (a letter, a word) goes whole even when the
        // scribble missed a corner of it; a stroke much bigger than the scribble (a joined-up
        // word, a long line) loses only the covered piece.
        let small = !t.solid && t.size <= 1.5 * m.diag;
        if frac >= if small { WHOLE_SMALL } else { WHOLE } {
            v.delete.push(t.id.clone());
            ink += if t.solid { t.per_pt * t.per_pt * inside as f32 } else { (t.pts.len() as f32 * t.per_pt).max(2.0) * (t.width + 6.0) };
        } else if !t.solid && !small && m.cuts(t) && length >= (0.3 * m.diag).max(3.0) {
            v.cut.push(t.id.clone());
            ink += length * (t.width + 6.0);
        }
    }
    // The rest of a letter written in several strokes: small strokes touching one that goes,
    // covered a fair bit themselves.
    let gone: Vec<&Target> = targets.iter().filter(|t| v.delete.contains(&t.id)).collect();
    for t in targets {
        if t.solid || v.delete.contains(&t.id) || v.cut.contains(&t.id) || t.size > 1.5 * m.diag {
            continue;
        }
        let Some(&(_, frac, length)) = covered.iter().find(|c| c.0 == t.id) else { continue };
        if frac >= WHOLE_TOUCHING && gone.iter().any(|o| !o.solid && touch(t, o)) {
            v.delete.push(t.id.clone());
            ink += length.max(2.0) * (t.width + 6.0);
        }
    }
    let area = cover.area();
    if v.is_empty() || area > COLOURING * ink {
        v = Verdict::default();
    } else {
        let r = pen + close;
        v.path = path.iter().map(|&(x, y)| (x, y, r)).collect();
    }
    Some(Analysis { motion: m, covered, area, ink, verdict: v })
}

/// The area a gesture covers, on a grid: per cell, how deep inside it lies (page points).
struct Cover {
    x0: f32,
    y0: f32,
    cell: f32,
    w: usize,
    h: usize,
    depth: Vec<f32>,
}

impl Cover {
    /// The path widened by `pen + close`, then narrowed by `close` again: a band as wide as the
    /// ink, with gaps narrower than 2 × `close` between passes filled.
    fn new(path: &[(f32, f32)], pen: f32, close: f32) -> Option<Cover> {
        let mut bb = Rect::EMPTY;
        for &(x, y) in path {
            bb = bb.include(x, y);
        }
        let r = pen + close;
        let bb = bb.inflate(r + 2.0);
        let diag = bb.width().hypot(bb.height());
        if !diag.is_finite() || bb.is_empty() {
            return None;
        }
        let cell = (diag / 90.0).clamp(0.25, 1.5).max(diag / 300.0);
        let w = (bb.width() / cell).ceil() as usize + 1;
        let h = (bb.height() / cell).ceil() as usize + 1;
        let mut mask = vec![false; w * h];
        let at = |x: f32, y: f32| ((x - bb.x0) / cell, (y - bb.y0) / cell);
        let segs: Vec<((f32, f32), (f32, f32))> = if path.len() == 1 { vec![(path[0], path[0])] } else { path.windows(2).map(|s| (s[0], s[1])).collect() };
        for (a, b) in segs {
            let (ax, ay) = at(a.0.min(b.0) - r, a.1.min(b.1) - r);
            let (bx, by) = at(a.0.max(b.0) + r, a.1.max(b.1) + r);
            for j in (ay.floor().max(0.0) as usize)..=(by.ceil() as usize).min(h - 1) {
                for i in (ax.floor().max(0.0) as usize)..=(bx.ceil() as usize).min(w - 1) {
                    let (cx, cy) = (bb.x0 + (i as f32 + 0.5) * cell, bb.y0 + (j as f32 + 0.5) * cell);
                    if seg_dist(cx, cy, a, b) <= r {
                        mask[j * w + i] = true;
                    }
                }
            }
        }
        // Narrow by `close` again (the gaps it filled stay filled), then measure depth.
        let d0 = edt(&mask, w, h);
        let band: Vec<bool> = d0.iter().map(|&d| d * cell > close).collect();
        let depth = edt(&band, w, h).into_iter().map(|d| d * cell).collect();
        Some(Cover { x0: bb.x0, y0: bb.y0, cell, w, h, depth })
    }

    /// How deep inside the covered area (x, y) lies; 0 outside.
    fn depth(&self, x: f32, y: f32) -> f32 {
        let (i, j) = ((x - self.x0) / self.cell, (y - self.y0) / self.cell);
        if !(i >= 0.0 && j >= 0.0) || i >= self.w as f32 || j >= self.h as f32 {
            return 0.0;
        }
        self.depth[j as usize * self.w + i as usize]
    }

    /// The covered area (square page points).
    fn area(&self) -> f32 {
        self.depth.iter().filter(|&&d| d > 0.0).count() as f32 * self.cell * self.cell
    }
}

fn seg_dist(px: f32, py: f32, a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-9 { 0.0 } else { (((px - a.0) * dx + (py - a.1) * dy) / l2).clamp(0.0, 1.0) };
    (px - (a.0 + t * dx)).hypot(py - (a.1 + t * dy))
}

/// Euclidean distance (in cells) from each cell to the nearest cell outside the mask; 0
/// outside. Felzenszwalb and Huttenlocher's two passes of 1-D lower envelopes.
pub(crate) fn edt(mask: &[bool], w: usize, h: usize) -> Vec<f32> {
    let inf = (2 * (w + h) * (w + h)) as f32;
    let mut g: Vec<f32> = mask.iter().map(|&m| if m { inf } else { 0.0 }).collect();
    let n = w.max(h);
    let (mut f, mut d) = (vec![0f32; n], vec![0f32; n]);
    let (mut v, mut z) = (vec![0usize; n], vec![0f32; n + 1]);
    for x in 0..w {
        for y in 0..h {
            f[y] = g[y * w + x];
        }
        envelope(&f[..h], &mut d[..h], &mut v, &mut z);
        for y in 0..h {
            g[y * w + x] = d[y];
        }
    }
    for y in 0..h {
        f[..w].copy_from_slice(&g[y * w..(y + 1) * w]);
        envelope(&f[..w], &mut d[..w], &mut v, &mut z);
        g[y * w..(y + 1) * w].copy_from_slice(&d[..w]);
    }
    g.into_iter().map(f32::sqrt).collect()
}

/// Squared distance transform of one row: d[q] = min over p of (q − p)² + f[p].
fn envelope(f: &[f32], d: &mut [f32], v: &mut [usize], z: &mut [f32]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f32::NEG_INFINITY;
    z[1] = f32::INFINITY;
    for q in 1..n {
        loop {
            let p = v[k];
            let s = ((f[q] + (q * q) as f32) - (f[p] + (p * p) as f32)) / (2.0 * (q as f32 - p as f32));
            if s <= z[k] && k > 0 {
                k -= 1;
                continue;
            }
            if s <= z[k] {
                // k == 0: the new parabola is lower everywhere.
                v[0] = q;
                z[1] = f32::INFINITY;
                break;
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = f32::INFINITY;
            break;
        }
    }
    k = 0;
    for (q, out) in d.iter_mut().enumerate().take(n) {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let p = v[k];
        let dq = q as f32 - p as f32;
        *out = dq * dq + f[p];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ink::{Brush, outline};
    use crate::plot::glyph;

    struct Rng(u64);
    impl Rng {
        fn f(&mut self) -> f32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 40) as f32 / (1u64 << 24) as f32
        }
        fn r(&mut self, a: f32, b: f32) -> f32 {
            a + (b - a) * self.f()
        }
        fn chance(&mut self, p: f32) -> bool {
            self.f() < p
        }
    }

    type Line = Vec<(f32, f32)>;

    /// The path sampled the way a pen reports it: 240 samples a second at `speed` pt/s, with
    /// a little noise.
    fn pen(path: &[(f32, f32)], speed: f32, g: &mut Rng) -> Vec<InkPoint> {
        let step = (speed / 240.0).max(0.05);
        let mut out = Vec::new();
        let mut t = 0.0f32;
        let mut left = 0.0f32; // how far into the next segment the next sample lies
        let mut emit = |x: f32, y: f32, t: &mut f32, g: &mut Rng| {
            out.push(InkPoint { x: x + g.r(-0.08, 0.08), y: y + g.r(-0.08, 0.08), pressure: 0.5, t_ms: *t as u32 });
            *t += 1000.0 / 240.0;
        };
        if let Some(&(x, y)) = path.first() {
            emit(x, y, &mut t, g);
        }
        for w in path.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = (b.0 - a.0).hypot(b.1 - a.1);
            let mut d = step - left;
            while d <= len {
                let k = d / len;
                emit(a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k, &mut t, g);
                d += step;
            }
            left = len - (d - step);
        }
        out
    }

    /// Turns rounded the way a quick hand rounds them: the path averaged over `w` points of
    /// its length (the turns lose about a quarter of that, the passes keep their length).
    fn round(p: Line, w: f32) -> Line {
        if w <= 0.0 || p.len() < 3 {
            return p;
        }
        let fine: Line = {
            let mut out = vec![p[0]];
            for s in p.windows(2) {
                let k = ((s[1].0 - s[0].0).hypot(s[1].1 - s[0].1) / 0.3).ceil().max(1.0) as usize;
                for j in 1..=k {
                    let t = j as f32 / k as f32;
                    out.push((s[0].0 + (s[1].0 - s[0].0) * t, s[0].1 + (s[1].1 - s[0].1) * t));
                }
            }
            out
        };
        let half = ((w / 0.3) / 2.0) as usize;
        (0..fine.len())
            .map(|i| {
                let (a, b) = (i.saturating_sub(half), (i + half + 1).min(fine.len()));
                let n = (b - a) as f32;
                let (sx, sy) = fine[a..b].iter().fold((0.0, 0.0), |s, q| (s.0 + q.0, s.1 + q.1));
                (sx / n, sy / n)
            })
            .collect()
    }

    fn turned(p: &[(f32, f32)], a: f32, cx: f32, cy: f32) -> Line {
        let (c, s) = (a.cos(), a.sin());
        p.iter().map(|&(x, y)| (cx + x * c - y * s, cy + x * s + y * c)).collect()
    }

    /// A word written in the stroke font `h` high from (x, y) (its top): print letters (one
    /// stroke per pen line) or joined up (one stroke). Returns the strokes per letter, and the
    /// letters' boxes.
    fn word(text: &str, x: f32, y: f32, h: f32, joined: bool, g: &mut Rng) -> (Vec<Vec<Line>>, Vec<Rect>) {
        let mut letters = Vec::new();
        let mut boxes = Vec::new();
        let mut cx = x;
        for c in text.chars() {
            let Some((w, lines)) = glyph(c) else { continue };
            let (k, slant) = (h * g.r(0.92, 1.08), g.r(-0.12, 0.12));
            let mut strokes = Vec::new();
            let mut bb = Rect::EMPTY;
            for l in lines {
                let pts: Line = l.iter().map(|&(u, v)| (cx + (u + slant * (1.0 - v)) * k + g.r(-0.15, 0.15), y + v * k + g.r(-0.15, 0.15))).collect();
                for &(px, py) in &pts {
                    bb = bb.include(px, py);
                }
                strokes.push(pts);
            }
            letters.push(strokes);
            boxes.push(bb);
            cx += (w + 0.1) * h;
        }
        if joined {
            let all: Line = letters.iter().flatten().flatten().copied().collect();
            let bb = boxes.iter().fold(Rect::EMPTY, |a, b| a.union(b));
            return (vec![vec![all]], vec![bb]);
        }
        (letters, boxes)
    }

    /// A zigzag over the box: passes across its short side, moving along its long side.
    fn zigzag(b: Rect, g: &mut Rng) -> Line {
        let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
        let (long, short, a) = if b.width() >= b.height() { (b.width(), b.height(), 0.0) } else { (b.height(), b.width(), std::f32::consts::FRAC_PI_2) };
        let short = short.max(2.0);
        let amp = short * g.r(1.3, 1.8);
        let spacing = short * g.r(0.12, 0.32);
        let (u0, u1) = (-long / 2.0 - short * g.r(0.0, 0.25), long / 2.0 + short * g.r(0.0, 0.25));
        let mut p = Vec::new();
        let mut u = u0;
        let mut up = true;
        while u <= u1 {
            p.push((u, if up { -amp / 2.0 } else { amp / 2.0 } + g.r(-0.08, 0.08) * short));
            up = !up;
            u += spacing * g.r(0.8, 1.2);
        }
        if p.len() < 5 {
            // Too few passes for a narrow box: go back and forth over it again.
            let back: Line = p.iter().rev().map(|&(x, y)| (x, -y)).collect();
            p.extend(back);
        }
        let p = round(p, spacing * g.r(0.0, 1.2));
        turned(&p, a + g.r(-1.0, 1.0) * (0.6 * short / long.max(1.0)).min(0.25), cx, cy)
    }

    /// Back and forth along the long side, drifting across the short one.
    fn back_and_forth(b: Rect, g: &mut Rng) -> Line {
        let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
        let (long, short, a) = if b.width() >= b.height() { (b.width(), b.height(), 0.0) } else { (b.height(), b.width(), std::f32::consts::FRAC_PI_2) };
        let span = long.max(3.0) * g.r(1.25, 1.7);
        let passes = g.r(5.0, 9.0) as usize;
        let drift = short.max(2.0) * g.r(1.05, 1.4);
        let p: Line = (0..=passes)
            .map(|i| (if i % 2 == 0 { -span / 2.0 } else { span / 2.0 } + g.r(-0.1, 0.1) * span, -drift / 2.0 + drift * i as f32 / passes as f32))
            .collect();
        let p = round(p, drift / passes as f32 * g.r(0.0, 1.2));
        turned(&p, a + g.r(-1.0, 1.0) * (0.6 * short / long.max(1.0)).min(0.2), cx, cy)
    }

    /// Little loops moving over the box (a Kringel).
    fn loops(b: Rect, g: &mut Rng) -> Line {
        let (cx, cy) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0);
        let (long, short, a) = if b.width() >= b.height() { (b.width(), b.height(), 0.0) } else { (b.height(), b.width(), std::f32::consts::FRAC_PI_2) };
        let short = short.max(2.0);
        let (rx, ry) = (short * g.r(0.45, 0.75), short * g.r(0.45, 0.75));
        let turns = g.r(2.5, 6.0) + long / short;
        let n = (turns * 40.0) as usize;
        let p: Line = (0..=n)
            .map(|i| {
                let t = i as f32 / n as f32;
                let phi = t * turns * std::f32::consts::TAU;
                (-long / 2.0 + long * t + rx * phi.cos(), ry * phi.sin())
            })
            .collect();
        turned(&p, a + g.r(-1.0, 1.0) * (0.6 * short / long.max(1.0)).min(0.3), cx, cy)
    }

    fn scribble(b: Rect, g: &mut Rng) -> Line {
        match (g.f() * 3.0) as usize {
            0 => zigzag(b, g),
            1 => back_and_forth(b, g),
            _ => loops(b, g),
        }
    }

    /// Scribbles are quick: about 6 to 14 passes a second, and over in 0.4 to 2.2 s.
    fn scribble_speed(b: Rect, path: &[(f32, f32)], g: &mut Rng) -> f32 {
        let len: f32 = path.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum();
        let by_passes = b.width().min(b.height()).max(2.0) * 1.3 * 2.0 * g.r(6.0, 14.0);
        by_passes.max(len / g.r(0.4, 2.2)).min(900.0)
    }

    /// A closed hand-drawn outline: a rectangle or an ellipse about (cx, cy).
    fn shape(cx: f32, cy: f32, w: f32, h: f32, g: &mut Rng) -> Line {
        let n = 80;
        let ellipse = g.chance(0.5);
        let mut p: Line = (0..=n + 3)
            .map(|i| {
                let t = (i as f32 / n as f32) * std::f32::consts::TAU;
                let (x, y) = if ellipse {
                    (w / 2.0 * t.cos(), h / 2.0 * t.sin())
                } else {
                    let (c, s) = (t.cos(), t.sin());
                    let k = (c.abs() / (w / 2.0)).max(s.abs() / (h / 2.0));
                    (c / k, s / k)
                };
                (cx + x + g.r(-0.4, 0.4), cy + y + g.r(-0.4, 0.4))
            })
            .collect();
        p = round(p, 1.5);
        p
    }

    /// Where a line through (px, py) along (dx, dy) enters and leaves the closed outline.
    fn chord(poly: &[(f32, f32)], px: f32, py: f32, dx: f32, dy: f32) -> Option<(f32, f32)> {
        let mut ts = Vec::new();
        for w in poly.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (ex, ey) = (b.0 - a.0, b.1 - a.1);
            let den = dx * ey - dy * ex;
            if den.abs() < 1e-6 {
                continue;
            }
            let t = ((a.0 - px) * ey - (a.1 - py) * ex) / den;
            let u = ((a.0 - px) * dy - (a.1 - py) * dx) / den;
            if (0.0..=1.0).contains(&u) {
                ts.push(t);
            }
        }
        if ts.len() < 2 {
            return None;
        }
        Some((ts.iter().copied().fold(f32::MAX, f32::min), ts.iter().copied().fold(f32::MIN, f32::max)))
    }

    /// Colouring in (or hatching) inside an outline: passes across it whose ends reach the
    /// outline give or take `over` (undershooting or overshooting a little).
    fn colouring(poly: &[(f32, f32)], spacing: (f32, f32), over: (f32, f32), g: &mut Rng) -> Line {
        let bb = poly.iter().fold(Rect::EMPTY, |a, &(x, y)| a.include(x, y));
        let (cx, cy) = ((bb.x0 + bb.x1) / 2.0, (bb.y0 + bb.y1) / 2.0);
        let a = g.r(0.0, std::f32::consts::PI);
        let (dx, dy) = (a.cos(), a.sin());
        let (nx, ny) = (-dy, dx);
        let reach = bb.width().hypot(bb.height()) / 2.0;
        // Colour a part or all of it (as much as fits into the scribble time).
        let (o0, o1) = if g.chance(0.5) { (-reach, reach) } else { let m = g.r(-reach * 0.5, reach * 0.3); (m, m + reach * g.r(0.4, 0.9)) };
        let mut p = Vec::new();
        let mut o = o0;
        let mut flip = false;
        while o <= o1 && p.len() < 70 {
            let (px, py) = (cx + nx * o, cy + ny * o);
            if let Some((t0, t1)) = chord(poly, px, py, dx, dy) {
                let (e0, e1) = (t0 - g.r(over.0, over.1), t1 + g.r(over.0, over.1));
                let (s, e) = if flip { (e1, e0) } else { (e0, e1) };
                p.push((px + dx * s, py + dy * s));
                p.push((px + dx * e, py + dy * e));
                flip = !flip;
            }
            o += g.r(spacing.0, spacing.1);
        }
        // One stroke zigzagging from pass to pass.
        let mut z = Vec::new();
        for (i, c) in p.chunks(2).enumerate() {
            if i == 0 {
                z.push(c[0]);
            }
            if c.len() == 2 {
                z.push(c[1]);
            }
            if let Some(next) = p.get((i + 1) * 2) {
                z.push(*next);
            }
        }
        round(z, spacing.0 * g.r(0.0, 0.6))
    }

    /// The targets of strokes, as samples (n0te Tablet) or as outlines (Android).
    fn targets(strokes: &[(String, Line)], width: f32, outlines: bool) -> Vec<Target> {
        strokes
            .iter()
            .map(|(id, l)| {
                let pts: Vec<InkPoint> = l.iter().map(|&(x, y)| InkPoint { x, y, pressure: 0.5, t_ms: 0 }).collect();
                if outlines {
                    let o = outline(&pts, &Brush { width, ..Brush::default() });
                    Target::outline(id.clone(), &o)
                } else {
                    Target::stroke(id.clone(), &pts, width)
                }
            })
            .collect()
    }

    /// The old rule (before 0.36): a quick back-and-forth with four sharp reversals, deleting
    /// every stroke it touches. For comparison in the printout.
    fn old_rule(pts: &[InkPoint], width: f32, strokes: &[(String, Line)]) -> Vec<String> {
        let n = pts.len();
        if n < 12 {
            return vec![];
        }
        let bb = pts.iter().fold(Rect::EMPTY, |a, p| a.include(p.x, p.y));
        let diag = bb.width().hypot(bb.height());
        let len: f32 = pts.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum();
        if diag < 6.0 || len < diag * 3.0 || pts[n - 1].t_ms - pts[0].t_ms > 2500 {
            return vec![];
        }
        let step = (diag / 40.0).max(2.0);
        let (mut rev, mut ax, mut ay, mut pdx, mut pdy) = (0, pts[0].x, pts[0].y, 0.0f32, 0.0f32);
        for p in &pts[1..] {
            let (dx, dy) = (p.x - ax, p.y - ay);
            if dx.hypot(dy) < step {
                continue;
            }
            if (pdx != 0.0 || pdy != 0.0) && dx * pdx + dy * pdy < -0.5 * dx.hypot(dy) * pdx.hypot(pdy) {
                rev += 1;
            }
            (pdx, pdy, ax, ay) = (dx, dy, p.x, p.y);
        }
        if rev < 4 {
            return vec![];
        }
        let r = width / 2.0 + 1.5 + width / 2.0;
        strokes
            .iter()
            .filter(|(_, l)| pts.iter().step_by(2).any(|q| l.windows(2).any(|w| seg_dist(q.x, q.y, w[0], w[1]) <= r)))
            .map(|(id, _)| id.clone())
            .collect()
    }

    struct Case {
        strokes: Vec<(String, Line)>,
        gesture: Vec<InkPoint>,
        width: f32,
        /// Must be deleted (all of them).
        gone: Vec<String>,
        /// Must be cut (each of them; deleting them whole is fine too).
        cut: Vec<String>,
    }

    impl Case {
        /// Whether the verdict deletes exactly `gone`, cuts `cut` and leaves everything else.
        fn ok(&self, v: &Verdict) -> bool {
            let touched: Vec<&String> = v.delete.iter().chain(&v.cut).collect();
            let all_gone = self.gone.iter().all(|id| v.delete.contains(id));
            let all_cut = self.cut.iter().all(|id| touched.contains(&id));
            let nothing_else = touched.iter().all(|id| self.gone.contains(id) || self.cut.contains(id));
            all_gone && all_cut && nothing_else
        }
        fn old_ok(&self) -> bool {
            let got = old_rule(&self.gesture, self.width, &self.strokes);
            let expect: Vec<&String> = self.gone.iter().chain(&self.cut).collect();
            expect.iter().all(|id| got.contains(id)) && got.iter().all(|id| expect.contains(&id))
        }
    }

    fn ids(prefix: &str, ls: &[Vec<Line>]) -> Vec<(String, Line)> {
        let mut out = Vec::new();
        for (i, letter) in ls.iter().enumerate() {
            for (j, l) in letter.iter().enumerate() {
                out.push((format!("{prefix}{i}.{j}"), l.clone()));
            }
        }
        out
    }

    const LETTERS: &str = "abdeghkmnopqrsuwxyzABDEFGHKMNPRSUWXYZ0123456789";

    fn some_word(g: &mut Rng, len: (usize, usize)) -> String {
        let n = g.r(len.0 as f32, len.1 as f32 + 0.99) as usize;
        let chars: Vec<char> = LETTERS.chars().collect();
        (0..n).map(|_| chars[(g.f() * chars.len() as f32) as usize % chars.len()]).collect()
    }

    /// Scribbling out one letter of a printed word: that letter goes, its neighbours stay.
    fn one_letter(g: &mut Rng) -> Case {
        let h = g.r(6.0, 18.0);
        let text = some_word(g, (3, 6));
        let (letters, boxes) = word(&text, 100.0, 100.0, h, false, g);
        let k = (g.f() * letters.len() as f32) as usize % letters.len();
        let b = boxes[k];
        let path = scribble(b, g);
        let gesture = pen(&path, scribble_speed(b, &path, g), g);
        let strokes = ids("l", &letters);
        let gone = strokes.iter().filter(|(id, _)| id.starts_with(&format!("l{k}."))).map(|(id, _)| id.clone()).collect();
        Case { strokes, gesture, width: g.r(0.8, 2.0), gone, cut: vec![] }
    }

    /// Scribbling out a whole printed word: all of it goes, the next word stays.
    fn whole_word(g: &mut Rng) -> Case {
        let h = g.r(6.0, 16.0);
        let (a, ab) = word(&some_word(g, (2, 6)), 100.0, 100.0, h, false, g);
        let wb = ab.iter().fold(Rect::EMPTY, |x, b| x.union(b));
        let (b, _) = word(&some_word(g, (2, 6)), wb.x1 + h * g.r(0.8, 1.5), 100.0, h, false, g);
        let path = scribble(wb, g);
        let gesture = pen(&path, scribble_speed(wb, &path, g), g);
        let mut strokes = ids("a", &a);
        let gone = strokes.iter().map(|(id, _)| id.clone()).collect();
        strokes.extend(ids("b", &b));
        Case { strokes, gesture, width: g.r(0.8, 2.0), gone, cut: vec![] }
    }

    /// Scribbling out one letter of a joined-up word: a piece of the word's stroke goes.
    fn joined_letter(g: &mut Rng) -> Case {
        let h = g.r(7.0, 16.0);
        let text = some_word(g, (4, 7));
        let (_, boxes) = word(&text, 100.0, 100.0, h, false, &mut Rng(g.0));
        let (joined, _) = word(&text, 100.0, 100.0, h, true, &mut Rng(g.0));
        g.f();
        let k = 1 + (g.f() * (boxes.len() - 2) as f32) as usize % (boxes.len() - 2);
        let b = boxes[k];
        let path = zigzag(b, g);
        let gesture = pen(&path, scribble_speed(b, &path, g), g);
        let strokes = ids("w", &joined);
        Case { strokes, gesture, width: g.r(0.8, 2.0), gone: vec![], cut: vec!["w0.0".into()] }
    }

    /// Colouring in a shape with the pen: nothing goes, the colouring stays ink.
    fn colour_in(g: &mut Rng) -> Case {
        let (w, h) = (g.r(20.0, 90.0), g.r(15.0, 70.0));
        let outline = shape(150.0, 150.0, w, h, g);
        let (nb, _) = word("ab", 150.0 + w / 2.0 + 6.0, 150.0 - 8.0, 10.0, false, g);
        let fill = colouring(&outline, (1.5, 4.0), (-2.0, 2.5), g);
        let gesture = pen(&fill, g.r(300.0, 700.0), g);
        let mut strokes = vec![("outline".to_string(), outline)];
        strokes.extend(ids("n", &nb));
        Case { strokes, gesture, width: g.r(0.8, 2.5), gone: vec![], cut: vec![] }
    }

    /// Hatching a shape in one stroke: nothing goes.
    fn hatch(g: &mut Rng) -> Case {
        let (w, h) = (g.r(25.0, 90.0), g.r(20.0, 70.0));
        let outline = shape(150.0, 150.0, w, h, g);
        let fill = colouring(&outline, (3.0, 8.0), (-1.0, 1.5), g);
        let gesture = pen(&fill, g.r(250.0, 600.0), g);
        Case { strokes: vec![("outline".to_string(), outline)], gesture, width: g.r(0.6, 1.5), gone: vec![], cut: vec![] }
    }

    /// Circling a word twice (to mark it): the word stays.
    fn circle_twice(g: &mut Rng) -> Case {
        let h = g.r(7.0, 16.0);
        let (letters, boxes) = word(&some_word(g, (2, 6)), 100.0, 100.0, h, false, g);
        let b = boxes.iter().fold(Rect::EMPTY, |x, b| x.union(b)).inflate(g.r(2.0, 8.0));
        let (cx, cy, rx, ry) = ((b.x0 + b.x1) / 2.0, (b.y0 + b.y1) / 2.0, b.width() / 2.0 * 1.15, b.height() / 2.0 * 1.25);
        let n = 160;
        let loops = g.r(1.9, 2.3);
        let p: Line = (0..=n)
            .map(|i| {
                let t = i as f32 / n as f32 * loops * std::f32::consts::TAU;
                (cx + rx * g.r(0.97, 1.03) * t.cos(), cy + ry * g.r(0.97, 1.03) * t.sin())
            })
            .collect();
        let gesture = pen(&p, g.r(200.0, 500.0), g);
        Case { strokes: ids("l", &letters), gesture, width: g.r(0.8, 2.0), gone: vec![], cut: vec![] }
    }

    /// Writing a joined-up word across a long line: the line stays whole.
    fn write_over_line(g: &mut Rng) -> Case {
        let h = g.r(7.0, 16.0);
        let (wd, boxes) = word(&some_word(g, (4, 8)), 100.0, 100.0, h, true, g);
        let b = boxes[0];
        let y = b.y0 + b.height() * g.r(0.3, 0.8);
        let line: Line = vec![(b.x0 - 40.0, y), (b.x1 + 40.0, y + g.r(-2.0, 2.0))];
        let gesture = pen(&wd[0][0], g.r(80.0, 200.0), g);
        Case { strokes: vec![("line".to_string(), line)], gesture, width: g.r(0.8, 2.0), gone: vec![], cut: vec![] }
    }

    /// The share of `n` cases from `make` that come out right, with the targets as samples
    /// and as outlines, and how the old rule did.
    fn rate(g: &mut Rng, name: &str, n: usize, make: fn(&mut Rng) -> Case) -> (f32, f32) {
        let (mut ok_s, mut ok_o, mut ok_old) = (0, 0, 0);
        for _ in 0..n {
            let c = make(g);
            let vs = judge(&c.gesture, c.width, &targets(&c.strokes, c.width, false));
            let vo = judge(&c.gesture, c.width, &targets(&c.strokes, c.width, true));
            ok_s += c.ok(&vs) as usize;
            ok_o += c.ok(&vo) as usize;
            ok_old += c.old_ok() as usize;
        }
        let (s, o) = (ok_s as f32 / n as f32, ok_o as f32 / n as f32);
        println!("{name}: samples {:.0} %, outlines {:.0} %  (old rule {:.0} %)", s * 100.0, o * 100.0, ok_old as f32 / n as f32 * 100.0);
        (s, o)
    }

    #[test]
    fn scribbles_delete_what_they_cover_and_colouring_stays() {
        let mut g = Rng(0x5eed_1234_abcd_0001);
        let n = 300;
        // Synthetic hands, so the bars guard against getting worse; they are not the real rate.
        // When this was written: one letter 92 % / 95 %, a word 93 % / 95 %, a joined-up letter
        // 78 % / 76 %, everything that must stay 100 % (the old rule: 2 %, 59 %, –, 39 %, 87 %,
        // 100 %, 16 %).
        let cases: [(&str, fn(&mut Rng) -> Case, f32); 7] = [
            ("one letter of a word", one_letter, 0.9),
            ("a whole word", whole_word, 0.9),
            ("one letter of a joined-up word", joined_letter, 0.7),
            ("colouring in a shape", colour_in, 0.99),
            ("hatching a shape", hatch, 0.99),
            ("circling a word twice", circle_twice, 0.99),
            ("writing across a line", write_over_line, 0.99),
        ];
        let mut failed = Vec::new();
        for (name, make, bar) in cases {
            let (s, o) = rate(&mut g, name, n, make);
            if s < bar || o < bar {
                failed.push(format!("{name}: {:.0} % / {:.0} % (bar {:.0} %)", s * 100.0, o * 100.0, bar * 100.0));
            }
        }
        assert!(failed.is_empty(), "below the bar: {failed:#?}");
    }

    /// A picture of a case: the covered area by depth (grey, darker = deeper; red where deep
    /// enough), the targets (blue; gone ones green) and the gesture (orange).
    fn svg(file: &str, c: &Case) {
        let path: Vec<(f32, f32)> = c.gesture.iter().map(|p| (p.x, p.y)).collect();
        let m = motion(&c.gesture).unwrap();
        let pen = (c.width * 0.5).clamp(0.2, 20.0);
        let close = (1.1 * m.spacing).clamp(0.4, m.diag / 4.0);
        let margin = pen + 0.5;
        let cov = Cover::new(&path, pen, close).unwrap();
        let mut s = String::new();
        let (x0, y0) = (cov.x0, cov.y0);
        let k = 12.0;
        s += &format!("<svg xmlns='http://www.w3.org/2000/svg' width='{}' height='{}'><rect width='100%' height='100%' fill='white'/>", cov.w as f32 * cov.cell * k, cov.h as f32 * cov.cell * k);
        for j in 0..cov.h {
            for i in 0..cov.w {
                let d = cov.depth[j * cov.w + i];
                if d > 0.0 {
                    let fill = if d >= margin + c.width * 0.5 { "#f4b0b0".to_string() } else { format!("rgb({0},{0},{0})", (230.0 - d * 40.0).max(120.0) as u8) };
                    s += &format!("<rect x='{}' y='{}' width='{}' height='{}' fill='{fill}'/>", i as f32 * cov.cell * k, j as f32 * cov.cell * k, cov.cell * k, cov.cell * k);
                }
            }
        }
        let poly = |pts: &[(f32, f32)], col: &str, w: f32| {
            let d: Vec<String> = pts.iter().map(|&(x, y)| format!("{:.1},{:.1}", (x - x0) * k, (y - y0) * k)).collect();
            format!("<polyline points='{}' fill='none' stroke='{col}' stroke-width='{w}' stroke-linejoin='round'/>", d.join(" "))
        };
        for (id, l) in &c.strokes {
            s += &poly(l, if c.gone.contains(id) || c.cut.contains(id) { "#1a9a3a" } else { "#2050d0" }, c.width * k);
        }
        s += &poly(&path, "#f08000", 1.5);
        s += "</svg>";
        std::fs::write(file, s).unwrap();
    }

    /// Why cases fail: `N0TE_SCRIBBLE_WHY=<case index> cargo test … why -- --nocapture`.
    #[test]
    fn why() {
        let Ok(k) = std::env::var("N0TE_SCRIBBLE_WHY") else { return };
        let k: usize = k.parse().unwrap_or(0);
        let makers: [fn(&mut Rng) -> Case; 7] = [one_letter, whole_word, joined_letter, colour_in, hatch, circle_twice, write_over_line];
        let mut g = Rng(0x5eed_1234_abcd_0001);
        let (mut shown, mut motionless, mut missed, mut extra) = (0, 0, 0, 0);
        for i in 0..300 {
            let c = makers[k](&mut g);
            let v = judge(&c.gesture, c.width, &targets(&c.strokes, c.width, false));
            if c.ok(&v) {
                continue;
            }
            if v.delete.iter().chain(&v.cut).any(|id| !c.gone.contains(id) && !c.cut.contains(id)) {
                extra += 1;
            } else {
                missed += 1;
            }
            match analyse(&c.gesture, c.width, &targets(&c.strokes, c.width, false)) {
                None => {
                    motionless += 1;
                    if motionless <= 6 {
                        let n = c.gesture.len();
                        let bb = c.gesture.iter().fold(Rect::EMPTY, |a, p| a.include(p.x, p.y));
                        let len: f32 = c.gesture.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).sum();
                        println!("#{i} no motion: n {n}, ms {}, diag {:.1}, len/diag {:.1}", c.gesture[n - 1].t_ms, bb.width().hypot(bb.height()), len / bb.width().hypot(bb.height()));
                    }
                }
                Some(a) if shown < 12 => {
                    if shown < 4 {
                        if let Ok(dir) = std::env::var("N0TE_SCRIBBLE_SVG") {
                            svg(&format!("{dir}/case{k}-{i}.svg"), &c);
                        }
                    }
                    shown += 1;
                    println!("#{i} {:?}\n   covered {:?}\n   area {:.0} ink {:.0} → delete {:?} cut {:?}; wanted gone {:?} cut {:?}", a.motion, a.covered, a.area, a.ink, a.verdict.delete, a.verdict.cut, c.gone, c.cut);
                }
                _ => {}
            }
        }
        println!("failures without scribble motion: {motionless}; missed {missed}, too much {extra}");
    }

    /// How the cases move (for tuning): `N0TE_SCRIBBLE_FEATURES=1 cargo test … features -- --nocapture`.
    #[test]
    fn features() {
        if std::env::var("N0TE_SCRIBBLE_FEATURES").is_err() {
            return;
        }
        for (name, make) in [("joined", joined_letter as fn(&mut Rng) -> Case), ("letter", one_letter), ("write", write_over_line), ("colour", colour_in)] {
            let mut g = Rng(5);
            let mut rows = Vec::new();
            for _ in 0..200 {
                let c = make(&mut g);
                if let Some(m) = motion(&c.gesture) {
                    let hz = m.turns as f32 / (m.ms.max(1) as f32 / 1000.0);
                    rows.push((hz, m.advance, m.spacing / m.diag));
                }
            }
            let q = |v: &mut Vec<f32>, f: f32| { v.sort_by(f32::total_cmp); v[((v.len() - 1) as f32 * f) as usize] };
            let mut hz: Vec<f32> = rows.iter().map(|r| r.0).collect();
            let mut ld: Vec<f32> = rows.iter().map(|r| r.1).collect();
            let mut sd: Vec<f32> = rows.iter().map(|r| r.2).collect();
            println!("{name:8} n={:3} turns/s p10 {:5.1} p50 {:5.1} p90 {:5.1} | advance p10 {:.3} p50 {:.3} p90 {:.3} | spacing/diag p10 {:.3} p50 {:.3} p90 {:.3}", rows.len(),
                q(&mut hz, 0.1), q(&mut hz, 0.5), q(&mut hz, 0.9), q(&mut ld, 0.1), q(&mut ld, 0.5), q(&mut ld, 0.9), q(&mut sd, 0.1), q(&mut sd, 0.5), q(&mut sd, 0.9));
        }
    }

    #[test]
    fn no_target_no_scribble() {
        let mut g = Rng(7);
        let b = Rect::new(0.0, 0.0, 40.0, 12.0);
        let gesture = pen(&zigzag(b, &mut g), 300.0, &mut g);
        assert!(motion(&gesture).is_some());
        assert!(judge(&gesture, 1.0, &[]).is_empty());
        let far = Target::stroke("far", &[InkPoint { x: 500.0, y: 500.0, pressure: 0.5, t_ms: 0 }, InkPoint { x: 520.0, y: 500.0, pressure: 0.5, t_ms: 9 }], 1.0);
        assert!(judge(&gesture, 1.0, &[far]).is_empty());
    }

    #[test]
    fn slow_or_straight_is_not_a_scribble() {
        let line: Vec<InkPoint> = (0..100).map(|i| InkPoint { x: i as f32, y: 0.0, pressure: 0.5, t_ms: i * 4 }).collect();
        assert!(motion(&line).is_none());
        let mut g = Rng(9);
        let mut slow = pen(&zigzag(Rect::new(0.0, 0.0, 40.0, 12.0), &mut g), 300.0, &mut g);
        for (i, p) in slow.iter_mut().enumerate() {
            p.t_ms = i as u32 * 60;
        }
        assert!(motion(&slow).is_none());
    }

    #[test]
    fn a_picture_goes_only_when_mostly_covered() {
        let mut g = Rng(11);
        let pic = Rect::new(100.0, 100.0, 140.0, 130.0);
        let over = pen(&zigzag(pic, &mut g), 900.0, &mut g);
        let v = judge(&over, 1.5, &[Target::boxed("pic", pic)]);
        assert_eq!(v.delete, vec!["pic".to_string()]);
        let corner = pen(&zigzag(Rect::new(95.0, 95.0, 108.0, 105.0), &mut g), 300.0, &mut g);
        assert!(judge(&corner, 1.5, &[Target::boxed("pic", pic)]).is_empty());
    }

    #[test]
    fn distance_transform_is_euclidean() {
        let (w, h) = (9, 9);
        let mut mask = vec![true; w * h];
        mask[4 * w + 4] = false;
        let d = edt(&mask, w, h);
        assert_eq!(d[4 * w + 4], 0.0);
        assert!((d[4 * w + 7] - 3.0).abs() < 1e-4);
        assert!((d[0] - (32.0f32).sqrt()).abs() < 1e-4);
        assert!(judge(&[InkPoint { x: f32::NAN, y: 0.0, pressure: 0.5, t_ms: 0 }; 20], 1.0, &[]).is_empty());
    }
}
