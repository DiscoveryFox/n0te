//! n0te Tablet: the app set up for pen and touch, the port of the Android tablet app.
//!
//! The pen writes ink (pen types, colours, widths and the fountain pen's character from
//! [`n0te_core::pens`], the same rules as on the Android tablet), the eraser takes whole
//! strokes or cuts them (eraser end of the pen, or the eraser tool), the lasso selects; fingers
//! pan and zoom. Strokes go live to the other devices while they are drawn (the sync service's
//! "live" command) and are stored at pen-up. Undo / redo work on this app's own edits, across
//! pages, like on the tablet.
//!
//! Pen events come from [`pen`] (Windows: the real pen; elsewhere the page view turns a mouse
//! drag into the same events) in window coordinates; each goes to the pane under it.

pub mod overlay;
pub mod pen;
pub mod toolbar;

use std::collections::HashMap;

use iced::{Point, Rectangle, Task};
use n0te_core::ink::{Brush, InkPoint};
use n0te_core::pens::{self, Fountain, FountainOptions, PenType};
use n0te_core::sheet::Sheet;

use crate::app::{App, Message, Screen};
use crate::pages::{Camera, LiveInk};
use crate::split::PaneKey;
use pen::{PenEvent, Phase};

/// The device name our own live strokes carry in the page view (other devices use their id).
pub const LOCAL: &str = "";

/// The tool the pen uses (the eraser end of the pen erases whatever the tool).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Pen,
    Marker,
    Eraser,
    Lasso,
}

/// How the eraser erases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EraserMode {
    /// Whole strokes it touches.
    Stroke,
    /// Only where it passes (strokes are cut).
    Area,
}

/// Toolbar actions.
#[derive(Debug, Clone)]
pub enum Msg {
    Tool(Tool),
    Pen(PenType),
    Color(usize),
    Width(usize),
    Palette(usize),
    EraserMode(EraserMode),
    Undo,
    Redo,
    /// The pen-type menu open / closed.
    PenMenu(bool),
    DeleteSelection,
    ClearSelection,
}

/// A stroke being written.
struct Drawing {
    pane: PaneKey,
    notebook: String,
    sheet: String,
    /// The sheet's top-left corner in page coordinates.
    origin: (f32, f32),
    brush: Brush,
    points: Vec<InkPoint>,
    fountain: Option<Fountain>,
    /// Time of the first sample (stored times count from there).
    t0: u32,
    live_id: u64,
    /// Samples already sent live, and how many live messages went out.
    sent: usize,
    seq: u32,
}

/// The eraser under way.
struct Erasing {
    pane: PaneKey,
    notebook: String,
    sheet: String,
    origin: (f32, f32),
    mode: EraserMode,
    /// The eraser's path in sheet points (x, y, radius).
    path: Vec<(f32, f32, f32)>,
    /// Strokes touched so far (stroke mode).
    hit: Vec<String>,
}

/// A lasso being drawn.
struct Lassoing {
    pane: PaneKey,
    notebook: String,
    sheet: String,
    origin: (f32, f32),
    polygon: Vec<f32>,
}

/// Selected strokes on one sheet.
#[derive(Debug, Clone)]
pub struct Selection {
    pub notebook: String,
    pub sheet: String,
    pub ids: Vec<String>,
    /// Around the selected strokes, in the sheet's points.
    pub bounds: n0te_core::ink::Rect,
}

/// n0te Tablet's state (in [`App`]; unused in n0te Laptop).
pub struct Tablet {
    pub tool: Tool,
    pub pen: PenType,
    pub palette: usize,
    pub color: usize,
    pub width: usize,
    pub widths: [f32; 4],
    pub fountain: FountainOptions,
    pub pencil_opacity: f32,
    pub eraser_mode: EraserMode,
    /// Eraser radius in screen points (the same size at every zoom).
    pub eraser_size: f32,
    pub pen_menu: bool,
    pub selection: Option<Selection>,
    /// The lasso's outline while it is drawn (page coordinates), for the view.
    pub lasso_outline: Option<(PaneKey, Vec<Point>)>,
    drawing: Option<Drawing>,
    erasing: Option<Erasing>,
    lassoing: Option<Lassoing>,
    /// Sheets kept open so their undo history lives on: (notebook, sheet) → sheet.
    sheets: HashMap<(String, String), Sheet>,
    /// Our edits, oldest first (undo goes back through them across pages); and what was undone.
    undo: Vec<(String, String)>,
    redo: Vec<(String, String)>,
    next_live: u64,
}

