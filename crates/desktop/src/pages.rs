//! The page canvas: a notebook laid out exactly like on the tablet ([`n0te_core::layout`]):
//! one column of pages with group titles between them, side pages left and right, boards as
//! landscape frames. Pan with the wheel / trackpad / left or middle drag, zoom with
//! ctrl+wheel around the cursor; a click on a group title folds or unfolds it.
//!
//! Drawing: each page's paper is one quad; its pattern goes into `canvas::Cache` tiles of
//! [`TILE_PX`] pixels at a power-of-two level of detail, built for the part in view only (a
//! huge page at a high zoom never builds a huge picture). Its strokes are sorted into square
//! cells ([`CELL`]), each cell one triangle mesh in page units (`ink::outline_mesh`, no
//! tessellation) drawn under a GPU transformation: only cells in view are drawn (the GPU uploads
//! a cached mesh again whenever the view moves, so what is out of view costs nothing), and after
//! an edit only the cells whose strokes changed are built again. Zoomed out, the meshes keep
//! fewer samples (never more than half a pixel off). Coordinate systems are ink too (their look
//! from the core, [`crate::plots`]): one more mesh per page under the strokes, rebuilt only when
//! a plot changes. A sheet's strokes are read from disk the first time it becomes visible;
//! caches of pages not drawn for a while are dropped (LRU).
//!
//! Every view of the same pages (the page view, the writing magnifier, the outline's small
//! pictures) has GPU caches of its own ([`Slot`]): a cache drawn twice in one frame would show
//! only where it was drawn last.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::sync::Arc;

use iced::advanced::graphics::color;
use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::graphics::mesh::{self, Indexed, Mesh, Renderer as _, SolidVertex2D};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::text::{self, Renderer as _};
use iced::advanced::widget::{self, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::keyboard;
use iced::widget::canvas::{self, Path, Stroke as LineStroke};
use iced::{Color, Element, Event, Length, Point, Rectangle, Size, Theme, Transformation, Vector};

use n0te_core::ink::{self, KIND_PENCIL, Rect};
use n0te_core::layout::{self as nl, Header, Placed};
use n0te_core::notebook::{Background, SheetSpec};
use n0te_core::plot::Plot;
use n0te_core::sheet::{Stroke, TextBox};

use crate::pdf::{self, Pdf};
use crate::i18n::tr;
use crate::theme;

pub const MIN_ZOOM: f32 = 0.08;
pub const MAX_ZOOM: f32 = 8.0;
/// Keep at most this many page caches alive (LRU by last frame drawn).
pub const MAX_CACHED: usize = 24;
/// Pencil passes (outer band, middle band, core) at the tablet's default softness.
const PENCIL_ALPHA: [f32; 3] = [0.84, 0.14, 0.24];
/// Strokes go into square cells this big (page points) by the middle of their box; a cell is
/// one mesh (an A4 page has four).
pub const CELL: f32 = 512.0;
/// A background tile is this many pixels of its level of detail on a side.
pub const TILE_PX: f32 = 1024.0;
/// Background tiles kept per page (the least recently drawn go first).
const MAX_TILES: usize = 64;

/// Which view draws (each has GPU caches of its own, see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Slot {
    #[default]
    Main,
    /// The writing magnifier's strip (n0te Tablet).
    Loupe,
    /// The outline's small pictures.
    Thumb,
}

/// What a sheet holds.
#[derive(Default)]
pub struct SheetContent {
    pub strokes: Vec<Rc<Stroke>>,
    /// Where the sheet's history stood when this was read: read again after an edit, only
    /// what changed since is decoded ([`SheetContent::read`]).
    pub version: Option<n0te_core::sheet::VersionMark>,
    pub texts: Vec<TextBox>,
    /// A text page's Markdown body.
    pub body: String,
    /// Images with their file (a blob path; drawn once the file is there).
    pub images: Vec<(n0te_core::sheet::ImageObj, std::path::PathBuf)>,
    /// Coordinate systems (drawn as ink under the strokes).
    pub plots: Vec<Plot>,
}

impl SheetContent {
    /// What `sheet` holds. With `old` (the same sheet read before), only what changed since is
    /// read: a page with a million samples gets one more stroke in a moment. `image`: an
    /// image's file.
    pub fn read(sheet: &n0te_core::sheet::Sheet, old: Option<&SheetContent>, image: impl Fn(&n0te_core::sheet::ImageObj) -> Option<std::path::PathBuf>) -> SheetContent {
        let version = Some(sheet.version_mark());
        if let Some(old) = old
            && let Some(v) = &old.version
            && let Some(ch) = sheet.changes_since(v)
        {
            let mut out: HashSet<&str> = ch.gone.iter().map(String::as_str).collect();
            out.extend(ch.strokes.iter().map(|s| s.id.as_str()));
            let mut strokes: Vec<Rc<Stroke>> = old.strokes.iter().filter(|s| !out.contains(s.id.as_str())).cloned().collect();
            strokes.extend(ch.strokes.into_iter().map(Rc::new));
            let (texts, body, images, plots) = if ch.other {
                (sheet.text_boxes(), sheet.page_text(), sheet.images().into_iter().filter_map(|i| image(&i).map(|p| (i, p))).collect(), sheet.plots())
            } else {
                (old.texts.clone(), old.body.clone(), old.images.clone(), old.plots.clone())
            };
            return SheetContent { strokes, version, texts, body, images, plots };
        }
        SheetContent {
            strokes: sheet.strokes().into_iter().map(Rc::new).collect(),
            version,
            texts: sheet.text_boxes(),
            body: sheet.page_text(),
            images: sheet.images().into_iter().filter_map(|i| image(&i).map(|p| (i, p))).collect(),
            plots: sheet.plots(),
        }
    }
}

/// Reads a sheet's content: from the sheet given (open here already) or else from the library;
/// strokes unchanged since the content given are taken from there ([`SheetContent::read`]).
pub type Loader = Box<dyn Fn(&str, Option<&n0te_core::sheet::Sheet>, Option<&SheetContent>) -> SheetContent>;

thread_local! {
    /// Image handles per file (iced decodes and caches by handle).
    static IMAGES: RefCell<HashMap<std::path::PathBuf, iced::advanced::image::Handle>> = RefCell::new(HashMap::new());
}

/// A background tile: (view, level of detail, column, row, the paper's extent it was cut to).
type TileKey = (Slot, u32, i32, i32, u64);

/// One cell of a page's ink: which strokes, their box, and its meshes as built so far.
struct Chunk {
    /// Of its strokes as drawn (ids, boxes, brushes, ends; erased or cut ones counted).
    fp: u64,
    /// Around its strokes (page-local).
    bounds: Rect,
    /// Indices into the content's strokes.
    strokes: Vec<usize>,
    /// (view, sample step, clip, mesh).
    meshes: Vec<(Slot, u8, Rectangle, mesh::Cache)>,
    last_frame: u64,
}

impl Chunk {
    /// Builds its mesh for `slot` at sample step `level` (the one it had for `slot` goes).
    fn build(&mut self, strokes: &[Rc<Stroke>], erasing: Option<&Erasing>, slot: Slot, level: u8, clip: Rectangle) -> mesh::Cache {
        let cache = mesh::Cache::new(Arc::from(chunk_meshes(strokes, &self.strokes, erasing, level as usize, clip)));
        self.meshes.retain(|m| m.0 != slot);
        self.meshes.push((slot, level, clip, cache.clone()));
        cache
    }
}

/// What a cell's meshes are clipped to: the page, or (a board on its own) the cell's ink.
fn chunk_clip(page: Option<Rectangle>, chunk: &Chunk) -> Rectangle {
    page.unwrap_or_else(|| {
        let b = chunk.bounds.inflate(4.0);
        Rectangle::new(Point::new(b.x0, b.y0), Size::new(b.width(), b.height()))
    })
}

/// A page's ink sorted into cells.
#[derive(Default)]
struct Cells {
    /// What they were sorted from: the content's generation and how far an erasing on it got.
    sig: Option<(u64, Option<(usize, usize, usize)>)>,
    map: HashMap<(i32, i32), Chunk>,
}

