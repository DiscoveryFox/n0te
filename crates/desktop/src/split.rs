//! Two notebooks side by side, or one notebook twice: a split inside the window (one above the
//! other when the window stands upright). The divider snaps to a third, the half or two thirds;
//! dragged near an edge it closes the pane there. The one toolbar works on the pane used last
//! (the active one, framed in the accent); each pane has a small head: ★, its title and page,
//! show it alone (or both again), close it. ★ marks the main pane: live with the tablet,
//! following and summons belong to it, and it moves only when the other pane's star is
//! clicked. Two panes showing the same notebook share its document (see [`Open`]).

use std::ops::{Deref, DerefMut};

use iced::widget::{Space, button, column, container, pin, responsive, row, scrollable, stack, text, text_input, tooltip};
use iced::{Alignment, Color, Element, Length, Point, Size, Task};

use crate::app::{App, Message, Open, Screen};
use crate::library::fit;
use crate::i18n::{tr, trf};
use crate::{shelf, theme, widgets};

/// Names a pane for as long as it is open: messages from its view (and from tasks it started)
/// carry it, so they reach it even after the panes were swapped.
pub type PaneKey = u64;

// --- where the panes go -----------------------------------------------------------------------

/// A pane's rectangle in the split's room (logical pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Area {
    pub const NONE: Area = Area { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };

    pub fn is_empty(&self) -> bool {
        self.w <= 0.0 || self.h <= 0.0
    }
}

/// The divider rests at a third, the half or two thirds.
pub const SNAPS: [f32; 3] = [1.0 / 3.0, 0.5, 2.0 / 3.0];
/// Dragged closer to an edge than this (share of the room): the pane there closes.
pub const CLOSE_AT: f32 = 0.14;
/// Room between the panes (px): where the divider is.
pub const GAP: f32 = 6.0;

/// Side by side in a wide room, above each other in an upright one.
pub fn side_by_side(w: f32, h: f32) -> bool {
    w >= h
}

/// The panes' rectangles in a `w` × `h` room: `n` panes (one or two), the first taking
/// `fraction` of it, `gap` between them; `maxed` (an index) takes the whole room and the other
/// gets an empty rectangle.
pub fn rects(w: f32, h: f32, n: usize, fraction: f32, gap: f32, maxed: Option<usize>) -> Vec<Area> {
    let full = Area { x: 0.0, y: 0.0, w, h };
    match n {
        0 => Vec::new(),
        1 => vec![full],
        _ => {
            if let Some(m) = maxed.filter(|m| *m < 2) {
                return (0..2).map(|i| if i == m { full } else { Area::NONE }).collect();
            }
            let f = fraction.clamp(0.1, 0.9);
            let (before, after) = ((gap / 2.0).floor(), gap - (gap / 2.0).floor());
            if side_by_side(w, h) {
                let cut = (w * f).floor();
                vec![Area { x: 0.0, y: 0.0, w: cut - before, h }, Area { x: cut + after, y: 0.0, w: w - cut - after, h }]
            } else {
                let cut = (h * f).floor();
                vec![Area { x: 0.0, y: 0.0, w, h: cut - before }, Area { x: 0.0, y: cut + after, w, h: h - cut - after }]
            }
        }
    }
}

/// What letting go of the divider does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Release {
    /// It rests at this share.
    Snap(f32),
    /// The pane with this index closes (dragged to its edge).
    Close(usize),
}

/// Letting go of the divider at `fraction`: the nearest rest, or near an edge that pane closes.
pub fn release(fraction: f32) -> Release {
    match closing(fraction) {
        Some(i) => Release::Close(i),
        None => Release::Snap(SNAPS.into_iter().min_by(|a, b| (a - fraction).abs().total_cmp(&(b - fraction).abs())).unwrap_or(0.5)),
    }
}

/// While dragging: the pane that would close if let go now.
pub fn closing(fraction: f32) -> Option<usize> {
    if fraction < CLOSE_AT {
        Some(0)
    } else if fraction > 1.0 - CLOSE_AT {
        Some(1)
    } else {
        None
    }
}

// --- the panes ------------------------------------------------------------------------------

/// The notebooks on screen: one, or two side by side (left to right; top to bottom upright).
pub struct Split {
    pub panes: Vec<Open>,
    /// The pane the toolbar works on: the one used last.
    pub active: PaneKey,
    /// ★: the pane live mirroring, following and summons go to.
    pub main: PaneKey,
    /// The first pane's share of the room.
    pub fraction: f32,
    /// A pane shown alone for a while (⤢); the other stays open behind it.
    pub maxed: Option<PaneKey>,
    /// While the divider is held: the share of the room it is at.
    pub dragging: Option<f32>,
    /// The ⧉ menu, while it is open.
    pub menu: Option<Menu>,
    /// A page being dragged out of an outline.
    pub drag: Option<PageDrag>,
    /// The pane the message being handled is for: the active one, unless a pane's own message
    /// (from its view, or a task it started) is handled. `Deref` gives this pane, so the code
    /// written for one open notebook works on the right one.
    target: PaneKey,
}

