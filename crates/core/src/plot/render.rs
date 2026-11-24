//! A plot as ink: grid, hatched areas, axes with heads, ticks and numbers, curves in their
//! line style, marked points, names at the curves. In page points, drawn like strokes.

use super::expr::Expr;
use super::font::{self, glyph, text_width};
use super::geo::{LabelMode, Obj, PointStyle, Val, fmt_expr, fmt_num, format_val};
use super::points::{self, PointKind, SpecialPoint};
use super::{LineStyle, MARK_CROSSINGS, MARK_EXTREMA, MARK_TURNS, MARK_ZEROS, Plot, format_num};
use crate::ink::{Brush, InkPoint, KIND_PEN};

/// The graph of `e` in page points, clipped to the plot's box: pieces wherever it is defined
/// and doesn't jump (poles, steps).
pub fn curve(e: &Expr, p: &Plot) -> Vec<Vec<(f32, f32)>> {
    if !p.is_valid() {
        return Vec::new();
    }
    let (x0, x1) = (p.x_min, p.x_max);
    let sx = p.w as f64 / (x1 - x0);
    let sy = p.h as f64 / (p.y_max - p.y_min);
    let to_page = |x: f64, y: f64| (p.x as f64 + (x - x0) * sx, p.y as f64 + (p.y_max - y) * sy);
    let n = ((p.w / 3.0) as usize).clamp(48, 400);
    // Samples (x, y) and where the curve breaks (None).
    let mut pts: Vec<Option<(f64, f64)>> = Vec::new();
    /// How finely to sample: page scale, how far off the straight line a piece may be, the
    /// largest step that is still a line at the finest level, the y band that is seen.
    struct Ctx {
        sx: f64,
        sy: f64,
        tol: f64,
        jump: f64,
        lo: f64,
        hi: f64,
    }
    let margin = (p.y_max - p.y_min) * 0.05;
    let cx = Ctx { sx, sy, tol: 0.12, jump: (p.h as f64 * 0.35).min(1.5), lo: p.y_min - margin, hi: p.y_max + margin };
    fn refine(e: &Expr, a: (f64, f64), b: (f64, f64), depth: u32, out: &mut Vec<Option<(f64, f64)>>, c: &Ctx) {
        let (ax, ay) = a;
        let (bx, by) = b;
        let mx = (ax + bx) / 2.0;
        let my = e.eval(mx);
        let (fa, fb, fm) = (ay.is_finite(), by.is_finite(), my.is_finite());
        if depth >= 12 {
            // Can't split further: a jump this steep is a pole or a step, not a line.
            if fa && fb && ((ay - by) * c.sy).abs() < c.jump {
                out.push(Some(b));
            } else {
                out.push(None);
                if fb {
                    out.push(Some(b));
                }
            }
            return;
        }
        let split = if fa && fb && fm {
            // Bent, or long where it is seen (dense points draw a smooth pen line).
            let seen = !(ay < c.lo && by < c.lo) && !(ay > c.hi && by > c.hi);
            ((my - (ay + by) / 2.0) * c.sy).abs() > c.tol || (seen && ((bx - ax) * c.sx).hypot((by - ay) * c.sy) > 4.0)
        } else {
            // Where it starts or stops being defined: find the edge.
            fa || fb || fm
        };
        if split {
            refine(e, a, (mx, my), depth + 1, out, c);
            refine(e, (mx, my), b, depth + 1, out, c);
        } else if fb {
            out.push(Some(b));
        } else {
            out.push(None);
        }
    }
    let first = (x0, e.eval(x0));
    pts.push(first.1.is_finite().then_some(first));
    let mut prev = first;
    for i in 1..=n {
        let x = x0 + (x1 - x0) * i as f64 / n as f64;
        let cur = (x, e.eval(x));
        refine(e, prev, cur, 0, &mut pts, &cx);
        prev = cur;
    }
    // Into page points, clipped to the box (a little over the edges for the pen's round end).
    let (top, bottom) = (p.y as f64 - 0.5, (p.y + p.h) as f64 + 0.5);
    let mut out: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut cur: Vec<(f32, f32)> = Vec::new();
    let mut last: Option<(f64, f64)> = None;
    let flush = |cur: &mut Vec<(f32, f32)>, out: &mut Vec<Vec<(f32, f32)>>| {
        if cur.len() >= 2 {
            out.push(std::mem::take(cur));
        } else {
            cur.clear();
        }
    };
    for s in pts {
        let Some((x, y)) = s else {
            flush(&mut cur, &mut out);
            last = None;
            continue;
        };
        let q = to_page(x, y);
        if let Some(pq) = last {
            // The segment pq → q, clipped to the band top..bottom.
            let (mut ax, mut ay, mut bx, mut by) = (pq.0, pq.1, q.0, q.1);
            let inside = |y: f64| y >= top && y <= bottom;
            if !(ay < top && by < top) && !(ay > bottom && by > bottom) {
                let cut = |ax: f64, ay: f64, bx: f64, by: f64, yy: f64| ax + (bx - ax) * (yy - ay) / (by - ay);
                let a_in = inside(ay);
                if !a_in {
                    let yy = if ay < top { top } else { bottom };
                    ax = cut(ax, ay, bx, by, yy);
                    ay = yy;
                }
                if !inside(by) {
                    let yy = if by < top { top } else { bottom };
                    bx = cut(pq.0, pq.1, bx, by, yy);
                    by = yy;
                }
                if !a_in {
                    flush(&mut cur, &mut out);
                    cur.push((ax as f32, ay as f32));
                } else if cur.is_empty() {
                    cur.push((ax as f32, ay as f32));
                }
                cur.push((bx as f32, by as f32));
                if !inside(q.1) {
                    flush(&mut cur, &mut out);
                }
            } else {
                flush(&mut cur, &mut out);
            }
        } else if q.1 >= top && q.1 <= bottom {
            cur.push((q.0 as f32, q.1 as f32));
        }
        last = Some(q);
    }
    flush(&mut cur, &mut out);
    out
}


/// A tick step of 1, 2 or 5 × 10^k that cuts `range` into about `count` parts.
pub fn nice_step(range: f64, count: f64) -> f64 {
    let raw = (range / count.max(1.0)).abs().max(1e-12);
    let mag = 10f64.powf(raw.log10().floor());
    for m in [1.0, 2.0, 5.0, 10.0] {
        if m * mag >= raw * 0.999 {
            return m * mag;
        }
    }
    10.0 * mag
}

/// A tick's number as written: German decimal comma, as many places as the step needs.
pub fn tick_label(v: f64, step: f64) -> String {
    let places = if step >= 0.999 { 0 } else { (-step.log10()).ceil().max(0.0) as usize };
    let v = if v.abs() < step * 1e-6 { 0.0 } else { v };
    let s = format!("{v:.places$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    s.replace('.', ",")
}

/// A tick at `k` · π / `d` as written: π/2, π, 3π/2, −π, 2π.
pub fn pi_label(k: i64, d: i64) -> String {
    if k == 0 {
        return "0".into();
    }
    let g = gcd(k.abs(), d);
    let (n, d) = (k / g, d / g);
    let sign = if n < 0 { "-" } else { "" };
    let n = n.abs();
    let num = if n == 1 { "π".to_string() } else { format!("{n}π") };
    if d == 1 { format!("{sign}{num}") } else { format!("{sign}{num}/{d}") }
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.max(1) } else { gcd(b, a % b) }
}

/// Ticks on an axis: (step, values, labels). With `pi`, steps are π/4, π/2, π, 2π … and the
/// numbers say so.
pub(crate) fn ticks_of(lo: f64, hi: f64, len: f32, pi: bool) -> (f64, Vec<f64>, Vec<String>) {
    let want = (len / 14.0).max(1.0) as f64;
    if pi {
        let unit = std::f64::consts::PI;
        let raw = (hi - lo) / want / unit;
        let (k, d) = [(1, 4), (1, 2), (1, 1), (2, 1), (5, 1), (10, 1), (20, 1), (50, 1)].into_iter().find(|(k, d)| *k as f64 / *d as f64 >= raw * 0.999).unwrap_or((100, 1));
        let step = unit * k as f64 / d as f64;
        let first = (lo / step).ceil() as i64;
        let last = (hi / step).floor() as i64;
        let vs: Vec<i64> = (first..=last).take(400).collect();
        return (step, vs.iter().map(|&i| i as f64 * step).collect(), vs.iter().map(|&i| pi_label(i * k, d)).collect());
    }
    let step = nice_step(hi - lo, want);
    let first = (lo / step).ceil() as i64;
    let last = (hi / step).floor() as i64;
    let vs: Vec<f64> = (first..=last).take(400).map(|i| i as f64 * step).collect();
    let labels = vs.iter().map(|&v| tick_label(v, step)).collect();
    (step, vs, labels)
}

