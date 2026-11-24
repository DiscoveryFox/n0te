//! Organising the library: a sidebar (the start, all notebooks, the pinned ones, the folder
//! tree, tags), search, order and cards or list above the notebooks, light right-click menus,
//! small dialogs to move, tag, rename and cover notebooks, several notebooks at once
//! ("Auswählen"), and dragging notebooks onto a folder, "Angeheftet" or a tag. Folder, tags and
//! pin live in each notebook's own document ("Uni/WS26": folders nest with "/"), so they sync
//! with it; a folder made here that holds nothing yet is kept in the laptop's settings. The
//! same rules as the tablet's (LibraryOrg.kt).

use std::collections::{BTreeSet, HashMap, HashSet};

use iced::widget::{Space, button, column, container, mouse_area, opaque, pin, responsive, row, scrollable, stack, svg, text, text_input};
use iced::{Alignment, Color, Element, Length, Padding, Point, Task};

use n0te_core::notebook::{Notebook, folder_path};

use crate::app::{App, Entry, Message};
use crate::library::{self, fit};
use crate::widgets::{self, Drag, pointer};
use crate::theme;

// --- state -----------------------------------------------------------------------------------

/// What the library shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Pick {
    /// The start: opened last, pinned, the folders, what lies in none.
    #[default]
    Home,
    All,
    Pinned,
    /// A folder ("Uni/WS26"): its notebooks, its subfolders as tiles before them.
    Folder(String),
    Tag(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    /// "Zuletzt geändert": the newest first.
    #[default]
    Recent,
    Name,
}

/// What a menu or an action is about: notebooks (one, or the ones ticked), a folder ("" =
/// the top level: "Alle Notizbücher") or a tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Notebooks(Vec<String>),
    Folder(String),
    Tag(String),
}

/// A menu row (or a button in the bar of "Auswählen").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Open,
    /// A notebook's title, a folder's or a tag's name.
    Rename,
    Move,
    Tags,
    /// Pins, or lets go when every one is pinned already.
    Pin,
    Cover,
    /// The first click asks, the second moves to the trash.
    Trash,
    /// A folder inside the target folder ("" = at the top).
    NewFolder,
    /// The folder goes, what was in it moves one level up (the first click asks).
    Dissolve,
    /// A tag off every notebook (the first click asks).
    Remove,
}

/// An open right-click menu: what it is about and where it was opened (window position).
#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub target: Target,
    pub at: Point,
}

/// What a name typed into the name dialog is for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Naming {
    Notebook(String),
    Folder(String),
    /// A tag, renamed on every notebook that has it.
    Tag(String),
    /// A new folder inside this one ("" = at the top).
    NewFolder(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Dialog {
    Name { what: Naming, text: String },
    /// "Verschieben nach …": the folder picked ("" = top level), a new folder being named.
    Move { ids: Vec<String>, to: String, new_folder: Option<String> },
    Tags { ids: Vec<String>, new_tag: String },
    Cover { ids: Vec<String> },
}

/// Where dragged notebooks can be let go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drop {
    /// Into a folder ("" = top level: "Alle Notizbücher").
    Folder(String),
    /// "Ohne Ordner" (shown while dragging something that lies in a folder): to the top level.
    Top,
    /// "Angeheftet": pinned.
    Pin,
    /// A tag in the sidebar: it is added.
    Tag(String),
}

/// Notebooks being dragged: which, where the pointer is (window), what is under it.
#[derive(Debug, Clone, PartialEq)]
pub struct Dragging {
    pub ids: Vec<String>,
    pub at: Point,
    pub over: Option<Drop>,
}

#[derive(Debug, Default)]
pub struct Shelf {
    pub pick: Pick,
    pub query: String,
    pub sort: Sort,
    /// Folders folded in the sidebar.
    pub folded: HashSet<String>,
    /// Folders that hold no notebook but show: made here (kept in the settings, `made`) or
    /// emptied here (for this session).
    pub empty: BTreeSet<String>,
    /// Folders made with "Neuer Ordner" that hold no notebook yet: kept in the settings.
    pub made: BTreeSet<String>,
    pub menu: Option<Menu>,
    pub dialog: Option<Dialog>,
    /// "Auswählen": the notebooks ticked (Some while that mode is on).
    pub selected: Option<BTreeSet<String>>,
    /// The row clicked once that asks before it acts (trash, dissolve, a tag off everywhere).
    pub asked: Option<(Target, Action)>,
    pub drag: Option<Dragging>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Pick(Pick),
    /// A folder in the sidebar: open / folded.
    Fold(String),
    Query(String),
    /// Esc in the search field: empty it.
    ClearQuery,
    Sort(Sort),
    /// Notebooks as a list (else cards); kept in the settings.
    ListView(bool),
    /// The menu for a target at a window position (right click, ⋯).
    Menu(Target, Point),
    CloseMenu,
    Act(Target, Action),
    /// The dialog's text field (a name, a new folder, a new tag).
    Text(String),
    /// The move dialog: a folder picked; "Neuer Ordner"; Enter in its name field.
    To(String),
    NewFolder,
    MakeFolder,
    /// The tags dialog: a tag on / off for every notebook; the typed tag added.
    ToggleTag(String),
    AddTag,
    Cover(String),
    /// The dialog's main button (also Enter in its field).
    Confirm,
    CloseDialog,
    /// "Auswählen" on / off; a card ticked or not; every card shown (or none).
    Select(bool),
    Tick(String),
    SelectAll,
    Drag(String, Drag),
    /// A drop target: a dragged notebook came over it / left it.
    Over(Drop, bool),
}

fn sm(m: Msg) -> Message {
    Message::Shelf(m)
}

// --- folders, tags, filters ------------------------------------------------------------------

/// The folder a folder lies in ("" = top level).
pub fn parent(path: &str) -> &str {
    path.rsplit_once('/').map(|(p, _)| p).unwrap_or("")
}

/// A folder's own name (the last part of its path).
pub fn name(path: &str) -> &str {
    path.rsplit_once('/').map(|(_, n)| n).unwrap_or(path)
}

/// A folder's name as typed: a "/" would nest, so it becomes a space (like on the tablet).
pub fn folder_name(typed: &str) -> String {
    typed.replace('/', " ").trim().to_string()
}

/// A folder `name` inside `parent`, tidied like the core stores it.
pub fn join(parent: &str, name: &str) -> String {
    folder_path(&format!("{parent}/{name}"))
}

/// Whether `folder` is `path` or lies inside it (every folder lies in the top level "").
pub fn within(folder: &str, path: &str) -> bool {
    path.is_empty() || folder == path || folder.strip_prefix(path).is_some_and(|rest| rest.starts_with('/'))
}

/// `folder` after the folder `from` (with everything in it) moved to `to`; None if it isn't in
/// `from`. Renaming moves it next to itself, dissolving moves it to its parent.
pub fn moved(folder: &str, from: &str, to: &str) -> Option<String> {
    if from.is_empty() || !within(folder, from) {
        return None;
    }
    Some(folder_path(&format!("{to}/{}", &folder[from.len()..])))
}

/// A folder in the sidebar's tree: its notebooks are counted with its subfolders'.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRow {
    pub path: String,
    pub name: String,
    pub depth: usize,
    pub count: usize,
    pub children: bool,
}

/// The folder tree from the notebooks' folders and the empty ones of this session: every
/// folder with the ones above it, depth first, sorted by name (ignoring case).
pub fn folder_rows<'a>(folders: impl IntoIterator<Item = &'a str>, empty: impl IntoIterator<Item = &'a str>) -> Vec<FolderRow> {
    let mut count: HashMap<String, usize> = HashMap::new();
    let mut add = |f: &str, n: usize| {
        let f = folder_path(f);
        let mut at = 0;
        while !f.is_empty() {
            let end = f[at..].find('/').map(|i| at + i).unwrap_or(f.len());
            *count.entry(f[..end].to_string()).or_default() += n;
            if end == f.len() {
                break;
            }
            at = end + 1;
        }
    };
    for f in folders {
        add(f, 1);
    }
    for f in empty {
        add(f, 0);
    }
    let key = |p: &str| p.split('/').map(|part| (part.to_lowercase(), part.to_string())).collect::<Vec<_>>();
    let mut paths: Vec<(Vec<(String, String)>, String)> = count.keys().map(|p| (key(p), p.clone())).collect();
    paths.sort();
    let mut rows: Vec<FolderRow> =
        paths.into_iter().map(|(_, p)| FolderRow { name: name(&p).to_string(), depth: p.matches('/').count(), count: count[&p], children: false, path: p }).collect();
    for i in 0..rows.len() {
        rows[i].children = rows.get(i + 1).is_some_and(|next| parent(&next.path) == rows[i].path);
    }
    rows
}

/// The rows the sidebar shows: none inside a folded folder.
pub fn visible<'a>(rows: &'a [FolderRow], folded: &HashSet<String>) -> Vec<&'a FolderRow> {
    rows.iter().filter(|r| !folded.iter().any(|f| r.path != *f && within(&r.path, f))).collect()
}

/// Every tag in the library with how many notebooks carry it, sorted by name; tags that differ
/// only in case are one (the first spelling wins).
pub fn all_tags(entries: &[Entry]) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for t in entries.iter().flat_map(|e| e.tags.iter()) {
        match out.iter_mut().find(|(o, _)| o.to_lowercase() == t.to_lowercase()) {
            Some((_, n)) => *n += 1,
            None => out.push((t.clone(), 1)),
        }
    }
    out.sort_by(|a, b| (a.0.to_lowercase(), &a.0).cmp(&(b.0.to_lowercase(), &b.0)));
    out
}

pub fn has_tag(e: &Entry, tag: &str) -> bool {
    e.tags.iter().any(|t| t.to_lowercase() == tag.to_lowercase())
}

