//! Text boxes for PDF export: Markdown lines laid out with the standard PDF fonts (Helvetica,
//! Helvetica-Bold, Courier, WinAnsi encoding), wrapped with their real character widths.

/// A text box on a page (page points, y down like the app).
#[derive(Debug, Clone, PartialEq)]
pub struct PdfText {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub size: f32,
    /// Markdown.
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Regular,
    Bold,
    Mono,
}

/// One laid-out line: font, size, colour (grey for quotes), start x, baseline y, WinAnsi bytes.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub face: Face,
    pub size: f32,
    pub grey: bool,
    pub x: f32,
    pub baseline: f32,
    pub bytes: Vec<u8>,
}

// Advance widths (1/1000 em) of ASCII 32..=126.
const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667, 611, 778, 722,
    278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556,
    333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];
const HELV_BOLD: [u16; 95] = [
    278, 333, 474, 556, 556, 889, 722, 238, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 333, 333, 584, 584, 584, 611, 975, 722, 722, 722, 722, 667, 611, 778, 722,
    278, 556, 722, 611, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 333, 278, 333, 584, 556, 333, 556, 611, 556, 611, 556, 333, 611, 611, 278, 278, 556, 278, 889, 611, 611, 611, 611,
    389, 556, 333, 611, 556, 778, 556, 556, 500, 389, 280, 389, 584,
];

/// The character as one WinAnsi byte ('?' if it has none).
pub fn win_ansi(c: char) -> u8 {
    match c as u32 {
        0x20..=0x7E => c as u8,
        0xA0..=0xFF => c as u32 as u8,
        _ => match c {
            '€' => 0x80,
            '‚' => 0x82,
            '„' => 0x84,
            '…' => 0x85,
            '‘' => 0x91,
            '’' => 0x92,
            '“' => 0x93,
            '”' => 0x94,
            '•' => 0x95,
            '–' => 0x96,
            '—' => 0x97,
            '™' => 0x99,
            _ => b'?',
        },
    }
}

fn width(b: u8, face: Face) -> f32 {
    let w = match face {
        Face::Mono => 600,
        f => {
            let table = if f == Face::Bold { &HELV_BOLD } else { &HELV };
            match b {
                0x20..=0x7E => table[(b - 0x20) as usize],
                0x95 => 350,
                0x96 => 556,
                0x97 => 1000,
                0xC4 | 0xC0..=0xC3 | 0xC5 => 667,
                0xD6 | 0xD2..=0xD5 | 0xD8 => 778,
                0xDC | 0xD9..=0xDB => 722,
                0xDF => 611,
                0xE0..=0xFF => 556,
                _ => 556,
            }
        }
    };
    w as f32 / 1000.0
}

fn text_width(bytes: &[u8], face: Face, size: f32) -> f32 {
    bytes.iter().map(|&b| width(b, face)).sum::<f32>() * size
}

/// Inline Markdown marks are dropped (the PDF shows plain text in the line's face).
fn strip_inline(s: &str) -> String {
    let mut out = s.to_string();
    for m in ["**", "`", "$"] {
        if out.matches(m).count() >= 2 {
            out = out.replace(m, "");
        }
    }
    out
}

/// Lays out a text box: one entry per printed line.
pub fn layout(t: &PdfText) -> Vec<Line> {
    let mut out = Vec::new();
    let mut y = t.y;
    let mut code = false;
    for raw in t.text.split('\n') {
        let trimmed = raw.trim_start();
        if trimmed.starts_with("```") {
            code = !code;
            continue;
        }
        let (face, scale, indent, grey, prefix, body): (Face, f32, f32, bool, &str, String) = if code {
            (Face::Mono, 0.9, 0.0, false, "", raw.to_string())
        } else if let Some(h) = trimmed.strip_prefix("### ") {
            (Face::Bold, 1.15, 0.0, false, "", strip_inline(h))
        } else if let Some(h) = trimmed.strip_prefix("## ") {
            (Face::Bold, 1.3, 0.0, false, "", strip_inline(h))
        } else if let Some(h) = trimmed.strip_prefix("# ") {
            (Face::Bold, 1.6, 0.0, false, "", strip_inline(h))
        } else if let Some(item) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")) {
            (Face::Regular, 1.0, 0.9, false, "• ", strip_inline(item))
        } else if let Some(q) = trimmed.strip_prefix("> ") {
            (Face::Regular, 1.0, 0.9, true, "", strip_inline(q))
        } else {
            (Face::Regular, 1.0, 0.0, false, "", strip_inline(raw))
        };
        let size = t.size * scale;
        let lh = size * 1.25;
        if body.trim().is_empty() && prefix.is_empty() {
            y += lh * 0.6;
            continue;
        }
        let x0 = t.x + indent * t.size;
        let avail = (t.w - indent * t.size).max(size);
        // Greedy word wrap; the bullet hangs before the first line.
        let mut first = true;
        let mut line: Vec<u8> = Vec::new();
        let mut flush = |line: &mut Vec<u8>, first: &mut bool, y: &mut f32| {
            let (bytes, x) = if *first && !prefix.is_empty() {
                let p: Vec<u8> = prefix.chars().map(win_ansi).collect();
                let px = x0 - text_width(&p, face, size);
                ([p, std::mem::take(line)].concat(), px)
            } else {
                (std::mem::take(line), x0)
            };
            out.push(Line { face, size, grey, x, baseline: *y + size * 0.95, bytes });
            *y += lh;
            *first = false;
        };
        for word in body.split(' ') {
            let w: Vec<u8> = word.chars().map(win_ansi).collect();
            let candidate = if line.is_empty() { w.clone() } else { [line.clone(), vec![b' '], w.clone()].concat() };
            if !line.is_empty() && text_width(&candidate, face, size) > avail {
                flush(&mut line, &mut first, &mut y);
                line = w;
            } else {
                line = candidate;
            }
        }
        flush(&mut line, &mut first, &mut y);
    }
    out
}

/// A PDF string literal of WinAnsi bytes.
pub fn pdf_string(bytes: &[u8]) -> Vec<u8> {
    let mut s = vec![b'('];
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                s.push(b'\\');
                s.push(b);
            }
            0x20..=0x7E => s.push(b),
            _ => s.extend_from_slice(format!("\\{b:03o}").as_bytes()),
        }
    }
    s.push(b')');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_by_width_and_hangs_bullets() {
        let t = PdfText { x: 10.0, y: 20.0, w: 120.0, size: 12.0, text: "# Merke\n- Eine Folge konvergiert, wenn sie beschränkt ist\nEnde".into() };
        let lines = layout(&t);
        assert_eq!(lines[0].face, Face::Bold);
        assert!((lines[0].size - 19.2).abs() < 0.01);
        assert!(lines[1].bytes.starts_with(&[0x95, b' ']), "bullet");
        assert!(lines[1].x < 10.0 + 0.9 * 12.0, "bullet hangs left of the text");
        assert!(lines.len() >= 4, "the long item wrapped: {lines:?}");
        assert!(lines.iter().all(|l| text_width(&l.bytes, l.face, l.size) <= 120.0 + 12.0));
        assert!(lines.windows(2).all(|w| w[1].baseline > w[0].baseline));
        assert!(lines.iter().any(|l| l.bytes.contains(&0xE4)), "ä in WinAnsi");
    }

    #[test]
    fn strings_escape_parens_and_high_bytes() {
        assert_eq!(pdf_string(&[b'(', b'a', 0xE4, b')']), b"(\\(a\\344\\))".to_vec());
    }
}