/// A page dragged out of a pane's outline: its sheet and name, where the pointer is (window).
#[derive(Debug, Clone, PartialEq)]
pub struct PageDrag {
    pub from: PaneKey,
    pub sheet: String,
    pub name: String,
    pub at: Point,
}

/// The ⧉ menu: where it opened (window), "Alle Notizbücher …" chosen, what is typed there.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Menu {
    pub at: Point,
    pub all: bool,
    pub query: String,
}

impl Split {
    pub fn new(first: Open) -> Self {
        let k = first.key;
        Split { panes: vec![first], active: k, main: k, fraction: 0.5, maxed: None, dragging: None, menu: None, drag: None, target: k }
    }

    fn at(&self) -> usize {
        self.index(self.target).or_else(|| self.index(self.active)).unwrap_or(0)
    }

    /// The pane messages are for right now.
    pub fn key(&self) -> PaneKey {
        self.panes[self.at()].key
    }

    pub fn index(&self, key: PaneKey) -> Option<usize> {
        self.panes.iter().position(|p| p.key == key)
    }

    pub fn pane(&self, key: PaneKey) -> Option<&Open> {
        self.panes.iter().find(|p| p.key == key)
    }

    pub fn pane_mut(&mut self, key: PaneKey) -> Option<&mut Open> {
        self.panes.iter_mut().find(|p| p.key == key)
    }

    pub fn active_pane(&self) -> &Open {
        self.pane(self.active).unwrap_or(&self.panes[0])
    }

    /// The pane beside `key`.
    pub fn other(&self, key: PaneKey) -> Option<PaneKey> {
        self.panes.iter().find(|p| p.key != key).map(|p| p.key)
    }

    /// Two panes, both shown.
    pub fn is_split(&self) -> bool {
        self.panes.len() > 1 && self.maxed.is_none()
    }

    /// The pane that tells others where I look in `nb`: the main one when it shows it, else
    /// the first showing it (the same notebook twice sends one frame, not two).
    pub fn owner(&self, nb: &str) -> Option<PaneKey> {
        self.pane(self.main).filter(|p| p.id == nb).or_else(|| self.panes.iter().find(|p| p.id == nb)).map(|p| p.key)
    }

    pub fn shows(&self, nb: &str) -> bool {
        self.panes.iter().any(|p| p.id == nb)
    }

    /// The notebooks on screen, each once, left to right.
    pub fn notebooks(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for p in &self.panes {
            if !out.contains(&p.id) {
                out.push(p.id.clone());
            }
        }
        out
    }

    /// Where a page dragged out of pane `from`'s outline opens when dropped (window): on the
    /// other pane's pages, or with one pane shown on the right of its pages.
    pub fn drop_zone(&self, from: PaneKey) -> Option<iced::Rectangle> {
        let rect = |o: &Open| (o.camera.viewport.width > 0.0).then(|| iced::Rectangle::new(o.camera.origin, o.camera.viewport));
        match self.other(from).and_then(|k| self.pane(k)).filter(|_| self.is_split()) {
            Some(other) => rect(other),
            None => rect(self.pane(from)?).map(|r| iced::Rectangle { x: r.x + r.width * 0.62, width: r.width * 0.38, ..r }),
        }
    }

    /// Messages are for pane `key` from now on; returns the one they were for.
    pub(crate) fn retarget(&mut self, key: PaneKey) -> PaneKey {
        std::mem::replace(&mut self.target, key)
    }
}

impl Deref for Split {
    type Target = Open;
    fn deref(&self) -> &Open {
        &self.panes[self.at()]
    }
}

impl DerefMut for Split {
    fn deref_mut(&mut self) -> &mut Open {
        let i = self.at();
        &mut self.panes[i]
    }
}

/// A message from pane `key`'s view (or from a task it started), sent on to that pane. The
/// split's own messages name their pane already.
pub fn route(key: PaneKey, m: Message) -> Message {
    match m {
        Message::Pane(..) | Message::Activate(_) | Message::Split(_) => m,
        m => Message::Pane(key, Box::new(m)),
    }
}

/// A task's messages, sent on to pane `key`.
pub fn routed(key: PaneKey, task: Task<Message>) -> Task<Message> {
    task.map(move |m| route(key, m))
}

