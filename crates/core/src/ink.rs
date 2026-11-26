//! Ink: stroke samples, their compact binary encoding, and the stroke outline.
//!
//! The outline is computed here and nowhere else, so a stroke looks the same on the tablet,
//! the laptop, the phone and in an exported PDF.

use crate::error::{Error, Result};

/// One pen sample in page units (1 unit = 1 pt = 1/72 inch).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InkPoint {
    pub x: f32,
    pub y: f32,
    /// 0..1 (fingers and mice report 0.5).
    pub pressure: f32,
    /// Milliseconds since the stroke started.
    pub t_ms: u32,
}

/// How a stroke is drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Brush {
    /// Nominal width in page units at pressure 0.5.
    pub width: f32,
    /// 0 = constant width, 1 = width fully follows pressure.
    pub thinning: f32,
    /// 0xAARRGGBB.
    pub color: u32,
    /// How it is drawn: [`KIND_PEN`], [`KIND_PENCIL`] (grain texture), [`KIND_MARKER`].
    pub kind: u8,
}

pub const KIND_PEN: u8 = 0;
pub const KIND_PENCIL: u8 = 1;
pub const KIND_MARKER: u8 = 2;
/// A filled area (the paint bucket, [`crate::fill`]): its points go round its outline, holes
/// joined in by zero-width links; dashed ([`DASH_DASHED`]) it is hatched with lines `width`
/// wide instead. Apps before 0.37 draw such a stroke as a line round the area.
pub const KIND_FILL: u8 = 3;

/// How a pen stroke is dashed, in bits 4–5 of [`Brush::kind`] (apps before 0.36 don't know
/// these kinds and draw such strokes solid, rather than losing them).
pub const DASH_SOLID: u8 = 0;
pub const DASH_DASHED: u8 = 1;
pub const DASH_DOTTED: u8 = 2;
pub const DASH_DASH_DOT: u8 = 3;
const DASH_SHIFT: u8 = 4;

impl Brush {
    /// The kind without its dash: [`KIND_PEN`], [`KIND_PENCIL`] or [`KIND_MARKER`].
    pub fn base_kind(&self) -> u8 {
        self.kind & 0x0F
    }
    /// [`DASH_SOLID`], [`DASH_DASHED`], [`DASH_DOTTED`] or [`DASH_DASH_DOT`].
    pub fn dash(&self) -> u8 {
        (self.kind >> DASH_SHIFT) & 3
    }
    /// The same brush dashed `dash` (only pens dash and fills hatch; pencils and markers stay
    /// solid).
    pub fn with_dash(self, dash: u8) -> Brush {
        let dash = if matches!(self.base_kind(), KIND_PEN | KIND_FILL) { dash & 3 } else { 0 };
        Brush { kind: self.base_kind() | (dash << DASH_SHIFT), ..self }
    }
}

/// The pieces a dashed stroke is drawn as, each a stroke of its own, along its way: dashes four
/// widths long with gaps of three, dots two widths apart, or dash and dot by turns (round ends
/// make the gaps look a width shorter). None for a solid stroke.
pub fn dash_runs(points: &[InkPoint], brush: &Brush) -> Option<Vec<Vec<InkPoint>>> {
    let dash = brush.dash();
    if dash == DASH_SOLID || brush.base_kind() != KIND_PEN || points.len() < 2 {
        return None;
    }
    let w = brush.width.max(0.6);
    // (drawn, length in widths); a drawn piece of length 0 is a dot.
    let pattern: &[(bool, f32)] = match dash {
        DASH_DASHED => &[(true, 4.0), (false, 3.0)],
        DASH_DOTTED => &[(true, 0.0), (false, 2.2)],
        _ => &[(true, 4.0), (false, 2.4), (true, 0.0), (false, 2.4)],
    };
    let lerp = |a: &InkPoint, b: &InkPoint, t: f32| InkPoint { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t, pressure: a.pressure + (b.pressure - a.pressure) * t, t_ms: a.t_ms + ((b.t_ms as f32 - a.t_ms as f32) * t).max(0.0) as u32 };
    let pts: Vec<&InkPoint> = points.iter().filter(|p| p.x.is_finite() && p.y.is_finite()).collect();
    if pts.len() < 2 {
        return None;
    }
    let mut runs: Vec<Vec<InkPoint>> = Vec::new();
    let mut run: Vec<InkPoint> = Vec::new();
    let (mut k, mut left) = (0usize, pattern[0].1 * w);
    if pattern[0].0 {
        run.push(*pts[0]);
    }
    for seg in pts.windows(2) {
        let (a, b) = (seg[0], seg[1]);
        let len = (b.x - a.x).hypot(b.y - a.y);
        let mut at = 0.0f32;
        while len - at >= left {
            at += left;
            let p = lerp(a, b, if len > 0.0 { at / len } else { 0.0 });
            if pattern[k].0 {
                run.push(p);
                runs.push(std::mem::take(&mut run));
            }
            k = (k + 1) % pattern.len();
            left = pattern[k].1 * w;
            if pattern[k].0 {
                run.push(p);
            }
            if runs.len() > 20_000 {
                return Some(runs);
            }
        }
        left -= len - at;
        if pattern[k].0 {
            run.push(*b);
        }
    }
    if !run.is_empty() {
        runs.push(run);
    }
    Some(runs)
}

