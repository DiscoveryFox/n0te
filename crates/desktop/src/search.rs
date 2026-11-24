//! Finding things in the open notebook — headings, handwriting as the tablet read it, typed
//! text, the PDF behind a page — and the headings under each page in the outline. Both read
//! every sheet, so they run in the background (iced tasks) with their own handle on the
//! library; the sheets being edited here go in as they are on screen (unsaved text counts).

use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use n0te_core::notebook::{Notebook, OutlineRow, SheetSpec};
use n0te_core::search::{Hit, HitKind, normalize, score, words};
use n0te_core::sheet::{Heading, Sheet};
use n0te_core::store::Library;

use crate::pdf::Pdf;
use crate::sidebar::page_name;

/// The pause after the last key before the search runs.
pub const DEBOUNCE: Duration = Duration::from_millis(250);
/// How long a spot that was jumped to lights up on its page.
pub const FLASH: Duration = Duration::from_millis(1500);

/// The search field and what it found.
#[derive(Default)]
pub struct Search {
    pub query: String,
    /// Bumped by every key: only the last key's timer starts the search.
    pub typed: u64,
    /// Bumped by every search started and shared with the running one: an outdated search
    /// stops early.
    pub latest: Arc<AtomicU64>,
    /// The results shown and the query they answer (None: the first search is still running).
    pub found: Option<(String, Vec<PageHits>)>,
    /// A search is under way.
    pub busy: bool,
    /// The notebook changed while the field had text: search again.
    pub stale: bool,
}

impl Search {
    /// Text in the field: the sidebar shows the results instead of the outline.
    pub fn active(&self) -> bool {
        !self.query.trim().is_empty()
    }

    /// Empties the field; a timer or search still under way comes to nothing.
    pub fn clear(&mut self) {
        self.query.clear();
        self.found = None;
        self.busy = false;
        self.stale = false;
        self.typed += 1;
        self.latest.fetch_add(1, Ordering::Relaxed);
    }

    /// Every hit shown, in order, with the page it is on.
    pub fn hits(&self) -> impl Iterator<Item = (&PageHits, &Hit)> {
        self.found.iter().flat_map(|(_, pages)| pages.iter().flat_map(|p| p.hits.iter().map(move |h| (p, h))))
    }
}

/// A sheet of the notebook: its page number (side sheets carry their main page's), its side
/// (0 main, -1, -2 … left, 1, 2 … right), its own title (main sheets) and what it is.
#[derive(Debug, Clone)]
pub struct Target {
    pub sheet_id: String,
    pub number: u32,
    pub side: i32,
    pub title: String,
    pub spec: SheetSpec,
}

/// Every sheet in outline order — each main sheet, then its side sheets (left, then right) —
/// also those in folded groups, numbered like the pages.
pub fn targets(nb: &Notebook) -> Vec<Target> {
    let mut out = Vec::new();
    let mut number = 0;
    for row in nb.outline(true) {
        if let OutlineRow::Sheet { sheet_id, title, spec, left, right, .. } = row {
            number += 1;
            out.push(Target { sheet_id, number, side: 0, title, spec });
            for (i, s) in left.into_iter().enumerate() {
                out.push(Target { sheet_id: s.sheet_id, number, side: -(i as i32 + 1), title: String::new(), spec: s.spec });
            }
            for (i, s) in right.into_iter().enumerate() {
                out.push(Target { sheet_id: s.sheet_id, number, side: i as i32 + 1, title: String::new(), spec: s.spec });
            }
        }
    }
    out
}

/// What the search found on one page.
#[derive(Debug, Clone, PartialEq)]
pub struct PageHits {
    pub sheet_id: String,
    pub number: u32,
    pub side: i32,
    /// Its name: its title, else its first heading, else what it is ("PDF-Seite 3").
    pub name: String,
    /// Best first, then top to bottom.
    pub hits: Vec<Hit>,
}

/// One search: what to look for, where, and the sheets being edited as they are on screen.
pub struct Job {
    pub root: PathBuf,
    pub notebook: String,
    pub query: String,
    pub targets: Vec<Target>,
    /// Snapshots of the sheets being edited here, by sheet id (read instead of the stored ones).
    pub open: HashMap<String, Vec<u8>>,
    pub pdf: Option<Arc<Pdf>>,
    /// The search's number; it stops once `latest` has moved on.
    pub generation: u64,
    pub latest: Arc<AtomicU64>,
}