static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn next_generation() -> u64 {
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

pub struct Page {
    pub placed: Placed,
    content: RefCell<Option<Rc<SheetContent>>>,
    /// Changes whenever the content is dropped or replaced (the cells sort it again).
    generation: Cell<u64>,
    /// Pattern tiles with the frame they were last drawn in.
    tiles: RefCell<HashMap<TileKey, (canvas::Cache, u64)>>,
    cells: RefCell<Cells>,
    /// Per view: the plots the mesh shows, and the mesh: kept while the sheet's plots stay the
    /// same (a reload for a new stroke doesn't rebuild it).
    plots: RefCell<Vec<(Slot, Vec<Plot>, Option<mesh::Cache>)>>,
    /// The coarse ink mesh of the page's small picture (outline): built when first shown,
    /// dropped whenever the sheet changes (with its content).
    thumb: RefCell<Option<(f32, mesh::Cache)>>,
    last_frame: Cell<u64>,
}

impl Page {
    fn new(placed: Placed) -> Self {
        Page {
            placed,
            content: RefCell::new(None),
            generation: Cell::new(next_generation()),
            tiles: RefCell::new(HashMap::new()),
            cells: RefCell::new(Cells::default()),
            plots: RefCell::new(Vec::new()),
            thumb: RefCell::new(None),
            last_frame: Cell::new(0),
        }
    }

    /// The strokes for the small picture `zoom` pixels per point: no two samples closer than
    /// half a pixel, plenty that small (a huge page's picture stays small too).
    fn thumb_mesh(&self, load: &Loader, zoom: f32) -> mesh::Cache {
        if let Some((z, cache)) = self.thumb.borrow().as_ref()
            && (z / zoom - 1.0).abs() < 0.25
        {
            return cache.clone();
        }
        let content = self.content(load);
        let (w, h) = self.size();
        let tol = 0.5 / zoom.max(1e-3);
        let cache = mesh::Cache::new(Arc::from(ink_meshes_tol(content.strokes.iter().map(|s| (s.points.as_slice(), &s.brush)), tol, Rectangle::new(Point::ORIGIN, Size::new(w, h)))));
        *self.thumb.borrow_mut() = Some((zoom, cache.clone()));
        cache
    }

    pub fn rect(&self) -> Rect {
        self.placed.rect
    }

    fn size(&self) -> (f32, f32) {
        (self.placed.rect.width(), self.placed.rect.height())
    }

    fn is_cached(&self) -> bool {
        !self.tiles.borrow().is_empty() || self.cells.borrow().map.values().any(|c| !c.meshes.is_empty())
    }

    fn evict(&self) {
        self.tiles.borrow_mut().clear();
        for c in self.cells.borrow_mut().map.values_mut() {
            c.meshes.clear();
        }
        self.plots.borrow_mut().clear();
    }

    fn content(&self, load: &Loader) -> Rc<SheetContent> {
        self.content.borrow_mut().get_or_insert_with(|| Rc::new(load(&self.placed.sheet_id, None, None))).clone()
    }

    /// Drops the content (read again when next needed).
    fn forget(&self) {
        *self.content.borrow_mut() = None;
        *self.thumb.borrow_mut() = None;
        self.generation.set(next_generation());
    }

    /// The paper's pattern over the part `view` (page-local) of a page `extent` big (None: a
    /// board shown on its own, paper everywhere), drawn at `at` (the page's top-left on screen):
    /// the tiles in view at the zoom's level of detail, each built once.
    #[allow(clippy::too_many_arguments)]
    fn draw_pattern(&self, r: &mut iced::Renderer, at: Point, cam: &Camera, slot: Slot, view: Rect, extent: Option<(f32, f32)>, frame_no: u64) {
        let spec = &self.placed.spec;
        let board = self.placed.is_board();
        let bg = if board { Background::Dots } else { spec.background };
        if !board && (spec.pdf.is_some() || matches!(bg, Background::Blank | Background::Text)) {
            return;
        }
        let step = if board && spec.spacing < 4.0 { 24.0 } else { spec.spacing.max(4.0) };
        let lod = lod_for(cam.zoom);
        let span = TILE_PX / lod;
        let ext_key = extent.map_or(0, |(w, h)| (w.to_bits() as u64) << 32 | h.to_bits() as u64);
        let mut tiles = self.tiles.borrow_mut();
        for (ix, iy) in tiles_in(view, extent, span) {
            let key = (slot, lod.to_bits(), ix, iy, ext_key);
            let tile = tile_rect(ix, iy, span, extent);
            let entry = tiles.entry(key).or_insert_with(|| (canvas::Cache::new(), frame_no));
            entry.1 = frame_no;
            let geometry = entry.0.draw(r, Size::new(tile.width() * lod, tile.height() * lod), |frame| {
                frame.scale(lod);
                frame.translate(Vector::new(-tile.x0, -tile.y0));
                draw_pattern(frame, bg, extent, tile, step, lod);
            });
            let pos = Point::new(at.x + tile.x0 * cam.zoom, at.y + tile.y0 * cam.zoom);
            r.with_translation(Vector::new(pos.x, pos.y), |r| {
                r.with_transformation(Transformation::scale(cam.zoom / lod), |r| r.draw_geometry(geometry));
            });
        }
        if tiles.len() > MAX_TILES {
            let mut ages: Vec<(u64, TileKey)> = tiles.iter().map(|(k, v)| (v.1, *k)).collect();
            ages.sort_by_key(|a| a.0);
            for (_, k) in ages.into_iter().take(tiles.len() - MAX_TILES) {
                tiles.remove(&k);
            }
        }
    }

    /// Sorts the strokes into cells (again, once they or the erasing on them changed); a cell
    /// whose strokes look the same keeps its meshes.
    fn sort_cells(&self, content: &SheetContent, erasing: Option<&Erasing>) {
        let sig = (self.generation.get(), erasing.map(|e| (e.hidden.len(), e.cut.len(), e.dabs.len())));
        let mut cells = self.cells.borrow_mut();
        if cells.sig == Some(sig) {
            return;
        }
        cells.sig = Some(sig);
        let mut fresh: HashMap<(i32, i32), (Vec<usize>, std::collections::hash_map::DefaultHasher, Rect)> = HashMap::new();
        for (i, s) in content.strokes.iter().enumerate() {
            let cut = match erasing {
                Some(e) if e.hidden.contains(&s.id) => continue,
                Some(e) if e.cut.contains(&s.id) => Some(e.dabs.iter().filter(|d| touches(&s.bounds, &[d.0, d.1, d.2], s.brush.width)).count()),
                _ => None,
            };
            let e = fresh.entry(cell_of(&s.bounds)).or_insert_with(|| (Vec::new(), Default::default(), Rect::EMPTY));
            e.0.push(i);
            stroke_fp(s, cut, &mut e.1);
            e.2 = e.2.union(&s.bounds);
        }
        let old = std::mem::take(&mut cells.map);
        let mut old = old;
        for (cell, (strokes, h, bounds)) in fresh {
            use std::hash::Hasher;
            let fp = h.finish();
            let chunk = match old.remove(&cell) {
                Some(c) if c.fp == fp => Chunk { strokes, bounds, ..c },
                _ => Chunk { fp, bounds, strokes, meshes: Vec::new(), last_frame: 0 },
            };
            cells.map.insert(cell, chunk);
        }
    }

    /// Builds the meshes of the cells in `view` (page-local) that the zoom's level lacks, until
    /// `deadline`; whether all are there (the page view does this before it draws a frame, so a
    /// zoom past a level builds a little per frame and draws the cells it hasn't got to yet as
    /// they were).
    #[allow(clippy::too_many_arguments)]
    fn prepare_ink(&self, cam: &Camera, slot: Slot, view: Rect, load: &Loader, erasing: Option<&Erasing>, clip: Option<Rectangle>, deadline: std::time::Instant) -> bool {
        let content = self.content(load);
        self.sort_cells(&content, erasing);
        let level = mesh_step(cam.zoom) as u8;
        let mut cells = self.cells.borrow_mut();
        for chunk in cells.map.values_mut().filter(|c| c.bounds.intersects(&view)) {
            let clip = chunk_clip(clip, chunk);
            if chunk.meshes.iter().any(|m| m.0 == slot && m.1 == level && m.2 == clip) {
                continue;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            chunk.build(&content.strokes, erasing, slot, level, clip);
        }
        true
    }

    /// Draws the ink in `view` (page-local) at `at`: the cells in view, each mesh built when
    /// first needed (one at another level stands in while [`Page::prepare_ink`] gets to it).
    /// `clip`: the page (None: a board on its own, unclipped).
    #[allow(clippy::too_many_arguments)]
    fn draw_ink(&self, r: &mut iced::Renderer, at: Point, cam: &Camera, slot: Slot, view: Rect, load: &Loader, erasing: Option<&Erasing>, clip: Option<Rectangle>, frame_no: u64) {
        let content = self.content(load);
        self.sort_cells(&content, erasing);
        let level = mesh_step(cam.zoom) as u8;
        let mut cells = self.cells.borrow_mut();
        let mut seen: Vec<(&(i32, i32), &mut Chunk)> = cells.map.iter_mut().filter(|(_, c)| c.bounds.intersects(&view)).collect();
        seen.sort_by_key(|(k, _)| **k);
        for (_, chunk) in seen {
            chunk.last_frame = frame_no;
            let clip = chunk_clip(clip, chunk);
            let cache = match chunk.meshes.iter().find(|m| m.0 == slot && m.2 == clip) {
                Some(m) => m.3.clone(),
                None => chunk.build(&content.strokes, erasing, slot, level, clip),
            };
            r.with_translation(Vector::new(at.x, at.y), |r| {
                r.with_transformation(Transformation::scale(cam.zoom), |r| r.draw_mesh_cache(cache));
            });
        }
    }

    /// The page's coordinate systems as one mesh in page units (None: it has none), built when
    /// they change.
    fn plot_mesh(&self, load: &Loader, slot: Slot) -> Option<mesh::Cache> {
        let content = self.content(load);
        let mut all = self.plots.borrow_mut();
        if let Some((_, plots, cache)) = all.iter().find(|p| p.0 == slot)
            && *plots == content.plots
        {
            return cache.clone();
        }
        let (w, h) = self.size();
        let cache = crate::plots::meshes(&content.plots, Rectangle::new(Point::ORIGIN, Size::new(w, h))).map(|m| mesh::Cache::new(Arc::from(m)));
        all.retain(|p| p.0 != slot);
        all.push((slot, content.plots.clone(), cache.clone()));
        cache
    }

    /// The topmost plot at a sheet point (a few points around its box count).
    fn plot_at(&self, load: &Loader, x: f32, y: f32) -> Option<String> {
        let m = 4.0;
        self.content(load).plots.iter().rev().find(|p| x >= p.x - m && x <= p.x + p.w + m && y >= p.y - m && y <= p.y + p.h + m).map(|p| p.id.clone())
    }
}

/// One stretch of a PDF page's background to draw (a band of [`SheetSpec::bands`]): the tiles of
/// the PDF page at its own size, `width` × [`SheetSpec::base_height`], drawn `shift` points
/// further down the page and clipped to `dst`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PdfPart {
    /// The whole stretch, in points on the PDF page (requested at thumbnail scale).
    pub src: Rect,
    /// Its part in view, in points on the PDF page (requested sharp); `None` if out of view.
    pub visible: Option<Rect>,
    /// Where it lies on the page (page points).
    pub dst: Rect,
    /// Page y minus PDF y.
    pub shift: f32,
}

/// The stretches of a PDF page's background to draw while `visible` (page points) is in view:
/// a page without gaps is one stretch, the whole page, as always; a page with space inserted
/// one per band that lies in view.
pub fn pdf_parts(spec: &SheetSpec, visible: Rect) -> Vec<PdfPart> {
    let w = spec.width;
    if spec.gaps.is_empty() {
        let whole = Rect::new(0.0, 0.0, w, spec.height);
        return vec![PdfPart { src: whole, visible: (!visible.is_empty()).then_some(visible), dst: whole, shift: 0.0 }];
    }
    spec.bands()
        .into_iter()
        .filter(|b| b.len > 0.0)
        .filter_map(|b| {
            let dst = Rect { x0: 0.0, y0: b.dst, x1: w, y1: b.dst + b.len };
            let seen = Rect { x0: visible.x0.max(0.0), y0: visible.y0.max(dst.y0), x1: visible.x1.min(w), y1: visible.y1.min(dst.y1) };
            if visible.is_empty() || seen.x0 >= seen.x1 || seen.y0 >= seen.y1 {
                return None;
            }
            let shift = b.dst - b.src;
            let src = Rect { x0: 0.0, y0: b.src, x1: w, y1: b.src + b.len };
            Some(PdfPart { src, visible: Some(Rect { y0: seen.y0 - shift, y1: seen.y1 - shift, ..seen }), dst, shift })
        })
        .collect()
}

/// Whether `strokes` holds the finished live stroke `l` (same colour, about the same place and
/// size; a recognised shape keeps its extent, so it matches too).
pub fn stored_has<'a>(strokes: impl IntoIterator<Item = &'a Stroke>, l: &LiveInk) -> bool {
    let Some(first) = l.points.first() else { return true };
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x, first.y);
    for q in &l.points {
        x0 = x0.min(q.x);
        y0 = y0.min(q.y);
        x1 = x1.max(q.x);
        y1 = y1.max(q.y);
    }
    let tol = 2.0 + l.brush.width * 2.0 + 0.2 * (x1 - x0).max(y1 - y0);
    strokes.into_iter().any(|s| {
        let b = s.bounds;
        s.brush.color == l.brush.color && (b.x0 - x0).abs() <= tol && (b.y0 - y0).abs() <= tol && (b.x1 - x1).abs() <= tol && (b.y1 - y1).abs() <= tol
    })
}

/// What someone erasing elsewhere has done to a sheet so far (until the stored result is in).
#[derive(Debug)]
pub struct Erasing {
    /// Whole strokes hidden by the stroke eraser.
    pub hidden: HashSet<String>,
    /// Strokes under the area eraser (cut by `dabs`).
    pub cut: HashSet<String>,
    pub dabs: Vec<(f32, f32, f32)>,
    pub at: std::time::Instant,
}

impl Default for Erasing {
    fn default() -> Self {
        Erasing { hidden: HashSet::new(), cut: HashSet::new(), dabs: Vec::new(), at: std::time::Instant::now() }
    }
}

impl Erasing {
    /// The strokes as they look with the erasing applied.
    pub fn apply(&self, strokes: &[Stroke]) -> Vec<Stroke> {
        let mut out = Vec::with_capacity(strokes.len());
        for s in strokes {
            if self.hidden.contains(&s.id) {
                continue;
            }
            if self.cut.contains(&s.id)
                && let Some(runs) = n0te_core::sheet::cut_stroke(&s.points, s.brush.width, &self.dabs)
            {
                for (i, run) in runs.into_iter().enumerate() {
                    out.push(Stroke { id: format!("{}#{i}", s.id), brush: s.brush, bounds: n0te_core::sheet::bounds_of(&run, s.brush.width), points: run });
                }
                continue;
            }
            out.push(s.clone());
        }
        out
    }

    /// The stored sheet has none of the strokes this hid or cut any more: it's done.
    fn settled(&self, strokes: &[Rc<Stroke>]) -> bool {
        !strokes.iter().any(|s| self.hidden.contains(&s.id) || self.cut.contains(&s.id))
    }
}

/// Whether an eraser dab (x, y, r) can reach into a stroke's bounds.
fn touches(b: &Rect, d: &[f32; 3], width: f32) -> bool {
    let m = d[2] + width;
    d[0] >= b.x0 - m && d[0] <= b.x1 + m && d[1] >= b.y0 - m && d[1] <= b.y1 + m
}

/// Every how many samples the ink meshes keep at a zoom (zoomed out, full resolution only
/// costs); also the level a cell's mesh is built for.
pub fn mesh_step(zoom: f32) -> usize {
    match zoom {
        z if z >= 0.6 => 1,
        z if z >= 0.3 => 2,
        _ => 4,
    }
}

/// Every `step`-th sample (and the last); all of them for `step` 1.
fn thin<'a>(points: &'a [ink::InkPoint], step: usize, buf: &'a mut Vec<ink::InkPoint>) -> &'a [ink::InkPoint] {
    if step > 1 && points.len() > 2 * step {
        buf.clear();
        buf.extend(points.iter().step_by(step).copied());
        if (points.len() - 1) % step != 0 {
            buf.push(*points.last().expect("non-empty"));
        }
        buf
    } else {
        points
    }
}

/// The samples at least `tol` apart (the first and the last always stay); all of them for
/// `tol` 0.
pub fn decimate<'a>(points: &'a [ink::InkPoint], tol: f32, buf: &'a mut Vec<ink::InkPoint>) -> &'a [ink::InkPoint] {
    if tol <= 0.0 || points.len() <= 2 {
        return points;
    }
    buf.clear();
    let mut last = points[0];
    buf.push(last);
    for p in &points[1..points.len() - 1] {
        if (p.x - last.x).hypot(p.y - last.y) >= tol {
            buf.push(*p);
            last = *p;
        }
    }
    buf.push(points[points.len() - 1]);
    buf
}

/// The cell a stroke goes into: where the middle of its box lies.
pub fn cell_of(b: &Rect) -> (i32, i32) {
    (((b.x0 + b.x1) * 0.5 / CELL).floor() as i32, ((b.y0 + b.y1) * 0.5 / CELL).floor() as i32)
}

