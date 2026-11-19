//! The page canvas: a notebook laid out exactly like on the tablet ([`n0te_core::layout`]):
//! one column of pages with group titles between them, side pages left and right, boards as
//! landscape frames. Pan with the wheel / trackpad / left or middle drag, zoom with
//! ctrl+wheel around the cursor; a click on a group title folds or unfolds it.
//!
//! Drawing: each page's paper and pattern go into one `canvas::Cache` at a power-of-two level
//! of detail; its strokes become one triangle mesh in page units (`ink::outline_mesh`, no
//! tessellation) drawn under a GPU transformation, so neither panning nor zooming rebuilds
//! anything. A sheet's strokes are read from disk the first time it becomes visible; caches
//! of pages not drawn for a while are dropped (LRU).

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
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
use n0te_core::sheet::{Stroke, TextBox};

use crate::pdf::{self, Pdf};
use crate::theme;

pub const MIN_ZOOM: f32 = 0.08;
pub const MAX_ZOOM: f32 = 8.0;
/// Keep at most this many page caches alive (LRU by last frame drawn).
pub const MAX_CACHED: usize = 24;
/// Pencil passes (outer band, middle band, core) at the tablet's default softness.
const PENCIL_ALPHA: [f32; 3] = [0.84, 0.14, 0.24];

/// What a sheet holds.
#[derive(Default)]
pub struct SheetContent {
    pub strokes: Vec<Stroke>,
    pub texts: Vec<TextBox>,
    /// A text page's Markdown body.
    pub body: String,
}

/// Reads a sheet's content (from the library).
pub type Loader = Box<dyn Fn(&str) -> SheetContent>;

pub struct Page {
    pub placed: Placed,
    content: RefCell<Option<Rc<SheetContent>>>,
    cache: RefCell<canvas::Cache>,
    lod: Cell<f32>,
    /// (sample step, mesh): zoomed-out views use a decimated mesh.
    ink: RefCell<Option<(usize, mesh::Cache)>>,
    last_frame: Cell<u64>,
}

impl Page {
    fn new(placed: Placed) -> Self {
        Page {
            placed,
            content: RefCell::new(None),
            cache: RefCell::new(canvas::Cache::new()),
            lod: Cell::new(0.0),
            ink: RefCell::new(None),
            last_frame: Cell::new(0),
        }
    }

    pub fn rect(&self) -> Rect {
        self.placed.rect
    }

    fn size(&self) -> (f32, f32) {
        (self.placed.rect.width(), self.placed.rect.height())
    }

    fn is_cached(&self) -> bool {
        self.lod.get() > 0.0 || self.ink.borrow().is_some()
    }

    fn evict(&self) {
        *self.cache.borrow_mut() = canvas::Cache::new();
        *self.ink.borrow_mut() = None;
        self.lod.set(0.0);
    }

    fn content(&self, load: &Loader) -> Rc<SheetContent> {
        self.content.borrow_mut().get_or_insert_with(|| Rc::new(load(&self.placed.sheet_id))).clone()
    }

    /// Paper and pattern at `lod`, built on first use.
    fn background(&self, renderer: &iced::Renderer, lod: f32) -> canvas::Geometry {
        let (w, h) = self.size();
        let spec = &self.placed.spec;
        let board = self.placed.is_board();
        let geometry = self.cache.borrow().draw(renderer, Size::new(w * lod, h * lod), |frame| {
            frame.scale(lod);
            frame.fill_rectangle(Point::ORIGIN, Size::new(w, h), theme::p().paper);
            if board {
                draw_pattern(frame, Background::Dots, w, h, spec.spacing.max(4.0), lod);
            } else if spec.pdf.is_none() {
                draw_pattern(frame, spec.background, w, h, spec.spacing.max(4.0), lod);
            }
        });
        self.lod.set(lod);
        geometry
    }

    /// The strokes as triangle meshes in page units (one per pencil pass), built on first use.
    fn ink_mesh(&self, zoom: f32, load: &Loader) -> mesh::Cache {
        let step = mesh_step(zoom);
        if let Some((s, cache)) = self.ink.borrow().as_ref()
            && *s == step
        {
            return cache.clone();
        }
        let content = self.content(load);
        let (w, h) = self.size();
        let cache = mesh::Cache::new(Arc::from(ink_meshes(&content.strokes, step, Rectangle::new(Point::ORIGIN, Size::new(w, h)))));
        *self.ink.borrow_mut() = Some((step, cache.clone()));
        cache
    }
}

