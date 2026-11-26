//! n0te Tablet's tool bar (under the notebook's top bar), laid out like the Android app's:
//! tools, the pen type, six colours of the current palette, four widths, the ruler, the writing
//! magnifier, a board on its own, undo / redo; with a selection, its actions. Big enough for a
//! finger or the pen.

use iced::widget::{Space, button, column, container, mouse_area, row, text};
use iced::{Alignment, Border, Color, Element, Length, Theme};
use n0te_core::pens::{self, PenType};

use super::{EraserMode, MAX_FAVOURITES, Msg, Tablet, Tool};
use crate::app::Message;
use crate::pages::argb;
use crate::i18n::{tr, trf};
use crate::theme;

/// Touch target size (px).
const TARGET: f32 = 40.0;

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

const UNDO: &[u8] = glyph!("M9 14 4 9l5-5M4 9h10.5a5.5 5.5 0 0 1 0 11H11");
const REDO: &[u8] = glyph!("M15 14l5-5-5-5M20 9H9.5a5.5 5.5 0 0 0 0 11H13");

/// A colour that reads on the dark chrome: very dark inks show in the text colour.
fn visible_on_chrome(c: Color) -> Color {
    let lum = 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
    let bg = theme::p().chrome;
    let bg_lum = 0.2126 * bg.r + 0.7152 * bg.g + 0.0722 * bg.b;
    if (lum - bg_lum).abs() < 0.18 { theme::p().text } else { c }
}

fn icon_button<'a>(glyph: &'static [u8], m: Option<Msg>, label: &'a str) -> Element<'a, Message> {
    let color = if m.is_some() { theme::p().text } else { theme::p().muted };
    crate::widgets::tip(button(crate::shelf::icon(glyph, 20.0, color)).padding(10).on_press_maybe(m.map(msg)).style(theme::ghost), label)
}

fn tool_label(t: Tool) -> &'static str {
    match t {
        Tool::Pen => tr("Stift"),
        Tool::Marker => tr("Marker"),
        Tool::Eraser => tr("Radierer"),
        Tool::Lasso => tr("Lasso"),
        Tool::Text => tr("Text"),
        Tool::Laser => tr("Laser"),
        Tool::Space => tr("Platz"),
        Tool::Fill => tr("Füllen"),
    }
}

/// What the bar shows besides the tools: whether the page in the middle is a board (and open
/// on its own).
#[derive(Debug, Clone, Copy, Default)]
pub struct Bar {
    /// None: no board there; Some(open): a board, open on its own or not.
    pub board: Option<bool>,
    /// The window's width (logical px; 0: unknown): too narrow for one row, the bar takes two.
    pub width: f32,
}

pub fn pen_label(p: PenType) -> &'static str {
    match p {
        PenType::Fountain => tr("Füller"),
        PenType::Ballpoint => tr("Kugelschreiber"),
        PenType::Pencil => tr("Bleistift"),
        PenType::Brush => tr("Pinsel"),
    }
}

fn pen_hint(p: PenType) -> &'static str {
    match p {
        PenType::Fountain => tr("Strich folgt dem Druck"),
        PenType::Ballpoint => tr("Gleichmäßiger Strich"),
        PenType::Pencil => tr("Graphit: weich am Rand, dunkler bei Druck"),
        PenType::Brush => tr("Stark druckabhängig"),
    }
}

fn msg(m: Msg) -> Message {
    Message::Tablet(m)
}

/// A round colour swatch; ringed when chosen.
/// A favourite pen: its colour, about its width (a marker as a bar), ringed when it is the pen.
fn favourite_button<'a>(c: u32, width: f32, marker: bool, on: bool, i: usize) -> Element<'a, Message> {
    let fill = if marker { Color { a: 0.55, ..argb(c | 0xFF00_0000) } } else { argb(c | 0xFF00_0000) };
    let d = (3.0 + width * 2.2).clamp(5.0, 20.0);
    let (w, h, r) = if marker { (18.0, d.min(10.0), 2.0) } else { (d, d, d / 2.0) };
    let mark = container(Space::new().width(w).height(h)).style(move |_: &Theme| container::Style { background: Some(fill.into()), border: Border { radius: r.into(), ..Default::default() }, ..Default::default() });
    let ring = theme::p().accent;
    button(container(mark).width(28).height(28).center_x(28).center_y(28).style(move |_: &Theme| container::Style {
        border: Border { radius: 14.0.into(), width: if on { 2.0 } else { 1.0 }, color: if on { ring } else { theme::alpha(Color::WHITE, 0.25) } },
        ..Default::default()
    }))
    .padding(3)
    .on_press(msg(Msg::Favourite(i)))
    .style(theme::ghost)
    .into()
}

