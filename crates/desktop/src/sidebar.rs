//! Left sidebar of a notebook: "Gliederung · 12 Seiten" with a switch between the list and
//! page pictures, the search field, then the outline — groups fold, pages jump, the headings
//! marked on a page under it, a red ribbon on pages marked to find again (only those: "Gemerkt"),
//! a right click renames, marks or deletes — numbered like the pages; below it new pages and
//! groups, and folding every group. While there is text in the field: what the search found,
//! page by page.

use std::collections::{HashMap, HashSet};

use iced::widget::text::Span;
use iced::widget::{Column, Space, button, column, container, rich_text, row, scrollable, span, stack, svg, text, text_input};
use iced::{Alignment, Color, Element, Font, Length, Padding, Point, Task};

use n0te_core::notebook::{Background, OutlineRow, SheetSpec};
use n0te_core::search::{HitKind, words};
use n0te_core::sheet::Heading;

use crate::app::{App, Message, Open, Screen};
use crate::library::fit;
use crate::search::{self, Search};
use crate::shelf::{self, MenuRow};
use crate::i18n::{tr, trf};
use crate::{pages, pdf, split, theme, widgets};

pub const WIDTH: f32 = 272.0;
/// Hits listed at most (a common word in a long notebook would make thousands of rows).
pub const MAX_ROWS: usize = 200;
/// Page pictures: two to a row in the pictures view, small in the list and the marked pages.
const PICTURE_W: f32 = 112.0;
const ROW_PICTURE_W: f32 = 18.0;
const MARKED_PICTURE_W: f32 = 30.0;

/// The ids of one pane's outline. Two outlines can show at once (two notebooks side by side):
/// the active pane's carry the plain ids, so focusing the search field and scrolling the list
/// reach only that one; the other pane's get an ending of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ids {
    pub other: bool,
}

impl Ids {
    pub const ACTIVE: Ids = Ids { other: false };

    fn id(self, plain: String) -> iced::widget::Id {
        iced::widget::Id::from(if self.other { format!("{plain}~other") } else { plain })
    }

    /// The search field (Ctrl+F focuses it).
    pub fn input(self) -> iced::widget::Id {
        self.id("n0te-search".into())
    }

    /// The scrolling list (outline, pictures, marked pages or results).
    pub fn list(self) -> iced::widget::Id {
        self.id("n0te-sidebar-list".into())
    }

    /// The `i`-th hit in the results.
    pub fn hit(self, i: usize) -> iced::widget::Id {
        self.id(format!("n0te-hit-{i}"))
    }

    /// A heading's row in the outline.
    pub fn heading(self, id: &str) -> iced::widget::Id {
        self.id(format!("n0te-heading-{id}"))
    }

    /// A page's or group's row (list), picture (pictures) or entry (marked pages).
    pub fn page(self, node: &str) -> iced::widget::Id {
        self.id(format!("n0te-page-{node}"))
    }

    /// One of the outline's buttons: "outline-list", "pictures", "add-page", "add-group", "fold-all".
    pub fn named(self, name: &str) -> iced::widget::Id {
        self.id(name.into())
    }
}

/// The active pane's search field.
pub fn input_id() -> iced::widget::Id {
    Ids::ACTIVE.input()
}

/// The active pane's list.
pub fn list_id() -> iced::widget::Id {
    Ids::ACTIVE.list()
}

/// The `i`-th hit in the active pane's results.
pub fn hit_id(i: usize) -> iced::widget::Id {
    Ids::ACTIVE.hit(i)
}

/// A heading's row in the active pane's outline.
pub fn heading_id(id: &str) -> iced::widget::Id {
    Ids::ACTIVE.heading(id)
}

/// A page's or group's row in the active pane's outline.
pub fn page_id(node: &str) -> iced::widget::Id {
    Ids::ACTIVE.page(node)
}

/// A row of the page menu: "rename", "mark", "beside", "delete", "fold".
pub fn menu_id(what: &str) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-page-menu-{what}"))
}

/// The rename dialog's field.
pub fn rename_id() -> iced::widget::Id {
    iced::widget::Id::new("n0te-page-rename")
}

/// A pane's outline state: only the marked pages, the page menu, the rename dialog.
#[derive(Debug, Default)]
pub struct Outline {
    pub only_marked: bool,
    /// The menu of a page or group (node) at a window position.
    pub menu: Option<(String, Point)>,
    /// The node whose "Löschen" was clicked once.
    pub asked: Option<String>,
    /// Renaming a page or group: (node, the name typed).
    pub renaming: Option<(String, String)>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    /// Page pictures (else the list); kept in the settings.
    Pictures(bool),
    OnlyMarked(bool),
    Menu(String, Point),
    CloseMenu,
    Rename(String),
    RenameText(String),
    RenameDone,
    CloseRename,
    Mark(String, bool),
    /// The page opens beside (in the other pane when there are two).
    Beside(String),
    /// A page (sheet, its name) dragged out of the outline: dropped on the other pane or the
    /// right of the window, it opens there.
    Drag(String, String, widgets::Drag),
    /// The first click asks, the second deletes (undoable).
    Delete(String),
    Undo,
    /// Fold or unfold one group (from its menu).
    Fold(String, bool),
    /// "+ Seite" after the page in view (its paper and size), "+ Gruppe" (named right away).
    AddPage,
    AddGroup,
    /// Every group shown: folded (true) or open.
    FoldAll(bool),
}

fn om(m: Msg) -> Message {
    Message::Outline(m)
}

