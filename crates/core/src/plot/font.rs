//! The stroke font of a plot's writing (numbers, names, points): each glyph a few pen lines in
//! a box 1 high (capitals and digits from 0 to the line at 1, lowercase from 0.45, descenders to
//! 1.3), with its width. Drawn with the hand's wobble, so it looks written, not typeset.

use std::f32::consts::PI;

type Lines = Vec<Vec<(f32, f32)>>;

/// An arc of the ellipse about (cx, cy), angles in degrees, counter-clockwise (y up).
fn arc(cx: f32, cy: f32, rx: f32, ry: f32, a0: f32, a1: f32) -> Vec<(f32, f32)> {
    let n = (((a1 - a0).abs() / 12.0).ceil() as usize).max(2);
    (0..=n)
        .map(|i| {
            let a = (a0 + (a1 - a0) * i as f32 / n as f32) * PI / 180.0;
            (cx + rx * a.cos(), cy - ry * a.sin())
        })
        .collect()
}

fn bez(a: (f32, f32), c: (f32, f32), b: (f32, f32)) -> Vec<(f32, f32)> {
    (0..=10)
        .map(|i| {
            let t = i as f32 / 10.0;
            let u = 1.0 - t;
            (u * u * a.0 + 2.0 * u * t * c.0 + t * t * b.0, u * u * a.1 + 2.0 * u * t * c.1 + t * t * b.1)
        })
        .collect()
}

fn join(parts: &[Vec<(f32, f32)>]) -> Vec<(f32, f32)> {
    parts.iter().flatten().copied().collect()
}

fn l(pts: &[(f32, f32)]) -> Vec<(f32, f32)> {
    pts.to_vec()
}