// --- what the split's controls say ------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum Msg {
    /// ⧉: the menu at a window position (None: closed), its "Alle Notizbücher …", the search
    /// typed there.
    Menu(Option<Point>),
    All,
    Query(String),
    /// Opens a notebook (at a page) beside the active pane: in the other pane when there are two.
    Beside(String, Option<String>),
    /// The active notebook once more beside it, at the page in view.
    Again,
    /// Left and right (top and bottom) change places.
    Swap,
    /// One pane alone (⤢), or both again (None).
    Alone(Option<PaneKey>),
    Close(PaneKey),
    /// ★ to this pane.
    Main(PaneKey),
    /// The divider dragged to a share of the room; let go there.
    Drag(f32),
    Release(f32),
}

fn sm(m: Msg) -> Message {
    Message::Split(m)
}

/// The search field of "Alle Notizbücher …".
pub fn find_id() -> iced::widget::Id {
    iced::widget::Id::new("n0te-split-find")
}

/// A row of the ⧉ menu: "again", "swap", "alone", "close-other", "all", or `nb-<id>`.
pub fn menu_id(what: &str) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-split-{what}"))
}

/// The ⧉ button in the toolbar.
pub fn button_id() -> iced::widget::Id {
    iced::widget::Id::new("n0te-split-button")
}

/// A pane head's button: "main", "alone", "close", for the pane at `index`.
pub fn head_id(what: &str, index: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-pane-{index}-{what}"))
}

// --- how it looks -----------------------------------------------------------------------------

/// The split's room under the toolbar: the panes, the divider between them, the active one
/// framed; while the divider is dragged near an edge, the pane that would close says so.
pub fn view<'a>(app: &'a App, s: &'a Split) -> Element<'a, Message> {
    responsive(move |size| room(app, s, size)).into()
}

fn room<'a>(app: &'a App, s: &'a Split, size: Size) -> Element<'a, Message> {
    let maxed = s.maxed.and_then(|k| s.index(k));
    let fraction = s.dragging.unwrap_or(s.fraction);
    let rects = rects(size.width, size.height, s.panes.len(), fraction, GAP, maxed);
    let across = side_by_side(size.width, size.height);
    let both = s.is_split();
    // Always pane, gap, pane: a pane keeps its place (and its widgets' state) when the other
    // opens, closes or is shown alone.
    let slot = |i: usize| -> Element<'a, Message> {
        let (Some(p), Some(r)) = (s.panes.get(i), rects.get(i).filter(|r| !r.is_empty())) else { return Space::new().into() };
        let e = pane(app, s, p, i, r.w);
        // The first of two gets its share; the last takes what is left.
        let fixed = both && i == 0;
        match (across, fixed) {
            (true, true) => container(e).width(r.w).height(Length::Fill).into(),
            (false, true) => container(e).width(Length::Fill).height(r.h).into(),
            _ => container(e).width(Length::Fill).height(Length::Fill).into(),
        }
    };
    let gap: Element<'a, Message> = match (both, across) {
        (true, true) => Space::new().width(GAP).into(),
        (true, false) => Space::new().height(GAP).into(),
        _ => Space::new().into(),
    };
    let panes: Element<'a, Message> = if across { row![slot(0), gap, slot(1)].height(Length::Fill).into() } else { column![slot(0), gap, slot(1)].width(Length::Fill).into() };
    let divider: Element<'a, Message> = if both { widgets::divider(across, fraction.clamp(0.1, 0.9), |f| sm(Msg::Drag(f)), |f| sm(Msg::Release(f))).into() } else { Space::new().into() };
    let hint: Element<'a, Message> = match (both, s.dragging.and_then(closing).and_then(|i| rects.get(i))) {
        (true, Some(r)) => {
            let chip = container(text(theme::cased(tr("Loslassen schließt"))).font(theme::label_font()).size(14).color(Color::WHITE))
                .padding([8, 14])
                .style(|_| container::Style { background: Some(theme::p().accent.into()), border: iced::Border { radius: 14.0.into(), ..Default::default() }, ..Default::default() });
            pin(container(chip).center(Length::Fill).width(r.w).height(r.h).style(theme::scrim)).x(r.x).y(r.y).into()
        }
        _ => Space::new().into(),
    };
    stack![panes, divider, hint].width(Length::Fill).height(Length::Fill).into()
}