/// A pane's outline (`ids`: whether it is the active pane's, see [`Ids`]).
pub fn view<'a>(app: &'a App, o: &'a Open, current: u32, ids: Ids) -> Element<'a, Message> {
    let search = &o.search;
    let pictures = app.settings.outline_pictures;
    let view_pick = |glyph: &'static [u8], on: bool, id: &'static str| {
        let label = if id == "pictures" { tr("Als Seitenbilder") } else { tr("Als Liste") };
        widgets::tip(container(button(shelf::icon(glyph, 14.0, if on { theme::p().chrome } else { theme::p().muted })).padding([4, 7]).on_press(om(Msg::Pictures(id == "pictures"))).style(theme::tab(on))).id(ids.named(id)), label)
    };
    let n = o.pages.total_pages;
    let head = row![
        theme::caption(tr("Gliederung")),
        text(theme::cased(&format!(" · {}", if n == 1 { trf("{} Seite", &[&n]) } else { trf("{} Seiten", &[&n]) }))).font(theme::meta_font()).size(11).color(theme::p().muted),
        Space::new().width(Length::Fill),
        view_pick(shelf::LIST, !pictures, "outline-list"),
        view_pick(shelf::GRID, pictures, "pictures"),
    ]
    .spacing(4)
    .align_y(Alignment::Center);
    let mut body = column![
        container(head).padding(Padding { top: 10.0, right: 10.0, bottom: 2.0, left: 18.0 }),
        container(field(&search.query, ids)).padding(Padding { top: 6.0, right: 10.0, bottom: 2.0, left: 10.0 }),
    ];
    let pad = Padding { top: 6.0, right: 8.0, bottom: 10.0, left: 8.0 };
    if search.active() {
        body = body.push(scrollable(results(search, ids).padding(pad)).id(ids.list()).style(theme::scroll_dark).height(Length::Fill));
        return container(body).width(WIDTH).height(Length::Fill).style(theme::surface).into();
    }
    let marked: Vec<&String> = o.marked.iter().filter(|m| o.pages.pages.iter().any(|p| p.placed.node == **m)).collect();
    let only = o.outline.only_marked && !o.marked.is_empty();
    if !o.marked.is_empty() {
        let pick = |label: String, on: bool, m: Msg| button(theme::btn_small(&label)).padding([4, 10]).on_press(om(m)).style(theme::tab(on));
        body = body.push(
            container(row![pick(tr("Alle").into(), !only, Msg::OnlyMarked(false)), pick(trf("Gemerkt · {}", &[&o.marked.len()]), only, Msg::OnlyMarked(true))].spacing(6))
                .padding(Padding { top: 6.0, right: 10.0, bottom: 0.0, left: 10.0 }),
        );
    }
    let list = if only {
        marked_pages(o, &app.headings, &marked, current, ids)
    } else if pictures {
        pictures_view(o, &app.headings, current, ids)
    } else {
        outline(app, o, current, ids)
    };
    body = body.push(scrollable(list.padding(pad)).id(ids.list()).style(theme::scroll_dark).height(Length::Fill));
    if !app.read_only_nb(&o.id) {
        body = body.push(footer(o, ids));
    }
    container(body).width(WIDTH).height(Length::Fill).style(theme::surface).into()
}

/// "+ Seite", "+ Gruppe", and every group folded or open.
fn footer(o: &Open, ids: Ids) -> Element<'_, Message> {
    let small = |label: &str, m: Msg, id: &'static str| container(button(theme::btn_small(label)).padding([4, 9]).on_press(om(m)).style(theme::ghost)).id(ids.named(id));
    let mut r = row![small(tr("+ Seite"), Msg::AddPage, "add-page"), small(tr("+ Gruppe"), Msg::AddGroup, "add-group"), Space::new().width(Length::Fill)].spacing(4).align_y(Alignment::Center);
    let groups: Vec<bool> = o.rows.iter().filter_map(|r| if let OutlineRow::Group { collapsed, .. } = r { Some(*collapsed) } else { None }).collect();
    if !groups.is_empty() {
        let open = groups.iter().any(|c| !c);
        r = r.push(small(if open { tr("Zuklappen") } else { tr("Aufklappen") }, Msg::FoldAll(open), "fold-all"));
    }
    column![container(Space::new()).height(1).width(Length::Fill).style(theme::hairline), container(r).padding([6, 8])].into()
}

/// What a page is called in the outline: its title, else its first heading, else a text
/// page's first line, else what it is.
pub fn page_name(title: &str, spec: &SheetSpec, first_heading: Option<&str>, first_line: Option<&str>) -> String {
    if !title.trim().is_empty() {
        return title.to_string();
    }
    if let Some(h) = first_heading.map(str::trim).filter(|h| !h.is_empty()) {
        return h.to_string();
    }
    if spec.background == Background::Text
        && let Some(l) = first_line.map(str::trim).filter(|l| !l.is_empty())
    {
        return l.to_string();
    }
    match spec.pdf {
        Some((_, page)) => trf("PDF-Seite {}", &[&(page + 1)]),
        None if spec.is_board() => "Board".into(),
        None if spec.background == Background::Text => tr("Textseite").into(),
        None => tr("Seite").into(),
    }
}

/// A page's name as the outline shows it (title, heading, a text page's first line, kind).
fn name_of(o: &Open, headings: &HashMap<String, Vec<Heading>>, title: &str, sheet_id: &str, spec: &SheetSpec) -> String {
    let heads: &[Heading] = headings.get(sheet_id).map(Vec::as_slice).unwrap_or(&[]);
    let first = heads.iter().find(|h| !h.text.trim().is_empty()).map(|h| h.text.as_str());
    let line = if title.trim().is_empty() && first.is_none() && spec.background == Background::Text { o.pages.first_line(sheet_id) } else { None };
    page_name(title, spec, first, line.as_deref())
}