/// Dashed pieces joined into one polygon: each piece's outline, then on to the next one's first
/// vertex and, after the last, back the same way, so the joins have no area (they fill nothing)
/// while the whole line still hits and selects as one.
fn joined(runs: &[Vec<InkPoint>], brush: &Brush) -> Vec<f32> {
    let polys: Vec<Vec<f32>> = runs.iter().map(|r| solid_outline(r, brush)).filter(|p| p.len() >= 6).collect();
    let mut out = Vec::with_capacity(polys.iter().map(|p| p.len() + 4).sum());
    for p in &polys {
        out.extend_from_slice(p);
        out.extend_from_slice(&p[..2]);
    }
    for p in polys.iter().rev().skip(1) {
        out.extend_from_slice(&p[..2]);
    }
    out
}

impl Default for Brush {
    fn default() -> Self {
        Brush { width: 2.0, thinning: 0.6, color: 0xFF111111, kind: 0 }
    }
}

/// Quantization of coordinates: 1/64 pt is far below what a pen can resolve.
const XY_SCALE: f32 = 64.0;
/// Coordinates are kept within ±this many points (about 350 km of board): far beyond any
/// page, and small enough that quantized deltas never overflow.
pub const MAX_COORD: f32 = 1e9;
const FORMAT_V1: u8 = 1;

/// Encodes samples as: version, count, then per sample zigzag-varint deltas of x and y
/// (1/64 pt), the pressure as a byte, and the time delta as a varint. A typical sample takes
/// 4-6 bytes instead of 16.
pub fn encode_points(points: &[InkPoint]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + points.len() * 6);
    out.push(FORMAT_V1);
    put_uvarint(&mut out, points.len() as u64);
    let (mut px, mut py, mut pt) = (0i64, 0i64, 0u32);
    for p in points {
        let q = |v: f32| (v.clamp(-MAX_COORD, MAX_COORD) * XY_SCALE).round() as i64;
        let (qx, qy) = (q(p.x), q(p.y));
        put_uvarint(&mut out, zigzag(qx - px));
        put_uvarint(&mut out, zigzag(qy - py));
        out.push((p.pressure.clamp(0.0, 1.0) * 255.0).round() as u8);
        put_uvarint(&mut out, p.t_ms.saturating_sub(pt) as u64);
        px = qx;
        py = qy;
        pt = p.t_ms.max(pt);
    }
    out
}

/// Decodes what [`encode_points`] wrote.
pub fn decode_points(bytes: &[u8]) -> Result<Vec<InkPoint>> {
    let mut r = Reader { b: bytes, i: 0 };
    let version = r.byte()?;
    if version != FORMAT_V1 {
        return Err(Error::Format(format!("unknown ink format {version}")));
    }
    let n = r.uvarint()? as usize;
    if n > bytes.len() {
        return Err(Error::Format("ink point count exceeds data".into()));
    }
    let mut out = Vec::with_capacity(n);
    let (mut x, mut y, mut t) = (0i64, 0i64, 0u64);
    for _ in 0..n {
        // Wrapping: what was encoded never overflows; damaged data must not panic.
        x = x.wrapping_add(unzigzag(r.uvarint()?));
        y = y.wrapping_add(unzigzag(r.uvarint()?));
        let p = r.byte()? as f32 / 255.0;
        t = t.wrapping_add(r.uvarint()?);
        out.push(InkPoint { x: x as f32 / XY_SCALE, y: y as f32 / XY_SCALE, pressure: p, t_ms: t as u32 });
    }
    Ok(out)
}

