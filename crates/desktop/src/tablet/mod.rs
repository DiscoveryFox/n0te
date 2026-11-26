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
pub mod extras;
pub mod recognize;
pub mod toolbar;
pub mod tools;

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
    /// Put the pen down and drag: space goes into the page there, down or sideways.
    Space,
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
    /// Picks a favourite pen (its index); keys 1–6 do too.
    Favourite(usize),
    /// Keeps the pen as it is now as a favourite.
    AddFavourite,
    /// Lets go of a favourite (right click or the pen's side button on it).
    RemoveFavourite(usize),
    EraserMode(EraserMode),
    Undo,
    Redo,
    /// The pen-type menu open / closed.
    PenMenu(bool),
    FingerDraws(bool),
    /// "Mitwandern" on / off: zoomed in, a lift in the band at the right moves the page on.
    WriteFlow(bool),
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
    /// The ruler on / off.
    Ruler(bool),
    /// Fingers moved the ruler (in that pane).
    RulerMoved(PaneKey, tools::Ruler),
    /// The writing magnifier on / off.
    Loupe(bool),
    /// Its frame back or on along the line.
    LoupeStep(bool),
    /// Its frame to the next line.
    LoupeNewLine,
    /// A finger slid (or pinched) the paper in its strip: the strip's camera now.
    LoupeCamera(crate::pages::Camera),
    /// The board in view on its own / back to the pages.
    Board(bool),
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
    /// The strokes around it as possible lines for arrowheads (ids alongside) and the area they
    /// were read from, read when first needed.
    lines: Option<(n0te_core::ink::Rect, (Vec<String>, Vec<n0te_core::arrows::LineStroke>))>,
    /// Strokes the snap replaces (the first barb of a two-barb head).
    replaces: Vec<String>,
    /// Written through the magnifier: its strip's camera (else the pane's).
    map: Option<Camera>,
    /// Along the ruler's edge (-1 top, 1 bottom; 0 free).
    edge: i8,
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
    /// Erasing through the magnifier: its strip's camera (else the pane's).
    map: Option<Camera>,
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

/// One of this app's edits, for undo / redo.
#[derive(Debug, Clone)]
pub(crate) enum Edit {
    /// An edit of a sheet (in the sheet's own history): (notebook, sheet).
    Sheet(String, String),
    /// Space put in or taken out: the sheet's part when something moved, and the page's
    /// shape before and after (notebook, sheet, moved, node, before, after).
    Space(String, String, bool, String, extras::Shape, extras::Shape),
}

impl Edit {
    /// The notebook the edit was made in.
    fn notebook(&self) -> &str {
        match self {
            Edit::Sheet(nb, _) | Edit::Space(nb, ..) => nb,
        }
    }
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
/// A favourite pen: marker or pen, its type, palette, colour and width slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Favourite {
    pub marker: bool,
    pub pen: PenType,
    pub palette: usize,
    pub color: usize,
    pub width: usize,
}

/// At most this many favourites.
pub const MAX_FAVOURITES: usize = 6;

impl Favourite {
    fn key(&self) -> String {
        format!("{}|{}|{}|{}|{}", if self.marker { "M" } else { "P" }, self.pen.key(), self.palette, self.color, self.width)
    }
    fn from_key(s: &str) -> Option<Favourite> {
        let f: Vec<&str> = s.split('|').collect();
        let [m, pen, palette, color, width] = f.as_slice() else { return None };
        Some(Favourite { marker: *m == "M", pen: PenType::from_key(pen)?, palette: palette.parse::<usize>().ok()? % pens::PALETTES.len(), color: color.parse::<usize>().ok()? % 6, width: width.parse::<usize>().ok()? % 4 })
    }
}

