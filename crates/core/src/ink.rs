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
}

impl Default for Brush {
    fn default() -> Self {
        Brush { width: 2.0, thinning: 0.6, color: 0xFF111111 }
    }
}

/// Quantization of coordinates: 1/64 pt is far below what a pen can resolve.
const XY_SCALE: f32 = 64.0;
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
        let qx = (p.x * XY_SCALE).round() as i64;
        let qy = (p.y * XY_SCALE).round() as i64;
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
        x += unzigzag(r.uvarint()?);
        y += unzigzag(r.uvarint()?);
        let p = r.byte()? as f32 / 255.0;
        t += r.uvarint()?;
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
}

/// The filled outline of a stroke as one closed polygon (x, y pairs, page units).
///
/// Samples are smoothed (exponential "streamline"), the half width follows pressure, and both
/// ends get round caps. A single sample becomes a dot. The polygon is meant to be filled with
/// the non-zero rule, which also handles self-overlapping strokes.
pub fn outline(points: &[InkPoint], brush: &Brush) -> Vec<f32> {
    let pts = smooth(points);
    if pts.is_empty() {
        return Vec::new();
    }
    let radius = |p: f32| -> f32 {
        let k = 1.0 - brush.thinning.clamp(0.0, 1.0) + brush.thinning.clamp(0.0, 1.0) * (p * 2.0).clamp(0.15, 2.0);
        (brush.width * 0.5 * k).max(0.25)
    };
    if pts.len() == 1 || path_length(&pts) < 0.5 {
        let p = pts[0];
        return circle(p.0, p.1, radius(p.2), 16);
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
    let mut out = Vec::with_capacity((2 * n + 24) * 2);
    for &(x, y) in &left {
        out.extend_from_slice(&[x, y]);
    }
    // End cap: half circle from the left side around to the right side.
    let (ex, ey, er) = (pts[n - 1].0, pts[n - 1].1, radius(pts[n - 1].2));
    cap(&mut out, ex, ey, er, direction(&pts, n - 1), 10);
    for &(x, y) in right.iter().rev() {
        out.extend_from_slice(&[x, y]);
    }
    let (sx, sy, sr) = (pts[0].0, pts[0].1, radius(pts[0].2));
    let (dx, dy) = direction(&pts, 0);
    cap(&mut out, sx, sy, sr, (-dx, -dy), 10);
    out
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
}