/// Axis-aligned bounds of the samples, widened by half the maximum stroke width.
pub fn bounds(points: &[InkPoint], brush: &Brush) -> Rect {
    let mut r = Rect::EMPTY;
    for p in points {
        r = r.include(p.x, p.y);
    }
    r.inflate(brush.width * (1.0 + brush.thinning) * 0.5 + 1.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    pub const EMPTY: Rect = Rect { x0: f32::INFINITY, y0: f32::INFINITY, x1: f32::NEG_INFINITY, y1: f32::NEG_INFINITY };

    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect { x0: x0.min(x1), y0: y0.min(y1), x1: x0.max(x1), y1: y0.max(y1) }
    }
    pub fn is_empty(&self) -> bool {
        self.x0 > self.x1 || self.y0 > self.y1
    }
    pub fn include(self, x: f32, y: f32) -> Rect {
        Rect { x0: self.x0.min(x), y0: self.y0.min(y), x1: self.x1.max(x), y1: self.y1.max(y) }
    }
    pub fn inflate(self, d: f32) -> Rect {
        if self.is_empty() {
            return self;
        }
        Rect { x0: self.x0 - d, y0: self.y0 - d, x1: self.x1 + d, y1: self.y1 + d }
    }
    pub fn intersects(&self, o: &Rect) -> bool {
        !(self.is_empty() || o.is_empty()) && self.x0 <= o.x1 && o.x0 <= self.x1 && self.y0 <= o.y1 && o.y0 <= self.y1
    }
    pub fn union(self, o: &Rect) -> Rect {
        if o.is_empty() { return self; }
        Rect { x0: self.x0.min(o.x0), y0: self.y0.min(o.y0), x1: self.x1.max(o.x1), y1: self.y1.max(o.y1) }
    }
    pub fn width(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn height(&self) -> f32 {
        self.y1 - self.y0
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
}

impl Default for Rect {
    fn default() -> Self {
        Rect::EMPTY
    }
}

/// The filled outline of a stroke as one closed polygon (x, y pairs, page units).
///
/// Samples are smoothed (exponential "streamline"), the half width follows pressure, and both
/// ends get round caps. A single sample becomes a dot. The polygon is meant to be filled with
/// the non-zero rule, which also handles self-overlapping strokes.
pub fn outline(points: &[InkPoint], brush: &Brush) -> Vec<f32> {
    if brush.base_kind() == KIND_FILL {
        return fill_outline(points, brush);
    }
    if let Some(runs) = dash_runs(points, brush) {
        return joined(&runs, &Brush { kind: brush.base_kind(), ..*brush });
    }
    solid_outline(points, brush)
}

fn solid_outline(points: &[InkPoint], brush: &Brush) -> Vec<f32> {
    match shape(points, brush) {
        Shape::Empty => Vec::new(),
        Shape::Dot(x, y, r) => circle(x, y, r, 16),
        Shape::Body { pts, left, right, r_start, r_end } => {
            let n = pts.len();
            let mut out = Vec::with_capacity((2 * n + 24) * 2);
            for &(x, y) in &left {
                out.extend_from_slice(&[x, y]);
            }
            // End cap: half circle from the left side around to the right side.
            cap(&mut out, pts[n - 1].0, pts[n - 1].1, r_end, direction(&pts, n - 1), 10);
            for &(x, y) in right.iter().rev() {
                out.extend_from_slice(&[x, y]);
            }
            let (dx, dy) = direction(&pts, 0);
            cap(&mut out, pts[0].0, pts[0].1, r_start, (-dx, -dy), 10);
            out
        }
    }
}

/// Extra fills that make a pencil look like graphite: a middle band and a pressure-hungry
/// core, drawn translucent over the stroke's own [`outline`] (light pressure leaves only the
/// faint outer band, hard pressure a dark core). Empty for every other kind.
pub fn pencil_layers(points: &[InkPoint], brush: &Brush) -> Vec<Vec<f32>> {
    if brush.kind != KIND_PENCIL {
        return Vec::new();
    }
    let [mid, core] = pencil_layer_brushes(brush);
    vec![outline(points, &mid), outline(points, &core)]
}

/// The brushes of a pencil's middle band and core (see [`pencil_layers`]); for renderers that
/// build their own geometry (meshes on the laptop). Sized so the bands stay inside the outline
/// up to full pressure (the ragged rim survives).
pub fn pencil_layer_brushes(brush: &Brush) -> [Brush; 2] {
    [Brush { width: brush.width * 0.7, thinning: 0.5, ..*brush }, Brush { width: brush.width * 0.5, thinning: 0.95, ..*brush }]
}

/// The same shape as [`outline`], as triangles ready for a GPU: a strip between the left and
/// right edge plus fans for the round caps. Appends vertices (x, y pairs) to `xy` and triangle
/// indices (into all of `xy`) to `indices`, so many strokes batch into one mesh.
///
/// Unlike the polygon this needs no tessellation; where a stroke overlaps itself triangles
/// overlap too, which is invisible for opaque ink (draw translucent ink, e.g. highlighters,
/// into a separate layer).
pub fn outline_mesh(points: &[InkPoint], brush: &Brush, xy: &mut Vec<f32>, indices: &mut Vec<u32>) {
    if brush.base_kind() == KIND_FILL {
        fill_mesh(points, brush, xy, indices);
        return;
    }
    if let Some(runs) = dash_runs(points, brush) {
        let solid = Brush { kind: brush.base_kind(), ..*brush };
        for r in &runs {
            outline_mesh(r, &solid, xy, indices);
        }
        return;
    }
    let base = (xy.len() / 2) as u32;
    match shape(points, brush) {
        Shape::Empty => {}
        Shape::Dot(x, y, r) => {
            xy.extend_from_slice(&[x, y]);
            xy.extend(circle(x, y, r, 16));
            for k in 0..16u32 {
                indices.extend_from_slice(&[base, base + 1 + k, base + 1 + (k + 1) % 16]);
            }
        }
        Shape::Body { pts, left, right, r_start, r_end } => {
            let n = pts.len() as u32;
            xy.reserve((2 * n as usize + 24) * 2);
            indices.reserve((2 * n as usize + 20) * 3);
            // Vertex 2i = left[i], 2i + 1 = right[i].
            for (l, r) in left.iter().zip(&right) {
                xy.extend_from_slice(&[l.0, l.1, r.0, r.1]);
            }
            for i in 0..n - 1 {
                let (l0, r0, l1, r1) = (base + 2 * i, base + 2 * i + 1, base + 2 * i + 2, base + 2 * i + 3);
                indices.extend_from_slice(&[l0, r0, l1, l1, r0, r1]);
            }
            // Caps as fans around the end points, from one edge to the other.
            let mut fan = |cx: f32, cy: f32, r: f32, d: (f32, f32), from: u32, to: u32| {
                let center = (xy.len() / 2) as u32;
                xy.extend_from_slice(&[cx, cy]);
                let first_arc = center + 1;
                cap(xy, cx, cy, r, d, 10);
                let arc = (xy.len() / 2) as u32 - first_arc;
                let mut prev = from;
                for k in 0..arc {
                    indices.extend_from_slice(&[center, prev, first_arc + k]);
                    prev = first_arc + k;
                }
                indices.extend_from_slice(&[center, prev, to]);
            };
            let last = n - 1;
            fan(pts[last as usize].0, pts[last as usize].1, r_end, direction(&pts, last as usize), base + 2 * last, base + 2 * last + 1);
            let (dx, dy) = direction(&pts, 0);
            fan(pts[0].0, pts[0].1, r_start, (-dx, -dy), base + 1, base);
        }
    }
}

// --- filled areas ---------------------------------------------------------------------------

/// Where a horizontal line at `y` is inside the polygon (non-zero rule): spans (x0, x1).
fn spans(poly: &[(f32, f32)], y: f32) -> Vec<(f32, f32)> {
    let n = poly.len();
    let mut cross: Vec<(f32, i32)> = Vec::new();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.1 <= y && b.1 > y) || (b.1 <= y && a.1 > y) {
            let x = a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0);
            cross.push((x, if b.1 > a.1 { 1 } else { -1 }));
        }
    }
    cross.sort_by(|p, q| p.0.total_cmp(&q.0));
    let mut out = Vec::new();
    let (mut w, mut start) = (0, 0.0);
    for (x, d) in cross {
        let was = w;
        w += d;
        if was == 0 && w != 0 {
            start = x;
        } else if was != 0 && w == 0 && x > start {
            out.push((start, x));
        }
    }
    out
}

