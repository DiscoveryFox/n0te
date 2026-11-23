//! Finding words in notes: handwriting as the tablet read it, typed text, headings, PDF text.
//! Forgiving: case, umlauts (ä = ae) and punctuation don't matter, and a slip of the recogniser
//! — one letter in five wrong, missing or extra — still matches.

/// `s` lower-cased, umlauts spelled out (ä → ae, ß → ss), everything but letters and digits a
/// single space.
pub fn normalize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = true;
    for c in s.chars().flat_map(|c| c.to_lowercase()) {
        let rep: &str = match c {
            'ä' => "ae",
            'ö' => "oe",
            'ü' => "ue",
            'ß' => "ss",
            _ => "",
        };
        if !rep.is_empty() {
            out.push_str(rep);
            space = false;
        } else if c.is_alphanumeric() {
            out.push(c);
            space = false;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    out.trim_end().to_string()
}

/// The words of a query, normalized.
pub fn words(query: &str) -> Vec<String> {
    normalize(query).split(' ').filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// How far `word` is from its best match anywhere inside `text` (edits: a letter wrong, missing
/// or extra), if within the slips allowed (one per five letters; short words exactly).
fn fuzzy_find(word: &str, text: &str) -> Option<usize> {
    if text.contains(word) {
        return Some(0);
    }
    let k = word.chars().count() / 5;
    if k == 0 {
        return None;
    }
    // Sellers: edit distance of `word` to any substring of `text`.
    let p: Vec<char> = word.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let mut prev: Vec<usize> = (0..=p.len()).collect();
    let mut best = usize::MAX;
    for &tc in &t {
        let mut cur = vec![0usize; p.len() + 1];
        for i in 1..=p.len() {
            let sub = prev[i - 1] + usize::from(p[i - 1] != tc);
            cur[i] = sub.min(prev[i] + 1).min(cur[i - 1] + 1);
        }
        best = best.min(cur[p.len()]);
        prev = cur;
    }
    (best <= k).then_some(best)
}

/// Whether all the query's `words` are in `text` (normalized), in any order: the slips it took
/// (0 = exact), or None.
pub fn score(words: &[String], text: &str) -> Option<usize> {
    if words.is_empty() {
        return None;
    }
    let mut total = 0;
    for w in words {
        total += fuzzy_find(w, text)?;
    }
    Some(total)
}

/// One thing found on a page: what kind, its text (as shown), where (page points), how well.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub kind: HitKind,
    pub text: String,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    /// Slips it took to match (0 = exact).
    pub slips: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitKind {
    /// Handwriting (as recognised).
    Ink,
    /// A typed text box or a text page.
    Text,
    /// A heading the user marked.
    Heading,
    /// The PDF behind the page.
    Pdf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forgiving_matches() {
        assert_eq!(normalize("Grenzwert-Sätze: Übung 3!"), "grenzwert saetze uebung 3");
        let w = words("Grenzwert");
        assert_eq!(score(&w, &normalize("Der Grenzwert ist 2")), Some(0));
        // The recogniser read one letter wrong.
        assert_eq!(score(&w, &normalize("Der Grenzwerf ist 2")), Some(1));
        // Short words must match exactly; every word must be there.
        assert_eq!(score(&words("ist"), &normalize("isf")), None);
        assert_eq!(score(&words("Grenzwert Folge"), &normalize("Grenzwert der Reihe")), None);
        assert_eq!(score(&words("folge grenzwert"), &normalize("Grenzwert der Folge")), Some(0));
        assert_eq!(score(&words("Uebung"), &normalize("Übung 3")), Some(0));
        assert_eq!(score(&[], "x"), None);
    }
}