impl Default for Tablet {
    fn default() -> Self {
        Tablet {
            tool: Tool::Pen,
            pen: PenType::Fountain,
            palette: 0,
            color: 0,
            width: 1,
            widths: pens::WIDTHS,
            fountain: FountainOptions::default(),
            pencil_opacity: pens::PENCIL_OPACITY,
            eraser_mode: EraserMode::Stroke,
            eraser_size: 10.0,
            pen_menu: false,
            selection: None,
            lasso_outline: None,
            drawing: None,
            erasing: None,
            lassoing: None,
            sheets: HashMap::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            // Unique enough across restarts (live ids only tell strokes apart while drawn).
            next_live: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64 & 0xFFFF_FFFF).unwrap_or(1) << 16,
        }
    }
}

impl Tablet {
    /// The colour in use (0xAARRGGBB).
    pub fn ink(&self) -> u32 {
        pens::PALETTES[self.palette % pens::PALETTES.len()][self.color % 6]
    }

    /// The brush the pen writes with now.
    pub fn brush(&self) -> Brush {
        let w = self.widths[self.width % 4];
        match self.tool {
            Tool::Marker => pens::marker_brush(self.ink(), w),
            _ => pens::pen_brush(self.pen, self.ink(), w, &self.fountain, self.pencil_opacity),
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// Whether the pen is doing something right now (drawing, erasing, lasso).
    pub fn busy(&self) -> bool {
        self.drawing.is_some() || self.erasing.is_some() || self.lassoing.is_some()
    }
}

/// Where a window point lands: the pane, its camera's screen rectangle, the page point.
fn locate(app: &App, x: f32, y: f32) -> Option<(PaneKey, Camera, Point)> {
    let Screen::Notebook(s) = &app.screen else { return None };
    let p = Point::new(x, y);
    s.panes.iter().find_map(|o| {
        let r = Rectangle::new(o.camera.origin, o.camera.viewport);
        r.contains(p).then(|| (o.key, o.camera, o.camera.to_page(r, p)))
    })
}

impl App {
    /// The canvas areas of the panes (window coordinates), where the pen writes instead of
    /// tapping; none while a panel or dialog covers the pages.
    pub(crate) fn ink_areas(&self) -> Vec<Rectangle> {
        let Screen::Notebook(s) = &self.screen else { return Vec::new() };
        if self.palette.is_some() || self.show_settings || self.show_help || self.history.is_some() || self.share.is_some() || self.page_panel || self.show_pdf_import || self.plot_edit.is_some() || self.tablet.pen_menu {
            return Vec::new();
        }
        s.panes.iter().filter(|o| o.text_page.as_ref().is_none_or(|t| t.docked)).map(|o| Rectangle::new(o.camera.origin, o.camera.viewport)).collect()
    }

    /// Toolbar actions.
    pub(crate) fn on_tablet(&mut self, m: Msg) -> Task<Message> {
        let t = &mut self.tablet;
        match m {
            Msg::Tool(tool) => {
                t.tool = tool;
                if tool != Tool::Lasso {
                    t.selection = None;
                }
            }
            Msg::Pen(p) => {
                t.pen = p;
                t.tool = Tool::Pen;
                t.pen_menu = false;
            }
            Msg::Color(i) => {
                t.color = i;
                if !matches!(t.tool, Tool::Pen | Tool::Marker) {
                    t.tool = Tool::Pen;
                }
                if let Some(sel) = t.selection.clone() {
                    let color = t.ink();
                    self.edit_tracked(&sel.notebook, &sel.sheet, |s| s.recolor(&sel.ids, color));
                }
            }
            Msg::Width(i) => {
                t.width = i;
                if !matches!(t.tool, Tool::Pen | Tool::Marker) {
                    t.tool = Tool::Pen;
                }
            }
            Msg::Palette(i) => t.palette = i % pens::PALETTES.len(),
            Msg::EraserMode(mode) => {
                t.eraser_mode = mode;
                t.tool = Tool::Eraser;
            }
            Msg::PenMenu(on) => t.pen_menu = on,
            Msg::Undo => self.tablet_undo(true),
            Msg::Redo => self.tablet_undo(false),
            Msg::DeleteSelection => {
                if let Some(sel) = self.tablet.selection.take() {
                    self.edit_tracked(&sel.notebook, &sel.sheet, |s| s.remove(&sel.ids));
                }
            }
            Msg::ClearSelection => self.tablet.selection = None,
        }
        Task::none()
    }

    /// A pen event (window coordinates).
    pub(crate) fn on_pen(&mut self, e: PenEvent) -> Task<Message> {
        match e.phase {
            Phase::Down => {
                let Some(s) = e.last() else { return Task::none() };
                self.pen_down(&e, s);
            }
            Phase::Move => {
                for s in &e.samples {
                    self.pen_move(*s);
                }
                self.send_live(false);
            }
            Phase::Up => {
                if let Some(s) = e.last() {
                    self.pen_move(s);
                }
                self.pen_up();
            }
            Phase::Cancel => self.pen_cancel(),
            Phase::Hover | Phase::Leave => {}
        }
        Task::none()
    }

    fn pen_down(&mut self, e: &PenEvent, s: pen::Sample) {
        self.pen_cancel();
        let Some((key, cam, at)) = locate(self, s.x, s.y) else { return };
        let Screen::Notebook(split) = &mut self.screen else { return };
        split.active = key;
        let Some(o) = split.pane(key) else { return };
        let notebook = o.id.clone();
        let tool = if e.eraser || e.barrel { Tool::Eraser } else { self.tablet.tool };
        // A tap on a group title folds or unfolds the group.
        if let Some(node) = o.pages.header_node_at(at) {
            self.fold_group(key, node);
            return;
        }
        // The pen writes on the sheet it went down on (boards included).
        let Some(placed) = o.pages.sheet_at(at.x, at.y).cloned() else { return };
        if self.read_only_nb(&notebook) {
            self.say("Nur lesen: ändern kann, wer dich eingeladen hat");
            return;
        }
        let origin = (placed.rect.x0, placed.rect.y0);
        let local = (at.x - origin.0, at.y - origin.1);
        match tool {
            Tool::Pen | Tool::Marker => {
                let t = &mut self.tablet;
                t.selection = None;
                let brush = t.brush();
                let fountain = (tool == Tool::Pen && t.pen == PenType::Fountain).then(|| Fountain::new(t.fountain));
                t.next_live += 1;
                t.drawing = Some(Drawing { pane: key, notebook, sheet: placed.sheet_id.clone(), origin, brush, points: Vec::new(), fountain, t0: s.t_ms, live_id: t.next_live, sent: 0, seq: 0 });
                self.pen_move(s);
            }
            Tool::Eraser => {
                let radius = self.tablet.eraser_size / cam.zoom;
                let mode = if e.eraser || e.barrel { EraserMode::Stroke } else { self.tablet.eraser_mode };
                self.tablet.erasing = Some(Erasing { pane: key, notebook, sheet: placed.sheet_id.clone(), origin, mode, path: vec![(local.0, local.1, radius)], hit: Vec::new() });
                self.erase_step();
            }
            Tool::Lasso => {
                self.tablet.selection = None;
                self.tablet.lassoing = Some(Lassoing { pane: key, notebook, sheet: placed.sheet_id.clone(), origin, polygon: vec![local.0, local.1] });
                self.tablet.lasso_outline = Some((key, vec![at]));
            }
        }
    }

    fn pen_move(&mut self, s: pen::Sample) {
        if let Some(d) = &mut self.tablet.drawing {
            let Some(cam) = pane_camera(&self.screen, d.pane) else { return };
            let at = cam.to_page(Rectangle::new(cam.origin, cam.viewport), Point::new(s.x, s.y));
            let (x, y) = (at.x - d.origin.0, at.y - d.origin.1);
            let t = s.t_ms.wrapping_sub(d.t0);
            // Skip samples that don't move (a resting pen floods them).
            if d.points.last().is_some_and(|p| (p.x - x).hypot(p.y - y) < 0.05 && p.t_ms + 4 > t) {
                return;
            }
            let pressure = match &mut d.fountain {
                Some(f) => f.pressure(x, y, s.pressure, t as f32),
                None => s.pressure,
            };
            d.points.push(InkPoint { x, y, pressure, t_ms: t });
            let (sheet, brush, points, id) = (d.sheet.clone(), d.brush, d.points.clone(), d.live_id);
            if let Screen::Notebook(split) = &mut self.screen
                && let Some(o) = split.pane_mut(d.pane)
            {
                o.pages.live.borrow_mut().insert((LOCAL.to_string(), id), LiveInk { sheet_id: sheet, brush, points, done: None });
            }
            return;
        }
        if let Some(er) = &mut self.tablet.erasing {
            let Some(cam) = pane_camera(&self.screen, er.pane) else { return };
            let at = cam.to_page(Rectangle::new(cam.origin, cam.viewport), Point::new(s.x, s.y));
            let r = self.tablet.eraser_size / cam.zoom;
            er.path.push((at.x - er.origin.0, at.y - er.origin.1, r));
            self.erase_step();
            return;
        }
        if let Some(l) = &mut self.tablet.lassoing {
            let Some(cam) = pane_camera(&self.screen, l.pane) else { return };
            let at = cam.to_page(Rectangle::new(cam.origin, cam.viewport), Point::new(s.x, s.y));
            l.polygon.extend([at.x - l.origin.0, at.y - l.origin.1]);
            if let Some((_, pts)) = &mut self.tablet.lasso_outline {
                pts.push(at);
            }
        }
    }

    fn pen_up(&mut self) {
        if let Some(mut d) = self.tablet.drawing.take() {
            if let Some(f) = &d.fountain {
                f.taper_end(&mut d.points);
            }
            // A dot: a tap leaves a point, as on paper.
            if d.points.len() == 1 {
                let p = d.points[0];
                d.points.push(InkPoint { x: p.x + 0.01, y: p.y + 0.01, ..p });
            }
            let (nb, sheet, brush, points, id) = (d.notebook.clone(), d.sheet.clone(), d.brush, d.points.clone(), d.live_id);
            // Mark the live stroke done: it stays until the stored one is drawn.
            if let Screen::Notebook(split) = &mut self.screen
                && let Some(o) = split.pane_mut(d.pane)
                && let Some(l) = o.pages.live.borrow_mut().get_mut(&(LOCAL.to_string(), id))
            {
                l.points = points.clone();
                l.done = Some(std::time::Instant::now());
            }
            self.tablet.drawing = Some(d);
            self.send_live(true);
            self.tablet.drawing = None;
            self.edit_tracked(&nb, &sheet, |s| s.add_stroke(brush, &points).map(|_| ()));
            return;
        }
        if let Some(er) = self.tablet.erasing.take() {
            let (nb, sheet) = (er.notebook.clone(), er.sheet.clone());
            if let Screen::Notebook(split) = &mut self.screen
                && let Some(o) = split.pane_mut(er.pane)
            {
                o.pages.erasing.borrow_mut().remove(&sheet);
            }
            match er.mode {
                EraserMode::Stroke if !er.hit.is_empty() => self.edit_tracked(&nb, &sheet, |s| s.remove(&er.hit)),
                EraserMode::Area => {
                    let path = er.path.clone();
                    self.edit_tracked(&nb, &sheet, |s| s.erase_area(&path).map(|_| ()));
                }
                _ => {}
            }
            return;
        }
        if let Some(l) = self.tablet.lassoing.take() {
            self.tablet.lasso_outline = None;
            if l.polygon.len() < 6 {
                return;
            }
            let Some(sheet) = self.tablet_sheet(&l.notebook, &l.sheet) else { return };
            let ids = sheet.lasso(&l.polygon);
            let bounds = ids.iter().filter_map(|id| sheet.stroke(id).ok().flatten()).fold(n0te_core::ink::Rect::EMPTY, |r, s| r.union(&s.bounds));
            self.tablet.selection = (!ids.is_empty()).then(|| Selection { notebook: l.notebook, sheet: l.sheet, ids, bounds });
        }
    }

    fn pen_cancel(&mut self) {
        if let Some(d) = self.tablet.drawing.take()
            && let Screen::Notebook(split) = &mut self.screen
            && let Some(o) = split.pane_mut(d.pane)
        {
            o.pages.live.borrow_mut().remove(&(LOCAL.to_string(), d.live_id));
        }
        if let Some(er) = self.tablet.erasing.take()
            && let Screen::Notebook(split) = &mut self.screen
            && let Some(o) = split.pane_mut(er.pane)
        {
            o.pages.erasing.borrow_mut().remove(&er.sheet);
            o.pages.reload_sheet(&er.sheet);
        }
        self.tablet.lassoing = None;
        self.tablet.lasso_outline = None;
    }

    /// Shows what the eraser takes so far (hidden strokes / cut area) before it is stored.
    fn erase_step(&mut self) {
        let Some(er) = &self.tablet.erasing else { return };
        let Some(&(x, y, r)) = er.path.last() else { return };
        let (nb, sheet, mode, pane) = (er.notebook.clone(), er.sheet.clone(), er.mode, er.pane);
        let hits = match mode {
            EraserMode::Stroke => self.tablet_sheet(&nb, &sheet).map(|s| s.hit_test(x, y, r)).unwrap_or_default(),
            EraserMode::Area => Vec::new(),
        };
        let er = self.tablet.erasing.as_mut().expect("erasing");
        let new: Vec<String> = hits.into_iter().filter(|h| !er.hit.contains(h)).collect();
        er.hit.extend(new.iter().cloned());
        let dabs: Vec<[f32; 3]> = if mode == EraserMode::Area { vec![[x, y, r]] } else { Vec::new() };
        if let Screen::Notebook(split) = &mut self.screen
            && let Some(o) = split.pane_mut(pane)
        {
            o.pages.live_erase(&sheet, &new, &dabs);
        }
        if let Some(sync) = &self.sync
            && (!new.is_empty() || !dabs.is_empty())
        {
            sync.send(&["erase".into(), nb, sheet, serde_json::to_string(&new).unwrap_or_default(), serde_json::to_string(&dabs).unwrap_or_default()]);
        }
    }

    /// Sends the new samples of the stroke being drawn to the other devices.
    fn send_live(&mut self, done: bool) {
        let Some(sync) = self.sync.clone() else { return };
        let Some(d) = &mut self.tablet.drawing else { return };
        if d.sent >= d.points.len() && !done {
            return;
        }
        let mut bytes = Vec::with_capacity((d.points.len() - d.sent.min(d.points.len())) * 16);
        for p in &d.points[d.sent.min(d.points.len())..] {
            for v in [p.x, p.y, p.pressure, p.t_ms as f32] {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        d.sent = d.points.len();
        let b = d.brush;
        sync.send(&[
            "live".into(),
            d.notebook.clone(),
            d.sheet.clone(),
            d.live_id.to_string(),
            d.seq.to_string(),
            b.width.to_string(),
            b.thinning.to_string(),
            b.color.to_string(),
            b.kind.to_string(),
            serde_json::to_string(&bytes).unwrap_or_default(),
            if done { "1".into() } else { "0".into() },
        ]);
        d.seq += 1;
    }

    /// The sheet kept open for edits (and undo), loaded on first use.
    fn tablet_sheet(&mut self, nb: &str, sheet: &str) -> Option<&Sheet> {
        let k = (nb.to_string(), sheet.to_string());
        if !self.tablet.sheets.contains_key(&k) {
            let s = self.lib.load_sheet(nb, sheet).ok()?;
            self.tablet.sheets.insert(k.clone(), s);
        }
        let s = self.tablet.sheets.get(&k)?;
        // Edits from elsewhere (sync, the other pane) come in first.
        let _ = self.lib.refresh_sheet(nb, sheet, s);
        Some(s)
    }

    /// Changes a sheet through its kept-open copy, stores it and remembers the edit for undo.
    pub(crate) fn edit_tracked(&mut self, nb: &str, sheet: &str, f: impl FnOnce(&Sheet) -> n0te_core::Result<()>) {
        if self.read_only_nb(nb) {
            self.say("Nur lesen: ändern kann, wer dich eingeladen hat");
            return;
        }
        let Some(s) = self.tablet_sheet(nb, sheet) else {
            self.say("Seite nicht lesbar");
            return;
        };
        if let Err(e) = f(s) {
            self.say(format!("Nicht geändert: {e}"));
            return;
        }
        self.store_kept(nb, sheet);
        self.tablet.undo.push((nb.to_string(), sheet.to_string()));
        self.tablet.redo.clear();
    }

    /// Stores a kept-open sheet (it syncs from there) and redraws the panes showing it.
    fn store_kept(&mut self, nb: &str, sheet: &str) {
        let k = (nb.to_string(), sheet.to_string());
        let Some(s) = self.tablet.sheets.remove(&k) else { return };
        self.store_sheet(nb, sheet, &s);
        self.tablet.sheets.insert(k, s);
    }

    /// Undo (or redo) this app's last edit, on whatever page it was.
    pub(crate) fn tablet_undo(&mut self, undo: bool) {
        let entry = if undo { self.tablet.undo.pop() } else { self.tablet.redo.pop() };
        let Some((nb, sheet)) = entry else { return };
        let Some(s) = self.tablet_sheet(&nb, &sheet) else { return };
        let done = if undo { s.undo() } else { s.redo() };
        match done {
            Ok(true) => {
                self.store_kept(&nb, &sheet);
                if undo { self.tablet.redo.push((nb, sheet)) } else { self.tablet.undo.push((nb, sheet)) }
            }
            Ok(false) => {}
            Err(e) => self.say(format!("Rückgängig ging nicht: {e}")),
        }
        self.tablet.selection = None;
    }
}

impl App {
    fn fold_group(&mut self, key: PaneKey, node: String) {
        let _ = self.update(Message::Pane(key, Box::new(Message::Fold(node))));
    }
}

fn pane_camera(screen: &Screen, key: PaneKey) -> Option<Camera> {
    match screen {
        Screen::Notebook(s) => s.pane(key).map(|o| o.camera),
        Screen::Library => None,
    }
}
