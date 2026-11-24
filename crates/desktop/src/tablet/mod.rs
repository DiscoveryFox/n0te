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

pub mod keyboard;
pub mod overlay;
pub mod pen;
pub mod recognize;
pub mod toolbar;

use std::collections::HashMap;

use iced::{Point, Rectangle, Task};
use n0te_core::ink::{Brush, InkPoint};
use n0te_core::pens::{self, Fountain, FountainOptions, PenType};
use n0te_core::sheet::Sheet;
use n0te_core::snap;

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
    /// Tap: a new text box there, or edit the one tapped.
    Text,
    /// A red point the others see (live session, guests).
    Laser,
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
    FingerDraws(bool),
    DeleteSelection,
    ClearSelection,
    Copy,
    Cut,
    /// Paste what was copied in the middle of the view.
    Paste,
    /// A frame while drawing (hold timers for shapes and arrows).
    Frame,
    /// Handwriting of a sheet was read (notebook, sheet, lines, keys to drop).
    Indexed(String, String, Vec<n0te_core::sheet::IndexLine>, Vec<String>),
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
    /// Where the pen last settled (shapes snap when it rests there long enough).
    rest: snap::Rest,
    /// The hold checks that already ran since the pen settled: (shape, arrow).
    fired: (bool, bool),
    /// The stroke snapped to a shape and follows the pen while it stays down.
    held: Option<snap::Held>,
    snapped: bool,
    /// When the pen went down (for the hold timers), in the stroke's own milliseconds.
    started: std::time::Instant,
    /// The page's strokes as possible lines for arrowheads (ids alongside), read on first need.
    lines: Option<(Vec<String>, Vec<n0te_core::arrows::LineStroke>)>,
    /// Strokes the snap replaces (the first barb of a two-barb head).
    replaces: Vec<String>,
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

/// The selection being moved (or scaled from its bottom-right corner).
struct Moving {
    pane: PaneKey,
    sheet: String,
    origin: (f32, f32),
    /// Where the pen went down (sheet points).
    start: (f32, f32),
    scale_mode: bool,
    /// The selected strokes as they were.
    strokes: Vec<(String, Brush, Vec<InkPoint>)>,
    bounds: n0te_core::ink::Rect,
    dx: f32,
    dy: f32,
    scale: f32,
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
    /// A finger writes like the pen (devices without a pen); two fingers still pan and zoom.
    pub finger_draws: bool,
    /// Where the pen hovers (pane, page point), for the cursor ring.
    pub hover: Option<(PaneKey, Point)>,
    pub selection: Option<Selection>,
    /// The lasso's outline while it is drawn (page coordinates), for the view.
    pub lasso_outline: Option<(PaneKey, Vec<Point>)>,
    /// The laser point (pane, page point) while the laser tool touches.
    pub laser: Option<(PaneKey, Point)>,
    drawing: Option<Drawing>,
    erasing: Option<Erasing>,
    lassoing: Option<Lassoing>,
    moving: Option<Moving>,
    /// Where and when the pen went down (window point, ms): a short touch without moving is a tap.
    down: Option<(Point, u32, u32)>,
    /// Copied strokes, relative to their top-left corner.
    clip: Vec<(Brush, Vec<InkPoint>)>,
    /// Sheets whose handwriting is to be read (since when), and whether a read runs.
    index_due: HashMap<(String, String), std::time::Instant>,
    indexing: bool,
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
            finger_draws: false,
            hover: None,
            selection: None,
            lasso_outline: None,
            laser: None,
            drawing: None,
            erasing: None,
            lassoing: None,
            moving: None,
            down: None,
            clip: Vec::new(),
            index_due: HashMap::new(),
            indexing: false,
            sheets: HashMap::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            // Unique enough across restarts (live ids only tell strokes apart while drawn).
            next_live: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64 & 0xFFFF_FFFF).unwrap_or(1) << 16,
        }
    }
}

impl Tablet {
    /// Where the tool choice is kept: tablet.conf next to the app's settings.
    fn conf_path() -> std::path::PathBuf {
        crate::settings::path().with_file_name("tablet.conf")
    }