pub struct Tablet {
    /// Favourite pens, picked with one tap (or keys 1–6).
    pub favourites: Vec<Favourite>,
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
    /// "Mitwandern beim Schreiben" (off by default, like on the tablet).
    pub write_flow: bool,
    /// The line being written with Mitwandern on, and a move due.
    pub flow: Option<extras::Flow>,
    /// Where the pen hovers (pane, page point), for the cursor ring.
    pub hover: Option<(PaneKey, Point)>,
    pub selection: Option<Selection>,
    /// The lasso's outline while it is drawn (page coordinates), for the view.
    pub lasso_outline: Option<(PaneKey, Vec<Point>)>,
    /// The laser point (pane, page point) while the laser tool touches.
    pub laser: Option<(PaneKey, Point)>,
    /// The ruler on the pages (in that pane).
    pub ruler: Option<(PaneKey, tools::Ruler)>,
    /// The writing magnifier, when open.
    pub loupe: Option<extras::Loupe>,
    /// Space being dragged into a page.
    pub space: Option<extras::SpaceDrag>,
    /// A "+" being tapped or dragged.
    pub handle: Option<extras::HandleDrag>,
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
    /// Sheets kept open so their undo history lives on: (notebook, sheet) → sheet, and the
    /// notebook's change stamp when it last caught up with its file.
    sheets: HashMap<(String, String), (std::sync::Arc<Sheet>, u64)>,
    /// Our edits, oldest first (undo goes back through them across pages); and what was undone.
    undo: Vec<Edit>,
    redo: Vec<Edit>,
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
            write_flow: false,
            favourites: Vec::new(),
            flow: None,
            hover: None,
            selection: None,
            lasso_outline: None,
            laser: None,
            ruler: None,
            loupe: None,
            space: None,
            handle: None,
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
    /// The sheets of notebook `nb` kept open here (they may hold edits still on their way to
    /// disk): sheet id → sheet.
    pub(crate) fn kept_of(&self, nb: &str) -> HashMap<String, std::sync::Arc<Sheet>> {
        self.sheets.iter().filter(|((n, _), _)| n == nb).map(|((_, s), (sheet, _))| (s.clone(), sheet.clone())).collect()
    }

    /// Kept sheet `sheet` of `nb` caught up with the notebook's files at change stamp `stamp`.
    pub(crate) fn caught_up(&mut self, nb: &str, sheet: &str, stamp: u64) {
        if let Some(k) = self.sheets.get_mut(&(nb.to_string(), sheet.to_string())) {
            k.1 = stamp;
        }
    }