/// The search field: a loupe, the query, × to empty it (Escape does too).
fn field(query: &str, ids: Ids) -> Element<'_, Message> {
    let input = text_input(tr("Suchen …"), query)
        .id(ids.input())
        .on_input(Message::SearchQuery)
        .on_submit(Message::SearchSubmit)
        .padding(Padding { top: 7.0, bottom: 7.0, left: 30.0, right: 28.0 })
        .size(13.5)
        .style(theme::field);
    let mut layers = stack![
        widgets::on_escape(input, Message::SearchClear),
        container(icon(LOUPE, 14.0, theme::p().muted)).padding(Padding { left: 10.0, ..Padding::ZERO }).center_y(Length::Fill),
    ]
    .width(Length::Fill);
    if !query.is_empty() {
        let clear = widgets::tip(button(text("×").size(14).line_height(1.0)).padding([2, 7]).on_press(Message::SearchClear).style(theme::ghost), tr("Suche leeren (Esc)"));
        layers = layers.push(container(clear).padding(Padding { right: 4.0, ..Padding::ZERO }).align_right(Length::Fill).center_y(Length::Fill));
    }
    layers.into()
}

/// A page or group row with its right-click menu.
fn with_menu<'a>(content: impl Into<Element<'a, Message>>, node: &str, ids: Ids) -> Element<'a, Message> {
    let n = node.to_string();
    container(widgets::pointer(content).on_right(move |at| om(Msg::Menu(n.clone(), at)))).id(ids.page(node)).into()
}

/// A page's row or picture: its right-click menu, and dragged onto the other pane (or the right
/// of the window) it opens there.
fn page_row<'a>(content: impl Into<Element<'a, Message>>, node: &str, sheet: &str, name: &str, ids: Ids) -> Element<'a, Message> {
    let (n, sheet, name) = (node.to_string(), sheet.to_string(), name.to_string());
    let menu = n.clone();
    container(widgets::pointer(content).on_right(move |at| om(Msg::Menu(menu.clone(), at))).on_drag(move |d| om(Msg::Drag(sheet.clone(), name.clone(), d)))).id(ids.page(&n)).into()
}

fn outline<'a>(app: &'a App, o: &'a Open, current: u32, ids: Ids) -> Column<'a, Message> {
    let (rows, pages, headings) = (&o.rows, &o.pages, &app.headings);
    let numbers: HashMap<&str, u32> = pages.pages.iter().filter(|p| p.placed.side == 0).map(|p| (p.placed.node.as_str(), p.placed.number)).collect();
    let marked: HashSet<&str> = o.marked.iter().map(String::as_str).collect();
    let mut list = column![].spacing(1);
    for r in rows {
        match r {
            OutlineRow::Group { node, depth, title, collapsed, count } => {
                let content = row![
                    text(if *collapsed { "▸" } else { "▾" }).font(theme::SANS_BOLD).size(12).color(theme::p().accent).width(18),
                    text(fit(if title.trim().is_empty() { tr("Gruppe") } else { title }, 24)).font(theme::SANS_MEDIUM).size(13.5).wrapping(text::Wrapping::None),
                    Space::new().width(Length::Fill),
                    text(count.to_string()).font(theme::meta_font()).size(11).color(theme::p().muted),
                ]
                .align_y(Alignment::Center);
                let b = button(content)
                    .on_press(Message::Fold(node.clone()))
                    .padding(Padding { top: 7.0, bottom: 7.0, left: 10.0 + *depth as f32 * 14.0, right: 10.0 })
                    .width(Length::Fill)
                    .style(theme::row(false));
                list = list.push(with_menu(b, node, ids));
            }
            OutlineRow::Sheet { node, depth, sheet_id, title, spec, left, right } => {
                let n = numbers.get(node.as_str()).copied().unwrap_or(0);
                let heads: &[Heading] = headings.get(sheet_id).map(Vec::as_slice).unwrap_or(&[]);
                // Untitled: the first heading names the page (and isn't listed again under it).
                let named_by = (title.trim().is_empty()).then(|| heads.iter().position(|h| !h.text.trim().is_empty())).flatten();
                let name = name_of(o, headings, title, sheet_id, spec);
                let sides = format!("{}{}", if left.is_empty() { String::new() } else { format!("‹{} ", left.len()) }, if right.is_empty() { String::new() } else { format!("{}›", right.len()) });
                let picture: Element<'a, Message> = match pages.page_of_sheet(sheet_id) {
                    Some(i) => pages::thumb(pages, i, ROW_PICTURE_W, false).into(),
                    None => Space::new().width(ROW_PICTURE_W).into(),
                };
                let mut content = row![
                    text(format!("{n:02}")).font(theme::meta_font()).size(11.5).color(if n == current { theme::p().accent } else { theme::p().muted }).width(28),
                    picture,
                    Space::new().width(8),
                    text(fit(&name, 20)).font(theme::SANS).size(13).wrapping(text::Wrapping::None),
                    Space::new().width(Length::Fill),
                ]
                .align_y(Alignment::Center);
                if marked.contains(node.as_str()) {
                    content = content.push(shelf::icon(shelf::RIBBON, 12.0, theme::p().accent));
                    content = content.push(Space::new().width(4));
                }
                content = content.push(text(sides).font(theme::meta_font()).size(11).color(theme::p().accent));
                content = content.push(Space::new().width(6));
                // Only a page that differs from the usual A4 says its format ("A4 quer", "A5 hoch").
                content = content.push(text(format_tag(spec)).font(theme::meta_font()).size(11).color(theme::p().muted));
                let indent = 10.0 + *depth as f32 * 14.0;
                let b = button(content)
                    .on_press(Message::Jump(sheet_id.clone()))
                    .padding(Padding { top: 4.0, bottom: 4.0, left: indent, right: 10.0 })
                    .width(Length::Fill)
                    .style(theme::row(n == current));
                list = list.push(page_row(b, node, sheet_id, &name, ids));
                // The page's headings, under its name: a click shows the heading on the page.
                for (_, h) in heads.iter().enumerate().filter(|(i, _)| Some(*i) != named_by) {
                    let label = if h.text.trim().is_empty() { tr("Überschrift") } else { h.text.trim() };
                    let content = row![
                        text("—").size(11).color(theme::p().muted).width(16),
                        text(fit(label, 24)).size(12).color(theme::alpha(theme::p().text, 0.78)).wrapping(text::Wrapping::None),
                    ]
                    .align_y(Alignment::Center);
                    list = list.push(
                        container(
                            button(content)
                                .on_press(Message::Reveal(sheet_id.clone(), [h.x0, h.y0, h.x1, h.y1]))
                                .padding(Padding { top: 3.0, bottom: 4.0, left: indent + 46.0, right: 10.0 })
                                .width(Length::Fill)
                                .style(theme::row(false)),
                        )
                        .id(ids.heading(&h.id)),
                    );
                }
            }
        }
    }
    list
}