/// The tag colours (the pen's inks and a few more), the same as on the tablet.
pub const TAG_COLOURS: [u32; 8] = [0x1E5BD8, 0xD71921, 0x138A47, 0xE08A00, 0x7B3FD0, 0x0AA4B8, 0x8C6239, 0xAD1457];

/// A tag's colour: always the same for the same name, ignoring case (the tablet sums the
/// name's UTF-16 units, so does this).
pub fn tag_colour(tag: &str) -> Color {
    let sum = tag.to_lowercase().encode_utf16().fold(0u32, |s, c| s.wrapping_add(c as u32));
    let v = TAG_COLOURS[(sum % TAG_COLOURS.len() as u32) as usize];
    Color::from_rgb8((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// For the search: lower case, umlauts spelled out ("Übung" finds "uebung" and the other way).
fn plain(s: &str) -> String {
    s.to_lowercase().replace('ä', "ae").replace('ö', "oe").replace('ü', "ue").replace('ß', "ss")
}

/// Whether a notebook matches the search: every word in its title, folder or tags.
pub fn matches(e: &Entry, query: &str) -> bool {
    let q = plain(query.trim());
    let (title, folder) = (plain(&e.title), plain(&e.folder));
    let tags: Vec<String> = e.tags.iter().map(|t| plain(t)).collect();
    q.split(' ').filter(|w| !w.is_empty()).all(|w| title.contains(w) || folder.contains(w) || tags.iter().any(|t| t.contains(w)))
}

/// The notebooks to show, pinned ones first, then by `sort`: what is picked (in a folder only
/// what lies right in it), or, while something is typed into the search, whatever matches in
/// the whole library (like on the tablet).
pub fn shown<'a>(entries: &'a [Entry], pick: &Pick, query: &str, sort: Sort) -> Vec<&'a Entry> {
    let searching = !query.trim().is_empty();
    let mut out: Vec<&Entry> = entries
        .iter()
        .filter(|e| match pick {
            _ if searching => matches(e, query),
            Pick::Home | Pick::All => true,
            Pick::Pinned => e.pinned,
            Pick::Folder(f) => e.folder == *f,
            Pick::Tag(t) => has_tag(e, t),
        })
        .collect();
    out.sort_by(|a, b| {
        let by = match sort {
            Sort::Recent => b.modified.cmp(&a.modified),
            Sort::Name => std::cmp::Ordering::Equal,
        };
        b.pinned.cmp(&a.pinned).then(by).then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())).then_with(|| a.id.cmp(&b.id))
    });
    out
}

/// The folders right inside `path` ("" = the top level).
pub fn subfolders<'a>(rows: &'a [FolderRow], path: &str) -> Vec<&'a FolderRow> {
    rows.iter().filter(|r| parent(&r.path) == path).collect()
}

impl Shelf {
    /// Esc: closes what is open, one thing at a time (true: something was).
    pub fn escape(&mut self) -> bool {
        let closed = self.menu.take().is_some() || self.dialog.take().is_some() || self.drag.take().is_some() || self.selected.take().is_some();
        if closed {
            self.asked = None;
        }
        closed
    }
}

// --- ids (tests find the widgets by them) ----------------------------------------------------

pub fn search_id() -> iced::widget::Id {
    iced::widget::Id::new("n0te-library-search")
}

/// The text field of the open dialog.
pub fn field_id() -> iced::widget::Id {
    iced::widget::Id::new("n0te-library-field")
}

pub fn card_id(id: &str) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-card-{id}"))
}

/// A notebook in the start's "Zuletzt geöffnet".
pub fn recent_id(id: &str) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-recent-{id}"))
}

/// A sidebar row that isn't a folder or tag: "start", "pinned", "top" ("Ohne Ordner" while dragging).
pub fn side_id(what: &str) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-side-{what}"))
}

/// A part of the way at the top of a folder ("" = "Start").
pub fn crumb_id(path: &str) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-crumb-{path}"))
}

/// The "Neuer Ordner" tile on the start.
pub fn new_folder_id() -> iced::widget::Id {
    iced::widget::Id::new("n0te-new-folder")
}

/// A section of the start ("Zuletzt geöffnet", "Angeheftet", "Ordner", "Ohne Ordner").
pub fn section_id(title: &str) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-section-{title}"))
}

/// A folder's row in the sidebar ("" = "Alle Notizbücher").
pub fn folder_id(path: &str) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-folder-{path}"))
}

pub fn tile_id(path: &str) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-tile-{path}"))
}

pub fn tag_id(tag: &str) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-tag-{}", tag.to_lowercase()))
}

/// A tag's switch in the tags dialog.
pub fn tag_switch_id(tag: &str) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-tag-switch-{}", tag.to_lowercase()))
}

pub fn menu_id(a: Action) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-menu-{a:?}"))
}

/// A folder in the move dialog ("" = top level).
pub fn pick_id(path: &str) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-move-{path}"))
}

// --- views -----------------------------------------------------------------------------------

pub const WIDTH: f32 = 236.0;
const MENU_W: f32 = 232.0;
const MENU_ROW: f32 = 33.0;

/// The library's sidebar: all notebooks, the pinned ones, the folder tree, the tags.
pub fn sidebar(app: &App) -> Element<'_, Message> {
    let s = &app.shelf;
    let dragging = s.drag.as_ref();
    let over = |d: Drop| s.drag.as_ref().is_some_and(|g| g.over.as_ref() == Some(&d));
    let muted = theme::p().muted;
    // While searching the whole library nothing is picked.
    let picked = |p: &Pick| s.query.trim().is_empty() && s.pick == *p;
    let lead = |g: &'static [u8], on: bool| icon(g, 15.0, if on { theme::p().text } else { muted });
    let mut list = column![].spacing(1);
    let on = picked(&Pick::Home);
    list = list.push(container(side_row(lead(HOME, on), "Start", None, on, false, 0.0, Msg::Pick(Pick::Home))).id(side_id("start")));
    let on = picked(&Pick::All);
    let all = side_row(lead(GRID, on), "Alle Notizbücher", Some(app.entries.len()), on, over(Drop::Folder(String::new())), 0.0, Msg::Pick(Pick::All));
    list = list.push(container(drop_target(all, Some(Target::Folder(String::new())), Drop::Folder(String::new()), dragging.is_some())).id(folder_id("")));
    // While dragging: "Angeheftet" pins what is let go there, "Ohne Ordner" takes it out of its folder.
    let pinned = app.entries.iter().filter(|e| e.pinned).count();
    if pinned > 0 || dragging.is_some() {
        let on = picked(&Pick::Pinned);
        let b = side_row(lead(PIN, on), "Angeheftet", Some(pinned), on, over(Drop::Pin), 0.0, Msg::Pick(Pick::Pinned));
        list = list.push(container(drop_target(b, None, Drop::Pin, dragging.is_some())).id(side_id("pinned")));
    }
    if dragging.is_some_and(|d| d.ids.iter().any(|id| app.entries.iter().any(|e| e.id == *id && !e.folder.is_empty()))) {
        let b = side_row(lead(UP, false), "Ohne Ordner", None, false, over(Drop::Top), 0.0, Msg::Pick(Pick::Home));
        list = list.push(container(drop_target(b, None, Drop::Top, true)).id(side_id("top")));
    }
    // "+": a folder in the one open, else at the top.
    let inside = match &s.pick {
        Pick::Folder(f) => f.clone(),
        _ => String::new(),
    };
    let plus = button(text("+").size(15).line_height(1.0)).padding([1, 7]).on_press(sm(Msg::Act(Target::Folder(inside), Action::NewFolder))).style(theme::ghost);
    list = list.push(section("Ordner", Some(plus.into())));
    let rows = folder_rows(app.entries.iter().map(|e| e.folder.as_str()), s.empty.iter().map(String::as_str));
    if rows.is_empty() {
        list = list.push(container(text("Noch keine. Über + anlegen, dann Notizbücher hineinziehen.").size(12).color(muted)).padding(Padding { top: 2.0, bottom: 4.0, left: 10.0, right: 8.0 }));
    }
    for r in visible(&rows, &s.folded) {
        let on = picked(&Pick::Folder(r.path.clone()));
        let chevron: Element<'_, Message> = if r.children {
            let open = !s.folded.contains(&r.path);
            button(icon(if open { CHEVRON_DOWN } else { CHEVRON_RIGHT }, 12.0, muted))
                .padding(Padding { right: 4.0, ..Padding::ZERO })
                .width(16)
                .on_press(sm(Msg::Fold(r.path.clone())))
                .style(|_, _| button::Style::default())
                .into()
        } else {
            Space::new().width(16).into()
        };
        let lead = row![chevron, icon(FOLDER, 15.0, if on { theme::p().text } else { muted })].align_y(Alignment::Center).into();
        let b = side_row(lead, &r.name, Some(r.count), on, over(Drop::Folder(r.path.clone())), r.depth as f32 * 14.0, Msg::Pick(Pick::Folder(r.path.clone())));
        list = list.push(container(drop_target(b, Some(Target::Folder(r.path.clone())), Drop::Folder(r.path.clone()), dragging.is_some())).id(folder_id(&r.path)));
    }
    let tags = all_tags(&app.entries);
    if !tags.is_empty() {
        list = list.push(section("Tags", None));
        for (t, n) in tags {
            let on = s.query.trim().is_empty() && matches!(&s.pick, Pick::Tag(p) if p.to_lowercase() == t.to_lowercase());
            let b = side_row(container(dot(tag_colour(&t), 8.0)).center_x(15).into(), &t, Some(n), on, over(Drop::Tag(t.clone())), 0.0, Msg::Pick(Pick::Tag(t.clone())));
            list = list.push(container(drop_target(b, Some(Target::Tag(t.clone())), Drop::Tag(t.clone()), dragging.is_some())).id(tag_id(&t)));
        }
    }
    container(scrollable(list.padding(Padding { top: 12.0, right: 10.0, bottom: 16.0, left: 10.0 })).style(theme::scroll_dark).height(Length::Fill))
        .width(WIDTH)
        .height(Length::Fill)
        .style(theme::chrome)
        .into()
}

