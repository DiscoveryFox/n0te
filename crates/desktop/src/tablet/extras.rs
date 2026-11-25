//! n0te Tablet's helpers beyond the pens, as on the Android tablet: the ruler, space put into
//! a page (down or sideways), the writing magnifier ("Schreiblupe"), scribbling to delete, the
//! "+" beside the current page row and boards on their own.

use std::time::{Duration, Instant};

use iced::{Point, Rectangle, Size};
use n0te_core::ink::{InkPoint, Rect};
use n0te_core::layout::Placed;
use n0te_core::notebook::{Axis, Background, Below, Side, SheetSpec, SpacePlan, SpacePlanX};

use super::tools::{self, HandleDir, LOUPE_ADVANCE, LOUPE_BASELINE, LOUPE_SHARE};
use super::{Drawing, Edit, LOCAL, Tool};
use crate::app::{App, Screen};
use crate::i18n::{tr, trf};
use crate::pages::{Camera, SpacePreview};
use crate::split::PaneKey;

/// A page's shape: what space going in or out changes (and undo puts back).
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub width: f32,
    pub height: f32,
    pub gaps: Vec<(f32, f32)>,
}

impl Shape {
    fn of(spec: &SheetSpec) -> Shape {
        Shape { width: spec.width, height: spec.height, gaps: spec.gaps.clone() }
    }
}

/// The writing magnifier: a strip along the bottom of its pane shows a frame of one page large;
/// writing in the strip lands in the frame. Lifting the pen in the strip's right part moves the
/// frame on (at the page's right edge: the next line, back to where the line began).
#[derive(Debug, Clone)]
pub struct Loupe {
    pub pane: PaneKey,
    pub notebook: String,
    pub sheet: String,
    /// The frame on the page (sheet points: top-left and width; its height follows the strip).
    pub x: f32,
    pub y: f32,
    pub w: f32,
    /// Where the frame's lines begin (a new line goes back there).
    pub line_x: f32,
    /// The ink written on the frame's line so far (top, bottom).
    pub ink: Option<(f32, f32)>,
    /// Where the last stroke in the strip ended (sheet x): past the middle the move-on part
    /// shows stronger.
    pub last_x: Option<f32>,
    /// The frame moves on then (a stroke lifted in the strip's right part).
    pub advance_at: Option<Instant>,
    /// The pen holding the frame's corner (resizing it): where it was last (window).
    corner: Option<Point>,
}

/// Space being dragged into a page: the line where the pen went down, what lies beyond it, and
/// what the drag asks for so far.
#[derive(Debug, Clone)]
pub struct SpaceDrag {
    pane: PaneKey,
    notebook: String,
    page: Placed,
    /// Where the pen went down: on the sheet, and on the pages.
    at: (f32, f32),
    down: Point,
    below: Below,
    right: Below,
    step: f32,
    step_x: f32,
    /// Which way, from where the pen first goes: None until it moved a little.
    pub sideways: Option<bool>,
    plan: Option<SpacePlan>,
    plan_x: Option<SpacePlanX>,
    asked: f32,
}

impl SpaceDrag {
    /// How far what lies beyond moves, and how much longer (wider) the page gets.
    fn amount(&self) -> (f32, f32) {
        match self.sideways {
            Some(true) => self.plan_x.as_ref().map_or((0.0, 0.0), |p| (p.dx, p.width - self.page.spec.width)),
            Some(false) => self.plan.as_ref().map_or((0.0, 0.0), |p| (p.dy, p.height - self.page.spec.height)),
            None => (0.0, 0.0),
        }
    }

    /// What the drag does, told on the page.
    pub fn label(&self) -> String {
        let (d, grow) = self.amount();
        let sideways = self.sideways == Some(true);
        tools::space_label(self.page.spec.background, sideways, d, if sideways { self.step_x } else { self.step }, grow, self.asked)
    }

    /// For the view: the page, the line (sheet points), the way and the amount.
    pub fn marks(&self) -> (&Placed, (f32, f32), Option<bool>, f32) {
        (&self.page, self.at, self.sideways, self.amount().0)
    }

    pub fn pane(&self) -> PaneKey {
        self.pane
    }