/// A glyph: its width and lines; None for characters the font doesn't have.
pub fn glyph(c: char) -> Option<(f32, Lines)> {
    // Shapes used more than once.
    let bowl_r = || arc(0.27, 0.725, 0.2, 0.27, 0.0, 360.0);
    let bowl_l = || arc(0.22, 0.725, 0.2, 0.27, 0.0, 360.0);
    let p_top = || join(&[l(&[(0.06, 1.0), (0.06, 0.0), (0.28, 0.0)]), arc(0.28, 0.26, 0.2, 0.26, 90.0, -90.0), l(&[(0.28, 0.52), (0.06, 0.52)])]);
    let o = || arc(0.3, 0.5, 0.27, 0.5, 90.0, 450.0);
    Some(match c {
        '0' => (0.56, vec![arc(0.28, 0.5, 0.25, 0.5, 90.0, 450.0)]),
        '1' => (0.4, vec![l(&[(0.04, 0.22), (0.26, 0.0), (0.26, 1.0)])]),
        '2' => (0.56, vec![join(&[arc(0.27, 0.27, 0.24, 0.25, 160.0, -25.0), l(&[(0.03, 1.0), (0.54, 1.0)])])]),
        '3' => (0.54, vec![join(&[arc(0.26, 0.25, 0.22, 0.24, 150.0, -90.0), arc(0.26, 0.735, 0.25, 0.265, 90.0, -150.0)])]),
        '4' => (0.58, vec![l(&[(0.4, 1.0), (0.4, 0.0), (0.03, 0.68), (0.56, 0.68)])]),
        '5' => (0.54, vec![join(&[l(&[(0.48, 0.0), (0.12, 0.0), (0.1, 0.43)]), arc(0.27, 0.69, 0.24, 0.31, 125.0, -145.0)])]),
        '6' => (0.56, vec![join(&[bez((0.46, 0.03), (0.07, 0.1), (0.06, 0.6)), arc(0.29, 0.7, 0.23, 0.3, 172.0, 532.0)])]),
        '7' => (0.54, vec![l(&[(0.02, 0.0), (0.52, 0.0), (0.2, 1.0)])]),
        '8' => (0.56, vec![arc(0.28, 0.25, 0.2, 0.24, 270.0, 630.0), arc(0.28, 0.73, 0.25, 0.27, 90.0, 450.0)]),
        '9' => (0.56, vec![join(&[arc(0.27, 0.3, 0.23, 0.3, 0.0, 360.0), bez((0.5, 0.3), (0.5, 0.85), (0.14, 0.99))])]),
        '-' | '−' => (0.42, vec![l(&[(0.06, 0.56), (0.36, 0.56)])]),
        ',' => (0.22, vec![l(&[(0.12, 0.88), (0.12, 0.96), (0.05, 1.14)])]),
        '.' => (0.2, vec![l(&[(0.09, 0.95), (0.11, 0.97)])]),
        ' ' => (0.3, vec![]),
        '(' => (0.34, vec![arc(0.36, 0.5, 0.24, 0.62, 125.0, 235.0)]),
        ')' => (0.34, vec![arc(-0.02, 0.5, 0.24, 0.62, 55.0, -55.0)]),
        '|' => (0.22, vec![l(&[(0.11, -0.05), (0.11, 1.12)])]),
        '\'' | '′' => (0.2, vec![l(&[(0.13, -0.02), (0.07, 0.3)])]),
        '/' => (0.42, vec![l(&[(0.4, -0.05), (0.02, 1.05)])]),
        '=' => (0.46, vec![l(&[(0.06, 0.48), (0.4, 0.48)]), l(&[(0.06, 0.68), (0.4, 0.68)])]),
        '+' => (0.48, vec![l(&[(0.06, 0.62), (0.42, 0.62)]), l(&[(0.24, 0.44), (0.24, 0.8)])]),
        '·' => (0.22, vec![l(&[(0.1, 0.6), (0.11, 0.61)])]),
        '≈' => (0.46, vec![join(&[bez((0.04, 0.5), (0.13, 0.38), (0.23, 0.5)), bez((0.23, 0.5), (0.33, 0.62), (0.42, 0.5))]), join(&[bez((0.04, 0.7), (0.13, 0.58), (0.23, 0.7)), bez((0.23, 0.7), (0.33, 0.82), (0.42, 0.7))])]),
        'π' => (0.58, vec![l(&[(0.02, 0.5), (0.56, 0.47)]), l(&[(0.17, 0.49), (0.15, 1.0)]), l(&[(0.41, 0.48), (0.42, 0.92), (0.5, 1.0)])]),
        // Lowercase.
        'a' => (0.5, vec![bowl_l(), l(&[(0.43, 0.45), (0.43, 1.0)])]),
        'b' => (0.5, vec![l(&[(0.06, 0.0), (0.06, 1.0)]), bowl_r()]),
        'c' => (0.46, vec![arc(0.25, 0.725, 0.2, 0.27, 40.0, 320.0)]),
        'd' => (0.5, vec![bowl_l(), l(&[(0.42, 0.0), (0.42, 1.0)])]),
        'e' => (0.5, vec![join(&[l(&[(0.05, 0.73), (0.45, 0.73)]), arc(0.25, 0.725, 0.2, 0.27, 0.0, 320.0)])]),
        'f' => (0.42, vec![join(&[arc(0.34, 0.17, 0.12, 0.15, 10.0, 180.0), l(&[(0.22, 0.17), (0.22, 1.0)])]), l(&[(0.06, 0.47), (0.4, 0.47)])]),
        'g' => (0.5, vec![arc(0.22, 0.7, 0.19, 0.25, 0.0, 360.0), join(&[l(&[(0.41, 0.45), (0.41, 1.1)]), arc(0.24, 1.1, 0.17, 0.17, 0.0, -150.0)])]),
        'h' => (0.5, vec![l(&[(0.06, 0.0), (0.06, 1.0)]), join(&[arc(0.24, 0.66, 0.18, 0.18, 170.0, 0.0), l(&[(0.42, 0.66), (0.42, 1.0)])])]),
        'i' => (0.25, vec![l(&[(0.12, 0.45), (0.12, 1.0)]), l(&[(0.12, 0.24), (0.125, 0.26)])]),
        'j' => (0.32, vec![join(&[l(&[(0.22, 0.45), (0.22, 1.12)]), arc(0.08, 1.12, 0.14, 0.15, 0.0, -140.0)]), l(&[(0.22, 0.24), (0.225, 0.26)])]),
        'k' => (0.46, vec![l(&[(0.06, 0.0), (0.06, 1.0)]), l(&[(0.4, 0.45), (0.07, 0.76)]), l(&[(0.17, 0.67), (0.43, 1.0)])]),
        'l' => (0.25, vec![l(&[(0.1, 0.0), (0.1, 0.9), (0.13, 0.98), (0.2, 1.0)])]),
        'm' => (0.6, vec![l(&[(0.05, 0.45), (0.05, 1.0)]), join(&[arc(0.17, 0.62, 0.12, 0.15, 170.0, 0.0), l(&[(0.29, 0.62), (0.29, 1.0)])]), join(&[arc(0.41, 0.62, 0.12, 0.15, 170.0, 0.0), l(&[(0.53, 0.62), (0.53, 1.0)])])]),
        'n' => (0.5, vec![l(&[(0.06, 0.45), (0.06, 1.0)]), join(&[arc(0.24, 0.64, 0.18, 0.18, 170.0, 0.0), l(&[(0.42, 0.64), (0.42, 1.0)])])]),
        'o' => (0.5, vec![arc(0.25, 0.725, 0.2, 0.27, 90.0, 450.0)]),
        'p' => (0.5, vec![l(&[(0.06, 0.45), (0.06, 1.3)]), bowl_r()]),
        'q' => (0.5, vec![bowl_l(), l(&[(0.42, 0.45), (0.42, 1.3)])]),
        'r' => (0.38, vec![l(&[(0.06, 0.45), (0.06, 1.0)]), arc(0.24, 0.66, 0.18, 0.18, 170.0, 60.0)]),
        's' => (0.44, vec![join(&[arc(0.22, 0.59, 0.16, 0.135, 40.0, 270.0), arc(0.22, 0.86, 0.17, 0.14, 90.0, -140.0)])]),
        't' => (0.4, vec![join(&[l(&[(0.18, 0.15), (0.18, 0.9)]), arc(0.28, 0.9, 0.1, 0.1, 180.0, 270.0)]), l(&[(0.04, 0.45), (0.36, 0.45)])]),
        'u' => (0.5, vec![join(&[l(&[(0.06, 0.45), (0.06, 0.8)]), arc(0.23, 0.8, 0.17, 0.2, 180.0, 360.0), l(&[(0.4, 0.45)])]), l(&[(0.4, 0.45), (0.42, 1.0)])]),
        'v' => (0.45, vec![l(&[(0.03, 0.45), (0.22, 1.0), (0.41, 0.45)])]),
        'w' => (0.57, vec![l(&[(0.02, 0.45), (0.15, 1.0), (0.28, 0.6), (0.41, 1.0), (0.54, 0.45)])]),
        'x' => (0.5, vec![l(&[(0.04, 0.45), (0.46, 1.0)]), l(&[(0.46, 0.45), (0.04, 1.0)])]),
        'y' => (0.52, vec![l(&[(0.04, 0.45), (0.26, 0.96)]), l(&[(0.49, 0.45), (0.27, 0.98), (0.14, 1.26)])]),
        'z' => (0.48, vec![l(&[(0.05, 0.45), (0.42, 0.45), (0.05, 1.0), (0.44, 1.0)])]),
        // Capitals.
        'A' => (0.55, vec![l(&[(0.02, 1.0), (0.27, 0.0), (0.52, 1.0)]), l(&[(0.11, 0.64), (0.43, 0.64)])]),
        'B' => (0.54, vec![join(&[l(&[(0.06, 1.0), (0.06, 0.0), (0.28, 0.0)]), arc(0.28, 0.24, 0.18, 0.24, 90.0, -90.0)]), join(&[l(&[(0.06, 0.48), (0.3, 0.48)]), arc(0.3, 0.74, 0.2, 0.26, 90.0, -90.0), l(&[(0.06, 1.0)])])]),
        'C' => (0.56, vec![arc(0.3, 0.5, 0.27, 0.5, 45.0, 315.0)]),
        'D' => (0.56, vec![join(&[l(&[(0.06, 0.0), (0.06, 1.0), (0.24, 1.0)]), arc(0.24, 0.5, 0.28, 0.5, -90.0, 90.0), l(&[(0.06, 0.0)])])]),
        'E' => (0.5, vec![l(&[(0.46, 0.0), (0.06, 0.0), (0.06, 1.0), (0.46, 1.0)]), l(&[(0.06, 0.5), (0.38, 0.5)])]),
        'F' => (0.48, vec![l(&[(0.46, 0.0), (0.06, 0.0), (0.06, 1.0)]), l(&[(0.06, 0.5), (0.36, 0.5)])]),
        'G' => (0.58, vec![join(&[arc(0.3, 0.5, 0.27, 0.5, 45.0, 330.0), l(&[(0.53, 0.55), (0.34, 0.55)])])]),
        'H' => (0.54, vec![l(&[(0.06, 0.0), (0.06, 1.0)]), l(&[(0.48, 0.0), (0.48, 1.0)]), l(&[(0.06, 0.5), (0.48, 0.5)])]),
        'I' => (0.2, vec![l(&[(0.1, 0.0), (0.1, 1.0)])]),
        'J' => (0.44, vec![join(&[l(&[(0.38, 0.0), (0.38, 0.72)]), arc(0.21, 0.72, 0.17, 0.28, 0.0, -170.0)])]),
        'K' => (0.5, vec![l(&[(0.06, 0.0), (0.06, 1.0)]), l(&[(0.46, 0.0), (0.07, 0.6)]), l(&[(0.2, 0.46), (0.48, 1.0)])]),
        'L' => (0.44, vec![l(&[(0.06, 0.0), (0.06, 1.0), (0.42, 1.0)])]),
        'M' => (0.62, vec![l(&[(0.04, 1.0), (0.07, 0.0), (0.31, 0.68), (0.55, 0.0), (0.58, 1.0)])]),
        'N' => (0.54, vec![l(&[(0.06, 1.0), (0.06, 0.0), (0.48, 1.0), (0.48, 0.0)])]),
        'O' => (0.6, vec![o()]),
        'P' => (0.5, vec![p_top()]),
        'Q' => (0.6, vec![o(), l(&[(0.36, 0.72), (0.58, 1.04)])]),
        'R' => (0.52, vec![p_top(), l(&[(0.26, 0.52), (0.5, 1.0)])]),
        'S' => (0.52, vec![join(&[arc(0.26, 0.25, 0.21, 0.24, 30.0, 270.0), arc(0.26, 0.74, 0.23, 0.26, 90.0, -150.0)])]),
        'T' => (0.56, vec![l(&[(0.02, 0.0), (0.54, 0.0)]), l(&[(0.28, 0.0), (0.28, 1.0)])]),
        'U' => (0.54, vec![join(&[l(&[(0.06, 0.0), (0.06, 0.68)]), arc(0.27, 0.68, 0.21, 0.32, 180.0, 360.0), l(&[(0.48, 0.0)])])]),
        'V' => (0.54, vec![l(&[(0.02, 0.0), (0.27, 1.0), (0.52, 0.0)])]),
        'W' => (0.68, vec![l(&[(0.02, 0.0), (0.18, 1.0), (0.34, 0.3), (0.5, 1.0), (0.66, 0.0)])]),
        'X' => (0.54, vec![l(&[(0.04, 0.0), (0.5, 1.0)]), l(&[(0.5, 0.0), (0.04, 1.0)])]),
        'Y' => (0.54, vec![l(&[(0.02, 0.0), (0.27, 0.5), (0.52, 0.0)]), l(&[(0.27, 0.5), (0.27, 1.0)])]),
        'Z' => (0.56, vec![l(&[(0.04, 0.0), (0.5, 0.0), (0.04, 1.0), (0.52, 1.0)])]),
        '°' => (0.3, vec![arc(0.14, 0.14, 0.1, 0.12, 90.0, 450.0)]),
        ':' => (0.22, vec![l(&[(0.1, 0.5), (0.11, 0.52)]), l(&[(0.1, 0.93), (0.11, 0.95)])]),
        ';' => (0.22, vec![l(&[(0.11, 0.5), (0.12, 0.52)]), l(&[(0.12, 0.9), (0.12, 0.97), (0.06, 1.12)])]),
        '!' => (0.2, vec![l(&[(0.1, 0.0), (0.1, 0.7)]), l(&[(0.1, 0.94), (0.11, 0.96)])]),
        '?' => (0.46, vec![join(&[arc(0.22, 0.25, 0.19, 0.22, 150.0, -60.0), l(&[(0.22, 0.62)])]), l(&[(0.22, 0.94), (0.23, 0.96)])]),
        '√' => (0.56, vec![l(&[(0.02, 0.58), (0.12, 0.52), (0.24, 1.0), (0.42, -0.04), (0.56, -0.04)])]),
        '<' => (0.44, vec![l(&[(0.4, 0.38), (0.06, 0.62), (0.4, 0.86)])]),
        '>' => (0.44, vec![l(&[(0.04, 0.38), (0.38, 0.62), (0.04, 0.86)])]),
        '≤' => (0.44, vec![l(&[(0.4, 0.3), (0.06, 0.52), (0.4, 0.74)]), l(&[(0.06, 0.9), (0.4, 0.9)])]),
        '≥' => (0.44, vec![l(&[(0.04, 0.3), (0.38, 0.52), (0.04, 0.74)]), l(&[(0.04, 0.9), (0.38, 0.9)])]),
        '{' => (0.34, vec![join(&[bez((0.3, -0.05), (0.14, -0.04), (0.16, 0.25)), bez((0.16, 0.25), (0.17, 0.5), (0.04, 0.52)), bez((0.04, 0.52), (0.17, 0.55), (0.16, 0.8)), bez((0.16, 0.8), (0.14, 1.08), (0.3, 1.08))])]),
        '}' => (0.34, vec![join(&[bez((0.04, -0.05), (0.2, -0.04), (0.18, 0.25)), bez((0.18, 0.25), (0.17, 0.5), (0.3, 0.52)), bez((0.3, 0.52), (0.17, 0.55), (0.18, 0.8)), bez((0.18, 0.8), (0.2, 1.08), (0.04, 1.08))])]),
        'α' => (0.54, vec![join(&[arc(0.24, 0.72, 0.19, 0.26, 20.0, 340.0), bez((0.4, 0.86), (0.46, 1.0), (0.52, 1.0))]), l(&[(0.4, 0.58), (0.48, 0.45)])]),
        'β' => (0.5, vec![join(&[l(&[(0.08, 1.3), (0.08, 0.2)]), arc(0.24, 0.24, 0.16, 0.2, 180.0, -90.0), arc(0.26, 0.72, 0.19, 0.28, 90.0, -110.0)])]),
        'γ' => (0.5, vec![join(&[bez((0.02, 0.5), (0.18, 0.4), (0.26, 0.95)), l(&[(0.24, 1.28)])]), l(&[(0.46, 0.45), (0.25, 0.98)])]),
        'δ' => (0.48, vec![join(&[bez((0.38, 0.05), (0.08, 0.0), (0.18, 0.28)), bez((0.18, 0.28), (0.25, 0.42), (0.36, 0.55)), arc(0.24, 0.75, 0.18, 0.25, 50.0, 410.0)])]),
        'ε' => (0.42, vec![join(&[arc(0.24, 0.6, 0.17, 0.14, 60.0, 270.0), arc(0.25, 0.86, 0.19, 0.14, 90.0, -70.0)])]),
        'θ' => (0.5, vec![arc(0.25, 0.5, 0.2, 0.5, 90.0, 450.0), l(&[(0.06, 0.5), (0.44, 0.5)])]),
        'φ' => (0.54, vec![arc(0.27, 0.72, 0.22, 0.27, 90.0, 450.0), l(&[(0.27, 0.3), (0.27, 1.3)])]),
        '∞' => (0.66, vec![join(&[arc(0.17, 0.66, 0.15, 0.17, 0.0, 360.0), arc(0.49, 0.66, 0.15, 0.17, 180.0, -180.0)])]),
        'ä' | 'ö' | 'ü' => {
            let (w, mut lines) = glyph(match c {
                'ä' => 'a',
                'ö' => 'o',
                _ => 'u',
            })?;
            lines.push(l(&[(0.14, 0.28), (0.15, 0.3)]));
            lines.push(l(&[(0.34, 0.28), (0.35, 0.3)]));
            (w, lines)
        }
        _ => return None,
    })
}