/// The hatch lines of a filled area (45°, `gap` apart): segments in page points.
fn hatch(poly: &[(f32, f32)], gap: f32) -> Vec<((f32, f32), (f32, f32))> {
    let (c, s) = (std::f32::consts::FRAC_1_SQRT_2, std::f32::consts::FRAC_1_SQRT_2);
    // Turned so the hatching runs horizontally, then back.
    let turned: Vec<(f32, f32)> = poly.iter().map(|&(x, y)| (x * c + y * s, -x * s + y * c)).collect();
    let (y0, y1) = turned.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
    let mut out = Vec::new();
    if !(y1 > y0) || turned.len() < 3 || !(y1 - y0).is_finite() {
        return out;
    }
    // At most 20 000 lines, however big the area (one from another device may be absurd).
    let gap = gap.max((y1 - y0) / 20_000.0);
    let mut y = (y0 / gap).ceil() * gap;
    while y <= y1 && out.len() < 20_000 {
        for (a, b) in spans(&turned, y) {
            out.push(((a * c - y * s, a * s + y * c), (b * c - y * s, b * s + y * c)));
        }
        let next = y + gap;
        if next <= y {
            break;
        }
        y = next;
    }
    out
}

fn hatch_gap(brush: &Brush) -> f32 {
    (brush.width * 3.0 + 3.0).max(4.0)
}