/// A sidebar row as a drop target (while something is dragged), with its right-click menu.
fn drop_target<'a>(b: button::Button<'a, Message>, menu: Option<Target>, drop: Drop, dragging: bool) -> Element<'a, Message> {
    let mut p = pointer(b);
    if let Some(menu) = menu {
        p = p.on_right(move |at| sm(Msg::Menu(menu.clone(), at)));
    }
    p.drop_target(dragging, sm(Msg::Over(drop.clone(), true)), sm(Msg::Over(drop, false))).into()
}

fn side_row<'a>(lead: Element<'a, Message>, label: &str, count: Option<usize>, selected: bool, over: bool, indent: f32, on: Msg) -> button::Button<'a, Message> {
    let content = row![
        lead,
        Space::new().width(9),
        text(fit(label, 22)).font(if selected { theme::SANS_MEDIUM } else { theme::SANS }).size(13.5).wrapping(text::Wrapping::None),
        Space::new().width(Length::Fill),
        text(count.map(|n| n.to_string()).unwrap_or_default()).font(theme::meta_font()).size(11).color(theme::p().muted),
    ]
    .align_y(Alignment::Center);
    button(content)
        .on_press(sm(on))
        .padding(Padding { top: 6.0, bottom: 6.0, left: 10.0 + indent, right: 10.0 })
        .width(Length::Fill)
        .style(theme::drop_row(selected, over))
}

fn section<'a>(label: &str, action: Option<Element<'a, Message>>) -> Element<'a, Message> {
    let mut r = row![theme::caption(label), Space::new().width(Length::Fill)].align_y(Alignment::Center);
    if let Some(a) = action {
        r = r.push(a);
    }
    container(r).padding(Padding { top: 16.0, bottom: 4.0, left: 10.0, right: 4.0 }).height(Length::Shrink).into()
}

/// Above the cards: the search, the order, "Auswählen"; while selecting the actions for every
/// notebook ticked instead.
pub fn toolbar(app: &App) -> Element<'_, Message> {
    let s = &app.shelf;
    let bar = if let Some(sel) = &s.selected {
        let ids: Vec<String> = app.entries.iter().filter(|e| sel.contains(&e.id)).map(|e| e.id.clone()).collect();
        let any = !ids.is_empty();
        let act = |a: Action| any.then(|| sm(Msg::Act(Target::Notebooks(ids.clone()), a)));
        let all_pinned = any && app.entries.iter().filter(|e| sel.contains(&e.id)).all(|e| e.pinned);
        let asked = any && s.asked == Some((Target::Notebooks(ids.clone()), Action::Trash));
        let n = ids.len();
        let shown_all = {
            let v = shown(&app.entries, &s.pick, &s.query, s.sort);
            !v.is_empty() && v.iter().all(|e| sel.contains(&e.id))
        };
        row![
            text(format!("{n} ausgewählt")).font(theme::title_font()).size(15).color(theme::p().text),
            Space::new().width(20),
            button(theme::btn("Verschieben")).padding(theme::pad()).on_press_maybe(act(Action::Move)).style(theme::secondary),
            button(theme::btn("Tags")).padding(theme::pad()).on_press_maybe(act(Action::Tags)).style(theme::secondary),
            button(theme::btn(if all_pinned { "Lösen" } else { "Anheften" })).padding(theme::pad()).on_press_maybe(act(Action::Pin)).style(theme::secondary),
            button(theme::btn(if asked { "Wirklich in den Papierkorb?" } else { "Papierkorb" })).padding(theme::pad()).on_press_maybe(act(Action::Trash)).style(if asked { theme::accent_button } else { theme::secondary }),
            Space::new().width(Length::Fill),
            button(theme::btn(if shown_all { "Keine" } else { "Alle" })).padding(theme::pad()).on_press(sm(Msg::SelectAll)).style(theme::ghost),
            button(theme::btn("Fertig")).padding(theme::pad()).on_press(sm(Msg::Select(false))).style(theme::accent_button),
        ]
        .spacing(8)
        .align_y(Alignment::Center)
    } else {
        let sort = |label: &str, by: Sort| button(theme::btn_small(label)).padding(theme::pad()).on_press(sm(Msg::Sort(by))).style(theme::tab(s.sort == by));
        let list = app.settings.lib_list;
        let view = |glyph: &'static [u8], on: bool, id: &'static str| {
            container(button(icon(glyph, 15.0, if on { theme::p().chrome } else { theme::p().muted })).padding([6, 9]).on_press(sm(Msg::ListView(id == "list"))).style(theme::tab(on))).id(iced::widget::Id::new(id))
        };
        row![
            search_field(&s.query),
            Space::new().width(Length::Fill),
            sort("Zuletzt geändert", Sort::Recent),
            sort("Name", Sort::Name),
            Space::new().width(6),
            view(GRID, !list, "grid"),
            view(LIST, list, "list"),
            Space::new().width(10),
            button(theme::btn("Auswählen")).padding(theme::pad()).on_press(sm(Msg::Select(true))).style(theme::secondary),
        ]
        .spacing(6)
        .align_y(Alignment::Center)
    };
    container(bar).padding(Padding { top: 12.0, bottom: 12.0, left: 32.0, right: 28.0 }).height(60).width(Length::Fill).style(theme::surface).into()
}

/// The search field: a loupe, what is typed, × to empty it (Esc does too; Ctrl+F gets there).
fn search_field(query: &str) -> Element<'_, Message> {
    let input = text_input("Notizbücher suchen …", query)
        .id(search_id())
        .on_input(|q| sm(Msg::Query(q)))
        .padding(Padding { top: 7.0, bottom: 7.0, left: 30.0, right: 28.0 })
        .size(13.5)
        .style(theme::field);
    let mut layers = stack![
        widgets::on_escape(input, sm(Msg::ClearQuery)),
        container(icon(LOUPE, 14.0, theme::p().muted)).padding(Padding { left: 10.0, ..Padding::ZERO }).center_y(Length::Fill),
    ]
    .width(Length::Fixed(320.0));
    if !query.is_empty() {
        let clear = button(text("×").size(14).line_height(1.0)).padding([2, 7]).on_press(sm(Msg::ClearQuery)).style(theme::ghost);
        layers = layers.push(container(clear).padding(Padding { right: 4.0, ..Padding::ZERO }).align_right(Length::Fill).center_y(Length::Fill));
    }
    layers.into()
}

/// The heading over the cards: where the library looks (a folder with the way to it), how
/// many notebooks.
pub fn header<'a>(app: &'a App, count: usize, folders: usize) -> Element<'a, Message> {
    let s = &app.shelf;
    let mut facts = format!("{count} {}", if count == 1 { "Notizbuch" } else { "Notizbücher" });
    if folders > 0 {
        facts += &format!(" · {folders} Ordner");
    }
    let facts = text(theme::cased(&facts)).font(theme::meta_font()).size(13).color(theme::p().muted);
    if !s.query.trim().is_empty() {
        return column![theme::heading("Suche", 30.0), facts].spacing(2).into();
    }
    match &s.pick {
        Pick::Home => column![theme::heading("Bibliothek", 30.0), facts].spacing(2).into(),
        Pick::All => column![theme::heading("Alle Notizbücher", 30.0), facts].spacing(2).into(),
        Pick::Pinned => column![theme::heading("Angeheftet", 30.0), facts].spacing(2).into(),
        Pick::Tag(t) => column![row![dot(tag_colour(t), 12.0), theme::heading(t, 30.0)].spacing(12).align_y(Alignment::Center), facts].spacing(2).into(),
        Pick::Folder(f) => {
            // "Start › Uni › WS26": every part but the last opens.
            let crumb = |label: String, path: String| {
                let to = if path.is_empty() { Pick::Home } else { Pick::Folder(path.clone()) };
                container(button(text(label).font(theme::meta_font()).size(12.5)).padding([2, 0]).on_press(sm(Msg::Pick(to))).style(theme::link)).id(crumb_id(&path))
            };
            let mut way = row![crumb("Start".into(), String::new())].spacing(7).align_y(Alignment::Center);
            let parts: Vec<&str> = f.split('/').collect();
            for (i, part) in parts.iter().enumerate() {
                way = way.push(text("›").size(12.5).color(theme::p().muted));
                if i + 1 == parts.len() {
                    way = way.push(text(part.to_string()).font(theme::meta_font()).size(12.5).color(theme::p().text));
                } else {
                    way = way.push(crumb(part.to_string(), parts[..=i].join("/")));
                }
            }
            column![way, theme::heading(name(f), 30.0), facts].spacing(4).into()
        }
    }
}

/// A subfolder before the cards: opens on a click, takes dropped cards, has the folder menu.
pub fn folder_tile<'a>(app: &'a App, r: &FolderRow) -> Element<'a, Message> {
    let over = app.shelf.drag.as_ref().is_some_and(|d| d.over == Some(Drop::Folder(r.path.clone())));
    let count = format!("{} {}", r.count, if r.count == 1 { "Notizbuch" } else { "Notizbücher" });
    let content = row![
        icon(FOLDER, 22.0, theme::p().accent),
        column![
            text(fit(&r.name, 17)).font(theme::title_font()).size(14.5).color(theme::p().text).wrapping(text::Wrapping::None),
            text(theme::cased(&count)).font(theme::meta_font()).size(if theme::s().label == theme::Label::MonoCaps { theme::LABEL } else { 12.0 }).color(theme::p().muted),
        ]
        .spacing(2),
    ]
    .spacing(12)
    .align_y(Alignment::Center);
    let b = button(content).padding([14, 14]).width(library::COVER.0 + 20.0).on_press(sm(Msg::Pick(Pick::Folder(r.path.clone())))).style(theme::card_ticked(over));
    container(drop_target(b, Some(Target::Folder(r.path.clone())), Drop::Folder(r.path.clone()), app.shelf.drag.is_some())).id(tile_id(&r.path)).into()
}