    /// The tools as last left (pen type, palette, colour, width, the widths, eraser, fountain pen).
    pub fn load() -> Tablet {
        let mut t = Tablet::default();
        let Ok(text) = std::fs::read_to_string(Self::conf_path()) else { return t };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            let num = |d: f32| v.parse::<f32>().unwrap_or(d);
            match k.trim() {
                "pen" => t.pen = PenType::from_key(v).unwrap_or(t.pen),
                "tool" => {
                    t.tool = match v {
                        "marker" => Tool::Marker,
                        "eraser" => Tool::Eraser,
                        "lasso" => Tool::Lasso,
                        "text" => Tool::Text,
                        _ => Tool::Pen,
                    }
                }
                "palette" => t.palette = v.parse().unwrap_or(0) % pens::PALETTES.len(),
                "color" => t.color = v.parse().unwrap_or(0) % 6,
                "width" => t.width = v.parse().unwrap_or(1) % 4,
                "widths" => {
                    for (i, w) in v.split(',').filter_map(|w| w.trim().parse::<f32>().ok()).take(4).enumerate() {
                        t.widths[i] = w.clamp(pens::MIN_WIDTH, pens::MAX_WIDTH);
                    }
                }
                "eraser" => t.eraser_mode = if v == "area" { EraserMode::Area } else { EraserMode::Stroke },
                "eraser_size" => t.eraser_size = num(t.eraser_size).clamp(3.0, 60.0),
                "fp_sensitivity" => t.fountain.sensitivity = num(t.fountain.sensitivity).clamp(0.0, 1.0),
                "fp_speed" => t.fountain.speed = num(t.fountain.speed).clamp(0.0, 1.0),
                "fp_nib" => t.fountain.nib = pens::Nib::from_key(v),
                "fp_taper" => t.fountain.taper = v != "0",
                "pencil_opacity" => t.pencil_opacity = num(t.pencil_opacity).clamp(0.15, 1.0),
                "finger_draws" => t.finger_draws = v == "1",
                _ => {}
            }
        }
        t
    }

    /// Writes the tool choice (n0te Tablet only; tests never do).
    pub fn save(&self) {
        let tool = match self.tool {
            Tool::Marker => "marker",
            Tool::Eraser => "eraser",
            Tool::Lasso => "lasso",
            Tool::Text => "text",
            Tool::Pen | Tool::Laser => "pen",
        };
        let widths = self.widths.iter().map(|w| format!("{w:.2}")).collect::<Vec<_>>().join(",");
        let f = &self.fountain;
        let text = format!(
            "pen={}\ntool={tool}\npalette={}\ncolor={}\nwidth={}\nwidths={widths}\neraser={}\neraser_size={:.1}\nfp_sensitivity={:.2}\nfp_speed={:.2}\nfp_nib={}\nfp_taper={}\npencil_opacity={:.2}\nfinger_draws={}\n",
            self.pen.key(),
            self.palette,
            self.color,
            self.width,
            if self.eraser_mode == EraserMode::Area { "area" } else { "stroke" },
            self.eraser_size,
            f.sensitivity,
            f.speed,
            f.nib.key(),
            u8::from(f.taper),
            self.pencil_opacity,
            u8::from(self.finger_draws),
        );
        let path = Self::conf_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(path, text);
    }

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

    /// Whether something was copied (Einfügen shows).
    pub fn clip_is_empty(&self) -> bool {
        self.clip.is_empty()
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// Whether the pen is doing something right now (drawing, erasing, lasso).
    pub fn busy(&self) -> bool {
        self.drawing.is_some() || self.erasing.is_some() || self.lassoing.is_some() || self.moving.is_some()
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
        let remember = matches!(m, Msg::Tool(_) | Msg::Pen(_) | Msg::Color(_) | Msg::Width(_) | Msg::Palette(_) | Msg::EraserMode(_) | Msg::FingerDraws(_));
        let task = self.tablet_action(m);
        if remember && self.persist_settings {
            self.tablet.save();
        }
        task
    }

    fn tablet_action(&mut self, m: Msg) -> Task<Message> {
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
            Msg::FingerDraws(on) => t.finger_draws = on,
            Msg::Undo => self.tablet_undo(true),
            Msg::Redo => self.tablet_undo(false),
            Msg::DeleteSelection => {
                if let Some(sel) = self.tablet.selection.take() {
                    self.edit_tracked(&sel.notebook, &sel.sheet, |s| s.remove(&sel.ids));
                }
            }
            Msg::ClearSelection => self.tablet.selection = None,
            Msg::Copy | Msg::Cut => {
                let Some(sel) = self.tablet.selection.clone() else { return Task::none() };
                let Some(sheet) = self.tablet_sheet(&sel.notebook, &sel.sheet) else { return Task::none() };
                let (x0, y0) = (sel.bounds.x0, sel.bounds.y0);
                let clip: Vec<(Brush, Vec<InkPoint>)> = sel
                    .ids
                    .iter()
                    .filter_map(|id| sheet.stroke(id).ok().flatten())
                    .map(|st| (st.brush, st.points.iter().map(|p| InkPoint { x: p.x - x0, y: p.y - y0, ..*p }).collect()))
                    .collect();
                self.tablet.clip = clip;
                if matches!(m, Msg::Cut) {
                    self.tablet.selection = None;
                    self.edit_tracked(&sel.notebook, &sel.sheet, |s| s.remove(&sel.ids));
                }
                self.say(crate::i18n::trf("{} Striche kopiert", &[&self.tablet.clip.len()]));
            }
            Msg::Paste => self.paste_strokes(),
            Msg::Frame => self.tablet_frame(),
            Msg::Indexed(nb, sheet, put, drop) => {
                self.tablet.indexing = false;
                if (!put.is_empty() || !drop.is_empty())
                    && let Some(s) = self.tablet_sheet(&nb, &sheet)
                    && s.set_index(&put, &drop).is_ok()
                {
                    self.store_kept(&nb, &sheet);
                }
            }
        }
        Task::none()
    }

    /// A pen event (window coordinates).
    pub(crate) fn on_pen(&mut self, e: PenEvent) -> Task<Message> {
        match e.phase {
            Phase::Down => {
                let Some(s) = e.last() else { return Task::none() };
                self.tablet.down = Some((Point::new(s.x, s.y), s.t_ms, s.t_ms));
                return self.pen_down(&e, s);
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
                let tap = self.tablet.down.take().is_some_and(|(_, t0, far)| far == t0 && e.last().is_none_or(|s| s.t_ms.wrapping_sub(t0) < 400));
                return self.pen_up(tap);
            }
            Phase::Cancel => self.pen_cancel(),
            Phase::Hover => self.tablet.hover = e.last().and_then(|s| locate(self, s.x, s.y)).map(|(k, _, at)| (k, at)),
            Phase::Leave => self.tablet.hover = None,
        }
        Task::none()
    }

    fn pen_down(&mut self, e: &PenEvent, s: pen::Sample) -> Task<Message> {
        self.pen_cancel();
        let Some((key, cam, at)) = locate(self, s.x, s.y) else { return Task::none() };
        let Screen::Notebook(split) = &mut self.screen else { return Task::none() };
        split.active = key;
        let Some(o) = split.pane(key) else { return Task::none() };
        let notebook = o.id.clone();
        let tool = if e.eraser || e.barrel { Tool::Eraser } else { self.tablet.tool };
        // A tap on a group title folds or unfolds the group.
        if let Some(node) = o.pages.header_node_at(at) {
            self.fold_group(key, node);
            return Task::none();
        }
        // The pen writes on the sheet it went down on (boards included).
        let Some(placed) = o.pages.sheet_at(at.x, at.y).cloned() else { return Task::none() };
        let text_hit = o.pages.text_at(at.x, at.y);
        if tool == Tool::Laser {
            self.tablet.laser = Some((key, at));
            self.send_laser(key, &placed.sheet_id, Some((at.x - placed.rect.x0, at.y - placed.rect.y0)));
            return Task::none();
        }
        if self.read_only_nb(&notebook) {
            self.say(crate::i18n::tr("Nur lesen: ändern kann, wer dich eingeladen hat"));
            return Task::none();
        }
        let origin = (placed.rect.x0, placed.rect.y0);
        let local = (at.x - origin.0, at.y - origin.1);
        // On the selection: move it (from its bottom-right corner: scale it).
        if let Some(sel) = self.tablet.selection.clone()
            && sel.notebook == notebook
            && sel.sheet == placed.sheet_id
        {
            let pad = 10.0 / cam.zoom;
            let b = sel.bounds.inflate(pad);
            if b.contains(local.0, local.1) {
                let corner = (local.0 - b.x1).hypot(local.1 - b.y1) < 22.0 / cam.zoom;
                let Some(sheet) = self.tablet_sheet(&notebook, &placed.sheet_id) else { return Task::none() };
                let strokes = sel.ids.iter().filter_map(|id| sheet.stroke(id).ok().flatten()).map(|st| (st.id, st.brush, st.points)).collect();
                if let Screen::Notebook(split) = &mut self.screen
                    && let Some(o) = split.pane_mut(key)
                {
                    o.pages.live_erase(&placed.sheet_id, &sel.ids, &[]);
                }
                self.tablet.moving = Some(Moving { pane: key, sheet: placed.sheet_id.clone(), origin, start: local, scale_mode: corner, strokes, bounds: sel.bounds, dx: 0.0, dy: 0.0, scale: 1.0 });
                return Task::none();
            }
            self.tablet.selection = None;
        }
        if tool == Tool::Text {
            return match text_hit {
                Some((sheet, id)) => Task::done(Message::EditText(sheet, id)),
                None if !placed.is_board() => Task::done(Message::NewText(placed.sheet_id.clone(), local.0, local.1)),
                None => Task::none(),
            };
        }
        match tool {
            Tool::Pen | Tool::Marker => {
                let t = &mut self.tablet;
                t.selection = None;
                let brush = t.brush();
                let fountain = (tool == Tool::Pen && t.pen == PenType::Fountain).then(|| Fountain::new(t.fountain));
                t.next_live += 1;
                t.drawing = Some(Drawing {
                    pane: key,
                    notebook,
                    sheet: placed.sheet_id.clone(),
                    origin,
                    brush,
                    points: Vec::new(),
                    fountain,
                    t0: s.t_ms,
                    live_id: t.next_live,
                    sent: 0,
                    seq: 0,
                    rest: snap::Rest::new(local.0, local.1, 0),
                    fired: (false, false),
                    held: None,
                    snapped: false,
                    started: std::time::Instant::now(),
                    lines: None,
                    replaces: Vec::new(),
                });
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
            Tool::Text | Tool::Laser => {}
        }
        Task::none()
    }

    fn pen_move(&mut self, s: pen::Sample) {
        if let Some((p0, t0, far)) = &mut self.tablet.down
            && p0.distance(Point::new(s.x, s.y)) > 4.0
        {
            *far = t0.wrapping_add(1);
        }
        if let Some((key, _)) = self.tablet.laser {
            let Some((k, _, at)) = locate(self, s.x, s.y) else { return };
            if k != key {
                return;
            }
            self.tablet.laser = Some((key, at));
            let hit = pane_camera(&self.screen, key).and(match &self.screen {
                Screen::Notebook(split) => split.pane(key).and_then(|o| o.pages.sheet_at(at.x, at.y).map(|p| (p.sheet_id.clone(), p.rect.x0, p.rect.y0))),
                Screen::Library => None,
            });
            if let Some((sheet, x0, y0)) = hit {
                self.send_laser(key, &sheet, Some((at.x - x0, at.y - y0)));
            }
            return;
        }
        if let Some(m) = &mut self.tablet.moving {
            let Some(cam) = pane_camera(&self.screen, m.pane) else { return };
            let at = cam.to_page(Rectangle::new(cam.origin, cam.viewport), Point::new(s.x, s.y));
            let (x, y) = (at.x - m.origin.0, at.y - m.origin.1);
            if m.scale_mode {
                let (w0, h0) = (m.bounds.width().max(1.0), m.bounds.height().max(1.0));
                let k = ((x - m.bounds.x0) / w0).max((y - m.bounds.y0) / h0).max(0.1);
                m.scale = k.clamp(0.1, 10.0);
            } else {
                m.dx = x - m.start.0;
                m.dy = y - m.start.1;
            }
            let (sheet, pane) = (m.sheet.clone(), m.pane);
            let (ox, oy, k, dx, dy) = (m.bounds.x0, m.bounds.y0, m.scale, m.dx, m.dy);
            let previews: Vec<(String, LiveInk)> = m
                .strokes
                .iter()
                .map(|(id, b, pts)| {
                    let points = pts.iter().map(|p| InkPoint { x: ox + (p.x - ox) * k + dx, y: oy + (p.y - oy) * k + dy, ..*p }).collect();
                    (format!("sel:{id}"), LiveInk { sheet_id: sheet.clone(), brush: Brush { width: b.width * k, ..*b }, points, done: None })
                })
                .collect();
            if let Screen::Notebook(split) = &mut self.screen
                && let Some(o) = split.pane_mut(pane)
            {
                let mut live = o.pages.live.borrow_mut();
                for (id, l) in previews {
                    live.insert((LOCAL.to_string(), hash_id(&id)), l);
                }
            }
            return;
        }
        if let Some(d) = &mut self.tablet.drawing {
            let Some(cam) = pane_camera(&self.screen, d.pane) else { return };
            let at = cam.to_page(Rectangle::new(cam.origin, cam.viewport), Point::new(s.x, s.y));
            let (x, y) = (at.x - d.origin.0, at.y - d.origin.1);
            let t = s.t_ms.wrapping_sub(d.t0);
            // Snapped: the shape follows the pen (a line's end, a box's corner).
            if let Some(h) = &mut d.held {
                let u = h.pen_at(x, y, t, cam.zoom, 1.0);
                if let Some(pts) = u.points {
                    d.points = pts;
                    self.show_drawing();
                }
                if let Some(free) = u.freed {
                    self.say(crate::i18n::tr(if free { "Linie frei: rastet nicht mehr ein" } else { "Linie rastet wieder ein" }));
                }
                return;
            }
            // Moving on from where it rested: the hold checks start over.
            if d.rest.moved(x, y, t, cam.zoom) {
                d.fired = (false, false);
            }
            // Skip samples that hardly move (a resting pen floods them).
            if d.points.last().is_some_and(|p| (p.x - x).hypot(p.y - y) < snap::MIN_STEP_PX / cam.zoom) {
                return;
            }
            let pressure = match &mut d.fountain {
                Some(f) => f.pressure(x, y, s.pressure, t as f32),
                None => s.pressure,
            };
            d.points.push(InkPoint { x, y, pressure, t_ms: t });
            self.show_drawing();
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

    fn pen_up(&mut self, tap: bool) -> Task<Message> {
        if let Some((key, _)) = self.tablet.laser.take() {
            self.send_laser(key, "", None);
            return Task::none();
        }
        if let Some(m) = self.tablet.moving.take() {
            let Some(sel) = self.tablet.selection.clone() else { return Task::none() };
            if let Screen::Notebook(split) = &mut self.screen
                && let Some(o) = split.pane_mut(m.pane)
            {
                let mut live = o.pages.live.borrow_mut();
                for (id, _, _) in &m.strokes {
                    live.remove(&(LOCAL.to_string(), hash_id(&format!("sel:{id}"))));
                }
                drop(live);
                o.pages.erasing.borrow_mut().remove(&m.sheet);
            }
            if m.dx != 0.0 || m.dy != 0.0 || m.scale != 1.0 {
                let (dx, dy, k, ox, oy) = (m.dx, m.dy, m.scale, m.bounds.x0, m.bounds.y0);
                self.edit_tracked(&sel.notebook, &sel.sheet, |s| s.transform(&sel.ids, dx, dy, k, ox, oy));
                if let Some(sel) = &mut self.tablet.selection {
                    let b = m.bounds;
                    sel.bounds = n0te_core::ink::Rect::new(ox + (b.x0 - ox) * k + dx, oy + (b.y0 - oy) * k + dy, ox + (b.x1 - ox) * k + dx, oy + (b.y1 - oy) * k + dy);
                }
            } else if let Screen::Notebook(split) = &mut self.screen
                && let Some(o) = split.pane_mut(m.pane)
            {
                o.pages.reload_sheet(&m.sheet);
            }
            return Task::none();
        }
        // A tap with the pen on a coordinate system or a text box opens it (no dot).
        if tap && let Some(d) = &self.tablet.drawing {
            let first = d.points.first().copied();
            let target = match (&self.screen, first) {
                (Screen::Notebook(split), Some(p)) => split.pane(d.pane).and_then(|o| {
                    let (x, y) = (p.x + d.origin.0, p.y + d.origin.1);
                    o.pages.plot_at(x, y).map(|(s, id)| Message::EditPlot(s, id)).or_else(|| o.pages.text_at(x, y).map(|(s, id)| Message::EditText(s, id)))
                }),
                _ => None,
            };
            if let Some(m) = target {
                self.pen_cancel();
                return Task::done(m);
            }
        }
        if let Some(mut d) = self.tablet.drawing.take() {
            // Rested long enough before lifting: a shape or an arrowhead after all.
            let mut changed = d.snapped;
            if !d.snapped && d.points.len() > 4 {
                let zoom = pane_camera(&self.screen, d.pane).map(|c| c.zoom).unwrap_or(1.0);
                let held = (d.started.elapsed().as_millis() as u32).saturating_sub(d.rest.since());
                let lines = self.drawing_lines(&mut d);
                if let Some(sn) = snap::on_lift(&d.points, zoom, held, &lines.1, self.tablet.snap_options()) {
                    if let Some(i) = sn.delete
                        && let Some(id) = lines.0.get(i)
                    {
                        d.replaces.push(id.clone());
                    }
                    d.points = sn.shape.points;
                    changed = true;
                }
            }
            // The fountain pen lifts off thin, unless the stroke became a shape.
            if let Some(f) = &d.fountain
                && !changed
            {
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
            let replaces = d.replaces.clone();
            self.tablet.drawing = Some(d);
            self.send_live(true);
            self.tablet.drawing = None;
            self.edit_tracked(&nb, &sheet, |s| {
                if !replaces.is_empty() {
                    s.remove(&replaces)?;
                }
                s.add_stroke(brush, &points).map(|_| ())
            });
            return Task::none();
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
            return Task::none();
        }
        if let Some(l) = self.tablet.lassoing.take() {
            self.tablet.lasso_outline = None;
            if l.polygon.len() < 6 {
                return Task::none();
            }
            let Some(sheet) = self.tablet_sheet(&l.notebook, &l.sheet) else { return Task::none() };
            let ids = sheet.lasso(&l.polygon);
            let bounds = ids.iter().filter_map(|id| sheet.stroke(id).ok().flatten()).fold(n0te_core::ink::Rect::EMPTY, |r, s| r.union(&s.bounds));
            self.tablet.selection = (!ids.is_empty()).then(|| Selection { notebook: l.notebook, sheet: l.sheet, ids, bounds });
        }
        Task::none()
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
            self.say(crate::i18n::tr("Nur lesen: ändern kann, wer dich eingeladen hat"));
            return;
        }
        let Some(s) = self.tablet_sheet(nb, sheet) else {
            self.say(crate::i18n::tr("Seite nicht lesbar"));
            return;
        };
        if let Err(e) = f(s) {
            self.say(crate::i18n::trf("Nicht geändert: {}", &[&e]));
            return;
        }
        self.store_kept(nb, sheet);
        self.tablet.undo.push((nb.to_string(), sheet.to_string()));
        self.tablet.redo.clear();
        self.tablet.index_due.insert((nb.to_string(), sheet.to_string()), std::time::Instant::now());
    }

    /// Every second: reads the handwriting of a sheet left alone for a moment (one at a time,
    /// on a worker thread). Nothing happens where the system has no handwriting recognizer.
    pub(crate) fn tablet_tick(&mut self) -> Task<Message> {
        if self.tablet.indexing || self.tablet.busy() || self.tablet.index_due.is_empty() {
            return Task::none();
        }
        static AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if !*AVAILABLE.get_or_init(recognize::available) {
            self.tablet.index_due.clear();
            return Task::none();
        }
        let Some(k) = self.tablet.index_due.iter().filter(|(_, t)| t.elapsed().as_millis() > 1500).map(|(k, _)| k.clone()).next() else { return Task::none() };
        self.tablet.index_due.remove(&k);
        let (nb, sheet) = k;
        let Some(input) = self.tablet_sheet(&nb, &sheet).and_then(recognize::prepare) else { return Task::none() };
        self.tablet.indexing = true;
        let (tx, rx) = iced::futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(recognize::read(input));
        });
        Task::perform(rx, move |r| {
            let (put, drop) = r.unwrap_or_default();
            Message::Tablet(Msg::Indexed(nb.clone(), sheet.clone(), put, drop))
        })
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
            Err(e) => self.say(crate::i18n::trf("Rückgängig ging nicht: {}", &[&e])),
        }
        self.tablet.selection = None;
    }
}