/// Runs a search (off the UI thread: it reads every sheet, and the PDF pages' text through the
/// PDF worker): the pages with hits in outline order, on each the best hits first. None if a
/// newer search started meanwhile.
pub fn run(job: &Job) -> Option<Vec<PageHits>> {
    let w = words(&job.query);
    let mut out = Vec::new();
    if w.is_empty() {
        return Some(out);
    }
    let lib = Library::open(&job.root).ok()?;
    for t in &job.targets {
        if job.latest.load(Ordering::Relaxed) != job.generation {
            return None;
        }
        let sheet = match job.open.get(&t.sheet_id) {
            Some(bytes) => Sheet::load(lib.peer(), bytes).ok(),
            None => lib.load_sheet(&job.notebook, &t.sheet_id).ok(),
        };
        let width = if t.spec.width > 0.0 { t.spec.width } else { 595.0 };
        let mut hits = sheet.as_ref().map(|s| s.search(&job.query, width)).unwrap_or_default();
        if let (Some(pdf), Some((blob, page))) = (&job.pdf, &t.spec.pdf) {
            hits.extend(pdf_hits(&w, &pdf.page_lines(blob, *page), &t.spec));
            hits.sort_by(|a, b| a.slips.cmp(&b.slips).then(a.y0.total_cmp(&b.y0)));
        }
        if hits.is_empty() {
            continue;
        }
        let first = sheet.as_ref().and_then(|s| s.headings().into_iter().map(|h| h.text).find(|t| !t.trim().is_empty()));
        let line = sheet.as_ref().and_then(|s| crate::pages::first_line(&s.page_text()));
        out.push(PageHits { sheet_id: t.sheet_id.clone(), number: t.number, side: t.side, name: page_name(&t.title, &t.spec, first.as_deref(), line.as_deref()), hits });
    }
    Some(out)
}

/// The lines of a PDF page (as [`Pdf::page_lines`] gives them) that hold the query's `words`,
/// where they lie on the page now: space inserted into the page moves the PDF below it down.
pub fn pdf_hits(words: &[String], lines: &[(String, n0te_pdf::Rect)], spec: &SheetSpec) -> Vec<Hit> {
    let bands = spec.bands();
    lines
        .iter()
        .filter_map(|(text, r)| {
            let slips = score(words, &normalize(text))?;
            let shift = bands.iter().find(|b| r.y0 >= b.src && r.y0 < b.src + b.len).map(|b| b.dst - b.src).unwrap_or(0.0);
            Some(Hit { kind: HitKind::Pdf, text: text.clone(), x0: r.x0, y0: r.y0 + shift, x1: r.x1, y1: r.y1 + shift, slips })
        })
        .collect()
}

/// The headings of `sheets` (top to bottom on each), read from the library (in the background:
/// a notebook has many sheets).
pub fn read_headings(root: &Path, notebook: &str, sheets: &[String]) -> Vec<(String, Vec<Heading>)> {
    let Ok(lib) = Library::open(root) else { return Vec::new() };
    sheets.iter().map(|s| (s.clone(), lib.load_sheet(notebook, s).map(|sheet| sheet.headings()).unwrap_or_default())).collect()
}

/// Ready after `d`. The app's executor has no timers, so a short-lived thread does the waiting
/// (no worker of the pool sleeps).
pub async fn after(d: Duration) {
    let (tx, rx) = iced::futures::channel::oneshot::channel();
    std::thread::spawn(move || {
        std::thread::sleep(d);
        let _ = tx.send(());
    });
    let _ = rx.await;
}

/// Where the query's `words` (normalized, see [`n0te_core::search::words`]) stand in `text`:
/// byte ranges, in order, merged. Matched like the search does — case, umlauts (ä = ae) and
/// punctuation don't matter; a slip of the recogniser isn't marked.
pub fn emphasis(text: &str, words: &[String]) -> Vec<Range<usize>> {
    // The text normalized char by char, each with the bytes of the character it came from.
    let mut norm: Vec<(char, usize, usize)> = Vec::new();
    for (i, c) in text.char_indices() {
        let end = i + c.len_utf8();
        for lc in c.to_lowercase() {
            let rep = match lc {
                'ä' => "ae",
                'ö' => "oe",
                'ü' => "ue",
                'ß' => "ss",
                _ => "",
            };
            if !rep.is_empty() {
                norm.extend(rep.chars().map(|r| (r, i, end)));
            } else if lc.is_alphanumeric() {
                norm.push((lc, i, end));
            } else {
                norm.push((' ', i, end));
            }
        }
    }
    let chars: Vec<char> = norm.iter().map(|n| n.0).collect();
    let mut found: Vec<Range<usize>> = Vec::new();
    for w in words {
        let w: Vec<char> = w.chars().collect();
        if w.is_empty() || w.len() > chars.len() {
            continue;
        }
        let mut i = 0;
        while i + w.len() <= chars.len() {
            if chars[i..i + w.len()] == w[..] {
                found.push(norm[i].1..norm[i + w.len() - 1].2);
                i += w.len();
            } else {
                i += 1;
            }
        }
    }
    found.sort_by_key(|r| (r.start, r.end));
    let mut out: Vec<Range<usize>> = Vec::new();
    for r in found {
        match out.last_mut() {
            Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
            _ => out.push(r),
        }
    }
    out
}