/// "Neuer Ordner" on the start, after the folders.
pub fn new_folder_tile<'a>() -> Element<'a, Message> {
    let content = row![icon(FOLDER_PLUS, 22.0, theme::p().muted), text("Neuer Ordner").font(theme::title_font()).size(14.5).color(theme::p().muted)].spacing(12).align_y(Alignment::Center);
    container(button(content).padding([20, 14]).width(library::COVER.0 + 20.0).on_press(sm(Msg::Act(Target::Folder(String::new()), Action::NewFolder))).style(theme::card_ticked(false))).id(new_folder_id()).into()
}

/// A section of the start: its caption, how many, a hairline.
pub fn section_head<'a>(title: &str, count: Option<usize>) -> Element<'a, Message> {
    let mut r = row![theme::caption(title)].align_y(Alignment::Center);
    if let Some(n) = count {
        r = r.push(Space::new().width(10));
        r = r.push(text(n.to_string()).font(theme::meta_font()).size(11.5).color(theme::alpha(theme::p().muted, 0.8)));
    }
    r = r.push(Space::new().width(14));
    r = r.push(container(Space::new()).height(1).width(Length::Fill).style(theme::hairline));
    container(r).id(section_id(title)).into()
}

/// Up to three tags under a card's title: a dot in the tag's colour and its name, shorter the
/// more there are; with more than three, two and how many more.
pub fn tag_chips<'a>(tags: &[String]) -> Element<'a, Message> {
    let n = if tags.len() > 3 { 2 } else { tags.len() };
    let budget = match (n, tags.len() > 3) {
        (1, _) => 20,
        (2, false) => 9,
        (2, true) => 8,
        _ => 6,
    };
    let mut r = row![].spacing(9).align_y(Alignment::Center);
    for t in tags.iter().take(n) {
        r = r.push(row![dot(tag_colour(t), 7.0), text(fit(t, budget)).font(theme::SANS).size(11.5).color(theme::p().muted).wrapping(text::Wrapping::None)].spacing(4).align_y(Alignment::Center));
    }
    if tags.len() > n {
        r = r.push(text(format!("+{}", tags.len() - n)).font(theme::meta_font()).size(11).color(theme::p().muted));
    }
    r.into()
}

/// The rows of a menu: (action, glyph, label, danger).
fn menu_rows(app: &App, target: &Target) -> Vec<(Action, &'static [u8], &'static str, bool)> {
    let asked = |a: Action| app.shelf.asked.as_ref().is_some_and(|(t, b)| t == target && *b == a);
    match target {
        Target::Folder(f) if f.is_empty() => vec![(Action::NewFolder, FOLDER_PLUS, "Neuer Ordner", false)],
        Target::Folder(_) => vec![
            (Action::Rename, PEN, "Umbenennen", false),
            (Action::NewFolder, FOLDER_PLUS, "Neuer Unterordner", false),
            (Action::Dissolve, DISSOLVE, if asked(Action::Dissolve) { "Wirklich auflösen?" } else { "Auflösen" }, true),
        ],
        Target::Tag(_) => vec![(Action::Rename, PEN, "Umbenennen", false), (Action::Remove, TAG, if asked(Action::Remove) { "Wirklich von allen entfernen?" } else { "Von allen entfernen" }, true)],
        Target::Notebooks(ids) => {
            let pinned = !ids.is_empty() && ids.iter().all(|id| app.entries.iter().any(|e| e.id == *id && e.pinned));
            let pin = if pinned { (Action::Pin, UNPIN, "Lösen", false) } else { (Action::Pin, PIN, "Anheften", false) };
            let trash = (Action::Trash, TRASH, if asked(Action::Trash) { "Wirklich in den Papierkorb?" } else { "In den Papierkorb" }, true);
            if ids.len() == 1 {
                vec![
                    (Action::Open, OPEN, "Öffnen", false),
                    (Action::Rename, PEN, "Umbenennen", false),
                    (Action::Move, MOVE, "Verschieben nach …", false),
                    (Action::Tags, TAG, "Tags …", false),
                    pin,
                    (Action::Cover, COVER, "Cover …", false),
                    trash,
                ]
            } else {
                vec![(Action::Move, MOVE, "Verschieben nach …", false), (Action::Tags, TAG, "Tags …", false), pin, trash]
            }
        }
    }
}

/// A row of a right-click menu: a glyph and a short label, nothing more (danger: in the
/// accent, after a hairline).
pub struct MenuRow {
    pub id: iced::widget::Id,
    pub glyph: &'static [u8],
    pub label: &'static str,
    pub danger: bool,
    pub on: Message,
}

fn menu_view(app: &App, m: &Menu) -> Vec<MenuRow> {
    menu_rows(app, &m.target)
        .into_iter()
        .map(|(a, glyph, label, danger)| MenuRow { id: menu_id(a), glyph, label, danger, on: sm(Msg::Act(m.target.clone(), a)) })
        .collect()
}

/// A menu's sheet and how tall it is.
fn menu_sheet<'a>(rows: Vec<MenuRow>) -> (Element<'a, Message>, f32) {
    let mut col = column![].spacing(1);
    let mut height = 10.0;
    for (i, r) in rows.into_iter().enumerate() {
        if r.danger && i > 0 {
            col = col.push(container(container(Space::new()).height(1).width(Length::Fill).style(theme::hairline)).padding([4, 6]));
            height += 9.0;
        }
        let content = row![icon(r.glyph, 15.0, if r.danger { theme::p().accent } else { theme::p().muted }), text(r.label).font(theme::SANS).size(13.5).wrapping(text::Wrapping::None)].spacing(11).align_y(Alignment::Center);
        col = col.push(container(button(content).padding([7, 10]).width(Length::Fill).on_press(r.on).style(theme::menu_row(r.danger))).id(r.id));
        height += MENU_ROW;
    }
    (container(col).padding(5).width(MENU_W).style(theme::menu).into(), height)
}

/// A right-click menu at a window position, over an invisible layer that closes it (`close`):
/// left of the pointer near the right edge, above it near the bottom.
pub fn float_menu<'a>(at: Point, rows: impl Fn() -> Vec<MenuRow> + 'a, close: Message) -> Element<'a, Message> {
    stack![
        widgets::dismiss(close),
        responsive(move |size| {
            let (sheet, h) = menu_sheet(rows());
            let x = at.x.min(size.width - MENU_W - 8.0).max(4.0);
            let y = if at.y + h > size.height - 8.0 { (at.y - h).max(4.0) } else { at.y };
            pin(sheet).x(x).y(y).into()
        }),
    ]
    .into()
}

/// A dialog over the dimmed screen; a click beside it sends `close`.
pub fn dialog_layer<'a>(panel: Element<'a, Message>, close: Message) -> Element<'a, Message> {
    stack![
        mouse_area(container(Space::new()).width(Length::Fill).height(Length::Fill).style(theme::scrim)).on_press(close),
        container(opaque(panel)).padding(Padding { top: 90.0, ..Padding::ZERO }).center_x(Length::Fill),
    ]
    .into()
}

/// The library over its dialog, its right-click menu and a dragged card: always the same
/// layers, so the cards keep their state (a drag under way) when one of them shows.
pub fn layers<'a>(app: &'a App, base: Element<'a, Message>) -> Element<'a, Message> {
    let s = &app.shelf;
    let dialog: Element<'a, Message> = match &s.dialog {
        Some(d) => dialog_view(app, d),
        None => Space::new().into(),
    };
    let menu: Element<'a, Message> = match &s.menu {
        Some(m) => float_menu(m.at, move || menu_view(app, m), sm(Msg::CloseMenu)),
        None => Space::new().into(),
    };
    let ghost: Element<'a, Message> = match &s.drag {
        Some(d) => {
            let label = match d.ids.as_slice() {
                [one] => app.entries.iter().find(|e| e.id == *one).map(|e| fit(&e.title, 28)).unwrap_or_default(),
                many => format!("{} Notizbücher", many.len()),
            };
            let chip = container(row![icon(MOVE, 14.0, theme::p().accent), text(label).font(theme::SANS_MEDIUM).size(13).color(theme::p().text)].spacing(8).align_y(Alignment::Center))
                .padding([7, 12])
                .style(theme::menu);
            pin(chip).x(d.at.x + 14.0).y(d.at.y + 10.0).into()
        }
        None => Space::new().into(),
    };
    stack![base, dialog, menu, ghost].into()
}

/// A dialog over the dimmed library; a click beside it closes it.
fn dialog_view<'a>(app: &'a App, d: &'a Dialog) -> Element<'a, Message> {
    let panel = match d {
        Dialog::Name { what, text } => name_dialog(app, what, text),
        Dialog::Move { ids, to, new_folder } => move_dialog(app, ids, to, new_folder.as_deref()),
        Dialog::Tags { ids, new_tag } => tags_dialog(app, ids, new_tag),
        Dialog::Cover { ids } => cover_dialog(app, ids),
    };
    dialog_layer(panel, sm(Msg::CloseDialog))
}

/// What a dialog is about: "„Analysis I“", "3 Notizbücher".
fn about(app: &App, ids: &[String]) -> String {
    match ids {
        [one] => app.entries.iter().find(|e| e.id == *one).map(|e| format!("„{}“", fit(&e.title, 40))).unwrap_or_default(),
        many => format!("{} Notizbücher", many.len()),
    }
}

