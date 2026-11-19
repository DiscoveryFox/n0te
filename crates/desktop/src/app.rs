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

use n0te_core::layout::{self as nl, Options};
use n0te_core::notebook::{Background, Notebook, OutlineRow, SheetSpec};
use n0te_core::sheet::Sheet;
use n0te_core::store::Library;

use crate::pages::{self, Camera, Pages, SheetContent};
use crate::pdf::{self, Pdf};
use crate::{library, sidebar, theme};

/// A notebook in the library list.
#[derive(Debug, Clone)]
pub struct Entry {
    pub id: String,
    pub title: String,
    pub folder: String,
    pub sheets: usize,
    pub cover: String,
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
    NewTitle(String),
    Create,
    CancelCreate,
    DismissStatus,
    /// A PDF tile arrived (redraw).
    TileReady,
    FileDropped(PathBuf),
    ShowPdfImport(bool),
    ImportPdf(PathBuf),
    ShowExport(bool),
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
    pub last_export: Option<PathBuf>,
    /// Where exported PDFs go (~/Downloads).
    pub export_dir: PathBuf,
    pub editing: Option<Editing>,
    pub text_page: Option<TextPage>,
    pub entries: Vec<Entry>,
    pub bundles: Vec<Bundle>,
    pub screen: Screen,
    pub status: Option<String>,
    pub creating: Option<String>,
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
    pub fn new(root: impl AsRef<Path>) -> Self {
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
            last_export: None,
            export_dir: downloads(),
            editing: None,
            text_page: None,
            entries: Vec::new(),
            bundles: Vec::new(),
            screen: Screen::Library,
            status: None,
            creating: None,
            show_imports: false,
            sidebar: true,
        };
        app.refresh();
        app
    }

    /// Starts the app; `import` = a `.n0te` file given on the command line (opened after import).
    pub fn boot_with(root: PathBuf, import: Option<PathBuf>) -> (Self, Task<Message>) {
        let app = Self::new(root);
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
            Subscription::run(ticks).map(|_| Message::Tick),
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
                Some(Entry { title: nb.title(), folder: nb.folder(), sheets: nb.sheet_count(), cover: nb.cover(), id })
            })
            .collect();
        entries.sort_by(|a, b| (a.folder.to_lowercase(), a.title.to_lowercase()).cmp(&(b.folder.to_lowercase(), b.title.to_lowercase())));
        self.entries = entries;
        self.bundles = find_bundles(&bundle_dirs());
    }

    fn open(&mut self, id: &str) -> Result<(), String> {
        let nb = self.lib.load_notebook(id).map_err(|e| e.to_string())?;
        let rows = nb.outline(false);
        let layout = nl::layout(&rows, &nb.outline(true), Options::default());
        let lib = self.lib.clone();
        let nid = id.to_string();
        let pages = Pages::new(
            layout,
            Box::new(move |sheet| lib.load_sheet(&nid, sheet).map(|s| SheetContent { strokes: s.strokes(), texts: s.text_boxes(), body: s.page_text() }).unwrap_or_default()),
            self.pdf.clone(),
        );
        self.finish_text();
        self.close_text_page();
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
                self.screen = Screen::Library;
                self.refresh();
            }
            Message::Refresh => self.refresh(),
            Message::Camera(c) => {
                if let Screen::Notebook(o) = &mut self.screen {
                    o.camera = c;
                }
                self.follow_notes();
            }
            Message::Fold(node) => {
                let lib = self.lib.clone();
                if let Screen::Notebook(o) = &mut self.screen {
                    let collapsed = o.rows.iter().any(|r| matches!(r, OutlineRow::Group { node: n, collapsed: true, .. } if *n == node));
                    if o.nb.set_collapsed(&node, !collapsed).is_ok() {
                        let _ = lib.save_notebook(&o.nb);
                        o.rows = o.nb.outline(false);
                        o.pages.relayout(nl::layout(&o.rows, &o.nb.outline(true), Options::default()));
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
            Message::NewTitle(t) => self.creating = Some(t),
            Message::CancelCreate => self.creating = None,
            Message::Create => {
                let title = self.creating.take().unwrap_or_default();
                let title = if title.trim().is_empty() { "Neues Notizbuch".to_string() } else { title.trim().to_string() };
                let made = self.lib.create_notebook(&title).and_then(|nb| {
                    nb.add_sheet(None, 0, &SheetSpec { spacing: 17.0, ..SheetSpec::a4(Background::Grid) })?;
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
            Message::ShowExport(on) => self.export = on.then(HashSet::new),
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
            button(text("SCHLIESSEN").font(theme::MONO_BOLD).size(10)).padding([2, 8]).on_press(Message::CloseTextPage).style(theme::text_tool),
        ]
        .align_y(Alignment::Center);
        let paper = container(column![header, Space::new().height(14), t.doc.view().map(Message::Blocks)])
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
                .style(theme::editor_on_paper_framed);
            out.push(pin(column![tools, container(editor).width(Length::Fixed(e.w * cam.zoom))].spacing(4)).x(x - 4.0).y(y - 32.0).into());
        }
        out
    }

    /// Lays the open notebook out again after a structural change (keeps the camera).
    fn relayout(&mut self) {
        if let Screen::Notebook(o) = &mut self.screen {
            o.rows = o.nb.outline(false);
            o.pages.relayout(nl::layout(&o.rows, &o.nb.outline(true), Options::default()));
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
        match &self.screen {
            Screen::Library => library::view(self),
            Screen::Notebook(o) => self.notebook_view(o),
        }
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
        let panel: Option<Element<'a, Message>> = if self.show_pdf_import {
            Some(library::pdf_panel(&self.pdfs, "PDF EINFÜGEN — NACH DER AKTUELLEN SEITE"))
        } else {
            self.export.as_ref().map(|picked| crate::library::export_panel(o, picked))
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
        column![
            self.top_bar(o, current),
            container(Space::new()).height(1).width(Length::Fill).style(theme::hairline),
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
                button(text("‹ BIBLIOTHEK").font(theme::MONO_BOLD).size(theme::LABEL)).padding([5, 12]).on_press(Message::Back).style(theme::ghost),
                Space::new().width(18),
                text(o.nb.title().to_uppercase()).font(theme::MONO).size(theme::SMALL).color(theme::TEXT),
                Space::new().width(Length::Fill),
                chip(format!("{:>3.0} %", o.camera.zoom * 100.0)),
                Space::new().width(8),
                chip(format!("{current:02} / {:02}", o.pages.total_pages)),
                Space::new().width(14),
                button(text("+ TEXTSEITE").font(theme::MONO_BOLD).size(theme::LABEL)).padding([5, 12]).on_press(Message::NewTextPage).style(theme::tab(false)),
                Space::new().width(6),
                button(text("NOTIZEN").font(theme::MONO_BOLD).size(theme::LABEL)).padding([5, 12]).on_press(Message::ToggleNotes).style(theme::tab(self.text_page.as_ref().is_some_and(|t| t.docked))),
                Space::new().width(6),
                button(text("PDF EINFÜGEN").font(theme::MONO_BOLD).size(theme::LABEL)).padding([5, 12]).on_press(Message::ShowPdfImport(!self.show_pdf_import)).style(theme::tab(self.show_pdf_import)),
                Space::new().width(6),
                button(text("EXPORT").font(theme::MONO_BOLD).size(theme::LABEL)).padding([5, 12]).on_press(Message::ShowExport(self.export.is_none())).style(theme::tab(self.export.is_some())),
                Space::new().width(6),
                button(text("AN TABLET").font(theme::MONO_BOLD).size(theme::LABEL)).padding([5, 12]).on_press(Message::SendToTablet).style(theme::tab(false)),
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
                text(s.as_str()).font(theme::MONO).size(10.5).color(theme::TEXT),
                Space::new().width(10),
                match &self.last_export {
                    Some(p) => Element::from(button(text("ÖFFNEN").font(theme::MONO_BOLD).size(10)).padding([0, 8]).on_press(Message::OpenPath(p.clone())).style(theme::ghost)),
                    None => Space::new().into(),
                },
                Space::new().width(6),
                button(text("×").size(12)).padding([0, 6]).on_press(Message::DismissStatus).style(theme::ghost),
            ]
            .align_y(Alignment::Center)
            .into(),
            None => text("ESC BIBLIOTHEK · BILD↑↓ SEITEN · STRG +/− ZOOM · STRG B GLIEDERUNG").font(theme::MONO).size(10.5).color(theme::MUTED).into(),
        };
        container(row![left, Space::new().width(Length::Fill), text(format!("N0TE {}", env!("CARGO_PKG_VERSION"))).font(theme::MONO).size(10.5).color(theme::MUTED)].align_y(Alignment::Center))
            .padding([4, 14])
            .height(26)
            .width(Length::Fill)
            .style(theme::chrome)
            .into()
    }

    /// The "new notebook" title field (library).
    pub fn create_row(&self) -> Option<Element<'_, Message>> {
        let t = self.creating.as_ref()?;
        Some(
            row![
                text_input("Titel, z. B. Analysis I", t).on_input(Message::NewTitle).on_submit(Message::Create).padding(10).size(15).width(Length::Fixed(380.0)),
                Space::new().width(10),
                button(text("ANLEGEN").font(theme::MONO_BOLD).size(theme::LABEL)).padding([8, 14]).on_press(Message::Create).style(theme::tab(true)),
                Space::new().width(8),
                button(text("ABBRECHEN").font(theme::MONO_BOLD).size(theme::LABEL)).padding([8, 14]).on_press(Message::CancelCreate).style(theme::ghost),
            ]
            .align_y(Alignment::Center)
            .into(),
        )
    }

    pub fn scroll<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
        scrollable(content).style(theme::scroll_dark).height(Length::Fill).width(Length::Fill).into()
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

/// Sends a file to the tablet with the n0bridge CLI.
async fn send_with_n0bridge(path: PathBuf) -> Result<String, String> {
    let out = std::process::Command::new("n0bridge")
        .args(["send", "--to", "tablet"])
        .arg(&path)
        .output()
        .map_err(|e| format!("n0bridge nicht gefunden ({e})"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    if out.status.success() { Ok(text.trim().to_string()) } else { Err(text.trim().lines().last().unwrap_or("Fehler").to_string()) }
}
