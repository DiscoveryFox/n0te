//! A small Markdown renderer for text boxes on paper: headings, lists, quotes, code blocks,
//! **bold**, *italic*, `code` and $formulas$ (shown in mono until formula rendering exists).

use iced::widget::{column, container, rich_text, span, text::Span};
use iced::{Color, Element, Font, Length, Padding, font};

use crate::theme;

/// Inline spans of one line: **bold**, *italic*, `code`, $math$.
fn inline(line: &str, size: f32, base: Font, color: Color) -> Vec<Span<'static, (), Font>> {
    let mut out = Vec::new();
    let mut rest = line;
    let bold = Font { weight: font::Weight::Bold, ..base };
    let italic = Font { style: font::Style::Italic, ..base };
    while !rest.is_empty() {
        // The earliest marker wins.
        let marks = [("**", bold), ("`", theme::MONO), ("$", theme::MONO), ("*", italic)];
        let next = marks.iter().filter_map(|(m, f)| rest.find(m).map(|i| (i, *m, *f))).min_by_key(|(i, m, _)| (*i, usize::MAX - m.len()));
        match next {
            Some((i, m, f)) if rest[i + m.len()..].contains(m) => {
                if i > 0 {
                    out.push(span(rest[..i].to_string()).font(base).size(size).color(color));
                }
                let inner_start = i + m.len();
                let end = inner_start + rest[inner_start..].find(m).expect("checked");
                let mut s = span(rest[inner_start..end].to_string()).font(f).size(size).color(if m == "$" { theme::p().accent } else { color });
                if m == "`" {
                    s = s.background(theme::PAPER_TINT);
                }
                out.push(s);
                rest = &rest[end + m.len()..];
            }
            _ => {
                out.push(span(rest.to_string()).font(base).size(size).color(color));
                break;
            }
        }
    }
    out
}

/// The text as paragraphs at `size` (logical px) in `width`.
pub fn view<'a, M: 'a>(text: &str, size: f32, width: f32) -> Element<'a, M> {
    let mut col = column![].spacing(size * 0.25).width(Length::Fixed(width));
    let mut code: Option<Vec<String>> = None;
    for line in text.split('\n') {
        if line.trim_start().starts_with("```") {
            match code.take() {
                Some(lines) => {
                    let sp: Span<'static, (), Font> = span(lines.join("\n")).font(theme::MONO).size(size * 0.9).color(theme::INK);
                    let body = rich_text(vec![sp]);
                    col = col.push(container(body).padding(Padding::from(size * 0.4)).width(Length::Fill).style(theme::code_box));
                }
                None => code = Some(Vec::new()),
            }
            continue;
        }
        if let Some(lines) = code.as_mut() {
            lines.push(line.to_string());
            continue;
        }
        let t = line.trim_start();
        let (spans, pad): (Vec<Span<'static, (), Font>>, f32) = if let Some(h) = t.strip_prefix("### ") {
            (inline(h, size * 1.15, theme::SANS_BOLD, theme::INK), 0.0)
        } else if let Some(h) = t.strip_prefix("## ") {
            (inline(h, size * 1.3, theme::SANS_BOLD, theme::INK), 0.0)
        } else if let Some(h) = t.strip_prefix("# ") {
            (inline(h, size * 1.6, theme::SANS_BOLD, theme::INK), 0.0)
        } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            let mut v = vec![span("•  ").font(theme::SANS).size(size).color(theme::p().accent)];
            v.extend(inline(item, size, theme::SANS, theme::INK));
            (v, size * 0.6)
        } else if let Some(q) = t.strip_prefix("> ") {
            (inline(q, size, Font { style: font::Style::Italic, ..theme::SANS }, theme::PAPER_MUTED), size * 0.6)
        } else if t.is_empty() {
            (vec![span(" ").size(size * 0.5)], 0.0)
        } else {
            (inline(line, size, theme::SANS, theme::INK), 0.0)
        };
        let para = rich_text(spans).width(Length::Fill);
        col = col.push(container(para).padding(Padding { left: pad, ..Padding::ZERO }));
    }
    if let Some(lines) = code {
        let sp: Span<'static, (), Font> = span(lines.join("\n")).font(theme::MONO).size(size * 0.9).color(theme::INK);
        col = col.push(rich_text(vec![sp]));
    }
    col.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(v: &[Span<'static, (), Font>]) -> Vec<String> {
        v.iter().map(|s| s.text.to_string()).collect()
    }

    #[test]
    fn inline_marks_split_into_spans() {
        let v = inline("Es gilt **f'(x) = 0** für *alle* `x` mit $x>0$.", 14.0, theme::SANS, theme::INK);
        assert_eq!(texts(&v), vec!["Es gilt ", "f'(x) = 0", " für ", "alle", " ", "x", " mit ", "x>0", "."]);
        assert_eq!(v[1].font, Some(Font { weight: font::Weight::Bold, ..theme::SANS }));
    }

    #[test]
    fn an_unclosed_mark_stays_text() {
        assert_eq!(texts(&inline("3 * 4 = 12", 14.0, theme::SANS, theme::INK)), vec!["3 * 4 = 12"]);
    }
}