impl Tablet {
    /// Shape recognition and arrowheads (both on, like the tablet's defaults); heads only with
    /// the pen tool.
    pub fn snap_options(&self) -> snap::Options {
        snap::Options { shapes: true, arrows: true, pen: self.tool == Tool::Pen }
    }
}

impl App {
    /// Shows the stroke being drawn as it is now (live ink in its pane).
    fn show_drawing(&mut self) {
        let Some(d) = &self.tablet.drawing else { return };
        let (sheet, brush, points, id, pane) = (d.sheet.clone(), d.brush, d.points.clone(), d.live_id, d.pane);
        if let Screen::Notebook(split) = &mut self.screen
            && let Some(o) = split.pane_mut(pane)
        {
            o.pages.live.borrow_mut().insert((LOCAL.to_string(), id), LiveInk { sheet_id: sheet, brush, points, done: None });
        }
    }

    /// The strokes on the drawing's page as lines for arrowheads (read once per stroke).
    fn drawing_lines(&mut self, d: &mut Drawing) -> (Vec<String>, Vec<n0te_core::arrows::LineStroke>) {
        if d.lines.is_none() {
            let strokes = self.tablet_sheet(&d.notebook, &d.sheet).map(|s| s.strokes()).unwrap_or_default();
            d.lines = Some(strokes.iter().map(|s| (s.id.clone(), n0te_core::arrows::LineStroke::of(s))).unzip());
        }
        d.lines.clone().unwrap_or_default()
    }