/// Letter spacing (of the cap height).
pub const GAP: f32 = 0.16;

/// A fraction written stacked: `FRAC_OPEN` numerator `FRAC_BAR` denominator `FRAC_CLOSE`
/// (they nest). Numbers written as `¹⁄₃` (small digits around a fraction slash, as the app
/// writes them) are stacked too.
pub const FRAC_OPEN: char = '\u{E000}';
pub const FRAC_BAR: char = '\u{E001}';
pub const FRAC_CLOSE: char = '\u{E002}';

/// A glyph where [`layout`] puts it: `x` from the start and `lift` (its box's top, + down) in
/// cap heights, its size (of the cap height).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub c: char,
    pub x: f32,
    pub lift: f32,
    pub size: f32,
}

/// A fraction's bar, from `x0` to `x1` at height `y` (cap heights, + down from the line's top),
/// in a fraction of `size`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
    pub size: f32,
}

/// A text laid out for the hand: its glyphs and fraction bars, how wide it is, how far its ink
/// reaches up (`top`, < 0: above the line's top) and down (`bottom`), in cap heights.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Laid {
    pub glyphs: Vec<Placed>,
    pub bars: Vec<Bar>,
    pub width: f32,
    pub top: f32,
    pub bottom: f32,
}

impl Laid {
    fn grow(&mut self, top: f32, bottom: f32) {
        if self.glyphs.is_empty() && self.bars.is_empty() {
            (self.top, self.bottom) = (top, bottom);
        } else {
            self.top = self.top.min(top);
            self.bottom = self.bottom.max(bottom);
        }
    }
}

