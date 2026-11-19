//! The application: the library (notebooks by folder, import of `.n0te` bundles) and an open
//! notebook (outline sidebar, page canvas, top and status bar).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{Space, button, column, container, mouse_area, pin, row, scrollable, stack, text, text_editor, text_input};
use iced::{Alignment, Element, Length, Padding, Subscription, Task, Theme};

use n0te_core::layout as nl;
use n0te_core::notebook::{Background, Notebook, OutlineRow, SheetSpec};
use n0te_core::sheet::Sheet;
use n0te_core::store::Library;

use crate::pages::{self, Camera, Pages, SheetContent};
use crate::pdf::{self, Pdf};
use crate::live::{self, LiveEvent};
use crate::palette::{self, Command};
use crate::settings::Settings;
use crate::{library, sidebar, theme};

/// A notebook in the library list.
#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub folder: String,
    pub sheets: usize,
    pub cover: String,
    /// When the notebook last changed on disk.
    pub modified: Option<std::time::SystemTime>,
}

/// A `.n0te` file waiting in Downloads (sent from the tablet through n0bridge).
#[derive(Debug, Clone)]
pub struct Bundle {
    pub path: PathBuf,
    pub modified: SystemTime,
}

#[derive(Debug, Clone)]
pub enum Message {
    Open(String),
    Back,
    Refresh,
    Camera(Camera),
    Fold(String),
    Jump(String),
    Zoom(f32),
    ToggleSidebar,
    Key(keyboard::Event),
    ShowImports(bool),
    Import(PathBuf),
    SendToTablet,
    Sent(Result<String, String>),
    StartCreate,
    NewPaper(Background),
    NewSpacing(f32),
    NewFormat(usize),
    NewTitle(String),
    Create,
    CancelCreate,
    DismissStatus,
    /// A PDF tile arrived (redraw).
    TileReady,
    /// A LaTeX formula finished rendering (redraw).
    FormulaReady,
    /// Show the other device's view as a red frame.
    RemoteFrame(bool),
    FileDropped(PathBuf),
    ShowPdfImport(bool),
    ImportPdf(PathBuf),
    ShowExport(bool),
    ExportMode(ExportWhat),
    Export(ExportWhat),
    TogglePick(String),
    OpenPath(PathBuf),
    /// Text boxes: new at (sheet, x, y), edit (sheet, box), typing, move/resize drags, size,
    /// delete, done; a click on the canvas ends editing; the tick saves after a pause.
    NewText(String, f32, f32),
    EditText(String, String),
    EditAction(text_editor::Action),
    /// Text pages: open (sheet, docked as the notes column), close, block editor messages,
    /// a new text page after the current one, the notes column on/off.
    OpenTextPage(String, bool),
    CloseTextPage,
    Blocks(crate::blocks::Msg),
    NewTextPage,
    ToggleNotes,
    /// Sync service events; live session (M4); pairing; sharing with guests.
    Live(LiveEvent),
    StartLive,
    StopLive,
    FollowThem(bool),
    TheyFollow(bool),
    PairStart,
    PairToTablet,
    PairSent(Result<String, String>),
    ShowShare(bool),
    /// Version history of the open notebook: (unix time, label) per checkpoint.
    ShowHistory(bool),
    Restore(u64),
    Invite(bool, u64),
    Revoke(String),
    CopyText(String),
    StartJoin,
    CancelJoin,
    JoinLink(String),
    Join,
    /// Command palette, help, settings.
    OpenPalette,
    PaletteQuery(String),
    PaletteMove(i32),
    PaletteRun(Option<usize>),
    ShowHelp(bool),
    ShowSettings(bool),
    SetTheme(&'static str),
    SetRowGap(f32),
    FormulaCenter(bool),
    SetSideGap(f32),
    MoveText(iced::Vector),
    ResizeText(iced::Vector),
    PlaceText,
    TextSize(f32),
    DeleteText,
    DoneText,
    Tick,
}

/// The text box being edited: its sheet stays open while typing.
pub struct Editing {
    pub sheet_id: String,
    pub box_id: String,
    pub sheet: Sheet,
    pub content: text_editor::Content,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub size: f32,
    pub dirty: bool,
    pub last_change: Instant,
}

/// A live session with another own device.
#[derive(Debug, Clone)]
pub struct LiveSession {
    pub device: String,
    pub name: String,
    /// This view follows the other device's view.
    pub follow_them: bool,
    /// The other device follows this view.
    pub they_follow: bool,
}

/// A text page being edited (full page, or docked next to the slide as notes).
pub struct TextPage {
    pub sheet_id: String,
    /// The main page the notes belong to (docked only).
    pub main_node: Option<String>,
    pub sheet: Sheet,
    pub doc: crate::blocks::Doc,
    pub docked: bool,
    pub saved: String,
    pub dirty: bool,
    pub last_change: Instant,
}

pub fn editor_id() -> iced::widget::Id {
    iced::widget::Id::new("n0te-text-box")
}

/// What to export as PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportWhat {
    All,
    Current,
    Picked,
}

pub struct Open {
    pub id: String,
    pub nb: Notebook,
    pub rows: Vec<OutlineRow>,
    pub pages: Pages,
    pub camera: Camera,
}

pub enum Screen {
    Library,
    Notebook(Box<Open>),
}

pub struct App {
    pub lib: Rc<Library>,
    pub root: PathBuf,
    /// PDF rendering (None if pdfium is missing).
    pub pdf: Option<Arc<Pdf>>,
    pub show_pdf_import: bool,
    pub pdfs: Vec<(PathBuf, SystemTime)>,
    /// Export panel open, with the main sheets picked for "Auswahl".
    pub export: Option<HashSet<String>>,
    /// The scope chosen in the export dialog.
    pub export_mode: ExportWhat,
    pub last_export: Option<PathBuf>,
    /// Where exported PDFs go (~/Downloads).
    pub export_dir: PathBuf,
    pub editing: Option<Editing>,
    pub text_page: Option<TextPage>,
    pub settings: Settings,
    /// Only the real app writes the settings file (tests don't touch ~/.config).
    pub persist_settings: bool,
    /// Command palette: query and selected row.
    pub palette: Option<(String, usize)>,
    pub show_help: bool,
    pub show_settings: bool,
    /// The sync service connection (None: not running).
    pub sync: Option<Arc<live::Session>>,
    pub devices: Vec<live::Device>,
    devices_at: Option<Instant>,
    pub live_session: Option<LiveSession>,
    presence_at: Option<Instant>,
    pub pair_code: Option<String>,
    /// Share panel open, with the invites of the open notebook.
    pub share: Option<Vec<live::Invite>>,
    /// The newest invite link as a QR code (the view borrows it).
    pub share_qr: Option<iced::widget::qr_code::Data>,
    /// The pairing code as a QR code (the tablet scans it; no n0bridge needed).
    pub pair_qr: Option<iced::widget::qr_code::Data>,
    pub joining: Option<String>,
    /// History panel open: checkpoints (unix time, "date  size").
    pub history: Option<Vec<(u64, String)>>,
    /// Change stamp of the open notebook (saves by other processes on this machine).
    stamp: u64,
    /// Without a service connection (tests): don't try to connect.
    pub offline: bool,
    pub entries: Vec<Entry>,
    pub bundles: Vec<Bundle>,
    pub screen: Screen,
    pub status: Option<String>,
    pub creating: Option<String>,
    /// Paper, spacing (pt) and format (index into [`FORMATS`]) of the notebook being created.
    pub new_paper: Background,
    pub new_spacing: f32,
    pub new_format: usize,
    pub show_imports: bool,
    pub sidebar: bool,
}

/// The library folder: `$XDG_DATA_HOME/n0te` (~/.local/share/n0te).
pub fn default_root() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("n0te")
}

/// Where n0bridge drops received files, and Downloads itself.
fn bundle_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    vec![home.join("Downloads/n0bridge"), home.join("Downloads")]
}

pub fn find_bundles(dirs: &[PathBuf]) -> Vec<Bundle> {
    let mut out: Vec<Bundle> = dirs
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "n0te"))
        .filter_map(|e| Some(Bundle { modified: e.metadata().ok()?.modified().ok()?, path: e.path() }))
        .collect();
    out.sort_by(|a, b| b.modified.cmp(&a.modified));
    out.truncate(30);
    out
}

