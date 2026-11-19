//! Inline Markdown shown formatted while typing (an iced text_editor highlighter): **bold**,
//! *italic*, `code` and `$formula$` get their look, the marks themselves turn faint. iced's
//! editor can change colour and font per span, not size, so headings stay a block thing.

use std::ops::Range;

use iced::advanced::text::highlighter::{Format, Highlighter};
use iced::{Color, Font, font};

use crate::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Span {
    Mark,
    Bold,
    Italic,
    Code,
    Math,
}

/// Paired inline marks in a line: (byte start, byte end incl. marks, mark).
pub fn pairs(line: &str) -> Vec<(usize, usize, &'static str)> {
    let mut out = Vec::new();
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // Bytes, not str slices: `i` may sit inside a multi-byte character (ä, €, …).
        let m: Option<&'static str> = if b[i..].starts_with(b"**") {
            Some("**")
        } else if b[i] == b'`' {
            Some("`")
        } else if b[i..].starts_with(b"$$") {
            Some("$$")
        } else if b[i] == b'$' {
            Some("$")
        } else if b[i] == b'*' {
            Some("*")
        } else {
            None
        };
        let Some(m) = m else {
            i += 1;
            continue;
        };
        match line[i + m.len()..].find(m) {
            Some(rel) => {
                let end = i + m.len() + rel + m.len();
                out.push((i, end, m));
                i = end;
            }
            None => i += m.len(),
        }
    }
    out
}

/// The styled spans of one line (byte ranges).
pub fn spans(line: &str) -> Vec<(Range<usize>, Span)> {
    let mut out = Vec::new();
    for (a, b, m) in pairs(line) {
        let (i0, i1) = (a + m.len(), b - m.len());
        out.push((a..i0, Span::Mark));
        let kind = match m {
            "**" => Span::Bold,
            "*" => Span::Italic,
            "`" => Span::Code,
            _ => Span::Math,
        };
        if i1 > i0 {
            out.push((i0..i1, kind));
        }
        out.push((i1..b, Span::Mark));
    }
    out
}

pub struct MdInline {
    line: usize,
}

impl Highlighter for MdInline {
    type Settings = ();
    type Highlight = Span;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Span)>;

    fn new(_: &()) -> Self {
        MdInline { line: 0 }
    }
    fn update(&mut self, _: &()) {}
    fn change_line(&mut self, line: usize) {
        self.line = line;
    }
    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        self.line += 1;
        spans(line).into_iter()
    }
    fn current_line(&self) -> usize {
        self.line
    }
}

/// How each span looks on the paper.
pub fn format(span: &Span, _theme: &iced::Theme) -> Format<Font> {
    match span {
        Span::Mark => Format { color: Some(Color { a: 0.35, ..theme::PAPER_MUTED }), font: None },
        Span::Bold => Format { color: None, font: Some(theme::SANS_BOLD) },
        Span::Italic => Format { color: None, font: Some(Font { style: font::Style::Italic, ..theme::SANS }) },
        Span::Code => Format { color: Some(Color::from_rgb8(0x5A, 0x3E, 0x9E)), font: Some(theme::MONO) },
        Span::Math => Format { color: Some(theme::p().accent), font: Some(theme::MONO) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_inline_marks() {
        assert_eq!(pairs("a **b** *c* `d` $e$ $$f$$ *open"), vec![(2, 7, "**"), (8, 11, "*"), (12, 15, "`"), (16, 19, "$"), (20, 25, "$$")]);
        let s = spans("x **bold** y");
        assert_eq!(s, vec![(2..4, Span::Mark), (4..8, Span::Bold), (8..10, Span::Mark)]);
        // Multi-byte text stays on char boundaries.
        let s = spans("ä **ö** ü");
        for (r, _) in &s {
            assert!("ä **ö** ü".is_char_boundary(r.start) && "ä **ö** ü".is_char_boundary(r.end));
        }
    }
}