/// The outline as page pictures, two to a row, "01 Name" under each; groups as headings
/// across that fold; the page in view outlined in the accent, marked ones with a ribbon.
fn pictures_view<'a>(o: &'a Open, headings: &HashMap<String, Vec<Heading>>, current: u32, ids: Ids) -> Column<'a, Message> {
    let marked: HashSet<&str> = o.marked.iter().map(String::as_str).collect();
    let mut list = column![].spacing(12);
    let mut pair: Vec<Element<'a, Message>> = Vec::new();
    let flush = |list: Column<'a, Message>, pair: &mut Vec<Element<'a, Message>>| {
        if pair.is_empty() {
            return list;
        }
        list.push(iced::widget::Row::with_children(std::mem::take(pair)).spacing(10))
    };
    for r in &o.rows {
        match r {
            OutlineRow::Group { node, depth, title, collapsed, count } => {
                list = flush(list, &mut pair);
                let content = row![
                    text(if *collapsed { "▸" } else { "▾" }).font(theme::SANS_BOLD).size(12).color(theme::p().accent).width(18),
                    text(fit(if title.trim().is_empty() { tr("Gruppe") } else { title }, 26)).font(theme::SANS_BOLD).size(13.5).wrapping(text::Wrapping::None),
                    Space::new().width(Length::Fill),
                    text(count.to_string()).font(theme::meta_font()).size(11).color(theme::p().muted),
                ]
                .align_y(Alignment::Center);
                let b = button(content).on_press(Message::Fold(node.clone())).padding(Padding { top: 6.0, bottom: 4.0, left: 4.0 + *depth as f32 * 12.0, right: 6.0 }).width(Length::Fill).style(theme::row(false));
                list = list.push(with_menu(b, node, ids));
            }
            OutlineRow::Sheet { node, sheet_id, title, spec, .. } => {
                let Some(i) = o.pages.page_of_sheet(sheet_id) else { continue };
                let n = o.pages.pages[i].placed.number;
                let mut picture = stack![pages::thumb(&o.pages, i, PICTURE_W, n == current)];
                if marked.contains(node.as_str()) {
                    picture = picture.push(container(shelf::icon(shelf::RIBBON, 16.0, theme::p().accent)).padding(Padding { top: 3.0, right: 4.0, ..Padding::ZERO }).align_right(Length::Fill));
                }
                let name = name_of(o, headings, title, sheet_id, spec);
                let label = row![
                    text(format!("{n:02}")).font(theme::meta_font()).size(11).color(if n == current { theme::p().accent } else { theme::p().muted }),
                    text(fit(&name, 14)).size(12).color(theme::p().text).wrapping(text::Wrapping::None),
                ]
                .spacing(6)
                .align_y(Alignment::Center);
                let b = button(column![picture, label].spacing(5)).on_press(Message::Jump(sheet_id.clone())).padding(3).style(theme::row(false));
                pair.push(page_row(b, node, sheet_id, &name, ids));
                if pair.len() == 2 {
                    list = flush(list, &mut pair);
                }
            }
        }
    }
    flush(list, &mut pair)
}