    fn preview(&self) -> SpacePreview {
        let spec = &self.page.spec;
        let (axis, d, size, gaps) = match (self.sideways, &self.plan, &self.plan_x) {
            (Some(false), Some(p), _) => (Some(Axis::Down), p.dy, (spec.width, p.height), p.gaps.clone()),
            (Some(true), _, Some(p)) => (Some(Axis::Right), p.dx, (p.width, spec.height), spec.gaps.clone()),
            _ => (None, 0.0, (spec.width, spec.height), spec.gaps.clone()),
        };
        SpacePreview { sheet_id: self.page.sheet_id.clone(), at: self.at, axis, d, size, gaps }
    }
}

/// A "+" being tapped or dragged: which, where the pen went down (window) and how far it moved
/// outwards (px).
#[derive(Debug, Clone)]
pub struct HandleDrag {
    pub pane: PaneKey,
    pub handle: tools::Handle,
    down: Point,
    pub drag: f32,
}

/// A "+" dragged back this far (px) adds nothing.
const HANDLE_CANCEL: f32 = 40.0;
/// How long after a stroke lifted in the strip's right part the frame moves on.
const LOUPE_DELAY: Duration = Duration::from_millis(450);

impl App {
    fn pane_ref(&self, key: PaneKey) -> Option<&crate::app::Open> {
        match &self.screen {
            Screen::Notebook(s) => s.pane(key),
            Screen::Library => None,
        }
    }

    fn pane_mut_of(&mut self, key: PaneKey) -> Option<&mut crate::app::Open> {
        match &mut self.screen {
            Screen::Notebook(s) => s.pane_mut(key),
            Screen::Library => None,
        }
    }

    fn active_key(&self) -> Option<PaneKey> {
        match &self.screen {
            Screen::Notebook(s) => Some(s.active),
            Screen::Library => None,
        }
    }

    // --- the ruler -------------------------------------------------------------------------

    /// The ruler on (in the middle of the active pane's view, level) or off.
    pub(crate) fn set_ruler(&mut self, on: bool) {
        self.tablet.ruler = None;
        if !on {
            return;
        }
        let Some(key) = self.active_key() else { return };
        let Some(o) = self.pane_ref(key) else { return };
        let v = o.camera.view_rect();
        self.tablet.ruler = Some((key, tools::Ruler { cx: (v.x0 + v.x1) / 2.0, cy: (v.y0 + v.y1) / 2.0, angle: 0.0 }));
    }

    // --- boards on their own ----------------------------------------------------------------

    /// The board in the middle of the active pane's view on its own (endless paper, a little map
    /// while it moves), or back to the pages.
    pub(crate) fn set_board(&mut self, on: bool) {
        let Some(key) = self.active_key() else { return };
        self.tablet.selection = None;
        if on {
            self.set_loupe(false);
        }
        let Some(o) = self.pane_mut_of(key) else { return };
        if !on {
            if let Some(c) = o.pages.exit_board(o.camera) {
                o.camera = c;
            }
            return;
        }
        let current = o.pages.current_number(&o.camera);
        let Some(sheet) = o.pages.index_of_number(current).map(|i| &o.pages.pages[i].placed).filter(|p| p.is_board()).map(|p| p.sheet_id.clone()) else { return };
        if let Some(c) = o.pages.enter_board(&sheet, o.camera) {
            o.camera = c;
        }
    }

    // --- the "+" beside the current row ------------------------------------------------------

    /// The "+" shown in pane `key` (none on a board on its own, for guests who only read, or
    /// while the pen does something else).
    pub(crate) fn handles_in(&self, key: PaneKey) -> Vec<tools::Handle> {
        let Some(o) = self.pane_ref(key) else { return Vec::new() };
        if o.pages.board.is_some() || self.read_only_nb(&o.id) || (self.tablet.busy() && self.tablet.handle.is_none()) || o.text_page.as_ref().is_some_and(|t| !t.docked) {
            return Vec::new();
        }
        let g = self.settings.layout();
        tools::handles(&o.pages, &o.camera, g.side_gap, g.row_gap)
    }

    /// The pen down at window point `p`: on a "+", whose drag starts.
    pub(crate) fn handle_down(&mut self, key: PaneKey, cam: &Camera, p: Point) -> bool {
        let Some(h) = tools::handle_at(&self.handles_in(key), cam, p) else { return false };
        self.tablet.handle = Some(HandleDrag { pane: key, handle: h, down: p, drag: 0.0 });
        true
    }