/// A dialog's sheet: heading, what it is about, its body, its buttons.
pub fn frame<'a>(title: &str, sub: String, body: Element<'a, Message>, foot: Element<'a, Message>, width: f32) -> Element<'a, Message> {
    let mut head = column![theme::heading(title, 20.0)].spacing(4);
    if !sub.is_empty() {
        head = head.push(text(sub).size(13).color(theme::p().muted));
    }
    container(column![head, body, foot].spacing(18)).padding(22).width(Length::Fixed(width)).style(theme::panel).into()
}

fn field<'a>(placeholder: &str, value: &str, on_submit: Msg) -> Element<'a, Message> {
    widgets::on_escape(
        text_input(placeholder, value).id(field_id()).on_input(|t| sm(Msg::Text(t))).on_submit(sm(on_submit)).padding(10).size(15).style(theme::field),
        sm(Msg::CloseDialog),
    )
    .into()
}

fn foot<'a>(label: &str, ready: bool) -> Element<'a, Message> {
    row![
        Space::new().width(Length::Fill),
        button(theme::btn("Abbrechen")).padding(theme::pad()).on_press(sm(Msg::CloseDialog)).style(theme::ghost),
        button(theme::btn(label)).padding(theme::pad()).on_press_maybe(ready.then(|| sm(Msg::Confirm))).style(theme::accent_button),
    ]
    .spacing(8)
    .align_y(Alignment::Center)
    .into()
}

fn name_dialog<'a>(app: &'a App, what: &'a Naming, value: &'a str) -> Element<'a, Message> {
    let ready = match what {
        Naming::Folder(_) | Naming::NewFolder(_) => !folder_name(value).is_empty(),
        _ => !value.trim().is_empty(),
    };
    let (title, sub, placeholder, ok) = match what {
        Naming::Notebook(id) => ("Umbenennen", about(app, std::slice::from_ref(id)), "Titel", "Umbenennen"),
        Naming::Folder(f) => ("Ordner umbenennen", format!("„{f}“"), "Name", "Umbenennen"),
        Naming::Tag(t) => ("Tag umbenennen", format!("„{t}“, bei jedem Notizbuch"), "Name", "Umbenennen"),
        Naming::NewFolder(p) if p.is_empty() => ("Neuer Ordner", String::new(), "Name, z. B. Uni", "Anlegen"),
        Naming::NewFolder(p) => ("Neuer Unterordner", format!("in „{p}“"), "Name, z. B. WS26", "Anlegen"),
    };
    let mut body = column![field(placeholder, value, Msg::Confirm)].spacing(8);
    if matches!(what, Naming::NewFolder(_)) {
        body = body.push(text("Auch leer bleibt er auf diesem Laptop.").size(12).color(theme::p().muted));
    }
    frame(title, sub, body.into(), foot(ok, ready), 440.0)
}

fn move_dialog<'a>(app: &'a App, ids: &'a [String], to: &'a str, new_folder: Option<&'a str>) -> Element<'a, Message> {
    let rows = folder_rows(app.entries.iter().map(|e| e.folder.as_str()), app.shelf.empty.iter().map(String::as_str));
    let pick_row = |glyph: &'static [u8], label: String, path: String, depth: usize| {
        let on = path == to;
        let content = row![icon(glyph, 15.0, if on { theme::p().accent } else { theme::p().muted }), text(fit(&label, 36)).font(if on { theme::SANS_MEDIUM } else { theme::SANS }).size(13.5)].spacing(10).align_y(Alignment::Center);
        container(button(content).padding(Padding { top: 7.0, bottom: 7.0, left: 10.0 + depth as f32 * 18.0, right: 10.0 }).width(Length::Fill).on_press(sm(Msg::To(path.clone()))).style(theme::row(on))).id(pick_id(&path))
    };
    let mut list = column![pick_row(GRID, "Oberste Ebene".into(), String::new(), 0)].spacing(1);
    for r in &rows {
        list = list.push(pick_row(FOLDER, r.name.clone(), r.path.clone(), r.depth + 1));
    }
    let list = container(scrollable(list).style(theme::scroll_dark)).max_height(300.0);
    let new: Element<'a, Message> = match new_folder {
        Some(n) => {
            let ready = !folder_path(n).is_empty();
            column![
                row![
                    field("Name des neuen Ordners", n, Msg::MakeFolder),
                    button(theme::btn_small("Anlegen")).padding(theme::pad()).on_press_maybe(ready.then(|| sm(Msg::MakeFolder))).style(theme::secondary),
                ]
                .spacing(8)
                .align_y(Alignment::Center),
                text(if to.is_empty() { "ganz oben".to_string() } else { format!("in „{to}“") }).size(12).color(theme::p().muted),
            ]
            .spacing(6)
            .into()
        }
        None => button(row![icon(FOLDER_PLUS, 14.0, theme::p().text), theme::btn_small("Neuer Ordner")].spacing(8).align_y(Alignment::Center))
            .padding(theme::pad())
            .on_press(sm(Msg::NewFolder))
            .style(theme::ghost)
            .into(),
    };
    frame("Verschieben nach", about(app, ids), column![list, new].spacing(12).into(), foot("Verschieben", true), 460.0)
}

fn tags_dialog<'a>(app: &'a App, ids: &'a [String], new_tag: &'a str) -> Element<'a, Message> {
    let targets: Vec<&Entry> = app.entries.iter().filter(|e| ids.contains(&e.id)).collect();
    let mut chips = Vec::new();
    for (t, _) in all_tags(&app.entries) {
        let on = !targets.is_empty() && targets.iter().all(|e| has_tag(e, &t));
        let content = row![dot(tag_colour(&t), 8.0), text(t.clone()).font(theme::SANS_MEDIUM).size(13)].spacing(7).align_y(Alignment::Center);
        chips.push(container(button(content).padding([6, 12]).on_press(sm(Msg::ToggleTag(t.clone()))).style(theme::tab(on))).id(tag_switch_id(&t)).into());
    }
    let mut body = column![].spacing(14);
    if chips.is_empty() {
        body = body.push(text("Noch keine Tags.").size(13).color(theme::p().muted));
    } else {
        body = body.push(iced::widget::Row::with_children(chips).spacing(8).wrap().vertical_spacing(8));
    }
    let ready = !new_tag.trim().is_empty();
    body = body.push(
        row![
            field("Neuer Tag, z. B. Prüfung", new_tag, Msg::AddTag),
            button(theme::btn_small("Hinzufügen")).padding(theme::pad()).on_press_maybe(ready.then(|| sm(Msg::AddTag))).style(theme::secondary),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    let done = row![Space::new().width(Length::Fill), button(theme::btn("Fertig")).padding(theme::pad()).on_press(sm(Msg::Confirm)).style(theme::accent_button)];
    frame("Tags", about(app, ids), body.into(), done.into(), 480.0)
}

/// The covers to choose from (the tablet's, in its order).
pub const COVERS: [(&str, &str); 10] =
    [("page", "Erste Seite"), ("initial", "Initiale"), ("dots", "Punkte"), ("stripes", "Streifen"), ("black", "Schwarz"), ("red", "Rot"), ("blue", "Blau"), ("green", "Grün"), ("orange", "Orange"), ("purple", "Lila")];

fn cover_dialog<'a>(app: &'a App, ids: &'a [String]) -> Element<'a, Message> {
    let first = app.entries.iter().find(|e| ids.first() == Some(&e.id));
    let current = first.map(|e| if e.cover.is_empty() { "page" } else { e.cover.as_str() }).unwrap_or("page");
    let title = first.map(|e| e.title.clone()).unwrap_or_default();
    let mut tiles = Vec::new();
    for (kind, label) in COVERS {
        let content = column![library::cover_preview(kind, &title, 60.0, 78.0), text(label).size(12).color(theme::p().text)].spacing(6).align_x(Alignment::Center);
        tiles.push(button(content).padding(8).width(84).on_press(sm(Msg::Cover(kind.to_string()))).style(theme::option(current == kind)).into());
    }
    let body = iced::widget::Row::with_children(tiles).spacing(8).wrap().vertical_spacing(8);
    let close = row![Space::new().width(Length::Fill), button(theme::btn("Schließen")).padding(theme::pad()).on_press(sm(Msg::CloseDialog)).style(theme::ghost)];
    frame("Cover", about(app, ids), body.into(), close.into(), 504.0)
}

/// A small round mark in a colour (tags).
pub fn dot<'a>(c: Color, size: f32) -> Element<'a, Message> {
    container(Space::new().width(size).height(size))
        .style(move |_| container::Style { background: Some(c.into()), border: iced::Border { radius: (size / 2.0).into(), ..Default::default() }, ..Default::default() })
        .into()
}

pub fn icon<'a>(glyph: &'static [u8], size: f32, color: Color) -> Element<'a, Message> {
    svg(svg::Handle::from_memory(glyph)).width(size).height(size).style(move |_, _| svg::Style { color: Some(color) }).into()
}

// Line glyphs on a 24 × 24 grid, drawn like the tablet's.
macro_rules! glyph {
    ($d:literal) => {
        concat!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d=""##,
            $d,
            r##""/></svg>"##
        )
        .as_bytes()
    };
}

