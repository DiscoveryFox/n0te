//! The application: the library (notebooks by folder, import of `.n0te` bundles) and an open
//! notebook (outline sidebar, page canvas, top and status bar).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{Space, button, column, container, mouse_area, pin, row, scrollable, stack, text, text_editor, text_input};
use iced::{Alignment, Element, Length, Padding, Point, Subscription, Task, Theme};

use n0te_core::format;
use n0te_core::layout as nl;
use n0te_core::notebook::{Background, Notebook, OutlineRow, SheetSpec};
use n0te_core::sheet::{Heading, Sheet};
use n0te_core::store::Library;

use crate::gestures::Gesture;
use crate::pages::{self, Camera, Pages, SheetContent};
use crate::pdf::{self, Pdf};
use crate::live::{self, LiveEvent};
use crate::palette::{self, Command};
use crate::search::{self, Search};
use crate::settings::Settings;
use crate::split::{self, PaneKey, Split};
use crate::i18n::{tr, trc, trf};
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
    pub tags: Vec<String>,
    /// Shown first in the library.
    pub pinned: bool,
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
    /// Page size (pt) for the notebook being created (also remembered for the next).
    NewSize(f32, f32),
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
    /// The pair code typed or pasted (from another device's "koppeln"), and using it.
    PairEnter(String),
    PairAccept,
    PairToTablet,
    PairSent(Result<String, String>),
    ShowShare(bool),
    /// An invite's rights: read (false) or read-write (true).
    SetRole(String, bool),
    GuestFrames(bool),
    ShareView(bool),
    /// Bring one guest (or all: None) to where this laptop looks.
    Summon(Option<String>),
    /// Look where a device looks (last view it showed).
    JumpTo(String),
    /// As a guest: follow the owner's view on / off.
    FollowHost(bool),
    /// Open / close an invite's settings in the share panel.
    ToggleInvite(String),
    /// Waiting guest: let in (true) or turn away.
    Admit(String, bool),
    /// First click asks, the second removes the guest device.
    RemoveGuest(String),
    AllowAgain(String, String),
    /// Invite valid for N more days (0 = no expiry).
    SetExpiry(String, u64),
    /// Approval, single use.
    SetInviteOptions(String, bool, bool),
    /// PIN on (a new random one) / off.
    SetPin(String, bool),
    JoinPin(String),
    /// The service answered a join (link, its output or why not).
    Joined(String, Result<String, String>),
    /// After joining: the notebook arrived (or not, after the wait).
    JoinReady(String, bool),
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
    /// The page panel (paper and format of the page in view): open/close, paper, size, both for
    /// every page; turn the page in view (portrait ↔ landscape).
    ShowPage(bool),
    PagePaper(Background),
    PageSize(f32, f32),
    PageToAll,
    RotatePage,
    /// The warning before content would be cut off: change anyway (true) or leave it.
    ConfirmSize(bool),
    /// A size of its own, typed in mm: the fields open/closed, width, height.
    CustomSize(bool),
    SizeWidth(String),
    SizeHeight(String),
    /// Touchpad gestures (Wayland): the window to listen on, its display, a gesture, the frame
    /// clock (while the page glides or a found spot fades), gliding on/off.
    WindowOpened(iced::window::Id),
    /// n0te Tablet: a pen event (window coordinates), a tool bar action, the pen backend started.
    Pen(crate::tablet::pen::PenEvent),
    Tablet(crate::tablet::Msg),
    PenAttached(bool),
    /// The part of the one-second tick both programs share.
    TickRest,
    /// Updates: look now, what a look found, install the ready one and start again.
    CheckUpdate,
    UpdateStatus(crate::updater::Status),
    UpdateNow,
    AutoUpdate(bool),
    WaylandDisplay(Option<usize>),
    Gesture(Gesture),
    Glide(Instant),
    Momentum(bool),
    /// "auto", "de" or "en".
    Language(String),
    /// Search in the outline's field: typed; the pause after a key is over (the key's number);
    /// the results (search number, query, pages; None: a newer search took over); Enter (show
    /// the first hit); empty the field; Ctrl+F.
    SearchQuery(String),
    SearchDue(u64),
    Found(u64, String, Option<Vec<search::PageHits>>),
    SearchSubmit,
    SearchClear,
    FocusSearch,
    /// Show a spot of a sheet (sheet points) and light it up: a search hit, a heading.
    Reveal(String, [f32; 4]),
    /// Headings read in the background: the notebook, each sheet's headings.
    Headings(String, Vec<(String, Vec<Heading>)>),
    /// Coordinate systems: a click on one (sheet, plot) opens its dialog; a new one on the page
    /// in view; the dialog's fields and buttons.
    EditPlot(String, String),
    InsertPlot,
    Plot(crate::plots::Msg),
    /// The library's sidebar, search, menus, dialogs, selection and drag and drop.
    Shelf(crate::shelf::Msg),
    /// The outline: list or pictures, marked pages, the page menu, new pages and groups.
    Outline(sidebar::Msg),
    /// Two notebooks side by side: a message for one pane (from its view, or from a task it
    /// started); a press or a scroll in a pane makes it the active one; the split's controls.
    Pane(PaneKey, Box<Message>),
    Activate(PaneKey),
    Split(split::Msg),
}

/// The text box being edited: its sheet stays open while typing. Only in the active pane (the
/// keyboard goes with it).
pub struct Editing {
    pub pane: PaneKey,
    pub notebook: String,
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

/// A page size change held back because content would stick out: the sheets (nodes), the new
/// size, what sticks out ("3 Striche und 1 Bild", how many objects, on how many pages), and the
/// line to show once it's done.
#[derive(Debug, Clone, PartialEq)]
pub struct SizeWarning {
    pub nodes: Vec<String>,
    pub w: f32,
    pub h: f32,
    pub what: String,
    pub count: usize,
    pub pages: usize,
    pub done: Option<String>,
}

/// What to export as PDF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportWhat {
    All,
    Current,
    Picked,
}

/// A notebook on screen: alone, or one of two side by side (a pane, see [`split`]). Two panes
/// showing the same notebook share its document (`nb`): an edit in one shows in the other at
/// once, and the notebook's undo is one. The view of it is the pane's own.
pub struct Open {
    /// Names this pane while it is open.
    pub key: PaneKey,
    pub id: String,
    pub nb: Rc<Notebook>,
    pub rows: Vec<OutlineRow>,
    /// The pages marked to find again (main and side pages, outline order).
    pub marked: Vec<String>,
    pub pages: Pages,
    pub camera: Camera,
    /// The outline column shows.
    pub sidebar: bool,
    /// The outline's search field and what it found in this notebook.
    pub search: Search,
    /// The outline's own state: only the marked pages, its page menu, renaming.
    pub outline: sidebar::Outline,
    /// The text page being edited here (full page, or docked as the notes column).
    pub text_page: Option<TextPage>,
    /// A page to show once the view knows its size (opened beside at a page).
    pub start: Option<String>,
}

pub enum Screen {
    Library,
    Notebook(Box<Split>),
}

/// Headings to read (again) in one notebook: every sheet's, these sheets', and the ones not
/// read yet.
#[derive(Debug, Default)]
struct Due {
    all: bool,
    sheets: HashSet<String>,
    check: bool,
}

impl Due {
    fn is_empty(&self) -> bool {
        !self.all && !self.check && self.sheets.is_empty()
    }
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
    /// Guests in our notebooks right now, and notebooks we joined as a guest.
    pub guests: Vec<live::Guest>,
    pub joined: Vec<live::Joined>,
    /// New guests waiting to be let in (our invites with approval).
    pub waiting: Vec<live::Guest>,
    /// The guest whose "Entfernen" was clicked once (second click removes).
    pub removing: Option<String>,
    /// Joining an invite with a PIN: what's typed so far (Some: the field shows).
    pub join_pin: Option<String>,
    /// As a guest: following the owner's view (its device id).
    pub follow_host: Option<String>,
    /// Last view each device showed: (sheet, viewport in sheet points) — jump there.
    pub last_views: std::collections::HashMap<String, (String, [f32; 4])>,
    /// The invite whose settings are open in the share panel.
    pub invite_open: Option<String>,
    /// When where this laptop looks was last sent, per notebook.
    presence_at: HashMap<String, Instant>,
    pub pair_code: Option<String>,
    /// A pair code from another device, being typed / pasted.
    pub pair_enter: String,
    /// Share panel open, with the invites of the open notebook.
    pub share: Option<Vec<live::Invite>>,
    /// The newest invite link as a QR code (the view borrows it).
    pub share_qr: Option<iced::widget::qr_code::Data>,
    /// The pairing code as a QR code (the tablet scans it; no n0bridge needed).
    pub pair_qr: Option<iced::widget::qr_code::Data>,
    pub joining: Option<String>,
    /// History panel open: checkpoints (unix time, "date  size").
    pub history: Option<Vec<(u64, String)>>,
    /// Change stamps of the notebooks on screen (saves by other processes on this machine).
    stamps: HashMap<String, u64>,
    /// Without a service connection (tests): don't try to connect.
    pub offline: bool,
    pub entries: Vec<Entry>,
    pub bundles: Vec<Bundle>,
    pub screen: Screen,
    pub status: Option<String>,
    pub creating: Option<String>,
    /// Paper and spacing (pt) of the notebook being created; its size is in the settings.
    pub new_paper: Background,
    pub new_spacing: f32,
    /// Page panel open.
    pub page_panel: bool,
    /// A size of its own being typed (mm): Some while those fields are open.
    pub size_text: Option<(String, String)>,
    /// Touchpad: a pinch under way (its pane, the zoom at its start, the fingers' centre in the
    /// view), and the page gliding after a swipe (its pane, velocity px/s, last frame).
    pub pinch: Option<(PaneKey, f32, Point)>,
    /// A size change that would cut content off, waiting for "Trotzdem ändern".
    pub size_warning: Option<SizeWarning>,
    pub glide: Option<(PaneKey, f32, f32, Instant)>,
    pub show_imports: bool,
    /// The outline shows in a notebook opened from the library (the last choice).
    pub sidebar: bool,
    /// The headings of the notebooks on screen, by sheet id (top to bottom), read in the
    /// background.
    pub headings: HashMap<String, Vec<Heading>>,
    /// Headings to read (again), per notebook; one read at a time.
    headings_due: HashMap<String, Due>,
    headings_reading: bool,
    /// The "Koordinatensystem" panel, open for one plot, and the pane it belongs to.
    pub plot_edit: Option<crate::plots::Editor>,
    pub plot_pane: PaneKey,
    /// How the library is organised and looked at (folder, search, menu, dialog, selection).
    pub shelf: crate::shelf::Shelf,
    /// The notebook whose last deleted page or group "Rückgängig" brings back.
    pub undo_delete: Option<String>,
    /// The last pane key handed out.
    last_key: PaneKey,
    /// n0te Tablet: tools, the stroke being written, selection, undo (unused in n0te Laptop).
    pub tablet: crate::tablet::Tablet,
    /// Updates (Windows): what the last look found, and when it ran.
    pub update: crate::updater::Status,
    update_at: Option<Instant>,
    /// When the app started (the first update look waits a little).
    started: Instant,
}

/// The library folder, the one the sync service uses: `$N0TE_LIBRARY`, else
/// `$XDG_DATA_HOME/n0te` (~/.local/share/n0te); Windows `%APPDATA%\n0te`.
pub fn default_root() -> PathBuf {
    n0te_platform::paths::library_root()
}