/// The marked pages wherever they are (in folded groups they don't show): picture, number,
/// name, a ribbon that unmarks; a click shows the page.
fn marked_pages<'a>(o: &'a Open, headings: &HashMap<String, Vec<Heading>>, marked: &[&'a String], current: u32, ids: Ids) -> Column<'a, Message> {
    let mut list = column![].spacing(2);
    if marked.is_empty() {
        return list.push(container(text(tr("Die gemerkten Seiten liegen in zugeklappten Gruppen.")).size(12).color(theme::p().muted)).padding([8, 10]));
    }
    let titles: HashMap<&str, (&str, &SheetSpec)> = o.rows.iter().filter_map(|r| if let OutlineRow::Sheet { node, title, spec, .. } = r { Some((node.as_str(), (title.as_str(), spec))) } else { None }).collect();
    for node in marked {
        let Some(i) = o.pages.pages.iter().position(|p| p.placed.node == **node) else { continue };
        let p = &o.pages.pages[i].placed;
        let main_name = titles.get(p.main_node.as_str()).map(|(t, spec)| {
            let sheet = o.pages.pages.iter().find(|q| q.placed.node == p.main_node).map(|q| q.placed.sheet_id.clone()).unwrap_or_default();
            name_of(o, headings, t, &sheet, spec)
        });
        let name = match p.side {
            0 => main_name.unwrap_or_else(|| tr("Seite").into()),
            s => format!("{} · {}", if s < 0 { format!("L{}", -s) } else { format!("R{s}") }, main_name.unwrap_or_else(|| tr("Seite").into())),
        };
        let on = p.side == 0 && p.number == current;
        let unmark = widgets::tip(button(shelf::icon(shelf::RIBBON, 14.0, theme::p().accent)).padding([4, 6]).on_press(om(Msg::Mark((*node).clone(), false))).style(theme::ghost), tr("Nicht mehr merken"));
        let content = row![
            pages::thumb(&o.pages, i, MARKED_PICTURE_W, on),
            text(format!("{:02}", p.number)).font(theme::meta_font()).size(11.5).color(if on { theme::p().accent } else { theme::p().muted }).width(26),
            text(fit(&name, 17)).size(13).wrapping(text::Wrapping::None),
            Space::new().width(Length::Fill),
            unmark,
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let b = button(content).on_press(Message::Jump(p.sheet_id.clone())).padding([5, 8]).width(Length::Fill).style(theme::row(on));
        list = list.push(page_row(b, node, &p.sheet_id, &name, ids));
    }
    list
}

/// The notebook under a pane's page menu and rename dialog (always these layers, so the page
/// view keeps its state when one shows). Their messages go to that pane.
pub fn layers<'a>(app: &'a App, base: Element<'a, Message>) -> Element<'a, Message> {
    let Screen::Notebook(s) = &app.screen else { return base };
    let dialog: Element<'a, Message> = match s.panes.iter().find_map(|o| o.outline.renaming.as_ref().map(|r| (o, r))) {
        Some((o, (node, value))) => {
            let group = o.rows.iter().any(|r| matches!(r, OutlineRow::Group { node: n, .. } if n == node));
            let input = widgets::on_escape(
                text_input(if group { tr("Name der Gruppe") } else { tr("Name der Seite") }, value).id(rename_id()).on_input(|t| om(Msg::RenameText(t))).on_submit(om(Msg::RenameDone)).padding(10).size(15).style(theme::field),
                om(Msg::CloseRename),
            );
            let foot = row![
                Space::new().width(Length::Fill),
                button(theme::btn(tr("Abbrechen"))).padding(theme::pad()).on_press(om(Msg::CloseRename)).style(theme::ghost),
                button(theme::btn(tr("Umbenennen"))).padding(theme::pad()).on_press(om(Msg::RenameDone)).style(theme::accent_button),
            ]
            .spacing(8);
            let sub = if group { String::new() } else { tr("Leer: die erste Überschrift benennt sie").into() };
            let key = o.key;
            shelf::dialog_layer(shelf::frame(if group { tr("Gruppe umbenennen") } else { tr("Seite umbenennen") }, sub, input.into(), foot.into(), 420.0), om(Msg::CloseRename)).map(move |m| split::route(key, m))
        }
        None => Space::new().into(),
    };
    let menu: Element<'a, Message> = match s.panes.iter().find_map(|o| o.outline.menu.as_ref().map(|m| (o, m))) {
        Some((o, (node, at))) => {
            let key = o.key;
            shelf::float_menu(*at, move || menu_rows(o, node), om(Msg::CloseMenu)).map(move |m| split::route(key, m))
        }
        None => Space::new().into(),
    };
    stack![base, dialog, menu].into()
}

/// A page's menu: rename, mark, open beside, delete; a group's: rename, fold, delete.
fn menu_rows(o: &Open, node: &str) -> Vec<MenuRow> {
    let asked = o.outline.asked.as_deref() == Some(node);
    let delete = MenuRow { id: menu_id("delete"), glyph: shelf::TRASH, label: if asked { tr("Wirklich löschen?") } else { tr("Löschen") }, danger: true, on: om(Msg::Delete(node.to_string())) };
    let rename = MenuRow { id: menu_id("rename"), glyph: shelf::PEN, label: tr("Umbenennen"), danger: false, on: om(Msg::Rename(node.to_string())) };
    match o.rows.iter().find_map(|r| if let OutlineRow::Group { node: n, collapsed, .. } = r { (n == node).then_some(*collapsed) } else { None }) {
        Some(collapsed) => vec![
            rename,
            MenuRow {
                id: menu_id("fold"),
                glyph: if collapsed { shelf::FOLD_DOWN } else { shelf::FOLD_UP },
                label: if collapsed { tr("Aufklappen") } else { tr("Zuklappen") },
                danger: false,
                on: om(Msg::Fold(node.to_string(), !collapsed)),
            },
            delete,
        ],
        None => {
            let marked = o.marked.iter().any(|m| m == node);
            vec![
                rename,
                MenuRow { id: menu_id("mark"), glyph: shelf::RIBBON, label: if marked { tr("Nicht mehr merken") } else { tr("Merken") }, danger: false, on: om(Msg::Mark(node.to_string(), !marked)) },
                MenuRow { id: menu_id("beside"), glyph: split::SPLIT, label: tr("Daneben öffnen"), danger: false, on: om(Msg::Beside(node.to_string())) },
                delete,
            ]
        }
    }
}

/// Brings the page in view into view in the active pane's outline too (opening, switching list
/// and pictures): its row about a third down the list.
pub fn reveal_current(app: &App) -> Task<Message> {
    let Screen::Notebook(s) = &app.screen else { return Task::none() };
    let o = s.active_pane();
    let current = o.pages.current_number(&o.camera);
    let Some(i) = o.pages.index_of_number(current) else { return Task::none() };
    iced::advanced::widget::operate(Reveal { list: list_id(), target: page_id(&o.pages.pages[i].placed.node), scroll: None, found: None })
}

/// Finds a row and the list it is in, then scrolls the list there.
struct Reveal {
    list: iced::widget::Id,
    target: iced::widget::Id,
    scroll: Option<(iced::Rectangle, iced::Rectangle)>,
    found: Option<iced::Rectangle>,
}