/// Adds what a stroke looks like to a cell's fingerprint (`cut`: how many eraser dabs cut it).
fn stroke_fp(s: &Stroke, cut: Option<usize>, h: &mut impl std::hash::Hasher) {
    use std::hash::Hash;
    s.id.hash(h);
    s.points.len().hash(h);
    for v in [s.brush.width, s.brush.thinning, s.bounds.x0, s.bounds.y0, s.bounds.x1, s.bounds.y1] {
        v.to_bits().hash(h);
    }
    (s.brush.color, s.brush.kind, cut).hash(h);
    for p in [s.points.first(), s.points.last()].into_iter().flatten() {
        (p.x.to_bits(), p.y.to_bits(), p.pressure.to_bits()).hash(h);
    }
}

/// The meshes of the strokes `which` (indices into `strokes`), with what an erasing hides or
/// cuts left out.
fn chunk_meshes(strokes: &[Rc<Stroke>], which: &[usize], erasing: Option<&Erasing>, step: usize, clip: Rectangle) -> Vec<Mesh> {
    let mut runs: Vec<(Vec<ink::InkPoint>, ink::Brush)> = Vec::new();
    let mut whole: Vec<&Stroke> = Vec::with_capacity(which.len());
    for &i in which {
        let s = &*strokes[i];
        match erasing {
            Some(e) if e.hidden.contains(&s.id) => {}
            Some(e) if e.cut.contains(&s.id) => match n0te_core::sheet::cut_stroke(&s.points, s.brush.width, &e.dabs) {
                Some(cut) => runs.extend(cut.into_iter().map(|r| (r, s.brush))),
                None => whole.push(s),
            },
            _ => whole.push(s),
        }
    }
    ink_meshes_of(whole.iter().map(|s| (s.points.as_slice(), &s.brush)).chain(runs.iter().map(|(p, b)| (p.as_slice(), b))), step, clip)
}

/// The background tiles (column, row) `span` page points big that `view` (page-local) touches
/// on paper `extent` big (None: endless, a board on its own).
pub fn tiles_in(view: Rect, extent: Option<(f32, f32)>, span: f32) -> Vec<(i32, i32)> {
    let (mut v, mut out) = (view, Vec::new());
    if let Some((w, h)) = extent {
        v = Rect { x0: v.x0.max(0.0), y0: v.y0.max(0.0), x1: v.x1.min(w), y1: v.y1.min(h) };
    }
    if !(v.x0 < v.x1 && v.y0 < v.y1) || span <= 0.0 {
        return out;
    }
    let (c0, c1) = ((v.x0 / span).floor() as i32, ((v.x1 / span).ceil() as i32 - 1).max((v.x0 / span).floor() as i32));
    let (r0, r1) = ((v.y0 / span).floor() as i32, ((v.y1 / span).ceil() as i32 - 1).max((v.y0 / span).floor() as i32));
    // Far too many (a broken camera): draw none rather than hang.
    if (c1 - c0 + 1) as i64 * (r1 - r0 + 1) as i64 > 4096 {
        return out;
    }
    for r in r0..=r1 {
        for c in c0..=c1 {
            out.push((c, r));
        }
    }
    out
}

/// A tile's rectangle (page-local), cut to the paper's extent.
pub fn tile_rect(ix: i32, iy: i32, span: f32, extent: Option<(f32, f32)>) -> Rect {
    let r = Rect { x0: ix as f32 * span, y0: iy as f32 * span, x1: (ix + 1) as f32 * span, y1: (iy + 1) as f32 * span };
    match extent {
        Some((w, h)) => Rect { x1: r.x1.min(w), y1: r.y1.min(h), ..r },
        None => r,
    }
}

/// All strokes of a page as solid meshes (page units, colour per vertex). Pencils are three
/// translucent bands like on the tablet (without the grain). `step`: every how many samples.
pub fn ink_meshes(strokes: &[Stroke], step: usize, clip: Rectangle) -> Vec<Mesh> {
    ink_meshes_of(strokes.iter().map(|s| (s.points.as_slice(), &s.brush)), step, clip)
}

/// Like [`ink_meshes`], for any ink: (points, brush) per stroke.
pub fn ink_meshes_of<'a>(strokes: impl IntoIterator<Item = (&'a [ink::InkPoint], &'a ink::Brush)>, step: usize, clip: Rectangle) -> Vec<Mesh> {
    let mut buf = Vec::new();
    let mut out = MeshBuilder::default();
    for (points, brush) in strokes {
        out.push(thin(points, step, &mut buf), brush);
    }
    out.finish(clip)
}

/// Like [`ink_meshes_of`], keeping samples at least `tol` page points apart ([`decimate`]).
pub fn ink_meshes_tol<'a>(strokes: impl IntoIterator<Item = (&'a [ink::InkPoint], &'a ink::Brush)>, tol: f32, clip: Rectangle) -> Vec<Mesh> {
    let mut buf = Vec::new();
    let mut out = MeshBuilder::default();
    for (points, brush) in strokes {
        out.push(decimate(points, tol, &mut buf), brush);
    }
    out.finish(clip)
}

/// Strokes going into one solid mesh (page units, colour per vertex).
#[derive(Default)]
struct MeshBuilder {
    xy: Vec<f32>,
    indices: Vec<u32>,
    vertices: Vec<SolidVertex2D>,
}

impl MeshBuilder {
    fn band(&mut self, points: &[ink::InkPoint], brush: &ink::Brush, color: Color) {
        let start = self.xy.len();
        ink::outline_mesh(points, brush, &mut self.xy, &mut self.indices);
        let packed = color::pack(color);
        self.vertices.extend(self.xy[start..].chunks_exact(2).map(|v| SolidVertex2D { position: [v[0], v[1]], color: packed }));
    }

    /// A stroke (pencils are three translucent bands like on the tablet, without the grain).
    fn push(&mut self, points: &[ink::InkPoint], brush: &ink::Brush) {
        let base = argb(brush.color);
        if brush.kind == KIND_PENCIL {
            self.band(points, brush, Color { a: base.a * PENCIL_ALPHA[0], ..base });
            for (i, b) in ink::pencil_layer_brushes(brush).iter().enumerate() {
                self.band(points, b, Color { a: base.a * PENCIL_ALPHA[i + 1], ..base });
            }
        } else {
            self.band(points, brush, base);
        }
    }

    fn finish(self, clip: Rectangle) -> Vec<Mesh> {
        if self.vertices.is_empty() {
            return Vec::new();
        }
        vec![Mesh::Solid { buffers: Indexed { vertices: self.vertices, indices: self.indices }, transformation: Transformation::IDENTITY, clip_bounds: clip }]
    }
}

/// The multiples of `step` from `first` (a multiple) on that lie within `lo..=hi` and at most
/// `last` (lines of a ruling inside a tile).
fn ticks(step: f32, first: f32, last: f32, lo: f32, hi: f32) -> impl Iterator<Item = f32> {
    let k0 = ((lo.max(first)) / step).ceil() as i64;
    let k1 = ((hi.min(last)) / step).floor() as i64;
    (k0..=k1).map(move |k| k as f32 * step)
}

/// A paper ruling in page units (the same spacing and margins as the tablet), the part of it
/// in `tile` (page-local) on paper `extent` big (None: endless dots, a board on its own).
fn draw_pattern(frame: &mut canvas::Frame, bg: Background, extent: Option<(f32, f32)>, tile: Rect, step: f32, lod: f32) {
    let (w, h) = extent.unwrap_or((f32::INFINITY, f32::INFINITY));
    let lw = (1.0 / lod).max(0.5);
    let line = LineStroke::default().with_color(theme::PATTERN).with_width(lw);
    // Lines across the tile (the frame cuts them at its edge): from one side to the other.
    let (x0, x1) = (tile.x0.max(0.0), tile.x1.min(w));
    let (y0, y1) = (tile.y0.max(0.0), tile.y1.min(h));
    let hline = |frame: &mut canvas::Frame, y: f32, s: &LineStroke| frame.stroke(&Path::line(Point::new(x0, y), Point::new(x1, y)), s.clone());
    match bg {
        Background::Blank | Background::Text => {}
        Background::Lines => {
            for y in ticks(step, step * 2.0, h, tile.y0 - lw, tile.y1 + lw) {
                hline(frame, y, &line);
            }
        }
        Background::Cornell => {
            let mut y = 90.0;
            while y < h - 150.0 {
                if y >= tile.y0 - lw && y <= tile.y1 + lw {
                    hline(frame, y, &line);
                }
                y += step;
            }
            let mw = (1.5 / lod).max(0.75);
            let margin = LineStroke::default().with_color(theme::alpha(theme::p().accent, 0.4)).with_width(mw);
            let (top, bottom) = (70.0f32.max(y0 - mw), (h - 150.0).min(y1 + mw));
            if 150.0 >= tile.x0 - mw && 150.0 <= tile.x1 + mw && top < bottom {
                frame.stroke(&Path::line(Point::new(150.0, top), Point::new(150.0, bottom)), margin.clone());
            }
            for y in [h - 150.0, 70.0] {
                if y >= tile.y0 - mw && y <= tile.y1 + mw {
                    hline(frame, y, &margin);
                }
            }
        }
        Background::Grid => {
            for x in ticks(step, step, w, tile.x0 - lw, tile.x1 + lw) {
                frame.stroke(&Path::line(Point::new(x, y0), Point::new(x, y1)), line.clone());
            }
            for y in ticks(step, step, h, tile.y0 - lw, tile.y1 + lw) {
                hline(frame, y, &line);
            }
        }
        Background::Dots => {
            // Zoomed out: every 2nd/4th dot, like the tablet.
            let mut s = step;
            while s * lod < 8.0 {
                s *= 2.0;
            }
            let r = (1.3 / lod).max(0.6);
            let (first, last) = if extent.is_some() { (0.0, (w, h)) } else { (f32::NEG_INFINITY, (w, h)) };
            let dots = Path::new(|b| {
                for y in ticks(s, first, last.1, tile.y0 - r, tile.y1 + r) {
                    for x in ticks(s, first, last.0, tile.x0 - r, tile.x1 + r) {
                        b.circle(Point::new(x, y), r);
                    }
                }
            });
            frame.fill(&dots, theme::PATTERN_DOT);
        }
    }
}

pub fn argb(c: u32) -> Color {
    Color::from_rgba8((c >> 16) as u8, (c >> 8) as u8, c as u8, ((c >> 24) as f32) / 255.0)
}

/// Another device's view on this notebook: the part of a sheet it shows, its laser pointer.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteMark {
    pub sheet_id: String,
    /// Sheet points.
    pub rect: Rect,
    pub laser: Option<(f32, f32)>,
    pub name: String,
}

/// A spot that was jumped to (a search hit, a heading): it lights up in the accent and fades.
#[derive(Debug, Clone, PartialEq)]
pub struct Flash {
    pub sheet_id: String,
    /// Sheet points.
    pub rect: Rect,
    pub at: std::time::Instant,
}

impl Flash {
    /// 1 when it starts, fading to 0 over [`crate::search::FLASH`] (eased: it holds a moment).
    pub fn strength(&self) -> f32 {
        let t = (self.at.elapsed().as_secs_f32() / crate::search::FLASH.as_secs_f32()).clamp(0.0, 1.0);
        1.0 - t * t
    }
    pub fn done(&self) -> bool {
        self.at.elapsed() >= crate::search::FLASH
    }
}

/// A stroke another device is drawing right now (shown until the stored stroke arrives).
#[derive(Default)]
pub struct LiveInk {
    pub sheet_id: String,
    pub brush: ink::Brush,
    pub points: Vec<ink::InkPoint>,
    pub done: Option<std::time::Instant>,
    /// Its mesh per view, with what it was built from (built again only when the points change).
    pub mesh: RefCell<Vec<(Slot, (usize, [u32; 6]), mesh::Cache)>>,
}

impl LiveInk {
    /// The stroke as a mesh in page units (None: nothing to draw yet).
    fn mesh(&self, slot: Slot, clip: Rectangle) -> Option<mesh::Cache> {
        let (first, last) = (self.points.first()?, self.points.last()?);
        let key = (self.points.len(), [first.x.to_bits(), first.y.to_bits(), last.x.to_bits(), last.y.to_bits(), last.pressure.to_bits(), self.brush.width.to_bits()]);
        let mut all = self.mesh.borrow_mut();
        if let Some((_, _, cache)) = all.iter().find(|m| m.0 == slot && m.1 == key) {
            return Some(cache.clone());
        }
        let meshes = ink_meshes_tol([(self.points.as_slice(), &self.brush)], 0.0, clip);
        all.retain(|m| m.0 != slot);
        if meshes.is_empty() {
            return None;
        }
        let cache = mesh::Cache::new(Arc::from(meshes));
        all.push((slot, key, cache.clone()));
        Some(cache)
    }
}