fn fill_poly(points: &[InkPoint]) -> Vec<(f32, f32)> {
    points.iter().filter(|p| p.x.is_finite() && p.y.is_finite()).map(|p| (p.x, p.y)).collect()
}

/// A filled area as one polygon: the area itself, or its hatch lines as thin bars joined by
/// zero-width links.
fn fill_outline(points: &[InkPoint], brush: &Brush) -> Vec<f32> {
    let poly = fill_poly(points);
    if brush.dash() == DASH_SOLID {
        return poly.iter().flat_map(|&(x, y)| [x, y]).collect();
    }
    let w = brush.width.max(0.3) / 2.0;
    let bars: Vec<[(f32, f32); 4]> = hatch(&poly, hatch_gap(brush)).into_iter().map(|(a, b)| bar(a, b, w)).collect();
    let mut out = Vec::with_capacity(bars.len() * 10);
    for q in &bars {
        for p in q {
            out.extend_from_slice(&[p.0, p.1]);
        }
        out.extend_from_slice(&[q[0].0, q[0].1]);
    }
    for q in bars.iter().rev().skip(1) {
        out.extend_from_slice(&[q[0].0, q[0].1]);
    }
    out
}

/// A segment as a bar `w` either side of it.
fn bar(a: (f32, f32), b: (f32, f32), w: f32) -> [(f32, f32); 4] {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l = dx.hypot(dy).max(1e-6);
    let (nx, ny) = (-dy / l * w, dx / l * w);
    [(a.0 + nx, a.1 + ny), (b.0 + nx, b.1 + ny), (b.0 - nx, b.1 - ny), (a.0 - nx, a.1 - ny)]
}

