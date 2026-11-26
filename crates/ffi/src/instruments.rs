//! The drawing instruments for the tablet (core `instruments`): ruler, Geodreieck, protractor,
//! compass. `kind`: 0 ruler, 1 Geodreieck, 2 protractor.

use n0te_core::instruments::{self, Compass, Instrument, Kind};

fn kind(k: u8) -> Kind {
    match k {
        1 => Kind::SetSquare,
        2 => Kind::Protractor,
        _ => Kind::Ruler,
    }
}

fn at(k: u8, cx: f32, cy: f32, angle: f32) -> Instrument {
    Instrument { kind: kind(k), cx, cy, angle }
}

/// How an instrument looks, in its own coordinates (draw it translated to its point and turned
/// by its angle): the `outline` (x, y pairs), the scale's `marks` (x0, y0, x1, y1, level) and
/// numbers.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiInstrumentShape {
    pub outline: Vec<f32>,
    pub edges: Vec<f32>,
    pub marks: Vec<f32>,
    pub labels: Vec<FfiScaleLabel>,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiScaleLabel {
    pub x: f32,
    pub y: f32,
    pub text: String,
    pub turn: f32,
    pub size: f32,
}

#[uniffi::export]
pub fn instrument_shape(kind: u8) -> FfiInstrumentShape {
    let i = at(kind, 0.0, 0.0, 0.0);
    let (marks, labels) = i.scale();
    FfiInstrumentShape {
        outline: i.outline().into_iter().flat_map(|(x, y)| [x, y]).collect(),
        edges: i.edges().into_iter().flat_map(|(a, b)| [a.0, a.1, b.0, b.1]).collect(),
        marks: marks.into_iter().flat_map(|m| [m.a.0, m.a.1, m.b.0, m.b.1, m.level as f32]).collect(),
        labels: labels.into_iter().map(|l| FfiScaleLabel { x: l.x, y: l.y, text: l.text, turn: l.turn, size: l.size }).collect(),
    }
}

#[uniffi::export]
pub fn instrument_contains(kind: u8, cx: f32, cy: f32, angle: f32, x: f32, y: f32) -> bool {
    at(kind, cx, cy, angle).contains(x, y)
}

/// The edge a pen at (x, y) is at, within `tol` page points: its index, or -1.
#[uniffi::export]
pub fn instrument_edge_at(kind: u8, cx: f32, cy: f32, angle: f32, x: f32, y: f32, tol: f32) -> i32 {
    at(kind, cx, cy, angle).edge_at(x, y, tol).map_or(-1, |e| e as i32)
}

/// (x, y) moved onto edge `edge`, half a line `width` outside it.
#[uniffi::export]
#[allow(clippy::too_many_arguments)]
pub fn instrument_snap(kind: u8, cx: f32, cy: f32, angle: f32, x: f32, y: f32, edge: u32, width: f32) -> Vec<f32> {
    let (sx, sy) = at(kind, cx, cy, angle).snap(x, y, edge as usize, width);
    vec![sx, sy]
}

/// The angle turned to `raw`, snapping (ruler 45°, the others 15°).
#[uniffi::export]
pub fn instrument_turned(kind: u8, raw: f32) -> f32 {
    at(kind, 0.0, 0.0, 0.0).turned_to(raw).angle
}

/// What the degree scale reads at (x, y) (Geodreieck, protractor), else null.
#[uniffi::export]
pub fn instrument_reads(kind: u8, cx: f32, cy: f32, angle: f32, x: f32, y: f32) -> Option<f32> {
    at(kind, cx, cy, angle).reads(x, y)
}

/// The compass's arc round (cx, cy) with radius `r` from angle `a0` turning by `sweep`, as
/// stroke samples (packed x, y, pressure, t).
#[uniffi::export]
#[allow(clippy::too_many_arguments)]
pub fn compass_arc(cx: f32, cy: f32, r: f32, a0: f32, sweep: f32, pressure: f32, t0: u32, t1: u32) -> Vec<u8> {
    crate::pack_points(&Compass { cx, cy, r }.arc(a0, sweep, pressure, t0, t1))
}

/// The radius as it reads ("3,5 cm").
#[uniffi::export]
pub fn compass_label(r: f32) -> String {
    Compass { cx: 0.0, cy: 0.0, r }.label(false)
}

/// A compass radius snapped to half centimetres, or whole squares (`square` > 0).
#[uniffi::export]
pub fn compass_snap_radius(r: f32, square: f32) -> f32 {
    instruments::snap_radius(r, (square > 0.0).then_some(square))
}

/// Where a pin snaps near (x, y): stroke ends and crossings of straight strokes, from each
/// stroke's `ends` (x0, y0, x1, y1, length: 5 numbers each); empty when nothing is near.
#[uniffi::export]
pub fn snap_pin(ends: Vec<f32>, x: f32, y: f32, radius: f32) -> Vec<f32> {
    let s: Vec<((f32, f32), (f32, f32), f32)> = ends.chunks_exact(5).map(|c| ((c[0], c[1]), (c[2], c[3]), c[4])).collect();
    instruments::snap_point_ends(&s, x, y, radius).map(|(a, b)| vec![a, b]).unwrap_or_default()
}