/// One pane in the room: its head (with two panes), its notebook, the accent frame when it is
/// the active one of two. A press or a scroll in it makes it the active one; its messages go
/// to it.
fn pane<'a>(app: &'a App, s: &'a Split, p: &'a Open, index: usize, width: f32) -> Element<'a, Message> {
    let two = s.panes.len() > 1;
    let active = s.active == p.key;
    // The same children with one pane or two: opening beside keeps this pane's state.
    let (head, rule): (Element<'a, Message>, Element<'a, Message>) = if two {
        (head(s, p, index, width), container(Space::new()).height(1).width(Length::Fill).style(theme::hairline).into())
    } else {
        (Space::new().into(), Space::new().into())
    };
    let body = column![head, rule, app.pane_view(p, active)];
    let frame: Element<'a, Message> = if s.is_split() && active {
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(|_| container::Style { border: iced::Border { color: theme::alpha(theme::p().accent, 0.75), width: 2.0, radius: 0.0.into() }, ..Default::default() })
            .into()
    } else {
        Space::new().into()
    };
    let key = p.key;
    let content: Element<'a, Message> = stack![body, frame].into();
    widgets::touched(content.map(move |m| route(key, m)), Message::Activate(key)).into()
}

/// A pane's head: ★ (main pane), its title and page, show it alone or both again, close it.
fn head<'a>(s: &'a Split, p: &'a Open, index: usize, width: f32) -> Element<'a, Message> {
    let main = s.main == p.key;
    let alone = s.maxed == Some(p.key);
    let tip = |b: Element<'a, Message>, label: &'static str| -> Element<'a, Message> {
        tooltip(b, container(text(label).size(12)).padding([3, 8]).style(theme::chip), tooltip::Position::Bottom).gap(4).into()
    };
    let glyph = |g: &'static [u8], c: Color, m: Msg, what: &str| -> Element<'a, Message> {
        container(button(shelf::icon(g, 15.0, c)).padding([4, 6]).on_press(sm(m)).style(theme::ghost)).id(head_id(what, index)).into()
    };
    let title = p.nb.title();
    let title = if title.trim().is_empty() { tr("Ohne Titel").to_string() } else { title };
    let current = p.pages.current_number(&p.camera);
    // The title gets what room the buttons leave (about 7 px a character).
    let room = ((width - 190.0) / 7.5).max(6.0) as usize;
    let content = row![
        tip(glyph(STAR, if main { theme::p().accent } else { theme::p().muted }, Msg::Main(p.key), "main"), if main { tr("Hauptbereich: Live, Folgen, Herbeirufen") } else { tr("Zum Hauptbereich machen") }),
        Space::new().width(4),
        text(fit(&title, room)).font(theme::SANS_MEDIUM).size(13).color(theme::p().text).wrapping(text::Wrapping::None),
        text(theme::cased(&format!(" · {current}/{}", p.pages.total_pages))).font(theme::meta_font()).size(11).color(theme::p().muted).wrapping(text::Wrapping::None),
        Space::new().width(Length::Fill),
        tip(glyph(if alone { RESTORE } else { MAXIMIZE }, theme::p().text, Msg::Alone(if alone { None } else { Some(p.key) }), "alone"), if alone { tr("Beide zeigen") } else { tr("Nur diesen zeigen") }),
        tip(glyph(CLOSE, theme::p().text, Msg::Close(p.key), "close"), tr("Schließen")),
    ]
    .align_y(Alignment::Center);
    container(content).padding([0, 6]).height(32).width(Length::Fill).center_y(32).style(theme::chrome).into()
}

/// The ⧉ button for the toolbar: its menu opens where it was clicked.
pub fn menu_button<'a>(open: bool) -> Element<'a, Message> {
    let b = button(shelf::icon(SPLIT, 16.0, if open { theme::p().chrome } else { theme::p().text })).padding([5, 8]).style(theme::tab(open));
    let b = container(widgets::pointer(b).on_left(|at| sm(Msg::Menu(Some(at))))).id(button_id());
    tooltip(b, container(text(tr("Daneben öffnen")).size(12)).padding([3, 8]).style(theme::chip), tooltip::Position::Bottom).gap(4).into()
}

/// Width of the ⧉ menu.
const MENU_W: f32 = 300.0;