    /// Every frame while the pen draws: the hold timers (a shape after 0.5 s at rest, an arrow
    /// after 0.9 s), as on the tablet.
    pub(crate) fn tablet_frame(&mut self) {
        let Some(mut d) = self.tablet.drawing.take() else { return };
        if !d.snapped && d.points.len() >= 5 {
            let held = (d.started.elapsed().as_millis() as u32).saturating_sub(d.rest.since());
            let o = self.tablet.snap_options();
            let due = if !d.fired.0 && held >= snap::SHAPE_HOLD_MS {
                d.fired.0 = true;
                Some(snap::Hold::Shape)
            } else if o.arrows && !d.fired.1 && held >= snap::ARROW_HOLD_MS {
                d.fired.1 = true;
                Some(snap::Hold::Arrow)
            } else {
                None
            };
            if let Some(hold) = due {
                let zoom = pane_camera(&self.screen, d.pane).map(|c| c.zoom).unwrap_or(1.0);
                let lines = self.drawing_lines(&mut d);
                if let Some(sn) = snap::on_hold(hold, &d.points, zoom, &lines.1, o) {
                    if let Some(i) = sn.delete
                        && let Some(id) = lines.0.get(i)
                    {
                        d.replaces.push(id.clone());
                        if let Screen::Notebook(split) = &mut self.screen
                            && let Some(p) = split.pane_mut(d.pane)
                        {
                            p.pages.live_erase(&d.sheet, std::slice::from_ref(id), &[]);
                        }
                    }
                    let pen = d.points.last().map(|p| (p.x, p.y)).unwrap_or_default();
                    d.held = snap::Held::new(&sn.shape, pen.0, pen.1);
                    d.points = sn.shape.points;
                    d.snapped = true;
                }
            }
        }
        self.tablet.drawing = Some(d);
        self.show_drawing();
    }