impl App {
    /// The app with default settings (tests); the real app loads them ([`App::boot_with`]).
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self::with_settings(root, Settings::default())
    }

    pub fn with_settings(root: impl AsRef<Path>, settings: Settings) -> Self {
        theme::set_theme(&settings.theme);
        crate::settings::FORMULA_CENTER.store(settings.formula_center, std::sync::atomic::Ordering::Relaxed);
        let root = root.as_ref().to_path_buf();
        let lib = Rc::new(Library::open(&root).expect("library folder"));
        let pdf = match Pdf::start(&root) {
            Ok(p) => Some(p),
            Err(e) => {
                eprintln!("n0te: no PDF rendering: {e}");
                None
            }
        };
        let mut app = App {
            lib,
            root,
            pdf,
            show_pdf_import: false,
            pdfs: Vec::new(),
            export: None,
            export_mode: ExportWhat::All,
            last_export: None,
            export_dir: downloads(),
            editing: None,
            text_page: None,
            settings,
            persist_settings: false,
            palette: None,
            show_help: false,
            show_settings: false,
            sync: None,
            devices: Vec::new(),
            devices_at: None,
            live_session: None,
            presence_at: None,
            pair_code: None,
            share: None,
            share_qr: None,
            pair_qr: None,
            joining: None,
            history: None,
            stamp: 0,
            offline: true,
            entries: Vec::new(),
            bundles: Vec::new(),
            screen: Screen::Library,
            status: None,
            creating: None,
            new_paper: Background::Grid,
            new_spacing: 17.0,
            new_format: 0,
            show_imports: false,
            sidebar: true,
        };
        app.refresh();
        app
    }

    /// Starts the app; `import` = a `.n0te` file given on the command line (opened after import).
    pub fn boot_with(root: PathBuf, import: Option<PathBuf>) -> (Self, Task<Message>) {
        let mut app = Self::with_settings(root, Settings::load());
        app.persist_settings = true;
        app.offline = false;
        app.connect_sync();
        let task = import.map(|p| Task::done(Message::Import(p))).unwrap_or_else(Task::none);
        (app, task)
    }

    pub fn theme(&self) -> Theme {
        theme::theme()
    }

    pub fn title(&self) -> String {
        match &self.screen {
            Screen::Library => "n0te".into(),
            Screen::Notebook(o) => format!("n0te — {}", o.nb.title()),
        }
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            keyboard::listen().map(Message::Key),
            Pdf::subscription().map(|_| Message::TileReady),
            crate::latex::subscription().map(|_| Message::FormulaReady),
            Subscription::run(ticks).map(|_| Message::Tick),
            live::subscription().map(Message::Live),
            iced::event::listen_with(|e, _, _| match e {
                iced::Event::Window(iced::window::Event::FileDropped(p)) => Some(Message::FileDropped(p)),
                _ => None,
            }),
        ])
    }

    fn refresh(&mut self) {
        let lib = &self.lib;
        let mut entries: Vec<Entry> = lib
            .notebook_ids()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| {
                let nb = lib.load_notebook(&id).ok()?;
                let modified = std::fs::metadata(lib.root().join("notebooks").join(&id)).and_then(|m| m.modified()).ok();
                Some(Entry { title: nb.title(), folder: nb.folder(), sheets: nb.sheet_count(), cover: nb.cover(), modified, id })
            })
            .collect();
        entries.sort_by(|a, b| (a.folder.to_lowercase(), a.title.to_lowercase()).cmp(&(b.folder.to_lowercase(), b.title.to_lowercase())));
        self.entries = entries;
        self.bundles = find_bundles(&bundle_dirs());
    }

    fn open(&mut self, id: &str) -> Result<(), String> {
        let nb = self.lib.load_notebook(id).map_err(|e| e.to_string())?;
        let rows = nb.outline(false);
        let layout = nl::layout(&rows, &nb.outline(true), self.settings.layout());
        let lib = self.lib.clone();
        let nid = id.to_string();
        let pages = Pages::new(
            layout,
            Box::new(move |sheet| {
                lib.load_sheet(&nid, sheet)
                    .map(|s| SheetContent {
                        strokes: s.strokes(),
                        texts: s.text_boxes(),
                        body: s.page_text(),
                        images: s.images().into_iter().filter_map(|i| lib.blob_path(&i.blob).ok().map(|p| (i, p))).collect(),
                    })
                    .unwrap_or_default()
            }),
            self.pdf.clone(),
        );
        pages.show_remote_frame.set(self.settings.remote_frame);
        self.finish_text();
        self.close_text_page();
        self.stamp = self.lib.change_stamp(id);
        self.screen = Screen::Notebook(Box::new(Open { id: id.to_string(), nb, rows, pages, camera: Camera::default() }));
        Ok(())
    }

    fn say(&mut self, s: impl Into<String>) {
        self.status = Some(s.into());
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Open(id) => {
                if let Err(e) = self.open(&id) {
                    self.say(format!("Notizbuch nicht lesbar: {e}"));
                }
            }
            Message::Back => {
                self.finish_text();
                self.close_text_page();
                self.live_session = None;
                self.share = None;
                self.history = None;
                self.screen = Screen::Library;
                self.refresh();
            }
            Message::Refresh => self.refresh(),
            Message::Camera(c) => {
                if let Screen::Notebook(o) = &mut self.screen {
                    o.camera = c;
                }
                self.follow_notes();
                self.send_presence();
            }
            Message::Fold(node) => {
                let lib = self.lib.clone();
                if let Screen::Notebook(o) = &mut self.screen {
                    let collapsed = o.rows.iter().any(|r| matches!(r, OutlineRow::Group { node: n, collapsed: true, .. } if *n == node));
                    if o.nb.set_collapsed(&node, !collapsed).is_ok() {
                        let _ = lib.save_notebook(&o.nb);
                        o.rows = o.nb.outline(false);
                        o.pages.relayout(nl::layout(&o.rows, &o.nb.outline(true), self.settings.layout()));
                        o.camera = o.pages.clamp(o.camera);
                    }
                }
            }
            Message::Jump(sheet) => {
                if let Screen::Notebook(o) = &mut self.screen
                    && let Some(i) = o.pages.page_of_sheet(&sheet)
                {
                    o.camera = o.pages.camera_for(i, &o.camera);
                }
            }
            Message::Zoom(z) => {
                if let Screen::Notebook(o) = &mut self.screen {
                    let c = o.camera;
                    let (cx, cy) = (c.x + c.viewport.width / c.zoom / 2.0, c.y + c.viewport.height / c.zoom / 2.0);
                    let zoom = z.clamp(pages::MIN_ZOOM, pages::MAX_ZOOM);
                    o.camera = o.pages.clamp(Camera { x: cx - c.viewport.width / zoom / 2.0, y: cy - c.viewport.height / zoom / 2.0, zoom, ..c });
                }
            }
            Message::ToggleSidebar => self.sidebar = !self.sidebar,
            Message::Key(keyboard::Event::KeyPressed { key, modifiers, .. }) => return self.shortcut(key, modifiers),
            Message::Key(_) => {}
            Message::ShowImports(on) => {
                self.show_imports = on;
                if on {
                    self.bundles = find_bundles(&bundle_dirs());
                }
            }
            Message::Import(path) => match std::fs::read(&path).map_err(|e| e.to_string()).and_then(|b| self.lib.import_bundle(&b).map_err(|e| e.to_string())) {
                Ok(id) => {
                    self.show_imports = false;
                    self.refresh();
                    let title = self.entries.iter().find(|e| e.id == id).map(|e| e.title.clone()).unwrap_or_default();
                    let reopen = matches!(&self.screen, Screen::Notebook(o) if o.id == id) || matches!(self.screen, Screen::Library);
                    if reopen {
                        let _ = self.open(&id);
                    }
                    self.say(format!("„{title}“ übernommen (mit dem Stand hier zusammengeführt)"));
                }
                Err(e) => self.say(format!("Import fehlgeschlagen: {e}")),
            },
            Message::SendToTablet => {
                let Screen::Notebook(o) = &self.screen else { return Task::none() };
                let dir = std::env::temp_dir().join("n0te-out");
                let safe: String = o.nb.title().chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
                let path = dir.join(format!("{}.n0te", safe.trim().replace(' ', "_")));
                let bytes = match self.lib.export_bundle(&o.id) {
                    Ok(b) => b,
                    Err(e) => {
                        self.say(format!("Export fehlgeschlagen: {e}"));
                        return Task::none();
                    }
                };
                if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, bytes)) {
                    self.say(format!("Export fehlgeschlagen: {e}"));
                    return Task::none();
                }
                self.say("Sende an das Tablet …");
                return Task::perform(send_with_n0bridge(path), Message::Sent);
            }
            Message::Sent(r) => self.say(match r {
                Ok(_) => "An das Tablet gesendet — dort in n0te: IMPORTIEREN".to_string(),
                Err(e) => format!("Senden fehlgeschlagen: {e}"),
            }),
            Message::StartCreate => self.creating = Some(String::new()),
            Message::NewPaper(b) => {
                if b != self.new_paper {
                    self.new_paper = b;
                    self.new_spacing = default_spacing(b);
                }
            }
            Message::NewSpacing(v) => self.new_spacing = v,
            Message::NewFormat(i) => self.new_format = i.min(FORMATS.len() - 1),
            Message::NewTitle(t) => self.creating = Some(t),
            Message::CancelCreate => self.creating = None,
            Message::Create => {
                let title = self.creating.take().unwrap_or_default();
                let title = if title.trim().is_empty() { "Neues Notizbuch".to_string() } else { title.trim().to_string() };
                let made = self.lib.create_notebook(&title).and_then(|nb| {
                    let (_, w, h) = FORMATS[self.new_format];
                    nb.add_sheet(None, 0, &SheetSpec { width: w, height: h, background: self.new_paper, spacing: self.new_spacing, pdf: None })?;
                    self.lib.save_notebook(&nb)?;
                    Ok(nb.id())
                });
                match made {
                    Ok(id) => {
                        self.refresh();
                        let _ = self.open(&id);
                    }
                    Err(e) => self.say(format!("Anlegen fehlgeschlagen: {e}")),
                }
            }
            Message::DismissStatus => self.status = None,
            Message::TileReady => {}
            Message::FileDropped(p) => {
                let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                return match ext.as_str() {
                    "n0te" => self.update(Message::Import(p)),
                    "pdf" => self.update(Message::ImportPdf(p)),
                    _ => {
                        self.say("Nur PDF- und .n0te-Dateien");
                        Task::none()
                    }
                };
            }
            Message::ShowPdfImport(on) => {
                self.show_pdf_import = on;
                if on {
                    self.pdfs = pdf::recent_pdfs();
                }
            }
            Message::ImportPdf(path) => {
                self.show_pdf_import = false;
                self.import_pdf(&path);
            }
            Message::ShowExport(on) => {
                self.export = on.then(HashSet::new);
                self.export_mode = ExportWhat::All;
            }
            Message::ExportMode(m) => self.export_mode = m,
            Message::TogglePick(node) => {
                if let Some(p) = &mut self.export
                    && !p.remove(&node)
                {
                    p.insert(node);
                }
            }
            Message::Export(what) => self.export_pdf(what),
            Message::OpenPath(p) => {
                let _ = std::process::Command::new("xdg-open").arg(p).spawn();
            }
            Message::NewText(sheet, x, y) => return self.new_text(&sheet, x, y),
            Message::EditText(sheet, id) => return self.edit_text(&sheet, &id),
            Message::FormulaReady => {}
            Message::EditAction(a) => {
                if let Some(e) = &mut self.editing {
                    let edit = a.is_edit();
                    e.content.perform(a);
                    if edit {
                        e.dirty = true;
                        e.last_change = Instant::now();
                    }
                }
            }
            Message::MoveText(d) | Message::ResizeText(d) => {
                let zoom = match &self.screen {
                    Screen::Notebook(o) => o.camera.zoom,
                    _ => 1.0,
                };
                if let Some(e) = &mut self.editing {
                    if matches!(message, Message::MoveText(_)) {
                        e.x += d.x / zoom;
                        e.y += d.y / zoom;
                    } else {
                        e.w = (e.w + d.x / zoom).max(40.0);
                    }
                }
            }
            Message::PlaceText => self.save_text(true),
            Message::TextSize(d) => {
                if let Some(e) = &mut self.editing {
                    e.size = (e.size + d).clamp(6.0, 72.0);
                }
                self.save_text(true);
            }
            Message::DeleteText => {
                if let Some(e) = self.editing.take() {
                    let _ = e.sheet.remove(std::slice::from_ref(&e.box_id));
                    self.store_sheet(&e.sheet_id, &e.sheet);
                }
            }
            Message::DoneText => self.finish_text(),
            Message::Tick => {
                if self.editing.as_ref().is_some_and(|e| e.dirty && e.last_change.elapsed().as_millis() > 800) {
                    self.save_text(false);
                }
                if self.text_page.as_ref().is_some_and(|t| t.dirty && t.last_change.elapsed().as_millis() > 800) {
                    self.save_text_page();
                }
                if !self.offline {
                    if self.sync.is_none() {
                        self.connect_sync();
                    } else if self.devices_at.is_none_or(|t| t.elapsed().as_secs() >= 10) {
                        self.devices = live::devices();
                        self.devices_at = Some(Instant::now());
                    }
                }
                // Saves by another process here (the sync service, a second window).
                if let Screen::Notebook(o) = &self.screen {
                    let now = self.lib.change_stamp(&o.id);
                    if now != self.stamp {
                        self.stamp = now;
                        self.refresh_open(true, &[]);
                    }
                }
            }
            Message::Live(e) => return self.on_live(e),
            Message::StartLive => {
                let Some(d) = self.devices.iter().find(|d| d.online).cloned() else {
                    self.say("Kein Gerät online: Tablet koppeln (Einstellungen) und n0te dort öffnen");
                    return Task::none();
                };
                let Some(nb) = self.notebook_id() else { return Task::none() };
                if let Some(s) = &self.sync {
                    s.send(&["open".into(), d.id.clone(), nb]);
                }
                self.live_session = Some(LiveSession { device: d.id, name: d.name.clone(), follow_them: false, they_follow: false });
                self.presence_at = None;
                self.send_presence();
                self.say(format!("Live mit {}: es öffnet das Notizbuch", d.name));
            }
            Message::StopLive => {
                if let (Some(l), Some(s)) = (&self.live_session, &self.sync)
                    && l.they_follow
                {
                    s.send(&["follow".into(), l.device.clone(), "0".into()]);
                }
                self.live_session = None;
                if let Screen::Notebook(o) = &self.screen {
                    o.pages.remote.borrow_mut().clear();
                }
            }
            Message::RemoteFrame(on) => {
                self.settings.remote_frame = on;
                if let Screen::Notebook(o) = &self.screen {
                    o.pages.show_remote_frame.set(on);
                }
                if self.persist_settings {
                    self.settings.save();
                }
            }
            Message::FollowThem(on) => {
                if let Some(l) = &mut self.live_session {
                    // One direction at a time: following them ends them following us.
                    if on && l.they_follow {
                        l.they_follow = false;
                        if let Some(s) = &self.sync {
                            s.send(&["follow".into(), l.device.clone(), "0".into()]);
                        }
                    }
                    l.follow_them = on;
                }
            }
            Message::TheyFollow(on) => {
                if let Some(l) = &mut self.live_session {
                    l.they_follow = on;
                    if on {
                        l.follow_them = false;
                    }
                    if let Some(s) = &self.sync {
                        s.send(&["follow".into(), l.device.clone(), if on { "1".into() } else { "0".into() }]);
                    }
                }
                self.presence_at = None;
                self.send_presence();
            }
            Message::PairStart => match live::request(&["pair-code"]) {
                Ok(code) => {
                    self.pair_qr = iced::widget::qr_code::Data::new(code.as_bytes()).ok();
                    self.pair_code = Some(code);
                }
                Err(e) => self.say(format!("Koppeln: {e}")),
            },
            Message::PairToTablet => {
                let Some(code) = self.pair_code.clone() else { return Task::none() };
                return Task::perform(
                    async move {
                        let to = tablet_target();
                        n0bridge(vec!["clipboard".into(), "set".into(), "--to".into(), to, code]).await
                    },
                    Message::PairSent,
                );
            }
            Message::PairSent(r) => self.say(match r {
                Ok(_) => "Code ist in der Tablet-Zwischenablage: dort n0te → Einstellungen → LAPTOP KOPPELN".to_string(),
                Err(e) => format!("Senden fehlgeschlagen: {e}"),
            }),
            Message::ShowShare(on) => {
                self.share = if on {
                    let nb = self.notebook_id().unwrap_or_default();
                    Some(live::invites().into_iter().filter(|i| i.notebook == nb).collect())
                } else {
                    None
                };
                self.share_qr = self.share.as_ref().and_then(|v| v.iter().rev().find(|i| !i.revoked)).and_then(|i| iced::widget::qr_code::Data::new(i.link.as_bytes()).ok());
            }
            Message::ShowHistory(on) => {
                self.history = if on {
                    let nb = self.notebook_id().unwrap_or_default();
                    match live::request(&["history", &nb]) {
                        Ok(out) => Some(
                            out.lines()
                                .filter_map(|l| {
                                    let mut it = l.splitn(2, ' ');
                                    let at = it.next()?.parse::<u64>().ok()?;
                                    Some((at, it.next().unwrap_or_default().trim().to_string()))
                                })
                                .collect(),
                        ),
                        Err(e) => {
                            self.say(format!("Verlauf: {e}"));
                            None
                        }
                    }
                } else {
                    None
                };
            }
            Message::Restore(at) => {
                let nb = self.notebook_id().unwrap_or_default();
                match live::request(&["restore", &nb, &at.to_string()]) {
                    Ok(out) => {
                        self.history = None;
                        let id = out.rsplit(' ').next().unwrap_or_default().to_string();
                        self.refresh();
                        if self.entries.iter().any(|e| e.id == id) {
                            let _ = self.open(&id);
                        }
                        self.say("Früherer Stand als neues Notizbuch wiederhergestellt (das aktuelle bleibt unverändert)");
                    }
                    Err(e) => self.say(format!("Wiederherstellen: {e}")),
                }
            }
            Message::Invite(rw, days) => {
                let Some(nb) = self.notebook_id() else { return Task::none() };
                let days = days.to_string();
                let mut args = vec!["invite", nb.as_str()];
                if rw {
                    args.push("--rw");
                }
                if days != "0" {
                    args.push("--days");
                    args.push(&days);
                }
                match live::request(&args) {
                    Ok(link) => {
                        self.say("Einladung erstellt und kopiert");
                        let _ = self.update(Message::ShowShare(true));
                        return iced::clipboard::write(link);
                    }
                    Err(e) => self.say(format!("Einladen: {e}")),
                }
            }
            Message::Revoke(id) => {
                match live::request(&["revoke", &id]) {
                    Ok(_) => self.say("Widerrufen: der Gast ist getrennt"),
                    Err(e) => self.say(format!("Widerrufen: {e}")),
                }
                return self.update(Message::ShowShare(true));
            }
            Message::CopyText(t) => {
                self.say("Kopiert");
                return iced::clipboard::write(t);
            }
            Message::StartJoin => self.joining = Some(String::new()),
            Message::CancelJoin => self.joining = None,
            Message::JoinLink(l) => self.joining = Some(l),
            Message::Join => {
                let link = self.joining.take().unwrap_or_default();
                match live::request(&["join", link.trim()]) {
                    Ok(out) => {
                        self.refresh();
                        let id = out.rsplit(' ').next().unwrap_or_default().to_string();
                        if self.entries.iter().any(|e| e.id == id) {
                            let _ = self.open(&id);
                        }
                        self.say("Beigetreten");
                    }
                    Err(e) => self.say(format!("Beitreten: {e}")),
                }
            }
            Message::OpenPalette => {
                self.palette = Some((String::new(), 0));
                return iced::widget::operation::focus(palette::input_id());
            }
            Message::PaletteQuery(q) => self.palette = Some((q, 0)),
            Message::PaletteMove(d) => {
                let n = self.palette_items().len().max(1) as i32;
                if let Some((_, sel)) = &mut self.palette {
                    *sel = ((*sel as i32 + d).rem_euclid(n)) as usize;
                }
            }
            Message::PaletteRun(i) => {
                let Some((_, sel)) = self.palette.clone() else { return Task::none() };
                let items = self.palette_items();
                let Some(it) = items.get(i.unwrap_or(sel)).cloned() else { return Task::none() };
                self.palette = None;
                return self.run(it.cmd);
            }
            Message::ShowHelp(on) => self.show_help = on,
            Message::ShowSettings(on) => self.show_settings = on,
            Message::SetTheme(id) => {
                self.settings.theme = id.to_string();
                if self.persist_settings { self.settings.save(); }
                theme::set_theme(id);
                self.reopen();
            }
            Message::FormulaCenter(on) => {
                self.settings.formula_center = on;
                crate::settings::FORMULA_CENTER.store(on, std::sync::atomic::Ordering::Relaxed);
                if self.persist_settings {
                    self.settings.save();
                }
            }
            Message::SetRowGap(v) => {
                self.settings.row_gap = v;
                if self.persist_settings { self.settings.save(); }
                self.relayout();
            }
            Message::SetSideGap(v) => {
                self.settings.side_gap = v;
                if self.persist_settings { self.settings.save(); }
                self.relayout();
            }
            Message::OpenTextPage(sheet, docked) => return self.open_text_page(&sheet, docked),
            Message::CloseTextPage => self.close_text_page(),
            Message::Blocks(m) => {
                if let Some(t) = &mut self.text_page {
                    let task = t.doc.update(m).map(Message::Blocks);
                    if t.doc.to_markdown() != t.saved {
                        t.dirty = true;
                        t.last_change = Instant::now();
                    }
                    return task;
                }
            }
            Message::NewTextPage => {
                let Screen::Notebook(o) = &self.screen else { return Task::none() };
                let current = o.pages.current_number(&o.camera);
                let node = o.pages.index_of_number(current).map(|i| o.pages.pages[i].placed.node.clone());
                let (parent, index) = pdf::position_after(&o.nb, node.as_deref());
                let spec = SheetSpec::a4(Background::Text);
                match o.nb.add_sheet(parent.as_deref(), index, &spec) {
                    Ok((_, sheet)) => {
                        let _ = self.lib.save_notebook(&o.nb);
                        self.relayout();
                        if let Screen::Notebook(o) = &mut self.screen
                            && let Some(i) = o.pages.page_of_sheet(&sheet)
                        {
                            o.camera = o.pages.camera_for(i, &o.camera);
                        }
                        return self.open_text_page(&sheet, false);
                    }
                    Err(e) => self.say(format!("Textseite: {e}")),
                }
            }
            Message::ToggleNotes => {
                if self.text_page.as_ref().is_some_and(|t| t.docked) {
                    self.close_text_page();
                } else {
                    return self.open_notes(true);
                }
            }
        }
        Task::none()
    }

    fn connect_sync(&mut self) {
        if let Some(s) = live::Session::connect() {
            self.sync = Some(Arc::new(s));
            self.devices = live::devices();
            self.devices_at = Some(Instant::now());
        }
    }

    /// What the sync service reports.
    fn on_live(&mut self, e: LiveEvent) -> Task<Message> {
        match e {
            LiveEvent::Hello { .. } => {}
            LiveEvent::Changed { notebook, index, sheets } => {
                if self.notebook_id().as_deref() == Some(notebook.as_str()) {
                    self.refresh_open(index, &sheets);
                } else if matches!(self.screen, Screen::Library) {
                    self.refresh();
                }
            }
            LiveEvent::Online { .. } | LiveEvent::Offline { .. } | LiveEvent::Paired { .. } => {
                // Reconnected or gone: the other app may have restarted, so nobody follows anyone
                // any more (a stale follow on both sides made the views chase each other).
                if let LiveEvent::Online { id, .. } | LiveEvent::Offline { id } = &e
                    && let Some(l) = self.live_session.as_mut().filter(|l| &l.device == id)
                {
                    l.follow_them = false;
                    l.they_follow = false;
                }
                if let LiveEvent::Paired { name, .. } = &e {
                    self.pair_code = None;
                    self.pair_qr = None;
                    self.say(format!("Gekoppelt mit {name}"));
                }
                self.devices = live::devices();
                self.devices_at = Some(Instant::now());
            }
            LiveEvent::Presence { device, notebook, sheet, viewport, laser, .. } => {
                let Screen::Notebook(o) = &mut self.screen else { return Task::none() };
                if o.id != notebook {
                    return Task::none();
                }
                let name = self.devices.iter().find(|d| d.id == device).map(|d| d.name.clone()).unwrap_or_else(|| "Tablet".into());
                let Some(sheet) = sheet else { return Task::none() };
                let mark = crate::pages::RemoteMark { sheet_id: sheet.clone(), rect: n0te_core::ink::Rect::new(viewport[0], viewport[1], viewport[2], viewport[3]), laser: laser.map(|l| (l[0], l[1])), name };
                {
                    let mut marks = o.pages.remote.borrow_mut();
                    marks.retain(|m| m.name != mark.name);
                    marks.push(mark.clone());
                }
                // Following: show what the other device shows.
                if self.live_session.as_ref().is_some_and(|l| l.follow_them && l.device == device)
                    && let Some(page) = o.pages.pages.iter().find(|p| p.placed.sheet_id == sheet).map(|p| p.placed.rect)
                    && mark.rect.width() > 1.0
                {
                    let c = o.camera;
                    let zoom = (c.viewport.width / mark.rect.width()).clamp(pages::MIN_ZOOM, pages::MAX_ZOOM);
                    let (cx, cy) = (page.x0 + (mark.rect.x0 + mark.rect.x1) / 2.0, page.y0 + (mark.rect.y0 + mark.rect.y1) / 2.0);
                    o.camera = o.pages.clamp(Camera { x: cx - c.viewport.width / zoom / 2.0, y: cy - c.viewport.height / zoom / 2.0, zoom, ..c });
                }
            }
            LiveEvent::Stroke { device, notebook, sheet, stroke, brush, samples, done } => {
                if let Screen::Notebook(o) = &self.screen
                    && o.id == notebook
                {
                    o.pages.live_samples(&device, stroke, &sheet, brush, &samples, done);
                }
            }
            LiveEvent::Erase { notebook, sheet, ids, dabs } => {
                if let Screen::Notebook(o) = &self.screen
                    && o.id == notebook
                {
                    o.pages.live_erase(&sheet, &ids, &dabs);
                }
            }
            LiveEvent::Open { device, notebook } => {
                let name = self.devices.iter().find(|d| d.id == device).map(|d| d.name.clone()).unwrap_or_else(|| "Tablet".into());
                if self.entries.iter().any(|e| e.id == notebook) || self.lib.load_notebook(&notebook).is_ok() {
                    if self.notebook_id().as_deref() != Some(notebook.as_str()) {
                        let _ = self.open(&notebook);
                    }
                    // Keep who follows whom when the same device asks again (e.g. "Laptop folgt mir").
                    let (follow_them, they_follow) = self.live_session.as_ref().filter(|l| l.device == device).map(|l| (l.follow_them, l.they_follow)).unwrap_or((false, false));
                    self.live_session = Some(LiveSession { device, name: name.clone(), follow_them, they_follow });
                    // Where this view is, right away (the tablet may want to follow it).
                    self.presence_at = None;
                    self.send_presence();
                    self.say(format!("{name} hat dieses Notizbuch live geöffnet"));
                }
            }
            LiveEvent::Follow { device, on } => {
                if let Some(l) = self.live_session.as_mut().filter(|l| l.device == device) {
                    l.follow_them = on;
                    // They lead now (they stopped following us when they asked).
                    if on {
                        l.they_follow = false;
                    }
                } else if on {
                    let name = self.devices.iter().find(|d| d.id == device).map(|d| d.name.clone()).unwrap_or_else(|| "Tablet".into());
                    self.live_session = Some(LiveSession { device, name, follow_them: true, they_follow: false });
                }
            }
            LiveEvent::Guest { name, on, .. } => self.say(if on { format!("Gast verbunden: {name}") } else { "Gast getrennt".to_string() }),
            LiveEvent::Revoked { .. } => self.say("Die Einladung zu diesem Notizbuch wurde widerrufen"),
            LiveEvent::Error { message } => self.say(format!("Sync: {message}")),
            LiveEvent::Disconnected => self.sync = None,
        }
        Task::none()
    }

    /// Another device changed the open notebook: outline and/or sheets read again.
    fn refresh_open(&mut self, index: bool, sheets: &[String]) {
        let lib = self.lib.clone();
        let Screen::Notebook(o) = &mut self.screen else { return };
        if index || sheets.is_empty() {
            let _ = lib.refresh_notebook(&o.nb);
            o.rows = o.nb.outline(false);
            o.pages.relayout(nl::layout(&o.rows, &o.nb.outline(true), self.settings.layout()));
            o.camera = o.pages.clamp(o.camera);
        }
        let all: Vec<String> = if sheets.is_empty() { o.pages.pages.iter().map(|p| p.placed.sheet_id.clone()).collect() } else { sheets.to_vec() };
        for s in &all {
            o.pages.reload_sheet(s);
        }
        // An open text page that hasn't been typed in since: show the new text.
        let id = o.id.clone();
        if let Some(t) = &mut self.text_page
            && !t.dirty
            && all.contains(&t.sheet_id)
            && lib.refresh_sheet(&id, &t.sheet_id, &t.sheet).unwrap_or(false)
        {
            t.saved = t.sheet.page_text();
            t.doc = crate::blocks::Doc::from_markdown(&t.saved);
        }
        if let Some(e) = &self.editing
            && all.contains(&e.sheet_id)
        {
            let _ = lib.refresh_sheet(&id, &e.sheet_id, &e.sheet);
        }
    }

    /// Live: where this view is (sheet points of the page in the middle), at most 10× a second.
    fn send_presence(&mut self) {
        let (Some(_), Some(sync)) = (&self.live_session, &self.sync) else { return };
        if self.presence_at.is_some_and(|t| t.elapsed().as_millis() < 100) {
            return;
        }
        let Screen::Notebook(o) = &self.screen else { return };
        let c = o.camera;
        let (cx, cy) = (c.x + c.viewport.width / c.zoom / 2.0, c.y + c.viewport.height / c.zoom / 2.0);
        let Some(p) = o.pages.sheet_at(cx, cy).or_else(|| o.pages.pages.iter().map(|p| &p.placed).min_by(|a, b| (a.rect.y0 - cy).abs().total_cmp(&(b.rect.y0 - cy).abs()))) else { return };
        let v = c.view_rect();
        let r = [v.x0 - p.rect.x0, v.y0 - p.rect.y0, v.x1 - p.rect.x0, v.y1 - p.rect.y0];
        let args: Vec<String> = ["presence".to_string(), o.id.clone(), p.sheet_id.clone()].into_iter().chain(r.iter().map(|x| format!("{x:.1}"))).chain([format!("{:.3}", c.zoom)]).collect();
        sync.send(&args);
        self.presence_at = Some(Instant::now());
    }

    fn palette_items(&self) -> Vec<palette::Item> {
        palette::filter(palette::items(self), self.palette.as_ref().map(|(q, _)| q.as_str()).unwrap_or(""))
    }

    /// Runs a palette command.
    fn run(&mut self, cmd: Command) -> Task<Message> {
        match cmd {
            Command::NewNotebook => self.update(Message::StartCreate),
            Command::ImportPdf | Command::InsertPdf => self.update(Message::ShowPdfImport(true)),
            Command::ImportBundle => self.update(Message::ShowImports(true)),
            Command::OpenNotebook(id) => self.update(Message::Open(id)),
            Command::Library => self.update(Message::Back),
            Command::NewTextPage => self.update(Message::NewTextPage),
            Command::Notes => self.update(Message::ToggleNotes),
            Command::NewTextBox => {
                let ev = keyboard::Event::KeyPressed {
                    key: Key::Character("t".into()),
                    modified_key: Key::Character("t".into()),
                    physical_key: keyboard::key::Physical::Unidentified(keyboard::key::NativeCode::Unidentified),
                    location: keyboard::Location::Standard,
                    modifiers: keyboard::Modifiers::CTRL,
                    text: None,
                    repeat: false,
                };
                self.update(Message::Key(ev))
            }
            Command::Export => self.update(Message::ShowExport(true)),
            Command::SendToTablet => self.update(Message::SendToTablet),
            Command::ToggleSidebar => self.update(Message::ToggleSidebar),
            Command::Zoom(z) => self.update(Message::Zoom(z)),
            Command::GoTo(sheet) => self.update(Message::Jump(sheet)),
            Command::Theme(id) => self.update(Message::SetTheme(id)),
            Command::Settings => self.update(Message::ShowSettings(true)),
            Command::Help => self.update(Message::ShowHelp(true)),
            Command::History => self.update(Message::ShowHistory(true)),
            Command::Share => self.update(Message::ShowShare(true)),
            Command::Live => self.update(Message::StartLive),
        }
    }

    /// Opens the current notebook again (new theme: paper caches), keeping the camera.
    fn reopen(&mut self) {
        let Screen::Notebook(o) = &self.screen else { return };
        let (id, cam) = (o.id.clone(), o.camera);
        if self.open(&id).is_ok()
            && let Screen::Notebook(o) = &mut self.screen
        {
            o.camera = cam;
        }
    }

    /// The current main page's notes sheet (a text side sheet on the right), if any.
    fn notes_sheet(&self) -> Option<(String, String)> {
        let Screen::Notebook(o) = &self.screen else { return None };
        let current = o.pages.current_number(&o.camera);
        let main = o.pages.index_of_number(current).map(|i| o.pages.pages[i].placed.node.clone())?;
        let notes = o.pages.pages.iter().map(|p| &p.placed).filter(|p| p.main_node == main && p.side > 0 && p.spec.background == Background::Text).min_by_key(|p| p.side);
        Some((main, notes.map(|p| p.sheet_id.clone()).unwrap_or_default()))
    }

    /// Opens the notes column for the current page; with `create`, makes the notes sheet.
    fn open_notes(&mut self, create: bool) -> Task<Message> {
        let Some((main, mut sheet)) = self.notes_sheet() else { return Task::none() };
        if sheet.is_empty() {
            if !create {
                self.close_text_page();
                return Task::none();
            }
            let Screen::Notebook(o) = &self.screen else { return Task::none() };
            match o.nb.add_side_sheet(&main, n0te_core::notebook::Side::Right, 0, &SheetSpec::a4(Background::Text)) {
                Ok((_, s)) => {
                    let _ = self.lib.save_notebook(&o.nb);
                    sheet = s;
                    self.relayout();
                }
                Err(e) => {
                    self.say(format!("Notizen: {e}"));
                    return Task::none();
                }
            }
        }
        let task = self.open_text_page(&sheet, true);
        if let Some(t) = &mut self.text_page {
            t.main_node = Some(main);
        }
        task
    }

    /// The docked notes follow the page in view.
    fn follow_notes(&mut self) {
        let Some(t) = &self.text_page else { return };
        if !t.docked {
            return;
        }
        let Some((main, sheet)) = self.notes_sheet() else { return };
        if t.main_node.as_deref() == Some(main.as_str()) {
            return;
        }
        if sheet.is_empty() {
            // This page has no notes yet: keep the column, empty, until the user writes.
            self.save_text_page();
            if let Some(t) = &mut self.text_page {
                t.main_node = Some(main);
                t.sheet_id.clear();
                t.doc = crate::blocks::Doc::from_markdown("");
                t.saved.clear();
            }
        } else {
            let _ = self.open_text_page(&sheet, true);
            if let Some(t) = &mut self.text_page {
                t.main_node = Some(main);
            }
        }
    }

    fn open_text_page(&mut self, sheet_id: &str, docked: bool) -> Task<Message> {
        self.finish_text();
        self.close_text_page();
        let Some(nb) = self.notebook_id() else { return Task::none() };
        let Ok(sheet) = self.lib.load_sheet(&nb, sheet_id) else { return Task::none() };
        let md = sheet.page_text();
        let doc = crate::blocks::Doc::from_markdown(&md);
        // The cursor goes to the end of the last block.
        let end = doc.blocks.last().map(|b| {
            let t = b.text();
            let line = t.split('\n').count().saturating_sub(1);
            let column = t.split('\n').last().map(|l| l.chars().count()).unwrap_or(0);
            (b.id, text_editor::Position { line, column })
        });
        let mut tp = TextPage { sheet_id: sheet_id.to_string(), main_node: None, sheet, doc, docked, saved: md, dirty: false, last_change: Instant::now() };
        let task = match end {
            Some((id, position)) => tp.doc.focus_block(id, text_editor::Cursor { position, selection: None }).map(Message::Blocks),
            None => Task::none(),
        };
        self.text_page = Some(tp);
        task
    }

    fn save_text_page(&mut self) {
        let Some(t) = self.text_page.as_mut() else { return };
        let md = t.doc.to_markdown();
        if t.sheet_id.is_empty() {
            // Notes for a page that had none: the first words create the notes sheet.
            if md.trim().is_empty() {
                t.dirty = false;
                return;
            }
            let Some(main) = t.main_node.clone() else { return };
            let Screen::Notebook(o) = &self.screen else { return };
            let Ok((_, s)) = o.nb.add_side_sheet(&main, n0te_core::notebook::Side::Right, 0, &SheetSpec::a4(Background::Text)) else { return };
            let _ = self.lib.save_notebook(&o.nb);
            let Some(nb) = self.notebook_id() else { return };
            let Ok(sheet) = self.lib.load_sheet(&nb, &s) else { return };
            if let Some(t) = self.text_page.as_mut() {
                t.sheet_id = s;
                t.sheet = sheet;
            }
            self.relayout();
        }
        let Some(t) = self.text_page.as_mut() else { return };
        let _ = t.sheet.set_page_text(&md);
        t.saved = md;
        t.dirty = false;
        let (id, snap) = (t.sheet_id.clone(), t.sheet.snapshot().unwrap_or_default());
        if let Ok(sheet) = Sheet::load(0, &snap) {
            self.store_sheet(&id, &sheet);
        }
    }

    fn close_text_page(&mut self) {
        if self.text_page.as_ref().is_some_and(|t| t.dirty) {
            self.save_text_page();
        }
        self.text_page = None;
    }

    /// The text page editor: a paper column with the block editor (full width, or the docked
    /// notes column).
    fn text_page_view<'a>(&'a self, t: &'a TextPage, o: &'a Open) -> Element<'a, Message> {
        let label = o
            .pages
            .pages
            .iter()
            .find(|p| p.placed.sheet_id == t.sheet_id)
            .map(|p| p.placed.label())
            .or_else(|| t.main_node.as_ref().and_then(|m| o.pages.pages.iter().find(|p| &p.placed.node == m)).map(|p| p.placed.label()))
            .unwrap_or_default();
        let title = if t.docked { format!("NOTIZEN · SEITE {label}") } else { format!("TEXTSEITE · {label}") };
        let header = row![
            text(title).font(theme::MONO).size(10.5).color(theme::PAPER_MUTED),
            Space::new().width(Length::Fill),
            text(if t.dirty { "…" } else { "GESPEICHERT" }).font(theme::MONO).size(10.5).color(theme::PAPER_MUTED),
            Space::new().width(10),
            button(theme::btn_small("Schließen")).padding([2, 8]).on_press(Message::CloseTextPage).style(theme::text_tool),
        ]
        .align_y(Alignment::Center);
        // Word-like formatting for the block being edited.
        let kind = t.doc.focus.and_then(|id| t.doc.index(id)).map(|i| t.doc.blocks[i].kind.clone());
        let tb = |label: &'static str, msg: crate::blocks::Msg, on: bool, f: iced::Font| {
            button(text(label).font(f).size(13).color(if on { theme::p().accent } else { theme::INK }))
                .padding([3, 8])
                .on_press(Message::Blocks(msg))
                .style(theme::text_tool)
        };
        use crate::blocks::{Kind as K, Msg as B};
        let italic = iced::Font { style: iced::font::Style::Italic, ..theme::SANS };
        let tools = row![
            tb("¶", B::SetKind(K::Paragraph), kind == Some(K::Paragraph), theme::SANS),
            tb("H1", B::SetKind(K::Heading(1)), kind == Some(K::Heading(1)), theme::SANS_BOLD),
            tb("H2", B::SetKind(K::Heading(2)), kind == Some(K::Heading(2)), theme::SANS_BOLD),
            tb("H3", B::SetKind(K::Heading(3)), kind == Some(K::Heading(3)), theme::SANS_BOLD),
            Space::new().width(10),
            tb("B", B::Wrap("**"), false, theme::SANS_BOLD),
            tb("I", B::Wrap("*"), false, italic),
            tb("</>", B::Wrap("`"), false, theme::MONO),
            tb("∑", B::Wrap("$"), false, theme::SANS),
            Space::new().width(10),
            tb("•", B::SetKind(K::Bullet), kind == Some(K::Bullet), theme::SANS),
            tb("1.", B::SetKind(K::Numbered), kind == Some(K::Numbered), theme::SANS),
            tb("☐", B::SetKind(K::Todo(false)), matches!(kind, Some(K::Todo(_))), theme::SANS),
            tb("❝", B::SetKind(K::Quote), kind == Some(K::Quote), theme::SANS),
            tb("$$", B::SetKind(K::Formula), kind == Some(K::Formula), theme::MONO),
        ]
        .spacing(2)
        .align_y(Alignment::Center)
        .wrap();
        let paper = container(column![header, Space::new().height(8), tools, Space::new().height(12), t.doc.view().map(Message::Blocks)])
            .padding(Padding { top: 28.0, right: if t.docked { 28.0 } else { 64.0 }, bottom: 64.0, left: 28.0 })
            .max_width(if t.docked { 520.0 } else { 820.0 })
            .width(Length::Fill)
            .style(theme::paper);
        let page = container(paper).padding(Padding { top: 20.0, right: 20.0, bottom: 40.0, left: 20.0 }).center_x(Length::Fill);
        container(scrollable(page).style(theme::scroll_dark).height(Length::Fill).width(Length::Fill)).style(theme::surface).width(Length::Fill).height(Length::Fill).into()
    }

    fn notebook_id(&self) -> Option<String> {
        match &self.screen {
            Screen::Notebook(o) => Some(o.id.clone()),
            Screen::Library => None,
        }
    }

    /// Writes a sheet and makes the canvas read it again.
    fn store_sheet(&mut self, sheet_id: &str, sheet: &Sheet) {
        let Some(nb) = self.notebook_id() else { return };
        if let Err(e) = self.lib.save_sheet(&nb, sheet_id, sheet) {
            self.say(format!("Speichern fehlgeschlagen: {e}"));
        }
        if let Screen::Notebook(o) = &self.screen {
            o.pages.reload_sheet(sheet_id);
        }
    }

    /// Saves the box being edited (text, and with `place` its position/size).
    fn save_text(&mut self, place: bool) {
        let Some(e) = self.editing.as_mut() else { return };
        let text = e.content.text();
        let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
        let _ = e.sheet.set_text(&e.box_id, &text);
        if place {
            let _ = e.sheet.place_text_box(&e.box_id, e.x, e.y, e.w, e.size);
        }
        e.dirty = false;
        let (sheet_id, sheet) = (e.sheet_id.clone(), Sheet::load(0, &e.sheet.snapshot().unwrap_or_default()));
        if let Ok(sheet) = sheet {
            self.store_sheet(&sheet_id, &sheet);
        }
    }

    /// Ends editing: saves; an empty box is removed.
    fn finish_text(&mut self) {
        if self.editing.is_none() {
            return;
        }
        self.save_text(true);
        if let Some(e) = self.editing.take()
            && e.content.text().trim().is_empty()
        {
            let _ = e.sheet.remove(std::slice::from_ref(&e.box_id));
            self.store_sheet(&e.sheet_id, &e.sheet);
        }
    }

    fn edit_text(&mut self, sheet_id: &str, box_id: &str) -> Task<Message> {
        if self.editing.as_ref().is_some_and(|e| e.box_id == box_id) {
            return Task::none();
        }
        self.finish_text();
        let Some(nb) = self.notebook_id() else { return Task::none() };
        let Ok(sheet) = self.lib.load_sheet(&nb, sheet_id) else { return Task::none() };
        let Some(tb) = sheet.text_boxes().into_iter().find(|t| t.id == box_id) else { return Task::none() };
        let mut content = text_editor::Content::with_text(&tb.text);
        content.perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
        self.editing = Some(Editing { sheet_id: sheet_id.to_string(), box_id: box_id.to_string(), sheet, content, x: tb.x, y: tb.y, w: tb.w, size: tb.size, dirty: false, last_change: Instant::now() });
        iced::widget::operation::focus(editor_id())
    }

    fn new_text(&mut self, sheet_id: &str, x: f32, y: f32) -> Task<Message> {
        self.finish_text();
        let Some(nb) = self.notebook_id() else { return Task::none() };
        let page_w = match &self.screen {
            Screen::Notebook(o) => o.pages.pages.iter().find(|p| p.placed.sheet_id == sheet_id).map(|p| p.placed.rect.width()).unwrap_or(595.0),
            _ => 595.0,
        };
        let Ok(sheet) = self.lib.load_sheet(&nb, sheet_id) else { return Task::none() };
        let w = (page_w - x - 24.0).clamp(80.0, 360.0);
        let Ok(id) = sheet.add_text_box(x, y, w, 14.0, "") else { return Task::none() };
        self.store_sheet(sheet_id, &sheet);
        self.edit_text(sheet_id, &id)
    }

    /// Text boxes over the canvas: the edited one as an editor with grips, the others as
    /// rendered Markdown (a click edits).
    fn text_layers<'a>(&'a self, o: &'a Open) -> Vec<Element<'a, Message>> {
        let cam = o.camera;
        if cam.viewport.width <= 0.0 || cam.zoom < 0.2 {
            return Vec::new();
        }
        let mut out: Vec<Element<'a, Message>> = Vec::new();
        for (sheet, page, tb) in o.pages.visible_texts(&cam) {
            if self.editing.as_ref().is_some_and(|e| e.box_id == tb.id) {
                continue;
            }
            let x = (page.x0 + tb.x - cam.x) * cam.zoom;
            let y = (page.y0 + tb.y - cam.y) * cam.zoom;
            let body = crate::markdown::view(&tb.text, tb.size * cam.zoom, tb.w * cam.zoom);
            let on_press = if tb.id == n0te_core::sheet::BODY_ID {
                let docked = o.pages.pages.iter().any(|p| p.placed.sheet_id == sheet && p.placed.side != 0);
                Message::OpenTextPage(sheet.clone(), docked)
            } else {
                Message::EditText(sheet.clone(), tb.id.clone())
            };
            let area = mouse_area(body).on_press(on_press).interaction(iced::mouse::Interaction::Text);
            out.push(pin(area).x(x).y(y).into());
        }
        if let Some(e) = &self.editing
            && let Some(page) = o.pages.pages.iter().find(|p| p.placed.sheet_id == e.sheet_id).map(|p| p.placed.rect)
        {
            let x = (page.x0 + e.x - cam.x) * cam.zoom;
            let y = (page.y0 + e.y - cam.y) * cam.zoom;
            let small = |label: &'static str, m: Message| button(text(label).font(theme::SANS_BOLD).size(12)).padding([3, 8]).on_press(m).style(theme::text_tool);
            let tools = row![
                crate::widgets::grip("⠿", iced::mouse::Interaction::Grab, Message::MoveText, Message::PlaceText),
                Space::new().width(4),
                small("A−", Message::TextSize(-2.0)),
                small("A+", Message::TextSize(2.0)),
                small("×", Message::DeleteText),
                small("✓", Message::DoneText),
                Space::new().width(Length::Fill),
                crate::widgets::grip("↔", iced::mouse::Interaction::ResizingHorizontally, Message::ResizeText, Message::PlaceText),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .width(Length::Fixed((e.w * cam.zoom).max(150.0)));
            let editor = text_editor(&e.content)
                .id(editor_id())
                .on_action(Message::EditAction)
                .size(e.size * cam.zoom)
                .font(theme::SANS)
                .padding(4)
                .placeholder("Text, Markdown: # Titel, - Punkt, **fett**, $x^2$")
                .style(theme::editor_on_paper_framed)
                .highlight_with::<crate::mdstyle::MdInline>((), crate::mdstyle::format);
            out.push(pin(column![tools, container(editor).width(Length::Fixed(e.w * cam.zoom))].spacing(4)).x(x - 4.0).y(y - 32.0).into());
        }
        out
    }

    /// Lays the open notebook out again after a structural change (keeps the camera).
    fn relayout(&mut self) {
        if let Screen::Notebook(o) = &mut self.screen {
            o.rows = o.nb.outline(false);
            o.pages.relayout(nl::layout(&o.rows, &o.nb.outline(true), self.settings.layout()));
            o.camera = o.pages.clamp(o.camera);
        }
    }

    /// A PDF: in the library a new notebook named after the file, in a notebook new pages
    /// right after the current one.
    fn import_pdf(&mut self, path: &Path) {
        let Some(pdf) = self.pdf.clone() else { return self.say("PDF geht nicht: libpdfium.so fehlt") };
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return self.say(format!("Nicht lesbar: {e}")),
        };
        let name = path.file_stem().map(|s| s.to_string_lossy().replace('_', " ")).unwrap_or_else(|| "PDF".into());
        match &self.screen {
            Screen::Notebook(o) => {
                let current = o.pages.current_number(&o.camera);
                let node = o.pages.index_of_number(current).map(|i| o.pages.pages[i].placed.node.clone());
                let (parent, index) = pdf::position_after(&o.nb, node.as_deref());
                match pdf.import(&self.lib, &o.nb, bytes, parent.as_deref(), index) {
                    Ok(n) => {
                        self.relayout();
                        self.say(format!("{n} Seiten aus „{name}“ eingefügt"));
                    }
                    Err(e) => self.say(format!("Import fehlgeschlagen: {e}")),
                }
            }
            Screen::Library => {
                let made = self.lib.create_notebook(&name).map_err(|e| e.to_string()).and_then(|nb| pdf.import(&self.lib, &nb, bytes, None, 0).map(|_| nb.id()));
                match made {
                    Ok(id) => {
                        self.refresh();
                        let _ = self.open(&id);
                    }
                    Err(e) => self.say(format!("Import fehlgeschlagen: {e}")),
                }
            }
        }
    }

    /// Writes the PDF to ~/Downloads (a free name) and remembers it for "ÖFFNEN".
    fn export_pdf(&mut self, what: ExportWhat) {
        let Some(pdf) = self.pdf.clone() else { return self.say("PDF geht nicht: libpdfium.so fehlt") };
        let Screen::Notebook(o) = &self.screen else { return };
        let row = |main: &str| -> Vec<String> {
            let mut r: Vec<&n0te_core::layout::Placed> = o.pages.pages.iter().map(|p| &p.placed).filter(|p| p.main_node == main).collect();
            r.sort_by_key(|p| p.side);
            r.into_iter().map(|p| p.sheet_id.clone()).collect()
        };
        let selected: Option<Vec<String>> = match what {
            ExportWhat::All => None,
            ExportWhat::Current => {
                let current = o.pages.current_number(&o.camera);
                o.pages.index_of_number(current).map(|i| row(&o.pages.pages[i].placed.node))
            }
            ExportWhat::Picked => {
                let picked = self.export.clone().unwrap_or_default();
                Some(o.pages.pages.iter().filter(|p| p.placed.side == 0 && picked.contains(&p.placed.node)).flat_map(|p| row(&p.placed.node)).collect())
            }
        };
        if selected.as_ref().is_some_and(|s| s.is_empty()) {
            return self.say("Keine Seite gewählt");
        }
        let title = o.nb.title();
        match pdf.export(&self.lib, &o.nb, selected.as_deref()) {
            Ok((bytes, pages)) => {
                let path = free_path(&self.export_dir, &title, "pdf");
                match n0te_core::store::write_atomic(&path, &bytes) {
                    Ok(()) => {
                        self.export = None;
                        self.say(format!("{pages} Seiten exportiert: {}", path.display()));
                        self.last_export = Some(path);
                    }
                    Err(e) => self.say(format!("Speichern fehlgeschlagen: {e}")),
                }
            }
            Err(e) => self.say(format!("Export fehlgeschlagen: {e}")),
        }
    }

    /// Global shortcuts (keys no focused widget consumed).
    fn shortcut(&mut self, key: Key, m: keyboard::Modifiers) -> Task<Message> {
        // The palette takes the arrows and Esc while it is open.
        if self.palette.is_some() {
            return match key.as_ref() {
                Key::Named(Named::ArrowDown) => self.update(Message::PaletteMove(1)),
                Key::Named(Named::ArrowUp) => self.update(Message::PaletteMove(-1)),
                Key::Named(Named::Escape) => {
                    self.palette = None;
                    Task::none()
                }
                _ => Task::none(),
            };
        }
        if self.show_help || self.show_settings {
            if let Key::Named(Named::Escape | Named::F1) = key.as_ref() {
                self.show_help = false;
                self.show_settings = false;
            }
            return Task::none();
        }
        // Writing: Ctrl+B / Ctrl+I format like in a word processor (not the outline toggle).
        if m.command() && !m.shift() && let Key::Character(c @ ("b" | "i")) = key.as_ref() {
            let mark = if c == "b" { "**" } else { "*" };
            if let Some(e) = &mut self.editing {
                crate::blocks::wrap_in(&mut e.content, mark);
                e.dirty = true;
                e.last_change = Instant::now();
                return iced::widget::operation::focus(editor_id());
            }
            if self.text_page.is_some() {
                return self.update(Message::Blocks(crate::blocks::Msg::Wrap(mark)));
            }
        }
        match (m.command(), key.as_ref()) {
            (true, Key::Character("k")) => return self.update(Message::OpenPalette),
            (_, Key::Named(Named::F1)) => return self.update(Message::ShowHelp(true)),
            (true, Key::Character(",")) => return self.update(Message::ShowSettings(true)),
            (true, Key::Character("o")) => return self.update(Message::ShowPdfImport(true)),
            _ => {}
        }
        if let Key::Named(Named::Escape) = key.as_ref() {
            if self.editing.is_some() {
                self.finish_text();
                return Task::none();
            }
            if self.text_page.is_some() {
                self.close_text_page();
                return Task::none();
            }
            if self.creating.is_some() || self.show_imports {
                self.creating = None;
                self.show_imports = false;
                return Task::none();
            }
            return self.update(Message::Back);
        }
        let Screen::Notebook(o) = &mut self.screen else {
            return match (m.command(), key.as_ref()) {
                (true, Key::Character("n")) => self.update(Message::StartCreate),
                (true, Key::Character("i")) => self.update(Message::ShowImports(true)),
                _ => Task::none(),
            };
        };
        let c = o.camera;
        if m.command() {
            return match key.as_ref() {
                Key::Character("0") => self.update(Message::Zoom(1.0)),
                Key::Character("+" | "=") => self.update(Message::Zoom(c.zoom * 1.25)),
                Key::Character("-") => self.update(Message::Zoom(c.zoom / 1.25)),
                Key::Character("b") => self.update(Message::ToggleSidebar),
                Key::Character("n") => self.update(Message::NewTextPage),
                Key::Character("m") => self.update(Message::ToggleNotes),
                Key::Character("e") => self.update(Message::ShowExport(true)),
                Key::Character("h") => self.update(Message::ShowHistory(true)),
                Key::Character("t") => {
                    // A text box in the middle of the view, on the page there.
                    let (cx, cy) = (c.x + c.viewport.width / c.zoom / 2.0, c.y + c.viewport.height / c.zoom / 3.0);
                    match o.pages.sheet_at(cx, cy).filter(|p| !p.is_board()).map(|p| (p.sheet_id.clone(), cx - p.rect.x0, cy - p.rect.y0)) {
                        Some((sheet, x, y)) => self.update(Message::NewText(sheet, x, y)),
                        None => Task::none(),
                    }
                }
                _ => Task::none(),
            };
        }
        let current = o.pages.current_number(&c);
        let mains = o.pages.mains();
        let pos = mains.iter().position(|&i| o.pages.pages[i].placed.number >= current).unwrap_or(0);
        let target = match key.as_ref() {
            Key::Named(Named::PageDown) | Key::Named(Named::ArrowRight) => mains.get(pos + 1).copied(),
            Key::Named(Named::PageUp) | Key::Named(Named::ArrowLeft) => pos.checked_sub(1).and_then(|p| mains.get(p).copied()),
            Key::Named(Named::Home) => mains.first().copied(),
            Key::Named(Named::End) => mains.last().copied(),
            _ => None,
        };
        if let Some(i) = target {
            o.camera = o.pages.camera_for(i, &c);
        }
        Task::none()
    }

    pub fn view(&self) -> Element<'_, Message> {
        let base = match &self.screen {
            Screen::Library => library::view(self),
            Screen::Notebook(o) => self.notebook_view(o),
        };
        let overlay: Option<Element<'_, Message>> = if let Some((q, sel)) = &self.palette {
            Some(palette::view(q, &self.palette_items(), *sel))
        } else if self.show_help {
            Some(self.help_view())
        } else if self.show_settings {
            Some(self.settings_view())
        } else if let (Screen::Notebook(o), Some(picked)) = (&self.screen, &self.export) {
            Some(library::export_dialog(o, picked, self.export_mode, o.pages.current_number(&o.camera)))
        } else {
            None
        };
        match overlay {
            Some(o) => stack![base, container(Space::new()).width(Length::Fill).height(Length::Fill).style(theme::scrim), o].into(),
            None => base,
        }
    }

    /// Version history: checkpoints of this notebook; restoring makes a copy.
    fn history_panel<'a>(&'a self, list: &'a [(u64, String)]) -> Element<'a, Message> {
        let mut col = column![
            row![
                theme::heading("Verlauf", 20.0),
                Space::new().width(Length::Fill),
                button(theme::btn("Schließen")).padding(theme::pad()).on_press(Message::ShowHistory(false)).style(theme::ghost),
            ]
            .align_y(Alignment::Center),
            text("Der Sync-Dienst sichert alle 5 Minuten. Ältere Stände werden ausgedünnt: alles der letzten Stunde, stündlich für einen Tag, täglich für 30 Tage, danach wöchentlich.").size(12).color(theme::p().muted),
        ]
        .spacing(8);
        if list.is_empty() {
            col = col.push(text("Noch keine Stände gesichert.").size(13).color(theme::p().muted));
        }
        let mut rows = column![].spacing(2);
        for (at, label) in list.iter().rev() {
            rows = rows.push(
                row![
                    text(label.clone()).font(theme::meta_font()).size(12.5),
                    Space::new().width(Length::Fill),
                    button(theme::btn_small("Als Kopie wiederherstellen")).padding([3, 8]).on_press(Message::Restore(*at)).style(theme::ghost),
                ]
                .align_y(Alignment::Center),
            );
        }
        col = col.push(scrollable(rows).style(theme::scroll_dark).height(Length::Shrink).height(Length::Fixed(320.0)));
        container(col).padding(20).width(Length::Fill).max_width(760).style(theme::panel).into()
    }

    /// Share with guests: invite links (read / read-write), active invites with revoke.
    fn share_panel<'a>(&'a self, invites: &'a [live::Invite]) -> Element<'a, Message> {
        let mut col = column![
            row![theme::heading("Teilen", 20.0), Space::new().width(12), text("Gäste verbinden sich direkt, ohne Server.").size(12).color(theme::p().muted)].align_y(Alignment::Center),
            row![
                button(theme::btn("Nur lesen · 7 Tage")).padding(theme::pad()).on_press(Message::Invite(false, 7)).style(theme::secondary),
                Space::new().width(8),
                button(theme::btn("Lesen & schreiben · 7 Tage")).padding(theme::pad()).on_press(Message::Invite(true, 7)).style(theme::secondary),
                Space::new().width(8),
                button(theme::btn("Ohne Ablauf")).padding(theme::pad()).on_press(Message::Invite(false, 0)).style(theme::ghost),
                Space::new().width(Length::Fill),
                button(theme::btn("Schließen")).padding(theme::pad()).on_press(Message::ShowShare(false)).style(theme::ghost),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(10);
        if self.sync.is_none() {
            col = col.push(text("Der Sync-Dienst läuft nicht (systemctl --user enable --now n0te-sync).").size(13).color(theme::p().accent));
        }
        if invites.is_empty() {
            col = col.push(text("Noch keine Einladungen. Der Link wird beim Erstellen kopiert; Gäste öffnen ihn in n0te (EINLADUNG).").size(13).color(theme::p().muted));
        }
        for i in invites.iter().rev() {
            let state = if i.revoked { "widerrufen".to_string() } else if i.online > 0 { format!("{} verbunden", i.online) } else { "nicht verbunden".to_string() };
            let mut r = row![
                text(theme::cased(if i.rw { "Lesen & schreiben" } else { "Nur lesen" })).font(theme::label_font()).size(theme::label_size()).width(Length::Fixed(150.0)),
                text(state).size(12).color(theme::p().muted).width(Length::Fixed(130.0)),
                Space::new().width(Length::Fill),
            ]
            .align_y(Alignment::Center)
            .spacing(6);
            if !i.revoked {
                r = r.push(button(theme::btn_small("Link kopieren")).padding([3, 8]).on_press(Message::CopyText(i.link.clone())).style(theme::ghost));
                r = r.push(button(theme::btn_small("Widerrufen")).padding([3, 8]).on_press(Message::Revoke(i.id.clone())).style(theme::ghost));
            }
            col = col.push(r);
        }
        // The newest invite as a QR code (scan with the tablet/phone).
        if let Some(data) = &self.share_qr {
            col = col.push(row![scannable_qr(data, 3.0), text("Neueste Einladung als QR-Code").size(12).color(theme::p().muted)].spacing(12).align_y(Alignment::Center));
        }
        container(col).padding(20).width(Length::Fill).max_width(760).style(theme::panel).into()
    }

    fn help_view(&self) -> Element<'_, Message> {
        let keys = [
            ("Strg K", "Befehle, Seiten, Notizbücher suchen"),
            ("Esc", "Zurück · Bearbeiten beenden · Bibliothek"),
            ("Bild ↑ / ↓, ← / →", "Vorige / nächste Seite"),
            ("Pos1 / Ende", "Erste / letzte Seite"),
            ("Mausrad · Ziehen", "Verschieben"),
            ("Strg + Mausrad, Strg + / −", "Zoomen, Strg 0: 100 %"),
            ("Doppelklick · Strg T", "Textfeld"),
            ("Strg N", "Neue Textseite (Bibliothek: neues Notizbuch)"),
            ("Strg M", "Notizen neben der Seite"),
            ("Strg O", "PDF einfügen / importieren (oder Datei ins Fenster ziehen)"),
            ("Strg E", "PDF exportieren"),
            ("Strg B", "Gliederung ein/aus"),
            ("Strg H", "Verlauf: frühere Stände wiederherstellen"),
            ("Strg I", "Vom Tablet übernehmen (.n0te)"),
            ("Strg ,", "Einstellungen"),
            ("F1", "Diese Übersicht"),
        ];
        let mut col = column![theme::heading("Tastenkürzel", 22.0)].spacing(9);
        for (k, what) in keys {
            col = col.push(row![text(k).font(theme::MONO_BOLD).size(12).width(Length::Fixed(230.0)), text(what).size(14)].align_y(Alignment::Center));
        }
        col = col.push(button(theme::btn("Schließen")).padding(theme::pad()).on_press(Message::ShowHelp(false)).style(theme::ghost));
        container(container(col).padding(20).width(Length::Fixed(640.0)).style(theme::panel)).padding(Padding { top: 70.0, ..Padding::ZERO }).center_x(Length::Fill).into()
    }

    fn devices_view(&self) -> Element<'_, Message> {
        let mut col = column![].spacing(10);
        if self.sync.is_none() {
            col = col.push(text("Der Sync-Dienst läuft nicht. Starten: systemctl --user enable --now n0te-sync").size(13).color(theme::p().accent));
            return col.into();
        }
        if self.devices.is_empty() {
            col = col.push(text("Noch kein Gerät gekoppelt.").size(13).color(theme::p().muted));
        }
        for d in &self.devices {
            let online = d.online;
            col = col.push(
                row![
                    container(Space::new().width(8).height(8)).style(move |_| container::Style {
                        background: Some((if online { theme::p().accent } else { theme::p().muted }).into()),
                        border: iced::Border { radius: 4.0.into(), ..Default::default() },
                        ..Default::default()
                    }),
                    text(d.name.clone()).size(14).color(theme::p().text),
                    text(if online { "verbunden" } else { "nicht erreichbar" }).size(12).color(theme::p().muted),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            );
        }
        match &self.pair_code {
            None => col = col.push(button(theme::btn("Tablet koppeln")).padding(theme::pad()).on_press(Message::PairStart).style(theme::secondary)),
            Some(code) => {
                // The QR code: the tablet scans it (n0te → Einstellungen → Laptop → QR scannen).
                if let Some(data) = &self.pair_qr {
                    col = col.push(
                        row![
                            scannable_qr(data, 4.0),
                            Space::new().width(16),
                            column![
                                text("Am Tablet scannen").font(theme::title_font()).size(14).color(theme::p().text),
                                text("n0te → Einstellungen → Laptop → QR scannen. Dafür braucht es kein n0bridge. Der Code gilt 15 Minuten und nur einmal.").size(12).color(theme::p().muted),
                            ]
                            .spacing(4)
                            .width(Length::Fill),
                        ]
                        .align_y(Alignment::Center),
                    );
                }
                col = col.push(text_input("", code).size(12).font(theme::MONO).padding(8).style(theme::field));
                let mut buttons = row![button(theme::btn_small("Kopieren")).padding(theme::pad()).on_press(Message::CopyText(code.clone())).style(theme::secondary)].align_y(Alignment::Center);
                // With n0bridge on both devices the code can go straight to the tablet's clipboard.
                if n0bridge_installed() {
                    buttons = buttons.push(Space::new().width(8)).push(button(theme::btn_small("An Tablet schicken (n0bridge)")).padding(theme::pad()).on_press(Message::PairToTablet).style(theme::ghost));
                }
                col = col.push(buttons);
            }
        }
        col.into()
    }

    fn settings_view(&self) -> Element<'_, Message> {
        // Designs: a card per style with a tiny preview of its look.
        let mut cards = Vec::new();
        for t in theme::PALETTES.iter() {
            let on = t.id == theme::p().id;
            cards.push(
                button(
                    column![
                        style_preview(t),
                        text(t.name).font(theme::title_font()).size(14).color(theme::p().text),
                        text(t.style.desc).size(11.5).color(theme::p().muted),
                    ]
                    .spacing(5),
                )
                .padding(8)
                .width(Length::Fixed(176.0))
                .on_press(Message::SetTheme(t.id))
                .style(theme::option(on))
                .into(),
            );
        }
        let designs = iced::widget::Row::with_children(cards).spacing(10).wrap().vertical_spacing(10);
        let slider = |label: &'static str, v: f32, f: fn(f32) -> Message| {
            row![
                text(label).size(14).width(Length::Fixed(170.0)),
                iced::widget::slider(4.0..=160.0, v, f).style(theme::slider_style).width(Length::Fill),
                text(format!("{v:.0} pt")).font(theme::MONO).size(12).width(Length::Fixed(56.0)).align_x(iced::alignment::Horizontal::Right),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
        };
        let section = settings_section;
        let rule = || container(Space::new()).height(1).width(Length::Fill).style(theme::hairline);
        let body = column![
            section("Design", "Farben, Formen und Schrift. Exportierte PDFs bleiben weiß.", designs.into()),
            rule(),
            section("Abstände", "Zwischen den Seiten im Notizbuch.", column![slider("Zwischen Seiten", self.settings.row_gap, Message::SetRowGap), slider("Zu Seitenblättern", self.settings.side_gap, Message::SetSideGap)].spacing(8).into()),
            rule(),
            section(
                "Text",
                "Textseiten und Textfelder.",
                iced::widget::toggler(self.settings.formula_center).label("Formeln zentrieren ($$…$$ und Formelblöcke)").on_toggle(Message::FormulaCenter).size(20).text_size(14).spacing(12).style(theme::switch).into(),
            ),
            rule(),
            section("Geräte", "Sync mit dem Tablet, direkt und ohne Server.", self.devices_view()),
        ]
        .spacing(18);
        let col = column![
            row![theme::heading("Einstellungen", 24.0), Space::new().width(Length::Fill), button(theme::btn_small("Schließen")).padding(theme::pad()).on_press(Message::ShowSettings(false)).style(theme::ghost)].align_y(Alignment::Center),
            iced::widget::scrollable(body).style(theme::scroll_dark).height(Length::Shrink),
            text("Gespeichert in ~/.config/n0te/laptop.conf").font(theme::meta_font()).size(11.5).color(theme::p().muted),
        ]
        .spacing(20);
        container(container(col).padding(26).width(Length::Fixed(860.0)).max_height(820.0).style(theme::panel)).padding(Padding { top: 40.0, ..Padding::ZERO }).center_x(Length::Fill).into()
    }

    fn notebook_view<'a>(&'a self, o: &'a Open) -> Element<'a, Message> {
        let current = o.pages.current_number(&o.camera);
        let page_view = pages::page_view(&o.pages, o.camera, Message::Camera, Message::Fold).on_double_click(Message::NewText);
        let mut layers: Vec<Element<'a, Message>> = vec![page_view.into()];
        layers.extend(self.text_layers(o));
        let canvas: Element<'a, Message> = iced::widget::Stack::with_children(layers).into();
        let mut body = row![];
        if self.sidebar {
            body = body.push(sidebar::view(&o.rows, &o.pages, current));
            body = body.push(container(Space::new()).width(1).height(Length::Fill).style(theme::hairline));
        }
        let panel: Option<Element<'a, Message>> = if let Some(h) = &self.history {
            Some(self.history_panel(h))
        } else if let Some(invites) = &self.share {
            Some(self.share_panel(invites))
        } else if self.show_pdf_import {
            Some(library::pdf_panel(&self.pdfs, "PDF einfügen: nach der aktuellen Seite"))
        } else {
            None
        };
        let canvas: Element<'a, Message> = match panel {
            Some(p) => stack![canvas, container(p).padding(16).width(Length::Fill)].into(),
            None => canvas,
        };
        match &self.text_page {
            Some(t) if !t.docked => body = body.push(self.text_page_view(t, o)),
            Some(t) => {
                body = body.push(canvas);
                body = body.push(container(Space::new()).width(1).height(Length::Fill).style(theme::hairline));
                body = body.push(container(self.text_page_view(t, o)).width(Length::Fixed(460.0)));
            }
            None => body = body.push(canvas),
        }
        let live_bar: Element<'a, Message> = match &self.live_session {
            Some(l) => container(
                row![
                    text(theme::cased(&format!("Live mit {}", l.name))).font(theme::label_font()).size(theme::label_size()).color(theme::p().accent),
                    Space::new().width(18),
                    iced::widget::checkbox(l.follow_them).label("Ansicht folgt dem Tablet").on_toggle(Message::FollowThem).size(14).text_size(12),
                    Space::new().width(18),
                    iced::widget::checkbox(l.they_follow).label("Tablet folgt mir").on_toggle(Message::TheyFollow).size(14).text_size(12),
                    Space::new().width(18),
                    iced::widget::checkbox(self.settings.remote_frame).label("Rahmen zeigen").on_toggle(Message::RemoteFrame).size(14).text_size(12),
                    Space::new().width(Length::Fill),
                    button(theme::btn_small("Beenden")).padding([3, 10]).on_press(Message::StopLive).style(theme::ghost),
                ]
                .align_y(Alignment::Center),
            )
            .padding([4, 14])
            .width(Length::Fill)
            .style(theme::chrome)
            .into(),
            None => Space::new().height(0).into(),
        };
        column![
            self.top_bar(o, current),
            container(Space::new()).height(1).width(Length::Fill).style(theme::hairline),
            live_bar,
            body.height(Length::Fill),
            container(Space::new()).height(1).width(Length::Fill).style(theme::hairline),
            self.status_bar(),
        ]
        .into()
    }

    fn top_bar<'a>(&'a self, o: &'a Open, current: u32) -> Element<'a, Message> {
        let chip = |s: String| container(text(s).font(theme::MONO).size(theme::LABEL)).padding([3, 10]).style(theme::chip);
        container(
            row![
                button(theme::btn("‹ Bibliothek")).padding(theme::pad()).on_press(Message::Back).style(theme::ghost),
                Space::new().width(18),
                text(o.nb.title().to_uppercase()).font(theme::MONO).size(theme::SMALL).color(theme::p().text),
                Space::new().width(Length::Fill),
                chip(format!("{:>3.0} %", o.camera.zoom * 100.0)),
                Space::new().width(8),
                chip(format!("{current:02} / {:02}", o.pages.total_pages)),
                Space::new().width(14),
                button(theme::btn("+ Textseite")).padding(theme::pad()).on_press(Message::NewTextPage).style(theme::tab(false)),
                Space::new().width(6),
                button(theme::btn("Notizen")).padding(theme::pad()).on_press(Message::ToggleNotes).style(theme::tab(self.text_page.as_ref().is_some_and(|t| t.docked))),
                Space::new().width(6),
                button(theme::btn("PDF einfügen")).padding(theme::pad()).on_press(Message::ShowPdfImport(!self.show_pdf_import)).style(theme::tab(self.show_pdf_import)),
                Space::new().width(6),
                button(theme::btn("Export")).padding(theme::pad()).on_press(Message::ShowExport(self.export.is_none())).style(theme::tab(self.export.is_some())),
                Space::new().width(6),
                button(theme::btn("Teilen")).padding(theme::pad()).on_press(Message::ShowShare(self.share.is_none())).style(theme::tab(self.share.is_some())),
                Space::new().width(6),
                match &self.live_session {
                    Some(l) => button(theme::btn(&format!("● Live · {}", l.name))).padding(theme::pad()).on_press(Message::StopLive).style(theme::accent_button),
                    None => button(theme::btn("Live")).padding(theme::pad()).on_press(Message::StartLive).style(theme::tab(false)),
                },
            ]
            .align_y(Alignment::Center),
        )
        .padding(Padding { top: 6.0, bottom: 6.0, left: 14.0, right: 14.0 })
        .height(48)
        .width(Length::Fill)
        .style(theme::chrome)
        .into()
    }

    pub fn status_bar(&self) -> Element<'_, Message> {
        let left: Element<'_, Message> = match &self.status {
            Some(s) => row![
                text(s.as_str()).font(theme::MONO).size(10.5).color(theme::p().text),
                Space::new().width(10),
                match &self.last_export {
                    Some(p) => Element::from(button(theme::btn_small("Öffnen")).padding([0, 8]).on_press(Message::OpenPath(p.clone())).style(theme::ghost)),
                    None => Space::new().into(),
                },
                Space::new().width(6),
                button(text("×").size(12)).padding([0, 6]).on_press(Message::DismissStatus).style(theme::ghost),
            ]
            .align_y(Alignment::Center)
            .into(),
            None => text("STRG K BEFEHLE · F1 TASTENKÜRZEL · ESC ZURÜCK · BILD↑↓ SEITEN · STRG +/− ZOOM").font(theme::MONO).size(10.5).color(theme::p().muted).into(),
        };
        container(row![left, Space::new().width(Length::Fill), text(format!("N0TE {}", env!("CARGO_PKG_VERSION"))).font(theme::MONO).size(10.5).color(theme::p().muted)].align_y(Alignment::Center))
            .padding([4, 14])
            .height(26)
            .width(Length::Fill)
            .style(theme::chrome)
            .into()
    }

    /// The "new notebook" title field (library).
    pub fn create_row(&self) -> Option<Element<'_, Message>> {
        let t = self.creating.as_ref()?;
        let choice = |label: &str, on: bool, m: Message| button(theme::btn_small(label)).padding(theme::pad()).on_press(m).style(theme::tab(on));
        let mut papers = row![].spacing(6);
        for (b, label) in [(Background::Blank, "Leer"), (Background::Lines, "Linien"), (Background::Grid, "Kästchen"), (Background::Dots, "Punkte"), (Background::Cornell, "Cornell")] {
            papers = papers.push(choice(label, self.new_paper == b, Message::NewPaper(b)));
        }
        let mut formats = row![].spacing(6);
        for (i, (label, _, _)) in FORMATS.iter().enumerate() {
            formats = formats.push(choice(label, self.new_format == i, Message::NewFormat(i)));
        }
        let label = |s: &str| container(theme::caption(s)).width(Length::Fixed(130.0));
        let mut col = column![
            theme::heading("Neues Notizbuch", 20.0),
            row![
                text_input("Titel, z. B. Analysis I", t).on_input(Message::NewTitle).on_submit(Message::Create).padding(10).size(15).style(theme::field),
                Space::new().width(10),
                button(theme::btn("Anlegen")).padding(theme::pad()).on_press(Message::Create).style(theme::accent_button),
                Space::new().width(6),
                button(theme::btn("Abbrechen")).padding(theme::pad()).on_press(Message::CancelCreate).style(theme::ghost),
            ]
            .align_y(Alignment::Center),
            row![label("Papier"), papers].align_y(Alignment::Center),
        ]
        .spacing(12);
        if self.new_paper != Background::Blank {
            let what = match self.new_paper {
                Background::Grid => "Kästchengröße",
                Background::Dots => "Punktabstand",
                _ => "Linienabstand",
            };
            let mut quick = row![].spacing(6);
            for (mm, l) in [(4.0f32, "4 mm"), (5.0, "5 mm"), (7.0, "7 mm"), (8.5, "8,5 mm")] {
                let pt = mm / 0.3528;
                // Units keep their case, also in the mono designs ("5 mm", not "5 MM").
                let on = (self.new_spacing - pt).abs() < 0.6;
                quick = quick.push(button(text(l).font(theme::label_font()).size(theme::label_size() - 1.5)).padding(theme::pad()).on_press(Message::NewSpacing(pt)).style(theme::tab(on)));
            }
            col = col.push(
                row![
                    label(what),
                    iced::widget::slider(8.0..=40.0, self.new_spacing, Message::NewSpacing).step(0.5_f32).style(theme::slider_style).width(Length::Fixed(220.0)),
                    Space::new().width(12),
                    // 1 pt = 0.3528 mm: what you'd measure on paper.
                    text(format!("{:.0} pt · {:.1} mm", self.new_spacing, self.new_spacing * 0.3528)).font(theme::MONO).size(12).width(Length::Fixed(120.0)),
                    quick,
                ]
                .align_y(Alignment::Center),
            );
        }
        col = col.push(row![label("Format"), formats].align_y(Alignment::Center));
        Some(container(col).padding(20).width(Length::Fill).max_width(960).style(theme::panel).into())
    }

    pub fn scroll<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
        scrollable(content).style(theme::scroll_dark).height(Length::Fill).width(Length::Fill).into()
    }
}

