//! Left sidebar: the notebook outline from `Notebook::outline` (groups collapsible) plus the
//! flowing text page.

use iced::widget::scrollable::{Direction, Scrollbar};
use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Padding};

use n0te_core::notebook::{Background, OutlineRow};

use crate::app::{Message, Mode};
use crate::pages::Pages;
use crate::theme;

pub const WIDTH: f32 = 272.0;

/// iced 0.14 text has no ellipsis; rows are kept to one line by cutting the title.
fn fit(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max - 1).collect();
        format!("{}…", cut.trim_end())
    }
}

fn label(s: &str) -> Element<'_, Message> {
    text(s).font(theme::MONO).size(theme::LABEL).color(theme::MUTED).into()
}

fn bg_code(bg: Background) -> &'static str {
    match bg {
        Background::Blank => "LEER",
        Background::Lines => "LIN",
        Background::Grid => "KAR",
        Background::Dots => "PKT",
        Background::Cornell => "COR",
    }
}

pub fn view<'a>(rows: &'a [OutlineRow], pages: &'a Pages, mode: Mode, current_page: usize) -> Element<'a, Message> {
    let mut list = column![].spacing(1);

    list = list.push(container(label("DOKUMENT")).padding(Padding { top: 4.0, bottom: 6.0, left: 10.0, ..Padding::ZERO }));
    let text_row = row![
        text("T").font(theme::MONO_BOLD).size(12).color(theme::ACCENT).width(26),
        text("Mitschrift V08").font(theme::SANS).size(13.5),
        Space::new().width(Length::Fill),
        text("TEXT").font(theme::MONO).size(10).color(theme::MUTED),
    ]
    .align_y(Alignment::Center);
    list = list.push(button(text_row).on_press(Message::SetMode(Mode::Text)).padding([7, 10]).width(Length::Fill).style(theme::row(mode == Mode::Text)));

    list = list.push(container(label("SEITEN")).padding(Padding { top: 18.0, bottom: 6.0, left: 10.0, ..Padding::ZERO }));
    for r in rows {
        match r {
            OutlineRow::Group { node, depth, title, collapsed, count } => {
                let indent = 10.0 + *depth as f32 * 14.0;
                let content = row![
                    text(if *collapsed { "+" } else { "–" }).font(theme::MONO_BOLD).size(12).color(theme::MUTED).width(18),
                    text(fit(title, 24)).font(theme::SANS_MEDIUM).size(13.5).wrapping(text::Wrapping::None),
                    Space::new().width(Length::Fill),
                    text(format!("{count:02}")).font(theme::MONO).size(10.5).color(theme::MUTED),
                ]
                .align_y(Alignment::Center);
                list = list.push(
                    button(content)
                        .on_press(Message::ToggleGroup(node.clone()))
                        .padding(Padding { top: 7.0, bottom: 7.0, left: indent, right: 10.0 })
                        .width(Length::Fill)
                        .style(theme::row(false)),
                );
            }
            OutlineRow::Sheet { depth, sheet_id, title, spec, left, right, .. } => {
                let indent = 10.0 + *depth as f32 * 14.0;
                let number = pages.page_of_sheet(sheet_id).map(|i| pages.pages[i].number).unwrap_or(0);
                let selected = mode == Mode::Pages && number == current_page;
                let sides = left.len() + right.len();
                let side_tag: Element<'_, Message> = if sides > 0 {
                    text(format!("{}{}", if left.is_empty() { "" } else { "‹" }, if right.is_empty() { format!("{sides}") } else { format!("{sides}›") }))
                        .font(theme::MONO_BOLD)
                        .size(10.5)
                        .color(theme::ACCENT)
                        .into()
                } else {
                    Space::new().into()
                };
                let content = row![
                    text(format!("{number:02}")).font(theme::MONO).size(11).color(if selected { theme::ACCENT } else { theme::MUTED }).width(26),
                    text(fit(if title.is_empty() { "Seite" } else { title.as_str() }, if sides > 0 { 21 } else { 24 } - (*depth as usize) * 2)).font(theme::SANS).size(13.5).width(Length::Fill).wrapping(text::Wrapping::None),
                    side_tag,
                    text(bg_code(spec.background)).font(theme::MONO).size(9.5).color(theme::MUTED).width(30).align_x(Alignment::End),
                ]
                .spacing(6)
                .align_y(Alignment::Center);
                list = list.push(
                    button(content)
                        .on_press(Message::Jump(sheet_id.clone()))
                        .padding(Padding { top: 6.0, bottom: 6.0, left: indent, right: 10.0 })
                        .width(Length::Fill)
                        .style(theme::row(selected)),
                );
            }
        }
    }

    let scroll = scrollable(list.padding(Padding { top: 12.0, right: 10.0, bottom: 12.0, left: 8.0 }))
        .direction(Direction::Vertical(Scrollbar::new().width(3).scroller_width(3).margin(1)))
        .style(theme::scroll_dark)
        .height(Length::Fill);
    container(scroll)
        .width(WIDTH)
        .height(Length::Fill)
        .style(theme::chrome)
        .into()
}
