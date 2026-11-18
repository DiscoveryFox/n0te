//! The page canvas: a notebook's main column (with side pages left/right) laid out in one 2D
//! space, drawn by a custom widget with pan (scroll / trackpad / middle-drag) and zoom
//! (ctrl+scroll around the cursor).
//!
//! Each page's background and strokes are tessellated once into a `canvas::Cache` at a
//! power-of-two level of detail (LOD) and then drawn with a GPU transformation, so panning and
//! zooming within one LOD never rebuild paths. Caches of pages that have not been drawn for a
//! while are dropped.

use std::cell::{Cell, RefCell};
use std::time::Instant;

use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::text::{self, Renderer as _};
use iced::advanced::widget::{self, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::keyboard;
use iced::widget::canvas::{self, Fill, Path, fill};
use iced::{Color, Element, Event, Length, Point, Rectangle, Size, Theme, Transformation, Vector};

use n0te_core::ink::{self, Rect};
use n0te_core::notebook::{Background, Notebook, OutlineRow, SheetSpec, Side};
use n0te_core::sheet::{Sheet, Stroke};

use crate::theme;

/// Gap between main pages (vertical) and between side pages (horizontal), in page units.
pub const GAP_Y: f32 = 44.0;
pub const GAP_X: f32 = 36.0;
pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 8.0;
/// Keep at most this many page caches alive (LRU by last frame drawn).
pub const MAX_CACHED: usize = 24;

pub struct Page {
    pub sheet_id: String,
    pub spec: SheetSpec,
    /// 1-based number of the main page (side pages carry their main page's number).
    pub number: usize,
    pub side: Option<(Side, usize)>,
    /// Top-left corner in notebook space (page units).
    pub origin: Point,
    pub strokes: Vec<Stroke>,
    pub samples: usize,
    cache: RefCell<canvas::Cache>,
    lod: Cell<f32>,
    last_frame: Cell<u64>,
}

impl Page {
    pub fn rect(&self) -> Rect {
        Rect::new(self.origin.x, self.origin.y, self.origin.x + self.spec.width, self.origin.y + self.spec.height)
    }

    pub fn is_cached(&self) -> bool {
        self.lod.get() > 0.0
    }

    /// Drops the tessellated geometry (frees its memory; `Cache::clear` would keep it).
    pub fn evict(&self) {
        *self.cache.borrow_mut() = canvas::Cache::new();
        self.lod.set(0.0);
    }

    /// The page's geometry at `lod` (page units scaled by `lod`), built on first use.
    /// Returns the geometry and, if it had to be built, how long that took.
    pub fn geometry(&self, renderer: &iced::Renderer, lod: f32) -> (canvas::Geometry, Option<f32>) {
        let size = Size::new(self.spec.width * lod, self.spec.height * lod);
        let started = Instant::now();
        let built = Cell::new(false);
        let geometry = self.cache.borrow().draw(renderer, size, |frame| {
            built.set(true);
            frame.scale(lod);
            draw_page(frame, &self.spec, &self.strokes);
        });
        self.lod.set(lod);
        (geometry, built.get().then(|| started.elapsed().as_secs_f32() * 1000.0))
    }
}

/// Draws a page in page units: paper, pattern, strokes (filled outlines, non-zero rule).
pub fn draw_page(frame: &mut canvas::Frame, spec: &SheetSpec, strokes: &[Stroke]) {
    let (w, h) = (spec.width, spec.height);
    frame.fill_rectangle(Point::ORIGIN, Size::new(w, h), theme::PAPER);
    draw_background(frame, spec);
    for s in strokes {
        let poly = ink::outline(&s.points, &s.brush);
        if let Some(path) = polygon(&poly) {
            frame.fill(&path, Fill { style: canvas::Style::Solid(argb(s.brush.color)), rule: fill::Rule::NonZero });
        }
    }
}

pub fn polygon(poly: &[f32]) -> Option<Path> {
    if poly.len() < 6 {
        return None;
    }
    Some(Path::new(|b| {
        b.move_to(Point::new(poly[0], poly[1]));
        for xy in poly[2..].chunks_exact(2) {
            b.line_to(Point::new(xy[0], xy[1]));
        }
        b.close();
    }))
}

fn draw_background(frame: &mut canvas::Frame, spec: &SheetSpec) {
    let (w, h, s) = (spec.width, spec.height, spec.spacing.max(4.0));
    let line = 0.6;
    match spec.background {
        Background::Blank => {}
        Background::Lines | Background::Cornell => {
            let mut y = 64.0 + s * 2.0;
            while y < h - 24.0 {
                frame.fill_rectangle(Point::new(0.0, y), Size::new(w, line), theme::PATTERN);
                y += s;
            }
            if spec.background == Background::Cornell {
                frame.fill_rectangle(Point::new(140.0, 40.0), Size::new(0.8, h - 200.0), theme::PATTERN);
                frame.fill_rectangle(Point::new(0.0, h - 160.0), Size::new(w, 0.8), theme::PATTERN);
            } else {
                // Margin line in a pale accent.
                frame.fill_rectangle(Point::new(44.0, 0.0), Size::new(0.6, h), theme::alpha(theme::ACCENT, 0.25));
            }
        }
        Background::Grid => {
            let (ox, oy) = ((w % s) / 2.0, (h % s) / 2.0);
            let mut x = ox;
            while x <= w {
                frame.fill_rectangle(Point::new(x, 0.0), Size::new(0.5, h), theme::PATTERN);
                x += s;
            }
            let mut y = oy;
            while y <= h {
                frame.fill_rectangle(Point::new(0.0, y), Size::new(w, 0.5), theme::PATTERN);
                y += s;
            }
        }
        Background::Dots => {
            let (ox, oy) = ((w % s) / 2.0, (h % s) / 2.0);
            let dots = Path::new(|b| {
                let mut y = oy;
                while y <= h {
                    let mut x = ox;
                    while x <= w {
                        b.circle(Point::new(x, y), 0.9);
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

/// All pages of a notebook in reading order, laid out in notebook space.
pub struct Pages {
    pub pages: Vec<Page>,
    /// Index into `pages` of each main page, in order.
    pub mains: Vec<usize>,
    pub bounds: Rect,
    frame: Cell<u64>,
    pub stats: Stats,
}

#[derive(Default)]
pub struct Stats {
    /// Builds (tessellations) so far, and the time of the most recent one.
    pub builds: Cell<u32>,
    pub last_build_ms: Cell<f32>,
    pub max_build_ms: Cell<f32>,
    /// CPU time of the last `draw` call of the page view (including any builds).
    pub last_draw_ms: Cell<f32>,
    pub visible_pages: Cell<u32>,
    pub visible_strokes: Cell<u32>,
    pub evictions: Cell<u32>,
}

impl Pages {
    /// Lays out the notebook's main sheets (all groups expanded) with their side sheets.
    /// `sheets` resolves a sheet id to its document.
    pub fn new<'s>(notebook: &Notebook, mut sheet: impl FnMut(&str) -> Option<&'s Sheet>) -> Self {
        let mut pages = Vec::new();
        let mut mains = Vec::new();
        let mut y = 0.0f32;
        let mut bounds = Rect::EMPTY;
        let mut load = |sheet_id: &str, spec: &SheetSpec, number: usize, side: Option<(Side, usize)>, origin: Point| {
            let strokes = sheet(sheet_id).map(|s| s.strokes()).unwrap_or_default();
            let samples = strokes.iter().map(|s| s.points.len()).sum();
            Page {
                sheet_id: sheet_id.to_string(),
                spec: spec.clone(),
                number,
                side,
                origin,
                strokes,
                samples,
                cache: RefCell::new(canvas::Cache::new()),
                lod: Cell::new(0.0),
                last_frame: Cell::new(0),
            }
        };
        for row in notebook.outline(true) {
            let OutlineRow::Sheet { sheet_id, spec, left, right, .. } = row else { continue };
            if spec.is_board() {
                continue; // boards are not part of the paged column in this spike
            }
            let number = mains.len() + 1;
            mains.push(pages.len());
            let main = load(&sheet_id, &spec, number, None, Point::new(0.0, y));
            bounds = bounds.include(0.0, y).include(spec.width, y + spec.height);
            pages.push(main);
            let mut x = spec.width + GAP_X;
            for (i, s) in right.iter().enumerate() {
                pages.push(load(&s.sheet_id, &s.spec, number, Some((Side::Right, i)), Point::new(x, y)));
                bounds = bounds.include(x + s.spec.width, y + s.spec.height);
                x += s.spec.width + GAP_X;
            }
            let mut x = -GAP_X;
            for (i, s) in left.iter().enumerate() {
                x -= s.spec.width;
                pages.push(load(&s.sheet_id, &s.spec, number, Some((Side::Left, i)), Point::new(x, y)));
                bounds = bounds.include(x, y);
                x -= GAP_X;
            }
            y += spec.height + GAP_Y;
        }
        Pages { pages, mains, bounds, frame: Cell::new(0), stats: Stats::default() }
    }

    pub fn stroke_count(&self) -> usize {
        self.pages.iter().map(|p| p.strokes.len()).sum()
    }

    pub fn cached_pages(&self) -> usize {
        self.pages.iter().filter(|p| p.is_cached()).count()
    }

    pub fn page_of_sheet(&self, sheet_id: &str) -> Option<usize> {
        self.pages.iter().position(|p| p.sheet_id == sheet_id)
    }

    /// The main page (1-based number) under the vertical centre of the viewport.
    pub fn current_number(&self, camera: &Camera) -> usize {
        let cy = camera.y + camera.viewport.height / camera.zoom / 2.0;
        let mut best = 1;
        for &i in &self.mains {
            let p = &self.pages[i];
            if p.origin.y - GAP_Y / 2.0 <= cy {
                best = p.number;
            }
        }
        best
    }

    /// Camera that shows page `index` at the top: centred on the page, or on the page and its
    /// first right side page when that fits.
    pub fn camera_for(&self, index: usize, camera: &Camera) -> Camera {
        let p = &self.pages[index];
        let view_w = camera.viewport.width / camera.zoom;
        let (mut x0, mut x1) = (p.origin.x, p.origin.x + p.spec.width);
        if let Some(side) = self.pages.get(index + 1).filter(|s| s.side == Some((Side::Right, 0)) && s.number == p.number)
            && side.origin.x + side.spec.width - x0 <= view_w - 48.0 / camera.zoom
        {
            x1 = side.origin.x + side.spec.width;
        }
        if x1 - x0 > view_w {
            x0 = p.origin.x; // too wide: keep the page's left edge in view
            x1 = x0 + view_w - 48.0 / camera.zoom;
        }
        let cx = (x0 + x1) / 2.0;
        self.clamp(Camera { x: cx - view_w / 2.0, y: p.origin.y - 28.0 / camera.zoom, ..*camera })
    }

    /// Initial camera: the first page at 100 % (or smaller if the viewport is narrow).
    pub fn initial_camera(&self, viewport: Size) -> Camera {
        let w = self.pages.first().map(|p| p.spec.width).unwrap_or(595.0);
        let zoom = (viewport.width * 0.8 / w).clamp(0.25, 1.0);
        let cam = Camera { x: 0.0, y: 0.0, zoom, viewport };
        if self.pages.is_empty() { cam } else { self.camera_for(0, &cam) }
    }

    /// Keeps the content reachable: at most half a viewport of empty space around it.
    pub fn clamp(&self, mut c: Camera) -> Camera {
        if self.bounds.is_empty() || c.viewport.width <= 0.0 {
            return c;
        }
        c.zoom = c.zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        let (vw, vh) = (c.viewport.width / c.zoom, c.viewport.height / c.zoom);
        let b = &self.bounds;
        let (minx, maxx) = (b.x0 - vw * 0.5, b.x1 - vw * 0.5);
        let (miny, maxy) = (b.y0 - vh * 0.5, b.y1 - vh * 0.5);
        c.x = c.x.clamp(minx.min(maxx), maxx.max(minx));
        c.y = c.y.clamp(miny.min(maxy), maxy.max(miny));
        c
    }

    fn visible(&self, view: Rect) -> impl Iterator<Item = &Page> {
        self.pages.iter().filter(move |p| p.rect().intersects(&view))
    }

    /// Drops caches of pages not drawn recently when more than [`MAX_CACHED`] are alive.
    fn evict_stale(&self) {
        let cached: Vec<&Page> = self.pages.iter().filter(|p| p.is_cached()).collect();
        if cached.len() <= MAX_CACHED {
            return;
        }
        let mut by_age: Vec<&Page> = cached;
        by_age.sort_by_key(|p| p.last_frame.get());
        let now = self.frame.get();
        for p in by_age.iter().take(by_age.len() - MAX_CACHED) {
            if p.last_frame.get() + 2 < now {
                p.evict();
                self.stats.evictions.set(self.stats.evictions.get() + 1);
            }
        }
    }
}

/// Level of detail for a zoom: the next power of two at or above it, so cached geometry is
/// only ever scaled down (by at most 2x) on the GPU.
pub fn lod_for(zoom: f32) -> f32 {
    2f32.powf(zoom.max(1e-3).log2().ceil()).clamp(0.125, 8.0)
}

/// What part of notebook space the view shows. `x`/`y` is the page-space point at the
/// top-left of the viewport; one page unit is `zoom` logical pixels.
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
    pub fn to_screen(&self, bounds: Rectangle, p: Point) -> Point {
        Point::new(bounds.x + (p.x - self.x) * self.zoom, bounds.y + (p.y - self.y) * self.zoom)
    }
}

/// The page view widget.
pub struct PageView<'a, Message> {
    pages: &'a Pages,
    camera: Camera,
    on_camera: Box<dyn Fn(Camera) -> Message + 'a>,
}

pub fn page_view<'a, Message>(pages: &'a Pages, camera: Camera, on_camera: impl Fn(Camera) -> Message + 'a) -> PageView<'a, Message> {
    PageView { pages, camera, on_camera: Box::new(on_camera) }
}

#[derive(Default)]
struct State {
    drag: Option<Point>,
    modifiers: keyboard::Modifiers,
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
        let cam = self.camera;
        // Tell the app our size (first frame, resizes); it owns the camera.
        if cam.viewport != bounds.size() {
            let next = if cam.viewport.width <= 0.0 {
                self.pages.initial_camera(bounds.size())
            } else {
                self.pages.clamp(Camera { viewport: bounds.size(), ..cam })
            };
            shell.publish((self.on_camera)(next));
            return;
        }
        let publish = |shell: &mut Shell<'_, Message>, c: Camera| {
            let c = self.pages.clamp(c);
            if c != cam {
                shell.publish((self.on_camera)(c));
            }
            shell.capture_event();
        };
        match event {
            Event::Keyboard(keyboard::Event::ModifiersChanged(m)) => state.modifiers = *m,
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let Some(pos) = cursor.position_in(bounds) else { return };
                let (dx, dy) = match *delta {
                    mouse::ScrollDelta::Lines { x, y } => (x * LINE_PX, y * LINE_PX),
                    mouse::ScrollDelta::Pixels { x, y } => (x, y),
                };
                if state.modifiers.command() {
                    // Zoom around the cursor.
                    let factor = match *delta {
                        mouse::ScrollDelta::Lines { y, .. } => 1.15f32.powf(y),
                        mouse::ScrollDelta::Pixels { y, .. } => (y * 0.004).exp(),
                    };
                    let zoom = (cam.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
                    let anchor = Point::new(cam.x + pos.x / cam.zoom, cam.y + pos.y / cam.zoom);
                    publish(shell, Camera { x: anchor.x - pos.x / zoom, y: anchor.y - pos.y / zoom, zoom, ..cam });
                } else {
                    let (dx, dy) = if state.modifiers.shift() && dx == 0.0 { (dy, 0.0) } else { (dx, dy) };
                    publish(shell, Camera { x: cam.x - dx / cam.zoom, y: cam.y - dy / cam.zoom, ..cam });
                }
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Middle)) => {
                if let Some(p) = cursor.position_over(bounds) {
                    state.drag = Some(p);
                    shell.capture_event();
                }
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                if let Some(last) = state.drag {
                    state.drag = Some(*position);
                    let d = *position - last;
                    publish(shell, Camera { x: cam.x - d.x / cam.zoom, y: cam.y - d.y / cam.zoom, ..cam });
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Middle)) => state.drag = None,
            _ => {}
        }
    }

    fn mouse_interaction(&self, tree: &widget::Tree, layout: Layout<'_>, cursor: mouse::Cursor, _viewport: &Rectangle, _renderer: &iced::Renderer) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        if state.drag.is_some() {
            mouse::Interaction::Grabbing
        } else if cursor.is_over(layout.bounds()) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
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
        let started = Instant::now();
        let bounds = layout.bounds();
        let pages = self.pages;
        let cam = self.camera;
        let frame_no = pages.frame.get() + 1;
        pages.frame.set(frame_no);
        renderer.fill_quad(renderer::Quad { bounds, ..Default::default() }, theme::SURFACE);
        if cam.viewport.width <= 0.0 {
            return;
        }
        let lod = lod_for(cam.zoom);
        let current = pages.current_number(&cam);
        let (mut n_pages, mut n_strokes) = (0, 0);
        renderer.with_layer(bounds, |r| {
            for page in pages.visible(cam.view_rect()) {
                n_pages += 1;
                n_strokes += page.strokes.len() as u32;
                page.last_frame.set(frame_no);
                let (geometry, built) = page.geometry(r, lod);
                if let Some(ms) = built {
                    let s = &pages.stats;
                    s.builds.set(s.builds.get() + 1);
                    s.last_build_ms.set(ms);
                    s.max_build_ms.set(s.max_build_ms.get().max(ms));
                }
                let at = cam.to_screen(bounds, page.origin);
                r.with_translation(Vector::new(at.x, at.y), |r| {
                    r.with_transformation(Transformation::scale(cam.zoom / lod), |r| r.draw_geometry(geometry));
                });
                // Page number under main pages; a small side marker under side pages.
                let label = match page.side {
                    None => format!("{:02}", page.number),
                    Some((Side::Right, i)) => format!("{:02} › {}", page.number, i + 1),
                    Some((Side::Left, i)) => format!("{} ‹ {:02}", i + 1, page.number),
                };
                let color = if page.side.is_none() && page.number == current { theme::ACCENT } else { theme::MUTED };
                let size = 11.0;
                let below = Point::new(at.x, at.y + page.spec.height * cam.zoom + (GAP_Y * cam.zoom - size) / 2.0 - 1.0);
                if GAP_Y * cam.zoom >= 20.0 {
                    r.fill_text(
                        text::Text {
                            content: label,
                            bounds: Size::new(page.spec.width * cam.zoom, 16.0),
                            size: size.into(),
                            line_height: text::LineHeight::default(),
                            font: theme::MONO,
                            align_x: text::Alignment::Left,
                            align_y: iced::alignment::Vertical::Top,
                            shaping: text::Shaping::Advanced,
                            wrapping: text::Wrapping::None,
                        },
                        below,
                        color,
                        bounds,
                    );
                }
            }
        });
        pages.evict_stale();
        let s = &pages.stats;
        s.visible_pages.set(n_pages);
        s.visible_strokes.set(n_strokes);
        s.last_draw_ms.set(started.elapsed().as_secs_f32() * 1000.0);
    }
}

impl<'a, Message: 'a> From<PageView<'a, Message>> for Element<'a, Message> {
    fn from(v: PageView<'a, Message>) -> Self {
        Element::new(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lod_is_next_power_of_two() {
        assert_eq!(lod_for(1.0), 1.0);
        assert_eq!(lod_for(1.01), 2.0);
        assert_eq!(lod_for(0.6), 1.0);
        assert_eq!(lod_for(0.5), 0.5);
        assert_eq!(lod_for(3.0), 4.0);
        assert_eq!(lod_for(100.0), 8.0);
    }

    #[test]
    fn argb_unpacks() {
        let c = argb(0xFFD71921);
        assert!((c.r - 215.0 / 255.0).abs() < 1e-6 && (c.a - 1.0).abs() < 1e-6);
    }
}