    /// The page a "+" adds: below the row, or a side page at its left or right end; the view
    /// goes to it. Its paper: the current page's (not a PDF page's, a board's or a text page's:
    /// then the paper new pages get).
    fn add_page_at(&mut self, key: PaneKey, h: &tools::Handle) {
        let Some(o) = self.pane_ref(key) else { return };
        let like = o.pages.pages.iter().map(|p| &p.placed).find(|p| p.node == h.main_node).map(|p| p.spec.clone());
        let spec = match like {
            Some(s) if s.pdf.is_none() && !s.is_board() && s.background != Background::Text => SheetSpec { gaps: Vec::new(), ..s },
            _ => SheetSpec { width: self.settings.new_w, height: self.settings.new_h, background: self.new_paper, spacing: self.new_spacing, pdf: None, gaps: Vec::new() },
        };
        let (parent, index) = crate::pdf::position_after(&o.nb, Some(&h.main_node));
        let mut made = None;
        let (dir, main) = (h.dir, h.main_node.clone());
        self.in_pane(key, |app| {
            app.edit_notebook(|nb| {
                made = Some(match dir {
                    HandleDir::Below => nb.add_sheet(parent.as_deref(), index, &spec)?.1,
                    HandleDir::Left => nb.add_side_sheet(&main, Side::Left, usize::MAX, &spec)?.1,
                    HandleDir::Right => nb.add_side_sheet(&main, Side::Right, usize::MAX, &spec)?.1,
                });
                Ok(())
            })
        });
        if let Some(sheet) = made
            && let Some(o) = self.pane_mut_of(key)
            && let Some(i) = o.pages.page_of_sheet(&sheet)
        {
            o.camera = o.pages.camera_for(i, &o.camera);
        }
    }

    // --- space -------------------------------------------------------------------------------

    /// The space tool down at page point `at` on `page`: the line goes across there (or from
    /// top to bottom, if the pen then goes sideways).
    pub(crate) fn space_down(&mut self, key: PaneKey, page: &Placed, at: Point) {
        if page.is_board() {
            self.say(tr("Auf dem Board ist überall Platz"));
            return;
        }
        if page.spec.background == Background::Text {
            self.say(tr("Auf Textseiten fließt der Text: dort Leerzeilen tippen"));
            return;
        }
        let Some(o) = self.pane_ref(key) else { return };
        let notebook = o.id.clone();
        let local = (at.x - page.rect.x0, at.y - page.rect.y0);
        let Some(sheet) = self.tablet_sheet(&notebook, &page.sheet_id) else { return };
        let (below, right) = (sheet.below(local.1), sheet.beyond(Axis::Right, local.0));
        let (step, step_x) = tools::space_steps(page);
        let sd = SpaceDrag { pane: key, notebook, page: page.clone(), at: local, down: at, below, right, step, step_x, sideways: None, plan: None, plan_x: None, asked: 0.0 };
        self.show_space(&sd);
        self.tablet.space = Some(sd);
    }

    fn show_space(&self, sd: &SpaceDrag) {
        if let Some(o) = self.pane_ref(sd.pane) {
            *o.pages.space.borrow_mut() = Some(sd.preview());
        }
    }

    /// The space drag ends: the space goes in (or out) for good, as one step to undo; or not.
    pub(crate) fn end_space(&mut self, sd: SpaceDrag, commit: bool) {
        if let Some(o) = self.pane_ref(sd.pane) {
            *o.pages.space.borrow_mut() = None;
        }
        if !commit {
            return;
        }
        let pdf = sd.page.spec.pdf.is_some();
        let (d, grow) = sd.amount();
        let text = sd.label();
        let before = Shape::of(&sd.page.spec);
        let (axis, at, after) = match (sd.sideways, &sd.plan, &sd.plan_x) {
            (Some(false), Some(p), _) => (Axis::Down, sd.at.1, Shape { height: p.height, gaps: p.gaps.clone(), ..before.clone() }),
            (Some(true), _, Some(p)) => (Axis::Right, sd.at.0, Shape { width: p.width, ..before.clone() }),
            _ => return,
        };
        if d == 0.0 && grow == 0.0 && after == before {
            if sd.asked < 0.0 {
                self.say(match axis {
                    Axis::Down if pdf => tr("Hier ist kein eingefügter Platz zum Entfernen"),
                    Axis::Down => tr("Darunter ist nichts, das hochrutschen könnte"),
                    Axis::Right => tr("Rechts davon ist nichts, das herüberrutschen könnte"),
                });
            }
            return;
        }
        let mut moved = false;
        if d != 0.0 {
            let mut n = 0;
            let ok = self.edit_kept(&sd.notebook, &sd.page.sheet_id, |s| {
                n = s.shift_beyond(axis, at, d)?;
                Ok(())
            });
            moved = ok && n > 0;
        }
        if after != before {
            self.shape_page(&sd.notebook, &sd.page.node, &before, &after);
        }
        self.remember(Edit::Space(sd.notebook.clone(), sd.page.sheet_id.clone(), moved, sd.page.node.clone(), before, after));
        let what = text.chars().skip(1).collect::<String>();
        self.say(if d > 0.0 || (d == 0.0 && grow > 0.0) { trf("Platz eingefügt: {}", &[&what]) } else { trf("Platz entfernt: {}", &[&what]) });
    }