/// Over the notebook: where a dragged page would open and the page under the pointer, the ⧉
/// menu (always these layers, so what is under them keeps its state).
pub fn layers<'a>(app: &'a App, s: &'a Split, base: Element<'a, Message>) -> Element<'a, Message> {
    let drag: Element<'a, Message> = match (&s.drag, s.drag.as_ref().and_then(|d| s.drop_zone(d.from))) {
        (Some(d), Some(zone)) => {
            let hot = zone.contains(d.at);
            let label = container(text(theme::cased(if s.is_split() { tr("Hier öffnen") } else { tr("Daneben öffnen") })).font(theme::label_font()).size(14).color(if hot { Color::WHITE } else { theme::p().text }))
                .padding([9, 16])
                .style(move |_| container::Style {
                    background: Some((if hot { theme::p().accent } else { theme::p().chrome }).into()),
                    border: iced::Border { radius: 14.0.into(), ..Default::default() },
                    ..Default::default()
                });
            let area = container(label).center(Length::Fill).width(zone.width - 28.0).height(zone.height - 28.0).style(move |_| container::Style {
                background: Some(theme::alpha(theme::p().accent, if hot { 0.22 } else { 0.07 }).into()),
                border: iced::Border { color: theme::alpha(theme::p().accent, if hot { 1.0 } else { 0.5 }), width: if hot { 3.0 } else { 1.0 }, radius: 22.0.into() },
                ..Default::default()
            });
            let chip = container(row![shelf::icon(PAGE, 14.0, theme::p().accent), text(fit(&d.name, 28)).font(theme::SANS_MEDIUM).size(13).color(theme::p().text)].spacing(8).align_y(Alignment::Center))
                .padding([7, 12])
                .style(theme::menu);
            stack![pin(area).x(zone.x + 14.0).y(zone.y + 14.0), pin(chip).x(d.at.x + 14.0).y(d.at.y + 10.0)].into()
        }
        _ => Space::new().into(),
    };
    let menu: Element<'a, Message> = match &s.menu {
        Some(m) => stack![
            widgets::dismiss(sm(Msg::Menu(None))),
            responsive(move |size| {
                let x = m.at.x.min(size.width - MENU_W - 8.0).max(4.0);
                let y = (m.at.y + 8.0).min(size.height - 120.0).max(4.0);
                pin(menu_sheet(app, s, m)).x(x).y(y).into()
            }),
        ]
        .into(),
        None => Space::new().into(),
    };
    stack![base, drag, menu].into()
}

/// The ⧉ menu: with two panes swap, one alone, close the other; then what can open beside (this
/// notebook again, the ones opened lately, all of them with a search).
fn menu_sheet<'a>(app: &'a App, s: &'a Split, m: &'a Menu) -> Element<'a, Message> {
    let me = s.active_pane();
    let other = s.other(me.key).and_then(|k| s.pane(k));
    let section = |t: &str| container(text(theme::cased(t)).font(theme::label_font()).size(theme::label_size() - 1.0).color(theme::p().muted)).padding([6, 10]);
    let item = |glyph: &'static [u8], label: String, sub: Option<&'static str>, on: Message, id: iced::widget::Id| {
        let mut words = column![text(label).font(theme::SANS).size(13.5).wrapping(text::Wrapping::None)];
        if let Some(sub) = sub {
            words = words.push(text(sub).size(11.5).color(theme::p().muted));
        }
        container(button(row![shelf::icon(glyph, 15.0, theme::p().muted), words].spacing(11).align_y(Alignment::Center)).padding([7, 10]).width(Length::Fill).on_press(on).style(theme::menu_row(false))).id(id)
    };
    let rule = || container(container(Space::new()).height(1).width(Length::Fill).style(theme::hairline)).padding([4, 6]);
    let mut col = column![].spacing(1);
    match other {
        Some(o) => {
            col = col.push(section(tr("Zwei nebeneinander")));
            col = col.push(item(SPLIT, tr("Seiten tauschen").into(), None, sm(Msg::Swap), menu_id("swap")));
            col = col.push(match s.maxed {
                None => item(MAXIMIZE, tr("Nur diesen zeigen").into(), None, sm(Msg::Alone(Some(me.key))), menu_id("alone")),
                Some(_) => item(RESTORE, tr("Beide zeigen").into(), None, sm(Msg::Alone(None)), menu_id("alone")),
            });
            col = col.push(item(CLOSE, tr("Den anderen schließen").into(), None, sm(Msg::Close(o.key)), menu_id("close-other")));
            col = col.push(rule());
            col = col.push(section(tr("Im anderen Bereich öffnen")));
        }
        None => col = col.push(section(tr("Daneben öffnen"))),
    }
    col = col.push(item(COPY, tr("Dieses Notizbuch nochmal").into(), Some(tr("Zwei Stellen zugleich")), sm(Msg::Again), menu_id("again")));
    let others: Vec<&crate::app::Entry> = notebooks(app).into_iter().filter(|e| e.id != me.id && other.is_none_or(|o| o.id != e.id)).collect();
    let name = |e: &crate::app::Entry| if e.title.trim().is_empty() { tr("Ohne Titel").to_string() } else { fit(&e.title, 34) };
    if !m.all {
        for e in others.iter().take(5) {
            col = col.push(item(PAGE, name(e), None, sm(Msg::Beside(e.id.clone(), None)), menu_id(&format!("nb-{}", e.id))));
        }
        if others.len() > 5 {
            col = col.push(item(LOUPE, tr("Alle Notizbücher …").into(), None, sm(Msg::All), menu_id("all")));
        }
    } else {
        let field = widgets::on_escape(
            text_input(tr("Suchen"), &m.query).id(find_id()).on_input(|q| sm(Msg::Query(q))).padding([8, 10]).size(14).style(theme::field),
            sm(Msg::Menu(None)),
        );
        col = col.push(container(field).padding([6, 6]));
        let shown: Vec<&&crate::app::Entry> = others.iter().filter(|e| shelf::matches(e, &m.query)).collect();
        let mut list = column![].spacing(1);
        for e in &shown {
            list = list.push(item(PAGE, name(e), None, sm(Msg::Beside(e.id.clone(), None)), menu_id(&format!("nb-{}", e.id))));
        }
        if shown.is_empty() {
            list = list.push(container(text(tr("Nichts gefunden")).size(13).color(theme::p().muted)).padding([8, 10]));
        }
        col = col.push(container(scrollable(list).style(theme::scroll_dark)).max_height(360.0));
    }
    container(col).padding(5).width(MENU_W).style(theme::menu).into()
}