/// The laid-out notebook.
pub struct Pages {
    /// Other devices' views (live session).
    pub remote: RefCell<Vec<RemoteMark>>,
    /// Strokes being drawn on other devices, by (device, stroke).
    pub live: RefCell<HashMap<(String, u64), LiveInk>>,
    /// Someone erasing right now, per sheet (shown before the stored result arrives).
    pub erasing: RefCell<HashMap<String, Erasing>>,
    /// Draw other devices' views as frames (the laser point shows either way).
    pub show_remote_frame: Cell<bool>,
    /// The spot just jumped to, lighting up.
    pub flash: Option<Flash>,
    pub pages: Vec<Page>,
    pub headers: Vec<Header>,
    pub bounds: Rect,
    pub total_pages: u32,
    load: Loader,
    /// PDF backgrounds (None when pdfium is missing: PDF pages stay white).
    pdf: Option<Arc<Pdf>>,
    frame: Cell<u64>,
    /// Unique per opened notebook, so the view never carries a camera over to another one.
    id: u64,
}

static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

impl Pages {
    pub fn new(layout: nl::Layout, load: Loader, pdf: Option<Arc<Pdf>>) -> Self {
        Pages {
            remote: RefCell::new(Vec::new()),
            live: RefCell::new(HashMap::new()),
            erasing: RefCell::new(HashMap::new()),
            show_remote_frame: Cell::new(true),
            flash: None,
            pages: layout.sheets.into_iter().map(Page::new).collect(),
            headers: layout.headers,
            bounds: layout.bounds,
            total_pages: layout.total_pages,
            load,
            pdf,
            frame: Cell::new(0),
            id: NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        }
    }

    /// Reads a sheet again (it changed on disk or was edited): its meshes follow for the cells
    /// that changed.
    pub fn reload_sheet(&self, sheet_id: &str) {
        self.reload(sheet_id, None);
    }

    /// Like [`Pages::reload_sheet`], from the sheet as it is open here (just stored): nothing is
    /// read from disk.
    pub fn reload_sheet_from(&self, sheet_id: &str, sheet: &n0te_core::sheet::Sheet) {
        self.reload(sheet_id, Some(sheet));
    }

    fn reload(&self, sheet_id: &str, open: Option<&n0te_core::sheet::Sheet>) {
        let Some(p) = self.pages.iter().find(|p| p.placed.sheet_id == sheet_id) else {
            self.live.borrow_mut().retain(|_, l| !(l.sheet_id == sheet_id && l.done.is_some()));
            return;
        };
        let old = p.content.borrow().clone();
        let content = Rc::new((self.load)(sheet_id, open, old.as_deref()));
        // Read again but the same (a save that merged nothing): everything stays as it is.
        if let Some(old) = &old
            && old.version.is_some()
            && old.version == content.version
        {
            return;
        }
        p.forget();
        *p.content.borrow_mut() = Some(content.clone());
        // A finished live stroke goes only once the sheet holds it: the save that triggered this
        // reload may predate it (drawn just after), and dropping it then made it blink.
        // Live erasing likewise: until none of the strokes it hid or cut are stored any more.
        {
            let mut erasing = self.erasing.borrow_mut();
            if erasing.get(sheet_id).is_some_and(|e| e.settled(&content.strokes)) {
                erasing.remove(sheet_id);
            }
        }
        self.live.borrow_mut().retain(|_, l| !(l.sheet_id == sheet_id && l.done.is_some() && stored_has(content.strokes.iter().map(|s| &**s), l)));
    }

    /// Another device erasing on `sheet_id`: strokes it hid and area-eraser dabs (x, y, r).
    pub fn live_erase(&self, sheet_id: &str, ids: &[String], dabs: &[[f32; 3]]) {
        let Some(page) = self.pages.iter().find(|p| p.placed.sheet_id == sheet_id) else { return };
        let content = page.content(&self.load);
        let mut all = self.erasing.borrow_mut();
        all.retain(|_, e| e.at.elapsed().as_secs() < 15);
        let e = all.entry(sheet_id.to_string()).or_default();
        e.at = std::time::Instant::now();
        e.hidden.extend(ids.iter().cloned());
        for d in dabs {
            // Only strokes there now get cut; one drawn here afterwards stays whole.
            for s in content.strokes.iter().filter(|s| touches(&s.bounds, d, s.brush.width)) {
                e.cut.insert(s.id.clone());
            }
            e.dabs.push((d[0], d[1], d[2]));
        }
    }

    /// New samples (packed LE f32 x, y, pressure, t) of a stroke another device is drawing.
    pub fn live_samples(&self, device: &str, stroke: u64, sheet_id: &str, brush: (f32, f32, u32, u8), samples: &[u8], done: bool) {
        let mut live = self.live.borrow_mut();
        let l = live.entry((device.to_string(), stroke)).or_insert_with(|| LiveInk {
            sheet_id: sheet_id.to_string(),
            brush: ink::Brush { width: brush.0, thinning: brush.1, color: brush.2, kind: brush.3 },
            points: Vec::new(),
            done: None,
            mesh: Default::default(),
        });
        for c in samples.chunks_exact(16) {
            let f = |i: usize| f32::from_le_bytes(c[i..i + 4].try_into().expect("4 bytes"));
            l.points.push(ink::InkPoint { x: f(0), y: f(4), pressure: f(8), t_ms: f(12).max(0.0) as u32 });
        }
        if done {
            l.done = Some(std::time::Instant::now());
        }
        // Done strokes whose stored version never came: drop after a while.
        live.retain(|_, l| l.done.is_none_or(|t| t.elapsed().as_secs() < 8));
    }

    /// Text boxes on the pages in view: (sheet id, the sheet's rect, box).
    pub fn visible_texts(&self, cam: &Camera) -> Vec<(String, Rect, TextBox)> {
        let view = cam.view_rect();
        let mut out = Vec::new();
        for p in self.pages.iter().filter(|p| p.rect().intersects(&view)) {
            let c = p.content(&self.load);
            if p.placed.spec.background == Background::Text {
                // The text page's body, inside the page margins (an empty page shows a hint).
                let text = if c.body.trim().is_empty() { tr("*Leere Textseite — anklicken zum Schreiben*").to_string() } else { c.body.clone() };
                let w = (p.placed.rect.width() - 112.0).max(100.0);
                out.push((p.placed.sheet_id.clone(), p.placed.rect, TextBox { id: n0te_core::sheet::BODY_ID.into(), x: 56.0, y: 64.0, w, size: 12.0, text }));
            }
            for t in &c.texts {
                out.push((p.placed.sheet_id.clone(), p.placed.rect, t.clone()));
            }
        }
        out
    }

    /// The sheet under a page point.
    pub fn sheet_at(&self, x: f32, y: f32) -> Option<&Placed> {
        self.pages.iter().map(|p| &p.placed).find(|p| p.rect.contains(x, y))
    }

    /// A text page's first line (its name in the outline when it has none): Markdown marks
    /// left out, at most 60 characters.
    pub fn first_line(&self, sheet_id: &str) -> Option<String> {
        let page = self.pages.iter().find(|p| p.placed.sheet_id == sheet_id && p.placed.spec.background == Background::Text)?;
        first_line(&page.content(&self.load).body)
    }

    /// The coordinate system under a page point: (sheet id, plot id).
    pub fn plot_at(&self, x: f32, y: f32) -> Option<(String, String)> {
        let page = self.pages.iter().find(|p| p.rect().contains(x, y))?;
        let r = page.rect();
        page.plot_at(&self.load, x - r.x0, y - r.y0).map(|id| (page.placed.sheet_id.clone(), id))
    }

    /// The text box at page point (x, y): (sheet id, box id). A box is as tall as its lines.
    pub fn text_at(&self, x: f32, y: f32) -> Option<(String, String)> {
        let page = self.pages.iter().find(|p| p.rect().contains(x, y))?;
        let r = page.rect();
        let (lx, ly) = (x - r.x0, y - r.y0);
        let content = page.content(&self.load);
        content
            .texts
            .iter()
            .rev()
            .find(|t| {
                let lines = t.text.lines().count().max(1) as f32;
                lx >= t.x - 4.0 && lx <= t.x + t.w + 4.0 && ly >= t.y - 4.0 && ly <= t.y + lines * t.size * 1.5 + 4.0
            })
            .map(|t| (page.placed.sheet_id.clone(), t.id.clone()))
    }

    /// A sheet's coordinate systems as drawn (read from the library if not shown yet).
    pub fn plots_of(&self, sheet_id: &str) -> Vec<Plot> {
        self.pages.iter().find(|p| p.placed.sheet_id == sheet_id).map(|p| p.content(&self.load).plots.clone()).unwrap_or_default()
    }

    /// Draws a PDF page's background: the thumbnail, sharp tiles of the visible part on top.
    /// Space inserted into the page splits it ([`pdf_parts`]): each stretch of the PDF (at its own
    /// size, never stretched) is drawn moved down to its place, in a layer of its own clipped to
    /// that place (the software renderer clips images only to their layer), so whatever goes on
    /// the page over the PDF must be drawn in a later layer.
    fn draw_pdf(&self, r: &mut iced::Renderer, page: &Page, at: Point, cam: &Camera, view: &Rect, clip: Rectangle) {
        use iced::advanced::image::{FilterMethod, Image, Renderer as _};
        let spec = &page.placed.spec;
        let (Some(pdf), Some((blob, index))) = (&self.pdf, &spec.pdf) else { return };
        let rect = page.rect();
        let size = n0te_pdf::PageSize { width: spec.width, height: spec.base_height() };
        let visible = Rect::new(view.x0.max(rect.x0) - rect.x0, view.y0.max(rect.y0) - rect.y0, view.x1.min(rect.x1) - rect.x0, view.y1.min(rect.y1) - rect.y0);
        let b = pdf::bucket(cam.zoom);
        for part in pdf_parts(spec, visible) {
            // A page without gaps draws exactly as it always did; a split one keeps each stretch
            // inside its place on the page (the blank space between stays paper).
            let place = Rectangle::new(Point::new(at.x + part.dst.x0 * cam.zoom, at.y + part.dst.y0 * cam.zoom), Size::new(part.dst.width() * cam.zoom, part.dst.height() * cam.zoom));
            let Some(clip) = (if spec.gaps.is_empty() { Some(clip) } else { place.intersection(&clip) }) else { continue };
            let mut levels = vec![pdf.tiles_for(blob, *index, size, pdf::THUMB, part.src, n0te_pdf::worker::priority::BACKGROUND)];
            if b > pdf::THUMB
                && let Some(seen) = part.visible
            {
                levels.push(pdf.tiles_for(blob, *index, size, b, seen, n0te_pdf::worker::priority::VISIBLE));
            }
            let draw = |r: &mut iced::Renderer| {
                for tiles in levels {
                    for (t, img) in tiles {
                        if let Some(h) = img {
                            let bounds = Rectangle::new(Point::new(at.x + t.x0 * cam.zoom, at.y + (t.y0 + part.shift) * cam.zoom), Size::new(t.width() * cam.zoom, t.height() * cam.zoom));
                            r.draw_image(Image::new(h).filter_method(FilterMethod::Linear), bounds, clip);
                        }
                    }
                }
            };
            if spec.gaps.is_empty() {
                draw(r);
            } else {
                r.with_layer(clip, draw);
            }
        }
    }

    /// Draws a page's images (over the paper and PDF, under the ink). Decoded once per file.
    fn draw_images(&self, r: &mut iced::Renderer, page: &Page, at: Point, cam: &Camera, clip: Rectangle) {
        use iced::advanced::image::{FilterMethod, Image, Renderer as _};
        let content = page.content(&self.load);
        for (img, path) in &content.images {
            // From the bytes: blobs have no file extension, which the decoder would go by (a file
            // not there yet is looked for again next time; one read is never looked at again).
            let Some(handle) = IMAGES.with(|m| {
                let mut m = m.borrow_mut();
                if let Some(h) = m.get(path) {
                    return Some(h.clone());
                }
                let h = iced::advanced::image::Handle::from_bytes(std::fs::read(path).ok()?);
                m.insert(path.clone(), h.clone());
                Some(h)
            }) else {
                continue;
            };
            let bounds = Rectangle::new(Point::new(at.x + img.x * cam.zoom, at.y + img.y * cam.zoom), Size::new(img.w * cam.zoom, img.h * cam.zoom));
            r.draw_image(Image::new(handle).filter_method(FilterMethod::Linear), bounds, clip);
        }
    }

    /// A new layout (fold, unfold): pages that only moved keep their strokes and caches.
    pub fn relayout(&mut self, layout: nl::Layout) {
        let mut old: HashMap<String, Page> = self.pages.drain(..).map(|p| (p.placed.sheet_id.clone(), p)).collect();
        self.pages = layout
            .sheets
            .into_iter()
            .map(|placed| match old.remove(&placed.sheet_id) {
                Some(p) if p.placed.spec == placed.spec => Page { placed, ..p },
                _ => Page::new(placed),
            })
            .collect();
        self.headers = layout.headers;
        self.bounds = layout.bounds;
        self.total_pages = layout.total_pages;
    }

