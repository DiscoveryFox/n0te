//! Left sidebar: the search field, then the notebook outline (groups fold, pages jump, the
//! headings marked on a page under it), numbered like the pages — or, while there is text in
//! the field, what the search found, page by page.

use std::collections::HashMap;

use iced::widget::text::Span;
use iced::widget::{Column, Space, button, column, container, rich_text, row, scrollable, span, stack, svg, text, text_input};
use iced::{Alignment, Color, Element, Font, Length, Padding};

use n0te_core::notebook::{Background, OutlineRow, SheetSpec};
use n0te_core::search::{HitKind, words};
use n0te_core::sheet::Heading;

use crate::app::Message;
use crate::library::fit;
use crate::pages::Pages;
use crate::search::{self, Search};
use crate::{theme, widgets};

pub const WIDTH: f32 = 272.0;
/// Hits listed at most (a common word in a long notebook would make thousands of rows).
pub const MAX_ROWS: usize = 200;

/// The search field (Ctrl+F focuses it).
pub fn input_id() -> iced::widget::Id {
    iced::widget::Id::new("n0te-search")
}

/// The scrolling list (outline or results).
pub fn list_id() -> iced::widget::Id {
    iced::widget::Id::new("n0te-sidebar-list")
}

/// The `i`-th hit in the results.
pub fn hit_id(i: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-hit-{i}"))
}

/// A heading's row in the outline.
pub fn heading_id(id: &str) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-heading-{id}"))
}

/// `headings`: each main sheet's headings by sheet id, top to bottom.
pub fn view<'a>(rows: &'a [OutlineRow], pages: &'a Pages, current: u32, headings: &'a HashMap<String, Vec<Heading>>, search: &'a Search) -> Element<'a, Message> {
    let list = if search.active() { results(search) } else { outline(rows, pages, current, headings) };
    let body = column![
        container(field(&search.query)).padding(Padding { top: 10.0, right: 10.0, bottom: 2.0, left: 10.0 }),
        scrollable(list.padding(Padding { top: 6.0, right: 8.0, bottom: 10.0, left: 8.0 })).id(list_id()).style(theme::scroll_dark).height(Length::Fill),
    ];
    container(body).width(WIDTH).height(Length::Fill).style(theme::surface).into()
}

/// What a page is called in the outline: its title, else its first heading, else what it is.
pub fn page_name(title: &str, spec: &SheetSpec, first_heading: Option<&str>) -> String {
    if !title.trim().is_empty() {
        return title.to_string();
    }
    if let Some(h) = first_heading.map(str::trim).filter(|h| !h.is_empty()) {
        return h.to_string();
    }
    match spec.pdf {
        Some((_, page)) => format!("PDF-Seite {}", page + 1),
        None if spec.is_board() => "Board".into(),
        None if spec.background == Background::Text => "Textseite".into(),
        None => "Seite".into(),
    }
}

/// The search field: a loupe, the query, × to empty it (Escape does too).
fn field(query: &str) -> Element<'_, Message> {
    let input = text_input("Suchen …", query)
        .id(input_id())
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
        let clear = button(text("×").size(14).line_height(1.0)).padding([2, 7]).on_press(Message::SearchClear).style(theme::ghost);
        layers = layers.push(container(clear).padding(Padding { right: 4.0, ..Padding::ZERO }).align_right(Length::Fill).center_y(Length::Fill));
    }
    layers.into()
}