/// The library's notebooks, the ones opened here lately first.
fn notebooks(app: &App) -> Vec<&crate::app::Entry> {
    let opened = |id: &str| app.settings.lib_opened.iter().find(|(i, _)| i == id).map(|(_, t)| *t).unwrap_or(0);
    let mut all: Vec<&crate::app::Entry> = app.entries.iter().collect();
    all.sort_by(|a, b| opened(&b.id).cmp(&opened(&a.id)).then_with(|| a.title.to_lowercase().cmp(&b.title.to_lowercase())));
    all
}

// Line glyphs on a 24 × 24 grid, drawn like the tablet's (Icons.kt).
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

/// Two panes side by side: the ⧉ button, "Daneben öffnen", "Seiten tauschen".
pub const SPLIT: &[u8] = glyph!("M3 5h7.5v14H3ZM13.5 5H21v14h-7.5Z");
pub const STAR: &[u8] = glyph!("M12 3.5 14.35 9.26 20.56 9.72 15.8 13.74 17.29 19.78 12 16.5 6.71 19.78 8.2 13.74 3.44 9.72 9.65 9.26Z");
pub const MAXIMIZE: &[u8] = glyph!("M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5");
pub const RESTORE: &[u8] = glyph!("M9 4v5H4M15 4v5h5M20 15h-5v5M4 15h5v5");
pub const CLOSE: &[u8] = glyph!("M6 6 18 18M18 6 6 18");
const COPY: &[u8] = glyph!("M9 8h10v12H9ZM5 16V4h10");
const PAGE: &[u8] = glyph!("M6 3h12v18H6ZM9 8h6M9 12h6M9 16h4");
const LOUPE: &[u8] = glyph!("M15.3 15.3 20.5 20.5M17 10.5a6.5 6.5 0 1 1-13 0 6.5 6.5 0 0 1 13 0Z");

// --- what the split's controls do -------------------------------------------------------------

impl App {
    pub(crate) fn on_split(&mut self, m: Msg) -> Task<Message> {
        let Screen::Notebook(s) = &mut self.screen else { return Task::none() };
        match m {
            Msg::Menu(at) => s.menu = at.map(|at| Menu { at, all: false, query: String::new() }),
            Msg::All => {
                if let Some(m) = &mut s.menu {
                    m.all = true;
                }
                return iced::widget::operation::focus(find_id());
            }
            Msg::Query(q) => {
                if let Some(m) = &mut s.menu {
                    m.query = q;
                }
            }
            Msg::Again => {
                let current = s.pages.index_of_number(s.pages.current_number(&s.camera)).map(|i| s.pages.pages[i].placed.sheet_id.clone());
                let id = s.id.clone();
                return self.open_beside(&id, current);
            }
            Msg::Beside(id, sheet) => return self.open_beside(&id, sheet),
            Msg::Swap => {
                s.menu = None;
                if s.panes.len() == 2 {
                    s.panes.swap(0, 1);
                    s.fraction = 1.0 - s.fraction;
                }
            }
            Msg::Alone(key) => {
                s.menu = None;
                s.maxed = key.filter(|k| s.panes.len() > 1 && s.index(*k).is_some());
                if let Some(k) = s.maxed {
                    self.activate(k);
                }
            }
            Msg::Close(key) => self.close_pane(key),
            Msg::Main(key) => self.set_main(key),
            Msg::Drag(f) => s.dragging = Some(f.clamp(0.0, 1.0)),
            Msg::Release(f) => {
                s.dragging = None;
                match release(f) {
                    Release::Snap(x) => s.fraction = x,
                    Release::Close(i) => {
                        if let Some(k) = s.panes.get(i).map(|p| p.key) {
                            self.close_pane(k);
                        }
                    }
                }
            }
        }
        Task::none()
    }