/// Where n0bridge drops received files, and Downloads itself.
fn bundle_dirs() -> Vec<PathBuf> {
    let downloads = n0te_platform::paths::downloads().unwrap_or_else(|| PathBuf::from("Downloads"));
    vec![downloads.join("n0bridge"), downloads]
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
            settings,
            persist_settings: false,
            palette: None,
            show_help: false,
            show_settings: false,
            sync: None,
            devices: Vec::new(),
            devices_at: None,
            live_session: None,
            guests: Vec::new(),
            joined: Vec::new(),
            waiting: Vec::new(),
            removing: None,
            join_pin: None,
            follow_host: None,
            last_views: std::collections::HashMap::new(),
            invite_open: None,
            presence_at: HashMap::new(),
            pair_code: None,
            pair_enter: String::new(),
            share: None,
            share_qr: None,
            pair_qr: None,
            joining: None,
            history: None,
            stamps: HashMap::new(),
            offline: true,
            entries: Vec::new(),
            bundles: Vec::new(),
            screen: Screen::Library,
            status: None,
            creating: None,
            new_paper: Background::Grid,
            new_spacing: 17.0,
            page_panel: false,
            size_text: None,
            pinch: None,
            glide: None,
            size_warning: None,
            show_imports: false,
            sidebar: true,
            headings: HashMap::new(),
            headings_due: HashMap::new(),
            headings_reading: false,
            plot_edit: None,
            plot_pane: 0,
            shelf: crate::shelf::Shelf::default(),
            undo_delete: None,
            last_key: 0,
            tablet: Default::default(),
            update: crate::updater::Status::Idle,
            update_at: None,
            started: Instant::now(),
        };
        // Folders made here that are still empty (the settings keep them).
        app.shelf.made = app.settings.lib_folders.iter().cloned().collect();
        app.shelf.empty = app.shelf.made.clone();
        app.refresh();
        app
    }

    /// Starts the app; `import` = a `.n0te` file given on the command line (opened after import).
    pub fn boot_with(root: PathBuf, import: Option<PathBuf>) -> (Self, Task<Message>) {
        let mut app = Self::with_settings(root, Settings::load());
        crate::i18n::set_from(&app.settings.language);
        app.persist_settings = true;
        app.offline = false;
        if crate::mode() == crate::Mode::Tablet {
            app.tablet = crate::tablet::Tablet::load();
        }
        app.connect_sync();
        let task = import.map(|p| Task::done(Message::Import(p))).unwrap_or_else(Task::none);
        // Started from an invite link: the join dialog opens with it.
        let task = match crate::start_link() {
            Some(link) => Task::batch([task, Task::done(Message::JoinLink(link.to_string()))]),
            None => task,
        };
        // Touchpad gestures listen on the main window's Wayland display (its "opened" event
        // comes before subscriptions run).
        let gestures = iced::window::latest().and_then(|id| iced::window::run(id, crate::gestures::wayland_display)).map(Message::WaylandDisplay);
        // n0te Tablet reads the pen from the window itself (Windows).
        let pen = if crate::mode() == crate::Mode::Tablet { iced::window::latest().and_then(|id| iced::window::run(id, crate::tablet::pen::attach)).map(Message::PenAttached) } else { Task::none() };
        (app, Task::batch([task, gestures, pen]))
    }

    pub fn theme(&self) -> Theme {
        theme::theme()
    }

    pub fn title(&self) -> String {
        match &self.screen {
            Screen::Library => crate::app_name().into(),
            Screen::Notebook(o) => format!("{} — {}", crate::app_name(), o.nb.title()),
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
                // A click catches a gliding page.
                iced::Event::Mouse(iced::mouse::Event::ButtonPressed(_)) => Some(Message::Gesture(Gesture::Touch)),
                _ => None,
            }),
            iced::window::open_events().map(Message::WindowOpened),
            crate::gestures::subscription().map(Message::Gesture),
            if crate::mode() == crate::Mode::Tablet { crate::tablet::pen::subscription().map(Message::Pen) } else { Subscription::none() },
            // n0te Tablet: frames while a stroke is drawn (shapes snap when the pen rests).
            if crate::mode() == crate::Mode::Tablet && self.tablet_wants_frames() { iced::window::frames().map(|_| Message::Tablet(crate::tablet::Msg::Frame)) } else { Subscription::none() },
            // The frame clock: a gliding page, a found spot fading.
            if self.glide.is_some() || matches!(&self.screen, Screen::Notebook(s) if s.panes.iter().any(|o| o.pages.flash.as_ref().is_some_and(|f| !f.done()))) {
                iced::window::frames().map(Message::Glide)
            } else {
                Subscription::none()
            },
        ])
    }

    pub(crate) fn refresh(&mut self) {
        let lib = &self.lib;
        let mut entries: Vec<Entry> = lib
            .notebook_ids()
            .unwrap_or_default()
            .into_iter()
            .filter_map(|id| {
                let nb = lib.load_notebook(&id).ok()?;
                let modified = std::fs::metadata(lib.root().join("notebooks").join(&id)).and_then(|m| m.modified()).ok();
                Some(Entry { title: nb.title(), folder: nb.folder(), sheets: nb.sheet_count(), cover: nb.cover(), modified, tags: nb.tags(), pinned: nb.pinned(), id })
            })
            .collect();
        entries.sort_by(|a, b| (a.folder.to_lowercase(), a.title.to_lowercase()).cmp(&(b.folder.to_lowercase(), b.title.to_lowercase())));
        self.entries = entries;
        self.bundles = find_bundles(&bundle_dirs());
    }

    /// Shows notebook `id`: from the library alone; in a notebook in the pane messages are for
    /// (the same notebook again keeps its search and outline until read again).
    fn open(&mut self, id: &str) -> Result<(), String> {
        let key = match &self.screen {
            Screen::Notebook(s) => s.key(),
            Screen::Library => self.new_key(),
        };
        let mut pane = self.make_pane(key, id)?;
        self.finish_text();
        self.close_text_page();
        match &mut self.screen {
            Screen::Notebook(s) => {
                let i = s.index(key).unwrap_or(0);
                let old = &mut s.panes[i];
                pane.sidebar = old.sidebar;
                let same = old.id == id;
                if same {
                    pane.search = std::mem::take(&mut old.search);
                    pane.search.stale = true;
                    pane.outline = std::mem::take(&mut old.outline);
                }
                let before = std::mem::replace(&mut s.panes[i], pane).id;
                if !same {
                    if self.plot_pane == key {
                        self.plot_edit = None;
                    }
                    self.gone(&before);
                }
            }
            Screen::Library => {
                pane.sidebar = self.sidebar;
                self.headings.clear();
                self.screen = Screen::Notebook(Box::new(Split::new(pane)));
            }
        }
        self.watch(id);
        Ok(())
    }

    /// A new pane key.
    pub(crate) fn new_key(&mut self) -> PaneKey {
        self.last_key += 1;
        self.last_key
    }

    /// A pane showing notebook `id` (its outline on, the first page in view once it has a size),
    /// sharing the document with a pane that shows it already.
    pub(crate) fn make_pane(&self, key: PaneKey, id: &str) -> Result<Open, String> {
        let shared = match &self.screen {
            Screen::Notebook(s) => s.panes.iter().find(|p| p.id == id).map(|p| p.nb.clone()),
            Screen::Library => None,
        };
        let nb = match shared {
            Some(nb) => nb,
            None => Rc::new(self.lib.load_notebook(id).map_err(|e| e.to_string())?),
        };
        let rows = nb.outline(false);
        let pages = self.new_pages(id, nl::layout(&rows, &nb.outline(true), self.settings.layout()));
        Ok(Open {
            key,
            id: id.to_string(),
            marked: nb.marked(),
            rows,
            nb,
            pages,
            camera: Camera::default(),
            sidebar: true,
            search: Search::default(),
            outline: sidebar::Outline::default(),
            text_page: None,
            start: None,
        })
    }

    /// The pages of notebook `id` laid out, read from the library as they come into view.
    fn new_pages(&self, id: &str, layout: nl::Layout) -> Pages {
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
                        plots: s.plots(),
                    })
                    .unwrap_or_default()
            }),
            self.pdf.clone(),
        );
        pages.show_remote_frame.set(self.settings.remote_frame);
        pages
    }

    /// Notebook `id` came on screen: saves elsewhere are watched for, its headings read.
    pub(crate) fn watch(&mut self, id: &str) {
        self.stamps.insert(id.to_string(), self.lib.change_stamp(id));
        self.headings_due.entry(id.to_string()).or_default().all = true;
    }

    /// Notes that notebook `id` was opened now (the start's "Zuletzt geöffnet", the ⧉ menu).
    pub(crate) fn opened(&mut self, id: &str) {
        let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
        self.settings.opened_now(id, now);
        if self.persist_settings {
            self.settings.save();
        }
    }

    /// Notebook `nb` left the screen (its pane closed or shows another one): who saw where I
    /// looked in it sees no frame any more.
    pub(crate) fn gone(&mut self, nb: &str) {
        if matches!(&self.screen, Screen::Notebook(s) if s.shows(nb)) {
            return;
        }
        self.stamps.remove(nb);
        self.headings_due.remove(nb);
        self.presence_at.remove(nb);
        if self.watched(nb)
            && let Some(sync) = &self.sync
        {
            sync.send(&["presence".to_string(), nb.to_string(), String::new()]);
        }
    }

    /// Runs `f` for pane `key` (the pane messages are for meanwhile); None when it isn't open.
    pub(crate) fn in_pane<R>(&mut self, key: PaneKey, f: impl FnOnce(&mut App) -> R) -> Option<R> {
        let Screen::Notebook(s) = &mut self.screen else { return None };
        s.index(key)?;
        let before = s.retarget(key);
        let r = f(self);
        if let Screen::Notebook(s) = &mut self.screen {
            let back = if s.index(before).is_some() { before } else { s.active };
            s.retarget(back);
        }
        Some(r)
    }

    /// The pane messages are for.
    pub(crate) fn pane(&self) -> Option<&Open> {
        match &self.screen {
            Screen::Notebook(s) => Some(&***s),
            Screen::Library => None,
        }
    }

    pub(crate) fn pane_mut(&mut self) -> Option<&mut Open> {
        match &mut self.screen {
            Screen::Notebook(s) => Some(&mut ***s),
            Screen::Library => None,
        }
    }

    /// The pane showing notebook `nb` that speaks for it (see [`Split::owner`]).
    pub fn pane_showing(&self, nb: &str) -> Option<PaneKey> {
        match &self.screen {
            Screen::Notebook(s) => s.owner(nb),
            Screen::Library => None,
        }
    }

    /// The panes on screen (keys, left to right).
    fn pane_keys(&self) -> Vec<PaneKey> {
        match &self.screen {
            Screen::Notebook(s) => s.panes.iter().map(|p| p.key).collect(),
            Screen::Library => Vec::new(),
        }
    }

    /// The ids of the outline of the pane messages are for (see [`sidebar::Ids`]).
    fn ids(&self) -> sidebar::Ids {
        sidebar::Ids { other: matches!(&self.screen, Screen::Notebook(s) if s.key() != s.active) }
    }

    /// The panels the toolbar opened close (they belong to the active pane), and a size change
    /// still asking is left.
    pub(crate) fn close_panels(&mut self) {
        self.page_panel = false;
        self.size_text = None;
        self.size_warning = None;
        self.share = None;
        self.history = None;
        self.show_pdf_import = false;
        self.export = None;
        if let Screen::Notebook(s) = &mut self.screen {
            s.menu = None;
        }
    }

    pub(crate) fn say(&mut self, s: impl Into<String>) {
        self.status = Some(s.into());
    }

    /// Runs a message, then what it left to do in the background: headings to read (again), a
    /// search to run again after the notebook changed.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        let typing = self.editing.is_some();
        let task = self.handle(message);
        // n0te Tablet: the touch keyboard comes up when typing starts, and goes when it ends
        // (Windows; Windows doesn't do it by itself for iced's text fields).
        if crate::mode() == crate::Mode::Tablet && self.persist_settings && typing != self.editing.is_some() {
            if self.editing.is_some() { crate::tablet::keyboard::show() } else { crate::tablet::keyboard::hide() };
        }
        // Between messages they are for the active pane.
        if let Screen::Notebook(s) = &mut self.screen {
            let active = s.active;
            s.retarget(active);
        }
        let more = self.background();
        if crate::mode() == crate::Mode::Tablet {
            crate::tablet::pen::set_ink_areas(self.ink_areas());
        }
        if more.is_empty() { task } else { Task::batch(std::iter::once(task).chain(more)) }
    }

    fn handle(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Open(id) => {
                self.shelf.menu = None;
                self.shelf.drag = None;
                match self.open(&id) {
                    Ok(()) => {
                        self.opened(&id);
                        return sidebar::reveal_current(self);
                    }
                    Err(e) => self.say(trf("Notizbuch nicht lesbar: {}", &[&e])),
                }
            }
            Message::Back => {
                // What is being written is saved, in every pane.
                self.finish_text();
                for key in self.pane_keys() {
                    self.in_pane(key, |app| app.close_text_page());
                }
                self.plot_edit = None;
                self.live_session = None;
                self.share = None;
                self.history = None;
                self.undo_delete = None;
                self.screen = Screen::Library;
                self.stamps.clear();
                self.headings_due.clear();
                self.presence_at.clear();
                self.refresh();
            }
            Message::Refresh => self.refresh(),
            Message::Camera(c) => {
                let Screen::Notebook(s) = &mut self.screen else { return Task::none() };
                let speaks = s.owner(&s.id) == Some(s.key());
                let id = s.id.clone();
                let o = &mut ***s;
                let before = o.camera;
                o.camera = c;
                // Opened at a page: shown once the view knows its size.
                if c.viewport.width > 0.0
                    && let Some(sheet) = o.start.take()
                    && let Some(i) = o.pages.page_of_sheet(&sheet)
                {
                    o.camera = o.pages.camera_for(i, &c);
                }
                // Moved by hand (not resized): following the owner ends (it would snap back).
                if speaks && before.viewport == c.viewport && self.follow_host.take().is_some() {
                    self.say(tr("Folgen beendet"));
                }
                self.follow_notes();
                if speaks {
                    self.send_presence(&id);
                }
            }
            Message::Fold(node) => {
                let Some(o) = self.pane() else { return Task::none() };
                let collapsed = o.rows.iter().any(|r| matches!(r, OutlineRow::Group { node: n, collapsed: true, .. } if *n == node));
                if o.nb.set_collapsed(&node, !collapsed).is_ok() {
                    let _ = self.lib.save_notebook(&o.nb);
                    let id = o.id.clone();
                    self.relayout_book(&id);
                }
            }
            Message::Jump(sheet) => {
                if let Some(o) = self.pane_mut()
                    && let Some(i) = o.pages.page_of_sheet(&sheet)
                {
                    o.camera = o.pages.camera_for(i, &o.camera);
                }
            }
            Message::Zoom(z) => {
                if let Some(o) = self.pane_mut() {
                    let c = o.camera;
                    let (cx, cy) = (c.x + c.viewport.width / c.zoom / 2.0, c.y + c.viewport.height / c.zoom / 2.0);
                    let zoom = z.clamp(pages::MIN_ZOOM, pages::MAX_ZOOM);
                    o.camera = o.pages.clamp(Camera { x: cx - c.viewport.width / zoom / 2.0, y: cy - c.viewport.height / zoom / 2.0, zoom, ..c });
                }
            }
            Message::ToggleSidebar => {
                let Some(o) = self.pane_mut() else { return Task::none() };
                o.sidebar = !o.sidebar;
                self.sidebar = o.sidebar;
                if self.sidebar {
                    return sidebar::reveal_current(self);
                }
            }
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
                    match &self.screen {
                        Screen::Library => {
                            let _ = self.open(&id);
                        }
                        // On screen: what came in shows where it is open.
                        Screen::Notebook(s) if s.shows(&id) => self.refresh_open(&id, true, &[]),
                        Screen::Notebook(_) => {}
                    }
                    self.say(trf("„{}“ übernommen (mit dem Stand hier zusammengeführt)", &[&title]));
                }
                Err(e) => self.say(trf("Import fehlgeschlagen: {}", &[&e])),
            },
            Message::SendToTablet => {
                // Only through n0bridge (the palette offers it only then).
                if !n0bridge_installed() {
                    return Task::none();
                }
                let Screen::Notebook(o) = &self.screen else { return Task::none() };
                let dir = std::env::temp_dir().join("n0te-out");
                let safe: String = o.nb.title().chars().map(|c| if c.is_alphanumeric() || c == ' ' || c == '-' { c } else { '_' }).collect();
                let path = dir.join(format!("{}.n0te", safe.trim().replace(' ', "_")));
                let bytes = match self.lib.export_bundle(&o.id) {
                    Ok(b) => b,
                    Err(e) => {
                        self.say(trf("Export fehlgeschlagen: {}", &[&e]));
                        return Task::none();
                    }
                };
                if let Err(e) = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, bytes)) {
                    self.say(trf("Export fehlgeschlagen: {}", &[&e]));
                    return Task::none();
                }
                self.say(tr("Sende an das Tablet …"));
                return Task::perform(send_with_n0bridge(path), Message::Sent);
            }
            Message::Sent(r) => self.say(match r {
                Ok(_) => tr("An das Tablet gesendet — dort in n0te: IMPORTIEREN").to_string(),
                Err(e) => trf("Senden fehlgeschlagen: {}", &[&e]),
            }),
            Message::StartCreate => self.creating = Some(String::new()),
            Message::NewPaper(b) => {
                if b != self.new_paper {
                    self.new_paper = b;
                    self.new_spacing = default_spacing(b);
                }
            }
            Message::NewSpacing(v) => self.new_spacing = v,
            Message::NewSize(w, h) => {
                self.settings.new_w = w;
                self.settings.new_h = h;
                self.size_text = None;
                if self.persist_settings {
                    self.settings.save();
                }
            }
            Message::NewTitle(t) => self.creating = Some(t),
            Message::CancelCreate => {
                self.creating = None;
                self.size_text = None;
            }
            Message::Create => {
                let title = self.creating.take().unwrap_or_default();
                let title = if title.trim().is_empty() { tr("Neues Notizbuch").to_string() } else { title.trim().to_string() };
                let made = self.lib.create_notebook(&title).and_then(|nb| {
                    let (w, h) = (self.settings.new_w, self.settings.new_h);
                    nb.add_sheet(None, 0, &SheetSpec { width: w, height: h, background: self.new_paper, spacing: self.new_spacing, pdf: None, gaps: Vec::new() })?;
                    self.lib.save_notebook(&nb)?;
                    Ok(nb.id())
                });
                match made {
                    Ok(id) => {
                        self.refresh();
                        let _ = self.open(&id);
                    }
                    Err(e) => self.say(trf("Anlegen fehlgeschlagen: {}", &[&e])),
                }
            }
            Message::DismissStatus => self.status = None,
            Message::ShowPage(on) => {
                self.page_panel = on;
                self.size_text = None;
                if on {
                    self.share = None;
                    self.history = None;
                    self.show_pdf_import = false;
                }
            }
            Message::PagePaper(b) => {
                if let Some((node, spec)) = self.page_in_view() {
                    let spacing = if spec.background == b && spec.spacing >= 4.0 { spec.spacing } else { default_spacing(b) };
                    self.edit_notebook(|nb| nb.set_paper(&node, b, spacing));
                }
            }
            Message::PageSize(w, h) => {
                self.size_text = None;
                if let Some((node, _)) = self.page_in_view() {
                    self.resize(vec![node], w, h, None);
                }
            }
            Message::RotatePage => match self.page_in_view() {
                Some((node, spec)) if spec.pdf.is_none() && !spec.is_board() => {
                    let (w, h) = format::oriented(spec.width, spec.height, spec.width <= spec.height);
                    self.resize(vec![node], w, h, Some(trf("Seite gedreht: {}", &[&sidebar::format_name(w, h)])));
                }
                Some(_) => self.say(tr("PDF-Seiten und Boards lassen sich nicht drehen")),
                None => {}
            },
            Message::PageToAll => {
                if let (Some((_, spec)), Screen::Notebook(o)) = (self.page_in_view(), &self.screen) {
                    let sheets = plain_sheets(&o.nb);
                    let nodes: Vec<String> = sheets.iter().map(|(n, _)| n.clone()).collect();
                    // Paper first (nothing is cut off by it); the size may need a warning.
                    if spec.background != Background::Text {
                        self.edit_notebook(|nb| {
                            for (node, s) in &sheets {
                                // A text page keeps being one, and a text page passes on its size only.
                                if s.background != Background::Text {
                                    nb.set_paper(node, spec.background, spec.spacing)?;
                                }
                            }
                            Ok(())
                        });
                    }
                    let done = trf("{} für {} Seiten übernommen", &[&sidebar::format_name(spec.width, spec.height), &nodes.len()]);
                    self.resize(nodes, spec.width, spec.height, Some(done));
                }
            }
            Message::ConfirmSize(go) => {
                if let Some(w) = self.size_warning.take()
                    && go
                {
                    self.apply_size(&w.nodes, w.w, w.h, w.done);
                }
            }
            Message::CustomSize(open) => {
                self.size_text = open.then(|| {
                    let (w, h) = match (&self.screen, self.page_in_view()) {
                        (Screen::Notebook(_), Some((_, s))) if self.page_panel => (s.width, s.height),
                        _ => (self.settings.new_w, self.settings.new_h),
                    };
                    (format!("{:.0}", format::pt_to_mm(w)), format!("{:.0}", format::pt_to_mm(h)))
                });
            }
            Message::SizeWidth(t) => {
                if let Some((w, _)) = &mut self.size_text {
                    *w = t;
                }
            }
            Message::SizeHeight(t) => {
                if let Some((_, h)) = &mut self.size_text {
                    *h = t;
                }
            }
            Message::WindowOpened(id) => {
                let gestures = iced::window::run(id, crate::gestures::wayland_display).map(Message::WaylandDisplay);
                if crate::mode() == crate::Mode::Tablet {
                    return Task::batch([gestures, iced::window::run(id, crate::tablet::pen::attach).map(Message::PenAttached)]);
                }
                return gestures;
            }
            Message::CheckUpdate => return self.check_update(),
            Message::UpdateStatus(st) => self.update = st,
            Message::UpdateNow => {
                let mode = if crate::mode() == crate::Mode::Tablet { "tablet" } else { "laptop" };
                if crate::updater::install_now(mode) {
                    return iced::exit();
                }
                self.say(tr("Update ließ sich nicht starten"));
            }
            Message::AutoUpdate(on) => {
                self.settings.auto_update = on;
                if self.persist_settings {
                    self.settings.save();
                }
                if on {
                    return self.check_update();
                }
            }
            Message::Pen(e) => return self.on_pen(e),
            Message::Tablet(m) => return self.on_tablet(m),
            Message::PenAttached(_) => {}
            Message::WaylandDisplay(display) => {
                if let Some(d) = display {
                    crate::gestures::start(d);
                }
            }
            Message::Gesture(g) => return self.gesture(g),
            Message::Glide(now) => {
                if let Screen::Notebook(s) = &mut self.screen {
                    for o in s.panes.iter_mut().filter(|o| o.pages.flash.as_ref().is_some_and(|f| f.done())) {
                        o.pages.flash = None;
                    }
                }
                return self.glide_step(now);
            }
            Message::Language(l) => {
                crate::i18n::set_from(&l);
                self.settings.language = l;
                if self.persist_settings {
                    self.settings.save();
                }
            }
            Message::Momentum(on) => {
                self.settings.momentum = on;
                self.glide = None;
                if self.persist_settings {
                    self.settings.save();
                }
            }
            Message::TileReady => {}
            Message::FileDropped(p) => {
                let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                return match ext.as_str() {
                    "n0te" => self.handle(Message::Import(p)),
                    "pdf" => self.handle(Message::ImportPdf(p)),
                    _ => {
                        self.say(tr("Nur PDF- und .n0te-Dateien"));
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
                let _ = n0te_platform::open_path(p);
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
                let zoom = self.pane().map(|o| o.camera.zoom).unwrap_or(1.0);
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
                    self.store_sheet(&e.notebook, &e.sheet_id, &e.sheet);
                }
            }
            Message::DoneText => self.finish_text(),
            Message::Tick => {
                let due = self.update_at.map_or(self.persist_settings, |t| t.elapsed().as_secs() > 6 * 3600);
                if crate::updater::supported() && self.settings.auto_update && due && self.update != crate::updater::Status::Checking {
                    let first = self.update_at.is_none();
                    // The first look a little after start, not while the window opens.
                    if !first || self.started.elapsed().as_secs() > 20 {
                        let t = self.check_update();
                        let rest = self.handle(Message::TickRest);
                        let tablet = if crate::mode() == crate::Mode::Tablet { self.tablet_tick() } else { Task::none() };
                        return Task::batch([t, rest, tablet]);
                    }
                }
                if crate::mode() == crate::Mode::Tablet {
                    let t = self.tablet_tick();
                    let rest = self.handle(Message::TickRest);
                    return Task::batch([t, rest]);
                }
                return self.handle(Message::TickRest);
            }
            Message::TickRest => {
                if self.editing.as_ref().is_some_and(|e| e.dirty && e.last_change.elapsed().as_millis() > 800) {
                    self.save_text(false);
                }
                // Text pages typed in, after a pause (in every pane).
                let typed: Vec<PaneKey> = match &self.screen {
                    Screen::Notebook(s) => s.panes.iter().filter(|o| o.text_page.as_ref().is_some_and(|t| t.dirty && t.last_change.elapsed().as_millis() > 800)).map(|o| o.key).collect(),
                    Screen::Library => Vec::new(),
                };
                for key in typed {
                    self.in_pane(key, |app| app.save_text_page());
                }
                if !self.offline {
                    if self.sync.is_none() {
                        self.connect_sync();
                    } else if self.devices_at.is_none_or(|t| t.elapsed().as_secs() >= 10) {
                        self.devices = live::devices();
                self.refresh_guests();
                        self.devices_at = Some(Instant::now());
                    }
                }
                // Saves by another process here (the sync service, a second window), each
                // notebook on screen once.
                let shown = match &self.screen {
                    Screen::Notebook(s) => s.notebooks(),
                    Screen::Library => Vec::new(),
                };
                for nb in shown {
                    let now = self.lib.change_stamp(&nb);
                    if self.stamps.insert(nb.clone(), now).is_some_and(|before| before != now) {
                        self.refresh_open(&nb, true, &[]);
                    }
                }
            }
            Message::Live(e) => return self.on_live(e),
            Message::StartLive => {
                let Some(d) = self.devices.iter().find(|d| d.online).cloned() else {
                    self.say(tr("Kein Gerät online: Tablet koppeln (Einstellungen) und n0te dort öffnen"));
                    return Task::none();
                };
                // Live goes with ★: the tablet opens the main pane's notebook.
                let Screen::Notebook(s) = &self.screen else { return Task::none() };
                let nb = s.pane(s.main).map(|p| p.id.clone()).unwrap_or_default();
                if let Some(s) = &self.sync {
                    s.send(&["open".into(), d.id.clone(), nb]);
                }
                self.live_session = Some(LiveSession { device: d.id, name: d.name.clone(), follow_them: false, they_follow: false });
                self.send_presence_all();
                self.say(trf("Live mit {}: es öffnet das Notizbuch", &[&d.name]));
            }
            Message::StopLive => {
                if let (Some(l), Some(s)) = (&self.live_session, &self.sync)
                    && l.they_follow
                {
                    s.send(&["follow".into(), l.device.clone(), "0".into()]);
                }
                self.live_session = None;
                if let Screen::Notebook(s) = &self.screen {
                    for o in &s.panes {
                        o.pages.remote.borrow_mut().clear();
                    }
                }
            }
            Message::RemoteFrame(on) => {
                self.settings.remote_frame = on;
                if let Screen::Notebook(s) = &self.screen {
                    for o in &s.panes {
                        o.pages.show_remote_frame.set(on);
                    }
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
                self.send_presence_all();
            }
            Message::PairStart => match live::request(&["pair-code"]) {
                Ok(code) => {
                    self.pair_qr = iced::widget::qr_code::Data::new(code.as_bytes()).ok();
                    self.pair_code = Some(code);
                }
                Err(e) => self.say(trf("Koppeln: {}", &[&e])),
            },
            Message::PairEnter(t) => self.pair_enter = t,
            Message::PairAccept => {
                let code = self.pair_enter.trim().to_string();
                if code.is_empty() {
                    return Task::none();
                }
                match live::request(&["accept", &code]) {
                    Ok(name) => {
                        self.pair_enter.clear();
                        // The service says "Gekoppelt mit <name> (<id>)".
                        self.say(if name.trim().is_empty() { tr("Gekoppelt").to_string() } else { name.trim().to_string() });
                        self.devices = live::devices();
                    }
                    Err(e) => self.say(trf("Koppeln: {}", &[&e])),
                }
            }
            Message::PairToTablet => {
                let Some(code) = self.pair_code.clone().filter(|_| n0bridge_installed()) else { return Task::none() };
                return Task::perform(
                    async move {
                        let to = tablet_target();
                        n0bridge(vec!["clipboard".into(), "set".into(), "--to".into(), to, code]).await
                    },
                    Message::PairSent,
                );
            }
            Message::PairSent(r) => self.say(match r {
                Ok(_) => tr("Code ist in der Tablet-Zwischenablage: dort n0te → Einstellungen → LAPTOP KOPPELN").to_string(),
                Err(e) => trf("Senden fehlgeschlagen: {}", &[&e]),
            }),
            Message::ShowShare(on) => {
                self.share = if on {
                    let nb = self.notebook_id().unwrap_or_default();
                    Some(live::invites().into_iter().filter(|i| i.notebook == nb).collect())
                } else {
                    None
                };
                // The open invite as a QR code (scan with the tablet / phone).
                self.share_qr = self.share.as_ref().and_then(|v| v.iter().find(|i| Some(i.id.as_str()) == self.invite_open.as_deref() && !i.revoked)).and_then(|i| iced::widget::qr_code::Data::new(i.link.as_bytes()).ok());
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
                            self.say(trf("Verlauf: {}", &[&e]));
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
                        self.say(tr("Früherer Stand als neues Notizbuch wiederhergestellt (das aktuelle bleibt unverändert)"));
                    }
                    Err(e) => self.say(trf("Wiederherstellen: {}", &[&e])),
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
                        self.say(tr("Einladung erstellt und kopiert"));
                        // The new link opens (its QR code shows).
                        self.invite_open = live::invites().into_iter().find(|i| i.link == link).map(|i| i.id);
                        let _ = self.handle(Message::ShowShare(true));
                        return iced::clipboard::write(link);
                    }
                    Err(e) => self.say(trf("Einladen: {}", &[&e])),
                }
            }
            Message::SetRole(id, write) => {
                match live::request(&["set-role", &id, if write { "write" } else { "read" }]) {
                    Ok(_) => self.say(if write { tr("Gäste dürfen jetzt mitschreiben") } else { tr("Gäste dürfen jetzt nur lesen") }),
                    Err(e) => self.say(trf("Rechte: {}", &[&e])),
                }
                return self.handle(Message::ShowShare(true));
            }
            Message::GuestFrames(on) => {
                self.settings.guest_frames = on;
                if self.persist_settings {
                    self.settings.save();
                }
                if !on && let Screen::Notebook(s) = &self.screen {
                    let names: Vec<String> = self.guests.iter().map(|g| g.name.clone()).collect();
                    for o in &s.panes {
                        o.pages.remote.borrow_mut().retain(|m| !names.contains(&m.name));
                    }
                }
            }
            Message::ShareView(on) => {
                self.settings.share_view = on;
                if self.persist_settings {
                    self.settings.save();
                }
                if let Some(s) = &self.sync {
                    s.send(&["share-view".to_string(), if on { "on" } else { "off" }.to_string()]);
                }
            }
            Message::JoinReady(id, ready) => {
                self.refresh();
                self.refresh_guests();
                if ready {
                    let _ = self.open(&id);
                    self.say(tr("Beigetreten"));
                } else {
                    self.say(tr("Beigetreten — das Notizbuch kommt, sobald der Besitzer erreichbar ist"));
                }
            }
            Message::Summon(device) => {
                // To where I look in this notebook (the main pane's view when it shows it twice).
                let Some(nb) = self.notebook_id() else { return Task::none() };
                let Some((sheet, r)) = self.view_in(&nb) else { return Task::none() };
                let mut args: Vec<String> = vec!["summon".into(), nb, sheet];
                args.extend(r.iter().map(|x| format!("{x:.1}")));
                if let Some(d) = &device {
                    args.push(d.clone());
                }
                let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                match live::request(&refs) {
                    Ok(_) => self.say(match &device {
                        Some(d) => trf("{} kommt zu dir", &[&self.guests.iter().find(|g| &g.device == d).map(|g| g.name.clone()).unwrap_or_else(|| tr("Der Gast").into())]),
                        None => tr("Alle Gäste kommen zu dir").into(),
                    }),
                    Err(e) => self.say(trf("Holen: {}", &[&e])),
                }
            }
            Message::JumpTo(device) => {
                if let Some((sheet, r)) = self.last_views.get(&device).cloned() {
                    self.look_at(&sheet, r);
                } else {
                    self.say(tr("Diese Ansicht ist gerade nicht bekannt"));
                }
            }
            Message::FollowHost(on) => {
                let nb = self.notebook_id().unwrap_or_default();
                let owner = self.joined.iter().find(|j| j.notebook == nb).map(|j| j.owner.clone());
                self.follow_host = if on { owner } else { None };
                if let Some(d) = self.follow_host.clone()
                    && let Some((sheet, r)) = self.last_views.get(&d).cloned()
                    && let Some(key) = self.pane_showing(&nb)
                {
                    self.in_pane(key, |app| app.look_at(&sheet, r));
                }
            }
            Message::ToggleInvite(id) => {
                self.invite_open = if self.invite_open.as_deref() == Some(id.as_str()) { None } else { Some(id) };
                return self.handle(Message::ShowShare(true));
            }
            Message::Admit(device, yes) => {
                if let Err(e) = live::request(&[if yes { "admit" } else { "deny" }, &device]) {
                    self.say(trf("Einlass: {}", &[&e]));
                }
                self.refresh_guests();
                return self.handle(Message::ShowShare(true));
            }
            Message::RemoveGuest(device) => {
                if self.removing.as_deref() != Some(device.as_str()) {
                    self.removing = Some(device);
                    return Task::none();
                }
                self.removing = None;
                let name = self.guests.iter().find(|g| g.device == device).map(|g| g.name.clone()).unwrap_or_else(|| tr("Der Gast").into());
                match live::request(&["remove-guest", &device]) {
                    Ok(_) => self.say(trf("{} ist entfernt und kommt mit dem Link nicht mehr herein", &[&name])),
                    Err(e) => self.say(trf("Entfernen: {}", &[&e])),
                }
                self.refresh_guests();
                return self.handle(Message::ShowShare(true));
            }
            Message::AllowAgain(invite, device) => {
                if let Err(e) = live::request(&["allow-again", &invite, &device]) {
                    self.say(trf("Zulassen: {}", &[&e]));
                }
                return self.handle(Message::ShowShare(true));
            }
            Message::SetExpiry(invite, days) => {
                if let Err(e) = live::request(&["set-expiry", &invite, &days.to_string()]) {
                    self.say(trf("Ablauf: {}", &[&e]));
                }
                return self.handle(Message::ShowShare(true));
            }
            Message::SetInviteOptions(invite, approve, single) => {
                let mut args = vec!["set-options", invite.as_str()];
                if approve {
                    args.push("--approve");
                }
                if single {
                    args.push("--single");
                }
                if let Err(e) = live::request(&args) {
                    self.say(trf("Einladung: {}", &[&e]));
                }
                return self.handle(Message::ShowShare(true));
            }
            Message::SetPin(invite, on) => {
                let pin = if on { new_pin() } else { "off".into() };
                if let Err(e) = live::request(&["set-pin", &invite, &pin]) {
                    self.say(format!("PIN: {e}"));
                }
                return self.handle(Message::ShowShare(true));
            }
            Message::Revoke(id) => {
                match live::request(&["revoke", &id]) {
                    Ok(_) => self.say(tr("Widerrufen: der Gast ist getrennt")),
                    Err(e) => self.say(trf("Widerrufen: {}", &[&e])),
                }
                return self.handle(Message::ShowShare(true));
            }
            Message::CopyText(t) => {
                self.say(tr("Kopiert"));
                return iced::clipboard::write(t);
            }
            Message::StartJoin => self.joining = Some(String::new()),
            Message::CancelJoin => self.joining = None,
            Message::JoinLink(l) => self.joining = Some(l),
            Message::JoinPin(p) => self.join_pin = Some(p.chars().filter(char::is_ascii_digit).take(8).collect()),
            Message::Join => {
                let link = self.joining.take().unwrap_or_default();
                let pin = self.join_pin.take().filter(|p| !p.is_empty());
                self.say(tr("Verbinde mit dem Besitzer …"));
                // Off the UI thread: with approval the owner may take a while to let us in.
                return Task::perform(
                    async move {
                        let mut args = vec!["join".to_string(), link.trim().to_string()];
                        if let Some(p) = pin {
                            args.extend(["--pin".to_string(), p]);
                        }
                        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
                        let r = live::request(&refs);
                        (link, r)
                    },
                    |(link, r)| Message::Joined(link, r),
                );
            }
            Message::Joined(link, r) => {
                match r {
                    Err(e) if e.contains("PIN") => {
                        // The invite has a PIN: the field shows, then join again.
                        self.joining = Some(link);
                        self.join_pin = Some(String::new());
                        self.say(if e.contains("falsch") { tr("Die PIN stimmt nicht") } else { tr("Diese Einladung braucht eine PIN") });
                    }
                    Ok(out) => {
                        let id = out.rsplit(' ').next().unwrap_or_default().to_string();
                        self.say(tr("Lade das Notizbuch vom Besitzer …"));
                        // It arrives in the background: open it once it is here (opening it at
                        // once found nothing), waiting at most 45 s.
                        let root = self.lib.root().to_path_buf();
                        return Task::perform(
                            async move {
                                let until = Instant::now() + std::time::Duration::from_secs(45);
                                let mut ready = false;
                                while !ready && Instant::now() < until {
                                    ready = Library::open(&root).and_then(|l| l.load_notebook(&id)).map(|nb| nb.sheet_count() > 0).unwrap_or(false);
                                    if !ready {
                                        std::thread::sleep(std::time::Duration::from_millis(400));
                                    }
                                }
                                (id, ready)
                            },
                            |(id, ready)| Message::JoinReady(id, ready),
                        );
                    }
                    Err(e) => self.say(trf("Beitreten: {}", &[&e])),
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
                self.relayout_all();
            }
            Message::SetSideGap(v) => {
                self.settings.side_gap = v;
                if self.persist_settings { self.settings.save(); }
                self.relayout_all();
            }
            Message::OpenTextPage(sheet, docked) => return self.open_text_page(&sheet, docked),
            Message::CloseTextPage => self.close_text_page(),
            Message::Blocks(m) => {
                if let Some(t) = self.pane_mut().and_then(|o| o.text_page.as_mut()) {
                    let task = t.doc.update(m).map(Message::Blocks);
                    if t.doc.to_markdown() != t.saved {
                        t.dirty = true;
                        t.last_change = Instant::now();
                    }
                    return task;
                }
            }
            Message::NewTextPage => {
                let Some(o) = self.pane() else { return Task::none() };
                let current = o.pages.current_number(&o.camera);
                let node = o.pages.index_of_number(current).map(|i| o.pages.pages[i].placed.node.clone());
                let (parent, index) = pdf::position_after(&o.nb, node.as_deref());
                let spec = SheetSpec::a4(Background::Text);
                match o.nb.add_sheet(parent.as_deref(), index, &spec) {
                    Ok((_, sheet)) => {
                        let _ = self.lib.save_notebook(&o.nb);
                        self.relayout();
                        if let Some(o) = self.pane_mut()
                            && let Some(i) = o.pages.page_of_sheet(&sheet)
                        {
                            o.camera = o.pages.camera_for(i, &o.camera);
                        }
                        return self.open_text_page(&sheet, false);
                    }
                    Err(e) => self.say(trf("Textseite: {}", &[&e])),
                }
            }
            Message::SearchQuery(q) => {
                let Some(search) = self.search_mut() else { return Task::none() };
                search.query = q;
                if !search.active() {
                    search.clear();
                    return Task::none();
                }
                search.stale = false;
                return self.search_soon();
            }
            Message::SearchDue(n) => {
                if self.pane().is_some_and(|o| n == o.search.typed && o.search.active()) {
                    return self.start_search();
                }
            }
            Message::Found(generation, query, found) => {
                let list = self.ids().list();
                let Some(search) = self.search_mut() else { return Task::none() };
                if let Some(pages) = found
                    && generation == search.latest.load(std::sync::atomic::Ordering::Relaxed)
                {
                    search.busy = false;
                    let other_words = search.found.as_ref().is_none_or(|(q, _)| *q != query);
                    search.found = Some((query, pages));
                    // New words: the list starts at the top (the same search again stays put).
                    if other_words {
                        return iced::widget::operation::snap_to(list, iced::widget::operation::RelativeOffset::START);
                    }
                }
            }
            Message::SearchSubmit => {
                let first = self.pane().and_then(|o| o.search.hits().next().map(|(p, h)| (p.sheet_id.clone(), [h.x0, h.y0, h.x1, h.y1])));
                if let Some((sheet, r)) = first {
                    self.reveal(&sheet, r);
                }
            }
            Message::SearchClear => {
                if let Some(search) = self.search_mut() {
                    search.clear();
                }
            }
            Message::FocusSearch => {
                let input = self.ids().input();
                if let Some(o) = self.pane_mut() {
                    o.sidebar = true;
                    // What was searched before is selected: typing replaces it.
                    return iced::widget::operation::focus(input.clone()).chain(iced::widget::operation::select_all(input));
                }
                if self.shelf.selected.is_none() && self.shelf.dialog.is_none() {
                    let id = crate::shelf::search_id();
                    return iced::widget::operation::focus(id.clone()).chain(iced::widget::operation::select_all(id));
                }
            }
            Message::Reveal(sheet, r) => self.reveal(&sheet, r),
            Message::Headings(nb, read) => {
                self.headings_reading = false;
                if matches!(&self.screen, Screen::Notebook(s) if s.shows(&nb)) {
                    self.headings.extend(read);
                }
            }
            Message::ToggleNotes => {
                if self.pane().and_then(|o| o.text_page.as_ref()).is_some_and(|t| t.docked) {
                    self.close_text_page();
                } else {
                    return self.open_notes(true);
                }
            }
            Message::EditPlot(sheet, id) => return self.edit_plot(&sheet, &id),
            Message::InsertPlot => return self.insert_plot(),
            Message::Shelf(m) => return self.on_shelf(m),
            Message::Outline(m) => return self.on_outline(m),
            Message::Pane(key, m) => return self.in_pane(key, |app| app.handle(*m)).map(|task| split::routed(key, task)).unwrap_or_else(Task::none),
            Message::Activate(key) => self.activate(key),
            Message::Split(m) => return self.on_split(m),
            Message::Plot(m) => {
                use crate::plots::Reply;
                // The panel belongs to the pane it was opened in.
                if matches!(&self.screen, Screen::Notebook(s) if s.key() != self.plot_pane && s.index(self.plot_pane).is_some()) {
                    let key = self.plot_pane;
                    return self.handle(Message::Pane(key, Box::new(Message::Plot(m))));
                }
                let Some(e) = &mut self.plot_edit else { return Task::none() };
                match e.update(m) {
                    Reply::None => {}
                    Reply::Focus(id, at) => return crate::plots::focus(id, at),
                    Reply::Select(id, from, to) => return crate::plots::select(id, from, to),
                    Reply::Cancel => self.plot_edit = None,
                    Reply::Delete => {
                        if let Some(e) = self.plot_edit.take()
                            && self.edit_sheet(&e.sheet_id, |s| s.remove(std::slice::from_ref(&e.original.id)))
                        {
                            self.say(tr("Koordinatensystem gelöscht"));
                        }
                    }
                    Reply::Apply(p) => {
                        if let Some(e) = self.plot_edit.take()
                            && p != e.original
                        {
                            let mut gone = false;
                            self.edit_sheet(&e.sheet_id, |s| {
                                gone = !s.plots().iter().any(|q| q.id == p.id);
                                if gone {
                                    return Err(n0te_core::Error::NotFound(p.id.clone()));
                                }
                                s.set_plot(&p)
                            });
                            if gone {
                                self.say(tr("Das Koordinatensystem wurde inzwischen auf einem anderen Gerät gelöscht"));
                            }
                        }
                    }
                }
            }
        }
        Task::none()
    }

    /// The paper a sheet shows under its ink (boards: dots; PDF pages: none of ours).
    fn paper_of(&self, sheet_id: &str) -> (Background, f32) {
        let Screen::Notebook(o) = &self.screen else { return (Background::Blank, 0.0) };
        match o.pages.pages.iter().map(|p| &p.placed).find(|p| p.sheet_id == sheet_id) {
            Some(p) if p.is_board() => (Background::Dots, p.spec.spacing),
            Some(p) if p.spec.pdf.is_some() => (Background::Blank, 0.0),
            Some(p) => (p.spec.background, p.spec.spacing),
            None => (Background::Blank, 0.0),
        }
    }

    /// Opens the panel for a plot (as stored now), the cursor in its input line.
    fn edit_plot(&mut self, sheet_id: &str, plot_id: &str) -> Task<Message> {
        if self.read_only() {
            self.say(tr("Nur lesen: Koordinatensysteme ändert, wer dich eingeladen hat"));
            return Task::none();
        }
        self.finish_text();
        let Some(nb) = self.notebook_id() else { return Task::none() };
        let Some(plot) = self.lib.load_sheet(&nb, sheet_id).ok().and_then(|s| s.plots().into_iter().find(|p| p.id == plot_id)) else { return Task::none() };
        let paper = self.paper_of(sheet_id);
        self.plot_edit = Some(crate::plots::Editor::new(sheet_id, plot, paper));
        self.plot_pane = self.pane().map(|o| o.key).unwrap_or_default();
        crate::plots::focus(crate::plots::input_id(), None)
    }

    /// A new coordinate system in the middle of the view, on the page there (else the current
    /// one), stored at once; its panel opens with the cursor in the input line.
    fn insert_plot(&mut self) -> Task<Message> {
        if self.read_only() {
            self.say(tr("Nur lesen: einfügen kann, wer dich eingeladen hat"));
            return Task::none();
        }
        self.finish_text();
        let Screen::Notebook(o) = &self.screen else { return Task::none() };
        let c = o.camera;
        let (cx, cy) = (c.x + c.viewport.width / c.zoom / 2.0, c.y + c.viewport.height / c.zoom / 2.0);
        let Some(page) = o.pages.sheet_at(cx, cy).or_else(|| o.pages.index_of_number(o.pages.current_number(&c)).map(|i| &o.pages.pages[i].placed)) else { return Task::none() };
        if page.spec.background == Background::Text && !page.is_board() {
            self.say(tr("Auf einer Textseite geht das nicht: Koordinatensysteme gehören auf Papierseiten"));
            return Task::none();
        }
        let sheet_id = page.sheet_id.clone();
        let (pw, ph) = (page.rect.width(), page.rect.height());
        let (cx, cy) = (cx - page.rect.x0, cy - page.rect.y0);
        let paper = self.paper_of(&sheet_id);
        let mut plot = crate::plots::fresh(cx, cy, pw, ph, paper.0, paper.1);
        let mut id = String::new();
        if !self.edit_sheet(&sheet_id, |s| {
            id = s.add_plot(&plot)?;
            Ok(())
        }) {
            return Task::none();
        }
        plot.id = id;
        self.plot_edit = Some(crate::plots::Editor::new(&sheet_id, plot, paper));
        self.plot_pane = self.pane().map(|o| o.key).unwrap_or_default();
        crate::plots::focus(crate::plots::input_id(), None)
    }

    /// Changes a sheet of the open notebook and stores it (it syncs from there); false if it
    /// didn't work (said in the status bar).
    fn edit_sheet(&mut self, sheet_id: &str, f: impl FnOnce(&Sheet) -> n0te_core::Result<()>) -> bool {
        if self.read_only() {
            self.say(tr("Nur lesen: ändern kann, wer dich eingeladen hat"));
            return false;
        }
        let Some(nb) = self.notebook_id() else { return false };
        let sheet = match self.lib.load_sheet(&nb, sheet_id) {
            Ok(s) => s,
            Err(e) => {
                self.say(trf("Seite nicht lesbar: {}", &[&e]));
                return false;
            }
        };
        if let Err(e) = f(&sheet) {
            self.say(trf("Nicht geändert: {}", &[&e]));
            return false;
        }
        self.store_sheet(&nb, sheet_id, &sheet);
        true
    }

    /// What the last message left to do in the background: headings to read (one read at a
    /// time, the notebooks on screen in turn; what comes due meanwhile waits for the next), a
    /// search to run again in a pane.
    fn background(&mut self) -> Vec<Task<Message>> {
        let mut out = Vec::new();
        let Screen::Notebook(s) = &self.screen else { return out };
        if !self.headings_reading {
            for id in s.notebooks() {
                let Some(due) = self.headings_due.remove(&id).filter(|d| !d.is_empty()) else { continue };
                let Some(o) = s.panes.iter().find(|o| o.id == id) else { continue };
                let sheets: Vec<String> = o
                    .nb
                    .outline(true)
                    .into_iter()
                    .filter_map(|r| match r {
                        OutlineRow::Sheet { sheet_id, .. } => Some(sheet_id),
                        OutlineRow::Group { .. } => None,
                    })
                    .filter(|s| due.all || due.sheets.contains(s) || !self.headings.contains_key(s))
                    .collect();
                if !sheets.is_empty() {
                    self.headings_reading = true;
                    let root = self.lib.root().to_path_buf();
                    out.push(Task::perform(
                        async move {
                            let read = search::read_headings(&root, &id, &sheets);
                            (id, read)
                        },
                        |(nb, read)| Message::Headings(nb, read),
                    ));
                    break;
                }
            }
        }
        let stale: Vec<PaneKey> = s.panes.iter().filter(|o| o.search.stale && o.search.active()).map(|o| o.key).collect();
        for key in stale {
            let soon = self.in_pane(key, |app| {
                if let Some(search) = app.search_mut() {
                    search.stale = false;
                }
                app.search_soon()
            });
            if let Some(task) = soon {
                out.push(split::routed(key, task));
            }
        }
        out
    }

    /// Sheets of notebook `nb` changed (None: maybe all; empty: the notebook's pages): their
    /// headings are read again, new sheets' too, and a search in a pane showing it runs again.
    fn changed(&mut self, nb: &str, sheets: Option<&[String]>) {
        let due = self.headings_due.entry(nb.to_string()).or_default();
        match sheets {
            Some(s) => due.sheets.extend(s.iter().cloned()),
            None => due.all = true,
        }
        due.check = true;
        if let Screen::Notebook(s) = &mut self.screen {
            for o in s.panes.iter_mut().filter(|o| o.id == nb && o.search.active()) {
                o.search.stale = true;
            }
        }
    }

    /// The search of the pane messages are for.
    fn search_mut(&mut self) -> Option<&mut Search> {
        self.pane_mut().map(|o| &mut o.search)
    }

    /// The search runs a moment after the last key ([`search::DEBOUNCE`]).
    fn search_soon(&mut self) -> Task<Message> {
        let Some(search) = self.search_mut() else { return Task::none() };
        search.typed += 1;
        let n = search.typed;
        Task::perform(search::after(search::DEBOUNCE), move |_| Message::SearchDue(n))
    }

    /// Starts the search in the background (one still running stops).
    fn start_search(&mut self) -> Task<Message> {
        let Some(id) = self.notebook_id() else { return Task::none() };
        let open = self.open_sheets(&id);
        let (root, pdf) = (self.lib.root().to_path_buf(), self.pdf.clone());
        let Some(o) = self.pane_mut() else { return Task::none() };
        let generation = o.search.latest.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        let job = search::Job { root, notebook: o.id.clone(), query: o.search.query.clone(), targets: search::targets(&o.nb), open, pdf, generation, latest: o.search.latest.clone() };
        o.search.busy = true;
        o.search.stale = false;
        let query = job.query.clone();
        Task::perform(async move { search::run(&job) }, move |found| Message::Found(generation, query.clone(), found))
    }

    /// The sheets of notebook `nb` being edited here as they are on screen (typed, not saved
    /// yet, in any pane): snapshots by sheet id, for the search.
    fn open_sheets(&self, nb: &str) -> HashMap<String, Vec<u8>> {
        let mut out = HashMap::new();
        let copy = |sheet: &Sheet, edit: &dyn Fn(&Sheet)| -> Option<Vec<u8>> {
            let c = Sheet::load(0, &sheet.snapshot().ok()?).ok()?;
            edit(&c);
            c.snapshot().ok()
        };
        if let Some(e) = self.editing.as_ref().filter(|e| e.notebook == nb) {
            let text = e.content.text();
            let text = text.strip_suffix('\n').unwrap_or(&text).to_string();
            if let Some(b) = copy(&e.sheet, &|c| {
                let _ = c.set_text(&e.box_id, &text);
                let _ = c.place_text_box(&e.box_id, e.x, e.y, e.w, e.size);
            }) {
                out.insert(e.sheet_id.clone(), b);
            }
        }
        let panes = match &self.screen {
            Screen::Notebook(s) => s.panes.iter().filter(|o| o.id == nb).collect(),
            Screen::Library => Vec::new(),
        };
        for t in panes.into_iter().filter_map(|o| o.text_page.as_ref()).filter(|t| t.dirty && !t.sheet_id.is_empty()) {
            let md = t.doc.to_markdown();
            if let Some(b) = copy(&t.sheet, &|c| {
                let _ = c.set_page_text(&md);
            }) {
                out.insert(t.sheet_id.clone(), b);
            }
        }
        out
    }

    /// Shows a spot of a sheet (sheet points) in the middle of the view, at the zoom it has,
    /// and lights it up; folded groups around it open first.
    fn reveal(&mut self, sheet: &str, r: [f32; 4]) {
        let Some(o) = self.pane() else { return };
        if o.pages.page_of_sheet(sheet).is_none() {
            let folded = folded_around(&o.nb, sheet);
            if folded.is_empty() {
                return;
            }
            for g in &folded {
                let _ = o.nb.set_collapsed(g, false);
            }
            let _ = self.lib.save_notebook(&o.nb);
            let id = o.id.clone();
            self.relayout_book(&id);
        }
        let Some(o) = self.pane_mut() else { return };
        let Some(i) = o.pages.page_of_sheet(sheet) else { return };
        let page = o.pages.pages[i].placed.rect;
        let c = o.camera;
        let (cx, cy) = (page.x0 + (r[0] + r[2]) / 2.0, page.y0 + (r[1] + r[3]) / 2.0);
        o.camera = o.pages.clamp(Camera { x: cx - c.viewport.width / c.zoom / 2.0, y: cy - c.viewport.height / c.zoom / 2.0, ..c });
        o.pages.flash = Some(pages::Flash { sheet_id: sheet.to_string(), rect: n0te_core::ink::Rect::new(r[0], r[1], r[2], r[3]), at: Instant::now() });
    }

    fn connect_sync(&mut self) {
        if let Some(s) = live::Session::connect() {
            self.sync = Some(Arc::new(s));
            self.devices = live::devices();
                self.refresh_guests();
            self.devices_at = Some(Instant::now());
        }
    }

    /// What the sync service reports.
    fn on_live(&mut self, e: LiveEvent) -> Task<Message> {
        match e {
            LiveEvent::Hello { .. } => {
                // Connected to the sync service: who's around, and whether guests see this view.
                self.refresh_guests();
                if !self.settings.share_view
                    && let Some(sync) = &self.sync
                {
                    sync.send(&["share-view".to_string(), "off".to_string()]);
                }
            }
            LiveEvent::Changed { notebook, index, sheets } => match &self.screen {
                Screen::Notebook(s) if s.shows(&notebook) => self.refresh_open(&notebook, index, &sheets),
                Screen::Notebook(_) => {}
                Screen::Library => self.refresh(),
            },
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
                    self.say(trf("Gekoppelt mit {}", &[&name]));
                }
                self.devices = live::devices();
                self.refresh_guests();
                self.devices_at = Some(Instant::now());
            }
            LiveEvent::Presence { device, notebook, sheet, viewport, laser, .. } => {
                let Some(owner) = self.pane_showing(&notebook) else { return Task::none() };
                if let Some(s) = &sheet {
                    self.last_views.insert(device.clone(), (s.clone(), viewport));
                }
                // Following the owner (as a guest): the pane that speaks for the notebook goes there.
                if self.follow_host.as_deref() == Some(device.as_str())
                    && let Some(s) = &sheet
                {
                    let s = s.clone();
                    self.in_pane(owner, |app| app.look_at(&s, viewport));
                }
                // Named: own device, guest or the owner; guests' frames only when wanted.
                let guest = self.guests.iter().find(|g| g.device == device);
                let host = self.joined.iter().any(|j| j.owner == device);
                if (guest.is_some() || host) && !self.settings.guest_frames {
                    return Task::none();
                }
                let name = self
                    .devices
                    .iter()
                    .find(|d| d.id == device)
                    .map(|d| d.name.clone())
                    .or_else(|| guest.map(|g| g.name.clone()))
                    .or_else(|| self.joined.iter().find(|j| j.owner == device).map(|j| j.owner_name.clone()))
                    .unwrap_or_else(|| tr("Gast").into());
                let following = self.live_session.as_ref().is_some_and(|l| l.follow_them && l.device == device);
                let Screen::Notebook(s) = &mut self.screen else { return Task::none() };
                let main = s.main;
                // Every pane showing the notebook shows the frame. Looking nowhere (e.g. the owner
                // stopped sharing its view): its frame goes.
                for o in s.panes.iter_mut().filter(|o| o.id == notebook) {
                    let mut marks = o.pages.remote.borrow_mut();
                    marks.retain(|m| m.name != name);
                    if let Some(sheet) = &sheet {
                        marks.push(crate::pages::RemoteMark { sheet_id: sheet.clone(), rect: n0te_core::ink::Rect::new(viewport[0], viewport[1], viewport[2], viewport[3]), laser: laser.map(|l| (l[0], l[1])), name: name.clone() });
                    }
                }
                // Following the tablet: the main pane (★) shows what it shows.
                if let (true, Some(sheet), Some(o)) = (following, &sheet, s.pane_mut(main).filter(|o| o.id == notebook))
                    && let Some(page) = o.pages.pages.iter().find(|p| p.placed.sheet_id == *sheet).map(|p| p.placed.rect)
                    && viewport[2] - viewport[0] > 1.0
                {
                    let c = o.camera;
                    let zoom = (c.viewport.width / (viewport[2] - viewport[0])).clamp(pages::MIN_ZOOM, pages::MAX_ZOOM);
                    let (cx, cy) = (page.x0 + (viewport[0] + viewport[2]) / 2.0, page.y0 + (viewport[1] + viewport[3]) / 2.0);
                    o.camera = o.pages.clamp(Camera { x: cx - c.viewport.width / zoom / 2.0, y: cy - c.viewport.height / zoom / 2.0, zoom, ..c });
                }
            }
            LiveEvent::Stroke { device, notebook, sheet, stroke, brush, samples, done } => {
                if let Screen::Notebook(s) = &self.screen {
                    for o in s.panes.iter().filter(|o| o.id == notebook) {
                        o.pages.live_samples(&device, stroke, &sheet, brush, &samples, done);
                    }
                }
            }
            LiveEvent::Erase { notebook, sheet, ids, dabs } => {
                if let Screen::Notebook(s) = &self.screen {
                    for o in s.panes.iter().filter(|o| o.id == notebook) {
                        o.pages.live_erase(&sheet, &ids, &dabs);
                    }
                }
            }
            LiveEvent::Open { device, notebook } => {
                let name = self.devices.iter().find(|d| d.id == device).map(|d| d.name.clone()).unwrap_or_else(|| "Tablet".into());
                if self.entries.iter().any(|e| e.id == notebook) || self.lib.load_notebook(&notebook).is_ok() {
                    match &self.screen {
                        // Two side by side: it opens in the main pane (★).
                        Screen::Notebook(s) if s.panes.len() > 1 => {
                            let main = s.main;
                            if s.pane(main).is_some_and(|o| o.id != notebook)
                                && let Err(e) = self.replace_pane(main, &notebook, None)
                            {
                                self.say(trf("Notizbuch nicht lesbar: {}", &[&e]));
                            }
                        }
                        Screen::Notebook(s) if s.id == notebook => {}
                        _ => {
                            let _ = self.open(&notebook);
                        }
                    }
                    // Keep who follows whom when the same device asks again (e.g. "Laptop folgt mir").
                    let (follow_them, they_follow) = self.live_session.as_ref().filter(|l| l.device == device).map(|l| (l.follow_them, l.they_follow)).unwrap_or((false, false));
                    self.live_session = Some(LiveSession { device, name: name.clone(), follow_them, they_follow });
                    // Where this view is, right away (the tablet may want to follow it).
                    self.send_presence_all();
                    self.say(trf("{} hat dieses Notizbuch live geöffnet", &[&name]));
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
            LiveEvent::Guest { name, on, device, .. } => {
                self.say(if on { trf("{} ist dazugekommen", &[&name]) } else { trf("{} ist gegangen", &[&self.guests.iter().find(|g| g.device == device).map(|g| g.name.clone()).unwrap_or_else(|| tr("Ein Gast").into())]) });
                if !on && let Screen::Notebook(s) = &self.screen {
                    let gone = self.guests.iter().find(|g| g.device == device).map(|g| g.name.clone());
                    if let Some(gone) = gone {
                        for o in &s.panes {
                            o.pages.remote.borrow_mut().retain(|m| m.name != gone);
                        }
                    }
                }
                self.refresh_guests();
                // A new guest sees where this laptop looks right away (to jump there / follow).
                if on {
                    self.send_presence_all();
                }
                if self.share.is_some() {
                    return self.handle(Message::ShowShare(true));
                }
            }
            LiveEvent::Waiting { name, on, .. } => {
                self.refresh_guests();
                if on {
                    self.say(trf("{} möchte herein (Teilen → Hereinlassen)", &[&name]));
                }
                if self.share.is_some() {
                    return self.handle(Message::ShowShare(true));
                }
            }
            LiveEvent::JoinWaiting { owner } => self.say(trf("Warte, bis {} dich hereinlässt …", &[&owner])),
            LiveEvent::Summoned { device, notebook, sheet, viewport } => {
                // The pane that speaks for the notebook goes (the main one when it shows it twice).
                let Some(key) = self.pane_showing(&notebook) else { return Task::none() };
                if self.in_pane(key, |app| app.look_at(&sheet, viewport)) == Some(true) {
                    let name = self.joined.iter().find(|j| j.owner == device).map(|j| j.owner_name.clone()).or_else(|| self.devices.iter().find(|d| d.id == device).map(|d| d.name.clone())).unwrap_or_else(|| tr("Der Besitzer").into());
                    self.say(trf("{} hat dich zu sich geholt", &[&name]));
                }
            }
            LiveEvent::Role { write, .. } => {
                self.refresh_guests();
                self.say(if write { tr("Du darfst jetzt mitschreiben") } else { tr("Ab jetzt nur noch lesen") });
            }
            LiveEvent::Revoked { .. } => self.say(tr("Die Einladung zu diesem Notizbuch wurde widerrufen")),
            LiveEvent::Error { message } => self.say(format!("Sync: {message}")),
            LiveEvent::Disconnected => self.sync = None,
        }
        Task::none()
    }

    /// Another device (or process) changed notebook `nb`: its document is read again once,
    /// every pane showing it lays out and reads its sheets again.
    fn refresh_open(&mut self, nb: &str, index: bool, sheets: &[String]) {
        let lib = self.lib.clone();
        let layout = self.settings.layout();
        let Screen::Notebook(s) = &mut self.screen else { return };
        let Some(first) = s.panes.iter().find(|o| o.id == nb) else { return };
        if index || sheets.is_empty() {
            let _ = lib.refresh_notebook(&first.nb);
        }
        for o in s.panes.iter_mut().filter(|o| o.id == nb) {
            if index || sheets.is_empty() {
                o.rows = o.nb.outline(false);
                o.marked = o.nb.marked();
                o.pages.relayout(nl::layout(&o.rows, &o.nb.outline(true), layout));
                o.camera = o.pages.clamp(o.camera);
            }
            let all: Vec<String> = if sheets.is_empty() { o.pages.pages.iter().map(|p| p.placed.sheet_id.clone()).collect() } else { sheets.to_vec() };
            for s in &all {
                o.pages.reload_sheet(s);
            }
            // An open text page that hasn't been typed in since: show the new text.
            if let Some(t) = &mut o.text_page
                && !t.dirty
                && all.contains(&t.sheet_id)
                && lib.refresh_sheet(nb, &t.sheet_id, &t.sheet).unwrap_or(false)
            {
                t.saved = t.sheet.page_text();
                t.doc = crate::blocks::Doc::from_markdown(&t.saved);
            }
        }
        if let Some(e) = self.editing.as_ref().filter(|e| e.notebook == nb && (sheets.is_empty() || sheets.contains(&e.sheet_id))) {
            let _ = lib.refresh_sheet(nb, &e.sheet_id, &e.sheet);
        }
        // Headings and a search in the field follow (sheets in folded groups too).
        self.changed(nb, if sheets.is_empty() { None } else { Some(sheets) });
    }

    /// Who is connected around our notebooks (guests here, owners of joined ones).
    fn refresh_guests(&mut self) {
        if self.sync.is_some() {
            self.guests = live::guests();
            self.joined = live::joined();
            self.waiting = live::waiting();
        }
    }

    /// Where this laptop looks in notebook `nb`: the view of the pane that speaks for it.
    fn view_in(&self, nb: &str) -> Option<(String, [f32; 4])> {
        let Screen::Notebook(s) = &self.screen else { return None };
        view_of(s.pane(s.owner(nb)?)?)
    }

    /// Shows `r` (sheet points) of `sheet` in the pane messages are for (jump to / follow /
    /// summoned).
    fn look_at(&mut self, sheet: &str, r: [f32; 4]) -> bool {
        let Some(o) = self.pane_mut() else { return false };
        let Some(page) = o.pages.pages.iter().find(|p| p.placed.sheet_id == sheet).map(|p| p.placed.rect) else { return false };
        if r[2] - r[0] <= 1.0 {
            return false;
        }
        let c = o.camera;
        let zoom = (c.viewport.width / (r[2] - r[0])).clamp(pages::MIN_ZOOM, pages::MAX_ZOOM);
        let (cx, cy) = (page.x0 + (r[0] + r[2]) / 2.0, page.y0 + (r[1] + r[3]) / 2.0);
        o.camera = o.pages.clamp(Camera { x: cx - c.viewport.width / zoom / 2.0, y: cy - c.viewport.height / zoom / 2.0, zoom, ..c });
        true
    }

    /// Someone sees where this laptop looks in notebook `nb`: the own tablet live, guests in
    /// it, or (as a guest) its owner.
    fn watched(&self, nb: &str) -> bool {
        self.live_session.is_some() || self.guests.iter().any(|g| g.notebook == nb) || self.joined.iter().any(|j| j.notebook == nb && j.online)
    }

    /// What tells others where this laptop looks in notebook `nb` ("presence", notebook, sheet,
    /// the view in its points, zoom): the pane that speaks for it — one frame per notebook,
    /// the main pane's when it is open twice.
    pub fn presence_args(&self, nb: &str) -> Option<Vec<String>> {
        let Screen::Notebook(s) = &self.screen else { return None };
        let o = s.pane(s.owner(nb)?)?;
        let (sheet, r) = view_of(o)?;
        Some(["presence".to_string(), nb.to_string(), sheet].into_iter().chain(r.iter().map(|x| format!("{x:.1}"))).chain([format!("{:.3}", o.camera.zoom)]).collect())
    }

    /// Where this laptop looks in notebook `nb`, to who sees it, at most 10× a second.
    fn send_presence(&mut self, nb: &str) {
        if self.sync.is_none() || !self.watched(nb) || self.presence_at.get(nb).is_some_and(|t| t.elapsed().as_millis() < 100) {
            return;
        }
        let (Some(sync), Some(args)) = (&self.sync, self.presence_args(nb)) else { return };
        sync.send(&args);
        self.presence_at.insert(nb.to_string(), Instant::now());
    }

    /// Where this laptop looks, now, in every notebook on screen.
    pub(crate) fn send_presence_all(&mut self) {
        self.presence_at.clear();
        let shown = match &self.screen {
            Screen::Notebook(s) => s.notebooks(),
            Screen::Library => Vec::new(),
        };
        for nb in shown {
            self.send_presence(&nb);
        }
    }

    /// Live with the tablet goes with ★: it opens the main pane's notebook (after ★ moved, or
    /// another notebook came into the main pane).
    pub(crate) fn live_follows_main(&mut self) {
        let (Some(l), Some(sync), Screen::Notebook(s)) = (&self.live_session, &self.sync, &self.screen) else { return };
        if let Some(o) = s.pane(s.main) {
            sync.send(&["open".to_string(), l.device.clone(), o.id.clone()]);
        }
    }

    fn palette_items(&self) -> Vec<palette::Item> {
        palette::filter(palette::items(self), self.palette.as_ref().map(|(q, _)| q.as_str()).unwrap_or(""))
    }

    /// Runs a palette command.
    fn run(&mut self, cmd: Command) -> Task<Message> {
        match cmd {
            Command::NewNotebook => self.handle(Message::StartCreate),
            Command::ImportPdf | Command::InsertPdf => self.handle(Message::ShowPdfImport(true)),
            Command::ImportBundle => self.handle(Message::ShowImports(true)),
            Command::OpenNotebook(id) => self.handle(Message::Open(id)),
            Command::Library => self.handle(Message::Back),
            Command::NewTextPage => self.handle(Message::NewTextPage),
            Command::Notes => self.handle(Message::ToggleNotes),
            Command::InsertPlot => self.handle(Message::InsertPlot),
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
                self.handle(Message::Key(ev))
            }
            Command::Export => self.handle(Message::ShowExport(true)),
            Command::SendToTablet => self.handle(Message::SendToTablet),
            Command::ToggleSidebar => self.handle(Message::ToggleSidebar),
            Command::Zoom(z) => self.handle(Message::Zoom(z)),
            Command::GoTo(sheet) => self.handle(Message::Jump(sheet)),
            Command::Theme(id) => self.handle(Message::SetTheme(id)),
            Command::Settings => self.handle(Message::ShowSettings(true)),
            Command::Help => self.handle(Message::ShowHelp(true)),
            Command::History => self.handle(Message::ShowHistory(true)),
            Command::Share => self.handle(Message::ShowShare(true)),
            Command::Live => self.handle(Message::StartLive),
            Command::RotatePage => self.handle(Message::RotatePage),
            Command::PageSetup => self.handle(Message::ShowPage(true)),
            Command::Search => self.handle(Message::FocusSearch),
            Command::Again => self.handle(Message::Split(split::Msg::Again)),
            Command::Swap => self.handle(Message::Split(split::Msg::Swap)),
            Command::CloseOther => match &self.screen {
                Screen::Notebook(s) => match s.other(s.active) {
                    Some(k) => self.handle(Message::Split(split::Msg::Close(k))),
                    None => Task::none(),
                },
                Screen::Library => Task::none(),
            },
        }
    }

    /// Draws the notebooks on screen anew (new theme: paper caches), keeping the cameras.
    fn reopen(&mut self) {
        let fresh: Vec<(PaneKey, Pages)> = match &self.screen {
            Screen::Notebook(s) => s.panes.iter().map(|o| (o.key, self.new_pages(&o.id, nl::layout(&o.rows, &o.nb.outline(true), self.settings.layout())))).collect(),
            Screen::Library => return,
        };
        let Screen::Notebook(s) = &mut self.screen else { return };
        for (key, pages) in fresh {
            if let Some(o) = s.pane_mut(key) {
                o.pages = pages;
            }
        }
    }

    /// The current main page's notes sheet (a text side sheet on the right), if any.
    fn notes_sheet(&self) -> Option<(String, String)> {
        let o = self.pane()?;
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
            let Some(o) = self.pane() else { return Task::none() };
            match o.nb.add_side_sheet(&main, n0te_core::notebook::Side::Right, 0, &SheetSpec::a4(Background::Text)) {
                Ok((_, s)) => {
                    let _ = self.lib.save_notebook(&o.nb);
                    sheet = s;
                    self.relayout();
                }
                Err(e) => {
                    self.say(trf("Notizen: {}", &[&e]));
                    return Task::none();
                }
            }
        }
        let task = self.open_text_page(&sheet, true);
        if let Some(t) = self.text_page_mut() {
            t.main_node = Some(main);
        }
        task
    }

    /// The text page of the pane messages are for.
    fn text_page_mut(&mut self) -> Option<&mut TextPage> {
        self.pane_mut().and_then(|o| o.text_page.as_mut())
    }

    /// The docked notes follow the page in view.
    fn follow_notes(&mut self) {
        let Some(t) = self.pane().and_then(|o| o.text_page.as_ref()) else { return };
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
            if let Some(t) = self.text_page_mut() {
                t.main_node = Some(main);
                t.sheet_id.clear();
                t.doc = crate::blocks::Doc::from_markdown("");
                t.saved.clear();
            }
        } else {
            let _ = self.open_text_page(&sheet, true);
            if let Some(t) = self.text_page_mut() {
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
        if let Some(o) = self.pane_mut() {
            o.text_page = Some(tp);
        }
        task
    }

    fn save_text_page(&mut self) {
        let Some(o) = self.pane() else { return };
        let Some(t) = o.text_page.as_ref() else { return };
        let md = t.doc.to_markdown();
        let nb = o.id.clone();
        if t.sheet_id.is_empty() {
            // Notes for a page that had none: the first words create the notes sheet.
            if md.trim().is_empty() {
                if let Some(t) = self.text_page_mut() {
                    t.dirty = false;
                }
                return;
            }
            let Some(main) = t.main_node.clone() else { return };
            let Ok((_, s)) = o.nb.add_side_sheet(&main, n0te_core::notebook::Side::Right, 0, &SheetSpec::a4(Background::Text)) else { return };
            let _ = self.lib.save_notebook(&o.nb);
            let Ok(sheet) = self.lib.load_sheet(&nb, &s) else { return };
            if let Some(t) = self.text_page_mut() {
                t.sheet_id = s;
                t.sheet = sheet;
            }
            self.relayout();
        }
        let Some(t) = self.text_page_mut() else { return };
        let _ = t.sheet.set_page_text(&md);
        t.saved = md;
        t.dirty = false;
        let (id, snap) = (t.sheet_id.clone(), t.sheet.snapshot().unwrap_or_default());
        if let Ok(sheet) = Sheet::load(0, &snap) {
            self.store_sheet(&nb, &id, &sheet);
        }
    }

    /// Closes the text page of the pane messages are for (saved first).
    pub(crate) fn close_text_page(&mut self) {
        if self.pane().and_then(|o| o.text_page.as_ref()).is_some_and(|t| t.dirty) {
            self.save_text_page();
        }
        if let Some(o) = self.pane_mut() {
            o.text_page = None;
        }
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
        let title = if t.docked { trf("NOTIZEN · SEITE {}", &[&label]) } else { trf("TEXTSEITE · {}", &[&label]) };
        let header = row![
            text(title).font(theme::MONO).size(10.5).color(theme::PAPER_MUTED),
            Space::new().width(Length::Fill),
            text(if t.dirty { "…" } else { tr("GESPEICHERT") }).font(theme::MONO).size(10.5).color(theme::PAPER_MUTED),
            Space::new().width(10),
            button(theme::btn_small(tr("Schließen"))).padding([2, 8]).on_press(Message::CloseTextPage).style(theme::text_tool),
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

    /// The notebook of the pane messages are for.
    pub(crate) fn notebook_id(&self) -> Option<String> {
        self.pane().map(|o| o.id.clone())
    }

    /// The main page in the middle of the view: its node and spec.
    pub fn page_in_view(&self) -> Option<(String, SheetSpec)> {
        let o = self.pane()?;
        let i = o.pages.index_of_number(o.pages.current_number(&o.camera))?;
        let p = &o.pages.pages[i].placed;
        Some((p.node.clone(), p.spec.clone()))
    }

    /// The open notebook is someone else's and we may only read it.
    pub fn read_only(&self) -> bool {
        self.notebook_id().is_some_and(|id| self.read_only_nb(&id))
    }

    /// Notebook `nb` is someone else's and we may only read it.
    pub fn read_only_nb(&self, nb: &str) -> bool {
        self.joined.iter().any(|j| j.notebook == nb && !j.write)
    }

    /// Changes the open notebook, saves it and lays its pages out again (in every pane showing
    /// it: they share the document).
    pub(crate) fn edit_notebook(&mut self, f: impl FnOnce(&Notebook) -> n0te_core::Result<()>) {
        if self.read_only() {
            self.say(tr("Nur lesen: Seiten ändert, wer dich eingeladen hat"));
            return;
        }
        let Some(o) = self.pane() else { return };
        let done = f(&o.nb);
        let _ = self.lib.save_notebook(&o.nb);
        let id = o.id.clone();
        self.relayout_book(&id);
        self.changed(&id, Some(&[]));
        if let Err(e) = done {
            self.say(trf("Nicht geändert: {}", &[&e]));
        }
    }

    /// Lays out every pane showing notebook `id` again (its document changed), keeping their
    /// cameras.
    fn relayout_book(&mut self, id: &str) {
        let layout = self.settings.layout();
        let Screen::Notebook(s) = &mut self.screen else { return };
        for o in s.panes.iter_mut().filter(|o| o.id == id) {
            o.rows = o.nb.outline(false);
            o.marked = o.nb.marked();
            o.pages.relayout(nl::layout(&o.rows, &o.nb.outline(true), layout));
            o.camera = o.pages.clamp(o.camera);
        }
    }

    /// Every notebook on screen laid out again (the gaps between pages changed).
    fn relayout_all(&mut self) {
        let shown = match &self.screen {
            Screen::Notebook(s) => s.notebooks(),
            Screen::Library => Vec::new(),
        };
        for id in shown {
            self.relayout_book(&id);
            self.changed(&id, Some(&[]));
        }
    }

    /// Gives `nodes` the size `w` × `h`; first asks when strokes, images or text would end up past
    /// the edge (they stay stored, but are cut off on screen and in the PDF).
    fn resize(&mut self, nodes: Vec<String>, w: f32, h: f32, done: Option<String>) {
        if self.read_only() {
            self.say(tr("Nur lesen: Seiten ändert, wer dich eingeladen hat"));
            return;
        }
        let Some(o) = self.pane() else { return };
        let mut sheet_of = std::collections::HashMap::new();
        for row in o.nb.outline(true) {
            if let OutlineRow::Sheet { node, sheet_id, left, right, .. } = row {
                sheet_of.insert(node, sheet_id);
                sheet_of.extend(left.into_iter().chain(right).map(|s| (s.node, s.sheet_id)));
            }
        }
        let mut total = n0te_core::sheet::Outside::default();
        let mut pages = 0;
        for n in &nodes {
            let Some(sheet) = sheet_of.get(n).and_then(|sid| self.lib.load_sheet(&o.id, sid).ok()) else { continue };
            let out = sheet.outside(w, h);
            if !out.is_empty() {
                pages += 1;
                total.add(out);
            }
        }
        if total.is_empty() {
            self.apply_size(&nodes, w, h, done);
        } else {
            self.size_warning = Some(SizeWarning { nodes, w, h, what: describe_outside(&total), count: total.strokes + total.images + total.texts, pages, done });
        }
    }

    fn apply_size(&mut self, nodes: &[String], w: f32, h: f32, done: Option<String>) {
        self.edit_notebook(|nb| {
            for n in nodes {
                nb.set_size(n, w, h)?;
            }
            Ok(())
        });
        if let Some(d) = done {
            self.say(d);
        }
    }

    /// The warning before a size change cuts content off.
    fn size_warning_view<'a>(&'a self, w: &'a SizeWarning) -> Element<'a, Message> {
        let one = w.count == 1;
        let where_ = if w.pages > 1 { trf(" (auf {} Seiten)", &[&w.pages]) } else { String::new() };
        let col = column![
            theme::heading(tr("Nicht alles passt auf die Seite"), 20.0),
            text(if one {
                trf("{} ragt dann über den Rand{} und wird abgeschnitten, auch im PDF.", &[&w.what, &where_])
            } else {
                trf("{} ragen dann über den Rand{} und werden abgeschnitten, auch im PDF.", &[&w.what, &where_])
            })
            .size(15),
            text(tr("Gelöscht wird nichts: wird die Seite wieder größer, ist alles wieder da.")).size(13).color(theme::p().muted),
            row![
                Space::new().width(Length::Fill),
                button(theme::btn(tr("Abbrechen"))).padding(theme::pad()).on_press(Message::ConfirmSize(false)).style(theme::ghost),
                Space::new().width(8),
                button(theme::btn(tr("Trotzdem ändern"))).padding(theme::pad()).on_press(Message::ConfirmSize(true)).style(theme::accent_button),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(12);
        container(container(col).padding(24).width(Length::Fixed(520.0)).style(theme::panel)).padding(Padding { top: 120.0, ..Padding::ZERO }).center_x(Length::Fill).into()
    }

    /// A touchpad gesture: a pinch zooms around the fingers (and pans as they move) in the pane
    /// under them, a swipe that ends in motion lets the page glide on (in the active pane: the
    /// swipe scrolled it, which made it active), fingers on the touchpad catch it.
    fn gesture(&mut self, g: Gesture) -> Task<Message> {
        let Screen::Notebook(s) = &self.screen else {
            self.pinch = None;
            self.glide = None;
            return Task::none();
        };
        match g {
            Gesture::PinchBegin { x, y } => {
                self.glide = None;
                // Only over the pages (not the outline, the bars or a panel).
                let under = s.panes.iter().find(|o| {
                    let c = o.camera;
                    let p = Point::new(x - c.origin.x, y - c.origin.y);
                    p.x >= 0.0 && p.y >= 0.0 && p.x <= c.viewport.width && p.y <= c.viewport.height
                });
                let panel = self.page_panel || self.share.is_some() || self.history.is_some() || self.export.is_some() || self.show_settings || self.plot_edit.is_some();
                self.pinch = under.filter(|_| !panel).map(|o| (o.key, o.camera.zoom, Point::new(x - o.camera.origin.x, y - o.camera.origin.y)));
                if let Some((key, ..)) = self.pinch {
                    self.activate(key);
                }
                Task::none()
            }
            Gesture::Pinch { scale, dx, dy } => {
                let Some((key, z0, p)) = self.pinch else { return Task::none() };
                let Some(o) = s.pane(key) else { return Task::none() };
                let c = o.camera;
                // The page point under the fingers stays under them while they move and spread.
                let anchor = Point::new(c.x + p.x / c.zoom, c.y + p.y / c.zoom);
                let p = Point::new(p.x + dx, p.y + dy);
                let zoom = (z0 * scale).clamp(pages::MIN_ZOOM, pages::MAX_ZOOM);
                let next = o.pages.clamp(Camera { x: anchor.x - p.x / zoom, y: anchor.y - p.y / zoom, zoom, ..c });
                self.pinch = Some((key, z0, p));
                self.handle(Message::Pane(key, Box::new(Message::Camera(next))))
            }
            Gesture::PinchEnd => {
                self.pinch = None;
                Task::none()
            }
            Gesture::Fling { vx, vy } => {
                if self.settings.momentum && self.pinch.is_none() {
                    self.glide = Some((s.active, vx, vy, Instant::now()));
                }
                Task::none()
            }
            Gesture::Touch => {
                self.glide = None;
                Task::none()
            }
        }
    }

    /// One frame of a gliding page: it slows down like on paper (time constant 0.35 s) and stops
    /// at the notebook's edge.
    fn glide_step(&mut self, now: Instant) -> Task<Message> {
        let Some((key, vx, vy, last)) = self.glide else { return Task::none() };
        let Some(o) = (match &self.screen {
            Screen::Notebook(s) => s.pane(key),
            Screen::Library => None,
        }) else {
            self.glide = None;
            return Task::none();
        };
        let dt = now.saturating_duration_since(last).as_secs_f32().min(0.05);
        let c = o.camera;
        let want = Camera { x: c.x + vx * dt / c.zoom, y: c.y + vy * dt / c.zoom, ..c };
        let next = o.pages.clamp(want);
        let decay = (-dt / 0.35).exp();
        // An axis that hit the edge stops; the other glides on.
        let vx = if (next.x - want.x).abs() > 0.01 { 0.0 } else { vx * decay };
        let vy = if (next.y - want.y).abs() > 0.01 { 0.0 } else { vy * decay };
        self.glide = (vx.hypot(vy) > 20.0).then_some((key, vx, vy, now));
        if next == c {
            return Task::none();
        }
        self.handle(Message::Pane(key, Box::new(Message::Camera(next))))
    }

    /// Writes a sheet of notebook `nb` and makes every pane showing it read it again; the same
    /// text page open (and not typed in) in another pane shows the new text.
    pub(crate) fn store_sheet(&mut self, nb: &str, sheet_id: &str, sheet: &Sheet) {
        if let Err(e) = self.lib.save_sheet(nb, sheet_id, sheet) {
            self.say(trf("Speichern fehlgeschlagen: {}", &[&e]));
        }
        let lib = self.lib.clone();
        if let Screen::Notebook(s) = &mut self.screen {
            let body = sheet.page_text();
            for o in s.panes.iter_mut().filter(|o| o.id == nb) {
                o.pages.reload_sheet(sheet_id);
                if let Some(t) = &mut o.text_page
                    && t.sheet_id == sheet_id
                    && !t.dirty
                    && t.saved != body
                    && lib.refresh_sheet(nb, sheet_id, &t.sheet).unwrap_or(false)
                {
                    t.saved = t.sheet.page_text();
                    t.doc = crate::blocks::Doc::from_markdown(&t.saved);
                }
            }
        }
        self.changed(nb, Some(&[sheet_id.to_string()]));
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
        let (nb, sheet_id, sheet) = (e.notebook.clone(), e.sheet_id.clone(), Sheet::load(0, &e.sheet.snapshot().unwrap_or_default()));
        if let Ok(sheet) = sheet {
            self.store_sheet(&nb, &sheet_id, &sheet);
        }
    }

    /// Ends editing: saves; an empty box is removed.
    pub(crate) fn finish_text(&mut self) {
        if self.editing.is_none() {
            return;
        }
        self.save_text(true);
        if let Some(e) = self.editing.take()
            && e.content.text().trim().is_empty()
        {
            let _ = e.sheet.remove(std::slice::from_ref(&e.box_id));
            self.store_sheet(&e.notebook, &e.sheet_id, &e.sheet);
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
        let pane = self.pane().map(|o| o.key).unwrap_or_default();
        self.editing = Some(Editing { pane, notebook: nb, sheet_id: sheet_id.to_string(), box_id: box_id.to_string(), sheet, content, x: tb.x, y: tb.y, w: tb.w, size: tb.size, dirty: false, last_change: Instant::now() });
        iced::widget::operation::focus(editor_id())
    }

    fn new_text(&mut self, sheet_id: &str, x: f32, y: f32) -> Task<Message> {
        self.finish_text();
        let Some(nb) = self.notebook_id() else { return Task::none() };
        let page_w = self.pane().and_then(|o| o.pages.pages.iter().find(|p| p.placed.sheet_id == sheet_id)).map(|p| p.placed.rect.width()).unwrap_or(595.0);
        let Ok(sheet) = self.lib.load_sheet(&nb, sheet_id) else { return Task::none() };
        let w = (page_w - x - 24.0).clamp(80.0, 360.0);
        let Ok(id) = sheet.add_text_box(x, y, w, 14.0, "") else { return Task::none() };
        self.store_sheet(&nb, sheet_id, &sheet);
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
        // The box being typed into shows as an editor in its pane only.
        let editing = self.editing.as_ref().filter(|e| e.pane == o.key);
        for (sheet, page, tb) in o.pages.visible_texts(&cam) {
            if editing.is_some_and(|e| e.box_id == tb.id) {
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
        if let Some(e) = editing
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
                .placeholder(tr("Text, Markdown: # Titel, - Punkt, **fett**, $x^2$"))
                .style(theme::editor_on_paper_framed)
                .highlight_with::<crate::mdstyle::MdInline>((), crate::mdstyle::format);
            out.push(pin(column![tools, container(editor).width(Length::Fixed(e.w * cam.zoom))].spacing(4)).x(x - 4.0).y(y - 32.0).into());
        }
        out
    }

    /// Lays the open notebook out again after a structural change (keeps the cameras).
    fn relayout(&mut self) {
        let Some(id) = self.notebook_id() else { return };
        self.relayout_book(&id);
        self.changed(&id, Some(&[]));
    }

    /// A PDF: in the library a new notebook named after the file, in a notebook new pages
    /// right after the current one.
    fn import_pdf(&mut self, path: &Path) {
        let Some(pdf) = self.pdf.clone() else { return self.say(tr("PDF geht nicht: libpdfium.so fehlt")) };
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return self.say(trf("Nicht lesbar: {}", &[&e])),
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
                        self.say(trf("{} Seiten aus „{}“ eingefügt", &[&n, &name]));
                    }
                    Err(e) => self.say(trf("Import fehlgeschlagen: {}", &[&e])),
                }
            }
            Screen::Library => {
                let made = self.lib.create_notebook(&name).map_err(|e| e.to_string()).and_then(|nb| pdf.import(&self.lib, &nb, bytes, None, 0).map(|_| nb.id()));
                match made {
                    Ok(id) => {
                        self.refresh();
                        let _ = self.open(&id);
                    }
                    Err(e) => self.say(trf("Import fehlgeschlagen: {}", &[&e])),
                }
            }
        }
    }

    /// Writes the PDF to ~/Downloads (a free name) and remembers it for "ÖFFNEN".
    fn export_pdf(&mut self, what: ExportWhat) {
        let Some(pdf) = self.pdf.clone() else { return self.say(tr("PDF geht nicht: libpdfium.so fehlt")) };
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
            return self.say(tr("Keine Seite gewählt"));
        }
        let title = o.nb.title();
        match pdf.export(&self.lib, &o.nb, selected.as_deref()) {
            Ok((bytes, pages)) => {
                let path = free_path(&self.export_dir, &title, "pdf");
                match n0te_core::store::write_atomic(&path, &bytes) {
                    Ok(()) => {
                        self.export = None;
                        self.say(trf("{} Seiten exportiert: {}", &[&pages, &path.display()]));
                        self.last_export = Some(path);
                    }
                    Err(e) => self.say(trf("Speichern fehlgeschlagen: {}", &[&e])),
                }
            }
            Err(e) => self.say(trf("Export fehlgeschlagen: {}", &[&e])),
        }
    }

    /// Global shortcuts (keys no focused widget consumed).
    /// Looks for an update on a worker thread (Windows; downloads it when there is one).
    fn check_update(&mut self) -> Task<Message> {
        if !crate::updater::supported() || self.update == crate::updater::Status::Checking {
            return Task::none();
        }
        self.update_at = Some(Instant::now());
        self.update = crate::updater::Status::Checking;
        let repo = self.settings.update_repo.clone();
        let (tx, rx) = iced::futures::channel::oneshot::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::updater::check(&repo));
        });
        Task::perform(rx, |r| Message::UpdateStatus(r.unwrap_or(crate::updater::Status::Idle)))
    }

    fn shortcut(&mut self, key: Key, m: keyboard::Modifiers) -> Task<Message> {
        // The palette takes the arrows and Esc while it is open.
        if self.palette.is_some() {
            return match key.as_ref() {
                Key::Named(Named::ArrowDown) => self.handle(Message::PaletteMove(1)),
                Key::Named(Named::ArrowUp) => self.handle(Message::PaletteMove(-1)),
                Key::Named(Named::Escape) => {
                    self.palette = None;
                    Task::none()
                }
                _ => Task::none(),
            };
        }
        if self.size_warning.is_some() {
            return match key.as_ref() {
                Key::Named(Named::Escape) => self.handle(Message::ConfirmSize(false)),
                Key::Named(Named::Enter) => self.handle(Message::ConfirmSize(true)),
                _ => Task::none(),
            };
        }
        // The plot panel is open: Esc leaves a row being edited or cancels, the rest waits.
        if self.plot_edit.is_some() {
            return match key.as_ref() {
                Key::Named(Named::Escape) => self.handle(Message::Plot(crate::plots::Msg::Escape)),
                _ => Task::none(),
            };
        }
        // A page menu or rename dialog of the outline, the ⧉ menu: Esc closes them, the rest waits.
        if let Screen::Notebook(s) = &mut self.screen
            && (s.menu.is_some() || s.panes.iter().any(|o| o.outline.menu.is_some() || o.outline.renaming.is_some()))
        {
            if let Key::Named(Named::Escape) = key.as_ref() {
                s.menu = None;
                for o in &mut s.panes {
                    o.outline.menu = None;
                    o.outline.asked = None;
                    o.outline.renaming = None;
                }
            }
            return Task::none();
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
            if self.pane().is_some_and(|o| o.text_page.is_some()) {
                return self.handle(Message::Blocks(crate::blocks::Msg::Wrap(mark)));
            }
        }
        // n0te Tablet, not typing: undo/redo, copy/cut/paste and delete work on the ink.
        if crate::mode() == crate::Mode::Tablet && self.editing.is_none() && self.pane().is_some_and(|o| o.text_page.is_none()) {
            use crate::tablet::Msg as T;
            let t = match (m.command(), m.shift(), key.as_ref()) {
                (true, false, Key::Character("z")) => Some(T::Undo),
                (true, true, Key::Character("z" | "Z")) | (true, _, Key::Character("y")) => Some(T::Redo),
                (true, _, Key::Character("c")) if self.tablet.selection.is_some() => Some(T::Copy),
                (true, _, Key::Character("x")) if self.tablet.selection.is_some() => Some(T::Cut),
                (true, _, Key::Character("v")) if !self.tablet.clip_is_empty() => Some(T::Paste),
                (false, _, Key::Named(Named::Delete | Named::Backspace)) if self.tablet.selection.is_some() => Some(T::DeleteSelection),
                (false, _, Key::Named(Named::Escape)) if self.tablet.selection.is_some() => Some(T::ClearSelection),
                _ => None,
            };
            if let Some(t) = t {
                return self.handle(Message::Tablet(t));
            }
        }
        match (m.command(), key.as_ref()) {
            (true, Key::Character("k")) => return self.handle(Message::OpenPalette),
            (_, Key::Named(Named::F1)) => return self.handle(Message::ShowHelp(true)),
            (true, Key::Character(",")) => return self.handle(Message::ShowSettings(true)),
            (true, Key::Character("o")) => return self.handle(Message::ShowPdfImport(true)),
            (true, Key::Character("f")) => return self.handle(Message::FocusSearch),
            _ => {}
        }
        if let Key::Named(Named::Escape) = key.as_ref() {
            if self.editing.is_some() {
                self.finish_text();
                return Task::none();
            }
            // The library: its menu, dialog, a drag, "Auswählen", one at a time.
            if matches!(self.screen, Screen::Library) && self.shelf.escape() {
                return Task::none();
            }
            if self.pane().is_some_and(|o| o.text_page.is_some()) {
                self.close_text_page();
                return Task::none();
            }
            if self.creating.is_some() || self.show_imports {
                self.creating = None;
                self.show_imports = false;
                return Task::none();
            }
            if matches!(self.screen, Screen::Library) && !self.shelf.query.is_empty() {
                self.shelf.query.clear();
                return Task::none();
            }
            // Search results in the sidebar: the outline again.
            if let Some(search) = self.search_mut().filter(|s| s.active()) {
                search.clear();
                return Task::none();
            }
            // Two notebooks: one shown alone comes back beside the other, then the one beside
            // the main pane closes.
            if let Screen::Notebook(s) = &self.screen
                && s.panes.len() > 1
            {
                let side = s.other(s.main).unwrap_or(s.main);
                return self.handle(Message::Split(if s.maxed.is_some() { split::Msg::Alone(None) } else { split::Msg::Close(side) }));
            }
            return self.handle(Message::Back);
        }
        let Screen::Notebook(o) = &mut self.screen else {
            return match (m.command(), key.as_ref()) {
                (true, Key::Character("n")) => self.handle(Message::StartCreate),
                (true, Key::Character("i")) => self.handle(Message::ShowImports(true)),
                _ => Task::none(),
            };
        };
        let c = o.camera;
        if m.command() {
            return match key.as_ref() {
                Key::Character("0") => self.handle(Message::Zoom(1.0)),
                Key::Character("+" | "=") => self.handle(Message::Zoom(c.zoom * 1.25)),
                Key::Character("-") => self.handle(Message::Zoom(c.zoom / 1.25)),
                Key::Character("b") => self.handle(Message::ToggleSidebar),
                Key::Character("n") => self.handle(Message::NewTextPage),
                Key::Character("m") => self.handle(Message::ToggleNotes),
                Key::Character("e") => self.handle(Message::ShowExport(true)),
                Key::Character("h") => self.handle(Message::ShowHistory(true)),
                Key::Character("r") => self.handle(Message::RotatePage),
                Key::Character("t") => {
                    // A text box in the middle of the view, on the page there.
                    let (cx, cy) = (c.x + c.viewport.width / c.zoom / 2.0, c.y + c.viewport.height / c.zoom / 3.0);
                    match o.pages.sheet_at(cx, cy).filter(|p| !p.is_board()).map(|p| (p.sheet_id.clone(), cx - p.rect.x0, cy - p.rect.y0)) {
                        Some((sheet, x, y)) => self.handle(Message::NewText(sheet, x, y)),
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
            Screen::Library => crate::shelf::layers(self, library::view(self)),
            Screen::Notebook(s) => split::layers(self, s, sidebar::layers(self, self.notebook_view(s))),
        };
        let overlay: Option<Element<'_, Message>> = if let Some(w) = &self.size_warning {
            Some(self.size_warning_view(w))
        } else if let Some((q, sel)) = &self.palette {
            Some(palette::view(q, &self.palette_items(), *sel))
        } else if self.show_help {
            Some(self.help_view())
        } else if self.show_settings {
            Some(self.settings_view())
        } else if let (Screen::Notebook(o), Some(picked)) = (&self.screen, &self.export) {
            Some(library::export_dialog(o, picked, self.export_mode, o.pages.current_number(&o.camera)))
        } else if let (Screen::Notebook(_), Some(e)) = (&self.screen, &self.plot_edit) {
            // Esc in a field cancels too (the field would only let go of the focus). Its
            // messages go to the pane it was opened in.
            let key = self.plot_pane;
            Some(Element::from(crate::widgets::on_escape(e.view(), crate::plots::Msg::Escape)).map(move |m| split::route(key, Message::Plot(m))))
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
                theme::heading(tr("Verlauf"), 20.0),
                Space::new().width(Length::Fill),
                button(theme::btn(tr("Schließen"))).padding(theme::pad()).on_press(Message::ShowHistory(false)).style(theme::ghost),
            ]
            .align_y(Alignment::Center),
            text(tr("Der Sync-Dienst sichert alle 5 Minuten. Ältere Stände werden ausgedünnt: alles der letzten Stunde, stündlich für einen Tag, täglich für 30 Tage, danach wöchentlich.")).size(12).color(theme::p().muted),
        ]
        .spacing(8);
        if list.is_empty() {
            col = col.push(text(tr("Noch keine Stände gesichert.")).size(13).color(theme::p().muted));
        }
        let mut rows = column![].spacing(2);
        for (at, label) in list.iter().rev() {
            rows = rows.push(
                row![
                    text(label.clone()).font(theme::meta_font()).size(12.5),
                    Space::new().width(Length::Fill),
                    button(theme::btn_small(tr("Als Kopie wiederherstellen"))).padding([3, 8]).on_press(Message::Restore(*at)).style(theme::ghost),
                ]
                .align_y(Alignment::Center),
            );
        }
        col = col.push(scrollable(rows).style(theme::scroll_dark).height(Length::Shrink).height(Length::Fixed(320.0)));
        container(col).padding(20).width(Length::Fill).max_width(760).style(theme::panel).into()
    }

    /// Share with guests: invite links (read / read-write), active invites with revoke.
    /// The share panel. Owner: who waits / is here (jump, bring to me, remove), invites as
    /// one line each (click opens rights, validity, approval, one device, PIN, QR, revoke),
    /// a new link, the two view switches. Guest: jump to / follow the owner, its frame.
    fn share_panel<'a>(&'a self, invites: &'a [live::Invite]) -> Element<'a, Message> {
        let nb = self.notebook_id().unwrap_or_default();
        let small = |t: &str, m: Message| button(theme::btn_small(t)).padding(theme::pad()).on_press(m).style(theme::ghost);
        let muted = |t: String| text(t).size(12).color(theme::p().muted);
        let close = button(theme::btn(tr("Schließen"))).padding(theme::pad()).on_press(Message::ShowShare(false)).style(theme::ghost);

        // As a guest: the owner's view.
        if let Some(j) = self.joined.iter().find(|j| j.notebook == nb) {
            let following = self.follow_host.is_some();
            let col = column![
                row![theme::heading(tr("Gast"), 20.0), Space::new().width(Length::Fill), close].align_y(Alignment::Center),
                text(trf("bei {} · {}", &[&j.owner_name, &if j.write { tr("du darfst mitschreiben") } else { tr("nur lesen") }])).size(14).color(theme::p().text),
                row![
                    button(theme::btn(&trf("Zu {} springen", &[&j.owner_name]))).padding(theme::pad()).on_press(Message::JumpTo(j.owner.clone())).style(theme::secondary),
                    button(theme::btn(&trf("{} folgen", &[&j.owner_name]))).padding(theme::pad()).on_press(Message::FollowHost(!following)).style(theme::tab(following)),
                ]
                .spacing(8),
                muted(tr("Folgen endet, sobald du selbst verschiebst.").into()),
                iced::widget::toggler(self.settings.guest_frames).label(trf("Rahmen von {} zeigen", &[&j.owner_name])).on_toggle(Message::GuestFrames).size(18).text_size(13).spacing(10).style(theme::switch),
            ]
            .spacing(12);
            return container(col).padding(20).width(Length::Fill).max_width(620).style(theme::panel).into();
        }

        let mut col = column![
            row![theme::heading(tr("Teilen"), 20.0), Space::new().width(12), muted(tr("Direkt von hier, ohne Server.").into()), Space::new().width(Length::Fill), close].align_y(Alignment::Center),
        ]
        .spacing(10);
        if self.sync.is_none() {
            col = col.push(text(tr("Der Sync-Dienst läuft nicht (systemctl --user enable --now n0te-sync).")).size(13).color(theme::p().accent));
        }
        // At the door, then who is here.
        for g in self.waiting.iter().filter(|g| g.notebook == nb) {
            col = col.push(
                row![
                    text(trf("{} möchte herein", &[&g.name])).size(14).color(theme::p().text),
                    Space::new().width(Length::Fill),
                    small(tr("Nein"), Message::Admit(g.device.clone(), false)),
                    button(theme::btn_small(tr("Herein"))).padding(theme::pad()).on_press(Message::Admit(g.device.clone(), true)).style(theme::accent_button),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            );
        }
        let here: Vec<&live::Guest> = self.guests.iter().filter(|g| g.notebook == nb).collect();
        if here.is_empty() {
            col = col.push(muted(tr("Gerade ist niemand da.").into()));
        }
        for g in &here {
            let sure = self.removing.as_deref() == Some(g.device.as_str());
            col = col.push(
                row![
                    container(Space::new().width(8).height(8)).style(|_| container::Style { background: Some(theme::p().accent.into()), border: iced::Border { radius: 4.0.into(), ..Default::default() }, ..Default::default() }),
                    text(g.name.clone()).size(14).color(theme::p().text),
                    muted(if g.write { tr("schreibt").into() } else { tr("liest").into() }),
                    Space::new().width(Length::Fill),
                    small(tr("Hinschauen"), Message::JumpTo(g.device.clone())),
                    small(tr("Zu mir holen"), Message::Summon(Some(g.device.clone()))),
                    if sure {
                        button(theme::btn_small(tr("Wirklich entfernen?"))).padding(theme::pad()).on_press(Message::RemoveGuest(g.device.clone())).style(theme::accent_button)
                    } else {
                        small(tr("Entfernen"), Message::RemoveGuest(g.device.clone()))
                    },
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        if here.len() > 1 {
            col = col.push(row![Space::new().width(Length::Fill), button(theme::btn_small(tr("Alle zu mir holen"))).padding(theme::pad()).on_press(Message::Summon(None)).style(theme::secondary)]);
        }
        col = col.push(container(Space::new().height(1)).width(Length::Fill).style(theme::hairline));
        // Invites: one line each; the open one shows its settings.
        let now = unix_now();
        for i in invites.iter().rev().filter(|i| !i.revoked) {
            let open = self.invite_open.as_deref() == Some(i.id.as_str());
            let until = match i.expires {
                0 => tr("ohne Ablauf").to_string(),
                e if e < now => tr("abgelaufen").to_string(),
                e => remaining(e - now),
            };
            let mut facts = vec![until];
            if i.online > 0 {
                facts.push(trf("{} da", &[&i.online]));
            }
            if i.approve {
                facts.push(tr("bestätigen").into());
            }
            if i.single {
                facts.push(tr("ein Gerät").into());
            }
            if let Some(p) = &i.pin {
                facts.push(format!("PIN {p}"));
            }
            let mut card = column![
                row![
                    column![text(format!("Link · {}", if i.rw { tr("lesen & schreiben") } else { tr("nur lesen") })).size(14).color(theme::p().text), muted(facts.join(" · "))].spacing(2),
                    Space::new().width(Length::Fill),
                    small(tr("Link kopieren"), Message::CopyText(i.link.clone())),
                    small(if open { tr("Weniger") } else { tr("Einstellungen") }, Message::ToggleInvite(i.id.clone())),
                ]
                .spacing(6)
                .align_y(Alignment::Center),
            ]
            .spacing(10);
            if open {
                card = card.push(
                    row![
                        button(theme::btn_small(tr("Nur lesen"))).padding(theme::pad()).on_press(Message::SetRole(i.id.clone(), false)).style(theme::tab(!i.rw)),
                        button(theme::btn_small(tr("Lesen & schreiben"))).padding(theme::pad()).on_press(Message::SetRole(i.id.clone(), true)).style(theme::tab(i.rw)),
                        Space::new().width(18),
                        button(theme::btn_small(tr("1 Tag"))).padding(theme::pad()).on_press(Message::SetExpiry(i.id.clone(), 1)).style(theme::tab(false)),
                        button(theme::btn_small(tr("7 Tage"))).padding(theme::pad()).on_press(Message::SetExpiry(i.id.clone(), 7)).style(theme::tab(false)),
                        button(theme::btn_small(tr("30 Tage"))).padding(theme::pad()).on_press(Message::SetExpiry(i.id.clone(), 30)).style(theme::tab(false)),
                        button(theme::btn_small(tr("Unbegrenzt"))).padding(theme::pad()).on_press(Message::SetExpiry(i.id.clone(), 0)).style(theme::tab(i.expires == 0)),
                    ]
                    .spacing(6)
                    .align_y(Alignment::Center),
                );
                let (id1, id2, id3) = (i.id.clone(), i.id.clone(), i.id.clone());
                let (approve, single) = (i.approve, i.single);
                card = card.push(
                    row![
                        iced::widget::toggler(i.approve).label(tr("Neue Gäste erst bestätigen")).on_toggle(move |on| Message::SetInviteOptions(id1.clone(), on, single)).size(16).text_size(12).spacing(8).style(theme::switch),
                        Space::new().width(16),
                        iced::widget::toggler(i.single).label(tr("Nur ein Gerät")).on_toggle(move |on| Message::SetInviteOptions(id2.clone(), approve, on)).size(16).text_size(12).spacing(8).style(theme::switch),
                        Space::new().width(16),
                        iced::widget::toggler(i.pin.is_some()).label("PIN").on_toggle(move |on| Message::SetPin(id3.clone(), on)).size(16).text_size(12).spacing(8).style(theme::switch),
                    ]
                    .align_y(Alignment::Center),
                );
                if let Some(pin) = &i.pin {
                    card = card.push(
                        row![text(pin.clone()).font(theme::MONO_BOLD).size(20).color(theme::p().text), muted(tr("getrennt vom Link sagen").into()), Space::new().width(Length::Fill), small(tr("Neue PIN"), Message::SetPin(i.id.clone(), true))]
                            .spacing(12)
                            .align_y(Alignment::Center),
                    );
                }
                for (device, name) in &i.banned {
                    card = card.push(row![muted(trf("Entfernt: {}", &[&name])), Space::new().width(Length::Fill), small(tr("Wieder zulassen"), Message::AllowAgain(i.id.clone(), device.clone()))].align_y(Alignment::Center));
                }
                let qr: Element<'a, Message> = match &self.share_qr {
                    Some(data) => row![scannable_qr(data, 3.0), muted(tr("Mit dem Tablet scannen").into())].spacing(12).align_y(Alignment::Center).into(),
                    None => Space::new().into(),
                };
                card = card.push(row![qr, Space::new().width(Length::Fill), small(tr("Link widerrufen"), Message::Revoke(i.id.clone()))].align_y(Alignment::End));
            }
            col = col.push(container(card).padding(12).width(Length::Fill).style(theme::group));
        }
        col = col.push(
            row![
                muted(tr("Einladen:").into()),
                button(theme::btn_small(tr("Nur lesen"))).padding(theme::pad()).on_press(Message::Invite(false, 7)).style(theme::secondary),
                button(theme::btn_small(tr("Lesen & schreiben"))).padding(theme::pad()).on_press(Message::Invite(true, 7)).style(theme::secondary),
                muted(tr("· 7 Tage, änderbar").into()),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
        col = col.push(container(Space::new().height(1)).width(Length::Fill).style(theme::hairline));
        col = col.push(
            row![
                iced::widget::toggler(self.settings.guest_frames).label(tr("Zeigen, wo Gäste hinschauen")).on_toggle(Message::GuestFrames).size(16).text_size(12).spacing(8).style(theme::switch),
                Space::new().width(24),
                iced::widget::toggler(self.settings.share_view).label(tr("Gäste sehen, wo ich hinschaue")).on_toggle(Message::ShareView).size(16).text_size(12).spacing(8).style(theme::switch),
            ]
            .align_y(Alignment::Center),
        );
        // Long with several invites: scrolls inside the window.
        container(iced::widget::scrollable(container(col).padding(20)).height(Length::Shrink)).width(Length::Fill).max_width(820).max_height(760).style(theme::panel).into()
    }

    fn help_view(&self) -> Element<'_, Message> {
        let keys = [
            (tr("Strg K"), tr("Befehle, Seiten, Notizbücher suchen")),
            (tr("Strg F"), tr("Im Notizbuch suchen: Handschrift, Text, Überschriften, PDF")),
            (tr("Rechtsklick"), tr("Bibliothek: Menü für Notizbuch, Ordner, Tag")),
            (tr("Karte ziehen"), tr("Auf einen Ordner links: dorthin verschieben")),
            ("Esc", tr("Zurück · Bearbeiten beenden · zweites Notizbuch schließen · Bibliothek")),
            (tr("⧉ in der Leiste"), tr("Daneben öffnen: dieses Notizbuch nochmal oder ein anderes")),
            (tr("Strich zwischen zweien"), tr("Ziehen: Breite (⅓, ½, ⅔) · an den Rand: schließen")),
            (tr("Bild ↑ / ↓, ← / →"), tr("Vorige / nächste Seite")),
            (tr("Pos1 / Ende"), tr("Erste / letzte Seite")),
            (tr("Mausrad · Ziehen"), tr("Verschieben")),
            (tr("Touchpad: zwei Finger"), tr("Verschieben, auseinander / zusammen: zoomen")),
            (tr("Strg + Mausrad, Strg + / −"), tr("Zoomen, Strg 0: 100 %")),
            (tr("Strg R"), tr("Seite drehen (hoch ↔ quer)")),
            (tr("Doppelklick · Strg T"), tr("Textfeld")),
            (tr("Klick auf ein Koordinatensystem"), tr("Funktionen und Bereich ändern (Strg K: einfügen)")),
            (tr("Strg N"), tr("Neue Textseite (Bibliothek: neues Notizbuch)")),
            (tr("Strg M"), tr("Notizen neben der Seite")),
            (tr("Strg O"), tr("PDF einfügen / importieren (oder Datei ins Fenster ziehen)")),
            (tr("Strg E"), tr("PDF exportieren")),
            (tr("Strg B"), tr("Gliederung ein/aus")),
            (tr("Strg H"), tr("Verlauf: frühere Stände wiederherstellen")),
            (tr("Strg I"), tr("Vom Tablet übernehmen (.n0te)")),
            (tr("Strg ,"), tr("Einstellungen")),
            ("F1", tr("Diese Übersicht")),
        ];
        let mut col = column![theme::heading(tr("Tastenkürzel"), 22.0)].spacing(9);
        for (k, what) in keys {
            col = col.push(row![text(k).font(theme::MONO_BOLD).size(12).width(Length::Fixed(230.0)), text(what).size(14)].align_y(Alignment::Center));
        }
        col = col.push(button(theme::btn(tr("Schließen"))).padding(theme::pad()).on_press(Message::ShowHelp(false)).style(theme::ghost));
        container(container(col).padding(20).width(Length::Fixed(640.0)).style(theme::panel)).padding(Padding { top: 70.0, ..Padding::ZERO }).center_x(Length::Fill).into()
    }

    fn devices_view(&self) -> Element<'_, Message> {
        let mut col = column![].spacing(10);
        if self.sync.is_none() {
            col = col.push(text(live::service_hint()).size(13).color(theme::p().accent));
            return col.into();
        }
        if self.devices.is_empty() {
            col = col.push(text(tr("Noch kein Gerät gekoppelt.")).size(13).color(theme::p().muted));
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
                    text(if online { tr("verbunden") } else { tr("nicht erreichbar") }).size(12).color(theme::p().muted),
                ]
                .spacing(10)
                .align_y(Alignment::Center),
            );
        }
        // The other way round: a code (or n0te-pair: link) another device shows, typed or pasted
        // here — n0te Tablet on Windows pairs with a laptop like this (no camera needed).
        col = col.push(
            row![
                text_input(tr("Code eines anderen Geräts einfügen (n0te-pair:…)"), &self.pair_enter).on_input(Message::PairEnter).on_submit(Message::PairAccept).padding(8).size(13).style(theme::field),
                button(theme::btn_small(tr("Koppeln"))).padding(theme::pad()).on_press_maybe((!self.pair_enter.trim().is_empty()).then_some(Message::PairAccept)).style(theme::secondary),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        );
        let pair_label = if crate::mode() == crate::Mode::Tablet { tr("Gerät koppeln (Code zeigen)") } else { tr("Tablet koppeln") };
        match &self.pair_code {
            None => col = col.push(button(theme::btn(pair_label)).padding(theme::pad()).on_press(Message::PairStart).style(theme::secondary)),
            Some(code) => {
                // The QR code: the tablet scans it (n0te → Einstellungen → Laptop → QR scannen).
                if let Some(data) = &self.pair_qr {
                    col = col.push(
                        row![
                            scannable_qr(data, 4.0),
                            Space::new().width(16),
                            column![
                                text(tr("Am Tablet scannen")).font(theme::title_font()).size(14).color(theme::p().text),
                                text(tr("n0te → Einstellungen → Laptop → QR scannen. Dafür braucht es kein n0bridge. Der Code gilt 15 Minuten und nur einmal.")).size(12).color(theme::p().muted),
                            ]
                            .spacing(4)
                            .width(Length::Fill),
                        ]
                        .align_y(Alignment::Center),
                    );
                }
                col = col.push(text_input("", code).size(12).font(theme::MONO).padding(8).style(theme::field));
                let mut buttons = row![button(theme::btn_small(tr("Kopieren"))).padding(theme::pad()).on_press(Message::CopyText(code.clone())).style(theme::secondary)].align_y(Alignment::Center);
                // With n0bridge on both devices the code can go straight to the tablet's clipboard.
                if n0bridge_installed() {
                    buttons = buttons.push(Space::new().width(8)).push(button(theme::btn_small(tr("An Tablet schicken (n0bridge)"))).padding(theme::pad()).on_press(Message::PairToTablet).style(theme::ghost));
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
                        text(tr(t.name)).font(theme::title_font()).size(14).color(theme::p().text),
                        text(tr(t.style.desc)).size(11.5).color(theme::p().muted),
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
                text(format!("{:.0} mm", v * 0.3528)).font(theme::meta_font()).size(12).width(Length::Fixed(56.0)).align_x(iced::alignment::Horizontal::Right),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
        };
        let section = settings_section;
        let rule = || container(Space::new()).height(1).width(Length::Fill).style(theme::hairline);
        let body = column![
            section(tr("Design"), tr("Farben, Formen und Schrift. Exportierte PDFs bleiben weiß."), designs.into()),
            rule(),
            section(tr("Abstände"), tr("Zwischen den Seiten im Notizbuch."), column![slider(tr("Zwischen Seiten"), self.settings.row_gap, Message::SetRowGap), slider(tr("Zu Seitenblättern"), self.settings.side_gap, Message::SetSideGap)].spacing(8).into()),
            rule(),
            section(
                "Touchpad",
                tr("Zwei Finger verschieben, auseinander und zusammen zoomt."),
                iced::widget::toggler(self.settings.momentum)
                    .label(tr("Schwung: die Seite gleitet nach dem Wischen weiter (Finger auflegen hält sie an)"))
                    .on_toggle(Message::Momentum)
                    .size(20)
                    .text_size(14)
                    .spacing(12)
                    .style(theme::switch)
                    .into(),
            ),
            rule(),
            section(
                tr("Text"),
                tr("Textseiten und Textfelder."),
                iced::widget::toggler(self.settings.formula_center).label(tr("Formeln zentrieren ($$…$$ und Formelblöcke)")).on_toggle(Message::FormulaCenter).size(20).text_size(14).spacing(12).style(theme::switch).into(),
            ),
            rule(),
            section(
                tr("Sprache"),
                tr("Automatisch: unter Windows die Sprache des Systems."),
                {
                    let opt = |label: &'static str, v: &'static str| button(text(label).size(13)).padding([6, 12]).on_press(Message::Language(v.into())).style(theme::option(self.settings.language == v));
                    row![opt(tr("Automatisch"), "auto"), opt(tr("Deutsch"), "de"), opt(tr("English"), "en")].spacing(6).into()
                },
            ),
            rule(),
            section(tr("Geräte"), tr("Sync mit dem Tablet, direkt und ohne Server."), self.devices_view()),
        ]
        .spacing(18);
        // Windows: updates from GitHub (install once, new versions come by themselves).
        let body = if crate::updater::supported() {
            use crate::updater::Status;
            let state = match &self.update {
                Status::Idle => trf("Version {}", &[&crate::updater::VERSION]),
                Status::Checking => tr("Sucht nach Updates …").to_string(),
                Status::UpToDate => trf("Version {}: aktuell", &[&crate::updater::VERSION]),
                Status::Ready(v) => trf("n0te {} ist bereit: installiert sich beim nächsten Start", &[v]),
                Status::Available(v, _) => trf("n0te {} ist da (tragbare Version: bitte neu herunterladen)", &[v]),
                Status::Failed(e) => trf("Update-Suche ging nicht: {}", &[e]),
            };
            body.push(rule()).push(section(
                tr("Updates"),
                tr("Neue Versionen laden sich von GitHub und installieren sich beim nächsten Start."),
                column![
                    iced::widget::toggler(self.settings.auto_update).label(tr("Automatisch aktualisieren")).on_toggle(Message::AutoUpdate).size(20).text_size(14).spacing(12).style(theme::switch),
                    row![
                        text(state).size(13).color(theme::p().muted),
                        Space::new().width(Length::Fill),
                        button(theme::btn_small(tr("Jetzt suchen"))).padding(theme::pad()).on_press_maybe((self.update != Status::Checking).then_some(Message::CheckUpdate)).style(theme::ghost),
                    ]
                    .align_y(Alignment::Center),
                ]
                .spacing(10)
                .into(),
            ))
        } else {
            body
        };
        let col = column![
            row![theme::heading(tr("Einstellungen"), 24.0), Space::new().width(Length::Fill), button(theme::btn_small(tr("Schließen"))).padding(theme::pad()).on_press(Message::ShowSettings(false)).style(theme::ghost)].align_y(Alignment::Center),
            iced::widget::scrollable(body).style(theme::scroll_dark).height(Length::Shrink),
            text(tr("Gespeichert in ~/.config/n0te/laptop.conf")).font(theme::meta_font()).size(11.5).color(theme::p().muted),
        ]
        .spacing(20);
        container(container(col).padding(26).width(Length::Fixed(860.0)).max_height(820.0).style(theme::panel)).padding(Padding { top: 40.0, ..Padding::ZERO }).center_x(Length::Fill).into()
    }

    /// The notebook screen: the toolbar for the active pane, live with the tablet, the panes
    /// (one, or two side by side), the status bar.
    fn notebook_view<'a>(&'a self, s: &'a Split) -> Element<'a, Message> {
        let o = s.active_pane();
        let live_bar: Element<'a, Message> = match &self.live_session {
            Some(l) => {
                // With two panes: live goes with ★.
                let label = if s.panes.len() > 1 { trf("Live mit {} · ★", &[&l.name]) } else { trf("Live mit {}", &[&l.name]) };
                container(
                    row![
                        text(theme::cased(&label)).font(theme::label_font()).size(theme::label_size()).color(theme::p().accent),
                        Space::new().width(18),
                        iced::widget::checkbox(l.follow_them).label(tr("Ansicht folgt dem Tablet")).on_toggle(Message::FollowThem).size(14).text_size(12),
                        Space::new().width(18),
                        iced::widget::checkbox(l.they_follow).label(tr("Tablet folgt mir")).on_toggle(Message::TheyFollow).size(14).text_size(12),
                        Space::new().width(18),
                        iced::widget::checkbox(self.settings.remote_frame).label(tr("Rahmen zeigen")).on_toggle(Message::RemoteFrame).size(14).text_size(12),
                        Space::new().width(Length::Fill),
                        button(theme::btn_small(tr("Beenden"))).padding([3, 10]).on_press(Message::StopLive).style(theme::ghost),
                    ]
                    .align_y(Alignment::Center),
                )
                .padding([4, 14])
                .width(Length::Fill)
                .style(theme::chrome)
                .into()
            }
            None => Space::new().height(0).into(),
        };
        let tablet_bar: Element<'a, Message> = if crate::mode() == crate::Mode::Tablet { crate::tablet::toolbar::view(&self.tablet) } else { Space::new().height(0).into() };
        let panes: Element<'a, Message> = if crate::mode() == crate::Mode::Tablet && self.tablet.pen_menu {
            stack![split::view(self, s), container(crate::tablet::toolbar::pen_menu(&self.tablet)).padding([4, 10])].into()
        } else {
            split::view(self, s)
        };
        column![
            self.top_bar(s, o, o.pages.current_number(&o.camera)),
            container(Space::new()).height(1).width(Length::Fill).style(theme::hairline),
            tablet_bar,
            live_bar,
            panes,
            container(Space::new()).height(1).width(Length::Fill).style(theme::hairline),
            self.status_bar(),
        ]
        .into()
    }

    /// One pane: its outline, the pages (or its text page) with the text boxes, and, in the
    /// active one, the panel opened from the toolbar over them.
    pub(crate) fn pane_view<'a>(&'a self, o: &'a Open, active: bool) -> Element<'a, Message> {
        let current = o.pages.current_number(&o.camera);
        let page_view = pages::page_view(&o.pages, o.camera, Message::Camera, Message::Fold);
        // n0te Tablet: the pen (and the mouse) write; laptop: a double click makes a text box.
        let tablet = crate::mode() == crate::Mode::Tablet;
        let page_view = if tablet { page_view.on_pen(Message::Pen).finger_draws(self.tablet.finger_draws) } else { page_view.on_double_click(Message::NewText).on_plot_click(Message::EditPlot) };
        let mut layers: Vec<Element<'a, Message>> = vec![page_view.into()];
        layers.extend(self.text_layers(o));
        if tablet && let Some(overlay) = crate::tablet::overlay::view(&self.tablet, o) {
            layers.push(overlay);
        }
        let canvas: Element<'a, Message> = iced::widget::Stack::with_children(layers).into();
        let mut body = row![];
        if o.sidebar {
            body = body.push(sidebar::view(self, o, current, sidebar::Ids { other: !active }));
            body = body.push(container(Space::new()).width(1).height(Length::Fill).style(theme::hairline));
        }
        let panel: Option<Element<'a, Message>> = if !active {
            None
        } else if let Some(h) = &self.history {
            Some(self.history_panel(h))
        } else if let Some(invites) = &self.share {
            Some(self.share_panel(invites))
        } else if self.show_pdf_import {
            Some(library::pdf_panel(&self.pdfs, tr("PDF einfügen: nach der aktuellen Seite")))
        } else if self.page_panel {
            Some(self.page_panel_view(o))
        } else {
            None
        };
        let canvas: Element<'a, Message> = match panel {
            Some(p) => stack![canvas, container(p).padding(16).width(Length::Fill)].into(),
            None => canvas,
        };
        match &o.text_page {
            Some(t) if !t.docked => body = body.push(self.text_page_view(t, o)),
            Some(t) => {
                body = body.push(canvas);
                body = body.push(container(Space::new()).width(1).height(Length::Fill).style(theme::hairline));
                body = body.push(container(self.text_page_view(t, o)).width(Length::Fixed(460.0)));
            }
            None => body = body.push(canvas),
        }
        body.height(Length::Fill).into()
    }

    fn top_bar<'a>(&'a self, s: &'a Split, o: &'a Open, current: u32) -> Element<'a, Message> {
        let chip = |s: String| container(text(s).font(theme::meta_font()).size(theme::SMALL)).padding([3, 10]).style(theme::chip);
        // Two panes: live is the main pane's (★).
        let star = if s.panes.len() > 1 { "★ " } else { "" };
        container(
            row![
                button(theme::btn(tr("‹ Bibliothek"))).padding(theme::pad()).on_press(Message::Back).style(theme::ghost),
                Space::new().width(18),
                text(theme::cased(&o.nb.title())).font(theme::title_font()).size(14).color(theme::p().text),
                Space::new().width(Length::Fill),
                chip(format!("{:>3.0} %", o.camera.zoom * 100.0)),
                Space::new().width(8),
                // The page number opens the page's paper and format (like on the tablet).
                button(text(format!("{current} / {}", o.pages.total_pages)).font(theme::meta_font()).size(theme::SMALL))
                    .padding([3, 10])
                    .on_press(Message::ShowPage(!self.page_panel))
                    .style(theme::tab(self.page_panel)),
                Space::new().width(6),
                split::menu_button(s.menu.is_some()),
                Space::new().width(14),
                button(theme::btn(tr("+ Textseite"))).padding(theme::pad()).on_press(Message::NewTextPage).style(theme::tab(false)),
                Space::new().width(6),
                button(theme::btn(tr("+ Koordinatensystem"))).padding(theme::pad()).on_press(Message::InsertPlot).style(theme::tab(false)),
                Space::new().width(6),
                button(theme::btn(tr("Notizen"))).padding(theme::pad()).on_press(Message::ToggleNotes).style(theme::tab(o.text_page.as_ref().is_some_and(|t| t.docked))),
                Space::new().width(6),
                button(theme::btn(tr("PDF einfügen"))).padding(theme::pad()).on_press(Message::ShowPdfImport(!self.show_pdf_import)).style(theme::tab(self.show_pdf_import)),
                Space::new().width(6),
                button(theme::btn("Export")).padding(theme::pad()).on_press(Message::ShowExport(self.export.is_none())).style(theme::tab(self.export.is_some())),
                Space::new().width(6),
                // Guests here show on the button ("Teilen · 2 Gäste"); as a guest: whose notebook.
                button(theme::btn(&self.share_label(&o.id))).padding(theme::pad()).on_press(Message::ShowShare(self.share.is_none())).style(theme::tab(self.share.is_some())),
                Space::new().width(6),
                match &self.live_session {
                    Some(l) => button(theme::btn(&format!("{star}● Live · {}", l.name))).padding(theme::pad()).on_press(Message::StopLive).style(theme::accent_button),
                    None => button(theme::btn(&format!("{star}Live"))).padding(theme::pad()).on_press(Message::StartLive).style(theme::tab(false)),
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

    fn share_label(&self, nb: &str) -> String {
        if let Some(j) = self.joined.iter().find(|j| j.notebook == nb) {
            return trf("{} Gast bei {}{}", &[&if j.online { "●" } else { "○" }, &j.owner_name, &if j.write { "" } else { tr(" · nur lesen") }]);
        }
        let knocking: Vec<&live::Guest> = self.waiting.iter().filter(|g| g.notebook == nb).collect();
        if !knocking.is_empty() {
            return if knocking.len() == 1 { trf("Teilen · ● {} wartet", &[&knocking[0].name]) } else { trf("Teilen · ● {} warten", &[&knocking.len()]) };
        }
        let here: Vec<&live::Guest> = self.guests.iter().filter(|g| g.notebook == nb).collect();
        match here.len() {
            0 => tr("Teilen").into(),
            1 => trf("Teilen · ● {}", &[&here[0].name]),
            n => trf("Teilen · ● {} Gäste", &[&n]),
        }
    }

    pub fn status_bar(&self) -> Element<'_, Message> {
        let left: Element<'_, Message> = match &self.status {
            Some(s) => row![
                text(s.as_str()).font(theme::meta_font()).size(12).color(theme::p().text),
                Space::new().width(10),
                match &self.last_export {
                    Some(p) => Element::from(button(theme::btn_small(tr("Öffnen"))).padding([0, 8]).on_press(Message::OpenPath(p.clone())).style(theme::ghost)),
                    None => Space::new().into(),
                },
                Space::new().width(6),
                if self.undo_delete.as_ref().is_some_and(|nb| matches!(&self.screen, Screen::Notebook(s) if s.shows(nb))) {
                    Element::from(button(theme::btn_small(tr("Rückgängig"))).padding([0, 8]).on_press(Message::Outline(sidebar::Msg::Undo)).style(theme::ghost))
                } else {
                    Space::new().into()
                },
                Space::new().width(6),
                button(text("×").size(12)).padding([0, 6]).on_press(Message::DismissStatus).style(theme::ghost),
            ]
            .align_y(Alignment::Center)
            .into(),
            None => text(theme::cased(tr("Strg K: Befehle · F1: Tastenkürzel"))).font(theme::meta_font()).size(11.5).color(theme::p().muted).into(),
        };
        let update: Element<'_, Message> = match &self.update {
            crate::updater::Status::Ready(v) => row![
                text(trf("n0te {} ist bereit", &[v])).font(theme::meta_font()).size(12).color(theme::p().accent),
                Space::new().width(8),
                button(theme::btn_small(tr("Neu starten"))).padding([0, 8]).on_press(Message::UpdateNow).style(theme::ghost),
            ]
            .align_y(Alignment::Center)
            .into(),
            crate::updater::Status::Available(v, page) => button(text(trf("n0te {} ist da", &[v])).font(theme::meta_font()).size(12).color(theme::p().accent))
                .padding([0, 8])
                .on_press(Message::OpenPath(std::path::PathBuf::from(page)))
                .style(theme::ghost)
                .into(),
            _ => Space::new().into(),
        };
        container(row![left, Space::new().width(Length::Fill), update].align_y(Alignment::Center))
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
        for (b, label) in [(Background::Blank, tr("Leer")), (Background::Lines, tr("Linien")), (Background::Grid, tr("Kästchen")), (Background::Dots, trc("paper", "Punkte")), (Background::Cornell, "Cornell")] {
            papers = papers.push(choice(label, self.new_paper == b, Message::NewPaper(b)));
        }
        let label = |s: &str| container(theme::caption(s)).width(Length::Fixed(130.0));
        let mut col = column![
            theme::heading(tr("Neues Notizbuch"), 20.0),
            row![
                text_input(tr("Titel, z. B. Analysis I"), t).on_input(Message::NewTitle).on_submit(Message::Create).padding(10).size(15).style(theme::field),
                Space::new().width(10),
                button(theme::btn(tr("Anlegen"))).padding(theme::pad()).on_press(Message::Create).style(theme::accent_button),
                Space::new().width(6),
                button(theme::btn(tr("Abbrechen"))).padding(theme::pad()).on_press(Message::CancelCreate).style(theme::ghost),
            ]
            .align_y(Alignment::Center),
            row![label(tr("Papier")), papers].align_y(Alignment::Center),
        ]
        .spacing(12);
        if self.new_paper != Background::Blank {
            let what = match self.new_paper {
                Background::Grid => tr("Kästchengröße"),
                Background::Dots => tr("Punktabstand"),
                _ => tr("Linienabstand"),
            };
            let mut quick = row![].spacing(6);
            for (mm, l) in [(4.0f32, "4 mm"), (5.0, "5 mm"), (7.0, "7 mm"), (8.5, tr("8,5 mm"))] {
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
                    text(format!("{} mm", crate::i18n::number(self.new_spacing * 0.3528, 1))).font(theme::meta_font()).size(12).width(Length::Fixed(70.0)),
                    quick,
                ]
                .align_y(Alignment::Center),
            );
        }
        col = col.push(row![label("Format"), self.format_picker(self.settings.new_w, self.settings.new_h, Message::NewSize)].align_y(Alignment::Start));
        Some(container(col).padding(20).width(Length::Fill).max_width(960).style(theme::panel).into())
    }

    /// Picks a page size: upright or landscape, a format (A4, A5, …), or a size of its own in mm
    /// (typed, then applied: "2" on the way to "210" must not resize the page). `on` gets points.
    fn format_picker<'a>(&'a self, w: f32, h: f32, on: fn(f32, f32) -> Message) -> Element<'a, Message> {
        let choice = |label: &str, active: bool, m: Option<Message>| button(theme::btn_small(label)).padding(theme::pad()).on_press_maybe(m).style(theme::tab(active));
        let landscape = w > h;
        let square = (w - h).abs() < 1.0;
        let found = format::find(w, h).map(|(f, _)| f.id);
        let upright = format::oriented(w, h, false);
        let turned = format::oriented(w, h, true);
        let mut turns = row![
            choice(tr("Hoch"), !landscape, (!square && landscape).then(|| on(upright.0, upright.1))),
            choice(tr("Quer"), landscape, (!square && !landscape).then(|| on(turned.0, turned.1))),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        turns = turns.push(Space::new().width(10));
        turns = turns.push(text(format!("{:.0} × {:.0} mm", format::pt_to_mm(w), format::pt_to_mm(h))).font(theme::meta_font()).size(12).color(theme::p().muted));
        let mut names = row![].spacing(6);
        for f in format::FORMATS {
            // A slide lies in landscape by nature; the others keep the page's way round.
            let (a, b) = format::oriented(f.width, f.height, f.id == "slide" || landscape);
            names = names.push(choice(tr(f.name), self.size_text.is_none() && found == Some(f.id), Some(on(a, b))));
        }
        let custom = self.size_text.is_some() || found.is_none();
        names = names.push(choice(tr("Eigene …"), custom, Some(Message::CustomSize(self.size_text.is_none()))));
        let mut col = column![turns, names].spacing(8);
        if let Some((tw, th)) = &self.size_text {
            let mm = |t: &str| t.trim().replace(',', ".").parse::<f32>().ok().filter(|v| (25.4..=1400.0).contains(v));
            let apply = match (mm(tw), mm(th)) {
                (Some(a), Some(b)) => Some(on(format::mm_to_pt(a), format::mm_to_pt(b))),
                _ => None,
            };
            let ok = apply.is_some();
            col = col.push(
                row![
                    text_input(tr("Breite"), tw).on_input(Message::SizeWidth).padding(8).size(14).width(Length::Fixed(90.0)).style(theme::field),
                    text("×").size(14).color(theme::p().muted),
                    text_input(tr("Höhe"), th).on_input(Message::SizeHeight).padding(8).size(14).width(Length::Fixed(90.0)).style(theme::field),
                    text("mm").size(13).color(theme::p().muted),
                    Space::new().width(6),
                    button(theme::btn_small(tr("Übernehmen"))).padding(theme::pad()).on_press_maybe(apply).style(theme::secondary),
                    Space::new().width(10),
                    text(if ok { "" } else { tr("Jede Seite zwischen 25 mm und 1,4 m") }).size(12).color(theme::p().muted),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
            );
        }
        col.into()
    }

    /// Paper and format of the page in view; "für alle" passes both on to every page.
    fn page_panel_view<'a>(&'a self, o: &'a Open) -> Element<'a, Message> {
        let current = o.pages.current_number(&o.camera);
        let head = row![
            theme::heading(&trf("Seite {}", &[&current]), 20.0),
            Space::new().width(Length::Fill),
            button(theme::btn_small(tr("Schließen"))).padding(theme::pad()).on_press(Message::ShowPage(false)).style(theme::ghost),
        ]
        .align_y(Alignment::Center);
        let label = |s: &str| container(theme::caption(s)).width(Length::Fixed(110.0));
        let note = |s: String| text(s).size(13).color(theme::p().muted);
        let mut col = column![head].spacing(14);
        match self.page_in_view() {
            None => {}
            Some(_) if self.read_only() => col = col.push(note(tr("Nur lesen: Papier und Format ändert, wer dich eingeladen hat.").into())),
            Some((_, spec)) if spec.pdf.is_some() => col = col.push(note(trf("PDF-Seite: Hintergrund und Format ({}) kommen aus der PDF.", &[&sidebar::format_name(spec.width, spec.height)]))),
            Some((_, spec)) if spec.is_board() => col = col.push(note(tr("Ein Board ist unendlich groß, mit Punktraster.").into())),
            Some((_, spec)) => {
                let text_page = spec.background == Background::Text;
                if !text_page {
                    let mut papers = row![].spacing(6);
                    for (b, l) in [(Background::Blank, tr("Leer")), (Background::Lines, tr("Linien")), (Background::Grid, tr("Kästchen")), (Background::Dots, trc("paper", "Punkte")), (Background::Cornell, "Cornell")] {
                        papers = papers.push(button(theme::btn_small(l)).padding(theme::pad()).on_press(Message::PagePaper(b)).style(theme::tab(spec.background == b)));
                    }
                    col = col.push(row![label(tr("Papier")), papers].align_y(Alignment::Center));
                }
                col = col.push(row![label("Format"), self.format_picker(spec.width, spec.height, Message::PageSize)].align_y(Alignment::Start));
                col = col.push(
                    row![
                        Space::new().width(110),
                        button(theme::btn_small(tr("Für alle Seiten übernehmen"))).padding(theme::pad()).on_press(Message::PageToAll).style(theme::secondary),
                        Space::new().width(10),
                        note(if text_page { tr("Dieses Format für jede Seite (keine PDF-Seiten)").into() } else { tr("Papier und Format für jede Seite (keine PDF-Seiten)").into() }),
                    ]
                    .align_y(Alignment::Center),
                );
            }
        }
        container(col).padding(20).width(Length::Fill).max_width(820).style(theme::panel).into()
    }

    pub fn scroll<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
        scrollable(content).style(theme::scroll_dark).height(Length::Fill).width(Length::Fill).into()
    }
}