pub const GRID: &[u8] = glyph!("M4.5 4.5h6v6h-6ZM13.5 4.5h6v6h-6ZM4.5 13.5h6v6h-6ZM13.5 13.5h6v6h-6Z");
pub const FOLDER: &[u8] = glyph!("M3.5 6.5a1 1 0 0 1 1-1H10l2 2.5h7.5a1 1 0 0 1 1 1v9.5a1 1 0 0 1-1 1h-15a1 1 0 0 1-1-1Z");
const FOLDER_PLUS: &[u8] = glyph!("M3.5 6.5a1 1 0 0 1 1-1H10l2 2.5h7.5a1 1 0 0 1 1 1v9.5a1 1 0 0 1-1 1h-15a1 1 0 0 1-1-1ZM12 11v5.5M9.25 13.75h5.5");
const DISSOLVE: &[u8] = glyph!("M3.5 6.5a1 1 0 0 1 1-1H10l2 2.5h7.5a1 1 0 0 1 1 1v9.5a1 1 0 0 1-1 1h-15a1 1 0 0 1-1-1ZM12 16.5V11M9.5 13.5 12 11l2.5 2.5");
const MOVE: &[u8] = glyph!("M3.5 6.5a1 1 0 0 1 1-1H10l2 2.5h7.5a1 1 0 0 1 1 1v9.5a1 1 0 0 1-1 1h-15a1 1 0 0 1-1-1ZM8.5 13.75h7M13 11.25l2.5 2.5-2.5 2.5");
pub const PIN: &[u8] = glyph!("M9 3.5h6M10 3.5v6l-3.5 4h11L14 9.5v-6M12 13.5V21");
const UNPIN: &[u8] = glyph!("M9 3.5h6M10 3.5v6l-3.5 4h11L14 9.5v-6M12 13.5V21M3.5 3.5l17 17");
const TAG: &[u8] = glyph!("M3.5 12.2V4.5a1 1 0 0 1 1-1h7.7l8.3 8.3-8.7 8.7ZM8 8h.01");
pub const PEN: &[u8] = glyph!("M5 19 6 15 16 5 19 8 9 18 5 19M14 7 17 10");
const COVER: &[u8] = glyph!("M5 3.5h12.5a1 1 0 0 1 1 1v15a1 1 0 0 1-1 1H5ZM8.5 3.5v17");
pub const TRASH: &[u8] = glyph!("M4 7h16M9.5 7V4.5h5V7M6 7l1 12.5h10L18 7M10 11v5M14 11v5");
const OPEN: &[u8] = glyph!("M13.5 4.5h6v6M19.5 4.5 11 13M17.5 14v5a1 1 0 0 1-1 1h-11a1 1 0 0 1-1-1V8a1 1 0 0 1 1-1h5");
const LOUPE: &[u8] = glyph!("M15.3 15.3 20.5 20.5M17 10.5a6.5 6.5 0 1 1-13 0 6.5 6.5 0 0 1 13 0Z");
pub const CHECK: &[u8] = glyph!("M5.5 12.5 10 17 18.5 7.5");
const HOME: &[u8] = glyph!("M4 11 12 4.5l8 6.5M6 9.5V19.5h4.5v-5h3v5H18V9.5");
const UP: &[u8] = glyph!("M12 19V5.5M6.5 11 12 5.5l5.5 5.5");
pub const LIST: &[u8] = glyph!("M8.5 6h11M8.5 12h11M8.5 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01");
/// A bookmark ribbon: a marked page.
pub const RIBBON: &[u8] = glyph!("M7 3.5h10v17l-5-4-5 4Z");
pub const PLUS: &[u8] = glyph!("M12 5v14M5 12h14");
/// Folding all groups: up (closed) and down (open).
pub const FOLD_UP: &[u8] = glyph!("M7 14.5l5-5 5 5");
pub const FOLD_DOWN: &[u8] = glyph!("M7 9.5l5 5 5-5");
/// A group (a stack of pages).
pub const GROUP: &[u8] = glyph!("M5 8.5h14v11H5ZM7.5 5.5h9");
const CHEVRON_DOWN: &[u8] = glyph!("M6.5 9.5 12 15l5.5-5.5");
const CHEVRON_RIGHT: &[u8] = glyph!("M9.5 6.5 15 12l-5.5 5.5");

// --- what the library does -------------------------------------------------------------------

impl App {
    pub(crate) fn on_shelf(&mut self, m: Msg) -> Task<Message> {
        match m {
            Msg::Pick(p) => {
                self.shelf.pick = p;
                self.shelf.query.clear();
                self.shelf.menu = None;
            }
            Msg::Fold(path) => {
                if !self.shelf.folded.remove(&path) {
                    self.shelf.folded.insert(path);
                }
            }
            Msg::Query(q) => self.shelf.query = q,
            Msg::ClearQuery => self.shelf.query.clear(),
            Msg::Sort(s) => self.shelf.sort = s,
            Msg::ListView(on) => {
                self.settings.lib_list = on;
                if self.persist_settings {
                    self.settings.save();
                }
            }
            Msg::Menu(target, at) => {
                if self.shelf.drag.is_none() {
                    self.shelf.menu = Some(Menu { target, at });
                    self.shelf.asked = None;
                }
            }
            Msg::CloseMenu => {
                self.shelf.menu = None;
                self.shelf.asked = None;
            }
            Msg::Act(target, action) => return self.act(target, action),
            Msg::Text(t) => match &mut self.shelf.dialog {
                Some(Dialog::Name { text, .. }) => *text = t,
                Some(Dialog::Move { new_folder: Some(n), .. }) => *n = t,
                Some(Dialog::Tags { new_tag, .. }) => *new_tag = t,
                _ => {}
            },
            Msg::To(path) => {
                if let Some(Dialog::Move { to, .. }) = &mut self.shelf.dialog {
                    *to = path;
                }
            }
            Msg::NewFolder => {
                if let Some(Dialog::Move { new_folder, .. }) = &mut self.shelf.dialog {
                    *new_folder = Some(String::new());
                    return iced::widget::operation::focus(field_id());
                }
            }
            Msg::MakeFolder => {
                if let Some(Dialog::Move { to, new_folder, .. }) = &mut self.shelf.dialog
                    && let Some(n) = new_folder.as_deref()
                {
                    let path = join(to, &folder_name(n));
                    if path.is_empty() || path == *to {
                        return Task::none();
                    }
                    self.shelf.folded.remove(to.as_str());
                    self.shelf.empty.insert(path.clone());
                    self.shelf.made.insert(path.clone());
                    *to = path;
                    *new_folder = None;
                    self.keep_made();
                }
            }
            Msg::ToggleTag(tag) => {
                let Some(Dialog::Tags { ids, .. }) = &self.shelf.dialog else { return Task::none() };
                let ids = ids.clone();
                let all = ids.iter().all(|id| self.entries.iter().any(|e| e.id == *id && has_tag(e, &tag)));
                self.organize(&ids, |nb| {
                    let mut tags = nb.tags();
                    if all {
                        tags.retain(|t| t.to_lowercase() != tag.to_lowercase());
                    } else {
                        tags.push(tag.clone());
                    }
                    nb.set_tags(&tags)
                });
                self.tidy();
            }
            Msg::AddTag => self.add_tag(),
            Msg::Cover(kind) => {
                if let Some(Dialog::Cover { ids }) = self.shelf.dialog.take() {
                    self.organize(&ids, |nb| nb.set_cover(&kind));
                }
            }
            Msg::Confirm => return self.confirm(),
            Msg::CloseDialog => self.shelf.dialog = None,
            Msg::Select(on) => {
                self.shelf.selected = on.then(BTreeSet::new);
                self.shelf.asked = None;
                self.shelf.menu = None;
            }
            Msg::Tick(id) => {
                if let Some(sel) = &mut self.shelf.selected
                    && !sel.remove(&id)
                {
                    sel.insert(id);
                }
                self.shelf.asked = None;
            }
            Msg::SelectAll => {
                let ids: Vec<String> = shown(&self.entries, &self.shelf.pick, &self.shelf.query, self.shelf.sort).iter().map(|e| e.id.clone()).collect();
                if let Some(sel) = &mut self.shelf.selected {
                    if !ids.is_empty() && ids.iter().all(|id| sel.contains(id)) {
                        sel.clear();
                    } else {
                        sel.extend(ids);
                    }
                }
                self.shelf.asked = None;
            }
            Msg::Drag(id, d) => match d {
                Drag::Start(at) => {
                    if self.shelf.menu.is_none() && self.shelf.dialog.is_none() {
                        // A ticked card takes the other ticked ones along.
                        let ids = match &self.shelf.selected {
                            Some(sel) if sel.contains(&id) => self.entries.iter().filter(|e| sel.contains(&e.id)).map(|e| e.id.clone()).collect(),
                            _ => vec![id],
                        };
                        self.shelf.drag = Some(Dragging { ids, at, over: None });
                    }
                }
                Drag::Move(at) => {
                    if let Some(d) = &mut self.shelf.drag {
                        d.at = at;
                    }
                }
                Drag::End => {
                    if let Some(Dragging { ids, over: Some(to), .. }) = self.shelf.drag.take() {
                        self.drop_on(&ids, to);
                    }
                }
            },
            Msg::Over(drop, on) => {
                if let Some(d) = &mut self.shelf.drag {
                    if on {
                        d.over = Some(drop);
                    } else if d.over.as_ref() == Some(&drop) {
                        d.over = None;
                    }
                }
            }
        }
        Task::none()
    }