impl<T: 'static> iced::advanced::widget::Operation<T> for Reveal {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn iced::advanced::widget::Operation<T>)) {
        operate(self);
    }
    fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
        if id == Some(&self.target) {
            self.found = Some(bounds);
        }
    }
    fn scrollable(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle, content: iced::Rectangle, _translation: iced::Vector, _state: &mut dyn iced::advanced::widget::operation::Scrollable) {
        if id == Some(&self.list) {
            self.scroll = Some((bounds, content));
        }
    }
    fn finish(&self) -> iced::advanced::widget::operation::Outcome<T> {
        use iced::advanced::widget::operation::{Outcome, scrollable::{AbsoluteOffset, scroll_to}};
        match (self.scroll, self.found) {
            (Some((bounds, content)), Some(row)) => {
                let y = (row.y - content.y - bounds.height / 3.0).max(0.0);
                Outcome::Chain(Box::new(scroll_to(self.list.clone(), AbsoluteOffset { x: None, y: Some(y) })))
            }
            _ => Outcome::None,
        }
    }
}

// --- what the outline does -------------------------------------------------------------------

impl App {
    /// The outline of the pane messages are for.
    fn outline_mut(&mut self) -> Option<&mut Outline> {
        self.pane_mut().map(|o| &mut o.outline)
    }

    pub(crate) fn on_outline(&mut self, m: Msg) -> Task<Message> {
        match m {
            Msg::Pictures(on) => {
                self.settings.outline_pictures = on;
                if self.persist_settings {
                    self.settings.save();
                }
                return reveal_current(self);
            }
            Msg::OnlyMarked(on) => {
                if let Some(s) = self.outline_mut() {
                    s.only_marked = on;
                }
                if !on {
                    return reveal_current(self);
                }
            }
            Msg::Menu(node, at) => {
                let read_only = self.read_only();
                if let Some(s) = self.outline_mut().filter(|_| !read_only) {
                    s.menu = Some((node, at));
                    s.asked = None;
                }
            }
            Msg::CloseMenu => {
                if let Some(s) = self.outline_mut() {
                    s.menu = None;
                    s.asked = None;
                }
            }
            Msg::Rename(node) => {
                let Some(o) = self.pane_mut() else { return Task::none() };
                o.outline.menu = None;
                let title = o
                    .rows
                    .iter()
                    .find_map(|r| match r {
                        OutlineRow::Group { node: n, title, .. } | OutlineRow::Sheet { node: n, title, .. } if *n == node => Some(title.clone()),
                        _ => None,
                    })
                    .unwrap_or_default();
                o.outline.renaming = Some((node, title));
                return iced::widget::operation::focus(rename_id()).chain(iced::widget::operation::select_all(rename_id()));
            }
            Msg::RenameText(t) => {
                if let Some((_, v)) = self.outline_mut().and_then(|s| s.renaming.as_mut()) {
                    *v = t;
                }
            }
            Msg::RenameDone => {
                if let Some((node, v)) = self.outline_mut().and_then(|s| s.renaming.take()) {
                    self.edit_notebook(|nb| nb.set_title_of(&node, v.trim()));
                }
            }
            Msg::CloseRename => {
                if let Some(s) = self.outline_mut() {
                    s.renaming = None;
                }
            }
            Msg::Mark(node, on) => {
                if let Some(s) = self.outline_mut() {
                    s.menu = None;
                }
                self.edit_notebook(|nb| nb.set_marked(&node, on));
                if let Some(o) = self.pane_mut()
                    && o.marked.is_empty()
                {
                    o.outline.only_marked = false;
                }
            }
            Msg::Drag(sheet, name, d) => return self.drag_page(sheet, name, d),
            Msg::Beside(node) => {
                let Some(o) = self.pane_mut() else { return Task::none() };
                o.outline.menu = None;
                let sheet = o.pages.pages.iter().find(|p| p.placed.node == node).map(|p| p.placed.sheet_id.clone());
                let id = o.id.clone();
                return self.open_beside(&id, sheet);
            }
            Msg::Delete(node) => {
                let Some(o) = self.pane_mut() else { return Task::none() };
                if o.outline.asked.as_deref() != Some(node.as_str()) {
                    o.outline.asked = Some(node);
                    return Task::none();
                }
                o.outline.menu = None;
                o.outline.asked = None;
                let group = o.rows.iter().any(|r| matches!(r, OutlineRow::Group { node: n, .. } if *n == node));
                // The text page open on it: the page itself, one of its side pages, or the notes
                // docked beside it (they go with it).
                let on_it = o.text_page.as_ref().is_some_and(|t| {
                    t.main_node.as_deref() == Some(node.as_str())
                        || o.rows.iter().any(|r| matches!(r, OutlineRow::Sheet { node: n, sheet_id, .. } if *n == node && *sheet_id == t.sheet_id))
                        || o.pages.pages.iter().any(|p| (p.placed.node == node || p.placed.main_node == node) && p.placed.sheet_id == t.sheet_id)
                });
                let id = o.id.clone();
                self.finish_text();
                if group || on_it {
                    self.close_text_page();
                }
                self.edit_notebook(|nb| nb.delete_undoable(&node));
                // One history per notebook: "Rückgängig" undoes it whichever pane shows it.
                self.undo_delete = Some(id);
                self.say(if group { tr("Gruppe gelöscht") } else { tr("Seite gelöscht") });
            }
            Msg::Undo => {
                let Some(nb) = self.undo_delete.take() else { return Task::none() };
                let Some(key) = self.pane_showing(&nb) else { return Task::none() };
                self.in_pane(key, |app| app.edit_notebook(|nb| nb.undo_delete().map(|_| ())));
                self.say(tr("Wiederhergestellt"));
            }
            Msg::Fold(node, fold) => {
                if let Some(s) = self.outline_mut() {
                    s.menu = None;
                }
                self.edit_notebook(|nb| nb.set_collapsed(&node, fold));
            }
            Msg::FoldAll(fold) => {
                let Some(o) = self.pane() else { return Task::none() };
                let groups: Vec<String> = o.rows.iter().filter_map(|r| if let OutlineRow::Group { node, collapsed, .. } = r { (*collapsed != fold).then(|| node.clone()) } else { None }).collect();
                self.edit_notebook(|nb| groups.iter().try_for_each(|g| nb.set_collapsed(g, fold)));
            }
            Msg::AddPage => return self.add_page(),
            Msg::AddGroup => {
                let Some(o) = self.pane() else { return Task::none() };
                let current = o.pages.current_number(&o.camera);
                let after = o.pages.index_of_number(current).map(|i| o.pages.pages[i].placed.node.clone());
                let (parent, index) = pdf::position_after(&o.nb, after.as_deref());
                let mut made = None;
                self.edit_notebook(|nb| {
                    made = Some(nb.add_group(parent.as_deref(), index, tr("Neue Gruppe"))?);
                    Ok(())
                });
                if let Some(node) = made
                    && let Some(s) = self.outline_mut()
                {
                    s.renaming = Some((node, tr("Neue Gruppe").into()));
                    return iced::widget::operation::focus(rename_id()).chain(iced::widget::operation::select_all(rename_id()));
                }
            }
        }
        Task::none()
    }