    /// Gives page `node` of notebook `nb` the shape `to` (it has `from`): its size, a PDF page
    /// its gaps too.
    pub(crate) fn shape_page(&mut self, nb: &str, node: &str, from: &Shape, to: &Shape) {
        let Screen::Notebook(s) = &self.screen else { return };
        let Some(key) = s.panes.iter().find(|o| o.id == nb).map(|o| o.key) else { return };
        let (node, from, to) = (node.to_string(), from.clone(), to.clone());
        self.in_pane(key, |app| {
            app.edit_notebook(|n| {
                if from.width != to.width {
                    n.set_size(&node, to.width, to.height)?;
                }
                if from.height != to.height || from.gaps != to.gaps {
                    n.reshape(&node, to.height, &to.gaps)?;
                }
                Ok(())
            })
        });
    }

    // --- scribbling ------------------------------------------------------------------------

    /// A scribble over ink (Android: `isScribble`): the strokes it crosses go (one step to undo)
    /// and it isn't kept itself. Whether it crossed any.
    pub(super) fn scribble_away(&mut self, d: &Drawing) -> bool {
        let Some(sheet) = self.tablet_sheet(&d.notebook, &d.sheet) else { return false };
        let r = d.brush.width / 2.0 + 1.5;
        let path: Vec<(f32, f32)> = d.points.iter().map(|p| (p.x, p.y)).collect();
        let mut ids = sheet.hit_path(&path, r);
        let area = d.points.iter().fold(Rect::EMPTY, |a, p| a.include(p.x, p.y)).inflate(r + 20.0);
        for s in sheet.strokes_in(Some(area)) {
            if !ids.contains(&s.id) && tools::crosses(&s.points, s.brush.width, &d.points, r) {
                ids.push(s.id);
            }
        }
        if ids.is_empty() {
            return false;
        }
        if let Some(o) = self.pane_mut_of(d.pane) {
            o.pages.live.borrow_mut().remove(&(LOCAL.to_string(), d.live_id));
        }
        let n = ids.len();
        self.edit_tracked(&d.notebook, &d.sheet, |s| s.remove(&ids));
        self.say(if n == 1 { tr("1 Strich weggekritzelt").to_string() } else { trf("{} Striche weggekritzelt", &[&n]) });
        true
    }

    // --- the writing magnifier ----------------------------------------------------------------

    /// The magnifier's strip in its pane (window px) and the strip's camera onto its frame.
    pub fn loupe_view(&self) -> Option<(PaneKey, Rectangle, Camera)> {
        let l = self.tablet.loupe.as_ref()?;
        let o = self.pane_ref(l.pane)?;
        let page = o.pages.pages.iter().find(|p| p.placed.sheet_id == l.sheet)?;
        let strip = tools::loupe_strip(Rectangle::new(o.camera.origin, o.camera.viewport));
        let zoom = strip.width / l.w.max(1.0);
        let r = page.placed.rect;
        Some((l.pane, strip, Camera { x: r.x0 + l.x, y: r.y0 + l.y, zoom, viewport: strip.size(), origin: strip.position() }))
    }

    /// The buttons on the strip's top edge (window px): back, on, new line, close.
    pub fn loupe_controls(&self) -> Option<Rectangle> {
        let (_, strip, _) = self.loupe_view()?;
        let w = 300.0f32.min(strip.width);
        Some(Rectangle::new(Point::new(strip.x + strip.width - w, strip.y - 44.0), Size::new(w, 40.0)))
    }

    /// The frame's height for its width `w` (the strip's shape).
    fn frame_h(&self, w: f32) -> f32 {
        self.loupe_view().map_or(w * 0.3, |(_, s, _)| w * s.height / s.width.max(1.0))
    }