const SUPS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];
const SUBS: [char; 10] = ['₀', '₁', '₂', '₃', '₄', '₅', '₆', '₇', '₈', '₉'];

/// How far a glyph's ink reaches up and down in its box (0: the top, 1: the line).
fn reach(c: char) -> (f32, f32) {
    match glyph(c) {
        Some((_, lines)) if lines.iter().any(|l| !l.is_empty()) => lines.iter().flatten().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1))),
        _ => (0.45, 1.0),
    }
}

/// `¹⁄₃` at `i`: the numerator's and the denominator's digits, and where it ends.
fn small_fraction(chars: &[char], i: usize) -> Option<(String, String, usize)> {
    let digits = |j: &mut usize, of: &[char; 10]| {
        let mut s = String::new();
        while let Some(d) = chars.get(*j).and_then(|c| of.iter().position(|s| s == c)) {
            s.push(char::from(b'0' + d as u8));
            *j += 1;
        }
        s
    };
    let mut j = i;
    let num = digits(&mut j, &SUPS);
    if num.is_empty() || chars.get(j) != Some(&'⁄') {
        return None;
    }
    j += 1;
    let den = digits(&mut j, &SUBS);
    (!den.is_empty()).then_some((num, den, j))
}

/// Lays out `text` for the hand: `_1` low and small, `²`, `^2` high and small, fractions
/// stacked (see [`FRAC_OPEN`]). Characters the font lacks take no room.
pub fn layout(text: &str) -> Laid {
    layout_at(text, 1.0)
}