    pub fn page_of_sheet(&self, sheet_id: &str) -> Option<usize> {
        self.pages.iter().position(|p| p.placed.sheet_id == sheet_id)
    }

    /// Main pages in order (indices into `pages`).
    pub fn mains(&self) -> Vec<usize> {
        (0..self.pages.len()).filter(|&i| self.pages[i].placed.side == 0).collect()
    }

    /// The main page under the vertical centre of the viewport (its number).
    pub fn current_number(&self, camera: &Camera) -> u32 {
        let cy = camera.y + camera.viewport.height / camera.zoom / 2.0;
        let mut best = self.pages.iter().find(|p| p.placed.side == 0).map(|p| p.placed.number).unwrap_or(0);
        for p in self.pages.iter().filter(|p| p.placed.side == 0) {
            if p.placed.rect.y0 - 18.0 <= cy {
                best = p.placed.number;
            }
        }
        best
    }

    /// Index of the main page with `number` that is visible in the layout (folded ones aren't).
    pub fn index_of_number(&self, number: u32) -> Option<usize> {
        self.pages.iter().position(|p| p.placed.side == 0 && p.placed.number == number)
    }

    /// Camera that shows page `index` at the top, centred horizontally on it.
    pub fn camera_for(&self, index: usize, camera: &Camera) -> Camera {
        let r = self.pages[index].placed.rect;
        let view_w = camera.viewport.width / camera.zoom;
        self.clamp(Camera { x: r.x0 + r.width() / 2.0 - view_w / 2.0, y: r.y0 - 28.0 / camera.zoom, ..*camera })
    }

    /// The first page filling ~80 % of the width (at most 100 %).
    pub fn initial_camera(&self, viewport: Size) -> Camera {
        let w = self.pages.first().map(|p| p.placed.rect.width()).unwrap_or(nl::A4_W);
        let zoom = (viewport.width * 0.8 / w).clamp(0.25, 1.0);
        let cam = Camera { x: 0.0, y: 0.0, zoom, viewport, origin: Point::ORIGIN };
        if self.pages.is_empty() { cam } else { self.camera_for(0, &cam) }
    }

    /// The page in the middle of the view, as wide as it is with a margin either side (pt):
    /// what "fits the width" means when the view changes size.
    pub fn column_at(&self, c: &Camera) -> f32 {
        let (cx, cy) = (c.x + c.viewport.width / c.zoom / 2.0, c.y + c.viewport.height / c.zoom / 2.0);
        let w = self.sheet_at(cx, cy).or_else(|| self.pages.first().map(|p| &p.placed)).map(|p| p.rect.width()).unwrap_or(nl::A4_W);
        w + 2.0 * COLUMN_MARGIN
    }

    /// Keeps the content reachable: at most half a viewport of empty space around it.
    pub fn clamp(&self, mut c: Camera) -> Camera {
        c.zoom = c.zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        if self.bounds.is_empty() || c.viewport.width <= 0.0 {
            return c;
        }
        let (vw, vh) = (c.viewport.width / c.zoom, c.viewport.height / c.zoom);
        let b = &self.bounds;
        let (minx, maxx) = (b.x0 - vw * 0.5, b.x1 - vw * 0.5);
        let (miny, maxy) = (b.y0 - vh * 0.5, b.y1 - vh * 0.5);
        c.x = c.x.clamp(minx.min(maxx), maxx.max(minx));
        c.y = c.y.clamp(miny.min(maxy), maxy.max(miny));
        c
    }

    /// The node of the group title at page point `p`.
    pub fn header_node_at(&self, p: Point) -> Option<String> {
        self.header_at(p).map(|h| h.node.clone())
    }

    fn header_at(&self, p: Point) -> Option<&Header> {
        self.headers.iter().find(|h| h.rect.contains(p.x, p.y))
    }

    /// Before a frame of `cam` (view `slot`): builds what the pages in view lack at its zoom,
    /// for at most `budget`; whether everything is there (else the view asks for another frame).
    pub fn prepare(&self, cam: &Camera, slot: Slot, budget: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + budget;
        let view = cam.view_rect();
        let erasing = self.erasing.borrow();
        for page in self.pages.iter().filter(|p| p.rect().intersects(&view)) {
            let r = page.rect();
            let local = Rect { x0: view.x0 - r.x0, y0: view.y0 - r.y0, x1: view.x1 - r.x0, y1: view.y1 - r.y0 };
            let clip = Rectangle::new(Point::ORIGIN, Size::new(r.width(), r.height()));
            if !page.prepare_ink(cam, slot, local, &self.load, erasing.get(&page.placed.sheet_id), Some(clip), deadline) {
                return false;
            }
        }
        true
    }

    fn evict_stale(&self) {
        let mut cached: Vec<&Page> = self.pages.iter().filter(|p| p.is_cached()).collect();
        if cached.len() <= MAX_CACHED {
            return;
        }
        cached.sort_by_key(|p| p.last_frame.get());
        let now = self.frame.get();
        let n = cached.len() - MAX_CACHED;
        for p in cached.into_iter().take(n) {
            if p.last_frame.get() + 2 < now {
                p.evict();
            }
        }
    }
}

/// Room either side of a page that counts as fitting the width (pt).
const COLUMN_MARGIN: f32 = 24.0;

/// The camera after its view changed size (the window, the outline or the split's panes): the
/// page stays as big as it was, with more or less room around it, and a page that fitted the
/// width still fits (narrower, it shrinks to the width). What was in the middle stays in the
/// middle, what was near the top stays near the top. `column`: see [`Pages::column_at`].
pub fn resized(c: Camera, viewport: Size, column: f32) -> Camera {
    if c.viewport.width <= 0.0 || c.viewport.height <= 0.0 {
        return Camera { viewport, ..c };
    }
    let anchor = 80.0f32.min(c.viewport.height / 2.0);
    let (mx, ty) = (c.x + c.viewport.width / 2.0 / c.zoom, c.y + anchor / c.zoom);
    let fitted = column * c.zoom <= c.viewport.width * 1.02;
    let zoom = if fitted { c.zoom.min(viewport.width / column) } else { c.zoom }.clamp(MIN_ZOOM, MAX_ZOOM);
    Camera { x: mx - viewport.width / 2.0 / zoom, y: ty - anchor / zoom, zoom, viewport, ..c }
}

/// Level of detail for a zoom: the next power of two at or above it.
pub fn lod_for(zoom: f32) -> f32 {
    2f32.powf(zoom.max(1e-3).log2().ceil()).clamp(0.125, 8.0)
}

/// What part of notebook space the view shows: (`x`, `y`) at the viewport's top-left; one
/// page unit is `zoom` logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
    pub viewport: Size,
    /// Where the view sits in the window (touchpad gestures come in window coordinates).
    pub origin: Point,
}

impl Default for Camera {
    fn default() -> Self {
        Camera { x: 0.0, y: 0.0, zoom: 1.0, viewport: Size::ZERO, origin: Point::ORIGIN }
    }
}

impl Camera {
    pub fn view_rect(&self) -> Rect {
        Rect::new(self.x, self.y, self.x + self.viewport.width / self.zoom, self.y + self.viewport.height / self.zoom)
    }
    pub fn to_screen(&self, bounds: Rectangle, x: f32, y: f32) -> Point {
        Point::new(bounds.x + (x - self.x) * self.zoom, bounds.y + (y - self.y) * self.zoom)
    }
    pub fn to_page(&self, bounds: Rectangle, p: Point) -> Point {
        Point::new(self.x + (p.x - bounds.x) / self.zoom, self.y + (p.y - bounds.y) / self.zoom)
    }
}

/// The page view widget.
pub struct PageView<'a, Message> {
    pages: &'a Pages,
    camera: Camera,
    on_camera: Box<dyn Fn(Camera) -> Message + 'a>,
    on_fold: Box<dyn Fn(String) -> Message + 'a>,
    on_double: Option<Box<dyn Fn(String, f32, f32) -> Message + 'a>>,
    on_plot: Option<Box<dyn Fn(String, String) -> Message + 'a>>,
    /// n0te Tablet: the mouse draws like a pen (these events), fingers pan and zoom.
    on_pen: Option<Box<dyn Fn(crate::tablet::pen::PenEvent) -> Message + 'a>>,
    /// n0te Tablet: one finger writes too (two still pan and zoom).
    finger_draws: bool,
    /// Which view this is (its own GPU caches).
    slot: Slot,
}

pub fn page_view<'a, Message>(pages: &'a Pages, camera: Camera, on_camera: impl Fn(Camera) -> Message + 'a, on_fold: impl Fn(String) -> Message + 'a) -> PageView<'a, Message> {
    PageView { pages, camera, on_camera: Box::new(on_camera), on_fold: Box::new(on_fold), on_double: None, on_plot: None, on_pen: None, finger_draws: false, slot: Slot::Main }
}

impl<'a, Message> PageView<'a, Message> {
    /// A double click on a page: (sheet id, x, y in the sheet's points) — a new text box there.
    pub fn on_double_click(mut self, f: impl Fn(String, f32, f32) -> Message + 'a) -> Self {
        self.on_double = Some(Box::new(f));
        self
    }

    /// n0te Tablet: a left drag writes (sent as pen events in window coordinates, pressure
    /// 0.5) instead of panning; the middle button and the wheel still move the view; one finger
    /// pans and two pinch (both ignored while a pen is near the screen: a resting palm).
    pub fn on_pen(mut self, f: impl Fn(crate::tablet::pen::PenEvent) -> Message + 'a) -> Self {
        self.on_pen = Some(Box::new(f));
        self
    }

    /// n0te Tablet: one finger writes like the pen.
    pub fn finger_draws(mut self, on: bool) -> Self {
        self.finger_draws = on;
        self
    }

    /// Which view this is: every view of the same pages needs GPU caches of its own.
    pub fn slot(mut self, slot: Slot) -> Self {
        self.slot = slot;
        self
    }

    /// A click on a coordinate system: (sheet id, plot id) — its editor opens.
    pub fn on_plot_click(mut self, f: impl Fn(String, String) -> Message + 'a) -> Self {
        self.on_plot = Some(Box::new(f));
        self
    }
}

#[derive(Default)]
struct State {
    /// Where a left/middle drag started (screen) and whether it moved (else it's a click).
    drag: Option<(Point, Point, bool)>,
    /// The last click (time, page point), for double clicks.
    last_click: Option<(std::time::Instant, Point)>,
    modifiers: keyboard::Modifiers,
    /// (notebook, camera the app gave us, camera we published from it): iced delivers a batch
    /// of events before the app applies the camera, so each event builds on what we already
    /// published — for the same notebook only.
    pending: Option<(u64, Camera, Camera)>,
    /// n0te Tablet: the mouse is writing (left button held), since this instant.
    mouse_pen: Option<std::time::Instant>,
    /// Fingers on the view (id → position), for panning and pinching.
    fingers: Vec<(u64, Point)>,
    /// Finger writing: the finger that writes and since when.
    finger_pen: Option<(u64, std::time::Instant)>,
}

fn mouse_sample(p: Point, since: std::time::Instant) -> crate::tablet::pen::Sample {
    crate::tablet::pen::Sample { x: p.x, y: p.y, pressure: 0.5, tilt: (0.0, 0.0), t_ms: since.elapsed().as_millis() as u32 }
}

/// How long the page view builds meshes before a frame at most (the rest in the next ones).
const PREPARE_BUDGET: std::time::Duration = std::time::Duration::from_millis(8);

/// Pixels per wheel "line".
const LINE_PX: f32 = 48.0;