    /// The magnifier on (on the page in view, at its left margin, a little above the middle) or
    /// off.
    pub(crate) fn set_loupe(&mut self, on: bool) {
        if !on {
            self.tablet.loupe = None;
            return;
        }
        let Some(key) = self.active_key() else { return };
        let Some(o) = self.pane_ref(key) else { return };
        let page = (o.pages.board.is_none()).then(|| o.pages.index_of_number(o.pages.current_number(&o.camera)).map(|i| o.pages.pages[i].placed.clone())).flatten().filter(|p| !p.is_board());
        let Some(page) = page else {
            self.say(tr("Die Schreiblupe gibt es auf normalen Seiten, nicht auf Boards"));
            return;
        };
        let (pw, ph) = (page.rect.width(), page.rect.height());
        let strip = tools::loupe_strip(Rectangle::new(o.camera.origin, o.camera.viewport));
        let w = (pw * LOUPE_SHARE).clamp(40.0f32.min(pw), pw);
        let fh = w * strip.height / strip.width.max(1.0);
        let above = o.camera.to_page(Rectangle::new(o.camera.origin, o.camera.viewport), Point::new(strip.x, o.camera.origin.y + (strip.y - o.camera.origin.y) * 0.45));
        let y = (above.y - page.rect.y0).clamp(0.0, (ph - fh).max(0.0));
        self.tablet.loupe = Some(Loupe { pane: key, notebook: o.id.clone(), sheet: page.sheet_id.clone(), x: pw * 0.06, y, w, line_x: pw * 0.06, ink: None, last_x: None, advance_at: None, corner: None });
        self.snap_frame();
        self.follow_frame();
    }

    /// How wide the magnifier's page is (page points).
    pub fn loupe_page_width(&self) -> Option<f32> {
        self.loupe_page().map(|p| p.rect.width())
    }

    /// The magnifier's page.
    fn loupe_page(&self) -> Option<Placed> {
        let l = self.tablet.loupe.as_ref()?;
        self.pane_ref(l.pane)?.pages.pages.iter().find(|p| p.placed.sheet_id == l.sheet).map(|p| p.placed.clone())
    }

    /// The frame kept on its page.
    fn clamp_frame(&mut self) {
        let Some(p) = self.loupe_page() else { return };
        let Some(w) = self.tablet.loupe.as_ref().map(|l| l.w) else { return };
        let fh = self.frame_h(w);
        if let Some(l) = &mut self.tablet.loupe {
            l.w = l.w.min(p.rect.width());
            l.x = l.x.clamp(0.0, (p.rect.width() - l.w).max(0.0));
            l.y = l.y.clamp(0.0, (p.rect.height() - fh).max(0.0));
        }
    }

    /// On ruled, squared and dotted paper the frame sits on a line: the rule nearest its writing
    /// height (72 % down the frame).
    fn snap_frame(&mut self) {
        let Some(p) = self.loupe_page() else { return };
        let Some(w) = self.tablet.loupe.as_ref().map(|l| l.w) else { return };
        let fh = self.frame_h(w);
        if let (Some((off, sp)), Some(l)) = (tools::rules(p.spec.background, p.spec.spacing), &mut self.tablet.loupe) {
            let base = l.y + fh * LOUPE_BASELINE;
            l.y = off + ((base - off) / sp).round() * sp - fh * LOUPE_BASELINE;
        }
        self.clamp_frame();
    }

    /// Keeps the frame in view above the strip (the pages move, not the frame).
    fn follow_frame(&mut self) {
        let Some((key, strip, _)) = self.loupe_view() else { return };
        let Some(p) = self.loupe_page() else { return };
        let Some(l) = self.tablet.loupe.clone() else { return };
        let fh = self.frame_h(l.w);
        let Some(o) = self.pane_mut_of(key) else { return };
        let c = o.camera;
        let view = Rectangle::new(c.origin, c.viewport);
        let a = c.to_screen(view, p.rect.x0 + l.x, p.rect.y0 + l.y);
        let b = c.to_screen(view, p.rect.x0 + l.x + l.w, p.rect.y0 + l.y + fh);
        let (top, bottom) = (view.y + 60.0, strip.y - 56.0);
        let mut next = c;
        if a.x < view.x + 12.0 || b.x > view.x + view.width - 12.0 {
            next.x = p.rect.x0 + l.x + l.w / 2.0 - c.viewport.width / c.zoom / 2.0;
        }
        if a.y < top || b.y > bottom {
            next.y = p.rect.y0 + l.y - (top + (bottom - top).max(0.0) * 0.3 - view.y) / c.zoom;
        }
        if next != c {
            o.camera = o.pages.clamp(next);
        }
    }