/// At most about `max` characters of `text`, around its first marked word ("… context …").
pub fn excerpt(text: &str, words: &[String], max: usize) -> String {
    let text = text.trim();
    let n = text.chars().count();
    if n <= max {
        return text.to_string();
    }
    let first = emphasis(text, words).first().map(|r| text[..r.start].chars().count()).unwrap_or(0);
    let start = first.saturating_sub(max / 3).min(n - max);
    let body: String = text.chars().skip(start).take(max).collect();
    format!("{}{}{}", if start > 0 { "…" } else { "" }, body.trim(), if start + max < n { "…" } else { "" })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn marked<'a>(text: &'a str, query: &str) -> Vec<&'a str> {
        emphasis(text, &words(query)).into_iter().map(|r| &text[r]).collect()
    }

    #[test]
    fn marks_the_query_words_like_the_search_matches_them() {
        assert_eq!(marked("Übung zur Konvergenz", "uebung"), vec!["Übung"]);
        assert_eq!(marked("Grenzwert der Folge, Grenzwerte", "grenzwert"), vec!["Grenzwert", "Grenzwert"]);
        assert_eq!(marked("Satz 3: Grenzwert-Sätze", "sätze grenzwert"), vec!["Grenzwert", "Sätze"]);
        assert_eq!(marked("STRASSE", "straße"), vec!["STRASSE"]);
        // Overlapping words merge into one mark; nothing found, nothing marked.
        assert_eq!(marked("Konvergenz", "konv vergenz"), vec!["Konvergenz"]);
        assert!(marked("Integral", "grenzwert").is_empty());
    }

    #[test]
    fn long_lines_are_cut_around_the_first_mark() {
        let line = "Am Anfang steht ein langer Satz über vieles, dann endlich der Grenzwert der Folge und noch mehr Text danach bis zum Ende";
        let e = excerpt(line, &words("grenzwert"), 40);
        assert!(e.starts_with('…') && e.ends_with('…') && e.contains("Grenzwert"), "{e}");
        assert!(e.chars().count() <= 42);
        assert_eq!(excerpt("kurz", &words("x"), 40), "kurz");
    }

    #[test]
    fn pdf_lines_below_inserted_space_move_down() {
        let r = |y0: f32| n0te_pdf::Rect { x0: 72.0, y0, x1: 200.0, y1: y0 + 20.0 };
        let lines = vec![("Folie 1: Grenzwerte".to_string(), r(80.0)), ("Grenzwerte, zweiter Teil".to_string(), r(500.0)), ("Integral".to_string(), r(600.0))];
        let spec = SheetSpec { height: 942.0, gaps: vec![(300.0, 100.0)], pdf: Some(("b".repeat(64), 0)), ..SheetSpec::a4(n0te_core::notebook::Background::Blank) };
        let hits = pdf_hits(&words("grenzwerte"), &lines, &spec);
        assert_eq!(hits.len(), 2);
        assert_eq!((hits[0].kind, hits[0].y0), (HitKind::Pdf, 80.0));
        assert_eq!(hits[1].y0, 600.0, "100 pt of space above it");
    }

    #[test]
    fn every_sheet_in_outline_order_numbered_like_the_pages() {
        use n0te_core::notebook::{Background, Side};
        let nb = Notebook::create(1, "t").unwrap();
        let (a, _) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Grid)).unwrap();
        nb.add_side_sheet(&a, Side::Right, 0, &SheetSpec::a4(Background::Text)).unwrap();
        let g = nb.add_group(None, 1, "Woche 1").unwrap();
        nb.add_sheet(Some(&g), 0, &SheetSpec::a4(Background::Lines)).unwrap();
        nb.set_collapsed(&g, true).unwrap();
        nb.add_sheet(None, 2, &SheetSpec::board()).unwrap();
        let t = targets(&nb);
        assert_eq!(t.iter().map(|t| (t.number, t.side)).collect::<Vec<_>>(), vec![(1, 0), (1, 1), (2, 0), (3, 0)]);
        assert!(t[3].spec.is_board());
    }
}