fn outline<'a>(rows: &'a [OutlineRow], pages: &'a Pages, current: u32, headings: &'a HashMap<String, Vec<Heading>>) -> Column<'a, Message> {
    let numbers: HashMap<&str, u32> = pages.pages.iter().filter(|p| p.placed.side == 0).map(|p| (p.placed.node.as_str(), p.placed.number)).collect();
    let mut list = column![container(theme::caption("Gliederung")).padding(Padding { top: 4.0, bottom: 6.0, left: 10.0, ..Padding::ZERO })].spacing(1);
    for r in rows {
        match r {
            OutlineRow::Group { node, depth, title, collapsed, count } => {
                let content = row![
                    text(if *collapsed { "▸" } else { "▾" }).font(theme::SANS_BOLD).size(12).color(theme::p().accent).width(18),
                    text(fit(if title.trim().is_empty() { "Gruppe" } else { title }, 24)).font(theme::SANS_MEDIUM).size(13.5).wrapping(text::Wrapping::None),
                    Space::new().width(Length::Fill),
                    text(count.to_string()).font(theme::meta_font()).size(11).color(theme::p().muted),
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
                let heads: &[Heading] = headings.get(sheet_id).map(Vec::as_slice).unwrap_or(&[]);
                // Untitled: the first heading names the page (and isn't listed again under it).
                let named_by = (title.trim().is_empty()).then(|| heads.iter().position(|h| !h.text.trim().is_empty())).flatten();
                let name = page_name(title, spec, named_by.map(|i| heads[i].text.as_str()));
                let sides = format!("{}{}", if left.is_empty() { String::new() } else { format!("‹{} ", left.len()) }, if right.is_empty() { String::new() } else { format!("{}›", right.len()) });
                let content = row![
                    text(format!("{n:02}")).font(theme::meta_font()).size(11.5).color(if n == current { theme::p().accent } else { theme::p().muted }).width(28),
                    text(fit(&name, 22)).font(theme::SANS).size(13).wrapping(text::Wrapping::None),
                    Space::new().width(Length::Fill),
                    text(sides).font(theme::meta_font()).size(11).color(theme::p().accent),
                    Space::new().width(6),
                    // Only a page that differs from the usual A4 says its format ("A4 quer", "A5 hoch").
                    text(format_tag(spec)).font(theme::meta_font()).size(11).color(theme::p().muted),
                ]
                .align_y(Alignment::Center);
                let indent = 10.0 + *depth as f32 * 14.0;
                list = list.push(
                    button(content)
                        .on_press(Message::Jump(sheet_id.clone()))
                        .padding(Padding { top: 6.0, bottom: 6.0, left: indent, right: 10.0 })
                        .width(Length::Fill)
                        .style(theme::row(n == current)),
                );
                // The page's headings, under its name: a click shows the heading on the page.
                for (_, h) in heads.iter().enumerate().filter(|(i, _)| Some(*i) != named_by) {
                    let label = if h.text.trim().is_empty() { "Überschrift" } else { h.text.trim() };
                    let content = row![
                        text("—").size(11).color(theme::p().muted).width(16),
                        text(fit(label, 26)).size(12).color(theme::alpha(theme::p().text, 0.78)).wrapping(text::Wrapping::None),
                    ]
                    .align_y(Alignment::Center);
                    list = list.push(
                        container(
                            button(content)
                                .on_press(Message::Reveal(sheet_id.clone(), [h.x0, h.y0, h.x1, h.y1]))
                                .padding(Padding { top: 3.0, bottom: 4.0, left: indent + 28.0, right: 10.0 })
                                .width(Length::Fill)
                                .style(theme::row(false)),
                        )
                        .id(heading_id(&h.id)),
                    );
                }
            }
        }
    }
    list
}

/// What the search found: page by page in outline order (number and name), each hit with its
/// kind and its text, the query's words marked.
fn results(s: &Search) -> Column<'_, Message> {
    let mut list = column![].spacing(1);
    let count: usize = s.found.as_ref().map(|(_, pages)| pages.iter().map(|p| p.hits.len()).sum()).unwrap_or(0);
    let head: Element<'_, Message> = match &s.found {
        None => theme::caption("Sucht …").into(),
        Some(_) => row![theme::caption("Treffer"), Space::new().width(Length::Fill), text(count.to_string()).font(theme::meta_font()).size(11).color(theme::p().muted)].align_y(Alignment::Center).into(),
    };
    list = list.push(container(head).padding(Padding { top: 4.0, bottom: 6.0, left: 10.0, right: 10.0 }));
    let Some((query, pages)) = &s.found else { return list };
    if pages.is_empty() {
        let none = column![
            text("Nichts gefunden").size(13).color(theme::p().text),
            text("Handschrift findet die Suche, sobald das Tablet sie gelesen hat.").size(12).color(theme::p().muted),
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
                .id(hit_id(n)),
            );
            n += 1;
        }
    }
    if count > n {
        list = list.push(container(text(format!("… und {} weitere — genauer suchen", count - n)).size(12).color(theme::p().muted)).padding(Padding { top: 8.0, left: 10.0, right: 10.0, bottom: 0.0 }));
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
        _ => n0te_core::format::describe(spec.width, spec.height),
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
        assert_eq!(super::page_name("Analysis", &grid, Some("Satz 3")), "Analysis");
        assert_eq!(super::page_name("", &grid, Some("  Satz 3 ")), "Satz 3");
        assert_eq!(super::page_name(" ", &grid, None), "Seite");
        assert_eq!(super::page_name("", &grid, Some(" ")), "Seite");
        assert_eq!(super::page_name("", &SheetSpec { pdf: Some(("b".into(), 2)), ..grid.clone() }, None), "PDF-Seite 3");
        assert_eq!(super::page_name("", &SheetSpec::a4(Background::Text), None), "Textseite");
    }
}
