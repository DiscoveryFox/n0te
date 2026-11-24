//! The lines of handwriting on a page, from the strokes' boxes alone: strokes about as tall as
//! writing (not drawings, long rules or shapes) that lie at the same height and close together
//! form a line; a wide gap starts another one (two columns, a formula beside the text).
//!
//! The same rules as the Android app's `InkLines` (Handwriting.kt), including the line key, so
//! a line read on one device is never read again on another.

/// A stroke's box (sheet points).
#[derive(Debug, Clone, PartialEq)]
pub struct Box {
    pub id: String,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Box {
    pub fn new(id: impl Into<String>, x0: f32, y0: f32, x1: f32, y1: f32) -> Box {
        Box { id: id.into(), x0, y0, x1, y1 }
    }
    fn h(&self) -> f32 {
        self.y1 - self.y0
    }
    fn cy(&self) -> f32 {
        (self.y0 + self.y1) / 2.0
    }
}

fn median_height(boxes: &[Box], clamp: bool) -> f32 {
    let mut hs: Vec<f32> = boxes.iter().map(Box::h).filter(|h| *h > 2.0).collect();
    hs.sort_by(f32::total_cmp);
    match hs.get(hs.len() / 2) {
        Some(m) if clamp => m.clamp(4.0, 60.0),
        Some(m) => *m,
        None => 10.0,
    }
}

/// Groups the strokes into lines of writing (each left to right is not guaranteed; see [`key`]).
pub fn group(all: &[Box]) -> Vec<Vec<Box>> {
    if all.is_empty() {
        return Vec::new();
    }
    // How tall writing is on this page: the middle of the strokes' heights (dots aside).
    let med = median_height(all, true);
    let text: Vec<&Box> = all.iter().filter(|b| b.h() <= med * 3.5 && (b.x1 - b.x0) <= med * 14.0).collect();
    struct Line {
        top: f32,
        bottom: f32,
        left: f32,
        right: f32,
        boxes: Vec<Box>,
    }
    let mut lines: Vec<Line> = Vec::new();
    // The letters first: strokes at the same height form a line.
    let (small, mut big): (Vec<&Box>, Vec<&Box>) = text.into_iter().partition(|b| b.h() < med * 0.45);
    big.sort_by(|a, b| a.cy().total_cmp(&b.cy()));
    for b in big {
        let overlap = |l: &Line| b.y1.min(l.bottom) - b.y0.max(l.top);
        let best = lines
            .iter_mut()
            .max_by(|a, c| overlap(a).total_cmp(&overlap(c)))
            .filter(|l| overlap(l) >= 0.4 * b.h().min(l.bottom - l.top));
        match best {
            None => lines.push(Line { top: b.y0, bottom: b.y1, left: b.x0, right: b.x1, boxes: vec![b.clone()] }),
            Some(l) => {
                l.boxes.push(b.clone());
                l.top = l.top.min(b.y0);
                l.bottom = l.bottom.max(b.y1);
                l.left = l.left.min(b.x0);
                l.right = l.right.max(b.x1);
            }
        }
    }
    // Then dots, commas, dashes: with the line they sit on, over or under (a lone one is no writing).
    for b in small {
        let best = lines
            .iter_mut()
            .filter(|l| b.cy() >= l.top - med * 0.7 && b.cy() <= l.bottom + med * 0.7 && b.x1 >= l.left - med && b.x0 <= l.right + med)
            .min_by(|a, c| (b.cy() - (a.top + a.bottom) / 2.0).abs().total_cmp(&(b.cy() - (c.top + c.bottom) / 2.0).abs()));
        if let Some(l) = best {
            l.boxes.push(b.clone());
        }
    }
    // Wide gaps split a line.
    let mut out = Vec::new();
    for mut l in lines {
        l.boxes.sort_by(|a, b| a.x0.total_cmp(&b.x0));
        let mut cur: Vec<Box> = Vec::new();
        let mut right = f32::NEG_INFINITY;
        for b in l.boxes {
            if !cur.is_empty() && b.x0 - right > med * 3.0 {
                out.push(std::mem::take(&mut cur));
            }
            right = right.max(b.x1);
            cur.push(b);
        }
        if !cur.is_empty() {
            out.push(cur);
        }
    }
    out
}

/// The word stroke `id` belongs to: the strokes of its line close enough together (a gap of
/// more than about half the writing's height parts words). A stroke that is no writing is a
/// word of its own.
pub fn word(all: &[Box], id: &str) -> Vec<String> {
    let Some(line) = group(all).into_iter().find(|l| l.iter().any(|b| b.id == id)) else { return vec![id.to_string()] };
    let med = median_height(&line, false);
    let mut sorted = line;
    sorted.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    let mut words: Vec<Vec<Box>> = Vec::new();
    let mut right = f32::NEG_INFINITY;
    for b in sorted {
        let x1 = b.x1;
        match words.last_mut() {
            Some(w) if b.x0 - right <= med * 0.55 => w.push(b),
            _ => words.push(vec![b]),
        }
        right = right.max(x1);
    }
    words.into_iter().find(|w| w.iter().any(|b| b.id == id)).map(|w| w.into_iter().map(|b| b.id).collect()).unwrap_or_else(|| vec![id.to_string()])
}

/// A name for a line from its strokes (the same strokes: the same line, read once): the first
/// 8 bytes of the SHA-1 of the sorted ids joined by commas, in hex (as on Android).
pub fn key(line: &[Box]) -> String {
    let mut ids: Vec<&str> = line.iter().map(|b| b.id.as_str()).collect();
    ids.sort_unstable();
    let d = sha1_smol::Sha1::from(ids.join(",")).digest().bytes();
    d[..8].iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_at_one_height_form_a_line_and_a_gap_splits_it() {
        // Two words on one line, a second line below, a far-away formula beside the first.
        let mut all = Vec::new();
        for i in 0..5 {
            all.push(Box::new(format!("a{i}"), 10.0 + i as f32 * 9.0, 100.0, 17.0 + i as f32 * 9.0, 112.0));
        }
        all.push(Box::new("dot", 12.0, 96.0, 13.0, 97.0));
        for i in 0..4 {
            all.push(Box::new(format!("b{i}"), 10.0 + i as f32 * 9.0, 130.0, 17.0 + i as f32 * 9.0, 142.0));
        }
        all.push(Box::new("far", 300.0, 101.0, 320.0, 111.0));
        // A big drawing: no writing.
        all.push(Box::new("drawing", 0.0, 0.0, 400.0, 300.0));
        let lines = group(&all);
        assert_eq!(lines.len(), 3, "{lines:?}");
        let first = lines.iter().find(|l| l.iter().any(|b| b.id == "a0")).unwrap();
        assert!(first.iter().any(|b| b.id == "dot"), "the dot sits with its line");
        assert!(!first.iter().any(|b| b.id == "far"), "a wide gap splits the line");
        assert!(lines.iter().all(|l| l.iter().all(|b| b.id != "drawing")));
    }

    #[test]
    fn words_and_keys() {
        let all = vec![Box::new("a", 0.0, 0.0, 8.0, 10.0), Box::new("b", 9.0, 0.0, 17.0, 10.0), Box::new("c", 30.0, 0.0, 38.0, 10.0)];
        assert_eq!(word(&all, "a"), vec!["a", "b"]);
        assert_eq!(word(&all, "c"), vec!["c"]);
        // Order doesn't matter: the key is over the sorted ids, like Android's.
        let shuffled = key(&[all[2].clone(), all[0].clone(), all[1].clone()]);
        let expect: String = sha1_smol::Sha1::from("a,b,c").digest().bytes()[..8].iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!((key(&all), shuffled), (expect.clone(), expect));
        // A known vector: SHA-1("abc") starts a9993e364706816a.
        assert_eq!(key(&[Box::new("abc", 0.0, 0.0, 1.0, 1.0)]), "a9993e364706816a");
    }
}
