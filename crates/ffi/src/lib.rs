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

mod edit;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum FfiBackground {
    Blank,
    Lines,
    Grid,
    Dots,
    Cornell,
}

impl From<FfiBackground> for Background {
    fn from(b: FfiBackground) -> Self {
        match b {
            FfiBackground::Blank => Background::Blank,
            FfiBackground::Lines => Background::Lines,
            FfiBackground::Grid => Background::Grid,
            FfiBackground::Dots => Background::Dots,
            FfiBackground::Cornell => Background::Cornell,
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
}

impl From<FfiSheetSpec> for SheetSpec {
    fn from(s: FfiSheetSpec) -> Self {
        SheetSpec { width: s.width, height: s.height, background: s.background.into(), spacing: s.spacing, pdf: s.pdf_blob.map(|b| (b, s.pdf_page)) }
    }
}
impl From<SheetSpec> for FfiSheetSpec {
    fn from(s: SheetSpec) -> Self {
        let (pdf_blob, pdf_page) = match s.pdf {
            Some((b, p)) => (Some(b), p),
            None => (None, 0),
        };
        FfiSheetSpec { width: s.width, height: s.height, background: s.background.into(), spacing: s.spacing, pdf_blob, pdf_page }
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
        brush: s.brush.into(),
        id: s.id,
    }
}

/// Outline of a stroke being drawn (no storage), for the live pen layer. Packed LE f32 in/out.
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
        Ok(lock(&self.nb).delete(&node)?)
    }
    pub fn save(&self) -> R<()> {
        Ok(self.lib.inner.save_notebook(&lock(&self.nb))?)
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
    let a4 = FfiSheetSpec { width: 595.0, height: 842.0, background: FfiBackground::Grid, spacing: 20.0, pdf_blob: None, pdf_page: 0 };
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