/// Page formats for a new notebook: name, width, height (pt).
pub const FORMATS: [(&str, f32, f32); 4] = [("A4 hoch", 595.0, 842.0), ("A4 quer", 842.0, 595.0), ("A5 hoch", 420.0, 595.0), ("Letter", 612.0, 792.0)];

/// The usual spacing of a paper (pt): 6 mm boxes, 7.8 mm lines, 7 mm dots.
fn default_spacing(b: Background) -> f32 {
    match b {
        Background::Grid => 17.0,
        Background::Dots => 20.0,
        _ => 22.0,
    }
}

fn downloads() -> PathBuf {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Downloads")).unwrap_or_else(std::env::temp_dir)
}

/// `dir/title.ext`, or `title (2).ext` … if taken; unsafe characters replaced.
pub fn free_path(dir: &Path, title: &str, ext: &str) -> PathBuf {
    let safe: String = title.chars().map(|c| if c.is_alphanumeric() || " -_.,()".contains(c) { c } else { '_' }).collect();
    let base = if safe.trim().is_empty() { "n0te".to_string() } else { safe.trim().to_string() };
    let _ = std::fs::create_dir_all(dir);
    (1..)
        .map(|i| dir.join(if i == 1 { format!("{base}.{ext}") } else { format!("{base} ({i}).{ext}") }))
        .find(|p| !p.exists())
        .expect("a free name")
}

