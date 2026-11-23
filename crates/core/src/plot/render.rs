//! A plot as ink: grid, hatched areas, axes with heads, ticks and numbers, curves in their
//! line style, marked points, names at the curves. In page points, drawn like strokes.

use super::expr::{Curve, Expr, fn_name};
use super::font::{self, glyph, text_width};
use super::points::{self, PointKind};
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
fn ticks(lo: f64, hi: f64, len: f32, pi: bool) -> (f64, Vec<f64>, Vec<String>) {
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

    /// `text` with its top-left at (x, y), each glyph a little different (slant and size wander).
    fn write(&mut self, text: &str, x: f32, y: f32, cap: f32) -> Vec<Vec<InkPoint>> {
        let mut out = Vec::new();
        let mut ox = x;
        for c in text.chars() {
            let Some((w, lines)) = glyph(c) else { continue };
            let (slant, k, dy) = if self.clean { (0.08, 1.0, 0.0) } else { (0.08 + self.rng.next() * 0.05, 1.0 + self.rng.next() * 0.04, self.rng.next() * 0.03) };
            for l in lines {
                let path: Vec<(f32, f32)> = dense(&l, 0.06).iter().map(|&(gx, gy)| (ox + (gx + (1.0 - gy) * slant) * cap * k, y + (gy + dy) * cap * k)).collect();
                out.push(self.stroke(&path, 0.0));
            }
            ox += w * cap + font::GAP * cap;
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

/// Hatching between `f` and `base` (None: the x axis) over [from, to]: lines at 45°, cut to the
/// area and the box, in page points.
fn hatch(p: &Plot, f: &Expr, base: Option<&Expr>, from: f64, to: f64, gap: f32) -> Vec<((f32, f32), (f32, f32))> {
    let (lo, hi) = (from.min(to).max(p.x_min), from.max(to).min(p.x_max));
    if hi <= lo {
        return Vec::new();
    }
    let sx = p.w as f64 / (p.x_max - p.x_min);
    let sy = p.h as f64 / (p.y_max - p.y_min);
    let (x0, x1) = ((p.x as f64 + (lo - p.x_min) * sx) as f32, (p.x as f64 + (hi - p.x_min) * sx) as f32);
    // Where the area is, column by column (a column every half point).
    let step = 0.5f32;
    let cols = ((x1 - x0) / step).ceil() as usize + 1;
    let band: Vec<Option<(f32, f32)>> = (0..cols)
        .map(|i| {
            let px = (x0 + i as f32 * step).min(x1);
            let xv = p.x_min + (px - p.x) as f64 / sx;
            let (a, b) = (f.eval(xv), base.map_or(0.0, |g| g.eval(xv)));
            if !a.is_finite() || !b.is_finite() {
                return None;
            }
            let (ya, yb) = (p.y as f64 + (p.y_max - a) * sy, p.y as f64 + (p.y_max - b) * sy);
            let (t, bt) = (ya.min(yb).max(p.y as f64) as f32, ya.max(yb).min((p.y + p.h) as f64) as f32);
            (bt > t).then_some((t, bt))
        })
        .collect();
    let inside = |x: f32, y: f32| {
        if x < x0 || x > x1 {
            return false;
        }
        let i = (((x - x0) / step).round() as usize).min(cols - 1);
        band[i].is_some_and(|(t, b)| y >= t && y <= b)
    };
    // Lines y = c − x (rising to the right), `gap` apart across.
    let mut out = Vec::new();
    let d = gap * std::f32::consts::SQRT_2;
    let (ytop, ybot) = (p.y, p.y + p.h);
    let mut c = x0 + ytop;
    while c <= x1 + ybot {
        let mut run: Option<(f32, f32)> = None;
        let mut last = (0.0, 0.0);
        let mut x = x0;
        while x <= x1 + 1e-3 {
            let y = c - x;
            if inside(x, y) {
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

/// The plot as ink in page points, under-to-top: grid, areas, axes with ticks and numbers,
/// curves, marked points, names.
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
    let (x0, y0, x1, y1) = (p.x, p.y, p.x + p.w, p.y + p.h);
    let px = |v: f64| (p.x as f64 + (v - p.x_min) / (p.x_max - p.x_min) * p.w as f64) as f32;
    let py = |v: f64| (p.y as f64 + (p.y_max - v) / (p.y_max - p.y_min) * p.h as f64) as f32;
    let inside = |x: f32, y: f32| x >= x0 - 0.5 && x <= x1 + 0.5 && y >= y0 - 0.5 && y <= y1 + 0.5;
    let curves = p.curves();
    let graphs = points::graphs(&curves);
    // Where the axes run: through 0, else along the nearer edge.
    let ax_y = if p.y_min <= 0.0 && p.y_max >= 0.0 { py(0.0) } else if p.y_max < 0.0 { y0 } else { y1 };
    let ax_x = if p.x_min <= 0.0 && p.x_max >= 0.0 { px(0.0) } else if p.x_max < 0.0 { x1 } else { x0 };
    let (sx, xs, xl) = ticks(p.x_min, p.x_max, p.w, p.pi);
    let (sy, ys, yl) = ticks(p.y_min, p.y_max, p.h, false);
    if p.grid {
        for &v in &xs {
            out.push((grid, hand.line((px(v), y0), (px(v), y1), 0.15)));
        }
        for &v in &ys {
            out.push((grid, hand.line((x0, py(v)), (x1, py(v)), 0.15)));
        }
    }
    // Areas: hatched in their function's ink, under everything else.
    let mut area_labels: Vec<(f32, f32, String, u32)> = Vec::new();
    for (i, f) in p.fns.iter().enumerate() {
        let (Some(a), Some(Some(g))) = (&f.area, graphs.get(i)) else { continue };
        let base = a.to_fn.and_then(|j| graphs.get(j).cloned().flatten());
        if a.to_fn.is_some() && base.is_none() {
            continue;
        }
        let brush = Brush { width: (w * 0.45).clamp(0.3, 1.0), thinning: 0.0, color: (f.color & 0x00FF_FFFF) | 0xB000_0000, kind: KIND_PEN };
        for (s, e) in hatch(p, g, base.as_ref(), a.from, a.to, (cap * 0.55).max(2.6)) {
            out.push((brush, hand.line(s, e, 0.1)));
        }
        if p.coords {
            let (lo, hi) = (a.from.min(a.to).max(p.x_min), a.from.max(a.to).min(p.x_max));
            if hi > lo {
                let v = points::area(g, base.as_ref(), lo, hi);
                let m = (lo + hi) / 2.0;
                let ym = (g.eval(m) + base.as_ref().map_or(0.0, |b| b.eval(m))) / 2.0;
                if ym.is_finite() {
                    area_labels.push((px(m), py(ym.clamp(p.y_min, p.y_max)), format!("A ≈ {}", format_num(v, 2)), f.color));
                }
            }
        }
    }
    let wob = 0.35;
    // Axes, each with a head at its positive end.
    let head = (small * 0.035).clamp(3.5, 7.0);
    out.push((axis, hand.line((x0, ax_y), (x1 + head * 0.4, ax_y), wob)));
    out.push((axis, hand.line((ax_x, y1), (ax_x, y0 - head * 0.4), wob)));
    let tip_x = (x1 + head * 0.4, ax_y);
    let tip_y = (ax_x, y0 - head * 0.4);
    let sa = (26f32).to_radians();
    for (tip, dir) in [(tip_x, (1.0f32, 0.0f32)), (tip_y, (0.0, -1.0))] {
        for side in [-1.0f32, 1.0] {
            let (c, s) = (sa.cos(), sa.sin() * side);
            let back = (-dir.0 * c + dir.1 * s, -dir.1 * c - dir.0 * s);
            out.push((axis, hand.line(tip, (tip.0 + back.0 * head, tip.1 + back.1 * head), 0.0)));
        }
    }
    // Ticks and their numbers (every one that has room; 0 not where the other axis crosses;
    // none at an axis' end, where its head and name are).
    let tick = cap * 0.45;
    let zero_x = p.x_min < 0.0 && p.x_max > 0.0;
    let zero_y = p.y_min < 0.0 && p.y_max > 0.0;
    let gap = cap * 0.4;
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
        // A minus sign doesn't push the number off its tick.
        let shift = if t.starts_with('-') { text_width("-", cap) * 0.5 + 0.08 * cap } else { 0.0 };
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
        for s in hand.write(t, ax_x - tick - gap - tw, y - cap / 2.0, cap) {
            out.push((label, s));
        }
    }
    // The axes' names by their heads (the x name over the axis: the numbers are under it).
    let xn = if p.x_name.trim().is_empty() { "x" } else { p.x_name.trim() };
    let yn = if p.y_name.trim().is_empty() { "y" } else { p.y_name.trim() };
    for s in hand.write(xn, tip_x.0 - text_width(xn, cap) - cap * 0.1, ax_y - tick - gap * 0.6 - cap, cap) {
        out.push((label, s));
    }
    for s in hand.write(yn, tip_y.0 + tick + gap, tip_y.1 - cap * 0.3, cap) {
        out.push((label, s));
    }
    // The curves, in their own ink and line, on top; where to write each one's name.
    let mut names: Vec<(f32, f32, String, u32)> = Vec::new();
    let mut all_pieces: Vec<Vec<(f32, f32)>> = Vec::new();
    for (i, f) in p.fns.iter().enumerate() {
        let Some(Ok(c)) = curves.get(i) else { continue };
        let brush = Brush { width: w, thinning: thin, color: f.color, kind: KIND_PEN };
        let pieces: Vec<Vec<(f32, f32)>> = match c {
            Curve::Of(e) => curve(e, p),
            Curve::AtX(v) => {
                let x = px(*v);
                if x < x0 - 0.5 || x > x1 + 0.5 {
                    continue;
                }
                vec![(0..=((p.h / 3.0) as usize).max(1)).map(|k| (x, y1 - p.h * k as f32 / ((p.h / 3.0) as usize).max(1) as f32)).collect()]
            }
        };
        all_pieces.extend(pieces.iter().cloned());
        for piece in &pieces {
            for d in dashes(piece, f.style, w) {
                // Kinks (|x| at 0) stay sharp.
                out.push((brush, hand.stroke(&dense(&d, 4.0), if matches!(c, Curve::AtX(_)) { wob } else { 0.0 })));
            }
        }
        if p.names
            && let Some(longest) = pieces.iter().max_by(|a, b| span(a).total_cmp(&span(b)))
            && longest.len() >= 2
        {
            // Near the end of its longest piece, out of the way of the axes.
            let k = ((longest.len() - 1) as f32 * if matches!(c, Curve::AtX(_)) { 0.92 } else { 0.86 }) as usize;
            let (ax, ay) = longest[k];
            names.push((ax, ay, fn_name(i), f.color));
        }
    }
    // Marked points: little crosses, with their coordinates when asked for.
    let mut marks: Vec<(f32, f32, String, u32)> = Vec::new();
    let found = points::special_points(p, &graphs, p.marks & MARK_ZEROS != 0, p.marks & MARK_EXTREMA != 0, p.marks & MARK_TURNS != 0, p.marks & MARK_CROSSINGS != 0);
    for s in &found {
        let color = p.fns.get(s.on).map_or(p.color, |f| f.color);
        let text = if p.coords { format!("({}|{})", format_num(s.x, 2), format_num(s.y, 2)) } else { String::new() };
        let tag = match s.kind {
            PointKind::High => "H",
            PointKind::Low => "T",
            PointKind::Turn => "W",
            PointKind::Zero => "N",
            PointKind::Cross => "S",
        };
        marks.push((px(s.x), py(s.y), if p.coords { format!("{tag}{text}") } else { String::new() }, color));
    }
    for q in &p.points {
        let (x, y) = (px(q.x), py(q.y));
        if !inside(x, y) {
            continue;
        }
        let text = if p.coords { format!("{}({}|{})", q.name, format_num(q.x, 2), format_num(q.y, 2)) } else { q.name.clone() };
        marks.push((x, y, text, p.color));
    }
    let cross = (cap * 0.42).max(2.2);
    let mut placed: Vec<(f32, f32, f32, f32)> = Vec::new();
    // What labels keep off: the curves and the axes (sampled every couple of points).
    let mut ink: Vec<(f32, f32)> = Vec::new();
    for piece in &all_pieces {
        let mut last: Option<(f32, f32)> = None;
        for &q in piece {
            if last.is_none_or(|l| (q.0 - l.0).abs() + (q.1 - l.1).abs() > 1.5) {
                ink.push(q);
                last = Some(q);
            }
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
    let pcap = cap * 0.85;
    for (x, y, text, color) in &marks {
        let b = Brush { width: (w * 0.75).clamp(0.4, 2.0), thinning: thin, color: *color, kind: KIND_PEN };
        out.push((b, hand.line((x - cross, y - cross), (x + cross, y + cross), 0.0)));
        out.push((b, hand.line((x + cross, y - cross), (x - cross, y + cross), 0.0)));
        if text.is_empty() {
            continue;
        }
        let tw = text_width(text, pcap);
        let lb = Brush { width: label.width * 0.9, ..b };
        if let Some((lx, ly)) = spot(*x, *y, tw, pcap, cross, (x0, y0, x1, y1), &placed, &ink) {
            placed.push((lx, ly, lx + tw, ly + pcap * 1.3));
            for s in hand.write(text, lx, ly, pcap) {
                out.push((lb, s));
            }
        }
    }
    for (x, y, text, color) in names.iter().chain(area_labels.iter()) {
        let tw = text_width(text, cap);
        let lb = Brush { color: *color, ..label };
        if let Some((lx, ly)) = spot(*x, *y, tw, cap, cap * 0.35, (x0, y0, x1, y1), &placed, &ink) {
            placed.push((lx, ly, lx + tw, ly + cap * 1.3));
            for s in hand.write(text, lx, ly, cap) {
                out.push((lb, s));
            }
        }
    }
    out
}

/// How wide a piece of a curve runs across.
fn span(piece: &[(f32, f32)]) -> f32 {
    let (lo, hi) = piece.iter().fold((f32::MAX, f32::MIN), |(lo, hi), q| (lo.min(q.0), hi.max(q.0)));
    hi - lo
}

/// Where to write a label of width `tw` at a point: up-right of it, else up-left, down-right,
/// down-left — the first spot inside the box that covers no label already written and no ink
/// (`ink`: points of the curves and axes); failing that, the one that touches the least ink.
#[allow(clippy::too_many_arguments)]
fn spot(x: f32, y: f32, tw: f32, cap: f32, off: f32, bx: (f32, f32, f32, f32), placed: &[(f32, f32, f32, f32)], ink: &[(f32, f32)]) -> Option<(f32, f32)> {
    let h = cap * 1.3;
    let cands = [(x + off, y - off - h), (x - off - tw, y - off - h), (x + off, y + off), (x - off - tw, y + off)];
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
    cands.first().filter(|_| placed.is_empty()).copied()
}

#[cfg(test)]
mod tests {
    use super::super::expr::parse;
    use super::super::{Area, PlotFn, PlotPoint};
    use super::*;

    fn plot(fns: &[&str]) -> Plot {
        let mut p = Plot::centered(100.0, 100.0, 240.0, 160.0, 20.0, 0xFF22_2222, 1.2);
        p.fns = fns.iter().map(|f| PlotFn::new(f, 0xFFD0_2020)).collect();
        p
    }

    #[test]
    fn ticks_are_round_and_german() {
        assert_eq!(nice_step(10.0, 10.0), 1.0);
        assert_eq!(nice_step(12.0, 5.0), 5.0);
        assert_eq!(nice_step(1.0, 4.0), 0.5);
        assert_eq!(nice_step(0.3, 6.0), 0.05);
        assert_eq!(tick_label(0.5, 0.5), "0,5");
        assert_eq!(tick_label(-2.0, 1.0), "-2");
        assert_eq!(tick_label(1.25, 0.05), "1,25");
        assert_eq!(tick_label(3e-17, 0.1), "0");
        assert_eq!((pi_label(1, 2), pi_label(2, 2), pi_label(3, 2), pi_label(-1, 1), pi_label(4, 2)), ("π/2".into(), "π".into(), "3π/2".into(), "-π".into(), "2π".into()));
        let (_, vs, ls) = ticks(-7.0, 7.0, 200.0, true);
        assert!(ls.contains(&"π".to_string()) && ls.contains(&"-π".to_string()), "{ls:?}");
        assert!((vs[ls.iter().position(|l| l == "π").unwrap()] - std::f64::consts::PI).abs() < 1e-12);
    }

    #[test]
    fn curves_stay_in_the_box_and_break_at_poles() {
        let p = plot(&[]);
        assert_eq!((p.x_min, p.x_max, p.y_min, p.y_max), (-6.0, 6.0, -4.0, 4.0));
        let inside = |pieces: &[Vec<(f32, f32)>]| pieces.iter().flatten().all(|&(x, y)| (99.9..=340.1).contains(&x) && (99.4..=260.6).contains(&y));
        let par = curve(&parse("x^2 - 2").unwrap(), &p);
        assert_eq!(par.len(), 1, "{}", par.len());
        assert!(inside(&par));
        let low = par[0].iter().cloned().fold((0.0f32, 0.0f32), |a, b| if b.1 > a.1 { b } else { a });
        assert!((low.0 - 220.0).abs() < 1.0 && (low.1 - 220.0).abs() < 0.5, "{low:?}");
        let hyp = curve(&parse("1/x").unwrap(), &p);
        assert_eq!(hyp.len(), 2);
        assert!(inside(&hyp));
        assert!(hyp.iter().all(|piece| piece.iter().all(|q| q.0 < 220.0) || piece.iter().all(|q| q.0 > 220.0)));
        assert_eq!(curve(&parse("tan x").unwrap(), &p).len(), 5);
        // A step jumps: floor shows its steps -4 … 4 (the others are off the box).
        assert_eq!(curve(&parse("floor(x)").unwrap(), &p).len(), 9);
        let sq = curve(&parse("sqrt x").unwrap(), &p);
        assert_eq!(sq.len(), 1);
        assert!((sq[0][0].0 - 220.0).abs() < 0.2, "{:?}", sq[0][0]);
        let sin = curve(&parse("3 sin x").unwrap(), &p);
        assert!(sin[0].windows(2).all(|w| (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) < 6.0));
    }

    #[test]
    fn the_look_is_ink_in_the_box_and_the_same_every_time() {
        let mut p = plot(&["x^2/4 - 2", "sin x"]);
        p.id = "abc".into();
        let a = render(&p);
        assert_eq!(a, render(&p), "the same plot looks the same every time");
        assert!(a.len() > 20);
        for (_, pts) in &a {
            for q in pts {
                assert!(q.x > 80.0 && q.x < 360.0 && q.y > 85.0 && q.y < 285.0, "{q:?}");
            }
        }
        assert!(a.iter().any(|(b, _)| b.color == 0xFFD0_2020));
        assert!(a.iter().filter(|(b, _)| b.color != 0xFFD0_2020).all(|(b, _)| b.color & 0xFFFFFF == 0x222222));
        let axis = &a[0].1;
        let ys: Vec<f32> = axis.iter().map(|q| q.y).collect();
        let (lo, hi) = (ys.iter().cloned().fold(f32::MAX, f32::min), ys.iter().cloned().fold(f32::MIN, f32::max));
        assert!(hi - lo > 0.05 && hi - lo < 0.8, "{lo} {hi}");
        p.clean = true;
        let c = render(&p);
        let ys: Vec<f32> = c[0].1.iter().map(|q| q.y).collect();
        assert!(ys.iter().all(|y| (y - ys[0]).abs() < 1e-4), "clean is straight");
        p.grid = true;
        assert!(render(&p).len() > c.len() + 10);
        let mut q = plot(&["x^2/4 - 2", "sin x"]);
        q.id = "xyz".into();
        assert_ne!(render(&q)[0].1, a[0].1);
    }

    #[test]
    fn more_on_a_plot_more_ink() {
        let base = plot(&["x^2/2 - 2"]);
        let n0 = render(&base).len();
        // Dashed: one curve in many pieces.
        let mut d = base.clone();
        d.fns[0].style = LineStyle::Dashed;
        let curve_strokes = |p: &Plot| render(p).into_iter().filter(|(b, _)| b.color == 0xFFD0_2020).count();
        assert!(curve_strokes(&d) > 10 * curve_strokes(&base));
        // A hatched area under the curve from -2 to 2 (between it and the axis).
        let mut a = base.clone();
        a.fns[0].area = Some(Area { from: -2.0, to: 2.0, to_fn: None });
        let hatch_lines: Vec<_> = render(&a).into_iter().filter(|(b, _)| b.color == 0xB0D0_2020).collect();
        assert!(hatch_lines.len() > 15, "{}", hatch_lines.len());
        // Every hatch line lies over [-2, 2] (page 180 … 260) and between the curve and the axis.
        for (_, pts) in &hatch_lines {
            for q in pts {
                assert!(q.x > 179.0 && q.x < 261.0 && q.y > 179.0 && q.y < 221.0, "{q:?}");
            }
        }
        // Marked zeros and the low point, with coordinates; own points; names.
        let mut m = base.clone();
        m.marks = MARK_ZEROS | MARK_EXTREMA;
        m.coords = true;
        m.names = true;
        m.points = vec![PlotPoint { name: "A".into(), x: 1.0, y: 3.0 }];
        assert!(render(&m).len() > n0 + 30);
        // A vertical line and a reference to f: its derivative, dotted.
        let mut v = base.clone();
        v.fns.push(PlotFn::new("x = 3", 0xFF20_20D0));
        v.fns.push(PlotFn { style: LineStyle::Dotted, ..PlotFn::new("f'(x)", 0xFF20_A020) });
        let r = render(&v);
        let blue: Vec<_> = r.iter().filter(|(b, _)| b.color == 0xFF20_20D0).collect();
        assert_eq!(blue.len(), 1);
        assert!(blue[0].1.iter().all(|q| (q.x - 280.0).abs() < 0.6));
        assert!(r.iter().filter(|(b, _)| b.color == 0xFF20_A020).count() > 20, "f' is drawn, in dots");
        // π on the x axis.
        let mut t = plot(&["sin x"]);
        t.pi = true;
        assert!(render(&t).len() > 10);
    }

    #[test]
    fn dashes_follow_the_path() {
        let path: Vec<(f32, f32)> = (0..=100).map(|i| (i as f32, 0.0)).collect();
        let d = dashes(&path, LineStyle::Dashed, 1.0);
        // 7 on, 4.5 off: about 100 / 11.5 dashes, each about 7 long.
        assert!((8..=10).contains(&d.len()), "{}", d.len());
        for piece in &d[..d.len() - 1] {
            let len = piece.last().unwrap().0 - piece[0].0;
            assert!((len - 7.0).abs() < 0.01, "{len}");
        }
        assert_eq!(dashes(&path, LineStyle::Solid, 1.0).len(), 1);
    }
}