    /// Opens notebook `id` (at page `sheet`) beside the pane messages are for: in the other
    /// pane when there are two (both shown again), else as a second pane after it.
    pub(crate) fn open_beside(&mut self, id: &str, sheet: Option<String>) -> Task<Message> {
        let Screen::Notebook(s) = &mut self.screen else { return Task::none() };
        s.menu = None;
        let from = s.key();
        if let Some(other) = s.other(from) {
            s.maxed = None;
            let was_main = s.main == other;
            if s.pane(other).is_some_and(|p| p.id == id) {
                if let Some(sheet) = sheet {
                    self.show_sheet(other, &sheet);
                }
            } else {
                match self.replace_pane(other, id, sheet) {
                    // ★ there: live with the tablet goes on with the new notebook.
                    Ok(()) if was_main => {
                        self.live_follows_main();
                        self.send_presence_all();
                    }
                    Ok(()) => {}
                    Err(e) => self.say(trf("Notizbuch nicht lesbar: {}", &[&e])),
                }
            }
            self.opened(id);
            return Task::none();
        }
        let key = self.new_key();
        match self.make_pane(key, id) {
            Ok(mut p) => {
                // The second pane starts without its outline: the room goes to the page.
                p.sidebar = false;
                p.start = sheet;
                if let Screen::Notebook(s) = &mut self.screen {
                    let at = s.index(from).map(|i| i + 1).unwrap_or(s.panes.len());
                    s.panes.insert(at, p);
                }
                self.watch(id);
                self.opened(id);
                if !self.settings.split_seen {
                    self.settings.split_seen = true;
                    if self.persist_settings {
                        self.settings.save();
                    }
                    self.say(tr("Zwei nebeneinander: der Strich in der Mitte ändert die Breite, ★ ist für Live und Gäste"));
                }
            }
            Err(e) => self.say(trf("Notizbuch nicht lesbar: {}", &[&e])),
        }
        Task::none()
    }

    /// A page dragged out of the outline of the pane messages are for: let go on the drop zone,
    /// it opens there.
    pub(crate) fn drag_page(&mut self, sheet: String, name: String, d: widgets::Drag) -> Task<Message> {
        let Screen::Notebook(s) = &mut self.screen else { return Task::none() };
        match d {
            widgets::Drag::Start(at) | widgets::Drag::Move(at) => s.drag = Some(PageDrag { from: s.key(), sheet, name, at }),
            widgets::Drag::End => {
                let Some(d) = s.drag.take() else { return Task::none() };
                if s.drop_zone(d.from).is_some_and(|z| z.contains(d.at)) {
                    let id = s.id.clone();
                    return self.open_beside(&id, Some(d.sheet));
                }
            }
        }
        Task::none()
    }

    /// Shows page `sheet` in pane `key` (once its view knows its size, if it doesn't yet).
    fn show_sheet(&mut self, key: PaneKey, sheet: &str) {
        let Screen::Notebook(s) = &mut self.screen else { return };
        let Some(p) = s.pane_mut(key) else { return };
        match p.pages.page_of_sheet(sheet) {
            Some(i) if p.camera.viewport.width > 0.0 => p.camera = p.pages.camera_for(i, &p.camera),
            _ => p.start = Some(sheet.to_string()),
        }
    }

    /// Puts notebook `id` (at page `sheet`) where pane `key` is: ★ and the active frame stay in
    /// that place.
    pub(crate) fn replace_pane(&mut self, key: PaneKey, id: &str, sheet: Option<String>) -> Result<(), String> {
        let fresh = self.new_key();
        let mut p = self.make_pane(fresh, id)?;
        p.start = sheet;
        let old = self.leave_pane(key);
        let Screen::Notebook(s) = &mut self.screen else { return Ok(()) };
        let Some(i) = s.index(key) else { return Ok(()) };
        p.sidebar = s.panes[i].sidebar;
        s.panes[i] = p;
        if s.main == key {
            s.main = fresh;
        }
        if s.active == key {
            s.active = fresh;
        }
        if s.maxed == Some(key) {
            s.maxed = None;
        }
        self.watch(id);
        if let Some(nb) = old {
            self.gone(&nb);
        }
        Ok(())
    }

