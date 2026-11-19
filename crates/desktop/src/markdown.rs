//! A small Markdown renderer for text boxes on paper: headings, lists, quotes, code blocks,
//! checklists, numbered lists, **bold**, *italic*, `code` and $formulas$ (rendered by the local
//! TeX, see [`crate::latex`]).

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

/// A line that is only a display formula ($$…$$).
fn is_display_line(s: &str) -> bool {
    let t = s.trim();
    t.len() > 4 && t.starts_with("$$") && t.ends_with("$$") && !t[2..t.len() - 2].contains('$')
}

fn has_math(s: &str) -> bool {
    crate::mdstyle::pairs(s).iter().any(|(_, _, m)| m.starts_with('$'))
}

enum Piece<'a> {
    Text(&'a str, Font, f32),
    Math(&'a str, bool),
}

/// A line with formulas as widgets for a wrapping row: one widget per word, a word keeping its
/// bold / italic / code parts and formulas together (so "**fett**," has no gap before the comma).
pub fn inline_flow<'a, M: 'a>(line: &str, size: f32, base: Font, color: Color) -> Vec<Element<'a, M>> {
    let mut atoms: Vec<Piece<'_>> = Vec::new();
    let mut last = 0;
    for (a, e, m) in crate::mdstyle::pairs(line) {
        if a > last {
            atoms.push(Piece::Text(&line[last..a], base, size));
        }
        let inner = &line[a + m.len()..e - m.len()];
        atoms.push(match m {
            "$" | "$$" => Piece::Math(inner, m == "$$"),
            "**" => Piece::Text(inner, Font { weight: font::Weight::Bold, ..base }, size),
            "*" => Piece::Text(inner, Font { style: font::Style::Italic, ..base }, size),
            _ => Piece::Text(inner, theme::MONO, size * 0.92),
        });
        last = e;
    }
    if last < line.len() {
        atoms.push(Piece::Text(&line[last..], base, size));
    }
    // Group into words (whitespace separates words, nothing else does).
    let mut words: Vec<Vec<Piece<'_>>> = Vec::new();
    let mut cur: Vec<Piece<'_>> = Vec::new();
    for atom in atoms {
        match atom {
            Piece::Math(..) => cur.push(atom),
            Piece::Text(t, f, sz) => {
                for (i, chunk) in t.split(char::is_whitespace).enumerate() {
                    if i > 0 && !cur.is_empty() {
                        words.push(std::mem::take(&mut cur));
                    }
                    if !chunk.is_empty() {
                        cur.push(Piece::Text(chunk, f, sz));
                    }
                }
            }
        }
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
        .into_iter()
        .map(|word| {
            let parts: Vec<Element<'a, M>> = word
                .into_iter()
                .map(|p| match p {
                    Piece::Text(t, f, sz) => iced::widget::text(t.to_string()).font(f).size(sz).color(color).into(),
                    Piece::Math(l, d) => crate::blocks::formula_view(l, d, size),
                })
                .collect();
            if parts.len() == 1 {
                parts.into_iter().next().expect("one")
            } else {
                iced::widget::row(parts).align_y(iced::Alignment::Center).into()
            }
        })
        .collect()
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
        // (text, size, font, colour, indent, lead glyph)
        let todo = t.strip_prefix("- [ ] ").map(|r| (r, "☐  ")).or_else(|| t.strip_prefix("- [x] ").or_else(|| t.strip_prefix("- [X] ")).map(|r| (r, "☑  ")));
        let number = {
            let d = t.bytes().take_while(u8::is_ascii_digit).count();
            if (1..=3).contains(&d) { t[d..].strip_prefix(". ").map(|r| (r, &t[..d + 1])) } else { None }
        };
        let (body, sz, f, color, pad, lead): (&str, f32, Font, Color, f32, Option<String>) = if let Some(h) = t.strip_prefix("### ") {
            (h, size * 1.15, theme::SANS_BOLD, theme::INK, 0.0, None)
        } else if let Some(h) = t.strip_prefix("## ") {
            (h, size * 1.3, theme::SANS_BOLD, theme::INK, 0.0, None)
        } else if let Some(h) = t.strip_prefix("# ") {
            (h, size * 1.6, theme::SANS_BOLD, theme::INK, 0.0, None)
        } else if let Some((item, glyph)) = todo {
            let done = glyph.starts_with('☑');
            (item, size, theme::SANS, if done { theme::PAPER_MUTED } else { theme::INK }, size * 0.6, Some(glyph.to_string()))
        } else if let Some(item) = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")) {
            (item, size, theme::SANS, theme::INK, size * 0.6, Some("•  ".to_string()))
        } else if let Some((item, n)) = number {
            (item, size, theme::SANS, theme::INK, size * 0.6, Some(format!("{n}  ")))
        } else if let Some(q) = t.strip_prefix("> ") {
            (q, size, Font { style: font::Style::Italic, ..theme::SANS }, theme::PAPER_MUTED, size * 0.6, None)
        } else if t.is_empty() {
            let gap: Span<'static, (), Font> = span(" ").size(size * 0.5);
            col = col.push(rich_text(vec![gap]));
            continue;
        } else {
            (line, size, theme::SANS, theme::INK, 0.0, None)
        };
        let lead_span = lead.map(|g| span(g).font(theme::SANS_BOLD).size(sz).color(theme::p().accent));
        let centred = crate::settings::formula_center() && is_display_line(body);
        let para: Element<'a, M> = if has_math(body) {
            // With formulas: words and formula images in a wrapping row.
            let mut r = inline_flow(body, sz, f, color);
            if let Some(l) = lead_span {
                let lead: Vec<Span<'static, (), Font>> = vec![l];
                r.insert(0, rich_text(lead).into());
            }
            iced::widget::row(r).spacing(sz * 0.28).align_y(iced::Alignment::Center).wrap().into()
        } else {
            let mut spans: Vec<Span<'static, (), Font>> = lead_span.into_iter().collect();
            spans.extend(inline(body, sz, f, color));
            rich_text(spans).width(Length::Fill).into()
        };
        let para = container(para).padding(Padding { left: pad, ..Padding::ZERO });
        col = col.push(if centred { para.center_x(Length::Fill) } else { para });
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
