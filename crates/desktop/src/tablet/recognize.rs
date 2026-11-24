//! Handwriting → text, so written notes can be searched (like the Android app's Handwriting.kt,
//! which uses ML Kit). On Windows the system's offline ink recognizer reads the lines
//! (Windows.UI.Input.Inking; it needs the handwriting feature of a language, which Windows
//! installs with the language's "Handwriting" option). Lines are found and named with
//! [`n0te_core::inklines`], the same as on Android, and stored in the sheet's index, which
//! syncs: a line read on any device is searchable on all of them and never read twice.
//!
//! The work is split so the reading can run on a worker thread: [`prepare`] copies what is
//! needed from the sheet (UI thread), [`read`] recognizes (worker), the result goes back into
//! the sheet with `Sheet::set_index` (UI thread).

use std::collections::{HashMap, HashSet};

use n0te_core::ink::{InkPoint, KIND_MARKER};
use n0te_core::inklines::{self, Box as LineBox};
use n0te_core::sheet::{IndexLine, Sheet};

/// What the reader needs from one sheet.
pub struct Input {
    /// Lines not read yet: (key, strokes left to right with their points, box).
    fresh: Vec<(String, Vec<(String, Vec<InkPoint>)>, [f32; 4])>,
    /// Keys stored for lines that are gone.
    pub drop: Vec<String>,
    /// The text of lines read before, by key, and the reading order (for the text before a line).
    known: HashMap<String, String>,
    order: Vec<String>,
}

impl Input {
    pub fn is_empty(&self) -> bool {
        self.fresh.is_empty() && self.drop.is_empty()
    }
}