impl<'a, Message> Widget<Message, Theme, iced::Renderer> for PageView<'a, Message> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }
    fn state(&self) -> tree::State {
        tree::State::new(State::default())
    }
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }
    fn layout(&mut self, _tree: &mut widget::Tree, _renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        layout::Node::new(limits.max())
    }

    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &iced::Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();
        // No room (a pane being closed): nothing to report, the camera stays for later.
        if bounds.width < 1.0 || bounds.height < 1.0 {
            return;
        }
        // A frame is coming: build a little of what the zoom lacks (cells past a level stand in
        // as they were), another frame for the rest.
        if let Event::Window(iced::window::Event::RedrawRequested(_)) = event
            && self.camera.viewport.width > 0.0
            && !self.pages.prepare(&self.camera, self.slot, PREPARE_BUDGET)
        {
            shell.request_redraw();
        }
        let cam = match state.pending {
            Some((id, seen, published)) if id == self.pages.id && seen == self.camera => published,
            _ => self.camera,
        };
        // Tell the app our size and place (first frame, resizes, sidebar); it owns the camera.
        if cam.viewport != bounds.size() || cam.origin != bounds.position() {
            let next = if cam.viewport.width <= 0.0 { self.pages.initial_camera(bounds.size()) } else { self.pages.clamp(resized(cam, bounds.size(), self.pages.column_at(&cam))) };
            let next = Camera { origin: bounds.position(), ..next };
            state.pending = Some((self.pages.id, self.camera, next));
            shell.publish((self.on_camera)(next));
            return;
        }
        if let Some(on_pen) = &self.on_pen {
            use crate::tablet::pen::{PenEvent, Phase};
            let ev = |phase, p: Point, since| PenEvent { phase, samples: vec![mouse_sample(p, since)], eraser: false, barrel: false, real: false };
            match event {
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                    if let Some(p) = cursor.position_over(bounds) {
                        let now = std::time::Instant::now();
                        state.mouse_pen = Some(now);
                        shell.publish(on_pen(ev(Phase::Down, p, now)));
                        shell.capture_event();
                    }
                    return;
                }
                Event::Mouse(mouse::Event::CursorMoved { position }) if state.mouse_pen.is_some() => {
                    shell.publish(on_pen(ev(Phase::Move, *position, state.mouse_pen.expect("mouse pen"))));
                    shell.capture_event();
                    return;
                }
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                    if let Some(since) = state.mouse_pen.take() {
                        let p = cursor.position().unwrap_or(Point::ORIGIN);
                        shell.publish(on_pen(ev(Phase::Up, p, since)));
                        shell.capture_event();
                    }
                    return;
                }
                Event::Touch(t) => {
                    use iced::touch;
                    // A palm resting while the pen writes: ignore it.
                    if crate::tablet::pen::in_range() {
                        state.fingers.clear();
                        return;
                    }
                    // Finger writing: one finger writes; a second one turns it into pan / zoom.
                    if self.finger_draws {
                        let fe = |phase, p: Point, since| PenEvent { phase, samples: vec![mouse_sample(p, since)], eraser: false, barrel: false, real: false };
                        match *t {
                            touch::Event::FingerPressed { id, position } if state.fingers.is_empty() && state.finger_pen.is_none() && bounds.contains(position) => {
                                let now = std::time::Instant::now();
                                state.finger_pen = Some((id.0, now));
                                state.fingers.push((id.0, position));
                                shell.publish(on_pen(fe(Phase::Down, position, now)));
                                shell.capture_event();
                                return;
                            }
                            touch::Event::FingerPressed { .. } if state.finger_pen.is_some() => {
                                // A second finger: what the first wrote goes, the two pan / zoom.
                                shell.publish(on_pen(PenEvent { phase: Phase::Cancel, samples: Vec::new(), eraser: false, barrel: false, real: false }));
                                state.finger_pen = None;
                            }
                            touch::Event::FingerMoved { id, position } if state.finger_pen.is_some_and(|f| f.0 == id.0) => {
                                let since = state.finger_pen.expect("finger").1;
                                if let Some(f) = state.fingers.iter_mut().find(|f| f.0 == id.0) {
                                    f.1 = position;
                                }
                                shell.publish(on_pen(fe(Phase::Move, position, since)));
                                shell.capture_event();
                                return;
                            }
                            touch::Event::FingerLifted { id, position } | touch::Event::FingerLost { id, position } if state.finger_pen.is_some_and(|f| f.0 == id.0) => {
                                let since = state.finger_pen.take().expect("finger").1;
                                state.fingers.retain(|f| f.0 != id.0);
                                shell.publish(on_pen(fe(Phase::Up, position, since)));
                                shell.capture_event();
                                return;
                            }
                            _ => {}
                        }
                    }
                    let before: Vec<Point> = state.fingers.iter().map(|f| f.1).collect();
                    match *t {
                        touch::Event::FingerPressed { id, position } => {
                            if bounds.contains(position) && state.fingers.len() < 2 {
                                state.fingers.push((id.0, position));
                                shell.capture_event();
                            }
                            return;
                        }
                        touch::Event::FingerMoved { id, position } => {
                            let Some(f) = state.fingers.iter_mut().find(|f| f.0 == id.0) else { return };
                            f.1 = position;
                        }
                        touch::Event::FingerLifted { id, .. } | touch::Event::FingerLost { id, .. } => {
                            state.fingers.retain(|f| f.0 != id.0);
                            return;
                        }
                    }
                    let after: Vec<Point> = state.fingers.iter().map(|f| f.1).collect();
                    let next = match (before.as_slice(), after.as_slice()) {
                        ([a], [b]) => Some(Camera { x: cam.x - (b.x - a.x) / cam.zoom, y: cam.y - (b.y - a.y) / cam.zoom, ..cam }),
                        ([a0, a1], [b0, b1]) => {
                            let (ca, cb) = (Point::new((a0.x + a1.x) / 2.0, (a0.y + a1.y) / 2.0), Point::new((b0.x + b1.x) / 2.0, (b0.y + b1.y) / 2.0));
                            let (da, db) = (a0.distance(*a1).max(1.0), b0.distance(*b1).max(1.0));
                            let zoom = (cam.zoom * db / da).clamp(MIN_ZOOM, MAX_ZOOM);
                            // The page point under the old centre stays under the new one.
                            let anchor = cam.to_page(bounds, ca);
                            Some(Camera { x: anchor.x - (cb.x - bounds.x) / zoom, y: anchor.y - (cb.y - bounds.y) / zoom, zoom, ..cam })
                        }
                        _ => None,
                    };
                    if let Some(c) = next {
                        let c = self.pages.clamp(c);
                        if c != cam {
                            state.pending = Some((self.pages.id, self.camera, c));
                            shell.publish((self.on_camera)(c));
                        }
                        shell.capture_event();
                    }
                    return;
                }
                _ => {}
            }
        }
        let mut fold = None;
        let next = match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => {
                state.modifiers = *m;
                None
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => cursor.position_in(bounds).map(|pos| {
                let (dx, dy) = match *delta {
                    mouse::ScrollDelta::Lines { x, y } => (x * LINE_PX, y * LINE_PX),
                    mouse::ScrollDelta::Pixels { x, y } => (x, y),
                };
                if state.modifiers.command() {
                    let factor = match *delta {
                        mouse::ScrollDelta::Lines { y, .. } => 1.15f32.powf(y),
                        mouse::ScrollDelta::Pixels { y, .. } => (y * 0.004).exp(),
                    };
                    let zoom = (cam.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
                    let anchor = Point::new(cam.x + pos.x / cam.zoom, cam.y + pos.y / cam.zoom);
                    Camera { x: anchor.x - pos.x / zoom, y: anchor.y - pos.y / zoom, zoom, ..cam }
                } else {
                    let (dx, dy) = if state.modifiers.shift() && dx == 0.0 { (dy, 0.0) } else { (dx, dy) };
                    Camera { x: cam.x - dx / cam.zoom, y: cam.y - dy / cam.zoom, ..cam }
                }
            }),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left | mouse::Button::Middle)) => {
                if let Some(p) = cursor.position_over(bounds) {
                    state.drag = Some((p, p, false));
                    shell.capture_event();
                }
                None
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => match state.drag {
                Some((start, last, moved)) => {
                    let moved = moved || position.distance(start) > 4.0;
                    state.drag = Some((start, *position, moved));
                    let d = *position - last;
                    moved.then(|| Camera { x: cam.x - d.x / cam.zoom, y: cam.y - d.y / cam.zoom, ..cam })
                }
                None => None,
            },
            Event::Mouse(mouse::Event::ButtonReleased(button @ (mouse::Button::Left | mouse::Button::Middle))) => {
                if let Some((start, _, false)) = state.drag.take() {
                    let at = cam.to_page(bounds, start);
                    let double = state.last_click.is_some_and(|(t, p)| t.elapsed().as_millis() < 400 && p.distance(at) * cam.zoom < 8.0);
                    state.last_click = if double { None } else { Some((std::time::Instant::now(), at)) };
                    let plot = if *button == mouse::Button::Left && self.on_plot.is_some() { self.pages.plot_at(at.x, at.y) } else { None };
                    if let Some(h) = self.pages.header_at(at) {
                        // A click on a group title folds/unfolds the group.
                        fold = Some(h.node.clone());
                    } else if let (Some(f), Some((sheet, id))) = (&self.on_plot, plot) {
                        // A click on a coordinate system edits it (a second one makes no text box).
                        state.last_click = None;
                        shell.publish(f(sheet, id));
                        shell.capture_event();
                    } else if double
                        && let (Some(f), Some(p)) = (&self.on_double, self.pages.sheet_at(at.x, at.y))
                        && !p.is_board()
                    {
                        shell.publish(f(p.sheet_id.clone(), at.x - p.rect.x0, at.y - p.rect.y0));
                    }
                }
                None
            }
            _ => None,
        };
        if let Some(node) = fold {
            shell.publish((self.on_fold)(node));
            shell.capture_event();
        }
        if let Some(c) = next {
            let c = self.pages.clamp(c);
            if c != cam {
                state.pending = Some((self.pages.id, self.camera, c));
                shell.publish((self.on_camera)(c));
            }
            shell.capture_event();
        }
    }

    fn mouse_interaction(&self, tree: &widget::Tree, layout: Layout<'_>, cursor: mouse::Cursor, _viewport: &Rectangle, _renderer: &iced::Renderer) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();
        match (state.drag, cursor.position_over(bounds)) {
            (Some((_, _, true)), _) => mouse::Interaction::Grabbing,
            (_, Some(p)) if self.pages.header_at(self.camera.to_page(bounds, p)).is_some() => mouse::Interaction::Pointer,
            (_, Some(p)) if self.on_plot.is_some() && {
                let at = self.camera.to_page(bounds, p);
                self.pages.plot_at(at.x, at.y).is_some()
            } =>
            {
                mouse::Interaction::Pointer
            }
            (_, Some(_)) => mouse::Interaction::Grab,
            _ => mouse::Interaction::default(),
        }
    }

    fn draw(
        &self,
        _tree: &widget::Tree,
        renderer: &mut iced::Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let pages = self.pages;
        let cam = self.camera;
        let frame_no = pages.frame.get() + 1;
        pages.frame.set(frame_no);
        // Light designs: the desk is the chrome tone (pages are white, as is their surface), and
        // each page gets a hairline, so white paper never melts into it (no shadow: the software
        // renderer draws shadows past the view's clip).
        let light = !theme::p().dark;
        renderer.fill_quad(renderer::Quad { bounds, ..Default::default() }, if light { theme::p().chrome } else { theme::p().surface });
        if cam.viewport.width <= 0.0 {
            return;
        }
        let current = pages.current_number(&cam);
        let view = cam.view_rect();
        let label = |r: &mut iced::Renderer, content: String, at: Point, width: f32, color: Color, size: f32, font: iced::Font, align: text::Alignment| {
            r.fill_text(
                text::Text {
                    content,
                    bounds: Size::new(width, size * 1.6),
                    size: size.into(),
                    line_height: text::LineHeight::default(),
                    font,
                    align_x: align,
                    align_y: iced::alignment::Vertical::Top,
                    shaping: text::Shaping::Advanced,
                    wrapping: text::Wrapping::None,
                },
                at,
                color,
                bounds,
            );
        };
        let visible: Vec<&Page> = pages.pages.iter().filter(|p| p.rect().intersects(&view)).collect();
        let slot = self.slot;
        // The part of the view on a page, in its own points.
        let local = |page: &Page| {
            let r = page.rect();
            Rect { x0: view.x0 - r.x0, y0: view.y0 - r.y0, x1: view.x1 - r.x0, y1: view.y1 - r.y0 }
        };
        // Three layers: paper (+ PDF images), then ink, then labels — within one layer iced
        // draws images after meshes, which would put the PDF over the ink.
        renderer.with_layer(bounds, |r| {
            for page in &visible {
                page.last_frame.set(frame_no);
                let rect = page.rect();
                let at = cam.to_screen(bounds, rect.x0, rect.y0);
                let (w, h) = page.size();
                r.fill_quad(renderer::Quad { bounds: Rectangle::new(at, Size::new(w * cam.zoom, h * cam.zoom)), ..Default::default() }, theme::p().paper);
                page.draw_pattern(r, at, &cam, slot, local(page), Some((w, h)), frame_no);
                pages.draw_pdf(r, page, at, &cam, &view, bounds);
                if page.placed.spec.gaps.is_empty() {
                    pages.draw_images(r, page, at, &cam, bounds);
                } else {
                    // Over the layers of a split PDF page's stretches.
                    r.with_layer(bounds, |r| pages.draw_images(r, page, at, &cam, bounds));
                }
            }
        });
        renderer.with_layer(bounds, |r| {
            // Coordinate systems: ink under everything written (over the paper, PDF and images:
            // in the layer before, images go over meshes).
            for page in &visible {
                let Some(plots) = page.plot_mesh(&pages.load, slot) else { continue };
                let rect = page.rect();
                let at = cam.to_screen(bounds, rect.x0, rect.y0);
                r.with_translation(Vector::new(at.x, at.y), |r| {
                    r.with_transformation(Transformation::scale(cam.zoom), |r| r.draw_mesh_cache(plots));
                });
            }
            for l in pages.live.borrow().values() {
                let Some(page) = visible.iter().find(|p| p.placed.sheet_id == l.sheet_id) else { continue };
                let rect = page.rect();
                let Some(cache) = l.mesh(slot, Rectangle::new(Point::ORIGIN, Size::new(rect.width(), rect.height()))) else { continue };
                let at = cam.to_screen(bounds, rect.x0, rect.y0);
                r.with_translation(Vector::new(at.x, at.y), |r| {
                    r.with_transformation(Transformation::scale(cam.zoom), |r| r.draw_mesh_cache(cache));
                });
            }
            let erasing = pages.erasing.borrow();
            for page in &visible {
                let rect = page.rect();
                let at = cam.to_screen(bounds, rect.x0, rect.y0);
                let clip = Rectangle::new(Point::ORIGIN, Size::new(rect.width(), rect.height()));
                page.draw_ink(r, at, &cam, slot, local(page), &pages.load, erasing.get(&page.placed.sheet_id), Some(clip), frame_no);
            }
        });
        renderer.with_layer(bounds, |r| {
            for page in &visible {
                let rect = page.rect();
                let at = cam.to_screen(bounds, rect.x0, rect.y0);
                // Number left of main pages, "3 L1" above side pages, BOARD over boards.
                if cam.zoom >= 0.18 {
                    let p = &page.placed;
                    let color = if p.side == 0 && p.number == current { theme::p().accent } else { theme::p().muted };
                    if p.side == 0 {
                        // Not when it would stick out of a narrow view (a pane of the split):
                        // the software renderer doesn't clip text to the view.
                        if at.x - 46.0 - 24.0 >= bounds.x {
                            label(r, format!("{:02}", p.number), Point::new(at.x - 46.0, at.y), 40.0, color, 11.0, theme::MONO, text::Alignment::Right);
                        }
                    } else {
                        let w = rect.width() * cam.zoom;
                        label(r, p.label(), Point::new(at.x, at.y - 18.0), w, color, 11.0, theme::MONO, if p.side < 0 { text::Alignment::Left } else { text::Alignment::Right });
                    }
                    if p.is_board() {
                        label(r, "BOARD".into(), Point::new(at.x, at.y - 18.0), rect.width() * cam.zoom, theme::p().muted, 11.0, theme::MONO, text::Alignment::Right);
                    }
                }
            }
            // Group titles: fold mark, title, page count, a hairline.
            for h in pages.headers.iter().filter(|h| h.rect.intersects(&view)) {
                let at = cam.to_screen(bounds, h.rect.x0, h.rect.y0);
                let hh = h.rect.height() * cam.zoom;
                let w = h.rect.width() * cam.zoom;
                let size = (15.0 * cam.zoom).clamp(9.0, 22.0);
                let y = at.y + (hh - size * 1.3) / 2.0;
                label(r, if h.collapsed { "▸".into() } else { "▾".into() }, Point::new(at.x, y), 20.0, theme::p().accent, size, theme::SANS_BOLD, text::Alignment::Left);
                let title = if h.title.trim().is_empty() { tr("Gruppe").to_string() } else { h.title.clone() };
                label(r, format!("{title}   {}", h.count), Point::new(at.x + size * 1.4, y), w - size * 1.4, theme::p().text, size, theme::SANS_BOLD, text::Alignment::Left);
                r.fill_quad(renderer::Quad { bounds: Rectangle::new(Point::new(at.x, at.y + hh - 1.0), Size::new(w, 1.0)), ..Default::default() }, theme::p().border);
            }
            // Other devices: their view as a frame, their laser pointer as a red dot.
            for m in pages.remote.borrow().iter() {
                let Some(page) = pages.pages.iter().find(|p| p.placed.sheet_id == m.sheet_id) else { continue };
                let o = page.rect();
                let a = cam.to_screen(bounds, o.x0 + m.rect.x0, o.y0 + m.rect.y0);
                let b = cam.to_screen(bounds, o.x0 + m.rect.x1, o.y0 + m.rect.y1);
                let frame = Rectangle::new(a, Size::new((b.x - a.x).max(4.0), (b.y - a.y).max(4.0)));
                if pages.show_remote_frame.get() {
                    r.fill_quad(
                        renderer::Quad { bounds: frame, border: iced::Border { color: theme::alpha(theme::p().accent, 0.8), width: 2.0, radius: 4.0.into() }, ..Default::default() },
                        Color::TRANSPARENT,
                    );
                    label(r, m.name.to_uppercase(), Point::new(frame.x + 4.0, frame.y + 4.0), 200.0, theme::p().accent, 10.0, theme::MONO_BOLD, text::Alignment::Left);
                }
                if let Some((x, y)) = m.laser {
                    let c = cam.to_screen(bounds, o.x0 + x, o.y0 + y);
                    r.fill_quad(
                        renderer::Quad { bounds: Rectangle::new(Point::new(c.x - 7.0, c.y - 7.0), Size::new(14.0, 14.0)), border: iced::Border { radius: 7.0.into(), ..Default::default() }, ..Default::default() },
                        Color::from_rgba8(255, 59, 48, 0.9),
                    );
                }
            }
            // The spot just jumped to: an accent box around it that fades.
            if let Some(f) = pages.flash.as_ref().filter(|f| !f.done())
                && let Some(page) = visible.iter().find(|p| p.placed.sheet_id == f.sheet_id)
            {
                let o = page.rect();
                let pad = (4.0 * cam.zoom).max(3.0);
                let a = cam.to_screen(bounds, o.x0 + f.rect.x0, o.y0 + f.rect.y0);
                let b = cam.to_screen(bounds, o.x0 + f.rect.x1, o.y0 + f.rect.y1);
                let spot = Rectangle::new(Point::new(a.x - pad, a.y - pad), Size::new((b.x - a.x).max(2.0) + 2.0 * pad, (b.y - a.y).max(2.0) + 2.0 * pad));
                let k = f.strength();
                r.fill_quad(
                    renderer::Quad { bounds: spot, border: iced::Border { color: theme::alpha(theme::p().accent, 0.9 * k), width: 2.0, radius: 4.0.into() }, ..Default::default() },
                    theme::alpha(theme::p().accent, 0.22 * k),
                );
            }
        });
        pages.evict_stale();
    }
}

