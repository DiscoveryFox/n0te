//! UniFFI bindings of n0te-core for the Android app (Kotlin).
//!
//! Hot paths pass packed little-endian f32 bytes (samples as x,y,pressure,t quads; outlines as
//! x,y pairs): UniFFI turns `Vec<f32>` into a boxed `List<Float>` in Kotlin, while `Vec<u8>`
//! arrives as one `ByteArray` (read it with `ByteBuffer.order(LITTLE_ENDIAN).asFloatBuffer()`).
//!
//! PDF (import, tiles, search, export): see [`pdf`].

pub mod pdf;

use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Instant;

use n0te_core::ink::{self, Brush, InkPoint, Rect};
use n0te_core::notebook::{self as nbm, Background, OutlineRow, SheetSpec, Side};

uniffi::setup_scaffolding!();

mod android;
mod edit;
pub mod sync;

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum N0teError {
    #[error("{msg}")]
    Failed { msg: String },
}

impl From<n0te_core::Error> for N0teError {
    fn from(e: n0te_core::Error) -> Self {
        N0teError::Failed { msg: e.to_string() }
    }
}

type R<T> = Result<T, N0teError>;

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    // A panic while holding the lock leaves the data usable (every write is a whole op).
    m.lock().unwrap_or_else(|p| p.into_inner())
}

#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct FfiBrush {
    pub width: f32,
    pub thinning: f32,
    /// 0xAARRGGBB
    pub color: u32,
    /// 0 pen, 1 pencil (grain texture), 2 marker.
    pub kind: u8,
}

impl From<FfiBrush> for Brush {
    fn from(b: FfiBrush) -> Self {
        Brush { width: b.width, thinning: b.thinning, color: b.color, kind: b.kind }
    }
}
impl From<Brush> for FfiBrush {
    fn from(b: Brush) -> Self {
        FfiBrush { width: b.width, thinning: b.thinning, color: b.color, kind: b.kind }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiStroke {
    pub id: String,
    pub brush: FfiBrush,
    /// x0, y0, x1, y1
    pub bounds: Vec<f32>,
    /// Closed outline polygon as packed LE f32 x,y pairs; fill with the nonzero rule.
    pub outline: Vec<u8>,
    /// Pencil only: the middle band and the core (same packing), drawn translucent on top.
    pub layers: Vec<Vec<u8>>,
    /// Where the stroke starts and ends, each with the way it runs there (unit, pointing back
    /// into the stroke): x0, y0, dx0, dy0, x1, y1, dx1, dy1, then its length. For attaching
    /// arrowheads; empty for a dot.
    #[uniffi(default)]
    pub ends: Vec<f32>,
}

/// A stroke's ends and the way it runs there (see [`FfiStroke::ends`]): the tangent at each
/// end, read from 1.5 to 8 pt in (a curving end still points the way it ends, not the way it
/// went a while before). A little flick as the pen lifted (its last few points turning off
/// sharply) is left out: the end is where it began, the way is the stroke's own.
fn stroke_ends(pts: &[ink::InkPoint]) -> Vec<f32> {
    let n = pts.len();
    if n < 2 {
        return Vec::new();
    }
    let seg: Vec<f32> = pts.windows(2).map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y)).collect();
    let len: f32 = seg.iter().sum();
    if len < 1.0 {
        return Vec::new();
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
    let end = |from_end: bool| -> [f32; 4] {
        let tip = if from_end { pts[n - 1] } else { pts[0] };
        let (near, far) = (1.5f32.min(len / 8.0), 8f32.min(len / 3.0));
        let (ax, ay) = along(from_end, near);
        let (bx, by) = along(from_end, far);
        let (dx, dy) = unit(bx - ax, by - ay);
        // A flick: the last 4 pt run off at a sharp angle to the 4–12 pt before them.
        let (h0, h1) = (4f32.min(len / 6.0), 12f32.min(len / 2.0));
        let (px, py) = along(from_end, h0);
        let (qx, qy) = along(from_end, h1);
        let (fx, fy) = unit(px - tip.x, py - tip.y);
        let (gx, gy) = unit(qx - px, qy - py);
        if len > 20.0 && fx * gx + fy * gy < 0.5 {
            return [px, py, gx, gy];
        }
        [tip.x, tip.y, dx, dy]
    };
    let a = end(false);
    let b = end(true);
    vec![a[0], a[1], a[2], a[3], b[0], b[1], b[2], b[3], len]
}

/// Typed text on a page (Markdown), position and width in page points.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiTextBox {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub size: f32,
    pub text: String,
}

/// An image on a page: a blob placed at (x, y), w × h page points.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiImage {
    pub id: String,
    pub blob: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// A coordinate system instead of a picture (blob empty): selected, moved and resized
    /// like an image, drawn from [`plot_strokes`].
    #[uniffi(default)]
    pub plot: Option<FfiPlot>,
}