/// Where a pane looks: the sheet nearest the middle and the view in its points.
fn view_of(o: &Open) -> Option<(String, [f32; 4])> {
    let c = o.camera;
    let (cx, cy) = (c.x + c.viewport.width / c.zoom / 2.0, c.y + c.viewport.height / c.zoom / 2.0);
    let p = o.pages.sheet_at(cx, cy).or_else(|| o.pages.pages.iter().map(|p| &p.placed).min_by(|a, b| (a.rect.y0 - cy).abs().total_cmp(&(b.rect.y0 - cy).abs())))?;
    let v = c.view_rect();
    Some((p.sheet_id.clone(), [v.x0 - p.rect.x0, v.y0 - p.rect.y0, v.x1 - p.rect.x0, v.y1 - p.rect.y0]))
}

/// The folded groups around a sheet (main or side sheet), outermost first.
fn folded_around(nb: &Notebook, sheet: &str) -> Vec<String> {
    let mut groups: Vec<(String, bool)> = Vec::new();
    for row in nb.outline(true) {
        match row {
            OutlineRow::Group { node, depth, collapsed, .. } => {
                groups.truncate(depth as usize);
                groups.push((node, collapsed));
            }
            OutlineRow::Sheet { depth, sheet_id, left, right, .. } => {
                groups.truncate(depth as usize);
                if sheet_id == sheet || left.iter().chain(&right).any(|s| s.sheet_id == sheet) {
                    return groups.into_iter().filter(|g| g.1).map(|g| g.0).collect();
                }
            }
        }
    }
    Vec::new()
}