impl<'a, Message: 'a> From<PageView<'a, Message>> for Element<'a, Message> {
    fn from(v: PageView<'a, Message>) -> Self {
        Element::new(v)
    }
}

fn at_of(b: Rectangle) -> Point {
    Point::new(b.x, b.y)
}

/// A text's first line with something in it, Markdown marks ("# ", "- ", "> ") left out, at
/// most 60 characters.
pub fn first_line(body: &str) -> Option<String> {
    body.lines().map(|l| l.trim().trim_start_matches(['#', '-', '*', '>', ' ']).trim()).find(|l| !l.is_empty()).map(|l| l.chars().take(60).collect())
}

/// A page as a small picture (the outline): paper, PDF, pictures, coordinate systems and ink,
/// drawn from the same page data as the canvas (its ink as a coarse mesh of its own, built
/// once and again only after the sheet changed). Nothing is read or built while it is out of
/// view. A board is a dotted frame.
pub struct Thumb<'a> {
    pages: &'a Pages,
    index: usize,
    width: f32,
    current: bool,
}

/// Page `index` of the layout `width` wide (as tall as its shape says); `current`: outlined in
/// the accent.
pub fn thumb(pages: &Pages, index: usize, width: f32, current: bool) -> Thumb<'_> {
    Thumb { pages, index, width, current }
}

impl Thumb<'_> {
    fn height(&self) -> f32 {
        let r = self.pages.pages[self.index].rect();
        let aspect = if self.pages.pages[self.index].placed.is_board() { 0.75 } else { (r.height() / r.width().max(1.0)).clamp(0.3, 1.8) };
        (self.width * aspect).round()
    }
}

impl<Message> Widget<Message, Theme, iced::Renderer> for Thumb<'_> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.width), Length::Fixed(self.height()))
    }
    fn layout(&mut self, _tree: &mut widget::Tree, _renderer: &iced::Renderer, _limits: &layout::Limits) -> layout::Node {
        layout::Node::new(Size::new(self.width, self.height()))
    }
    fn draw(&self, _tree: &widget::Tree, renderer: &mut iced::Renderer, _theme: &Theme, _style: &renderer::Style, layout: Layout<'_>, _cursor: mouse::Cursor, viewport: &Rectangle) {
        let b = layout.bounds();
        let Some(clip) = b.intersection(viewport) else { return };
        let pages = self.pages;
        let page = &pages.pages[self.index];
        let rect = page.rect();
        let board = page.placed.is_board();
        let zoom = b.width / rect.width().max(1.0);
        let cam = Camera { x: rect.x0, y: rect.y0, zoom, viewport: b.size(), origin: at_of(b) };
        let at = at_of(b);
        renderer.with_layer(clip, |r| {
            r.fill_quad(renderer::Quad { bounds: b, ..Default::default() }, if board { theme::p().surface_2 } else { theme::p().paper });
            if board {
                let dots = 5.0;
                let mut y = b.y + dots;
                while y < b.y + b.height {
                    let mut x = b.x + dots;
                    while x < b.x + b.width {
                        r.fill_quad(renderer::Quad { bounds: Rectangle::new(Point::new(x, y), Size::new(1.0, 1.0)), ..Default::default() }, theme::p().muted);
                        x += dots * 2.0;
                    }
                    y += dots * 2.0;
                }
            } else {
                pages.draw_pdf(r, page, at, &cam, &rect, clip);
                pages.draw_images(r, page, at, &cam, clip);
            }
        });
        if !board {
            renderer.with_layer(clip, |r| {
                let plots = page.plot_mesh(&pages.load, Slot::Thumb);
                let ink = page.thumb_mesh(&pages.load, zoom);
                r.with_translation(Vector::new(at.x, at.y), |r| {
                    r.with_transformation(Transformation::scale(zoom), |r| {
                        if let Some(plots) = plots {
                            r.draw_mesh_cache(plots);
                        }
                        r.draw_mesh_cache(ink);
                    });
                });
            });
        }
        renderer.with_layer(clip, |r| {
            let (line, width) = if self.current { (theme::p().accent, 2.0) } else { (theme::p().border, 1.0) };
            r.fill_quad(renderer::Quad { bounds: b, border: iced::Border { color: line, width, radius: 2.0.into() }, ..Default::default() }, Color::TRANSPARENT);
        });
    }
}