/// One tick per second (text boxes save after a pause in typing).
fn ticks() -> iced::futures::stream::BoxStream<'static, ()> {
    use iced::futures::StreamExt;
    let (tx, rx) = iced::futures::channel::mpsc::unbounded();
    std::thread::spawn(move || {
        while tx.unbounded_send(()).is_ok() {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
    rx.boxed()
}

/// Runs the n0bridge CLI.
async fn n0bridge(args: Vec<String>) -> Result<String, String> {
    let out = std::process::Command::new("n0bridge").args(&args).output().map_err(|e| format!("n0bridge nicht gefunden ({e})"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    if out.status.success() { Ok(text.trim().to_string()) } else { Err(text.trim().lines().last().unwrap_or("Fehler").to_string()) }
}

/// A QR code that scanners read in every design: black cells on white with a white quiet
/// zone (iced's default takes the theme's text/background colours, inverted in dark themes).
fn scannable_qr(data: &iced::widget::qr_code::Data, cell: f32) -> Element<'_, Message> {
    let qr = iced::widget::qr_code(data).cell_size(cell).style(|_| iced::widget::qr_code::Style { cell: iced::Color::BLACK, background: iced::Color::WHITE });
    container(qr).padding(4.0 * cell).style(|_| container::Style { background: Some(iced::Color::WHITE.into()), ..Default::default() }).into()
}

/// A settings section: title and explanation on the left, the controls on the right.
fn settings_section<'a>(title: &str, desc: &str, body: Element<'a, Message>) -> Element<'a, Message> {
    row![
        column![text(title.to_string()).font(theme::title_font()).size(15).color(theme::p().text), text(desc.to_string()).size(12).color(theme::p().muted)].spacing(3).width(Length::Fixed(190.0)),
        Space::new().width(24),
        container(body).width(Length::Fill),
    ]
    .into()
}

/// A design in miniature: its chrome, heading face, a primary and a secondary button in its
/// shapes, and a sheet of its paper.
fn style_preview(t: &'static theme::Palette) -> Element<'static, Message> {
    use theme::{Fill, Heading};
    let st = t.style;
    let face = match st.heading {
        Heading::Dot => theme::DOT,
        Heading::Mono => theme::MONO_BOLD,
        Heading::Sans => theme::SANS_BOLD,
        Heading::Serif => theme::SERIF_MEDIUM,
    };
    let r = st.button.min(7.0);
    let primary = container(Space::new().width(30).height(11)).style(move |_| container::Style { background: Some(t.accent.into()), border: iced::Border { radius: r.into(), ..Default::default() }, ..Default::default() });
    let secondary = container(Space::new().width(24).height(11)).style(move |_| {
        let (bg, line) = match st.fill {
            Fill::Outline => (iced::Color::TRANSPARENT, t.border),
            Fill::Tonal => (if t.dark { theme::alpha(t.accent, 0.22) } else { t.surface_2 }, iced::Color::TRANSPARENT),
            Fill::Rule => (iced::Color::TRANSPARENT, theme::alpha(t.text, 0.6)),
        };
        container::Style { background: Some(bg.into()), border: iced::Border { radius: r.into(), width: if line.a > 0.0 { 1.0 } else { 0.0 }, color: line }, ..Default::default() }
    });
    let sheet = container(Space::new().width(30).height(40)).style(move |_| container::Style { background: Some(t.paper.into()), border: iced::Border { radius: st.card.min(3.0).into(), ..Default::default() }, ..Default::default() });
    container(
        row![
            column![text(if st.heading == Heading::Dot { "aa" } else { "Aa" }).font(face).size(if st.heading == Heading::Dot { 26 } else { 22 }).color(t.text), row![primary, Space::new().width(5), secondary]].spacing(6),
            Space::new().width(Length::Fill),
            sheet,
        ]
        .align_y(Alignment::Center),
    )
    .padding([10, 12])
    .width(Length::Fill)
    .style(move |_| container::Style { background: Some(t.chrome.into()), border: iced::Border { radius: st.card.min(10.0).into(), width: 1.0, color: t.border }, ..Default::default() })
    .into()
}

/// Whether the n0bridge CLI is installed (then codes can go straight to the tablet).
fn n0bridge_installed() -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("n0bridge").is_file()))
}