/// Whether this system can read handwriting at all.
pub fn available() -> bool {
    #[cfg(windows)]
    {
        win::available()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// The lines of `sheet` that changed since they were last read (None: nothing to do).
pub fn prepare(sheet: &Sheet) -> Option<Input> {
    let boxes: Vec<LineBox> = sheet
        .stroke_boxes()
        .into_iter()
        // Highlighter strokes say nothing.
        .filter(|(_, _, kind)| *kind != KIND_MARKER)
        .map(|(id, r, _)| LineBox::new(id, r.x0, r.y0, r.x1, r.y1))
        .collect();
    let lines = inklines::group(&boxes);
    let keyed: Vec<(String, &Vec<LineBox>)> = lines.iter().map(|l| (inklines::key(l), l)).collect();
    let stored: HashSet<String> = sheet.index_keys().into_iter().collect();
    let live: HashSet<&str> = keyed.iter().map(|(k, _)| k.as_str()).collect();
    let drop: Vec<String> = stored.iter().filter(|k| !live.contains(k.as_str())).cloned().collect();
    let known: HashMap<String, String> = sheet.index_lines().into_iter().map(|l| (l.key, l.text)).collect();
    let mut sorted: Vec<&(String, &Vec<LineBox>)> = keyed.iter().collect();
    let top = |l: &Vec<LineBox>| (l.iter().map(|b| b.y0).fold(f32::INFINITY, f32::min), l.iter().map(|b| b.x0).fold(f32::INFINITY, f32::min));
    sorted.sort_by(|a, b| {
        let (ay, ax) = top(a.1);
        let (by, bx) = top(b.1);
        ay.total_cmp(&by).then(ax.total_cmp(&bx))
    });
    let order: Vec<String> = sorted.iter().map(|(k, _)| k.clone()).collect();
    let mut fresh = Vec::new();
    for (k, line) in sorted {
        if stored.contains(k) {
            continue;
        }
        let mut l: Vec<&LineBox> = line.iter().collect();
        l.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        let strokes: Vec<(String, Vec<InkPoint>)> = l.iter().filter_map(|b| sheet.stroke(&b.id).ok().flatten().map(|s| (s.id, s.points))).collect();
        let r = [
            l.iter().map(|b| b.x0).fold(f32::INFINITY, f32::min),
            l.iter().map(|b| b.y0).fold(f32::INFINITY, f32::min),
            l.iter().map(|b| b.x1).fold(f32::NEG_INFINITY, f32::max),
            l.iter().map(|b| b.y1).fold(f32::NEG_INFINITY, f32::max),
        ];
        fresh.push((k.clone(), strokes, r));
    }
    let input = Input { fresh, drop, known, order };
    (!input.is_empty()).then_some(input)
}

/// Reads the fresh lines (worker thread): the lines to store and the keys to drop.
pub fn read(input: Input) -> (Vec<IndexLine>, Vec<String>) {
    let mut put = Vec::new();
    let mut before = String::new();
    let fresh: HashMap<&str, &(String, Vec<(String, Vec<InkPoint>)>, [f32; 4])> = input.fresh.iter().map(|f| (f.0.as_str(), f)).collect();
    for k in &input.order {
        let Some((key, strokes, r)) = fresh.get(k.as_str()) else {
            if let Some(t) = input.known.get(k) {
                before = t.clone();
            }
            continue;
        };
        let pts: Vec<&[InkPoint]> = strokes.iter().map(|(_, p)| p.as_slice()).collect();
        let read = recognize(&pts, &before);
        let Some(text) = read.first().cloned() else { continue };
        put.push(IndexLine { key: key.clone(), text: text.clone(), alts: read.into_iter().skip(1).take(2).collect(), strokes: strokes.iter().map(|(id, _)| id.clone()).collect(), x0: r[0], y0: r[1], x1: r[2], y1: r[3] });
        before = text;
    }
    (put, input.drop)
}

/// Up to three readings of one line of strokes, best first (empty: nothing read).
fn recognize(strokes: &[&[InkPoint]], before: &str) -> Vec<String> {
    #[cfg(windows)]
    {
        let _ = before;
        win::recognize(strokes).unwrap_or_default()
    }
    #[cfg(not(windows))]
    {
        let _ = (strokes, before);
        Vec::new()
    }
}

#[cfg(windows)]
mod win {
    use n0te_core::ink::InkPoint;
    use windows_collections::IIterable;
    use windows_numerics::Matrix3x2;
    use windows::Foundation::Point;
    use windows::UI::Input::Inking::{InkPoint as WinPoint, InkRecognitionTarget, InkRecognizerContainer, InkStrokeBuilder, InkStrokeContainer};

    pub fn available() -> bool {
        InkRecognizerContainer::new().and_then(|c| c.GetRecognizers()).map(|r| r.Size().unwrap_or(0) > 0).unwrap_or(false)
    }

    pub fn recognize(strokes: &[&[InkPoint]]) -> windows::core::Result<Vec<String>> {
        let builder = InkStrokeBuilder::new()?;
        let container = InkStrokeContainer::new()?;
        let identity = Matrix3x2 { M11: 1.0, M12: 0.0, M21: 0.0, M22: 1.0, M31: 0.0, M32: 0.0 };
        for s in strokes {
            // WinRT objects in a collection are nullable: Option<InkPoint>.
            let pts: Vec<Option<WinPoint>> = s.iter().filter_map(|p| WinPoint::CreateInkPoint(Point { X: p.x, Y: p.y }, p.pressure.clamp(0.05, 1.0)).ok()).map(Some).collect();
            if pts.len() < 2 {
                continue;
            }
            let it: IIterable<WinPoint> = pts.into();
            let stroke = builder.CreateStrokeFromInkPoints(&it, identity)?;
            container.AddStroke(&stroke)?;
        }
        let rec = InkRecognizerContainer::new()?;
        let results = rec.RecognizeAsync(&container, InkRecognitionTarget::All)?.join()?;
        // One result per word: the line is the words' best readings; the others vary the first word.
        let mut best = Vec::new();
        let mut alts: Vec<Vec<String>> = Vec::new();
        for r in &results {
            let c = r.GetTextCandidates()?;
            let words: Vec<String> = (0..c.Size()?).filter_map(|i| c.GetAt(i).ok()).map(|h| h.to_string()).collect();
            if let Some(w) = words.first() {
                best.push(w.clone());
            }
            alts.push(words);
        }
        if best.is_empty() {
            return Ok(Vec::new());
        }
        let line = best.join(" ");
        let mut out = vec![line];
        for i in 1..3 {
            if let Some(w) = alts.first().and_then(|a| a.get(i)) {
                let mut v = best.clone();
                v[0] = w.clone();
                out.push(v.join(" "));
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use n0te_core::ink::Brush;

    fn word(sheet: &Sheet, x: f32, y: f32, letters: usize) {
        for i in 0..letters {
            let x0 = x + i as f32 * 9.0;
            let pts: Vec<InkPoint> = (0..8).map(|k| InkPoint { x: x0 + k as f32, y: y + if k % 2 == 0 { 0.0 } else { 11.0 }, pressure: 0.5, t_ms: k * 10 }).collect();
            sheet.add_stroke(Brush::default(), &pts).unwrap();
        }
    }

    #[test]
    fn lines_to_read_are_found_and_read_ones_are_skipped() {
        let sheet = Sheet::create(1).unwrap();
        word(&sheet, 10.0, 100.0, 5);
        word(&sheet, 10.0, 140.0, 4);
        let input = prepare(&sheet).expect("two lines to read");
        assert_eq!(input.fresh.len(), 2);
        assert!(input.fresh[0].2[1] < input.fresh[1].2[1], "top line first");
        // Pretend the first was read: it isn't offered again; a line whose strokes went is dropped.
        let first = &input.fresh[0];
        let line = IndexLine { key: first.0.clone(), text: "hallo".into(), alts: vec![], strokes: first.1.iter().map(|(id, _)| id.clone()).collect(), x0: 0.0, y0: 0.0, x1: 1.0, y1: 1.0 };
        sheet.set_index(&[line], &[]).unwrap();
        let again = prepare(&sheet).expect("one line left");
        assert_eq!(again.fresh.len(), 1);
        assert!(again.drop.is_empty());
        // Without a recognizer (Linux) nothing is read, nothing breaks.
        let (put, drop) = read(again);
        assert!(put.is_empty() && drop.is_empty());
    }
}