fn swatch<'a>(c: u32, on: bool, i: usize) -> Element<'a, Message> {
    let fill = argb(c | 0xFF00_0000);
    let dot = container(Space::new().width(22).height(22)).style(move |_: &Theme| container::Style {
        background: Some(fill.into()),
        border: Border { radius: 11.0.into(), width: 1.0, color: theme::alpha(Color::WHITE, 0.25) },
        ..Default::default()
    });
    let ring = theme::p().accent;
    button(container(dot).padding(3).style(move |_: &Theme| container::Style {
        border: Border { radius: 15.0.into(), width: if on { 2.0 } else { 0.0 }, color: ring },
        ..Default::default()
    }))
    .padding(4)
    .on_press(msg(Msg::Color(i)))
    .style(theme::ghost)
    .into()
}

/// A width preset: a bar as thick as the line (scaled), ringed when chosen.
fn width_button<'a>(w: f32, on: bool, i: usize, color: Color) -> Element<'a, Message> {
    let h = (w * 2.2).clamp(2.0, 14.0);
    let bar = container(Space::new().width(22).height(h)).style(move |_: &Theme| container::Style { background: Some(color.into()), border: Border { radius: (h / 2.0).into(), ..Default::default() }, ..Default::default() });
    button(container(bar).center(Length::Fixed(TARGET - 8.0))).padding(2).on_press(msg(Msg::Width(i))).style(theme::option(on)).into()
}

fn tool_button<'a>(label: &str, on: bool, m: Msg) -> Element<'a, Message> {
    button(text(theme::cased(label)).font(theme::label_font()).size(theme::label_size() + 1.0)).padding([10, 12]).on_press(msg(m)).style(theme::option(on)).into()
}

/// About how wide a button with `label` is (the mono designs' capitals are wide).
fn label_w(label: &str, size: f32, pad: f32) -> f32 {
    label.chars().count() as f32 * size * 0.62 + pad + 4.0
}