/// Every how many samples the mesh keeps at a zoom (zoomed out, full resolution only costs).
pub fn mesh_step(zoom: f32) -> usize {
    match zoom {
        z if z >= 0.6 => 1,
        z if z >= 0.3 => 2,
        _ => 4,
    }
}

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

/// All strokes of a page as solid meshes (page units, colour per vertex). Pencils are three
/// translucent bands like on the tablet (without the grain).
pub fn ink_meshes(strokes: &[Stroke], step: usize, clip: Rectangle) -> Vec<Mesh> {
    let (mut xy, mut indices) = (Vec::new(), Vec::new());
    let mut vertices: Vec<SolidVertex2D> = Vec::new();
    let mut buf = Vec::new();
    let push = |points: &[ink::InkPoint], brush: &ink::Brush, color: Color, xy: &mut Vec<f32>, indices: &mut Vec<u32>, vertices: &mut Vec<SolidVertex2D>| {
        let start = xy.len();
        ink::outline_mesh(points, brush, xy, indices);
        let packed = color::pack(color);
        vertices.extend(xy[start..].chunks_exact(2).map(|v| SolidVertex2D { position: [v[0], v[1]], color: packed }));
    };
    for s in strokes {
        let points = thin(&s.points, step, &mut buf).to_vec();
        let base = argb(s.brush.color);
        if s.brush.kind == KIND_PENCIL {
            push(&points, &s.brush, Color { a: base.a * PENCIL_ALPHA[0], ..base }, &mut xy, &mut indices, &mut vertices);
            for (i, b) in ink::pencil_layer_brushes(&s.brush).iter().enumerate() {
                push(&points, b, Color { a: base.a * PENCIL_ALPHA[i + 1], ..base }, &mut xy, &mut indices, &mut vertices);
            }
        } else {
            push(&points, &s.brush, base, &mut xy, &mut indices, &mut vertices);
        }
    }
    if vertices.is_empty() {
        return Vec::new();
    }
    vec![Mesh::Solid { buffers: Indexed { vertices, indices }, transformation: Transformation::IDENTITY, clip_bounds: clip }]
}