/// A filled area as triangles: rows half a point high across it (they meet without
/// overlapping, so a see-through fill is even), or its hatch lines.
fn fill_mesh(points: &[InkPoint], brush: &Brush, xy: &mut Vec<f32>, indices: &mut Vec<u32>) {
    let poly = fill_poly(points);
    let quad = |q: [(f32, f32); 4], xy: &mut Vec<f32>, indices: &mut Vec<u32>| {
        let base = (xy.len() / 2) as u32;
        for p in q {
            xy.extend_from_slice(&[p.0, p.1]);
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };
    if brush.dash() != DASH_SOLID {
        let w = brush.width.max(0.3) / 2.0;
        for (a, b) in hatch(&poly, hatch_gap(brush)) {
            quad(bar(a, b, w), xy, indices);
        }
        return;
    }
    let (y0, y1) = poly.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
    if !(y1 > y0) || poly.len() < 3 {
        return;
    }
    if !(y1 - y0).is_finite() {
        return;
    }
    let h = 0.5f32.max((y1 - y0) / 4000.0);
    let mut y = y0;
    while y < y1 {
        let mid = y + h / 2.0;
        for (a, b) in spans(&poly, mid) {
            quad([(a, y), (b, y), (b, y + h), (a, y + h)], xy, indices);
        }
        let next = y + h;
        if next <= y {
            break;
        }
        y = next;
    }
}

enum Shape {
    Empty,
    Dot(f32, f32, f32),
    Body { pts: Vec<(f32, f32, f32)>, left: Vec<(f32, f32)>, right: Vec<(f32, f32)>, r_start: f32, r_end: f32 },
}

/// Smoothed samples and the left/right edges of a stroke (shared by outline and mesh).
fn shape(points: &[InkPoint], brush: &Brush) -> Shape {
    let pts = smooth(points);
    if pts.is_empty() {
        return Shape::Empty;
    }
    let radius = |p: f32| -> f32 {
        let k = 1.0 - brush.thinning.clamp(0.0, 1.0) + brush.thinning.clamp(0.0, 1.0) * (p * 2.0).clamp(0.15, 2.0);
        (brush.width * 0.5 * k).max(0.25)
    };
    if pts.len() == 1 || path_length(&pts) < 0.5 {
        let p = pts[0];
        return Shape::Dot(p.0, p.1, radius(p.2));
    }
    let n = pts.len();
    let mut left = Vec::with_capacity(n);
    let mut right = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b) = (pts[i.saturating_sub(1)], pts[(i + 1).min(n - 1)]);
        let (mut dx, mut dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-6 {
            (dx, dy) = (1.0, 0.0);
        } else {
            dx /= len;
            dy /= len;
        }
        let r = radius(pts[i].2);
        left.push((pts[i].0 - dy * r, pts[i].1 + dx * r));
        right.push((pts[i].0 + dy * r, pts[i].1 - dx * r));
    }
    let (r_start, r_end) = (radius(pts[0].2), radius(pts[n - 1].2));
    Shape::Body { pts, left, right, r_start, r_end }
}

fn smooth(points: &[InkPoint]) -> Vec<(f32, f32, f32)> {
    let mut out: Vec<(f32, f32, f32)> = Vec::with_capacity(points.len());
    for p in points {
        match out.last() {
            None => out.push((p.x, p.y, p.pressure)),
            Some(&(lx, ly, lp)) => {
                // Streamline 0.5: halfway toward the new sample; drop near-duplicates.
                let (x, y) = (lx + (p.x - lx) * 0.5, ly + (p.y - ly) * 0.5);
                if (x - lx).abs() + (y - ly).abs() < 0.05 {
                    continue;
                }
                out.push((x, y, lp + (p.pressure - lp) * 0.5));
            }
        }
    }
    if let (Some(last), Some(&(lx, ly, _))) = (points.last(), out.last()) {
        // End exactly where the pen lifted.
        if (last.x - lx).abs() + (last.y - ly).abs() > 0.05 {
            out.push((last.x, last.y, last.pressure));
        }
    }
    out
}

fn path_length(p: &[(f32, f32, f32)]) -> f32 {
    p.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).sum()
}

fn direction(p: &[(f32, f32, f32)], i: usize) -> (f32, f32) {
    let n = p.len();
    let (a, b) = if i == 0 { (p[0], p[1.min(n - 1)]) } else { (p[i - 1], p[i]) };
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-6 { (1.0, 0.0) } else { (dx / len, dy / len) }
}

/// Half circle around (cx, cy) bulging in direction d, from the left normal to the right normal.
fn cap(out: &mut Vec<f32>, cx: f32, cy: f32, r: f32, d: (f32, f32), steps: usize) {
    let base = d.1.atan2(d.0) + std::f32::consts::FRAC_PI_2; // left normal
    for k in 1..steps {
        let a = base - std::f32::consts::PI * k as f32 / steps as f32;
        out.extend_from_slice(&[cx + r * a.cos(), cy + r * a.sin()]);
    }
}

fn circle(cx: f32, cy: f32, r: f32, steps: usize) -> Vec<f32> {
    (0..steps)
        .flat_map(|k| {
            let a = std::f32::consts::TAU * k as f32 / steps as f32;
            [cx + r * a.cos(), cy + r * a.sin()]
        })
        .collect()
}

fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}
fn unzigzag(v: u64) -> i64 {
    ((v >> 1) as i64) ^ -((v & 1) as i64)
}
fn put_uvarint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push((v as u8) | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}
impl Reader<'_> {
    fn byte(&mut self) -> Result<u8> {
        let v = *self.b.get(self.i).ok_or_else(|| Error::Format("ink data truncated".into()))?;
        self.i += 1;
        Ok(v)
    }
    fn uvarint(&mut self) -> Result<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.byte()?;
            v |= ((b & 0x7f) as u64) << shift;
            if b < 0x80 {
                return Ok(v);
            }
        }
        Err(Error::Format("varint too long".into()))
    }
}

#[cfg(test)]
mod tests {

    /// Winding number of (x, y) in a polygon (the non-zero rule fills where it isn't 0).
    fn winding(x: f32, y: f32, poly: &[f32]) -> i32 {
        let n = poly.len() / 2;
        let mut w = 0;
        for i in 0..n {
            let (x0, y0, x1, y1) = (poly[2 * i], poly[2 * i + 1], poly[2 * ((i + 1) % n)], poly[2 * ((i + 1) % n) + 1]);
            if y0 <= y {
                if y1 > y && (x1 - x0) * (y - y0) - (x - x0) * (y1 - y0) > 0.0 {
                    w += 1;
                }
            } else if y1 <= y && (x1 - x0) * (y - y0) - (x - x0) * (y1 - y0) < 0.0 {
                w -= 1;
            }
        }
        w
    }

    #[test]
    fn dashed_and_dotted_strokes_fill_their_dashes_and_leave_gaps() {
        let line: Vec<InkPoint> = (0..=100).map(|i| InkPoint { x: i as f32 * 2.0, y: 50.0, pressure: 0.5, t_ms: i * 8 }).collect();
        let pen = Brush { width: 2.0, thinning: 0.0, color: 0xFF000000, kind: KIND_PEN };
        assert!(dash_runs(&line, &pen).is_none(), "solid");
        let dashed = pen.with_dash(DASH_DASHED);
        assert_eq!((dashed.base_kind(), dashed.dash()), (KIND_PEN, DASH_DASHED));
        let runs = dash_runs(&line, &dashed).unwrap();
        // 200 pt at 8 pt dash + 6 pt gap: about 14 dashes.
        assert!((13..=16).contains(&runs.len()), "{}", runs.len());
        let poly = outline(&line, &dashed);
        assert_ne!(winding(4.0, 50.0, &poly), 0, "in the first dash");
        assert_eq!(winding(11.0, 50.0, &poly), 0, "in the first gap");
        assert_ne!(winding(18.0, 50.0, &poly), 0, "in the second dash");
        // Dots: one piece of no length each (a round dot), two widths apart.
        let dots = dash_runs(&line, &pen.with_dash(DASH_DOTTED)).unwrap();
        assert!(dots.iter().all(|r| r.len() <= 2) && dots.len() > 40, "{}", dots.len());
        // The mesh has the same pieces.
        let (mut xy, mut idx) = (Vec::new(), Vec::new());
        outline_mesh(&line, &dashed, &mut xy, &mut idx);
        assert!(!idx.is_empty());
        // Pencils and markers don't dash.
        assert_eq!(Brush { kind: KIND_MARKER, ..pen }.with_dash(DASH_DASHED).dash(), DASH_SOLID);
    }
    use super::*;

    fn line(n: usize) -> Vec<InkPoint> {
        (0..n).map(|i| InkPoint { x: 10.0 + i as f32 * 1.7, y: 20.0 + (i as f32 * 0.3).sin() * 5.0, pressure: 0.3 + (i % 7) as f32 * 0.1, t_ms: i as u32 * 4 }).collect()
    }

    #[test]
    fn encoding_round_trips_within_quantization() {
        let pts = line(300);
        let bytes = encode_points(&pts);
        assert!(bytes.len() < pts.len() * 7, "{} bytes for {} samples", bytes.len(), pts.len());
        let back = decode_points(&bytes).unwrap();
        assert_eq!(back.len(), pts.len());
        for (a, b) in pts.iter().zip(&back) {
            assert!((a.x - b.x).abs() <= 0.5 / XY_SCALE + 1e-4);
            assert!((a.y - b.y).abs() <= 0.5 / XY_SCALE + 1e-4);
            assert!((a.pressure - b.pressure).abs() <= 0.5 / 255.0 + 1e-4);
            assert_eq!(a.t_ms, b.t_ms);
        }
    }