/// The bar itself: tools, colours, widths (the eraser's two ways instead while it is chosen),
/// pasting, the ruler, magnifier and board, undo / redo (what a selection can do floats over the
/// page: [`selection_bar`]). Where one row can't hold all of that (a small pen laptop, German
/// labels; counted with the widest of those choices, so the bar never changes its height when a
/// tool is picked), the colours and widths get a second row, and undo and redo stay in reach.
pub fn view(t: &Tablet, extra: Bar) -> Element<'_, Message> {
    let mut tools = row![].spacing(4).align_y(Alignment::Center);
    let mut tools_w = 0.0;
    let big = theme::label_size() + 1.0;
    // "Stift" shows the pen type and opens its menu when already chosen.
    let pen_on = t.tool == Tool::Pen;
    let pen_msg = if pen_on { Msg::PenMenu(!t.pen_menu) } else { Msg::Tool(Tool::Pen) };
    let pen = format!("{} ▾", pen_label(t.pen));
    tools_w += label_w(&pen, big, 24.0);
    tools = tools.push(tool_button(&pen, pen_on, pen_msg));
    for tool in [Tool::Marker, Tool::Eraser, Tool::Lasso, Tool::Space, Tool::Text, Tool::Laser, Tool::Fill] {
        tools_w += label_w(tool_label(tool), big, 24.0);
        tools = tools.push(tool_button(tool_label(tool), t.tool == tool, Msg::Tool(tool)));
    }
    let mut colors = row![].spacing(0).align_y(Alignment::Center);
    let pal = pens::PALETTES[t.palette % pens::PALETTES.len()];
    for (i, c) in pal.iter().enumerate() {
        colors = colors.push(swatch(*c, i == t.color && matches!(t.tool, Tool::Pen | Tool::Marker), i));
    }
    let ink = visible_on_chrome(argb(t.ink() | 0xFF00_0000));
    // The pens' widths; the eraser erases whole strokes or just where it goes instead.
    let mut widths = row![].spacing(2).align_y(Alignment::Center);
    if t.tool == Tool::Eraser {
        widths = widths.push(tool_button(tr("Ganze Striche"), t.eraser_mode == EraserMode::Stroke, Msg::EraserMode(EraserMode::Stroke)));
        widths = widths.push(tool_button(tr("Fläche"), t.eraser_mode == EraserMode::Area, Msg::EraserMode(EraserMode::Area)));
    } else {
        for (i, w) in t.widths.iter().enumerate() {
            widths = widths.push(width_button(*w, i == t.width, i, ink));
        }
    }
    let widths_w = (t.widths.len() as f32 * 38.0).max(label_w(tr("Ganze Striche"), big, 24.0) + label_w(tr("Fläche"), big, 24.0));
    // Favourite pens: a click picks one, a right click (or the pen's side button) lets it go, "+"
    // keeps the pen as it is now.
    let mut favs = row![].spacing(2).align_y(Alignment::Center);
    let now = t.current_favourite();
    for (i, f) in t.favourites.iter().enumerate() {
        let c = pens::PALETTES[f.palette % pens::PALETTES.len()][f.color % 6];
        let w = t.widths[f.width % 4];
        favs = favs.push(mouse_area(favourite_button(c, w, f.marker, now == Some(*f), i)).on_right_press(msg(Msg::RemoveFavourite(i))));
    }
    if now.is_some_and(|f| !t.favourites.contains(&f)) && t.favourites.len() < MAX_FAVOURITES {
        favs = favs.push(button(text("+").size(big)).padding([2, 8]).on_press(msg(Msg::AddFavourite)).style(theme::ghost));
    }
    let favs_w = (t.favourites.len() + 1) as f32 * 36.0 + 10.0;
    let paint_w = 14.0 + pal.len() as f32 * 36.0 + 10.0 + widths_w + favs_w;
    // Pasting what was copied (a selection's own actions float over the page).
    let small = theme::label_size() - 1.5;
    let pad = theme::pad().left + theme::pad().right;
    let mut actions = row![].align_y(Alignment::Center);
    let actions_w = label_w(tr("Einfügen"), small, pad) + 8.0;
    if t.selection.is_none() && !t.clip_is_empty() {
        actions = actions.push(button(theme::btn_small(tr("Einfügen"))).padding(theme::pad()).on_press(msg(Msg::Paste)).style(theme::ghost)).push(Space::new().width(8));
    }
    // The ruler, the writing magnifier and a board on its own (like the tablet's right group).
    // The instrument: a click lays it down / takes it away, "▾" picks which one.
    let laid = t.ruler.is_some() || t.compass.is_some();
    let mut extras = row![
        tool_button(instrument_label(t.instrument), laid, Msg::Ruler(!laid)),
        button(text("▾").size(theme::label_size())).padding([2, 6]).on_press(msg(Msg::InstrumentMenu(!t.instrument_menu))).style(theme::ghost)
    ]
    .spacing(0)
    .align_y(Alignment::Center);
    let extras_w = label_w(tr("Lineal"), big, 24.0) + label_w(tr("Lupe"), big, 24.0) + label_w(tr("Board"), big, 24.0);
    if extra.board != Some(true) {
        extras = extras.push(tool_button(tr("Lupe"), t.loupe.is_some(), Msg::Loupe(t.loupe.is_none())));
    }
    if let Some(open) = extra.board {
        extras = extras.push(tool_button(tr("Board"), open, Msg::Board(!open)));
    }
    let undo = icon_button(UNDO, t.can_undo().then_some(Msg::Undo), tr("Rückgängig (Strg Z)"));
    let redo = icon_button(REDO, t.can_redo().then_some(Msg::Redo), tr("Wiederholen (Strg Y)"));
    let end = row![extras, Space::new().width(8), undo, redo].align_y(Alignment::Center);
    // The widest the bar gets: the eraser's buttons, "Einfügen", the board's.
    let one_row = tools_w + paint_w + actions_w + extras_w + 8.0 + 2.0 * 40.0 + 20.0;
    let bar: Element<'_, Message> = if extra.width <= 0.0 || one_row <= extra.width {
        row![tools, Space::new().width(14), colors, Space::new().width(10), widths, Space::new().width(10), favs, Space::new().width(Length::Fill), actions, end].align_y(Alignment::Center).into()
    } else {
        column![
            row![tools, Space::new().width(Length::Fill), end].align_y(Alignment::Center),
            row![colors, Space::new().width(10), widths, Space::new().width(10), favs, Space::new().width(Length::Fill), actions].align_y(Alignment::Center),
        ]
        .spacing(2)
        .into()
    };
    container(bar).padding([4, 10]).width(Length::Fill).style(theme::chrome).into()
}

