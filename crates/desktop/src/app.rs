//! The application: the library (notebooks by folder, import of `.n0te` bundles) and an open
//! notebook (outline sidebar, page canvas, top and status bar).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::SystemTime;

use iced::keyboard::{self, Key, key::Named};
use iced::widget::{Space, button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Element, Length, Padding, Subscription, Task, Theme};

use n0te_core::layout::{self as nl, Options};
use n0te_core::notebook::{Background, Notebook, OutlineRow, SheetSpec};
use n0te_core::store::Library;

use crate::pages::{self, Camera, Pages};
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
        let pages = Pages::new(layout, Box::new(move |sheet| lib.load_sheet(&nid, sheet).map(|s| s.strokes()).unwrap_or_default()), self.pdf.clone());
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
                self.screen = Screen::Library;
                self.refresh();
            }
            Message::Refresh => self.refresh(),
            Message::Camera(c) => {
                if let Screen::Notebook(o) = &mut self.screen {
                    o.camera = c;
                }
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
        }
        Task::none()
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
        let canvas: Element<'a, Message> = pages::page_view(&o.pages, o.camera, Message::Camera, Message::Fold).into();
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
            Some(p) => iced::widget::stack![canvas, container(p).padding(16).width(Length::Fill)].into(),
            None => canvas,
        };
        body = body.push(canvas);
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