    /// The frame back or on along the line.
    pub(crate) fn loupe_step(&mut self, forward: bool) {
        if forward {
            self.loupe_move_on();
            return;
        }
        if let Some(l) = &mut self.tablet.loupe {
            l.advance_at = None;
            l.x = (l.x - l.w * 0.62).max(0.0);
        }
        self.clamp_frame();
        self.follow_frame();
    }

    /// The frame on along the line; at the page's right edge to the next line.
    fn loupe_move_on(&mut self) {
        let Some(p) = self.loupe_page() else { return };
        let right = p.rect.width() * 0.97;
        let Some(l) = &mut self.tablet.loupe else { return };
        l.advance_at = None;
        if l.x + l.w >= right - 1.0 {
            self.loupe_new_line();
            return;
        }
        l.x = (l.x + l.w * 0.62).min(right - l.w);
        l.last_x = None;
        self.clamp_frame();
        self.follow_frame();
    }

    /// The frame's next line: one rule (lined paper), whole boxes for what was written (squared,
    /// dotted), else a line's height; back to where the line began.
    pub(crate) fn loupe_new_line(&mut self) {
        let Some(p) = self.loupe_page() else { return };
        let Some(l) = self.tablet.loupe.clone() else { return };
        let fh = self.frame_h(l.w);
        let ink = l.ink.map_or(0.0, |(t, b)| b - t);
        let step = tools::line_step(p.spec.background, p.spec.spacing, ink, fh);
        let max_y = (p.rect.height() - fh).max(0.0);
        if l.y >= max_y - 0.5 {
            self.say(tr("Ende der Seite"));
            return;
        }
        if let Some(l) = &mut self.tablet.loupe {
            l.advance_at = None;
            l.y = (l.y + step).min(max_y);
            l.x = l.line_x;
            l.ink = None;
            l.last_x = None;
        }
        self.clamp_frame();
        self.snap_frame();
        self.follow_frame();
    }

    /// A finger slid or pinched the paper in the strip (or the wheel moved it): the frame goes
    /// where the strip's camera looks now.
    pub(crate) fn loupe_camera(&mut self, c: Camera) {
        let Some(p) = self.loupe_page() else { return };
        let Some((_, strip, _)) = self.loupe_view() else { return };
        if let Some(l) = &mut self.tablet.loupe {
            let pw = p.rect.width();
            l.w = (strip.width / c.zoom.max(1e-3)).clamp(pw * 0.12, pw * 0.8);
            l.x = c.x - p.rect.x0;
            l.y = c.y - p.rect.y0;
            l.advance_at = None;
            if l.x < l.line_x {
                l.line_x = l.x;
            }
        }
        self.clamp_frame();
    }

    /// The pen down at window point `p` in the strip of pane `key`: the strip's camera and the
    /// page point under the pen.
    pub(crate) fn loupe_map(&mut self, key: PaneKey, p: Point) -> Option<(Camera, Point)> {
        let (k, strip, cam) = self.loupe_view()?;
        if k != key || !strip.contains(p) || self.loupe_controls().is_some_and(|c| c.contains(p)) {
            return None;
        }
        if let Some(l) = &mut self.tablet.loupe {
            l.advance_at = None;
        }
        Some((cam, cam.to_page(strip, p)))
    }

    /// The pen down near the frame's bottom-right corner (on the page): it resizes the frame.
    pub(crate) fn loupe_corner_down(&mut self, key: PaneKey, p: Point) -> bool {
        let Some(page) = self.loupe_page() else { return false };
        let Some(l) = self.tablet.loupe.clone() else { return false };
        if l.pane != key {
            return false;
        }
        let fh = self.frame_h(l.w);
        let Some(o) = self.pane_ref(key) else { return false };
        let c = o.camera;
        let corner = c.to_screen(Rectangle::new(c.origin, c.viewport), page.rect.x0 + l.x + l.w, page.rect.y0 + l.y + fh);
        if corner.distance(p) > 26.0 {
            return false;
        }
        if let Some(l) = &mut self.tablet.loupe {
            l.corner = Some(p);
            l.advance_at = None;
        }
        true
    }