/// Every plain page of a notebook (main and side sheets; no PDF pages, no boards): (node, spec).
fn plain_sheets(nb: &Notebook) -> Vec<(String, SheetSpec)> {
    let mut out = Vec::new();
    for row in nb.outline(true) {
        if let OutlineRow::Sheet { node, spec, left, right, .. } = row {
            out.push((node, spec));
            out.extend(left.into_iter().chain(right).map(|s| (s.node, s.spec)));
        }
    }
    out.retain(|(_, s)| s.pdf.is_none() && !s.is_board());
    out
}

/// The usual spacing of a paper (pt): 6 mm boxes, 7.8 mm lines, 7 mm dots.
fn default_spacing(b: Background) -> f32 {
    match b {
        Background::Grid => 17.0,
        Background::Dots => 20.0,
        _ => 22.0,
    }
}

fn downloads() -> PathBuf {
    n0te_platform::paths::downloads().unwrap_or_else(std::env::temp_dir)
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
    let out = std::process::Command::new("n0bridge").args(&args).output().map_err(|e| trf("n0bridge nicht gefunden ({})", &[&e]))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    if out.status.success() { Ok(text.trim().to_string()) } else { Err(text.trim().lines().last().unwrap_or(tr("Fehler")).to_string()) }
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

/// Whether the n0bridge CLI is installed (then codes and notebooks can go straight to the
/// tablet). Never on Windows: n0bridge is Linux only.
pub(crate) fn n0bridge_installed() -> bool {
    !cfg!(windows) && std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("n0bridge").is_file()))
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
        .map_err(|e| trf("n0bridge nicht gefunden ({})", &[&e]))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    if out.status.success() { Ok(text.trim().to_string()) } else { Err(text.trim().lines().last().unwrap_or(tr("Fehler")).to_string()) }
}