/// What the selection can do, in a small bar over the page above (or below) it: how many
/// strokes, copy, cut, delete, done. `at`: its place in the pane (see
/// [`App::selection_bar`](crate::app::App)).
pub fn selection_bar<'a>(t: &'a Tablet) -> Option<Element<'a, Message>> {
    let sel = t.selection.as_ref()?;
    let mut bar = row![text(trf("{} ausgewählt", &[&sel.ids.len()])).font(theme::meta_font()).size(theme::SMALL).color(theme::p().muted), Space::new().width(6)].spacing(2).align_y(Alignment::Center);
    for (l, m) in [(tr("Kopieren"), Msg::Copy), (tr("Ausschneiden"), Msg::Cut), (tr("Löschen"), Msg::DeleteSelection), (tr("Fertig"), Msg::ClearSelection)] {
        bar = bar.push(button(theme::btn_small(l)).padding(theme::pad()).on_press(msg(m)).style(theme::ghost));
    }
    Some(container(bar).padding([4, 10]).style(theme::menu).into())
}

/// How big [`selection_bar`] is (logical px), for its place and to keep the pen off it.
pub fn selection_bar_size(t: &Tablet) -> Option<iced::Size> {
    let sel = t.selection.as_ref()?;
    let small = theme::label_size() - 1.5;
    let pad = theme::pad().left + theme::pad().right;
    let w = trf("{} ausgewählt", &[&sel.ids.len()]).chars().count() as f32 * 7.2 + 8.0 + [tr("Kopieren"), tr("Ausschneiden"), tr("Löschen"), tr("Fertig")].iter().map(|l| label_w(l, small, pad) + 2.0).sum::<f32>() + 20.0;
    Some(iced::Size::new(w, 40.0))
}

/// The pen-type menu (under the bar, left).
/// The instrument's name in the bar.
fn instrument_label(k: u8) -> &'static str {
    match k {
        1 => tr("Geodreieck"),
        2 => tr("Winkelmesser"),
        3 => tr("Zirkel"),
        _ => tr("Lineal"),
    }
}

/// Which instrument the ruler button lays down.
pub fn instrument_menu(t: &Tablet) -> Element<'_, Message> {
    let mut col = column![].spacing(2);
    for k in 0..4u8 {
        col = col.push(button(text(instrument_label(k)).size(15).color(theme::p().text)).padding([8, 12]).width(Length::Fixed(220.0)).on_press(msg(Msg::Instrument(k))).style(theme::row(k == t.instrument)));
    }
    let hint = if t.instrument == 3 { tr("Stift setzen: Nadel · ziehen: Radius · dann rundherum zeichnen") } else { tr("Ein Finger schiebt, zwei drehen; der Stift folgt jeder Kante") };
    col = col.push(text(hint).size(11.5).color(theme::p().muted).width(Length::Fixed(220.0)));
    container(col).padding(8).style(theme::menu).into()
}

pub fn pen_menu(t: &Tablet) -> Element<'_, Message> {
    let mut col = column![].spacing(2);
    for p in PenType::ALL {
        col = col.push(
            button(column![text(pen_label(p)).size(15).color(theme::p().text), text(pen_hint(p)).size(11.5).color(theme::p().muted)].spacing(2))
                .padding([8, 12])
                .width(Length::Fixed(260.0))
                .on_press(msg(Msg::Pen(p)))
                .style(theme::row(p == t.pen)),
        );
    }
    col = col.push(Space::new().height(6));
    // The line: solid, dashed, dotted, dash-dot (pencils stay solid).
    let mut dashes = row![].spacing(4);
    for (d, label) in [(0u8, "────"), (1, "─ ─ ─"), (2, "· · · ·"), (3, "─ · ─")] {
        dashes = dashes.push(button(text(label).size(12)).padding([4, 8]).on_press(msg(Msg::Dash(d))).style(theme::option(t.dash == d)));
    }
    col = col.push(dashes);
    col = col.push(Space::new().height(6));
    let mut pals = row![].spacing(4);
    for (i, name) in pens::PALETTE_NAMES.iter().enumerate() {
        pals = pals.push(button(text(tr(name)).size(11.5)).padding([4, 8]).on_press(msg(Msg::Palette(i))).style(theme::option(i == t.palette)));
    }
    col = col.push(
        iced::widget::toggler(t.finger_draws).label(tr("Mit dem Finger schreiben")).on_toggle(|on| msg(Msg::FingerDraws(on))).size(18).text_size(13).spacing(10).style(theme::switch),
    );
    col = col.push(
        iced::widget::toggler(t.write_flow).label(tr("Mitwandern beim Schreiben")).on_toggle(|on| msg(Msg::WriteFlow(on))).size(18).text_size(13).spacing(10).style(theme::switch),
    );
    col = col.push(iced::widget::scrollable(pals).direction(iced::widget::scrollable::Direction::Horizontal(Default::default())).width(Length::Fixed(260.0)));
    container(col).padding(8).style(theme::menu).into()
}
