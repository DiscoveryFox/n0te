//! Text of a page with the position of every character (for search and selection).

use crate::{PdfDoc, Rect, Result, display_to_user, lock};

/// One character of a page's text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextChar {
    pub ch: char,
    /// Box in page coordinates (points, top-left origin). Empty for generated characters.
    pub bbox: Rect,
    /// pdfium inserted it (a space between words or a line break), it is not drawn on the page.
    pub generated: bool,
}

/// The text of one page in reading order, as pdfium extracts it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageText {
    pub chars: Vec<TextChar>,
}

impl PageText {
    pub fn text(&self) -> String {
        self.chars.iter().map(|c| c.ch).collect()
    }

    /// The page's lines in reading order, each with the box around its drawn characters (lines
    /// break where pdfium put a line break).
    pub fn lines(&self) -> Vec<(String, Rect)> {
        let mut out = Vec::new();
        let mut text = String::new();
        let mut b: Option<Rect> = None;
        let flush = |text: &mut String, b: &mut Option<Rect>, out: &mut Vec<(String, Rect)>| {
            if let Some(r) = b.take()
                && !text.trim().is_empty()
            {
                out.push((text.trim().to_string(), r));
            }
            text.clear();
        };
        for c in &self.chars {
            if c.ch == '\n' || c.ch == '\r' {
                flush(&mut text, &mut b, &mut out);
                continue;
            }
            text.push(c.ch);
            if !c.generated {
                let r = c.bbox;
                b = Some(match b {
                    None => r,
                    Some(o) => Rect { x0: o.x0.min(r.x0), y0: o.y0.min(r.y0), x1: o.x1.max(r.x1), y1: o.y1.max(r.y1) },
                });
            }
        }
        flush(&mut text, &mut b, &mut out);
        out
    }

    /// Case-insensitive search. Each match is returned as one box per line it spans.
    pub fn find(&self, needle: &str) -> Vec<Vec<Rect>> {
        let fold = |c: char| c.to_lowercase().next().unwrap_or(c);
        let needle: Vec<char> = needle.chars().map(fold).collect();
        let hay: Vec<char> = self.chars.iter().map(|c| fold(c.ch)).collect();
        if needle.is_empty() || needle.len() > hay.len() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut i = 0;
        while i + needle.len() <= hay.len() {
            if hay[i..i + needle.len()] == needle[..] {
                out.push(line_boxes(&self.chars[i..i + needle.len()]));
                i += needle.len();
            } else {
                i += 1;
            }
        }
        out
    }
}

/// Merges character boxes into one box per line.
fn line_boxes(chars: &[TextChar]) -> Vec<Rect> {
    let mut out: Vec<Rect> = Vec::new();
    for c in chars.iter().filter(|c| !c.bbox.is_empty()) {
        let b = c.bbox;
        match out.last_mut() {
            Some(r) if overlap(r.y0, r.y1, b.y0, b.y1) > 0.5 * (b.y1 - b.y0).min(r.y1 - r.y0) => {
                *r = r.include(b.x0, b.y0).include(b.x1, b.y1);
            }
            _ => out.push(b),
        }
    }
    out
}

fn overlap(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

impl PdfDoc {
    /// Extracts the text of a page with character positions.
    pub fn page_text(&self, page: u32) -> Result<PageText> {
        let _g = lock();
        self.with_page(page, |p| {
            let to_display = display_to_user(p)?.invert();
            let text = p.text()?;
            let chars = text.chars();
            let mut out = Vec::with_capacity(chars.len() as usize);
            for c in chars.iter() {
                let generated = c.is_generated().unwrap_or(false);
                let bbox = match (generated, c.loose_bounds()) {
                    (false, Ok(r)) => to_display.map_rect(r.left().value as f64, r.bottom().value as f64, r.right().value as f64, r.top().value as f64),
                    _ => Rect::EMPTY,
                };
                out.push(TextChar { ch: c.unicode_char().unwrap_or('\u{FFFD}'), bbox, generated });
            }
            Ok(PageText { chars: out })
        })
    }
}