impl<'a, Message: 'a> From<Thumb<'a>> for Element<'a, Message> {
    fn from(t: Thumb<'a>) -> Self {
        Element::new(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_stroke_stays_until_the_sheet_holds_it() {
        let brush = ink::Brush { width: 1.0, thinning: 0.6, color: 0xFF11_1111, kind: 0 };
        let pts = |x: f32| (0..20).map(|i| ink::InkPoint { x: x + i as f32 * 3.0, y: 100.0 + (i % 3) as f32, pressure: 0.5, t_ms: i * 8 }).collect::<Vec<_>>();
        let live = LiveInk { sheet_id: "s".into(), brush, points: pts(50.0), done: Some(std::time::Instant::now()), ..Default::default() };
        let stored = |x: f32, color: u32| Stroke { id: "a".into(), brush: ink::Brush { color, ..brush }, bounds: Rect::new(x - 0.6, 99.4, x + 57.6, 102.6), points: pts(x) };
        // The earlier save had only another stroke: keep the live one.
        assert!(!stored_has(&[stored(300.0, brush.color)], &live));
        // Same place, other colour: not it.
        assert!(!stored_has(&[stored(50.0, 0xFFD7_1921)], &live));
        // The save with it: drop the live copy.
        assert!(stored_has(&[stored(300.0, brush.color), stored(50.0, brush.color)], &live));
    }

    #[test]
    fn pdf_pages_split_where_space_was_inserted() {
        let pdf = SheetSpec { pdf: Some(("b".repeat(64), 0)), ..SheetSpec::a4(Background::Blank) };
        let r = |x0: f32, y0: f32, x1: f32, y1: f32| Rect { x0, y0, x1, y1 };
        // Without gaps: the whole page, as always (the visible part as it is).
        let view = r(100.0, 200.0, 595.0, 500.0);
        assert_eq!(pdf_parts(&pdf, view), vec![PdfPart { src: r(0.0, 0.0, 595.0, 842.0), visible: Some(view), dst: r(0.0, 0.0, 595.0, 842.0), shift: 0.0 }]);

        // 100 pt inserted at 300: the PDF above stays, the rest goes 100 pt down.
        let split = SheetSpec { height: 942.0, gaps: vec![(300.0, 100.0)], ..pdf.clone() };
        let all = pdf_parts(&split, r(0.0, 0.0, 595.0, 942.0));
        assert_eq!(all.len(), 2);
        assert_eq!((all[0].src, all[0].dst, all[0].shift), (r(0.0, 0.0, 595.0, 300.0), r(0.0, 0.0, 595.0, 300.0), 0.0));
        assert_eq!((all[1].src, all[1].dst, all[1].shift), (r(0.0, 300.0, 595.0, 842.0), r(0.0, 400.0, 595.0, 942.0), 100.0));
        // Sharp tiles only for what is in view, in PDF points: the view 350..600 shows the blank
        // space and the PDF from 300 (page 400) to 500 (page 600).
        let part = pdf_parts(&split, r(0.0, 350.0, 595.0, 600.0));
        assert_eq!(part.len(), 1, "the band above is out of view: {part:?}");
        assert_eq!((part[0].visible, part[0].shift), (Some(r(0.0, 300.0, 595.0, 500.0)), 100.0));
        assert!(pdf_parts(&split, r(0.0, 320.0, 595.0, 380.0)).is_empty(), "only blank space in view");
        let top = pdf_parts(&split, r(50.0, 100.0, 200.0, 350.0));
        assert_eq!((top.len(), top[0].visible), (1, Some(r(50.0, 100.0, 200.0, 300.0))));

        // Space at the very top: the whole PDF moves down, nothing is drawn above it.
        let at_top = SheetSpec { height: 892.0, gaps: vec![(0.0, 50.0)], ..pdf.clone() };
        let p = pdf_parts(&at_top, r(0.0, 0.0, 595.0, 892.0));
        assert_eq!((p.len(), p[0].src, p[0].dst), (1, r(0.0, 0.0, 595.0, 842.0), r(0.0, 50.0, 595.0, 892.0)));
    }

    #[test]
    fn a_resized_view_keeps_the_page_size_and_a_fitting_page_fits() {
        let cam = Camera { x: -100.0, y: 300.0, zoom: 1.0, viewport: Size::new(800.0, 600.0), origin: Point::ORIGIN };
        // Wider: the page stays as big, the middle stays in the middle, the top at the top.
        let wide = resized(cam, Size::new(1200.0, 600.0), 643.0);
        assert_eq!((wide.zoom, wide.x + 600.0, wide.y), (1.0, cam.x + 400.0, 300.0));
        // Narrower than the page that fitted: it shrinks to the width, still centred.
        let narrow = resized(cam, Size::new(400.0, 600.0), 643.0);
        assert!((narrow.zoom - 400.0 / 643.0).abs() < 1e-5);
        assert!((narrow.x + 200.0 / narrow.zoom - (cam.x + 400.0)).abs() < 1e-3);
        assert!((narrow.y + 80.0 / narrow.zoom - (cam.y + 80.0)).abs() < 1e-3, "near the top stays near the top");
        // Zoomed in past the width: the page keeps its size when the view gets narrower.
        let close = Camera { zoom: 2.0, ..cam };
        assert_eq!(resized(close, Size::new(400.0, 600.0), 643.0).zoom, 2.0);
        // Taller or shorter only: nothing moves.
        let short = resized(cam, Size::new(800.0, 500.0), 643.0);
        assert_eq!((short.x, short.y, short.zoom), (cam.x, cam.y, cam.zoom));
    }

    #[test]
    fn lod_is_next_power_of_two() {
        assert_eq!(lod_for(1.0), 1.0);
        assert_eq!(lod_for(1.01), 2.0);
        assert_eq!(lod_for(0.5), 0.5);
        assert_eq!(lod_for(100.0), 8.0);
    }

    #[test]
    fn argb_unpacks() {
        let c = argb(0xFFD71921);
        assert!((c.r - 215.0 / 255.0).abs() < 1e-6 && (c.a - 1.0).abs() < 1e-6);
    }

    #[test]
    fn only_the_tiles_in_view_are_drawn() {
        let r = |x0: f32, y0: f32, x1: f32, y1: f32| Rect { x0, y0, x1, y1 };
        // A 2 m page at 400 % (level 4: tiles of 256 pt): a view of 350 × 225 pt touches a few
        // tiles, never the hundreds the page has.
        let big = Some((5669.0, 5669.0));
        assert_eq!(tiles_in(r(1000.0, 2000.0, 1350.0, 2225.0), big, 256.0), vec![(3, 7), (4, 7), (5, 7), (3, 8), (4, 8), (5, 8)]);
        assert_eq!(tiles_in(r(0.0, 0.0, 200.0, 200.0), big, 256.0), vec![(0, 0)]);
        // Exactly on an edge: the tile beyond it isn't touched.
        assert_eq!(tiles_in(r(0.0, 0.0, 256.0, 256.0), big, 256.0), vec![(0, 0)]);
        // The view off the page or around it: only the page's tiles.
        assert!(tiles_in(r(-500.0, -500.0, -10.0, -10.0), big, 256.0).is_empty());
        assert_eq!(tiles_in(r(-500.0, -500.0, 10.0, 10.0), Some((595.0, 842.0)), 1024.0), vec![(0, 0)]);
        // A board on its own: endless, tiles left of and above its corner too.
        assert_eq!(tiles_in(r(-10.0, -10.0, 10.0, 10.0), None, 512.0), vec![(-1, -1), (0, -1), (-1, 0), (0, 0)]);
        // The last tile ends with the paper.
        assert_eq!(tile_rect(22, 22, 256.0, big), r(5632.0, 5632.0, 5669.0, 5669.0));
        assert_eq!(tile_rect(-1, 0, 512.0, None), r(-512.0, 0.0, 0.0, 512.0));
        // A broken camera (far too many tiles) draws none rather than hang.
        assert!(tiles_in(r(0.0, 0.0, 1e7, 1e7), None, 1.0).is_empty());
    }

    #[test]
    fn a_tiles_ruling_is_the_pages_ruling_there() {
        // Lines from the second step on, inside the tile (and its line width), up to the page end.
        let at: Vec<f32> = ticks(20.0, 40.0, 842.0, 95.0, 205.0).collect();
        assert_eq!(at, vec![100.0, 120.0, 140.0, 160.0, 180.0, 200.0]);
        assert_eq!(ticks(20.0, 40.0, 842.0, -5.0, 45.0).collect::<Vec<_>>(), vec![40.0]);
        assert_eq!(ticks(20.0, 20.0, 842.0, 830.0, 900.0).collect::<Vec<_>>(), vec![840.0]);
        // Endless dots: negative positions too.
        assert_eq!(ticks(24.0, f32::NEG_INFINITY, f32::INFINITY, -50.0, 0.0).collect::<Vec<_>>(), vec![-48.0, -24.0, 0.0]);
    }

    #[test]
    fn strokes_go_into_cells_by_their_middle() {
        assert_eq!(cell_of(&Rect::new(10.0, 10.0, 100.0, 40.0)), (0, 0));
        assert_eq!(cell_of(&Rect::new(500.0, 10.0, 600.0, 40.0)), (1, 0), "middle at 550");
        assert_eq!(cell_of(&Rect::new(-30.0, -30.0, -10.0, -10.0)), (-1, -1));
        // Thinning keeps the ends and drops samples closer than the tolerance.
        let pts: Vec<ink::InkPoint> = (0..11).map(|i| ink::InkPoint { x: i as f32 * 0.3, y: 0.0, pressure: 0.5, t_ms: i }).collect();
        let mut buf = Vec::new();
        let d = decimate(&pts, 1.0, &mut buf);
        assert_eq!((d[0].x, d[d.len() - 1].x), (0.0, 3.0));
        assert!(d.windows(2).take(d.len() - 2).all(|w| w[1].x - w[0].x >= 1.0 - 1e-5));
        assert_eq!(decimate(&pts, 0.0, &mut buf).len(), 11);
    }

    #[test]
    fn a_page_reads_just_what_changed_and_rebuilds_just_those_cells() {
        let sheet = n0te_core::sheet::Sheet::create(7).unwrap();
        let brush = ink::Brush::default();
        let word = |x: f32, y: f32| (0..20).map(|i| ink::InkPoint { x: x + i as f32, y: y + (i % 3) as f32, pressure: 0.5, t_ms: i }).collect::<Vec<_>>();
        let far = sheet.add_stroke(brush, &word(1500.0, 1500.0)).unwrap();
        sheet.add_stroke(brush, &word(100.0, 100.0)).unwrap();
        let none = |_: &n0te_core::sheet::ImageObj| None;
        let first = SheetContent::read(&sheet, None, none);
        assert_eq!(first.strokes.len(), 2);
        let near = sheet.add_stroke(brush, &word(140.0, 100.0)).unwrap();
        let second = SheetContent::read(&sheet, Some(&first), none);
        assert_eq!(second.strokes.len(), 3);
        // The far stroke wasn't decoded again: the same one.
        let same = |c: &SheetContent| c.strokes.iter().find(|s| s.id == far).cloned().unwrap();
        assert!(Rc::ptr_eq(&same(&first), &same(&second)));
        assert!(second.strokes.iter().any(|s| s.id == near));

        // Cells: only the near cell's mesh is built again.
        let page = Page::new(Placed { sheet_id: "s".into(), node: "n".into(), main_node: "n".into(), spec: SheetSpec { width: 2000.0, height: 2000.0, ..SheetSpec::a4(Background::Blank) }, rect: Rect::new(0.0, 0.0, 2000.0, 2000.0), side: 0, number: 1 });
        let loads = Rc::new(Cell::new(0));
        let (c1, c2) = (Rc::new(first), Rc::new(second));
        let count = loads.clone();
        let load: Loader = Box::new(move |_, _, _| {
            count.set(count.get() + 1);
            SheetContent { strokes: Vec::new(), ..Default::default() }
        });
        *page.content.borrow_mut() = Some(c1.clone());
        let cam = Camera { x: 0.0, y: 0.0, zoom: 1.0, viewport: Size::new(2000.0, 2000.0), origin: Point::ORIGIN };
        let view = Rect::new(0.0, 0.0, 2000.0, 2000.0);
        let clip = Some(Rectangle::new(Point::ORIGIN, Size::new(2000.0, 2000.0)));
        assert!(page.prepare_ink(&cam, Slot::Main, view, &load, None, clip, std::time::Instant::now() + std::time::Duration::from_secs(5)));
        let mesh_of = |cell: (i32, i32)| page.cells.borrow().map[&cell].meshes[0].3.clone();
        let (near0, far0) = (mesh_of((0, 0)), mesh_of((2, 2)));
        *page.content.borrow_mut() = Some(c2);
        page.generation.set(next_generation());
        assert!(page.prepare_ink(&cam, Slot::Main, view, &load, None, clip, std::time::Instant::now() + std::time::Duration::from_secs(5)));
        assert_ne!(mesh_of((0, 0)).id(), near0.id(), "the cell with the new stroke is built again");
        assert_eq!(mesh_of((2, 2)).id(), far0.id(), "the other keeps its mesh");
        assert_eq!(loads.get(), 0, "nothing read from disk");
        // Out of view: not built at all; a deadline gone: nothing built, not complete.
        let page2 = Page::new(page.placed.clone());
        *page2.content.borrow_mut() = Some(c1);
        assert!(page2.prepare_ink(&cam, Slot::Main, Rect::new(0.0, 0.0, 600.0, 600.0), &load, None, clip, std::time::Instant::now() + std::time::Duration::from_secs(5)));
        assert!(page2.cells.borrow().map[&(2, 2)].meshes.is_empty());
        assert!(!page2.prepare_ink(&cam, Slot::Main, view, &load, None, clip, std::time::Instant::now()));
    }

    #[test]
    fn pencil_strokes_become_three_bands() {
        let pts: Vec<ink::InkPoint> = (0..10).map(|i| ink::InkPoint { x: i as f32 * 3.0, y: 5.0, pressure: 0.5, t_ms: i * 8 }).collect();
        let pen = Stroke { id: "a".into(), brush: ink::Brush::default(), bounds: Rect::EMPTY, points: pts.clone() };
        let pencil = Stroke { brush: ink::Brush { kind: KIND_PENCIL, ..ink::Brush::default() }, ..pen.clone() };
        let clip = Rectangle::new(Point::ORIGIN, Size::new(100.0, 100.0));
        let count = |m: &[Mesh]| match &m[0] { Mesh::Solid { buffers, .. } => buffers.vertices.len(), _ => 0 };
        let one = count(&ink_meshes(&[pen], 1, clip));
        let three = count(&ink_meshes(&[pencil], 1, clip));
        assert!(three > one * 2, "pencil: outline + middle band + core");
    }
}