/// The paired tablet's n0bridge device id (`n0bridge devices`), since not every n0bridge
/// command resolves the word "tablet"; falls back to "tablet".
fn tablet_target() -> String {
    std::process::Command::new("n0bridge")
        .arg("devices")
        .output()
        .ok()
        .and_then(|o| tablet_id(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_else(|| "tablet".into())
}

/// The id of the first device tagged `tablet` in `n0bridge devices` output.
fn tablet_id(devices: &str) -> Option<String> {
    devices.lines().find_map(|l| {
        let mut words = l.split_whitespace();
        let tagged = l.split_whitespace().any(|w| w.starts_with('[') && w.trim_matches(|c| c == '[' || c == ']').split(',').any(|t| t == "tablet"));
        if !tagged {
            return None;
        }
        words.find_map(|w| w.strip_prefix("id=")).map(str::to_string)
    })
}

/// Sends a file to the tablet with the n0bridge CLI.
async fn send_with_n0bridge(path: PathBuf) -> Result<String, String> {
    let out = std::process::Command::new("n0bridge")
        .args(["send", "--to", &tablet_target()])
        .arg(&path)
        .output()
        .map_err(|e| format!("n0bridge nicht gefunden ({e})"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    if out.status.success() { Ok(text.trim().to_string()) } else { Err(text.trim().lines().last().unwrap_or("Fehler").to_string()) }
}

#[cfg(test)]
mod n0bridge_tests {
    #[test]
    fn finds_the_tablet_id() {
        let out = "Tablet_s7_fe_flinn   id=b2b36918c88ef31d [tablet,terminal] app=0.14.6 akku=97%\nnothing-phone-3a-pro id=0c86c3a983ca106c [phone,primary,auth] app=0.14.2\n";
        assert_eq!(super::tablet_id(out).as_deref(), Some("b2b36918c88ef31d"));
        assert_eq!(super::tablet_id("phone id=1 [phone,primary]\n"), None);
    }
}