    /// The pen as it is now as a favourite (None for the other tools).
    pub fn current_favourite(&self) -> Option<Favourite> {
        matches!(self.tool, Tool::Pen | Tool::Marker).then_some(Favourite { marker: self.tool == Tool::Marker, pen: self.pen, palette: self.palette, color: self.color, width: self.width })
    }

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
                        "space" => Tool::Space,
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
                "write_flow" => t.write_flow = v == "1",
                "favourites" => t.favourites = v.split(';').filter_map(Favourite::from_key).take(MAX_FAVOURITES).collect(),
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
            Tool::Space => "space",
            Tool::Pen | Tool::Laser => "pen",
        };
        let widths = self.widths.iter().map(|w| format!("{w:.2}")).collect::<Vec<_>>().join(",");
        let f = &self.fountain;
        let text = format!(
            "pen={}\ntool={tool}\npalette={}\ncolor={}\nwidth={}\nwidths={widths}\neraser={}\neraser_size={:.1}\nfp_sensitivity={:.2}\nfp_speed={:.2}\nfp_nib={}\nfp_taper={}\npencil_opacity={:.2}\nfinger_draws={}\nwrite_flow={}\nfavourites={}\n",
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
            u8::from(self.write_flow),
            self.favourites.iter().map(Favourite::key).collect::<Vec<_>>().join(";"),
        );
        let path = Self::conf_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = n0te_core::store::write_atomic(&path, text.as_bytes());
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
        self.drawing.is_some() || self.erasing.is_some() || self.lassoing.is_some() || self.moving.is_some() || self.space.is_some() || self.handle.is_some()
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
        let areas = s.panes.iter().filter(|o| o.text_page.as_ref().is_none_or(|t| t.docked)).map(|o| Rectangle::new(o.camera.origin, o.camera.viewport)).collect();
        self.without_loupe_controls(areas)
    }

    /// Toolbar actions.
    pub(crate) fn on_tablet(&mut self, m: Msg) -> Task<Message> {
        let remember = matches!(m, Msg::Tool(_) | Msg::Pen(_) | Msg::Color(_) | Msg::Width(_) | Msg::Palette(_) | Msg::EraserMode(_) | Msg::FingerDraws(_) | Msg::WriteFlow(_) | Msg::Favourite(_) | Msg::AddFavourite | Msg::RemoveFavourite(_));
        if let Msg::Tool(t) = m
            && t != Tool::Space
            && let Some(sp) = self.tablet.space.take()
        {
            self.end_space(sp, false);
        }
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
            Msg::Favourite(i) => {
                if let Some(f) = t.favourites.get(i).copied() {
                    t.tool = if f.marker { Tool::Marker } else { Tool::Pen };
                    t.pen = f.pen;
                    t.palette = f.palette;
                    t.color = f.color;
                    t.width = f.width;
                    t.selection = None;
                }
            }
            Msg::AddFavourite => {
                if let Some(f) = t.current_favourite()
                    && !t.favourites.contains(&f)
                    && t.favourites.len() < MAX_FAVOURITES
                {
                    t.favourites.push(f);
                    self.say(crate::i18n::tr("Als Favorit gemerkt · Rechtsklick entfernt").to_string());
                }
            }
            Msg::RemoveFavourite(i) => {
                if i < t.favourites.len() {
                    t.favourites.remove(i);
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
            Msg::WriteFlow(on) => {
                t.write_flow = on;
                t.flow = None;
            }
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
            Msg::Ruler(on) => self.set_ruler(on),
            Msg::RulerMoved(key, r) => {
                if self.tablet.ruler.is_some_and(|(k, _)| k == key) {
                    self.tablet.ruler = Some((key, r));
                }
            }
            Msg::Loupe(on) => self.set_loupe(on),
            Msg::LoupeStep(forward) => self.loupe_step(forward),
            Msg::LoupeNewLine => self.loupe_new_line(),
            Msg::LoupeCamera(c) => self.loupe_camera(c),
            Msg::Board(on) => self.set_board(on),
            Msg::Indexed(nb, sheet, put, drop) => {
                self.tablet.indexing = false;
                if (!put.is_empty() || !drop.is_empty())
                    && let Some(s) = self.tablet_sheet(&nb, &sheet)
                    && s.set_index(&put, &drop).is_ok()
                {
                    self.store_kept(&nb, &sheet);
                }
                // Read for a notebook that left the screen meanwhile: let go of its page.
                if !matches!(&self.screen, Screen::Notebook(s) if s.shows(&nb)) && !self.tablet.index_due.contains_key(&(nb.clone(), sheet.clone())) {
                    self.tablet.sheets.remove(&(nb, sheet));
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
        let p = Point::new(s.x, s.y);
        // The magnifier's strip: the pen writes into its frame; on the frame's corner it resizes it.
        if let Some((lcam, lat)) = self.loupe_map(key, p) {
            return self.pen_down_at(e, s, key, lcam, lat, Some(lcam));
        }
        if self.loupe_corner_down(key, p) {
            return Task::none();
        }
        // A "+" beside the current row: a page there (tap, or drag outwards).
        if self.handle_down(key, &cam, p) {
            return Task::none();
        }
        self.pen_down_at(e, s, key, cam, at, None)
    }

    /// The pen down at page point `at` of pane `key` (`map`: through the magnifier's camera).
    fn pen_down_at(&mut self, e: &PenEvent, s: pen::Sample, key: PaneKey, cam: Camera, at: Point, map: Option<Camera>) -> Task<Message> {
        let Screen::Notebook(split) = &mut self.screen else { return Task::none() };
        split.active = key;
        let Some(o) = split.pane(key) else { return Task::none() };
        let notebook = o.id.clone();
        let tool = if e.eraser || e.barrel { Tool::Eraser } else { self.tablet.tool };
        // Through the magnifier only ink and the eraser, on its page.
        if map.is_some() && !matches!(tool, Tool::Pen | Tool::Marker | Tool::Eraser) {
            return Task::none();
        }
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
            o.pages.own_laser.borrow_mut().point(&placed.sheet_id, at.x - placed.rect.x0, at.y - placed.rect.y0, crate::pages::laser_now());
            return Task::none();
        }
        if self.read_only_nb(&notebook) {
            self.say(crate::i18n::tr("Nur lesen: ändern kann, wer dich eingeladen hat"));
            return Task::none();
        }
        if tool == Tool::Space {
            self.space_down(key, &placed, at);
            return Task::none();
        }
        let origin = (placed.rect.x0, placed.rect.y0);
        let mut local = (at.x - origin.0, at.y - origin.1);
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
                // At the ruler's edge: the stroke follows it.
                let edge = match t.ruler {
                    Some((k, r)) if k == key && map.is_none() => r.edge_at(at.x, at.y, tools::RULER_EDGE_PX / cam.zoom),
                    _ => 0,
                };
                if edge != 0
                    && let Some((_, r)) = t.ruler
                {
                    let (x, y) = r.snap(at.x, at.y, edge, brush.width);
                    local = (x - origin.0, y - origin.1);
                }
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
                    map,
                    edge,
                });
                self.pen_move(s);
            }
            Tool::Eraser => {
                let radius = self.tablet.eraser_size / cam.zoom;
                let mode = if e.eraser || e.barrel { EraserMode::Stroke } else { self.tablet.eraser_mode };
                self.tablet.erasing = Some(Erasing { pane: key, notebook, sheet: placed.sheet_id.clone(), origin, mode, path: vec![(local.0, local.1, radius)], hit: Vec::new(), map });
                self.erase_step();
            }
            Tool::Lasso => {
                self.tablet.selection = None;
                self.tablet.lassoing = Some(Lassoing { pane: key, notebook, sheet: placed.sheet_id.clone(), origin, polygon: vec![local.0, local.1] });
                self.tablet.lasso_outline = Some((key, vec![at]));
            }
            Tool::Text | Tool::Laser | Tool::Space => {}
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
            // On the page it went down on, so the trail stays one piece across page edges.
            if let Screen::Notebook(split) = &self.screen
                && let Some(o) = split.pane(key)
            {
                let mut own = o.pages.own_laser.borrow_mut();
                let page = own.sheet_id.clone().and_then(|id| o.pages.pages.iter().find(|p| p.placed.sheet_id == id).map(|p| p.placed.clone()));
                if let Some(p) = page {
                    own.point(&p.sheet_id, at.x - p.rect.x0, at.y - p.rect.y0, crate::pages::laser_now());
                }
            }
            return;
        }
        if self.extras_move(Point::new(s.x, s.y)) {
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
                    (format!("sel:{id}"), LiveInk { sheet_id: sheet.clone(), brush: Brush { width: b.width * k, ..*b }, points, ..Default::default() })
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
            let Some(cam) = d.map.or_else(|| pane_camera(&self.screen, d.pane)) else { return };
            let mut at = cam.to_page(Rectangle::new(cam.origin, cam.viewport), Point::new(s.x, s.y));
            // Along the ruler: on its edge.
            if d.edge != 0
                && let Some((_, r)) = self.tablet.ruler
            {
                let (x, y) = r.snap(at.x, at.y, d.edge, d.brush.width);
                at = Point::new(x, y);
            }
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
            let Some(cam) = er.map.or_else(|| pane_camera(&self.screen, er.pane)) else { return };
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
        if self.extras_up() {
            return Task::none();
        }
        // The magnifier: a tap in its strip's right part moves its frame on, a tap on a page
        // puts the frame there (no dot either way).
        if tap && (self.loupe_tap() || self.flow_tap()) {
            self.pen_cancel();
            return Task::none();
        }
        if let Some((key, _)) = self.tablet.laser.take() {
            if let Screen::Notebook(split) = &self.screen
                && let Some(o) = split.pane(key)
            {
                o.pages.own_laser.borrow_mut().lift(crate::pages::laser_now());
            }
            return Task::none();
        }
        if let Some(m) = self.tablet.moving.take() {
            self.end_move_preview(&m);
            let Some(sel) = self.tablet.selection.clone() else {
                if let Some(nb) = self.pane_notebook(m.pane) {
                    self.reload_kept(m.pane, &nb, &m.sheet);
                }
                return Task::none();
            };
            if m.dx != 0.0 || m.dy != 0.0 || m.scale != 1.0 {
                let (dx, dy, k, ox, oy) = (m.dx, m.dy, m.scale, m.bounds.x0, m.bounds.y0);
                self.edit_tracked(&sel.notebook, &sel.sheet, |s| s.transform(&sel.ids, dx, dy, k, ox, oy));
                if let Some(sel) = &mut self.tablet.selection {
                    let b = m.bounds;
                    sel.bounds = n0te_core::ink::Rect::new(ox + (b.x0 - ox) * k + dx, oy + (b.y0 - oy) * k + dy, ox + (b.x1 - ox) * k + dx, oy + (b.y1 - oy) * k + dy);
                }
            } else {
                self.reload_kept(m.pane, &sel.notebook, &m.sheet);
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
            // Scribbling over ink deletes what it covers (and isn't kept itself); colouring in
            // and hatching stay ink (core scribble).
            if d.brush.kind != n0te_core::ink::KIND_MARKER && !d.snapped && d.edge == 0 && self.scribble_away(&d) {
                return Task::none();
            }
            // Rested long enough before lifting: a shape or an arrowhead after all.
            let mut changed = d.snapped;
            let held = (d.started.elapsed().as_millis() as u32).saturating_sub(d.rest.since());
            // Only a pen resting before it lifts makes a shape or a head (else nothing to read);
            // never along the ruler.
            if !d.snapped && d.edge == 0 && d.points.len() > 4 && held > snap::LIFT_HOLD_MS {
                let zoom = pane_camera(&self.screen, d.pane).map(|c| c.zoom).unwrap_or(1.0);
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
            let (through_loupe, pane) = (d.map.is_some(), d.pane);
            self.tablet.drawing = Some(d);
            self.send_live(true);
            self.tablet.drawing = None;
            self.edit_tracked(&nb, &sheet, |s| {
                if !replaces.is_empty() {
                    s.remove(&replaces)?;
                }
                s.add_stroke(brush, &points).map(|_| ())
            });
            if through_loupe {
                self.loupe_after_stroke(&sheet, &points);
            } else {
                self.track_flow(pane, &sheet, &points);
            }
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
        if let Some(er) = self.tablet.erasing.take() {
            if let Screen::Notebook(split) = &mut self.screen
                && let Some(o) = split.pane_mut(er.pane)
            {
                o.pages.erasing.borrow_mut().remove(&er.sheet);
            }
            self.reload_kept(er.pane, &er.notebook, &er.sheet);
        }
        // A selection being moved stays where it was (its previews go, its strokes show again).
        if let Some(m) = self.tablet.moving.take() {
            self.end_move_preview(&m);
            if let Some(nb) = self.pane_notebook(m.pane) {
                self.reload_kept(m.pane, &nb, &m.sheet);
            }
        }
        self.tablet.lassoing = None;
        self.tablet.lasso_outline = None;
        self.extras_cancel();
    }

    /// The previews of a selection being moved go, its strokes show again where they are.
    fn end_move_preview(&mut self, m: &Moving) {
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
    }

    fn pane_notebook(&self, key: PaneKey) -> Option<String> {
        match &self.screen {
            Screen::Notebook(s) => s.pane(key).map(|o| o.id.clone()),
            Screen::Library => None,
        }
    }

    /// Notebook `nb` left the screen: this app's undo steps, the selection and the sheets kept
    /// open for it go (an undo, or Delete on a selection, must never change a notebook nobody
    /// sees; and its pages no longer stay in memory).
    pub(crate) fn tablet_forget(&mut self, nb: &str) {
        let t = &mut self.tablet;
        t.undo.retain(|e| e.notebook() != nb);
        t.redo.retain(|e| e.notebook() != nb);
        if t.selection.as_ref().is_some_and(|s| s.notebook == nb) {
            t.selection = None;
        }
        // Handwriting still to be read keeps its sheet (it is read once, then let go).
        let due: std::collections::HashSet<&(String, String)> = t.index_due.keys().collect();
        t.sheets.retain(|k, _| k.0 != nb || due.contains(k));
        if t.loupe.as_ref().is_some_and(|l| l.notebook == nb) {
            t.loupe = None;
        }
        t.flow = None;
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
    pub(crate) fn tablet_sheet(&mut self, nb: &str, sheet: &str) -> Option<&Sheet> {
        let k = (nb.to_string(), sheet.to_string());
        let stamp = self.lib.change_stamp(nb);
        if !self.tablet.sheets.contains_key(&k) {
            let s = self.lib.load_sheet(nb, sheet).ok()?;
            self.tablet.sheets.insert(k.clone(), (std::sync::Arc::new(s), stamp));
        }
        let (s, seen) = self.tablet.sheets.get_mut(&k)?;
        // Edits from elsewhere (sync, the other pane) come in first: read the file again only
        // when someone else saved the notebook since (a big page's file takes a while to read).
        if *seen != stamp && crate::saver::own_stamp(nb) != Some(stamp) {
            let _ = self.lib.refresh_sheet(nb, sheet, s);
        }
        *seen = stamp;
        Some(s)
    }

    /// Changes a sheet through its kept-open copy, stores it and remembers the edit for undo.
    pub(crate) fn edit_tracked(&mut self, nb: &str, sheet: &str, f: impl FnOnce(&Sheet) -> n0te_core::Result<()>) {
        if self.edit_kept(nb, sheet, f) {
            self.remember(Edit::Sheet(nb.to_string(), sheet.to_string()));
        }
    }

    /// Changes a sheet through its kept-open copy and stores it (undo is the caller's): whether
    /// it changed.
    pub(crate) fn edit_kept(&mut self, nb: &str, sheet: &str, f: impl FnOnce(&Sheet) -> n0te_core::Result<()>) -> bool {
        if self.read_only_nb(nb) {
            self.say(crate::i18n::tr("Nur lesen: ändern kann, wer dich eingeladen hat"));
            return false;
        }
        let Some(s) = self.tablet_sheet(nb, sheet) else {
            self.say(crate::i18n::tr("Seite nicht lesbar"));
            return false;
        };
        if let Err(e) = f(s) {
            self.say(crate::i18n::trf("Nicht geändert: {}", &[&e]));
            return false;
        }
        self.store_kept(nb, sheet);
        self.tablet.index_due.insert((nb.to_string(), sheet.to_string()), std::time::Instant::now());
        true
    }

    /// An edit to undo later (what was undone can't be redone any more).
    pub(crate) fn remember(&mut self, e: Edit) {
        self.tablet.undo.push(e);
        self.tablet.redo.clear();
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

    /// Pane `pane` shows sheet `sheet` again as it is (kept open here: from memory, its file
    /// may still be on its way).
    fn reload_kept(&mut self, pane: PaneKey, nb: &str, sheet: &str) {
        let kept = self.tablet.sheets.get(&(nb.to_string(), sheet.to_string())).map(|k| k.0.clone());
        if let Screen::Notebook(split) = &mut self.screen
            && let Some(o) = split.pane_mut(pane)
        {
            match kept {
                Some(s) => o.pages.reload_sheet_from(sheet, &s),
                None => o.pages.reload_sheet(sheet),
            }
        }
    }

    /// Stores a kept-open sheet (it syncs from there; the file is written in the background)
    /// and redraws the panes showing it (from memory, at once).
    fn store_kept(&mut self, nb: &str, sheet: &str) {
        let Some((s, _)) = self.tablet.sheets.get(&(nb.to_string(), sheet.to_string())) else { return };
        let s = s.clone();
        crate::saver::save(self.root.clone(), nb, sheet, s.clone());
        self.redraw_sheet(nb, sheet, &s);
    }

    /// A kept-open sheet was stored: the panes show what the save merged from disk (others'
    /// edits), and a failed save is told.
    pub(crate) fn on_saved(&mut self, saved: crate::saver::Saved) {
        if let Some(e) = &saved.error {
            self.say(crate::i18n::trf("Speichern fehlgeschlagen: {}", &[e]));
        }
        let k = (saved.notebook.clone(), saved.sheet.clone());
        if let Some((s, seen)) = self.tablet.sheets.get_mut(&k) {
            if let Some(stamp) = crate::saver::own_stamp(&saved.notebook) {
                *seen = stamp;
            }
            let s = s.clone();
            if let Screen::Notebook(split) = &mut self.screen {
                for o in split.panes.iter_mut().filter(|o| o.id == saved.notebook) {
                    o.pages.reload_sheet_from(&saved.sheet, &s);
                }
            }
        }
    }

    /// Undo (or redo) this app's last edit, on whatever page it was.
    pub(crate) fn tablet_undo(&mut self, undo: bool) {
        let entry = if undo { self.tablet.undo.pop() } else { self.tablet.redo.pop() };
        let Some(entry) = entry else { return };
        self.tablet.selection = None;
        let (nb, sheet, moved) = match &entry {
            Edit::Sheet(nb, sheet) => (nb.clone(), sheet.clone(), true),
            Edit::Space(nb, sheet, moved, ..) => (nb.clone(), sheet.clone(), *moved),
        };
        if moved {
            let Some(s) = self.tablet_sheet(&nb, &sheet) else { return };
            match if undo { s.undo() } else { s.redo() } {
                Ok(true) => self.store_kept(&nb, &sheet),
                Ok(false) => return,
                Err(e) => {
                    self.say(crate::i18n::trf("Rückgängig ging nicht: {}", &[&e]));
                    return;
                }
            }
        }
        // Space: the page takes its shape back (or again).
        if let Edit::Space(_, _, _, node, before, after) = &entry {
            if undo { self.shape_page(&nb, node, after, before) } else { self.shape_page(&nb, node, before, after) }
        }
        if undo { self.tablet.redo.push(entry) } else { self.tablet.undo.push(entry) }
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
        if let Screen::Notebook(split) = &mut self.screen
            && let Some(o) = split.pane_mut(d.pane)
        {
            let mut live = o.pages.live.borrow_mut();
            let l = live.entry((LOCAL.to_string(), d.live_id)).or_default();
            if l.sheet_id != d.sheet {
                l.sheet_id = d.sheet.clone();
            }
            l.brush = d.brush;
            l.points.clear();
            l.points.extend_from_slice(&d.points);
        }
    }

    /// The strokes near the one being drawn as lines for arrowheads (a head is at most 120 pt
    /// big and reaches as far again): none when it can't become a head (too long, not the pen).
    fn drawing_lines(&mut self, d: &mut Drawing) -> (Vec<String>, Vec<n0te_core::arrows::LineStroke>) {
        if d.points.len() > snap::MAX_HEAD_SAMPLES || !self.tablet.snap_options().pen {
            return Default::default();
        }
        let b = d.points.iter().fold(n0te_core::ink::Rect::EMPTY, |r, p| r.include(p.x, p.y)).inflate(140.0);
        if d.lines.as_ref().is_none_or(|(area, _)| !(area.x0 <= b.x0 && area.y0 <= b.y0 && area.x1 >= b.x1 && area.y1 >= b.y1)) {
            let area = b.inflate(60.0);
            let strokes = self.tablet_sheet(&d.notebook, &d.sheet).map(|s| s.strokes_in(Some(area))).unwrap_or_default();
            d.lines = Some((area, strokes.iter().map(|s| (s.id.clone(), n0te_core::arrows::LineStroke::of(s))).unzip()));
        }
        d.lines.as_ref().map(|l| l.1.clone()).unwrap_or_default()
    }

    /// Every frame while the pen draws: the hold timers (a shape after 0.5 s at rest, an arrow
    /// after 0.9 s), as on the tablet.
    pub(crate) fn tablet_frame(&mut self) {
        self.loupe_timer();
        self.flow_timer();
        let Some(mut d) = self.tablet.drawing.take() else { return };
        if !d.snapped && d.edge == 0 && d.points.len() >= 5 {
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
        self.tablet.drawing.as_ref().is_some_and(|d| !d.snapped) || self.tablet.loupe.as_ref().is_some_and(|l| l.advance_at.is_some()) || self.tablet.flow.as_ref().is_some_and(|f| f.due.is_some())
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