    /// A menu row or a button of "Auswählen".
    fn act(&mut self, target: Target, action: Action) -> Task<Message> {
        // Trash, dissolve, a tag off everywhere: the first click asks, the second acts.
        if matches!(action, Action::Trash | Action::Dissolve | Action::Remove) {
            let ask = (target.clone(), action);
            if self.shelf.asked.as_ref() != Some(&ask) {
                self.shelf.asked = Some(ask);
                return Task::none();
            }
        }
        self.shelf.menu = None;
        self.shelf.asked = None;
        let entry = |app: &App, id: &str| app.entries.iter().find(|e| e.id == id).cloned();
        match (target, action) {
            (Target::Notebooks(ids), Action::Open) => {
                if let Some(id) = ids.into_iter().next() {
                    return Task::done(Message::Open(id));
                }
            }
            (Target::Notebooks(ids), Action::Rename) => {
                if let Some(e) = ids.first().and_then(|id| entry(self, id)) {
                    self.shelf.dialog = Some(Dialog::Name { what: Naming::Notebook(e.id), text: e.title });
                    return focus_all();
                }
            }
            (Target::Notebooks(ids), Action::Move) if !ids.is_empty() => {
                // Where they are: the folder open, else the one they share.
                let to = match &self.shelf.pick {
                    Pick::Folder(f) => f.clone(),
                    _ => {
                        let folders: HashSet<String> = ids.iter().filter_map(|id| entry(self, id)).map(|e| e.folder).collect();
                        if folders.len() == 1 { folders.into_iter().next().unwrap_or_default() } else { String::new() }
                    }
                };
                self.shelf.dialog = Some(Dialog::Move { ids, to, new_folder: None });
            }
            (Target::Notebooks(ids), Action::Tags) if !ids.is_empty() => {
                self.shelf.dialog = Some(Dialog::Tags { ids, new_tag: String::new() });
                return iced::widget::operation::focus(field_id());
            }
            (Target::Notebooks(ids), Action::Pin) => {
                let pin = !ids.iter().all(|id| entry(self, id).is_some_and(|e| e.pinned));
                self.organize(&ids, |nb| nb.set_pinned(pin));
                self.tidy();
            }
            (Target::Notebooks(ids), Action::Cover) if !ids.is_empty() => self.shelf.dialog = Some(Dialog::Cover { ids }),
            (Target::Notebooks(ids), Action::Trash) if !ids.is_empty() => {
                let before = self.folders_now();
                let what = about(self, &ids);
                let mut gone = 0;
                for id in &ids {
                    match self.lib.trash_notebook(id) {
                        Ok(()) => gone += 1,
                        Err(e) => self.say(format!("Nicht in den Papierkorb gelegt: {e}")),
                    }
                }
                self.refresh();
                self.keep_emptied(before);
                if let Some(sel) = &mut self.shelf.selected {
                    sel.retain(|id| !ids.contains(id));
                }
                if gone > 0 {
                    self.say(format!("{what} {} im Papierkorb", if gone == 1 { "liegt" } else { "liegen" }));
                }
                self.tidy();
            }
            (Target::Folder(f), Action::Rename) if !f.is_empty() => {
                self.shelf.dialog = Some(Dialog::Name { text: name(&f).to_string(), what: Naming::Folder(f) });
                return focus_all();
            }
            (Target::Folder(f), Action::NewFolder) => {
                self.shelf.dialog = Some(Dialog::Name { what: Naming::NewFolder(f), text: String::new() });
                return iced::widget::operation::focus(field_id());
            }
            (Target::Folder(f), Action::Dissolve) if !f.is_empty() => {
                let up = parent(&f).to_string();
                self.move_folder(&f, &up);
                self.say(format!("Ordner „{}“ aufgelöst", name(&f)));
            }
            (Target::Tag(t), Action::Rename) => {
                self.shelf.dialog = Some(Dialog::Name { text: t.clone(), what: Naming::Tag(t) });
                return focus_all();
            }
            (Target::Tag(t), Action::Remove) => {
                let ids: Vec<String> = self.entries.iter().filter(|e| has_tag(e, &t)).map(|e| e.id.clone()).collect();
                let n = self.organize(&ids, |nb| nb.set_tags(&nb.tags().into_iter().filter(|x| x.to_lowercase() != t.to_lowercase()).collect::<Vec<_>>()));
                self.tidy();
                self.say(format!("Tag „{t}“ von {n} {} entfernt", if n == 1 { "Notizbuch" } else { "Notizbüchern" }));
            }
            _ => {}
        }
        Task::none()
    }

    /// The dialog's main button (or Enter in its field).
    fn confirm(&mut self) -> Task<Message> {
        match self.shelf.dialog.clone() {
            Some(Dialog::Name { what, text }) => {
                let value = text.trim().to_string();
                match what {
                    Naming::Notebook(id) if !value.is_empty() => {
                        self.organize(&[id], |nb| nb.set_title(&value));
                    }
                    Naming::Folder(f) if !folder_name(&value).is_empty() => {
                        let to = join(parent(&f), &folder_name(&value));
                        if to != f {
                            self.move_folder(&f, &to);
                        }
                    }
                    // A new folder opens (empty, the settings keep it).
                    Naming::NewFolder(p) if !folder_name(&value).is_empty() => {
                        let path = join(&p, &folder_name(&value));
                        self.shelf.folded.remove(&p);
                        self.shelf.empty.insert(path.clone());
                        self.shelf.made.insert(path.clone());
                        self.keep_made();
                        self.shelf.pick = Pick::Folder(path);
                        self.shelf.query.clear();
                    }
                    Naming::Tag(old) if !value.is_empty() => {
                        if value != old {
                            let ids: Vec<String> = self.entries.iter().filter(|e| has_tag(e, &old)).map(|e| e.id.clone()).collect();
                            let to = value.clone();
                            self.organize(&ids, |nb| nb.set_tags(&nb.tags().into_iter().map(|x| if x.to_lowercase() == old.to_lowercase() { to.clone() } else { x }).collect::<Vec<_>>()));
                            if matches!(&self.shelf.pick, Pick::Tag(p) if p.to_lowercase() == old.to_lowercase()) {
                                self.shelf.pick = Pick::Tag(value);
                            }
                        }
                    }
                    _ => return Task::none(),
                }
                self.shelf.dialog = None;
            }
            Some(Dialog::Move { ids, to, new_folder }) => {
                // A name typed but not taken yet still counts.
                let to = match new_folder.as_deref().map(|n| join(&to, &folder_name(n))) {
                    Some(made) if !made.is_empty() && made != to => made,
                    _ => to,
                };
                self.shelf.dialog = None;
                self.move_to(&ids, &to);
                if self.shelf.selected.is_some() {
                    self.shelf.selected = None;
                }
            }
            Some(Dialog::Tags { .. }) => {
                self.add_tag();
                self.shelf.dialog = None;
            }
            Some(Dialog::Cover { .. }) | None => self.shelf.dialog = None,
        }
        Task::none()
    }

    /// The tag typed in the tags dialog, for every notebook in it.
    fn add_tag(&mut self) {
        let Some(Dialog::Tags { ids, new_tag }) = &mut self.shelf.dialog else { return };
        let tag = std::mem::take(new_tag).trim().to_string();
        if tag.is_empty() {
            return;
        }
        let ids = ids.clone();
        self.organize(&ids, |nb| {
            let mut tags = nb.tags();
            tags.push(tag.clone());
            nb.set_tags(&tags)
        });
    }

    /// Moves notebooks into a folder ("" = top level). The folders they leave stay in the
    /// sidebar for now, even when nothing is left in them.
    pub(crate) fn move_to(&mut self, ids: &[String], to: &str) {
        let to = folder_path(to);
        let ids: Vec<String> = ids.iter().filter(|id| self.entries.iter().any(|e| e.id == **id && e.folder != to)).cloned().collect();
        if ids.is_empty() {
            return;
        }
        let what = about(self, &ids);
        let before = self.folders_now();
        let moved = self.organize(&ids, |nb| nb.set_folder(&to));
        self.keep_emptied(before);
        self.keep_made();
        if moved > 0 {
            let verb = if moved == 1 { "liegt" } else { "liegen" };
            self.say(if to.is_empty() { format!("{what} {verb} jetzt ganz oben") } else { format!("{what} {verb} jetzt in {to}") });
        }
    }

    /// Renames or moves a folder with everything in it (dissolving moves it to its parent):
    /// every notebook inside gets the new path, the sidebar's folds, the empty folders and the
    /// folder in view follow.
    pub(crate) fn move_folder(&mut self, from: &str, to: &str) {
        let changes: Vec<(String, String)> = self.entries.iter().filter_map(|e| moved(&e.folder, from, to).map(|f| (e.id.clone(), f))).collect();
        for (id, f) in &changes {
            self.organize_one(id, |nb| nb.set_folder(f));
        }
        let follow = |f: String| Some(moved(&f, from, to).unwrap_or(f)).filter(|f| !f.is_empty());
        self.shelf.empty = std::mem::take(&mut self.shelf.empty).into_iter().filter_map(follow).collect();
        self.shelf.made = std::mem::take(&mut self.shelf.made).into_iter().filter_map(follow).collect();
        self.shelf.folded = std::mem::take(&mut self.shelf.folded).into_iter().filter_map(follow).collect();
        if let Pick::Folder(f) = &self.shelf.pick
            && let Some(n) = moved(f, from, to)
        {
            self.shelf.pick = if n.is_empty() { Pick::Home } else { Pick::Folder(n) };
        }
        self.refresh();
        self.keep_made();
    }

    /// Dragged notebooks let go on a sidebar row: into a folder (or none), pinned, a tag on.
    fn drop_on(&mut self, ids: &[String], to: Drop) {
        match to {
            Drop::Folder(f) => self.move_to(ids, &f),
            Drop::Top => self.move_to(ids, ""),
            Drop::Pin => {
                let ids: Vec<String> = ids.iter().filter(|id| self.entries.iter().any(|e| e.id == **id && !e.pinned)).cloned().collect();
                let n = self.organize(&ids, |nb| nb.set_pinned(true));
                if n > 0 {
                    let what = about(self, &ids);
                    self.say(format!("{what} angeheftet"));
                }
            }
            Drop::Tag(t) => {
                let ids: Vec<String> = ids.iter().filter(|id| self.entries.iter().any(|e| e.id == **id && !has_tag(e, &t))).cloned().collect();
                let n = self.organize(&ids, |nb| {
                    let mut tags = nb.tags();
                    tags.push(t.clone());
                    nb.set_tags(&tags)
                });
                if n > 0 {
                    let what = about(self, &ids);
                    self.say(format!("{what}: Tag „{t}“"));
                }
            }
        }
        if let Some(sel) = &mut self.shelf.selected {
            sel.clear();
        }
    }