/// One function of a plot: what was typed and its ink (0xAARRGGBB), its line (0 solid,
/// 1 dashed, 2 dotted) and an area hatched under it.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiPlotFn {
    pub expr: String,
    pub color: u32,
    #[uniffi(default = 0)]
    pub style: u8,
    #[uniffi(default)]
    pub area: Option<FfiPlotArea>,
}

/// An area hatched under a function over [from, to], down to the x axis (to_fn None) or to
/// another of the plot's functions.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiPlotArea {
    pub from: f64,
    pub to: f64,
    pub to_fn: Option<u32>,
}

/// A point marked by hand on a plot.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiPlotPoint {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

/// A coordinate system on a page (see the core's `Plot`): box in page points, ranges, look,
/// functions.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct FfiPlot {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    pub grid: bool,
    pub clean: bool,
    pub color: u32,
    pub width: f32,
    pub fns: Vec<FfiPlotFn>,
    /// Function names at the curves.
    #[uniffi(default = false)]
    pub names: bool,
    /// The x axis in steps of π.
    #[uniffi(default = false)]
    pub pi: bool,
    #[uniffi(default = "")]
    pub x_name: String,
    #[uniffi(default = "")]
    pub y_name: String,
    /// What it marks by itself: 1 zeros, 2 highs and lows, 4 crossings, 8 turning points.
    #[uniffi(default = 0)]
    pub marks: u8,
    /// Coordinates at marked points, sizes of areas.
    #[uniffi(default = false)]
    pub coords: bool,
    #[uniffi(default)]
    pub points: Vec<FfiPlotPoint>,
}

fn style_code(s: n0te_core::plot::LineStyle) -> u8 {
    match s {
        n0te_core::plot::LineStyle::Solid => 0,
        n0te_core::plot::LineStyle::Dashed => 1,
        n0te_core::plot::LineStyle::Dotted => 2,
    }
}

impl From<n0te_core::plot::Plot> for FfiPlot {
    fn from(p: n0te_core::plot::Plot) -> Self {
        FfiPlot {
            id: p.id,
            x: p.x,
            y: p.y,
            w: p.w,
            h: p.h,
            x_min: p.x_min,
            x_max: p.x_max,
            y_min: p.y_min,
            y_max: p.y_max,
            grid: p.grid,
            clean: p.clean,
            color: p.color,
            width: p.width,
            fns: p
                .fns
                .into_iter()
                .map(|f| FfiPlotFn { expr: f.expr, color: f.color, style: style_code(f.style), area: f.area.map(|a| FfiPlotArea { from: a.from, to: a.to, to_fn: a.to_fn.map(|j| j as u32) }) })
                .collect(),
            names: p.names,
            pi: p.pi,
            x_name: p.x_name,
            y_name: p.y_name,
            marks: p.marks,
            coords: p.coords,
            points: p.points.into_iter().map(|q| FfiPlotPoint { name: q.name, x: q.x, y: q.y }).collect(),
        }
    }
}

impl From<FfiPlot> for n0te_core::plot::Plot {
    fn from(p: FfiPlot) -> Self {
        n0te_core::plot::Plot {
            id: p.id,
            x: p.x,
            y: p.y,
            w: p.w,
            h: p.h,
            x_min: p.x_min,
            x_max: p.x_max,
            y_min: p.y_min,
            y_max: p.y_max,
            grid: p.grid,
            clean: p.clean,
            color: p.color,
            width: p.width,
            fns: p
                .fns
                .into_iter()
                .map(|f| n0te_core::plot::PlotFn {
                    expr: f.expr,
                    color: f.color,
                    style: match f.style {
                        1 => n0te_core::plot::LineStyle::Dashed,
                        2 => n0te_core::plot::LineStyle::Dotted,
                        _ => n0te_core::plot::LineStyle::Solid,
                    },
                    area: f.area.map(|a| n0te_core::plot::Area { from: a.from, to: a.to, to_fn: a.to_fn.map(|j| j as usize) }),
                })
                .collect(),
            names: p.names,
            pi: p.pi,
            x_name: p.x_name,
            y_name: p.y_name,
            marks: p.marks,
            coords: p.coords,
            points: p.points.into_iter().map(|q| n0te_core::plot::PlotPoint { name: q.name, x: q.x, y: q.y }).collect(),
        }
    }
}

/// A plot as ink (page points): grid, axes, numbers, curves, ready to draw like strokes.
#[uniffi::export]
pub fn plot_strokes(plot: FfiPlot) -> Vec<FfiStroke> {
    n0te_core::plot::render(&plot.into())
        .into_iter()
        .map(|(brush, points)| to_ffi(n0te_core::sheet::Stroke { id: String::new(), bounds: n0te_core::sheet::bounds_of(&points, brush.width), brush, points }))
        .collect()
}

/// What is wrong with a typed function (German), or None when it can be drawn.
#[uniffi::export]
pub fn plot_check(expr: String) -> Option<String> {
    n0te_core::plot::check(&expr).err()
}

/// A y range that shows the plot's functions over its x range: [min, max], or empty.
#[uniffi::export]
pub fn plot_fit_y(plot: FfiPlot) -> Vec<f64> {
    n0te_core::plot::Plot::from(plot).fit_y().map(|(a, b)| vec![a, b]).unwrap_or_default()
}

