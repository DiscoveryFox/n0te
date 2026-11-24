//! n0te Tablet's tool bar (under the notebook's top bar), laid out like the Android app's:
//! tools, the pen type, six colours of the current palette, four widths, undo / redo; with a
//! selection, its actions. Big enough for a finger or the pen.

use iced::widget::{Space, button, column, container, row, text};
use iced::{Alignment, Border, Color, Element, Length, Theme};
use n0te_core::pens::{self, PenType};

use super::{EraserMode, Msg, Tablet, Tool};
use crate::app::Message;
use crate::pages::argb;
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

fn icon_button<'a>(glyph: &'static [u8], m: Option<Msg>) -> Element<'a, Message> {
    let color = if m.is_some() { theme::p().text } else { theme::p().muted };
    button(crate::shelf::icon(glyph, 20.0, color)).padding(10).on_press_maybe(m.map(msg)).style(theme::ghost).into()
}

fn tool_label(t: Tool) -> &'static str {
    match t {
        Tool::Pen => "Stift",
        Tool::Marker => "Marker",
        Tool::Eraser => "Radierer",
        Tool::Lasso => "Lasso",
        Tool::Text => "Text",
        Tool::Laser => "Laser",
    }
}

pub fn pen_label(p: PenType) -> &'static str {
    match p {
        PenType::Fountain => "Füller",
        PenType::Ballpoint => "Kugelschreiber",
        PenType::Pencil => "Bleistift",
        PenType::Brush => "Pinsel",
    }
}

fn pen_hint(p: PenType) -> &'static str {
    match p {
        PenType::Fountain => "Strich folgt dem Druck",
        PenType::Ballpoint => "Gleichmäßiger Strich",
        PenType::Pencil => "Graphit: weich am Rand, dunkler bei Druck",
        PenType::Brush => "Stark druckabhängig",
    }
}

fn msg(m: Msg) -> Message {
    Message::Tablet(m)
}

/// A round colour swatch; ringed when chosen.
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

/// The bar itself.
pub fn view(t: &Tablet) -> Element<'_, Message> {
    let mut tools = row![].spacing(4).align_y(Alignment::Center);
    // "Stift" shows the pen type and opens its menu when already chosen.
    let pen_on = t.tool == Tool::Pen;
    let pen_msg = if pen_on { Msg::PenMenu(!t.pen_menu) } else { Msg::Tool(Tool::Pen) };
    tools = tools.push(tool_button(&format!("{} ▾", pen_label(t.pen)), pen_on, pen_msg));
    for tool in [Tool::Marker, Tool::Eraser, Tool::Lasso, Tool::Text, Tool::Laser] {
        tools = tools.push(tool_button(tool_label(tool), t.tool == tool, Msg::Tool(tool)));
    }
    if t.tool == Tool::Eraser {
        tools = tools.push(Space::new().width(6));
        tools = tools.push(tool_button("Ganze Striche", t.eraser_mode == EraserMode::Stroke, Msg::EraserMode(EraserMode::Stroke)));
        tools = tools.push(tool_button("Fläche", t.eraser_mode == EraserMode::Area, Msg::EraserMode(EraserMode::Area)));
    }
    let mut colors = row![].spacing(0).align_y(Alignment::Center);
    let pal = pens::PALETTES[t.palette % pens::PALETTES.len()];
    for (i, c) in pal.iter().enumerate() {
        colors = colors.push(swatch(*c, i == t.color && matches!(t.tool, Tool::Pen | Tool::Marker), i));
    }
    let ink = visible_on_chrome(argb(t.ink() | 0xFF00_0000));
    let mut widths = row![].spacing(2).align_y(Alignment::Center);
    for (i, w) in t.widths.iter().enumerate() {
        widths = widths.push(width_button(*w, i == t.width, i, ink));
    }
    let undo = icon_button(UNDO, t.can_undo().then_some(Msg::Undo));
    let redo = icon_button(REDO, t.can_redo().then_some(Msg::Redo));
    let mut bar = row![tools, Space::new().width(14), colors, Space::new().width(10), widths, Space::new().width(Length::Fill)].align_y(Alignment::Center);
    if let Some(sel) = &t.selection {
        bar = bar.push(text(format!("{} ausgewählt", sel.ids.len())).font(theme::meta_font()).size(theme::SMALL).color(theme::p().muted));
        bar = bar.push(Space::new().width(8));
        bar = bar.push(button(theme::btn_small("Kopieren")).padding(theme::pad()).on_press(msg(Msg::Copy)).style(theme::ghost));
        bar = bar.push(button(theme::btn_small("Ausschneiden")).padding(theme::pad()).on_press(msg(Msg::Cut)).style(theme::ghost));
        bar = bar.push(button(theme::btn_small("Löschen")).padding(theme::pad()).on_press(msg(Msg::DeleteSelection)).style(theme::ghost));
        bar = bar.push(button(theme::btn_small("Fertig")).padding(theme::pad()).on_press(msg(Msg::ClearSelection)).style(theme::ghost));
        bar = bar.push(Space::new().width(8));
    }
    if !t.clip_is_empty() && t.selection.is_none() {
        bar = bar.push(button(theme::btn_small("Einfügen")).padding(theme::pad()).on_press(msg(Msg::Paste)).style(theme::ghost));
    }
    bar = bar.push(undo).push(redo);
    container(bar).padding([4, 10]).width(Length::Fill).style(theme::chrome).into()
}

/// The pen-type menu (under the bar, left).
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
    let mut pals = row![].spacing(4);
    for (i, name) in pens::PALETTE_NAMES.iter().enumerate() {
        pals = pals.push(button(text(*name).size(11.5)).padding([4, 8]).on_press(msg(Msg::Palette(i))).style(theme::option(i == t.palette)));
    }
    col = col.push(iced::widget::scrollable(pals).direction(iced::widget::scrollable::Direction::Horizontal(Default::default())).width(Length::Fixed(260.0)));
    container(col).padding(8).style(theme::menu).into()
}