    /// Whether the hold timers need frames now.
    pub fn tablet_wants_frames(&self) -> bool {
        self.tablet.drawing.as_ref().is_some_and(|d| !d.snapped)
    }
}

/// A live-ink key for a preview stroke (selection being moved).
fn hash_id(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish() | 1 << 63
}

impl App {
    /// Where this view looks, with the laser point (sheet points) or without it (`None`).
    fn send_laser(&mut self, key: PaneKey, sheet: &str, at: Option<(f32, f32)>) {
        let Some(sync) = self.sync.clone() else { return };
        let Screen::Notebook(split) = &self.screen else { return };
        let Some(o) = split.pane(key) else { return };
        let (x0, y0) = o.pages.pages.iter().find(|p| p.placed.sheet_id == sheet).map(|p| (p.placed.rect.x0, p.placed.rect.y0)).unwrap_or((0.0, 0.0));
        let v = o.camera.view_rect();
        let mut args = vec!["presence".to_string(), o.id.clone(), sheet.to_string()];
        args.extend([v.x0 - x0, v.y0 - y0, v.width(), v.height()].iter().map(|x| format!("{x:.1}")));
        args.push(format!("{:.3}", o.camera.zoom));
        if let Some((x, y)) = at {
            args.extend([format!("{x:.1}"), format!("{y:.1}")]);
        }
        sync.send(&args);
    }

