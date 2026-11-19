//! Left sidebar: the notebook outline (groups fold, pages jump), numbered like the pages.

use std::collections::HashMap;

use iced::widget::{Space, button, column, container, row, scrollable, text};
use iced::{Alignment, Element, Length, Padding};

use n0te_core::notebook::OutlineRow;

use crate::app::Message;
use crate::library::fit;
use crate::pages::{Pages, paper_code};
use crate::theme;

pub const WIDTH: f32 = 272.0;

pub fn view<'a>(rows: &'a [OutlineRow], pages: &'a Pages, current: u32) -> Element<'a, Message> {
    let numbers: HashMap<&str, u32> = pages.pages.iter().filter(|p| p.placed.side == 0).map(|p| (p.placed.node.as_str(), p.placed.number)).collect();
    let mut list = column![container(text("GLIEDERUNG").font(theme::MONO).size(theme::LABEL).color(theme::MUTED)).padding(Padding { top: 4.0, bottom: 6.0, left: 10.0, ..Padding::ZERO })].spacing(1);
    for r in rows {
        match r {
            OutlineRow::Group { node, depth, title, collapsed, count } => {
                let content = row![
                    text(if *collapsed { "▸" } else { "▾" }).font(theme::SANS_BOLD).size(12).color(theme::ACCENT).width(18),
                    text(fit(if title.trim().is_empty() { "Gruppe" } else { title }, 24)).font(theme::SANS_MEDIUM).size(13.5).wrapping(text::Wrapping::None),
                    Space::new().width(Length::Fill),
                    text(count.to_string()).font(theme::MONO).size(10).color(theme::MUTED),
                ]
                .align_y(Alignment::Center);
                list = list.push(
                    button(content)
                        .on_press(Message::Fold(node.clone()))
                        .padding(Padding { top: 7.0, bottom: 7.0, left: 10.0 + *depth as f32 * 14.0, right: 10.0 })
                        .width(Length::Fill)
                        .style(theme::row(false)),
                );
            }
            OutlineRow::Sheet { node, depth, sheet_id, title, spec, left, right } => {
                let n = numbers.get(node.as_str()).copied().unwrap_or(0);
                let name = if title.trim().is_empty() {
                    match spec.pdf {
                        Some((_, page)) => format!("PDF-Seite {}", page + 1),
                        None if spec.is_board() => "Board".into(),
                        None => "Seite".into(),
                    }
                } else {
                    title.clone()
                };
                let sides = format!("{}{}", if left.is_empty() { String::new() } else { format!("‹{} ", left.len()) }, if right.is_empty() { String::new() } else { format!("{}›", right.len()) });
                let content = row![
                    text(format!("{n:02}")).font(theme::MONO).size(11).color(if n == current { theme::ACCENT } else { theme::MUTED }).width(28),
                    text(fit(&name, 22)).font(theme::SANS).size(13).wrapping(text::Wrapping::None),
                    Space::new().width(Length::Fill),
                    text(sides).font(theme::MONO).size(10).color(theme::ACCENT),
                    Space::new().width(6),
                    text(paper_code(spec)).font(theme::MONO).size(9.5).color(theme::MUTED),
                ]
                .align_y(Alignment::Center);
                list = list.push(
                    button(content)
                        .on_press(Message::Jump(sheet_id.clone()))
                        .padding(Padding { top: 6.0, bottom: 6.0, left: 10.0 + *depth as f32 * 14.0, right: 10.0 })
                        .width(Length::Fill)
                        .style(theme::row(n == current)),
                );
            }
        }
    }
    container(scrollable(list.padding(Padding { top: 10.0, right: 8.0, bottom: 10.0, left: 8.0 })).style(theme::scroll_dark).height(Length::Fill))
        .width(WIDTH)
        .height(Length::Fill)
        .style(theme::surface)
        .into()
}