    /// A pen tap ended (no stroke): in the strip's right part the frame moves on; on a page
    /// above, the frame goes there. Whether it did.
    pub(crate) fn loupe_tap(&mut self) -> bool {
        let Some(l) = self.tablet.loupe.clone() else { return false };
        let Some(d) = &self.tablet.drawing else { return false };
        let Some(first) = d.points.first().copied() else { return false };
        if d.map.is_some() {
            if first.x > l.x + l.w * (1.0 - LOUPE_ADVANCE) {
                self.loupe_move_on();
                return true;
            }
            return false;
        }
        let (pane, sheet) = (d.pane, d.sheet.clone());
        let Some(o) = self.pane_ref(pane) else { return false };
        if o.pages.board.is_some() || pane != l.pane {
            return false;
        }
        let Some(page) = o.pages.pages.iter().find(|p| p.placed.sheet_id == sheet).map(|p| p.placed.clone()) else { return false };
        if page.is_board() {
            return false;
        }
        let fh = self.frame_h(l.w);
        if let Some(l) = &mut self.tablet.loupe {
            l.sheet = sheet;
            l.w = l.w.min(page.rect.width());
            l.x = first.x - l.w * 0.04;
            l.y = first.y - fh * LOUPE_BASELINE;
            l.ink = None;
            l.last_x = None;
            l.advance_at = None;
        }
        self.clamp_frame();
        self.snap_frame();
        if let Some(l) = &mut self.tablet.loupe {
            l.line_x = l.x;
        }
        self.follow_frame();
        true
    }

    /// A stroke written in the strip (sheet points): the line's ink grows; lifted in the right
    /// part, the frame moves on shortly after.
    pub(crate) fn loupe_after_stroke(&mut self, sheet: &str, points: &[InkPoint]) {
        let Some(l) = &mut self.tablet.loupe else { return };
        if l.sheet != sheet || points.is_empty() {
            return;
        }
        for p in points {
            l.ink = Some(l.ink.map_or((p.y, p.y), |(t, b)| (t.min(p.y), b.max(p.y))));
        }
        let last = points[points.len() - 1].x;
        l.last_x = Some(last);
        if last > l.x + l.w * (1.0 - LOUPE_ADVANCE) {
            l.advance_at = Some(Instant::now() + LOUPE_DELAY);
        }
    }

    /// Every frame while a move-on is due: it happens once its time came (not while the pen
    /// writes: a dot on the i right after cancels it).
    pub(crate) fn loupe_timer(&mut self) {
        let due = self.tablet.loupe.as_ref().and_then(|l| l.advance_at).is_some_and(|t| Instant::now() >= t);
        if due && !self.tablet.busy() {
            self.loupe_move_on();
        }
    }

    // --- the pen for all of them ---------------------------------------------------------------

    /// The pen moved to window point `p`: a "+", space or the magnifier's corner it holds follow.
    /// Whether it held one.
    pub(crate) fn extras_move(&mut self, p: Point) -> bool {
        if let Some(h) = &mut self.tablet.handle {
            let (dx, dy) = (p.x - h.down.x, p.y - h.down.y);
            h.drag = match h.handle.dir {
                HandleDir::Left => -dx,
                HandleDir::Right => dx,
                HandleDir::Below => dy,
            };
            return true;
        }
        if let Some(page) = self.loupe_page()
            && let Some(l) = self.tablet.loupe.as_ref().filter(|l| l.corner.is_some())
        {
            let zoom = self.pane_ref(l.pane).map_or(1.0, |o| o.camera.zoom);
            let pw = page.rect.width();
            if let Some(l) = &mut self.tablet.loupe {
                let last = l.corner.unwrap_or(p);
                l.w = (l.w + (p.x - last.x) / zoom).clamp(pw * 0.12, pw * 0.8);
                l.corner = Some(p);
            }
            self.clamp_frame();
            return true;
        }
        let Some(mut sd) = self.tablet.space.take() else { return false };
        let Some(cam) = self.pane_ref(sd.pane).map(|o| o.camera) else {
            self.tablet.space = Some(sd);
            return true;
        };
        let at = cam.to_page(Rectangle::new(cam.origin, cam.viewport), p);
        // Which way: from where the pen first goes (a PDF page only gets space downwards).
        if sd.sideways.is_none() {
            let (mx, my) = ((at.x - sd.down.x) * cam.zoom, (at.y - sd.down.y) * cam.zoom);
            if mx.hypot(my) >= 12.0 {
                let across = mx.abs() > my.abs();
                if across && sd.page.spec.pdf.is_some() {
                    self.say(tr("Auf PDF-Seiten geht Platz nur nach unten"));
                }
                sd.sideways = Some(across && sd.page.spec.pdf.is_none());
            }
        }
        match sd.sideways {
            Some(true) => {
                let dx = ((at.x - sd.down.x) / sd.step_x).round() * sd.step_x;
                if dx != sd.asked || sd.plan_x.is_none() {
                    sd.asked = dx;
                    sd.plan_x = Some(n0te_core::notebook::plan_space_x(&sd.page.spec, sd.at.0, dx, sd.right));
                }
            }
            Some(false) => {
                let dy = ((at.y - sd.down.y) / sd.step).round() * sd.step;
                if dy != sd.asked || sd.plan.is_none() {
                    sd.asked = dy;
                    sd.plan = Some(n0te_core::notebook::plan_space(&sd.page.spec, sd.at.1, dy, sd.below));
                }
            }
            None => {}
        }
        self.show_space(&sd);
        self.tablet.space = Some(sd);
        true
    }