    /// A page after the one in view, with its paper and size (after a PDF page, a board or a
    /// text page: the paper of new notebooks), then shown.
    fn add_page(&mut self) -> Task<Message> {
        let Some(o) = self.pane() else { return Task::none() };
        let current = o.pages.current_number(&o.camera);
        let at = o.pages.index_of_number(current).map(|i| &o.pages.pages[i].placed);
        let spec = match at.map(|p| &p.spec) {
            Some(s) if s.pdf.is_none() && !s.is_board() && s.background != Background::Text => SheetSpec { gaps: Vec::new(), ..s.clone() },
            _ => SheetSpec { width: self.settings.new_w, height: self.settings.new_h, background: self.new_paper, spacing: self.new_spacing, pdf: None, gaps: Vec::new() },
        };
        let (parent, index) = pdf::position_after(&o.nb, at.map(|p| p.node.as_str()));
        let mut made = None;
        self.edit_notebook(|nb| {
            made = Some(nb.add_sheet(parent.as_deref(), index, &spec)?.1);
            Ok(())
        });
        if let (Some(sheet), Some(o)) = (made, self.pane_mut())
            && let Some(i) = o.pages.page_of_sheet(&sheet)
        {
            o.camera = o.pages.camera_for(i, &o.camera);
        }
        Task::none()
    }
}

/// What the search found: page by page in outline order (number and name), each hit with its
/// kind and its text, the query's words marked.
fn results(s: &Search, ids: Ids) -> Column<'_, Message> {
    let mut list = column![].spacing(1);
    let count: usize = s.found.as_ref().map(|(_, pages)| pages.iter().map(|p| p.hits.len()).sum()).unwrap_or(0);
    let head: Element<'_, Message> = match &s.found {
        None => theme::caption(tr("Sucht …")).into(),
        Some(_) => row![theme::caption(tr("Treffer")), Space::new().width(Length::Fill), text(count.to_string()).font(theme::meta_font()).size(11).color(theme::p().muted)].align_y(Alignment::Center).into(),
    };
    list = list.push(container(head).padding(Padding { top: 4.0, bottom: 6.0, left: 10.0, right: 10.0 }));
    let Some((query, pages)) = &s.found else { return list };
    if pages.is_empty() {
        let none = column![
            text(tr("Nichts gefunden")).size(13).color(theme::p().text),
            text(tr("Handschrift findet die Suche, sobald das Tablet sie gelesen hat.")).size(12).color(theme::p().muted),
        ]
        .spacing(4);
        return list.push(container(none).padding([2, 10]));
    }
    let w = words(query);
    let mut n = 0;
    for p in pages {
        if n >= MAX_ROWS {
            break;
        }
        let side = match p.side {
            0 => String::new(),
            s if s < 0 => format!("L{} · ", -s),
            s => format!("R{s} · "),
        };
        list = list.push(
            container(
                row![
                    text(format!("{:02}", p.number)).font(theme::meta_font()).size(11.5).color(theme::p().muted).width(28),
                    text(fit(&format!("{side}{}", p.name), 26)).font(theme::SANS_MEDIUM).size(13).wrapping(text::Wrapping::None),
                ]
                .align_y(Alignment::Center),
            )
            .padding(Padding { top: if n == 0 { 2.0 } else { 10.0 }, bottom: 3.0, left: 10.0, right: 10.0 }),
        );
        for h in &p.hits {
            if n >= MAX_ROWS {
                break;
            }
            let line = search::excerpt(&h.text, &w, 70);
            let content = row![container(kind_icon(h.kind)).padding(Padding { top: 2.0, ..Padding::ZERO }), Space::new().width(8), marked(line, &w)].align_y(Alignment::Start);
            list = list.push(
                container(
                    button(content)
                        .on_press(Message::Reveal(p.sheet_id.clone(), [h.x0, h.y0, h.x1, h.y1]))
                        .padding(Padding { top: 5.0, bottom: 5.0, left: 38.0, right: 10.0 })
                        .width(Length::Fill)
                        .style(theme::row(false)),
                )
                .id(ids.hit(n)),
            );
            n += 1;
        }
    }
    if count > n {
        list = list.push(container(text(trf("… und {} weitere — genauer suchen", &[&(count - n)])).size(12).color(theme::p().muted)).padding(Padding { top: 8.0, left: 10.0, right: 10.0, bottom: 0.0 }));
    }
    list
}