fn layout_at(text: &str, size: f32) -> Laid {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Laid::default();
    let mut i = 0;
    out.width = lay(&chars, &mut i, size, &mut out, None);
    out
}

/// Lays out from `i` up to `stop` (or the end) at `size`, the line's top at lift 0, into `out`.
/// The width it takes.
fn lay(chars: &[char], i: &mut usize, size: f32, out: &mut Laid, stop: Option<char>) -> f32 {
    let mut x = 0.0f32;
    let mut last_gap = 0.0;
    let mut mode = 0u8;
    let put = |out: &mut Laid, x: &mut f32, last_gap: &mut f32, c: char, s: f32, lift: f32| {
        if let Some((w, _)) = glyph(c) {
            let (t, b) = reach(c);
            out.grow(lift + t * s, lift + b * s);
            out.glyphs.push(Placed { c, x: *x, lift, size: s });
            *x += w * s + GAP * s;
            *last_gap = GAP * s;
        }
    };
    while *i < chars.len() {
        let c = chars[*i];
        if Some(c) == stop {
            *i += 1;
            break;
        }
        // A fraction: the numerator over the denominator, both smaller, the bar at the minus
        // sign's height.
        let ns = size * 0.74;
        let parts = if c == FRAC_OPEN {
            *i += 1;
            let (mut num, mut den) = (Laid::default(), Laid::default());
            num.width = lay(chars, i, ns, &mut num, Some(FRAC_BAR));
            den.width = lay(chars, i, ns, &mut den, Some(FRAC_CLOSE));
            Some((num, den))
        } else if let Some((n, d, next)) = small_fraction(chars, *i) {
            *i = next;
            Some((layout_at(&n, ns), layout_at(&d, ns)))
        } else {
            None
        };
        if let Some((num, den)) = parts {
            let w = num.width.max(den.width) + 0.14 * size;
            let bar = 0.56 * size;
            let gap = 0.13 * size;
            for (part, dy) in [(&num, bar - gap - num.bottom), (&den, bar + gap - den.top)] {
                let dx = x + (w - part.width) / 2.0;
                out.grow(part.top + dy, part.bottom + dy);
                out.glyphs.extend(part.glyphs.iter().map(|g| Placed { x: g.x + dx, lift: g.lift + dy, ..*g }));
                out.bars.extend(part.bars.iter().map(|b| Bar { x0: b.x0 + dx, x1: b.x1 + dx, y: b.y + dy, ..*b }));
            }
            out.grow(bar, bar);
            out.bars.push(Bar { x0: x, x1: x + w, y: bar, size });
            x += w + GAP * size;
            last_gap = GAP * size;
            mode = 0;
            continue;
        }
        *i += 1;
        match c {
            '_' => {
                mode = 1;
                continue;
            }
            '^' => {
                mode = 2;
                continue;
            }
            '²' | '³' => {
                put(out, &mut x, &mut last_gap, if c == '²' { '2' } else { '3' }, size * 0.62, -0.2 * size);
                continue;
            }
            _ => {}
        }
        if mode != 0 && !(c.is_alphanumeric() || (mode == 2 && matches!(c, '-' | '−' | '.' | ','))) {
            mode = 0;
        }
        match mode {
            1 => put(out, &mut x, &mut last_gap, c, size * 0.62, 0.55 * size),
            2 => put(out, &mut x, &mut last_gap, c, size * 0.62, -0.2 * size),
            _ => put(out, &mut x, &mut last_gap, c, size, 0.0),
        }
    }
    (x - last_gap).max(0.0)
}