    /// The pen lifted: a "+" adds its page (unless dragged back), space goes in, the
    /// magnifier's corner is let go. Whether one of them had the pen.
    pub(crate) fn extras_up(&mut self) -> bool {
        if let Some(h) = self.tablet.handle.take() {
            if h.drag > -HANDLE_CANCEL {
                self.add_page_at(h.pane, &h.handle);
            }
            return true;
        }
        if self.tablet.loupe.as_ref().is_some_and(|l| l.corner.is_some()) {
            if let Some(l) = &mut self.tablet.loupe {
                l.corner = None;
            }
            self.snap_frame();
            self.follow_frame();
            return true;
        }
        if let Some(sd) = self.tablet.space.take() {
            self.end_space(sd, true);
            return true;
        }
        false
    }

    /// The pen was taken away: whatever it held stops (nothing changes).
    pub(crate) fn extras_cancel(&mut self) {
        self.tablet.handle = None;
        if let Some(l) = &mut self.tablet.loupe {
            l.corner = None;
        }
        if let Some(sd) = self.tablet.space.take() {
            self.end_space(sd, false);
        }
    }

    /// The window areas where the pen writes, without the magnifier's buttons (the pen taps
    /// those like a finger).
    pub(crate) fn without_loupe_controls(&self, areas: Vec<Rectangle>) -> Vec<Rectangle> {
        let Some(hole) = self.loupe_controls() else { return areas };
        areas.into_iter().flat_map(|a| subtract(a, hole)).collect()
    }

    /// Whether the tool in use is the space tool (the toolbar shows it chosen).
    pub fn space_tool(&self) -> bool {
        self.tablet.tool == Tool::Space
    }
}

/// `a` without `hole`: up to four rectangles around it.
pub fn subtract(a: Rectangle, hole: Rectangle) -> Vec<Rectangle> {
    let Some(h) = a.intersection(&hole) else { return vec![a] };
    let mut out = Vec::new();
    if h.y > a.y {
        out.push(Rectangle::new(a.position(), Size::new(a.width, h.y - a.y)));
    }
    if h.y + h.height < a.y + a.height {
        out.push(Rectangle::new(Point::new(a.x, h.y + h.height), Size::new(a.width, a.y + a.height - h.y - h.height)));
    }
    if h.x > a.x {
        out.push(Rectangle::new(Point::new(a.x, h.y), Size::new(h.x - a.x, h.height)));
    }
    if h.x + h.width < a.x + a.width {
        out.push(Rectangle::new(Point::new(h.x + h.width, h.y), Size::new(a.x + a.width - h.x - h.width, h.height)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hole_leaves_four_rectangles_around_it() {
        let a = Rectangle::new(Point::ORIGIN, Size::new(100.0, 100.0));
        let parts = subtract(a, Rectangle::new(Point::new(40.0, 40.0), Size::new(20.0, 20.0)));
        assert_eq!(parts.len(), 4);
        let area: f32 = parts.iter().map(|r| r.width * r.height).sum();
        assert!((area - (10000.0 - 400.0)).abs() < 1e-3);
        assert!(!parts.iter().any(|r| r.contains(Point::new(50.0, 50.0))));
        assert_eq!(subtract(a, Rectangle::new(Point::new(200.0, 0.0), Size::new(5.0, 5.0))), vec![a]);
    }
}