/// `line` with the query's words in the accent.
fn marked<'a>(line: String, words: &[String]) -> Element<'a, Message> {
    let plain = theme::alpha(theme::p().text, 0.88);
    let mut spans: Vec<Span<'a, (), Font>> = Vec::new();
    let mut at = 0;
    for r in search::emphasis(&line, words) {
        if r.start > at {
            spans.push(span(line[at..r.start].to_string()).color(plain));
        }
        spans.push(span(line[r.clone()].to_string()).font(theme::SANS_BOLD).color(theme::p().accent));
        at = r.end;
    }
    if at < line.len() {
        spans.push(span(line[at..].to_string()).color(plain));
    }
    rich_text(spans).font(theme::SANS).size(12.5).width(Length::Fill).into()
}

/// The small mark of a hit's kind: handwriting, typed text, heading, PDF, a function in a
/// coordinate system (the tablet's glyphs).
fn kind_icon<'a>(kind: HitKind) -> Element<'a, Message> {
    icon(glyph_of(kind), 14.0, if kind == HitKind::Heading { theme::p().accent } else { theme::p().muted })
}

/// The glyph (SVG) a hit of `kind` is listed with.
pub fn glyph_of(kind: HitKind) -> &'static [u8] {
    match kind {
        HitKind::Ink => PEN,
        HitKind::Text => TEXT,
        HitKind::Heading => HEADING,
        HitKind::Pdf => PDF,
        HitKind::Plot => PLOT,
    }
}

fn icon<'a>(glyph: &'static [u8], size: f32, color: Color) -> Element<'a, Message> {
    svg(svg::Handle::from_memory(glyph)).width(size).height(size).style(move |_, _| svg::Style { color: Some(color) }).into()
}

// Line glyphs on a 24 × 24 grid, drawn like the tablet's (Icons.kt).
const PEN: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 19 6 15 16 5 19 8 9 18 5 19M14 7 17 10"/></svg>"##;
const TEXT: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 7 5 4.5 19 4.5 19 7M12 4.5 12 19.5M9 19.5 15 19.5"/></svg>"##;
const HEADING: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 4 6 16M15 4 15 16M6 10 15 10M4 20.5 20 20.5"/></svg>"##;
const PDF: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 3 14 3 19 8 19 21 6 21ZM14 3 14 8 19 8M9 14 16 14M9 17 14 17"/></svg>"##;
/// An L-shaped pair of axes with a curve over them.
pub const PLOT: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 3.5 4 20 20.5 20M6.5 5 9 12.5 12 16 15 12.5 17.5 5"/></svg>"##;
const LOUPE: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="10.5" cy="10.5" r="6.5"/><path d="M15.3 15.3 20.5 20.5"/></svg>"##;

/// The format of a plain page when it isn't upright A4 (the paper is visible on the page anyway).
pub fn format_tag(spec: &n0te_core::notebook::SheetSpec) -> String {
    if spec.pdf.is_some() || spec.is_board() {
        return String::new();
    }
    match n0te_core::format::find(spec.width, spec.height) {
        Some((f, false)) if f.id == "a4" => String::new(),
        _ => format_name(spec.width, spec.height),
    }
}

/// A paper format by name in the app's language ("A4 quer", "Folie 16:9 hoch", "Quadrat",
/// "200 × 300 mm"): `n0te_core::format::describe`, translated.
pub fn format_name(width: f32, height: f32) -> String {
    use n0te_core::format;
    match format::find(width, height) {
        Some((f, _)) if f.width == f.height => tr(f.name).to_string(),
        // A slide is landscape by nature: only say so when it stands upright.
        Some((f, landscape)) if f.id == "slide" => if landscape { tr(f.name).to_string() } else { trf("{} hoch", &[&tr(f.name)]) },
        Some((f, true)) => trf("{} quer", &[&tr(f.name)]),
        Some((f, false)) => trf("{} hoch", &[&tr(f.name)]),
        None => format::describe(width, height),
    }
}

#[cfg(test)]
mod tests {
    use n0te_core::notebook::{Background, SheetSpec};

    #[test]
    fn only_pages_off_the_usual_a4_carry_a_format_tag() {
        assert_eq!(super::format_tag(&SheetSpec::a4(Background::Grid)), "");
        assert_eq!(super::format_tag(&SheetSpec { width: 842.0, height: 595.0, ..SheetSpec::a4(Background::Grid) }), "A4 quer");
        assert_eq!(super::format_tag(&SheetSpec::board()), "");
    }

    #[test]
    fn an_untitled_page_is_named_by_its_first_heading() {
        let grid = SheetSpec::a4(Background::Grid);
        assert_eq!(super::page_name("Analysis", &grid, Some("Satz 3"), None), "Analysis");
        assert_eq!(super::page_name("", &grid, Some("  Satz 3 "), None), "Satz 3");
        assert_eq!(super::page_name(" ", &grid, None, None), "Seite");
        assert_eq!(super::page_name("", &grid, Some(" "), None), "Seite");
        assert_eq!(super::page_name("", &SheetSpec { pdf: Some(("b".into(), 2)), ..grid.clone() }, None, None), "PDF-Seite 3");
        assert_eq!(super::page_name("", &SheetSpec::board(), None, None), "Board");
        let text = SheetSpec::a4(Background::Text);
        assert_eq!(super::page_name("", &text, None, None), "Textseite");
        // A text page without a name or heading: its first line.
        assert_eq!(super::page_name("", &text, None, Some(" Einkaufsliste ")), "Einkaufsliste");
        assert_eq!(super::page_name("", &text, Some("Satz 3"), Some("Einkaufsliste")), "Satz 3");
        assert_eq!(super::page_name("", &grid, None, Some("not a text page")), "Seite");
    }
}