/// How wide `text` is at cap height `cap` (characters the font lacks take no room).
pub fn text_width(text: &str, cap: f32) -> f32 {
    layout(text).width * cap
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_glyph_stays_in_its_box() {
        let all = "0123456789-,. ()|'/=+·≈πabcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZäöü°:;!?√<>≤≥{}αβγδεθφ∞";
        for c in all.chars() {
            let (w, lines) = glyph(c).unwrap_or_else(|| panic!("no glyph for {c}"));
            assert!(w > 0.1, "{c}");
            for line in &lines {
                assert!(!line.is_empty());
                for &(x, y) in line {
                    assert!((-0.06..=w + 0.06).contains(&x) && (-0.1..=1.32).contains(&y), "{c}: ({x}, {y}) out of {w}");
                }
            }
        }
        assert!(glyph('€').is_none());
        assert!(text_width("-12,5", 10.0) > text_width("12", 10.0));
        assert_eq!(text_width("", 10.0), 0.0);
    }

    #[test]
    fn fractions_are_stacked() {
        // x/3 as a fraction: x above the bar, 3 under it, both smaller, the bar across both.
        let l = layout(&format!("{FRAC_OPEN}x{FRAC_BAR}3{FRAC_CLOSE}"));
        assert_eq!(l.bars.len(), 1);
        let (x, three, bar) = (l.glyphs[0], l.glyphs[1], l.bars[0]);
        assert!(x.lift + x.size <= bar.y && three.lift >= bar.y, "{l:?}");
        assert!(x.size < 1.0 && bar.x1 - bar.x0 >= text_width("3", three.size));
        // Taller than a line, narrower than written in a row.
        let third = layout(&format!("{FRAC_OPEN}1{FRAC_BAR}3{FRAC_CLOSE}"));
        assert!(third.top < 0.0 && third.bottom > 1.0 && l.bottom > 1.0, "{third:?}");
        assert!(l.width < layout("x/3").width);
        // ¹⁄₃ (how the app writes a third) is stacked the same way; ² stays a power.
        let t = layout("y = ¹⁄₃x²");
        assert_eq!(t.bars.len(), 1);
        assert!(t.glyphs.iter().any(|g| g.c == '1' && g.lift < 0.0) && t.glyphs.iter().any(|g| g.c == '3' && g.lift > 0.5), "{t:?}");
        assert!(t.glyphs.iter().any(|g| g.c == '2' && g.size < 1.0 && g.lift < 0.0));
        // Nested: (1/2)/x.
        let n = layout(&format!("{FRAC_OPEN}{FRAC_OPEN}1{FRAC_BAR}2{FRAC_CLOSE}{FRAC_BAR}x{FRAC_CLOSE}"));
        assert_eq!(n.bars.len(), 2);
        // Plain text as before.
        assert_eq!(layout("A_1").glyphs.len(), 2);
        assert!((text_width("12", 10.0) - (glyph('1').unwrap().0 + GAP + glyph('2').unwrap().0) * 10.0).abs() < 1e-4);
    }
}