#[cfg(test)]
mod n0bridge_tests {
    #[test]
    fn finds_the_tablet_id() {
        let out = "Tablet_s7_fe_alex   id=a1b2c3d4e5f60718 [tablet,terminal] app=0.14.6 akku=97%\nphone-3a           id=0f1e2d3c4b5a6978 [phone,primary,auth] app=0.14.2\n";
        assert_eq!(super::tablet_id(out).as_deref(), Some("a1b2c3d4e5f60718"));
        assert_eq!(super::tablet_id("phone id=1 [phone,primary]\n"), None);
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// "3 Striche, 1 Bild und 2 Textfelder" in the app's language (`Outside::describe`, translated;
/// empty when nothing sticks out).
fn describe_outside(o: &n0te_core::sheet::Outside) -> String {
    let n = |k: usize, one: String, many: String| match k {
        0 => None,
        1 => Some(one),
        _ => Some(many),
    };
    let parts: Vec<String> = [
        n(o.strokes, trf("{} Strich", &[&o.strokes]), trf("{} Striche", &[&o.strokes])),
        n(o.images, trf("{} Bild", &[&o.images]), trf("{} Bilder", &[&o.images])),
        n(o.texts, trf("{} Textfeld", &[&o.texts]), trf("{} Textfelder", &[&o.texts])),
    ]
    .into_iter()
    .flatten()
    .collect();
    match parts.as_slice() {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => trf("{} und {}", &[&rest.join(", "), last]),
    }
}

/// "noch 6 Tage", "noch 5 Std.", "noch 20 Min." (no time zone needed).
fn remaining(secs: u64) -> String {
    match secs {
        s if s >= 2 * 86_400 => trf("noch {} Tage", &[&(s / 86_400)]),
        s if s >= 86_400 => tr("noch 1 Tag").into(),
        s if s >= 3600 => trf("noch {} Std.", &[&(s / 3600)]),
        s => trf("noch {} Min.", &[&(s / 60).max(1)]),
    }
}

/// A 4-digit PIN for an invite (random per call: RandomState keys come from the OS).
fn new_pin() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(unix_now());
    format!("{}", 1000 + h.finish() % 9000)
}