    /// Folders made here that a notebook lies in now need no keeping any more; the rest go
    /// into the settings (only the real app writes them).
    pub(crate) fn keep_made(&mut self) {
        let used: HashSet<&str> = self.entries.iter().map(|e| e.folder.as_str()).collect();
        self.shelf.made.retain(|f| !used.contains(f.as_str()));
        let list: Vec<String> = self.shelf.made.iter().cloned().collect();
        if list != self.settings.lib_folders {
            self.settings.lib_folders = list;
            if self.persist_settings {
                self.settings.save();
            }
        }
    }

    /// Changes notebooks in their own documents and saves them; the list is read again.
    /// Returns how many changed. Organising isn't writing: "Zuletzt geändert" stays as it was.
    pub(crate) fn organize(&mut self, ids: &[String], f: impl Fn(&Notebook) -> n0te_core::Result<()>) -> usize {
        let done = ids.iter().filter(|id| self.organize_one(id, &f)).count();
        self.refresh();
        done
    }

    fn organize_one(&mut self, id: &str, f: impl Fn(&Notebook) -> n0te_core::Result<()>) -> bool {
        let dir = self.lib.root().join("notebooks").join(id);
        let before = std::fs::metadata(&dir).and_then(|m| m.modified()).ok();
        let lib = self.lib.clone();
        match lib.load_notebook(id).and_then(|nb| {
            f(&nb)?;
            lib.save_notebook(&nb)
        }) {
            Ok(()) => {
                if let Some(t) = before {
                    let _ = std::fs::File::open(&dir).and_then(|d| d.set_modified(t));
                }
                true
            }
            Err(e) => {
                self.say(format!("Nicht geändert: {e}"));
                false
            }
        }
    }

    /// The folders notebooks lie in now (before some leave theirs)…
    fn folders_now(&self) -> Vec<String> {
        self.entries.iter().map(|e| e.folder.clone()).filter(|f| !f.is_empty()).collect()
    }

    /// …and the ones of them that went empty stay in the sidebar (for this session).
    fn keep_emptied(&mut self, before: Vec<String>) {
        let now: HashSet<&str> = self.entries.iter().map(|e| e.folder.as_str()).collect();
        for f in before {
            if !now.contains(f.as_str()) {
                self.shelf.empty.insert(f);
            }
        }
    }

    /// A tag nobody carries any more, or "Angeheftet" with nothing pinned: back to all.
    fn tidy(&mut self) {
        let gone = match &self.shelf.pick {
            Pick::Tag(t) => !self.entries.iter().any(|e| has_tag(e, t)),
            Pick::Pinned => !self.entries.iter().any(|e| e.pinned),
            _ => false,
        };
        if gone {
            self.shelf.pick = Pick::Home;
        }
    }
}

/// Focuses the dialog's field with its text selected (typing replaces it).
fn focus_all() -> Task<Message> {
    iced::widget::operation::focus(field_id()).chain(iced::widget::operation::select_all(field_id()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str, title: &str, folder: &str, tags: &[&str], pinned: bool, age: u64) -> Entry {
        Entry {
            id: id.into(),
            title: title.into(),
            folder: folder.into(),
            sheets: 1,
            cover: String::new(),
            modified: Some(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000 - age)),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            pinned,
        }
    }

    fn library() -> Vec<Entry> {
        vec![
            entry("a", "Analysis I", "Uni/WS26", &["Mathe", "Prüfung"], false, 50),
            entry("b", "Lineare Algebra", "Uni/WS26", &["mathe"], true, 10),
            entry("c", "Physik", "Uni/SS25/Labor", &[], false, 5),
            entry("d", "Ideen", "", &["Privat"], false, 1),
            entry("e", "Übungsblätter", "Uni", &[], false, 30),
        ]
    }

    #[test]
    fn the_folder_tree_holds_every_folder_with_the_ones_above_depth_first() {
        let lib = library();
        let empty = ["Arbeit".to_string(), "uni/Archiv".to_string()];
        let rows = folder_rows(lib.iter().map(|e| e.folder.as_str()), empty.iter().map(String::as_str));
        let got: Vec<(&str, usize, usize, bool)> = rows.iter().map(|r| (r.path.as_str(), r.depth, r.count, r.children)).collect();
        assert_eq!(
            got,
            vec![
                ("Arbeit", 0, 0, false),
                ("Uni", 0, 4, true),
                ("Uni/SS25", 1, 1, true),
                ("Uni/SS25/Labor", 2, 1, false),
                ("Uni/WS26", 1, 2, false),
                ("uni", 0, 0, true),
                ("uni/Archiv", 1, 0, false),
            ]
        );
        assert_eq!(rows[2].name, "SS25");
        // Folding a folder hides what is inside it, not itself.
        let folded: HashSet<String> = ["Uni".to_string()].into();
        let shown: Vec<&str> = visible(&rows, &folded).iter().map(|r| r.path.as_str()).collect();
        assert_eq!(shown, vec!["Arbeit", "Uni", "uni", "uni/Archiv"]);
        assert_eq!(subfolders(&rows, "Uni").iter().map(|r| r.name.as_str()).collect::<Vec<_>>(), vec!["SS25", "WS26"]);
        // Messy paths are tidied like the core stores them.
        let rows = folder_rows([" Uni / WS26/ "], []);
        assert_eq!(rows.iter().map(|r| r.path.as_str()).collect::<Vec<_>>(), vec!["Uni", "Uni/WS26"]);
    }

    #[test]
    fn notebooks_are_shown_by_folder_tag_search_and_pin_pinned_first() {
        let lib = library();
        let ids = |v: Vec<&Entry>| v.iter().map(|e| e.id.as_str()).collect::<Vec<_>>().join("");
        assert_eq!(ids(shown(&lib, &Pick::All, "", Sort::Recent)), "bdcea", "pinned first, then newest");
        assert_eq!(ids(shown(&lib, &Pick::All, "", Sort::Name)), "badce", "pinned first, then by name");
        assert_eq!(ids(shown(&lib, &Pick::Pinned, "", Sort::Recent)), "b");
        assert_eq!(ids(shown(&lib, &Pick::Folder("Uni".into()), "", Sort::Recent)), "e", "only what lies right in it");
        assert_eq!(ids(shown(&lib, &Pick::Folder("Uni/WS26".into()), "", Sort::Name)), "ba");
        assert_eq!(ids(shown(&lib, &Pick::Tag("MATHE".into()), "", Sort::Name)), "ba", "tags ignore case");
        // The search looks at titles, folders and tags, ignoring case and umlauts, every word.
        assert_eq!(ids(shown(&lib, &Pick::All, "labor", Sort::Name)), "c");
        assert_eq!(ids(shown(&lib, &Pick::All, "pruefung", Sort::Name)), "a");
        assert_eq!(ids(shown(&lib, &Pick::All, "uebung", Sort::Name)), "e");
        assert_eq!(ids(shown(&lib, &Pick::All, "ws26 analysis", Sort::Name)), "a");
        // The search looks through the whole library, whatever is picked.
        assert_eq!(ids(shown(&lib, &Pick::Folder("Uni".into()), "physik", Sort::Name)), "c");
        assert_eq!(ids(shown(&lib, &Pick::Tag("Labor".into()), "ideen", Sort::Name)), "d");
        assert_eq!(all_tags(&lib), vec![("Mathe".to_string(), 2), ("Privat".to_string(), 1), ("Prüfung".to_string(), 1)]);
    }

    #[test]
    fn renaming_and_dissolving_move_the_paths_inside_along() {
        // Renamed: the folder and what lies in it.
        assert_eq!(moved("Uni/WS26", "Uni/WS26", "Uni/Winter").as_deref(), Some("Uni/Winter"));
        assert_eq!(moved("Uni/WS26/Analysis", "Uni/WS26", "Uni/Winter").as_deref(), Some("Uni/Winter/Analysis"));
        assert_eq!(moved("Uni/WS26x", "Uni/WS26", "Uni/Winter"), None, "a name that only starts the same isn't inside");
        assert_eq!(moved("Arbeit", "Uni", "Studium"), None);
        // Dissolved: one level up; at the top what was in it goes to the top.
        assert_eq!(moved("Uni/WS26", "Uni/WS26", "Uni").as_deref(), Some("Uni"));
        assert_eq!(moved("Uni/WS26/Analysis", "Uni/WS26", "Uni").as_deref(), Some("Uni/Analysis"));
        assert_eq!(moved("Uni", "Uni", "").as_deref(), Some(""));
        assert_eq!(moved("Uni/SS25", "Uni", "").as_deref(), Some("SS25"));
        assert!(within("Uni/WS26", "Uni") && within("Uni", "Uni") && within("x", "") && !within("Unix", "Uni"));
        assert_eq!((parent("Uni/WS26/A"), name("Uni/WS26/A"), parent("Uni"), name("Uni")), ("Uni/WS26", "A", "", "Uni"));
        assert_eq!(join("", " Uni "), "Uni");
        assert_eq!(join("Uni", "WS26"), "Uni/WS26");
    }

    #[test]
    fn a_tag_keeps_its_colour_whatever_its_case() {
        assert_eq!(tag_colour("Mathe"), tag_colour("mathe"));
        // "a" is 97: 97 mod 8 = 1, the second colour (red); "ab" is 195: 3, orange.
        assert_eq!(tag_colour("a"), Color::from_rgb8(0xD7, 0x19, 0x21));
        assert_eq!(tag_colour("AB"), Color::from_rgb8(0xE0, 0x8A, 0x00));
        // Like the tablet (UTF-16 units): "😀" is two units, 0xD83D + 0xDE00, mod 8 = 5.
        assert_eq!(tag_colour("😀"), Color::from_rgb8(0x0A, 0xA4, 0xB8));
    }
}