    /// Pastes the copied strokes in the middle of the active pane's view, on the sheet there.
    fn paste_strokes(&mut self) {
        if self.tablet.clip.is_empty() {
            return;
        }
        let Screen::Notebook(split) = &self.screen else { return };
        let o = split.active_pane();
        let v = o.camera.view_rect();
        let (cx, cy) = ((v.x0 + v.x1) / 2.0, (v.y0 + v.y1) / 2.0);
        let Some(p) = o.pages.sheet_at(cx, cy).cloned() else { return };
        let nb = o.id.clone();
        let w = self.tablet.clip.iter().flat_map(|(_, pts)| pts.iter().map(|p| p.x)).fold(0.0f32, f32::max);
        let h = self.tablet.clip.iter().flat_map(|(_, pts)| pts.iter().map(|p| p.y)).fold(0.0f32, f32::max);
        let (x0, y0) = (cx - p.rect.x0 - w / 2.0, cy - p.rect.y0 - h / 2.0);
        let items: Vec<(Brush, Vec<InkPoint>)> = self.tablet.clip.iter().map(|(b, pts)| (*b, pts.iter().map(|q| InkPoint { x: q.x + x0, y: q.y + y0, ..*q }).collect())).collect();
        let mut ids = Vec::new();
        self.edit_tracked(&nb, &p.sheet_id, |s| {
            ids = s.add_strokes(&items)?;
            Ok(())
        });
        if !ids.is_empty() {
            self.tablet.tool = Tool::Lasso;
            self.tablet.selection = Some(Selection { notebook: nb, sheet: p.sheet_id, ids, bounds: n0te_core::ink::Rect::new(x0, y0, x0 + w, y0 + h) });
        }
    }

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