// --- the hand ---------------------------------------------------------------------------------

/// Small random numbers, the same for the same plot every time (its look doesn't flicker).
struct Rng(u64);

impl Rng {
    fn new(seed: &str, salt: u64) -> Rng {
        let mut h = 0xcbf2_9ce4_8422_2325u64 ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        for b in seed.bytes() {
            h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
        }
        Rng(h)
    }
    /// −1 … 1.
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        (z >> 11) as f32 / (1u64 << 53) as f32 * 2.0 - 1.0
    }
}

/// How the hand draws: a line wanders a little sideways and the pressure breathes along it,
/// lightest where the pen lands and lifts.
struct Hand {
    clean: bool,
    rng: Rng,
}

impl Hand {
    fn pressure(&mut self, along: &[f32], total: f32) -> Vec<f32> {
        if self.clean {
            return vec![0.5; along.len()];
        }
        let (ph, wl) = (self.rng.next() * 3.0, 60.0 + self.rng.next() * 25.0);
        along
            .iter()
            .map(|&d| {
                let ends = (d.min(total - d) / 2.5).clamp(0.0, 1.0);
                (0.5 + 0.07 * (d / wl * std::f32::consts::TAU + ph).sin()) * (0.72 + 0.28 * ends)
            })
            .collect()
    }

    /// The points along `path` with the hand's pressure (and, when `wobble` > 0, its sway).
    fn stroke(&mut self, path: &[(f32, f32)], wobble: f32) -> Vec<InkPoint> {
        let mut along = Vec::with_capacity(path.len());
        let mut d = 0.0;
        for (i, p) in path.iter().enumerate() {
            if i > 0 {
                d += (p.0 - path[i - 1].0).hypot(p.1 - path[i - 1].1);
            }
            along.push(d);
        }
        let pr = self.pressure(&along, d);
        let (a1, l1, p1) = (self.rng.next(), 70.0 + self.rng.next() * 30.0, self.rng.next() * 3.0);
        let (a2, l2, p2) = (self.rng.next(), 23.0 + self.rng.next() * 8.0, self.rng.next() * 3.0);
        let sway = if self.clean { 0.0 } else { wobble };
        path.iter()
            .enumerate()
            .map(|(i, &(x, y))| {
                let (nx, ny) = if path.len() < 2 {
                    (0.0, 0.0)
                } else {
                    let (a, b) = (path[i.saturating_sub(1)], path[(i + 1).min(path.len() - 1)]);
                    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                    let l = dx.hypot(dy).max(1e-6);
                    (-dy / l, dx / l)
                };
                let t = along[i];
                // The sway fades out at the ends (where the pen aims).
                let env = (t.min(d - t) / 6.0).clamp(0.0, 1.0);
                let off = sway * env * (0.65 * a1.signum() * (t / l1 * std::f32::consts::TAU + p1).sin() + 0.35 * a2 * (t / l2 * std::f32::consts::TAU + p2).sin());
                InkPoint { x: x + nx * off, y: y + ny * off, pressure: pr[i], t_ms: i as u32 * 8 }
            })
            .collect()
    }

    /// A straight line from `a` to `b`, sampled for the hand's sway.
    fn line(&mut self, a: (f32, f32), b: (f32, f32), wobble: f32) -> Vec<InkPoint> {
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        let n = ((len / 3.0) as usize).max(1);
        let path: Vec<(f32, f32)> = (0..=n).map(|i| (a.0 + (b.0 - a.0) * i as f32 / n as f32, a.1 + (b.1 - a.1) * i as f32 / n as f32)).collect();
        self.stroke(&path, wobble)
    }

    /// `text` with its top-left at (x, y), each glyph a little different (slant and size wander);
    /// `_1` written low and small, `²` and `^2` high and small.
    fn write(&mut self, text: &str, x: f32, y: f32, cap: f32) -> Vec<Vec<InkPoint>> {
        let mut out = Vec::new();
        let mut ox = x;
        for (c, size, lift) in font::runs(text) {
            let Some((w, lines)) = glyph(c) else { continue };
            let cs = cap * size;
            let (slant, k, dy) = if self.clean { (0.08, 1.0, 0.0) } else { (0.08 + self.rng.next() * 0.05, 1.0 + self.rng.next() * 0.04, self.rng.next() * 0.03) };
            let top = y + lift * cap;
            for l in lines {
                let path: Vec<(f32, f32)> = dense(&l, 0.06).iter().map(|&(gx, gy)| (ox + (gx + (1.0 - gy) * slant) * cs * k, top + (gy + dy) * cs * k)).collect();
                out.push(self.stroke(&path, 0.0));
            }
            ox += w * cs + font::GAP * cs;
        }
        out
    }
}

/// `path` with points at most `step` apart and every corner (a turn of over 25°) tripled:
/// the core's smoothing rounds the corners of a sparse path away (a 4 would lose its point).
pub(crate) fn dense(path: &[(f32, f32)], step: f32) -> Vec<(f32, f32)> {
    let mut out = Vec::with_capacity(path.len() * 2);
    for (i, &q) in path.iter().enumerate() {
        if i > 0 {
            let p = path[i - 1];
            let n = ((q.0 - p.0).hypot(q.1 - p.1) / step).ceil().max(1.0) as usize;
            for k in 1..n {
                let t = k as f32 / n as f32;
                out.push((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t));
            }
        }
        out.push(q);
        if i > 0 && i + 1 < path.len() {
            let (p, r) = (path[i - 1], path[i + 1]);
            let (a, b) = ((q.0 - p.0, q.1 - p.1), (r.0 - q.0, r.1 - q.1));
            let (la, lb) = (a.0.hypot(a.1), b.0.hypot(b.1));
            if la > 1e-6 && lb > 1e-6 && (a.0 * b.0 + a.1 * b.1) / (la * lb) < 25f32.to_radians().cos() {
                out.push(q);
                out.push(q);
            }
        }
    }
    out
}

/// `path` cut into dashes (or dots) along its length.
fn dashes(path: &[(f32, f32)], style: LineStyle, width: f32) -> Vec<Vec<(f32, f32)>> {
    let (on, off) = match style {
        LineStyle::Solid => return vec![path.to_vec()],
        LineStyle::Dashed => (5.0 + width * 2.0, 3.0 + width * 1.5),
        LineStyle::Dotted => (0.05, 2.2 + width * 2.0),
    };
    let mut out = Vec::new();
    let mut cur: Vec<(f32, f32)> = Vec::new();
    let mut drawing = true;
    let mut left = on;
    for w in path.windows(2) {
        let (mut a, b) = (w[0], w[1]);
        let mut seg = (b.0 - a.0).hypot(b.1 - a.1);
        if drawing && cur.is_empty() {
            cur.push(a);
        }
        while seg > 1e-6 {
            let step = seg.min(left);
            let t = step / seg;
            let q = (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t);
            if drawing {
                cur.push(q);
            }
            left -= step;
            seg -= step;
            a = q;
            if left <= 1e-6 {
                if drawing {
                    out.push(std::mem::take(&mut cur));
                }
                drawing = !drawing;
                left = if drawing { on } else { off };
                if drawing {
                    cur.push(a);
                }
            }
        }
    }
    if drawing && cur.len() >= 2 {
        out.push(cur);
    }
    out
}