    #[test]
    fn decoding_rejects_garbage() {
        assert!(decode_points(&[]).is_err());
        assert!(decode_points(&[9, 1, 2]).is_err());
        assert!(decode_points(&[FORMAT_V1, 200]).is_err());
        let mut ok = encode_points(&line(10));
        ok.truncate(ok.len() - 3);
        assert!(decode_points(&ok).is_err());
    }

    #[test]
    fn outline_is_closed_polygon_around_the_stroke() {
        let pts = line(50);
        let b = Brush::default();
        let poly = outline(&pts, &b);
        assert!(poly.len() >= 2 * 50 * 2, "too few vertices: {}", poly.len() / 2);
        let bb = bounds(&pts, &b);
        for xy in poly.chunks(2) {
            assert!(xy[0] >= bb.x0 && xy[0] <= bb.x1 && xy[1] >= bb.y0 && xy[1] <= bb.y1, "vertex {xy:?} outside {bb:?}");
        }
    }

    #[test]
    fn mesh_covers_the_outline_and_batches() {
        let pts = line(50);
        let b = Brush::default();
        let (mut xy, mut idx) = (Vec::new(), Vec::new());
        outline_mesh(&pts, &b, &mut xy, &mut idx);
        let first_vertices = xy.len() / 2;
        assert_eq!(idx.len() % 3, 0);
        assert!(idx.iter().all(|&i| (i as usize) < first_vertices));
        // Same edge and cap vertices as the polygon (plus the two cap centres).
        assert_eq!(first_vertices, outline(&pts, &b).len() / 2 + 2);
        let bb = bounds(&pts, &b);
        assert!(xy.chunks(2).all(|v| v[0] >= bb.x0 && v[0] <= bb.x1 && v[1] >= bb.y0 && v[1] <= bb.y1));
        // A second stroke appends with offset indices; a dot is a 16-triangle fan.
        outline_mesh(&[InkPoint { x: 5.0, y: 5.0, pressure: 0.5, t_ms: 0 }], &b, &mut xy, &mut idx);
        assert_eq!(xy.len() / 2, first_vertices + 17);
        assert!(idx[idx.len() - 48..].iter().all(|&i| i as usize >= first_vertices && (i as usize) < xy.len() / 2));
    }

    #[test]
    fn single_sample_is_a_dot() {
        let poly = outline(&[InkPoint { x: 5.0, y: 5.0, pressure: 0.5, t_ms: 0 }], &Brush::default());
        assert_eq!(poly.len(), 32);
        assert!(outline(&[], &Brush::default()).is_empty());
    }

    #[test]
    fn rect_intersection() {
        let a = Rect::new(0.0, 0.0, 10.0, 10.0);
        assert!(a.intersects(&Rect::new(5.0, 5.0, 20.0, 20.0)));
        assert!(!a.intersects(&Rect::new(11.0, 0.0, 20.0, 10.0)));
        assert!(!a.intersects(&Rect::EMPTY));
    }

    #[test]
    fn pencil_layers_only_for_pencils_and_narrower() {
        let pts: Vec<InkPoint> = (0..20).map(|i| InkPoint { x: i as f32 * 3.0, y: 10.0, pressure: 0.2, t_ms: i * 8 }).collect();
        assert!(pencil_layers(&pts, &Brush::default()).is_empty());
        let pencil = Brush { width: 4.0, thinning: 0.2, kind: KIND_PENCIL, ..Brush::default() };
        let layers = pencil_layers(&pts, &pencil);
        assert_eq!(layers.len(), 2);
        let height = |o: &[f32]| {
            let ys = o.chunks(2).map(|c| c[1]);
            ys.clone().fold(f32::MIN, f32::max) - ys.fold(f32::MAX, f32::min)
        };
        let full = height(&outline(&pts, &pencil));
        assert!(height(&layers[0]) < full && height(&layers[1]) < height(&layers[0]), "light pressure: core thinnest");
        let hard: Vec<InkPoint> = pts.iter().map(|p| InkPoint { pressure: 1.0, ..*p }).collect();
        let full = height(&outline(&hard, &pencil));
        assert!(pencil_layers(&hard, &pencil).iter().all(|l| height(l) < full), "full pressure: bands still inside");
    }
}