/// A paper ruling in page units (the same spacing and margins as the tablet).
fn draw_pattern(frame: &mut canvas::Frame, bg: Background, w: f32, h: f32, step: f32, lod: f32) {
    let line = LineStroke::default().with_color(theme::PATTERN).with_width((1.0 / lod).max(0.5));
    let hline = |frame: &mut canvas::Frame, y: f32| frame.stroke(&Path::line(Point::new(0.0, y), Point::new(w, y)), line.clone());
    match bg {
        Background::Blank | Background::Text => {}
        Background::Lines => {
            let mut y = step * 2.0;
            while y <= h {
                hline(frame, y);
                y += step;
            }
        }
        Background::Cornell => {
            let mut y = 90.0;
            while y < h - 150.0 {
                hline(frame, y);
                y += step;
            }
            let margin = LineStroke::default().with_color(theme::alpha(theme::p().accent, 0.4)).with_width((1.5 / lod).max(0.75));
            frame.stroke(&Path::line(Point::new(150.0, 70.0), Point::new(150.0, h - 150.0)), margin.clone());
            frame.stroke(&Path::line(Point::new(0.0, h - 150.0), Point::new(w, h - 150.0)), margin.clone());
            frame.stroke(&Path::line(Point::new(0.0, 70.0), Point::new(w, 70.0)), margin);
        }
        Background::Grid => {
            let mut x = step;
            while x <= w {
                frame.stroke(&Path::line(Point::new(x, 0.0), Point::new(x, h)), line.clone());
                x += step;
            }
            let mut y = step;
            while y <= h {
                hline(frame, y);
                y += step;
            }
        }
        Background::Dots => {
            // Zoomed out: every 2nd/4th dot, like the tablet.
            let mut s = step;
            while s * lod < 8.0 {
                s *= 2.0;
            }
            let dots = Path::new(|b| {
                let mut y = 0.0;
                while y <= h {
                    let mut x = 0.0;
                    while x <= w {
                        b.circle(Point::new(x, y), (1.3 / lod).max(0.6));
                        x += s;
                    }
                    y += s;
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

/// A stroke another device is drawing right now (shown until the stored stroke arrives).
pub struct LiveInk {
    pub sheet_id: String,
    pub brush: ink::Brush,
    pub points: Vec<ink::InkPoint>,
    pub done: Option<std::time::Instant>,
}

/// The laid-out notebook.
pub struct Pages {
    /// Other devices' views (live session).
    pub remote: RefCell<Vec<RemoteMark>>,
    /// Strokes being drawn on other devices, by (device, stroke).
    pub live: RefCell<HashMap<(String, u64), LiveInk>>,
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

    /// Drops a sheet's strokes and meshes (it changed on disk or was edited).
    pub fn reload_sheet(&self, sheet_id: &str) {
        if let Some(p) = self.pages.iter().find(|p| p.placed.sheet_id == sheet_id) {
            *p.content.borrow_mut() = None;
            *p.ink.borrow_mut() = None;
        }
        // Finished live strokes are now stored ones.
        self.live.borrow_mut().retain(|_, l| !(l.sheet_id == sheet_id && l.done.is_some()));
    }

    /// New samples (packed LE f32 x, y, pressure, t) of a stroke another device is drawing.
    pub fn live_samples(&self, device: &str, stroke: u64, sheet_id: &str, brush: (f32, f32, u32, u8), samples: &[u8], done: bool) {
        let mut live = self.live.borrow_mut();
        let l = live.entry((device.to_string(), stroke)).or_insert_with(|| LiveInk {
            sheet_id: sheet_id.to_string(),
            brush: ink::Brush { width: brush.0, thinning: brush.1, color: brush.2, kind: brush.3 },
            points: Vec::new(),
            done: None,
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
                let text = if c.body.trim().is_empty() { "*Leere Textseite — anklicken zum Schreiben*".to_string() } else { c.body.clone() };
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

    /// Draws a PDF page's background: the thumbnail, sharp tiles of the visible part on top.
    fn draw_pdf(&self, r: &mut iced::Renderer, page: &Page, at: Point, cam: &Camera, view: &Rect, clip: Rectangle) {
        use iced::advanced::image::{FilterMethod, Image, Renderer as _};
        let (Some(pdf), Some((blob, index))) = (&self.pdf, &page.placed.spec.pdf) else { return };
        let rect = page.rect();
        let size = n0te_pdf::PageSize { width: rect.width(), height: rect.height() };
        let whole = Rect::new(0.0, 0.0, size.width, size.height);
        let visible = Rect::new(view.x0.max(rect.x0) - rect.x0, view.y0.max(rect.y0) - rect.y0, view.x1.min(rect.x1) - rect.x0, view.y1.min(rect.y1) - rect.y0);
        let mut levels = vec![pdf.tiles_for(blob, *index, size, pdf::THUMB, whole, n0te_pdf::worker::priority::BACKGROUND)];
        let b = pdf::bucket(cam.zoom);
        if b > pdf::THUMB && !visible.is_empty() {
            levels.push(pdf.tiles_for(blob, *index, size, b, visible, n0te_pdf::worker::priority::VISIBLE));
        }
        for tiles in levels {
            for (t, img) in tiles {
                if let Some(h) = img {
                    let bounds = Rectangle::new(Point::new(at.x + t.x0 * cam.zoom, at.y + t.y0 * cam.zoom), Size::new(t.width() * cam.zoom, t.height() * cam.zoom));
                    r.draw_image(Image::new(h).filter_method(FilterMethod::Linear), bounds, clip);
                }
            }
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
        let cam = Camera { x: 0.0, y: 0.0, zoom, viewport };
        if self.pages.is_empty() { cam } else { self.camera_for(0, &cam) }
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

    fn header_at(&self, p: Point) -> Option<&Header> {
        self.headers.iter().find(|h| h.rect.contains(p.x, p.y))
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
}

impl Default for Camera {
    fn default() -> Self {
        Camera { x: 0.0, y: 0.0, zoom: 1.0, viewport: Size::ZERO }
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
}

pub fn page_view<'a, Message>(pages: &'a Pages, camera: Camera, on_camera: impl Fn(Camera) -> Message + 'a, on_fold: impl Fn(String) -> Message + 'a) -> PageView<'a, Message> {
    PageView { pages, camera, on_camera: Box::new(on_camera), on_fold: Box::new(on_fold), on_double: None }
}

impl<'a, Message> PageView<'a, Message> {
    /// A double click on a page: (sheet id, x, y in the sheet's points) — a new text box there.
    pub fn on_double_click(mut self, f: impl Fn(String, f32, f32) -> Message + 'a) -> Self {
        self.on_double = Some(Box::new(f));
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
}

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
        let cam = match state.pending {
            Some((id, seen, published)) if id == self.pages.id && seen == self.camera => published,
            _ => self.camera,
        };
        // Tell the app our size (first frame, resizes); it owns the camera.
        if cam.viewport != bounds.size() {
            let next = if cam.viewport.width <= 0.0 { self.pages.initial_camera(bounds.size()) } else { self.pages.clamp(Camera { viewport: bounds.size(), ..cam }) };
            state.pending = Some((self.pages.id, self.camera, next));
            shell.publish((self.on_camera)(next));
            return;
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
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left | mouse::Button::Middle)) => {
                if let Some((start, _, false)) = state.drag.take() {
                    let at = cam.to_page(bounds, start);
                    let double = state.last_click.is_some_and(|(t, p)| t.elapsed().as_millis() < 400 && p.distance(at) * cam.zoom < 8.0);
                    state.last_click = if double { None } else { Some((std::time::Instant::now(), at)) };
                    if let Some(h) = self.pages.header_at(at) {
                        // A click on a group title folds/unfolds the group.
                        fold = Some(h.node.clone());
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
        renderer.fill_quad(renderer::Quad { bounds, ..Default::default() }, theme::p().surface);
        if cam.viewport.width <= 0.0 {
            return;
        }
        let lod = lod_for(cam.zoom);
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
        // Three layers: paper (+ PDF images), then ink, then labels — within one layer iced
        // draws images after meshes, which would put the PDF over the ink.
        renderer.with_layer(bounds, |r| {
            for page in &visible {
                page.last_frame.set(frame_no);
                let geometry = page.background(r, lod);
                let rect = page.rect();
                let at = cam.to_screen(bounds, rect.x0, rect.y0);
                r.with_translation(Vector::new(at.x, at.y), |r| {
                    r.with_transformation(Transformation::scale(cam.zoom / lod), |r| r.draw_geometry(geometry));
                });
                pages.draw_pdf(r, page, at, &cam, &view, bounds);
            }
        });
        renderer.with_layer(bounds, |r| {
            for l in pages.live.borrow().values() {
                let Some(page) = visible.iter().find(|p| p.placed.sheet_id == l.sheet_id) else { continue };
                let rect = page.rect();
                let stroke = Stroke { id: String::new(), brush: l.brush, bounds: Rect::EMPTY, points: l.points.clone() };
                let meshes = ink_meshes(std::slice::from_ref(&stroke), 1, Rectangle::new(Point::ORIGIN, Size::new(rect.width(), rect.height())));
                if meshes.is_empty() {
                    continue;
                }
                let at = cam.to_screen(bounds, rect.x0, rect.y0);
                let cache = mesh::Cache::new(Arc::from(meshes));
                r.with_translation(Vector::new(at.x, at.y), |r| {
                    r.with_transformation(Transformation::scale(cam.zoom), |r| r.draw_mesh_cache(cache));
                });
            }
            for page in &visible {
                let ink = page.ink_mesh(cam.zoom, &pages.load);
                let rect = page.rect();
                let at = cam.to_screen(bounds, rect.x0, rect.y0);
                r.with_translation(Vector::new(at.x, at.y), |r| {
                    r.with_transformation(Transformation::scale(cam.zoom), |r| r.draw_mesh_cache(ink));
                });
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
                        label(r, format!("{:02}", p.number), Point::new(at.x - 46.0, at.y), 40.0, color, 11.0, theme::MONO, text::Alignment::Right);
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
                let title = if h.title.trim().is_empty() { "Gruppe".to_string() } else { h.title.clone() };
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
                r.fill_quad(
                    renderer::Quad { bounds: frame, border: iced::Border { color: theme::alpha(theme::p().accent, 0.8), width: 2.0, radius: 4.0.into() }, ..Default::default() },
                    Color::TRANSPARENT,
                );
                label(r, m.name.to_uppercase(), Point::new(frame.x + 4.0, frame.y + 4.0), 200.0, theme::p().accent, 10.0, theme::MONO_BOLD, text::Alignment::Left);
                if let Some((x, y)) = m.laser {
                    let c = cam.to_screen(bounds, o.x0 + x, o.y0 + y);
                    r.fill_quad(
                        renderer::Quad { bounds: Rectangle::new(Point::new(c.x - 7.0, c.y - 7.0), Size::new(14.0, 14.0)), border: iced::Border { radius: 7.0.into(), ..Default::default() }, ..Default::default() },
                        Color::from_rgba8(255, 59, 48, 0.9),
                    );
                }
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

/// The sheet spec's paper as a short code (outline sidebar).
pub fn paper_code(spec: &SheetSpec) -> &'static str {
    if spec.is_board() {
        return "BRD";
    }
    if spec.pdf.is_some() {
        return "PDF";
    }
    match spec.background {
        Background::Blank => "LEER",
        Background::Lines => "LIN",
        Background::Grid => "KAR",
        Background::Dots => "PKT",
        Background::Cornell => "COR",
        Background::Text => "TXT",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