/// Hatching inside a region of the page: lines at 45° rising to the right, `gap` apart across,
/// cut to where `inside` holds (in the box `bx`: x0, y0, x1, y1).
fn hatch_where(bx: (f32, f32, f32, f32), gap: f32, inside: &dyn Fn(f32, f32) -> bool) -> Vec<((f32, f32), (f32, f32))> {
    let (x0, y0, x1, y1) = bx;
    let mut out = Vec::new();
    if x1 <= x0 || y1 <= y0 {
        return out;
    }
    let d = gap * std::f32::consts::SQRT_2;
    let mut c = x0 + y0;
    while c <= x1 + y1 {
        let mut run: Option<(f32, f32)> = None;
        let mut last = (0.0, 0.0);
        let mut x = x0;
        while x <= x1 + 1e-3 {
            let y = c - x;
            if y >= y0 && y <= y1 && inside(x, y) {
                if run.is_none() {
                    run = Some((x, y));
                }
                last = (x, y);
            } else if let Some(s) = run.take()
                && (last.0 - s.0) > 0.6
            {
                out.push((s, last));
            }
            x += 0.4;
        }
        if let Some(s) = run
            && (last.0 - s.0) > 0.6
        {
            out.push((s, last));
        }
        c += d;
    }
    out
}

/// Whether (x, y) lies inside the polygon (page points).
fn in_poly(x: f32, y: f32, ps: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let n = ps.len();
    let mut j = n.wrapping_sub(1);
    for i in 0..n {
        let (xi, yi) = ps[i];
        let (xj, yj) = ps[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// The runs of a page path that lie inside the box (a little over its edges).
fn clip_path(path: &[(f32, f32)], bx: (f32, f32, f32, f32)) -> Vec<Vec<(f32, f32)>> {
    let m = 0.5;
    let inside = |q: &(f32, f32)| q.0.is_finite() && q.1.is_finite() && q.0 >= bx.0 - m && q.0 <= bx.2 + m && q.1 >= bx.1 - m && q.1 <= bx.3 + m;
    let mut out = Vec::new();
    let mut cur: Vec<(f32, f32)> = Vec::new();
    for q in path {
        if inside(q) {
            cur.push(*q);
        } else if !cur.is_empty() {
            if cur.len() >= 2 {
                out.push(std::mem::take(&mut cur));
            } else {
                cur.clear();
            }
        }
    }
    if cur.len() >= 2 {
        out.push(cur);
    }
    out
}

/// The part of the line through `p` along `d` (plot units) with parameter in [t0, t1] that lies
/// in the view, as a page segment.
fn clip_line(p: &Plot, o: (f64, f64), d: (f64, f64), t0: f64, t1: f64) -> Option<((f32, f32), (f32, f32))> {
    let (mut lo, mut hi) = (t0, t1);
    for (pc, dc, min, max) in [(o.0, d.0, p.x_min, p.x_max), (o.1, d.1, p.y_min, p.y_max)] {
        if dc.abs() < 1e-15 {
            if pc < min || pc > max {
                return None;
            }
            continue;
        }
        let (a, b) = ((min - pc) / dc, (max - pc) / dc);
        lo = lo.max(a.min(b));
        hi = hi.min(a.max(b));
    }
    if hi <= lo {
        return None;
    }
    Some((p.to_page(o.0 + d.0 * lo, o.1 + d.1 * lo), p.to_page(o.0 + d.0 * hi, o.1 + d.1 * hi)))
}

/// F(x, y) = 0 traced over the view (marching squares), as page polylines.
fn implicit_paths(p: &Plot, f: &Expr) -> Vec<Vec<(f32, f32)>> {
    let nx = ((p.w / 2.0) as usize).clamp(40, 240);
    let ny = ((p.h / 2.0) as usize).clamp(30, 240);
    let (dx, dy) = ((p.x_max - p.x_min) / nx as f64, (p.y_max - p.y_min) / ny as f64);
    let g: Vec<Vec<f64>> = (0..=ny).map(|j| (0..=nx).map(|i| f.eval2(p.x_min + i as f64 * dx, p.y_min + j as f64 * dy)).collect()).collect();
    let mut segs: Vec<((f64, f64), (f64, f64))> = Vec::new();
    for j in 0..ny {
        for i in 0..nx {
            let (x0, y0) = (p.x_min + i as f64 * dx, p.y_min + j as f64 * dy);
            let v = [g[j][i], g[j][i + 1], g[j + 1][i + 1], g[j + 1][i]];
            if v.iter().any(|x| !x.is_finite()) {
                continue;
            }
            let corners = [(x0, y0), (x0 + dx, y0), (x0 + dx, y0 + dy), (x0, y0 + dy)];
            let mut cross = Vec::new();
            for e in 0..4 {
                let (a, b) = (e, (e + 1) % 4);
                if (v[a] < 0.0) != (v[b] < 0.0) {
                    let t = v[a] / (v[a] - v[b]);
                    cross.push((corners[a].0 + (corners[b].0 - corners[a].0) * t, corners[a].1 + (corners[b].1 - corners[a].1) * t));
                }
            }
            if cross.len() == 2 {
                segs.push((cross[0], cross[1]));
            } else if cross.len() == 4 {
                segs.push((cross[0], cross[1]));
                segs.push((cross[2], cross[3]));
            }
        }
    }
    // Joined into paths where their ends meet.
    let key = |q: (f64, f64)| ((q.0 / dx * 1e3).round() as i64, (q.1 / dy * 1e3).round() as i64);
    let mut by_end: std::collections::HashMap<(i64, i64), Vec<usize>> = std::collections::HashMap::new();
    for (k, s) in segs.iter().enumerate() {
        by_end.entry(key(s.0)).or_default().push(k);
        by_end.entry(key(s.1)).or_default().push(k);
    }
    let mut used = vec![false; segs.len()];
    let mut out = Vec::new();
    for start in 0..segs.len() {
        if used[start] {
            continue;
        }
        used[start] = true;
        let mut path = std::collections::VecDeque::from(vec![segs[start].0, segs[start].1]);
        for forward in [true, false] {
            loop {
                let end = if forward { *path.back().unwrap() } else { *path.front().unwrap() };
                let Some(&next) = by_end.get(&key(end)).and_then(|v| v.iter().find(|&&k| !used[k])) else { break };
                used[next] = true;
                let s = segs[next];
                let other = if key(s.0) == key(end) { s.1 } else { s.0 };
                if forward {
                    path.push_back(other);
                } else {
                    path.push_front(other);
                }
            }
        }
        out.push(path.into_iter().map(|q| p.to_page(q.0, q.1)).collect());
    }
    out
}

/// Where the n-th visible slider of a plot sits (page points): track from x0 to x1 at y, and its
/// label's top-left. Sliders stack down the top-left corner unless placed.
pub fn slider_track(p: &Plot, i: usize) -> Option<(f32, f32, f32)> {
    let o = p.objs.get(i)?;
    o.style.slider?;
    if !o.style.visible {
        return None;
    }
    let cap = (p.w.min(p.h) * 0.045).clamp(5.0, 8.0);
    let len = (p.w * 0.3).clamp(40.0, 120.0);
    if let Some((fx, fy)) = o.style.at {
        let x0 = p.x + fx * p.w;
        let y = p.y + fy * p.h;
        return Some((x0, x0 + len, y));
    }
    let k = p.objs[..i].iter().filter(|q| q.style.slider.is_some() && q.style.visible && q.style.at.is_none()).count();
    let x0 = p.x + cap * 1.6;
    let y = p.y + cap * 3.0 + k as f32 * cap * 3.4;
    Some((x0, x0 + len, y))
}

/// The object under a page point (within `tol` page points): points and sliders first, then
/// lines and curves, then filled shapes. Its index.
pub fn hit(p: &Plot, x: f32, y: f32, tol: f32) -> Option<usize> {
    if !p.is_valid() {
        return None;
    }
    let vals = p.values();
    let seg_dist = |a: (f32, f32), b: (f32, f32)| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let l2 = (dx * dx + dy * dy).max(1e-9);
        let t = (((x - a.0) * dx + (y - a.1) * dy) / l2).clamp(0.0, 1.0);
        (x - a.0 - dx * t).hypot(y - a.1 - dy * t)
    };
    let path_dist = |path: &[(f32, f32)]| path.windows(2).map(|w| seg_dist(w[0], w[1])).fold(f32::MAX, f32::min);
    let mut best: Option<(u8, f32, usize)> = None;
    let consider = |rank: u8, d: f32, i: usize, best: &mut Option<(u8, f32, usize)>| {
        if d <= tol && best.is_none_or(|b| (rank, d) < (b.0, b.1)) {
            *best = Some((rank, d, i));
        }
    };
    let big = ((p.x_max - p.x_min).abs() + (p.y_max - p.y_min).abs()) * 4.0;
    for (i, o) in p.objs.iter().enumerate() {
        if !o.style.visible {
            continue;
        }
        if let Some((x0, x1, sy)) = slider_track(p, i) {
            consider(0, seg_dist((x0, sy), (x1, sy)), i, &mut best);
            continue;
        }
        let Ok(v) = &vals[i] else { continue };
        match v {
            Val::Point(q) => {
                let (qx, qy) = p.to_page(q.0, q.1);
                consider(0, (qx - x).hypot(qy - y) * 0.7, i, &mut best);
            }
            Val::Points(qs) => {
                for q in qs {
                    let (qx, qy) = p.to_page(q.0, q.1);
                    consider(0, (qx - x).hypot(qy - y) * 0.7, i, &mut best);
                }
            }
            Val::Func(e) => {
                for piece in curve(e, p) {
                    consider(1, path_dist(&piece), i, &mut best);
                }
            }
            Val::Line { a, b, c } => {
                let n2 = a * a + b * b;
                let o0 = (a * c / n2, b * c / n2);
                let l = n2.sqrt();
                if let Some((s, e)) = clip_line(p, o0, (-b / l, a / l), -big - o0.0.abs() - o0.1.abs(), big + o0.0.abs() + o0.1.abs()) {
                    consider(1, seg_dist(s, e), i, &mut best);
                }
            }
            Val::Lines(ls) => {
                for (a, b, c) in ls {
                    let n2 = a * a + b * b;
                    let o0 = (a * c / n2, b * c / n2);
                    let l = n2.sqrt();
                    if let Some((s, e)) = clip_line(p, o0, (-b / l, a / l), -big, big) {
                        consider(1, seg_dist(s, e), i, &mut best);
                    }
                }
            }
            Val::Segment(a, c) | Val::Ray(a, c) => {
                let d = (c.0 - a.0, c.1 - a.1);
                let t1 = if matches!(v, Val::Ray(..)) { big / d.0.hypot(d.1).max(1e-12) } else { 1.0 };
                if let Some((s, e)) = clip_line(p, *a, d, 0.0, t1) {
                    consider(1, seg_dist(s, e), i, &mut best);
                }
            }
            Val::Vector { from, d } => {
                if let Some((s, e)) = clip_line(p, *from, *d, 0.0, 1.0) {
                    consider(1, seg_dist(s, e), i, &mut best);
                }
            }
            Val::Circle(c, r) => {
                let path: Vec<(f32, f32)> = (0..=120).map(|k| {
                    let a = k as f64 / 120.0 * std::f64::consts::TAU;
                    p.to_page(c.0 + r * a.cos(), c.1 + r * a.sin())
                }).collect();
                consider(1, path_dist(&path), i, &mut best);
            }
            Val::Curve(fx, fy, a, c) => {
                let path: Vec<(f32, f32)> = (0..=300).map(|k| {
                    let t = a + (c - a) * k as f64 / 300.0;
                    p.to_page(fx.eval(t), fy.eval(t))
                }).collect();
                consider(1, path_dist(&path), i, &mut best);
            }
            Val::Implicit(f) => {
                for path in implicit_paths(p, f) {
                    consider(1, path_dist(&path), i, &mut best);
                }
            }
            Val::Polygon(ps) => {
                let pp: Vec<(f32, f32)> = ps.iter().map(|q| p.to_page(q.0, q.1)).collect();
                let mut closed = pp.clone();
                closed.push(pp[0]);
                consider(1, path_dist(&closed), i, &mut best);
                if in_poly(x, y, &pp) {
                    consider(2, 0.0, i, &mut best);
                }
            }
            Val::Angle { at, .. } => {
                let (qx, qy) = p.to_page(at.0, at.1);
                let cap = (p.w.min(p.h) * 0.045).clamp(5.0, 8.0);
                consider(2, ((qx - x).hypot(qy - y) - cap * 2.0).abs(), i, &mut best);
            }
            Val::Area { f, g, a, b, .. } => {
                let xv = p.to_plot(x, y).0;
                if xv >= a.min(*b) && xv <= a.max(*b) {
                    let (ya, yb) = (f.eval(xv), g.as_ref().map_or(0.0, |g| g.eval(xv)));
                    let (pa, pb) = (p.to_page(xv, ya).1, p.to_page(xv, yb).1);
                    if y >= pa.min(pb) && y <= pa.max(pb) {
                        consider(2, 0.0, i, &mut best);
                    }
                }
            }
            Val::Text(_) => {
                let (fx, fy) = o.style.at.unwrap_or((0.04, 0.06));
                let (tx, ty) = (p.x + fx * p.w, p.y + fy * p.h);
                let cap = (p.w.min(p.h) * 0.045).clamp(5.0, 8.0);
                if x >= tx - 2.0 && x <= tx + 120.0 && y >= ty - 2.0 && y <= ty + cap * 1.5 {
                    consider(2, 0.0, i, &mut best);
                }
            }
            _ => {}
        }
    }
    best.map(|b| b.2)
}

/// What an object's label says (None: no label).
fn label_text(o: &Obj, v: &Val) -> Option<String> {
    let name = o.name();
    let value = match v {
        Val::Func(e) => fmt_expr(e),
        other => format_val(other),
    };
    Some(match o.style.label {
        LabelMode::Off => return None,
        LabelMode::Name => name,
        LabelMode::Value => value,
        LabelMode::Caption => {
            if o.style.caption.trim().is_empty() {
                name
            } else {
                o.style.caption.clone()
            }
        }
        LabelMode::NameValue => match v {
            Val::Func(_) => format!("{name}(x) = {value}"),
            Val::Line { .. } | Val::Lines(_) | Val::Circle(..) | Val::Implicit(_) | Val::Curve(..) => format!("{name}: {value}"),
            Val::Points(_) => format!("{name} = {{{value}}}"),
            Val::Text(t) => t.clone(),
            _ => format!("{name} = {value}"),
        },
    })
}

/// A label to write (after everything is drawn, so it keeps off the ink).
struct Label {
    x: f32,
    y: f32,
    off: f32,
    text: String,
    color: u32,
}

/// The plot as ink in page points, under to top: grid, fills, axes with ticks and numbers,
/// lines and curves, points, sliders and texts, labels.
pub fn render(p: &Plot) -> Vec<(Brush, Vec<InkPoint>)> {
    let mut out: Vec<(Brush, Vec<InkPoint>)> = Vec::new();
    if !p.is_valid() {
        return out;
    }
    let mut hand = Hand { clean: p.clean, rng: Rng::new(&p.id, 1) };
    let thin = if p.clean { 0.0 } else { 0.35 };
    let w = p.width.clamp(0.3, 6.0);
    let axis = Brush { width: (w * 0.8).clamp(0.35, 3.0), thinning: thin, color: p.color, kind: KIND_PEN };
    let small = p.w.min(p.h);
    let cap = (small * 0.045).clamp(5.0, 8.0);
    let label = Brush { width: (cap * 0.13).clamp(0.35, w.max(0.5)), thinning: thin, color: p.color, kind: KIND_PEN };
    let grid = Brush { width: 0.3, thinning: 0.0, color: (p.color & 0x00FF_FFFF) | 0x4000_0000, kind: KIND_PEN };
    let minor = Brush { width: 0.2, thinning: 0.0, color: (p.color & 0x00FF_FFFF) | 0x2000_0000, kind: KIND_PEN };
    let (x0, y0, x1, y1) = (p.x, p.y, p.x + p.w, p.y + p.h);
    let bx = (x0, y0, x1, y1);
    let px = |v: f64| p.to_page(v, 0.0).0;
    let py = |v: f64| p.to_page(0.0, v).1;
    let ax_y = if p.y_min <= 0.0 && p.y_max >= 0.0 { py(0.0) } else if p.y_max < 0.0 { y0 } else { y1 };
    let ax_x = if p.x_min <= 0.0 && p.x_max >= 0.0 { px(0.0) } else if p.x_max < 0.0 { x1 } else { x0 };
    let (sx, xs, xl) = ticks_of(p.x_min, p.x_max, p.w, p.pi);
    let (sy, ys, yl) = ticks_of(p.y_min, p.y_max, p.h, false);
    if p.grid {
        if p.minor {
            for k in 0..((p.x_max - p.x_min) / sx * 5.0).min(400.0) as i64 + 1 {
                let v = (p.x_min / (sx / 5.0)).ceil() * (sx / 5.0) + k as f64 * sx / 5.0;
                if v <= p.x_max {
                    out.push((minor, hand.line((px(v), y0), (px(v), y1), 0.1)));
                }
            }
            for k in 0..((p.y_max - p.y_min) / sy * 5.0).min(400.0) as i64 + 1 {
                let v = (p.y_min / (sy / 5.0)).ceil() * (sy / 5.0) + k as f64 * sy / 5.0;
                if v <= p.y_max {
                    out.push((minor, hand.line((x0, py(v)), (x1, py(v)), 0.1)));
                }
            }
        }
        for &v in &xs {
            out.push((grid, hand.line((px(v), y0), (px(v), y1), 0.15)));
        }
        for &v in &ys {
            out.push((grid, hand.line((x0, py(v)), (x1, py(v)), 0.15)));
        }
    }
    let vals = p.values();
    let shown = |i: usize| p.objs[i].style.visible;
    let pen = |o: &Obj, k: f32| Brush { width: if o.style.width > 0.0 { o.style.width } else { w * k }, thinning: thin, color: o.style.color, kind: KIND_PEN };
    let hatch_pen = |o: &Obj| Brush { width: (w * 0.42).clamp(0.3, 1.0), thinning: 0.0, color: (o.style.color & 0x00FF_FFFF) | 0xB000_0000, kind: KIND_PEN };
    let gap_of = |o: &Obj| (cap * 0.95 * (1.0 - o.style.fill.clamp(0.0, 1.0) * 0.62)).max(1.6);
    let mut labels: Vec<Label> = Vec::new();
    let mut ink: Vec<(f32, f32)> = Vec::new();
    // Where words are written already (axis numbers and names, sliders, texts): labels keep off.
    let mut reserved: Vec<(f32, f32, f32, f32)> = Vec::new();
    // Fills first: areas, polygons, circles, angles.
    for (i, o) in p.objs.iter().enumerate() {
        let Ok(v) = &vals[i] else { continue };
        if !shown(i) {
            continue;
        }
        let fill = o.style.fill > 0.01;
        match v {
            Val::Area { f, g, a, b, value } => {
                let (lo, hi) = (a.min(*b).max(p.x_min), a.max(*b).min(p.x_max));
                if hi > lo {
                    let (fx0, fx1) = (px(lo), px(hi));
                    let inside = |x: f32, y: f32| {
                        let xv = p.to_plot(x, y).0;
                        let (ya, yb) = (f.eval(xv), g.as_ref().map_or(0.0, |g| g.eval(xv)));
                        if !ya.is_finite() || !yb.is_finite() {
                            return false;
                        }
                        let (pa, pb) = (py(ya), py(yb));
                        y >= pa.min(pb) && y <= pa.max(pb)
                    };
                    for (s, e) in hatch_where((fx0, y0, fx1, y1), gap_of(o).min(cap * 0.6), &inside) {
                        out.push((hatch_pen(o), hand.line(s, e, 0.1)));
                    }
                    let m = (lo + hi) / 2.0;
                    let ym = (f.eval(m) + g.as_ref().map_or(0.0, |g| g.eval(m))) / 2.0;
                    if let Some(t) = label_text(o, &Val::Num(*value))
                        && ym.is_finite()
                    {
                        labels.push(Label { x: px(m), y: py(ym.clamp(p.y_min, p.y_max)), off: 0.0, text: t, color: o.style.color });
                    }
                }
            }
            Val::Polygon(ps) if fill => {
                let pp: Vec<(f32, f32)> = ps.iter().map(|q| p.to_page(q.0, q.1)).collect();
                let (lx, ly, hx, hy) = pp.iter().fold((f32::MAX, f32::MAX, f32::MIN, f32::MIN), |a, q| (a.0.min(q.0), a.1.min(q.1), a.2.max(q.0), a.3.max(q.1)));
                for (s, e) in hatch_where((lx.max(x0), ly.max(y0), hx.min(x1), hy.min(y1)), gap_of(o), &|x, y| in_poly(x, y, &pp)) {
                    out.push((hatch_pen(o), hand.line(s, e, 0.1)));
                }
            }
            Val::Circle(c, r) if fill => {
                let (cx, cy) = p.to_page(c.0, c.1);
                let (ux, uy) = p.units_per_pt();
                let (rx, ry) = ((r / ux) as f32, (r / uy) as f32);
                let inside = |x: f32, y: f32| ((x - cx) / rx).powi(2) + ((y - cy) / ry).powi(2) <= 1.0;
                for (s, e) in hatch_where(((cx - rx).max(x0), (cy - ry).max(y0), (cx + rx).min(x1), (cy + ry).min(y1)), gap_of(o), &inside) {
                    out.push((hatch_pen(o), hand.line(s, e, 0.1)));
                }
            }
            _ => {}
        }
    }
    // Axes, each with a head at its positive end.
    let wob = 0.35;
    let head = (small * 0.035).clamp(3.5, 7.0);
    let tick = cap * 0.45;
    let gap = cap * 0.4;
    let tip_x = (x1 + head * 0.4, ax_y);
    let tip_y = (ax_x, y0 - head * 0.4);
    if p.axes {
        out.push((axis, hand.line((x0, ax_y), tip_x, wob)));
        out.push((axis, hand.line((ax_x, y1), tip_y, wob)));
        let sa = (26f32).to_radians();
        for (tip, dir) in [(tip_x, (1.0f32, 0.0f32)), (tip_y, (0.0, -1.0))] {
            for side in [-1.0f32, 1.0] {
                let (c, s) = (sa.cos(), sa.sin() * side);
                let back = (-dir.0 * c + dir.1 * s, -dir.1 * c - dir.0 * s);
                out.push((axis, hand.line(tip, (tip.0 + back.0 * head, tip.1 + back.1 * head), 0.0)));
            }
        }
        let mut t = x0;
        while t <= x1 {
            ink.push((t, ax_y));
            t += 1.5;
        }
        let mut t = y0;
        while t <= y1 {
            ink.push((ax_x, t));
            t += 1.5;
        }
        // Ticks and their numbers (every one that has room; 0 not where the other axis
        // crosses; none at an axis' end, where its head and name are).
        let zero_x = p.x_min < 0.0 && p.x_max > 0.0;
        let zero_y = p.y_min < 0.0 && p.y_max > 0.0;
        let widest = xl.iter().map(|t| text_width(t, cap)).fold(0.0f32, f32::max);
        let step_pt = (sx / (p.x_max - p.x_min) * p.w as f64) as f32;
        let every_x = [1i64, 2, 4, 5, 10, 20, 50].into_iter().find(|k| *k as f32 * step_pt >= widest + cap * 0.9).unwrap_or(100);
        let below = ax_y + tick + gap;
        for (v, t) in xs.iter().zip(&xl) {
            let x = px(*v);
            if (x - ax_x).abs() > 0.5 || !zero_x {
                out.push((axis, hand.line((x, ax_y - tick), (x, ax_y + tick), 0.0)));
            }
            let i = (v / sx).round() as i64;
            if i % every_x != 0 || (v.abs() < sx * 1e-6 && zero_y) || x > x1 - cap * 1.1 {
                continue;
            }
            let tw = text_width(t, cap);
            let shift = if t.starts_with('-') { text_width("-", cap) * 0.5 + 0.08 * cap } else { 0.0 };
            reserved.push((x - tw / 2.0 - shift, below, x + tw / 2.0 - shift, below + cap * 1.3));
            for s in hand.write(t, x - tw / 2.0 - shift, below, cap) {
                out.push((label, s));
            }
        }
        let step_pt_y = (sy / (p.y_max - p.y_min) * p.h as f64) as f32;
        let every_y = [1i64, 2, 5, 10, 20, 50].into_iter().find(|k| *k as f32 * step_pt_y >= cap * 1.9).unwrap_or(100);
        for (v, t) in ys.iter().zip(&yl) {
            let y = py(*v);
            if (y - ax_y).abs() > 0.5 || !zero_y {
                out.push((axis, hand.line((ax_x - tick, y), (ax_x + tick, y), 0.0)));
            }
            let i = (v / sy).round() as i64;
            if i % every_y != 0 || (v.abs() < sy * 1e-6 && zero_x) || y < y0 + cap * 1.1 {
                continue;
            }
            let tw = text_width(t, cap);
            reserved.push((ax_x - tick - gap - tw, y - cap / 2.0, ax_x - tick - gap, y + cap * 0.8));
            for s in hand.write(t, ax_x - tick - gap - tw, y - cap / 2.0, cap) {
                out.push((label, s));
            }
        }
        let xn = if p.x_name.trim().is_empty() { "x" } else { p.x_name.trim() };
        let yn = if p.y_name.trim().is_empty() { "y" } else { p.y_name.trim() };
        let (nx, ny) = (tip_x.0 - text_width(xn, cap) - cap * 0.1, ax_y - tick - gap * 0.6 - cap);
        reserved.push((nx, ny, nx + text_width(xn, cap), ny + cap * 1.3));
        for s in hand.write(xn, nx, ny, cap) {
            out.push((label, s));
        }
        let (mx, my) = (tip_y.0 + tick + gap, tip_y.1 - cap * 0.3);
        reserved.push((mx, my, mx + text_width(yn, cap), my + cap * 1.3));
        for s in hand.write(yn, mx, my, cap) {
            out.push((label, s));
        }
    }
    // Lines and curves.
    let mut graphs: Vec<(usize, Expr)> = Vec::new();
    for (i, o) in p.objs.iter().enumerate() {
        let Ok(v) = &vals[i] else { continue };
        if !shown(i) {
            continue;
        }
        let b = pen(o, 1.0);
        let line = o.style.line;
        macro_rules! draw {
            ($path:expr, $sway:expr) => {
                stroke_path(&mut hand, &mut out, &mut ink, &$path, line, b, $sway)
            };
        }
        let line_path = |s: (f32, f32), e: (f32, f32)| {
            let n = (((e.0 - s.0).hypot(e.1 - s.1) / 3.0) as usize).max(1);
            (0..=n).map(|k| (s.0 + (e.0 - s.0) * k as f32 / n as f32, s.1 + (e.1 - s.1) * k as f32 / n as f32)).collect::<Vec<_>>()
        };
        let big = ((p.x_max - p.x_min).abs() + (p.y_max - p.y_min).abs()) * 4.0;
        match v {
            Val::Func(e) => {
                let pieces = curve(e, p);
                if let Some(longest) = pieces.iter().max_by(|a, b| span(a).total_cmp(&span(b)))
                    && longest.len() >= 2
                    && let Some(t) = label_text(o, v)
                {
                    let k = ((longest.len() - 1) as f32 * 0.86) as usize;
                    labels.push(Label { x: longest[k].0, y: longest[k].1, off: cap * 0.35, text: t, color: o.style.color });
                }
                for piece in pieces {
                    draw!(piece, 0.0);
                }
                graphs.push((i, e.clone()));
            }
            Val::Line { .. } | Val::Lines(_) => {
                let ls: Vec<(f64, f64, f64)> = match v {
                    Val::Line { a, b, c } => vec![(*a, *b, *c)],
                    Val::Lines(ls) => ls.clone(),
                    _ => unreachable!(),
                };
                for (a, bb, c) in ls {
                    let n2 = a * a + bb * bb;
                    let o0 = (a * c / n2, bb * c / n2);
                    let d = (-bb, a);
                    let l = d.0.hypot(d.1);
                    if let Some((s, e)) = clip_line(p, o0, (d.0 / l, d.1 / l), -big - o0.0.abs() - o0.1.abs(), big + o0.0.abs() + o0.1.abs()) {
                        if let Some(t) = label_text(o, v) {
                            labels.push(Label { x: s.0 + (e.0 - s.0) * 0.78, y: s.1 + (e.1 - s.1) * 0.78, off: cap * 0.35, text: t.clone(), color: o.style.color });
                        }
                        draw!(line_path(s, e), wob);
                    }
                }
            }
            Val::Segment(a, c) | Val::Ray(a, c) => {
                let d = (c.0 - a.0, c.1 - a.1);
                let t1 = if matches!(v, Val::Ray(..)) { big / d.0.hypot(d.1).max(1e-12) } else { 1.0 };
                if let Some((s, e)) = clip_line(p, *a, d, 0.0, t1) {
                    if let Some(t) = label_text(o, v) {
                        labels.push(Label { x: (s.0 + e.0) / 2.0, y: (s.1 + e.1) / 2.0, off: cap * 0.35, text: t, color: o.style.color });
                    }
                    draw!(line_path(s, e), wob);
                }
            }
            Val::Vector { from, d } => {
                if let Some((s, e)) = clip_line(p, *from, *d, 0.0, 1.0) {
                    draw!(line_path(s, e), wob);
                    // A head at its end (when the end is in view).
                    let end = p.to_page(from.0 + d.0, from.1 + d.1);
                    if (end.0 - e.0).abs() + (end.1 - e.1).abs() < 0.5 {
                        let (dx, dy) = (e.0 - s.0, e.1 - s.1);
                        let l = dx.hypot(dy).max(1e-6);
                        let (ux, uy) = (dx / l, dy / l);
                        let hl = head * 0.9;
                        for side in [-1.0f32, 1.0] {
                            let a = 26f32.to_radians() * side;
                            let (c, sn) = (a.cos(), a.sin());
                            let back = (-(ux * c - uy * sn), -(ux * sn + uy * c));
                            out.push((b, hand.line(e, (e.0 + back.0 * hl, e.1 + back.1 * hl), 0.0)));
                        }
                    }
                    if let Some(t) = label_text(o, v) {
                        labels.push(Label { x: (s.0 + e.0) / 2.0, y: (s.1 + e.1) / 2.0, off: cap * 0.35, text: t, color: o.style.color });
                    }
                }
            }
            Val::Circle(c, r) => {
                let path: Vec<(f32, f32)> = (0..=240).map(|k| {
                    let a = k as f64 / 240.0 * std::f64::consts::TAU;
                    p.to_page(c.0 + r * a.cos(), c.1 + r * a.sin())
                }).collect();
                for piece in clip_path(&path, bx) {
                    draw!(piece, 0.0);
                }
                if let Some(t) = label_text(o, v) {
                    let q = p.to_page(c.0 + r * std::f64::consts::FRAC_1_SQRT_2, c.1 + r * std::f64::consts::FRAC_1_SQRT_2);
                    labels.push(Label { x: q.0, y: q.1, off: cap * 0.35, text: t, color: o.style.color });
                }
            }
            Val::Polygon(ps) => {
                let mut path: Vec<(f32, f32)> = ps.iter().map(|q| p.to_page(q.0, q.1)).collect();
                path.push(path[0]);
                let dense_path: Vec<(f32, f32)> = path.windows(2).flat_map(|w| line_path(w[0], w[1])).collect();
                for piece in clip_path(&dense_path, bx) {
                    draw!(piece, wob * 0.5);
                }
                if let Some(t) = label_text(o, v) {
                    let (cx, cy) = ps.iter().fold((0.0, 0.0), |a, q| (a.0 + q.0, a.1 + q.1));
                    let c = p.to_page(cx / ps.len() as f64, cy / ps.len() as f64);
                    labels.push(Label { x: c.0, y: c.1, off: 0.0, text: t, color: o.style.color });
                }
            }
            Val::Curve(fx, fy, a, c) => {
                let n = 800;
                let path: Vec<(f32, f32)> = (0..=n).map(|k| {
                    let t = a + (c - a) * k as f64 / n as f64;
                    p.to_page(fx.eval(t), fy.eval(t))
                }).collect();
                for piece in clip_path(&path, bx) {
                    draw!(piece, 0.0);
                }
            }
            Val::Implicit(f) => {
                for path in implicit_paths(p, f) {
                    for piece in clip_path(&path, bx) {
                        draw!(piece, 0.0);
                    }
                }
            }
            Val::Angle { at, from, sweep } => {
                let c = p.to_page(at.0, at.1);
                let rr = cap * 2.0;
                // The angle's directions on the page (y down), so it also reads with unequal units.
                let (ux, uy) = p.units_per_pt();
                let dir = |a: f64| {
                    let (dx, dy) = ((a.cos() / ux) as f32, (-a.sin() / uy) as f32);
                    let l = dx.hypot(dy).max(1e-6);
                    (dx / l, dy / l)
                };
                let (d0, d1) = (dir(*from), dir(from + sweep));
                let (a0, a1) = (d0.1.atan2(d0.0), d1.1.atan2(d1.0));
                // Counter-clockwise in the plot is clockwise on the page.
                let mut span = a0 - a1;
                if span < 0.0 {
                    span += std::f32::consts::TAU;
                }
                let right = (sweep.to_degrees() - 90.0).abs() < 0.05;
                let path: Vec<(f32, f32)> = if right {
                    let s = rr * 0.7;
                    vec![(c.0 + d0.0 * s, c.1 + d0.1 * s), (c.0 + (d0.0 + d1.0) * s, c.1 + (d0.1 + d1.1) * s), (c.0 + d1.0 * s, c.1 + d1.1 * s)]
                } else {
                    (0..=24).map(|k| {
                        let a = a0 - span * k as f32 / 24.0;
                        (c.0 + a.cos() * rr, c.1 + a.sin() * rr)
                    }).collect()
                };
                if o.style.fill > 0.01 && !right {
                    let inside = |x: f32, y: f32| {
                        let (dx, dy) = (x - c.0, y - c.1);
                        if dx.hypot(dy) > rr {
                            return false;
                        }
                        let mut a = a0 - dy.atan2(dx);
                        if a < 0.0 {
                            a += std::f32::consts::TAU;
                        }
                        a <= span
                    };
                    for (s, e) in hatch_where((c.0 - rr, c.1 - rr, c.0 + rr, c.1 + rr), gap_of(o).min(2.4), &inside) {
                        out.push((hatch_pen(o), hand.line(s, e, 0.0)));
                    }
                }
                out.push((pen(o, 0.8), hand.stroke(&dense(&path, 1.5), 0.0)));
                if right {
                    let q = (c.0 + (d0.0 + d1.0) * rr * 0.35, c.1 + (d0.1 + d1.1) * rr * 0.35);
                    out.push((pen(o, 1.2), hand.stroke(&[q, (q.0 + 0.05, q.1)], 0.0)));
                }
                if let Some(t) = label_text(o, v) {
                    let mid = a0 - span / 2.0;
                    labels.push(Label { x: c.0 + mid.cos() * rr * 1.7, y: c.1 + mid.sin() * rr * 1.7, off: 0.0, text: t, color: o.style.color });
                }
            }
            _ => {}
        }
    }
    // Points, on top.
    let mut pt_labels: Vec<Label> = Vec::new();
    for (i, o) in p.objs.iter().enumerate() {
        let Ok(v) = &vals[i] else { continue };
        if !shown(i) {
            continue;
        }
        let ps: Vec<(f64, f64)> = match v {
            Val::Point(q) => vec![*q],
            Val::Points(qs) => qs.clone(),
            _ => continue,
        };
        let r = if o.style.size > 0.0 { o.style.size } else { (cap * 0.42).max(2.2) };
        for (k, q) in ps.iter().enumerate() {
            let (x, y) = p.to_page(q.0, q.1);
            if x < x0 - 0.5 || x > x1 + 0.5 || y < y0 - 0.5 || y > y1 + 0.5 {
                continue;
            }
            match o.style.point {
                PointStyle::Dot => {
                    let b = Brush { width: r * 1.25, thinning: 0.0, color: o.style.color, kind: KIND_PEN };
                    out.push((b, hand.stroke(&[(x - 0.02, y), (x + 0.02, y)], 0.0)));
                }
                PointStyle::Cross => {
                    let b = pen(o, 0.75);
                    out.push((b, hand.line((x - r, y - r), (x + r, y + r), 0.0)));
                    out.push((b, hand.line((x + r, y - r), (x - r, y + r), 0.0)));
                }
                PointStyle::Ring => {
                    let path: Vec<(f32, f32)> = (0..=20).map(|j| {
                        let a = j as f32 / 20.0 * std::f32::consts::TAU;
                        (x + a.cos() * r * 0.7, y + a.sin() * r * 0.7)
                    }).collect();
                    out.push((pen(o, 0.6), hand.stroke(&path, 0.0)));
                }
            }
            let multi = ps.len() > 1;
            let nm = if multi { format!("{}_{}", o.name(), k + 1) } else { o.name() };
            let text = match o.style.label {
                LabelMode::Off => None,
                LabelMode::Name => Some(nm),
                LabelMode::Value => Some(format_val(&Val::Point(*q))),
                LabelMode::NameValue => Some(format!("{nm} = {}", format_val(&Val::Point(*q)))),
                LabelMode::Caption => Some(if o.style.caption.trim().is_empty() { nm } else { o.style.caption.clone() }),
            };
            if let Some(t) = text {
                pt_labels.push(Label { x, y, off: r * 1.1, text: t, color: o.style.color });
            }
        }
    }
    // Special points of functions (zeros, highs and lows, turning points, crossings).
    let crossing: Vec<Option<Expr>> = graphs.iter().map(|(i, e)| (p.objs[*i].style.special & MARK_CROSSINGS != 0).then(|| e.clone())).collect();
    let mut found: Vec<(SpecialPoint, u32, bool)> = Vec::new();
    for (i, e) in &graphs {
        let st = &p.objs[*i].style;
        if st.special & (MARK_ZEROS | MARK_EXTREMA | MARK_TURNS) != 0 {
            for s in points::special_points(p, &[Some(e.clone())], st.special & MARK_ZEROS != 0, st.special & MARK_EXTREMA != 0, st.special & MARK_TURNS != 0, false) {
                found.push((s, st.color, st.coords));
            }
        }
    }
    if crossing.iter().filter(|c| c.is_some()).count() >= 2 {
        for s in points::special_points(p, &crossing, false, false, false, true) {
            let st = &p.objs[graphs[s.on].0].style;
            found.push((s, st.color, st.coords));
        }
    }
    for (s, color, coords) in &found {
        let (x, y) = p.to_page(s.x, s.y);
        let r = (cap * 0.42).max(2.2);
        let b = Brush { width: (w * 0.75).clamp(0.4, 2.0), thinning: thin, color: *color, kind: KIND_PEN };
        out.push((b, hand.line((x - r, y - r), (x + r, y + r), 0.0)));
        out.push((b, hand.line((x + r, y - r), (x - r, y + r), 0.0)));
        if *coords {
            let tag = match s.kind {
                PointKind::High => "H",
                PointKind::Low => "T",
                PointKind::Turn => "W",
                PointKind::Zero => "N",
                PointKind::Cross => "S",
            };
            pt_labels.push(Label { x, y, off: r, text: format!("{tag}({}|{})", format_num(s.x, 2), format_num(s.y, 2)), color: *color });
        }
    }
    // Sliders and texts.
    for (i, o) in p.objs.iter().enumerate() {
        let Ok(v) = &vals[i] else { continue };
        if let (Some(s), Val::Num(value), Some((sx0, sx1, sy0))) = (o.style.slider, v, slider_track(p, i)) {
            let tb = Brush { width: (w * 0.6).clamp(0.35, 1.5), thinning: thin, color: o.style.color, kind: KIND_PEN };
            out.push((tb, hand.line((sx0, sy0), (sx1, sy0), 0.15)));
            for e in [sx0, sx1] {
                out.push((tb, hand.line((e, sy0 - cap * 0.25), (e, sy0 + cap * 0.25), 0.0)));
            }
            let t = if s.max > s.min { ((value - s.min) / (s.max - s.min)).clamp(0.0, 1.0) as f32 } else { 0.0 };
            let kx = sx0 + (sx1 - sx0) * t;
            let kb = Brush { width: cap * 0.75, thinning: 0.0, color: o.style.color, kind: KIND_PEN };
            out.push((kb, hand.stroke(&[(kx - 0.02, sy0), (kx + 0.02, sy0)], 0.0)));
            let text = format!("{} = {}", o.name(), fmt_num(*value));
            reserved.push((sx0, sy0 - cap * 1.9, sx1.max(sx0 + text_width(&text, cap * 0.85)), sy0 + cap * 0.5));
            for st in hand.write(&text, sx0, sy0 - cap * 1.9, cap * 0.85) {
                out.push((Brush { color: o.style.color, ..label }, st));
            }
            continue;
        }
        if let Val::Text(t) = v
            && shown(i)
        {
            let (fx, fy) = o.style.at.unwrap_or((0.04, 0.06));
            let (tx, ty) = (p.x + fx * p.w, p.y + fy * p.h);
            reserved.push((tx, ty, tx + text_width(t, cap), ty + cap * 1.3));
            for st in hand.write(t, tx, ty, cap) {
                out.push((Brush { color: o.style.color, ..label }, st));
            }
        }
    }
    // Labels: points first (they sit right by their point), then the rest.
    let mut placed = reserved;
    let words = placed.len();
    for (l, pc) in pt_labels.iter().map(|l| (l, cap * 0.85)).chain(labels.iter().map(|l| (l, cap))) {
        let tw = text_width(&l.text, pc);
        let lb = Brush { width: label.width * 0.95, color: l.color, ..label };
        if let Some((lx, ly)) = spot(l.x, l.y, tw, pc, l.off.max(cap * 0.2), bx, &placed, words, &ink) {
            placed.push((lx, ly, lx + tw, ly + pc * 1.3));
            for s in hand.write(&l.text, lx, ly, pc) {
                out.push((lb, s));
            }
        }
    }
    out
}

/// Notes where a path runs (labels keep off it) and draws it in its line style.
#[allow(clippy::too_many_arguments)]
fn stroke_path(hand: &mut Hand, out: &mut Vec<(Brush, Vec<InkPoint>)>, ink: &mut Vec<(f32, f32)>, path: &[(f32, f32)], line: LineStyle, b: Brush, sway: f32) {
    let mut last: Option<(f32, f32)> = None;
    for &q in path {
        if last.is_none_or(|l| (q.0 - l.0).abs() + (q.1 - l.1).abs() > 1.5) {
            ink.push(q);
            last = Some(q);
        }
    }
    for d in dashes(path, line, b.width) {
        out.push((b, hand.stroke(&dense(&d, 4.0), sway)));
    }
}

/// How wide a piece of a curve runs across.
fn span(piece: &[(f32, f32)]) -> f32 {
    let (lo, hi) = piece.iter().fold((f32::MAX, f32::MIN), |(lo, hi), q| (lo.min(q.0), hi.max(q.0)));
    hi - lo
}

/// Where to write a label of width `tw` at a point: up-right of it, else up-left, down-right,
/// down-left (then the same a line further out) — the first spot inside the box that covers
/// no words already written and no ink (`ink`: points of the curves and axes); failing that,
/// the one that touches the least ink. The first `words` of `placed` are axis numbers and
/// such, not labels.
#[allow(clippy::too_many_arguments)]
fn spot(x: f32, y: f32, tw: f32, cap: f32, off: f32, bx: (f32, f32, f32, f32), placed: &[(f32, f32, f32, f32)], words: usize, ink: &[(f32, f32)]) -> Option<(f32, f32)> {
    let h = cap * 1.3;
    let far = off + h * 0.9;
    let cands = [
        (x + off, y - off - h), (x - off - tw, y - off - h), (x + off, y + off), (x - off - tw, y + off),
        (x + off, y - far - h), (x - off - tw, y - far - h), (x + off, y + far), (x - off - tw, y + far),
    ];
    let inside = |&(lx, ly): &(f32, f32)| lx >= bx.0 && lx + tw <= bx.2 + cap && ly >= bx.1 - cap * 0.5 && ly + h <= bx.3 + cap * 0.5;
    let free = |&(lx, ly): &(f32, f32)| !placed.iter().any(|r| lx < r.2 && lx + tw > r.0 && ly < r.3 && ly + h > r.1);
    let pad = cap * 0.15;
    let hits = |&(lx, ly): &(f32, f32)| ink.iter().filter(|&&(px, py)| px > lx - pad && px < lx + tw + pad && py > ly - pad && py < ly + h + pad).count();
    let ok: Vec<&(f32, f32)> = cands.iter().filter(|c| inside(c) && free(c)).collect();
    if let Some(c) = ok.iter().find(|c| hits(c) == 0) {
        return Some(**c);
    }
    if let Some(c) = ok.iter().min_by_key(|c| hits(c)) {
        return Some(**c);
    }
    cands.first().filter(|_| placed.len() == words).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plot() -> Plot {
        Plot::centered(100.0, 100.0, 240.0, 160.0, 20.0, 0xFF22_2222, 1.2)
    }

    #[test]
    fn ticks_are_round_and_german() {
        assert_eq!(nice_step(10.0, 10.0), 1.0);
        assert_eq!(nice_step(12.0, 5.0), 5.0);
        assert_eq!(nice_step(0.3, 6.0), 0.05);
        assert_eq!(tick_label(0.5, 0.5), "0,5");
        assert_eq!(tick_label(3e-17, 0.1), "0");
        assert_eq!((pi_label(1, 2), pi_label(2, 2), pi_label(3, 2), pi_label(-1, 1), pi_label(4, 2)), ("π/2".into(), "π".into(), "3π/2".into(), "-π".into(), "2π".into()));
        let (_, vs, ls) = ticks_of(-7.0, 7.0, 200.0, true);
        assert!((vs[ls.iter().position(|l| l == "π").unwrap()] - std::f64::consts::PI).abs() < 1e-12);
    }

    #[test]
    fn curves_stay_in_the_box_and_break_at_poles() {
        let p = plot();
        let par = curve(&super::super::parse("x^2 - 2").unwrap(), &p);
        assert_eq!(par.len(), 1);
        assert_eq!(curve(&super::super::parse("1/x").unwrap(), &p).len(), 2);
        assert_eq!(curve(&super::super::parse("tan x").unwrap(), &p).len(), 5);
    }

    #[test]
    fn every_kind_of_object_is_drawn_in_the_box() {
        let inputs = [
            "A = (1, 2)", "B = (-3, -1)", "C = (4, -2)", "f(x) = 0.5x^2 - 3", "g = Gerade(A, B)", "s = Strecke(B, C)", "r = Strahl(C, A)", "u = Vektor(B, A)", "c = Kreis(A, 2)",
            "vieleck1 = Vieleck(A, B, C)", "α = Winkel(B, A, C)", "a = Schieberegler(-2, 2, 0.1)", "k: x^2 + y^2 = 9", "d = Kurve(cos(t), sin(t), t, 0, pi)", "S = Schneide(f, g)",
            "flaeche = Integral(f, -1, 1)", "text1 = Text(\"Hallo\")",
        ];
        let mut p = plot();
        p.id = "x".into();
        p.grid = true;
        p.minor = true;
        for i in inputs {
            p = p.add(i).unwrap_or_else(|e| panic!("{i}: {e}"));
        }
        assert!(p.errors().iter().all(|e| e.is_none()), "{:?}", p.errors());
        for o in &mut p.objs {
            o.style.label = LabelMode::NameValue;
        }
        let ink = render(&p);
        assert!(ink.len() > 120, "{}", ink.len());
        for (_, pts) in &ink {
            for q in pts {
                assert!(q.x > 75.0 && q.x < 365.0 && q.y > 80.0 && q.y < 290.0, "{q:?}");
            }
        }
        // Each object's colour shows up.
        for o in &p.objs {
            assert!(ink.iter().any(|(b, _)| b.color & 0xFFFFFF == o.style.color & 0xFFFFFF), "{} not drawn", o.def);
        }
        assert_eq!(ink, render(&p), "the same every time");
        // Hidden objects aren't drawn.
        let mut h = p.clone();
        for o in &mut h.objs {
            o.style.visible = false;
        }
        let empty = Plot { objs: vec![], ..p.clone() };
        assert_eq!(render(&h).len(), render(&empty).len());
        assert!(slider_track(&p, 11).is_some());
    }
}