/// What is wrong with each of the plot's functions (None: fine), references to the others
/// resolved (g(x) = f'(x) + 1).
#[uniffi::export]
pub fn plot_errors(plot: FfiPlot) -> Vec<Option<String>> {
    n0te_core::plot::Plot::from(plot).errors()
}

/// LaTeX from the handwriting recogniser as a function to type; Err: why it can't be drawn.
#[uniffi::export]
pub fn plot_from_latex(latex: String) -> R<String> {
    n0te_core::plot::latex_to_expr(&latex).map_err(|msg| N0teError::Failed { msg })
}

/// Points typed by hand ("A(1|2); B(-1|0,5)"); Err: what is wrong.
#[uniffi::export]
pub fn plot_parse_points(text: String) -> R<Vec<FfiPlotPoint>> {
    n0te_core::plot::parse_points(&text).map(|v| v.into_iter().map(|q| FfiPlotPoint { name: q.name, x: q.x, y: q.y }).collect()).map_err(|msg| N0teError::Failed { msg })
}

/// Points as they are typed.
#[uniffi::export]
pub fn plot_format_points(points: Vec<FfiPlotPoint>) -> String {
    n0te_core::plot::format_points(&points.into_iter().map(|q| n0te_core::plot::PlotPoint { name: q.name, x: q.x, y: q.y }).collect::<Vec<_>>())
}

/// The plot without its function number `index`: later ones move up a name, references and
/// areas follow.
#[uniffi::export]
pub fn plot_remove_fn(plot: FfiPlot, index: u32) -> FfiPlot {
    n0te_core::plot::Plot::from(plot).remove_fn(index as usize).into()
}

/// The x range widened to whole π/2 (for an axis in steps of π): [min, max].
#[uniffi::export]
pub fn plot_pi_range(plot: FfiPlot) -> Vec<f64> {
    let (a, b) = n0te_core::plot::Plot::from(plot).pi_range();
    vec![a, b]
}

/// The y range for the same units as across: [min, max].
#[uniffi::export]
pub fn plot_equal_units(plot: FfiPlot) -> Vec<f64> {
    let (a, b) = n0te_core::plot::Plot::from(plot).equal_units();
    vec![a, b]
}

/// A stroke's brush and samples (packed LE f32 x, y, pressure, t_ms), for copy/paste.
/// A stroke's id, box (page points) and kind (0 pen, 1 pencil, 2 highlighter …).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiStrokeBox {
    pub id: String,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub kind: u8,
}