    /// Closes pane `key`; the other stays, alone (it becomes active and ★ if this one was).
    pub(crate) fn close_pane(&mut self, key: PaneKey) {
        let Screen::Notebook(s) = &self.screen else { return };
        if s.panes.len() < 2 || s.index(key).is_none() {
            return;
        }
        let rest = s.other(key).unwrap_or(key);
        if s.active == key {
            self.activate(rest);
        }
        let old = self.leave_pane(key);
        let Screen::Notebook(s) = &mut self.screen else { return };
        s.panes.retain(|p| p.key != key);
        s.maxed = None;
        s.menu = None;
        s.dragging = None;
        let was_main = s.main == key;
        if was_main {
            s.main = rest;
        }
        if let Some(nb) = old {
            self.gone(&nb);
        }
        if was_main {
            self.live_follows_main();
            self.send_presence_all();
        }
    }

    /// Before pane `key` goes: the text being written there is saved, its graph editor closes.
    /// Returns its notebook.
    fn leave_pane(&mut self, key: PaneKey) -> Option<String> {
        if self.editing.as_ref().is_some_and(|e| e.pane == key) {
            self.finish_text();
        }
        if self.plot_edit.is_some() && self.plot_pane == key {
            self.plot_edit = None;
        }
        self.in_pane(key, |app| {
            app.close_text_page();
            app.notebook_id()
        })
        .flatten()
    }

    /// ★ to pane `key`: live with the tablet (following or followed) goes with it.
    fn set_main(&mut self, key: PaneKey) {
        let Screen::Notebook(s) = &mut self.screen else { return };
        if s.index(key).is_none() {
            return;
        }
        if s.main == key {
            self.say(tr("★ Hauptbereich: Live, Folgen und Herbeirufen gehören hierher"));
            return;
        }
        let before = s.pane(s.main).map(|p| p.id.clone());
        s.main = key;
        if before.as_deref() != s.pane(key).map(|p| p.id.as_str()) {
            self.live_follows_main();
        }
        self.send_presence_all();
        self.say(tr("★ Live und Gäste sehen jetzt diesen Bereich"));
    }

    /// Pane `key` is the active one now: the toolbar works on it. Typing in a text box ends in
    /// the pane before (the keyboard goes with the pane), its menus and the toolbar's panels close.
    pub(crate) fn activate(&mut self, key: PaneKey) {
        let Screen::Notebook(s) = &mut self.screen else { return };
        if s.active == key || s.index(key).is_none() {
            return;
        }
        let before = s.active;
        if let Some(p) = s.pane_mut(before) {
            p.outline.menu = None;
            p.outline.asked = None;
            p.outline.renaming = None;
        }
        s.active = key;
        s.retarget(key);
        if self.editing.as_ref().is_some_and(|e| e.pane == before) {
            self.finish_text();
        }
        self.close_panels();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_panes_share_the_room_side_by_side_or_stacked() {
        let r = rects(1000.0, 600.0, 2, 0.5, 6.0, None);
        assert_eq!(r[0], Area { x: 0.0, y: 0.0, w: 497.0, h: 600.0 });
        assert_eq!(r[1], Area { x: 503.0, y: 0.0, w: 497.0, h: 600.0 });
        // Upright: one above the other.
        let r = rects(600.0, 1000.0, 2, 1.0 / 3.0, 6.0, None);
        assert_eq!(r[0], Area { x: 0.0, y: 0.0, w: 600.0, h: 330.0 });
        assert_eq!(r[1], Area { x: 0.0, y: 336.0, w: 600.0, h: 664.0 });
        // One pane, or one shown alone: the whole room.
        assert_eq!(rects(800.0, 600.0, 1, 0.3, 6.0, None), vec![Area { x: 0.0, y: 0.0, w: 800.0, h: 600.0 }]);
        let r = rects(800.0, 600.0, 2, 0.3, 6.0, Some(1));
        assert!(r[0].is_empty() && r[1] == Area { x: 0.0, y: 0.0, w: 800.0, h: 600.0 });
        // The share stays between a tenth and nine tenths.
        assert_eq!(rects(1000.0, 600.0, 2, 0.01, 6.0, None)[0].w, 97.0);
        assert!(rects(0.0, 0.0, 0, 0.5, 6.0, None).is_empty());
    }

    #[test]
    fn the_divider_snaps_to_thirds_and_the_half_and_closes_at_an_edge() {
        assert_eq!(release(0.5), Release::Snap(0.5));
        assert_eq!(release(0.41), Release::Snap(1.0 / 3.0));
        assert_eq!(release(0.43), Release::Snap(0.5));
        assert_eq!(release(0.6), Release::Snap(2.0 / 3.0));
        assert_eq!(release(0.2), Release::Snap(1.0 / 3.0));
        assert_eq!(release(0.85), Release::Snap(2.0 / 3.0));
        assert_eq!(release(0.1), Release::Close(0));
        assert_eq!(release(0.9), Release::Close(1));
        assert_eq!((closing(0.13), closing(0.15), closing(0.5), closing(0.87)), (Some(0), None, None, Some(1)));
    }
}