/// A line of handwriting as the recogniser read it (see the core's `IndexLine`).
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiIndexLine {
    pub key: String,
    pub text: String,
    pub alts: Vec<String>,
    pub strokes: Vec<String>,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl From<n0te_core::sheet::IndexLine> for FfiIndexLine {
    fn from(l: n0te_core::sheet::IndexLine) -> Self {
        FfiIndexLine { key: l.key, text: l.text, alts: l.alts, strokes: l.strokes, x0: l.x0, y0: l.y0, x1: l.x1, y1: l.y1 }
    }
}
impl From<FfiIndexLine> for n0te_core::sheet::IndexLine {
    fn from(l: FfiIndexLine) -> Self {
        n0te_core::sheet::IndexLine { key: l.key, text: l.text, alts: l.alts, strokes: l.strokes, x0: l.x0, y0: l.y0, x1: l.x1, y1: l.y1 }
    }
}

/// A heading the user marked: its words, its strokes, where they are now.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiHeading {
    pub id: String,
    pub text: String,
    pub strokes: Vec<String>,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// A heading on a page, for the outline: the page (node, sheet id), the heading's id, words, height on the page.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiPageHeading {
    pub node: String,
    pub sheet_id: String,
    pub id: String,
    pub text: String,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiHitKind {
    Ink,
    Text,
    Heading,
    Pdf,
    Plot,
}

/// Something found in a notebook: on which sheet (node and sheet id), what kind, its text, where
/// on the page (page points), how many slips it took to match.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiSearchHit {
    pub node: String,
    pub sheet_id: String,
    pub kind: FfiHitKind,
    pub text: String,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub slips: u32,
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiStrokeData {
    pub brush: FfiBrush,
    pub samples: Vec<u8>,
}

pub(crate) fn pack_points(pts: &[InkPoint]) -> Vec<u8> {
    let mut out = Vec::with_capacity(pts.len() * 16);
    for p in pts {
        for v in [p.x, p.y, p.pressure, p.t_ms as f32] {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out
}

pub(crate) fn points_of(samples: &[u8]) -> Vec<InkPoint> {
    points(samples)
}

pub(crate) fn to_ffi_stroke(s: n0te_core::sheet::Stroke) -> FfiStroke {
    to_ffi(s)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiBackground {
    Blank,
    Lines,
    Grid,
    Dots,
    Cornell,
    /// Flowing text page (Markdown body).
    Text,
}

impl From<FfiBackground> for Background {
    fn from(b: FfiBackground) -> Self {
        match b {
            FfiBackground::Blank => Background::Blank,
            FfiBackground::Lines => Background::Lines,
            FfiBackground::Grid => Background::Grid,
            FfiBackground::Dots => Background::Dots,
            FfiBackground::Cornell => Background::Cornell,
            FfiBackground::Text => Background::Text,
        }
    }
}
impl From<Background> for FfiBackground {
    fn from(b: Background) -> Self {
        match b {
            Background::Blank => FfiBackground::Blank,
            Background::Lines => FfiBackground::Lines,
            Background::Grid => FfiBackground::Grid,
            Background::Dots => FfiBackground::Dots,
            Background::Cornell => FfiBackground::Cornell,
            Background::Text => FfiBackground::Text,
        }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiSheetSpec {
    /// 0 = infinite board
    pub width: f32,
    pub height: f32,
    pub background: FfiBackground,
    pub spacing: f32,
    pub pdf_blob: Option<String>,
    pub pdf_page: u32,
    /// PDF pages: blank space inserted into the PDF, pairs (at, height) in page points down the
    /// PDF as printed; `height` includes it.
    #[uniffi(default)]
    pub gaps: Vec<f32>,
}

fn pairs(v: &[f32]) -> Vec<(f32, f32)> {
    v.chunks_exact(2).map(|c| (c[0], c[1])).collect()
}

fn flat(v: &[(f32, f32)]) -> Vec<f32> {
    v.iter().flat_map(|&(a, b)| [a, b]).collect()
}

impl From<FfiSheetSpec> for SheetSpec {
    fn from(s: FfiSheetSpec) -> Self {
        let gaps = pairs(&s.gaps);
        SheetSpec { width: s.width, height: s.height, background: s.background.into(), spacing: s.spacing, pdf: s.pdf_blob.map(|b| (b, s.pdf_page)), gaps }
    }
}

/// A stretch of a page's background and where it lies: from `src` (down the background as
/// printed) for `len`, drawn from `dst` down the page.
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct FfiBand {
    pub src: f32,
    pub len: f32,
    pub dst: f32,
}

/// How a page's background lies on it: one band, one more per gap of inserted space.
#[uniffi::export]
pub fn background_bands(spec: FfiSheetSpec) -> Vec<FfiBand> {
    SheetSpec::from(spec).bands().into_iter().map(|b| FfiBand { src: b.src, len: b.len, dst: b.dst }).collect()
}

/// What lies below a line on a page: its top and bottom (none if nothing does).
#[derive(Debug, Clone, Copy, uniffi::Record)]
pub struct FfiBelow {
    pub top: Option<f32>,
    pub bottom: Option<f32>,
}

/// Space inserted or taken out: how far what lies below moves, the page's height and gaps after.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiSpacePlan {
    pub dy: f32,
    pub height: f32,
    pub gaps: Vec<f32>,
}

/// Space inserted at `y` (`dy` > 0) or taken out (`dy` < 0) on a page (see the core's `plan_space`):
/// nothing leaves the page, a PDF page splits at the line.
#[uniffi::export]
pub fn plan_space(spec: FfiSheetSpec, y: f32, dy: f32, below: FfiBelow) -> FfiSpacePlan {
    let p = n0te_core::notebook::plan_space(&spec.into(), y, dy, n0te_core::notebook::Below { top: below.top, bottom: below.bottom });
    FfiSpacePlan { dy: p.dy, height: p.height, gaps: flat(&p.gaps) }
}
impl From<SheetSpec> for FfiSheetSpec {
    fn from(s: SheetSpec) -> Self {
        let (pdf_blob, pdf_page) = match s.pdf {
            Some((b, p)) => (Some(b), p),
            None => (None, 0),
        };
        let gaps = flat(&s.gaps);
        FfiSheetSpec { width: s.width, height: s.height, background: s.background.into(), spacing: s.spacing, pdf_blob, pdf_page, gaps }
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiSideSheet {
    pub node: String,
    pub sheet_id: String,
    pub spec: FfiSheetSpec,
}

#[derive(Debug, Clone, uniffi::Enum)]
pub enum FfiOutlineRow {
    Group { node: String, depth: u32, title: String, collapsed: bool, count: u32 },
    Sheet { node: String, depth: u32, sheet_id: String, title: String, spec: FfiSheetSpec, left: Vec<FfiSideSheet>, right: Vec<FfiSideSheet> },
}

fn side_sheets(v: Vec<nbm::SideSheet>) -> Vec<FfiSideSheet> {
    v.into_iter().map(|s| FfiSideSheet { node: s.node, sheet_id: s.sheet_id, spec: s.spec.into() }).collect()
}

impl From<OutlineRow> for FfiOutlineRow {
    fn from(r: OutlineRow) -> Self {
        match r {
            OutlineRow::Group { node, depth, title, collapsed, count } => FfiOutlineRow::Group { node, depth, title, collapsed, count },
            OutlineRow::Sheet { node, depth, sheet_id, title, spec, left, right } => {
                FfiOutlineRow::Sheet { node, depth, sheet_id, title, spec: spec.into(), left: side_sheets(left), right: side_sheets(right) }
            }
        }
    }
}

/// Samples as packed LE f32 x,y,pressure,t_ms quads.
fn points(samples: &[u8]) -> Vec<InkPoint> {
    samples
        .chunks_exact(16)
        .map(|c| {
            let f = |i: usize| f32::from_le_bytes(c[i..i + 4].try_into().expect("4 bytes"));
            InkPoint { x: f(0), y: f(4), pressure: f(8), t_ms: f(12).max(0.0) as u32 }
        })
        .collect()
}

fn pack(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for x in v {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

fn to_ffi(s: n0te_core::sheet::Stroke) -> FfiStroke {
    FfiStroke {
        outline: pack(&ink::outline(&s.points, &s.brush)),
        layers: ink::pencil_layers(&s.points, &s.brush).iter().map(|l| pack(l)).collect(),
        bounds: vec![s.bounds.x0, s.bounds.y0, s.bounds.x1, s.bounds.y1],
        ends: stroke_ends(&s.points),
        brush: s.brush.into(),
        id: s.id,
    }
}

/// Outline of a stroke being drawn (no storage), for the live pen layer. Packed LE f32 in/out.
/// What would stick out of pages made smaller or turned: counts, pages, and a sentence
/// ("3 Striche und 1 Bild").
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiOutside {
    pub strokes: u32,
    pub images: u32,
    pub texts: u32,
    pub pages: u32,
    pub text: String,
}

/// A named paper size, upright (width ≤ height), in page points.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiPageFormat {
    pub id: String,
    pub name: String,
    pub width: f32,
    pub height: f32,
}

/// The paper sizes every device offers (A4, A5, …), upright.
#[uniffi::export]
pub fn page_formats() -> Vec<FfiPageFormat> {
    n0te_core::format::FORMATS.iter().map(|f| FfiPageFormat { id: f.id.into(), name: f.name.into(), width: f.width, height: f.height }).collect()
}

/// "A4 hoch", "A5 quer", "Quadrat", or "200 × 150 mm".
#[uniffi::export]
pub fn describe_page_size(width: f32, height: f32) -> String {
    n0te_core::format::describe(width, height)
}

/// The id of the format a page has (either way round), or "" for a size of its own.
#[uniffi::export]
pub fn page_format_id(width: f32, height: f32) -> String {
    n0te_core::format::find(width, height).map(|(f, _)| f.id.to_string()).unwrap_or_default()
}

#[uniffi::export]
pub fn stroke_outline(brush: FfiBrush, samples: Vec<u8>) -> Vec<u8> {
    pack(&ink::outline(&points(&samples), &brush.into()))
}

/// The pencil layers of a stroke being drawn (empty unless the brush is a pencil).
#[uniffi::export]
pub fn stroke_layers(brush: FfiBrush, samples: Vec<u8>) -> Vec<Vec<u8>> {
    ink::pencil_layers(&points(&samples), &brush.into()).iter().map(|l| pack(l)).collect()
}

/// Named so it doesn't clash with JNA's `com.sun.jna.Library` in the generated Kotlin.
#[derive(uniffi::Object)]
pub struct NoteLibrary {
    inner: n0te_core::store::Library,
}

#[uniffi::export]
impl NoteLibrary {
    #[uniffi::constructor]
    pub fn open(root: String) -> R<Arc<Self>> {
        Ok(Arc::new(NoteLibrary { inner: n0te_core::store::Library::open(root)? }))
    }

    pub fn notebook_ids(&self) -> R<Vec<String>> {
        Ok(self.inner.notebook_ids()?)
    }

    pub fn create_notebook(self: Arc<Self>, title: String) -> R<Arc<NotebookHandle>> {
        let nb = self.inner.create_notebook(&title)?;
        Ok(NotebookHandle::new(self, nb))
    }

    pub fn load_notebook(self: Arc<Self>, id: String) -> R<Arc<NotebookHandle>> {
        let nb = self.inner.load_notebook(&id)?;
        Ok(NotebookHandle::new(self, nb))
    }

    pub fn put_blob(&self, data: Vec<u8>) -> R<String> {
        Ok(self.inner.put_blob(&data)?)
    }

    /// Where a blob lives on disk (it may not be there yet: synced in the background).
    pub fn blob_path(&self, id: String) -> R<String> {
        Ok(self.inner.blob_path(&id)?.to_string_lossy().into_owned())
    }

    /// Writes the notebook as a `.n0te` bundle to `path` (to carry it to another device).
    pub fn export_bundle(&self, id: String, path: String) -> R<()> {
        let bytes = self.inner.export_bundle(&id)?;
        std::fs::write(&path, bytes).map_err(|e| N0teError::Failed { msg: e.to_string() })?;
        Ok(())
    }

    /// Imports a `.n0te` bundle (merging into the copy here, if any); returns the notebook id.
    pub fn import_bundle(&self, path: String) -> R<String> {
        let bytes = std::fs::read(&path).map_err(|e| N0teError::Failed { msg: e.to_string() })?;
        Ok(self.inner.import_bundle(&bytes)?)
    }

    /// Moves a notebook to the trash folder (out of the library, still on disk).
    pub fn trash_notebook(&self, id: String) -> R<()> {
        Ok(self.inner.trash_notebook(&id)?)
    }
}

#[derive(uniffi::Object)]
pub struct NotebookHandle {
    lib: Arc<NoteLibrary>,
    pub(crate) nb: Mutex<n0te_core::notebook::Notebook>,
    /// Sheets opened through this handle, so an export sees their unsaved strokes.
    open_sheets: Mutex<Vec<(String, Weak<SheetHandle>)>>,
}

impl NotebookHandle {
    fn new(lib: Arc<NoteLibrary>, nb: n0te_core::notebook::Notebook) -> Arc<Self> {
        Arc::new(NotebookHandle { lib, nb: Mutex::new(nb), open_sheets: Mutex::new(Vec::new()) })
    }

    /// The most recently opened live handle of `sheet_id`.
    fn open_sheet_handle(&self, sheet_id: &str) -> Option<Arc<SheetHandle>> {
        lock(&self.open_sheets).iter().rev().filter(|(id, _)| id == sheet_id).find_map(|(_, w)| w.upgrade())
    }
}

#[uniffi::export]
impl NotebookHandle {
    pub fn id(&self) -> String {
        lock(&self.nb).id()
    }
    pub fn title(&self) -> String {
        lock(&self.nb).title()
    }
    pub fn outline(&self, expand_all: bool) -> Vec<FfiOutlineRow> {
        lock(&self.nb).outline(expand_all).into_iter().map(Into::into).collect()
    }
    pub fn add_group(&self, parent: Option<String>, index: u32, title: String) -> R<String> {
        Ok(lock(&self.nb).add_group(parent.as_deref(), index as usize, &title)?)
    }
    /// Returns [node, sheet id].
    pub fn add_sheet(&self, parent: Option<String>, index: u32, spec: FfiSheetSpec) -> R<Vec<String>> {
        let (n, s) = lock(&self.nb).add_sheet(parent.as_deref(), index as usize, &spec.into())?;
        Ok(vec![n, s])
    }
    /// Returns [node, sheet id].
    pub fn add_side_sheet(&self, main: String, right: bool, index: u32, spec: FfiSheetSpec) -> R<Vec<String>> {
        let side = if right { Side::Right } else { Side::Left };
        let (n, s) = lock(&self.nb).add_side_sheet(&main, side, index as usize, &spec.into())?;
        Ok(vec![n, s])
    }
    pub fn move_node(&self, node: String, parent: Option<String>, index: u32) -> R<()> {
        Ok(lock(&self.nb).move_node(&node, parent.as_deref(), index as usize)?)
    }
    pub fn set_collapsed(&self, group: String, collapsed: bool) -> R<()> {
        Ok(lock(&self.nb).set_collapsed(&group, collapsed)?)
    }
    pub fn delete_node(&self, node: String) -> R<()> {
        Ok(lock(&self.nb).delete_undoable(&node)?)
    }
    /// Takes back the last delete_node (true if something came back).
    pub fn undo_delete(&self) -> R<bool> {
        Ok(lock(&self.nb).undo_delete()?)
    }
    pub fn save(&self) -> R<()> {
        Ok(self.lib.inner.save_notebook(&lock(&self.nb))?)
    }
    /// Where the words of `query` are in this notebook, page by page (in order), best first on
    /// each page: headings, handwriting as read, typed text, and with `pdf` the PDF pages' text.
    /// Open sheets are searched as they are now (unsaved strokes too).
    pub fn search(&self, query: String, pdf: Option<Arc<crate::pdf::PdfService>>) -> Vec<FfiSearchHit> {
        use n0te_core::search::{HitKind, normalize, score, words};
        let w = words(&query);
        if w.is_empty() {
            return Vec::new();
        }
        let id = self.id();
        let rows = lock(&self.nb).outline(true);
        let open: Vec<(String, Arc<SheetHandle>)> = lock(&self.open_sheets).iter().filter_map(|(s, h)| h.upgrade().map(|h| (s.clone(), h))).collect();
        let mut sheets: Vec<(String, String, SheetSpec)> = Vec::new();
        for row in rows {
            if let n0te_core::notebook::OutlineRow::Sheet { node, sheet_id, spec, left, right, .. } = row {
                for s in left {
                    sheets.push((s.node, s.sheet_id, s.spec));
                }
                sheets.push((node, sheet_id, spec));
                for s in right {
                    sheets.push((s.node, s.sheet_id, s.spec));
                }
            }
        }
        let mut out = Vec::new();
        for (node, sheet_id, spec) in sheets {
            let kind = |k: HitKind| match k {
                HitKind::Ink => FfiHitKind::Ink,
                HitKind::Text => FfiHitKind::Text,
                HitKind::Heading => FfiHitKind::Heading,
                HitKind::Pdf => FfiHitKind::Pdf,
                HitKind::Plot => FfiHitKind::Plot,
            };
            let mut hits: Vec<FfiSearchHit> = Vec::new();
            let width = if spec.width > 0.0 { spec.width } else { 595.0 };
            let found = match open.iter().find(|(s, _)| *s == sheet_id) {
                Some((_, h)) => lock(&h.sheet).search(&query, width),
                None => self.lib.inner.load_sheet(&id, &sheet_id).map(|s| s.search(&query, width)).unwrap_or_default(),
            };
            for h in found {
                hits.push(FfiSearchHit { node: node.clone(), sheet_id: sheet_id.clone(), kind: kind(h.kind), text: h.text, x0: h.x0, y0: h.y0, x1: h.x1, y1: h.y1, slips: h.slips as u32 });
            }
            if let (Some(svc), Some((blob, page))) = (&pdf, &spec.pdf) {
                let bands = spec.bands();
                for (text, r) in svc.page_lines(blob, *page) {
                    if let Some(slips) = score(&w, &normalize(&text)) {
                        // Where that line lies now (space inserted into the page moves the PDF below it down).
                        let shift = bands.iter().find(|b| r.y0 >= b.src && r.y0 < b.src + b.len).map(|b| b.dst - b.src).unwrap_or(0.0);
                        hits.push(FfiSearchHit { node: node.clone(), sheet_id: sheet_id.clone(), kind: FfiHitKind::Pdf, text, x0: r.x0, y0: r.y0 + shift, x1: r.x1, y1: r.y1 + shift, slips: slips as u32 });
                    }
                }
            }
            hits.sort_by(|a, b| a.slips.cmp(&b.slips).then(a.y0.total_cmp(&b.y0)));
            out.extend(hits);
        }
        out
    }

    /// Every page's headings (in page order, top to bottom on each), for the outline. Open
    /// sheets as they are now.
    pub fn headings(&self) -> Vec<FfiPageHeading> {
        let id = self.id();
        let rows = lock(&self.nb).outline(true);
        let open: Vec<(String, Arc<SheetHandle>)> = lock(&self.open_sheets).iter().filter_map(|(s, h)| h.upgrade().map(|h| (s.clone(), h))).collect();
        let mut out = Vec::new();
        for row in rows {
            if let n0te_core::notebook::OutlineRow::Sheet { node, sheet_id, .. } = row {
                let hs = match open.iter().find(|(s, _)| *s == sheet_id) {
                    Some((_, h)) => lock(&h.sheet).headings(),
                    None => self.lib.inner.load_sheet(&id, &sheet_id).map(|s| s.headings()).unwrap_or_default(),
                };
                for h in hs {
                    out.push(FfiPageHeading { node: node.clone(), sheet_id: sheet_id.clone(), id: h.id, text: h.text, x0: h.x0, y0: h.y0, x1: h.x1, y1: h.y1 });
                }
            }
        }
        out
    }

    pub fn open_sheet(self: Arc<Self>, sheet_id: String) -> R<Arc<SheetHandle>> {
        let id = self.id();
        let sheet = self.lib.inner.load_sheet(&id, &sheet_id)?;
        let handle = Arc::new(SheetHandle { notebook: self.clone(), sheet_id: sheet_id.clone(), sheet: Mutex::new(sheet) });
        let mut open = lock(&self.open_sheets);
        open.retain(|(_, w)| w.strong_count() > 0);
        open.push((sheet_id, Arc::downgrade(&handle)));
        Ok(handle)
    }
}

#[derive(uniffi::Object)]
pub struct SheetHandle {
    notebook: Arc<NotebookHandle>,
    sheet_id: String,
    pub(crate) sheet: Mutex<n0te_core::sheet::Sheet>,
}

#[uniffi::export]
impl SheetHandle {
    /// Strokes intersecting the rectangle, with outlines ready to fill.
    pub fn strokes_in(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Vec<FfiStroke> {
        lock(&self.sheet).strokes_in(Some(Rect::new(x0, y0, x1, y1))).into_iter().map(to_ffi).collect()
    }
    pub fn all_strokes(&self) -> Vec<FfiStroke> {
        lock(&self.sheet).strokes().into_iter().map(to_ffi).collect()
    }
    /// Stores a finished stroke and returns it with its outline.
    pub fn add_stroke(&self, brush: FfiBrush, samples: Vec<u8>) -> R<FfiStroke> {
        let pts = points(&samples);
        let s = lock(&self.sheet);
        let id = s.add_stroke(brush.into(), &pts)?;
        let stroke = s.stroke(&id)?.ok_or_else(|| N0teError::Failed { msg: "stroke vanished".into() })?;
        Ok(to_ffi(stroke))
    }
    pub fn remove(&self, ids: Vec<String>) -> R<()> {
        Ok(lock(&self.sheet).remove(&ids)?)
    }
    pub fn recolor(&self, ids: Vec<String>, color: u32) -> R<()> {
        Ok(lock(&self.sheet).recolor(&ids, color)?)
    }
    pub fn transform(&self, ids: Vec<String>, dx: f32, dy: f32, scale: f32, ox: f32, oy: f32) -> R<()> {
        Ok(lock(&self.sheet).transform(&ids, dx, dy, scale, ox, oy)?)
    }
    /// Turns and stretches strokes about (ax, ay) by the complex factor re + i·im (width kept).
    pub fn spin(&self, ids: Vec<String>, ax: f32, ay: f32, re: f32, im: f32) -> R<()> {
        Ok(lock(&self.sheet).spin(&ids, ax, ay, re, im)?)
    }
    pub fn save(&self) -> R<()> {
        let id = self.notebook.id();
        Ok(self.notebook.lib.inner.save_sheet(&id, &self.sheet_id, &lock(&self.sheet))?)
    }
}

#[derive(Debug, Clone, uniffi::Record)]
pub struct BenchResult {
    pub lines: Vec<String>,
}

/// The M0 speed check, run on the device: a 500-sheet notebook and a dense sheet, through the
/// same code paths the app uses. Writes into `root` (a scratch directory).
#[uniffi::export]
pub fn run_core_bench(root: String) -> R<BenchResult> {
    let mut lines = Vec::new();
    let lib = Arc::new(NoteLibrary { inner: n0te_core::store::Library::open(&root)? });
    let t = Instant::now();
    let nb = lib.clone().create_notebook("Bench".into())?;
    let mut group = None;
    let a4 = FfiSheetSpec { width: 595.0, height: 842.0, background: FfiBackground::Grid, spacing: 20.0, pdf_blob: None, pdf_page: 0, gaps: vec![] };
    let mut first_sheet = String::new();
    for i in 0..500u32 {
        if i % 25 == 0 {
            group = Some(nb.add_group(None, u32::MAX, format!("Woche {:02}", i / 25 + 1))?);
        }
        let ids = nb.add_sheet(group.clone(), u32::MAX, a4.clone())?;
        if i == 0 {
            first_sheet = ids[1].clone();
        }
    }
    nb.save()?;
    lines.push(format!("build+save 500 sheets: {:?}", t.elapsed()));
    let t = Instant::now();
    let nb2 = lib.clone().load_notebook(nb.id())?;
    let rows = nb2.outline(true);
    lines.push(format!("open notebook + outline ({} rows): {:?}", rows.len(), t.elapsed()));

    let sheet = nb2.clone().open_sheet(first_sheet.clone())?;
    let brush = FfiBrush { width: 2.0, thinning: 0.6, color: 0xFF111111, kind: 0 };
    let t = Instant::now();
    let mut seed = 7u64;
    for s in 0..400u32 {
        let mut samples = Vec::with_capacity(480);
        for i in 0..120u32 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            samples.extend_from_slice(&[40.0 + (s % 20) as f32 * 25.0 + i as f32 * 0.4, 40.0 + (s / 20) as f32 * 38.0 + ((seed >> 40) % 10) as f32 * 0.3, 0.3 + ((seed >> 33) % 60) as f32 / 100.0, (i * 4) as f32]);
        }
        sheet.add_stroke(brush, pack(&samples))?;
    }
    lines.push(format!("add 400 strokes x 120 samples: {:?} ({:.2} ms each)", t.elapsed(), t.elapsed().as_secs_f64() * 1000.0 / 400.0));
    let t = Instant::now();
    sheet.save()?;
    lines.push(format!("save dense sheet: {:?}", t.elapsed()));
    let t = Instant::now();
    let reopened = nb2.open_sheet(first_sheet)?;
    let all = reopened.all_strokes();
    let verts: usize = all.iter().map(|s| s.outline.len() / 8).sum();
    lines.push(format!("load dense sheet + {} outlines ({} vertices): {:?}", all.len(), verts, t.elapsed()));
    let t = Instant::now();
    let part = reopened.strokes_in(0.0, 0.0, 300.0, 200.0);
    lines.push(format!("cull to viewport ({} strokes): {:?}", part.len(), t.elapsed()));
    let live: Vec<f32> = (0..300).flat_map(|i| [i as f32, (i as f32 * 0.1).sin() * 20.0, 0.5, i as f32 * 4.0]).collect();
    let live = pack(&live);
    let t = Instant::now();
    for _ in 0..100 {
        std::hint::black_box(stroke_outline(brush, live.clone()));
    }
    lines.push(format!("live outline, 300 samples: {:.3} ms per call", t.elapsed().as_secs_f64() * 1000.0 / 100.0));
    Ok(BenchResult { lines })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bench_runs_on_host() {
        let dir = std::env::temp_dir().join(format!("n0te-ffi-bench-{}", n0te_core::ids::new_id(3)));
        let r = run_core_bench(dir.to_string_lossy().into_owned()).unwrap();
        assert_eq!(r.lines.len(), 7, "{:?}", r.lines);
        for l in &r.lines {
            println!("{l}");
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}
