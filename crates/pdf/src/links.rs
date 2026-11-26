//! A PDF's links and its table of contents (pdfium's link annotations and bookmarks).

use pdfium_render::prelude::PdfBookmark;

use crate::{PdfDoc, Rect, Result, display_to_user, lock};

/// Where a link in a PDF goes.
#[derive(Debug, Clone, PartialEq)]
pub enum LinkTarget {
    /// A page of the same PDF (0-based).
    Page(u32),
    /// A web address (or another URI).
    Uri(String),
}

/// A link on a page: its area (page coordinates: points, top-left origin) and where it goes.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfLink {
    pub rect: Rect,
    pub target: LinkTarget,
}

/// An entry of the table of contents: its title, the page it opens (0-based, if it opens one
/// in this PDF) and its sub-entries.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineEntry {
    pub title: String,
    pub page: Option<u32>,
    pub children: Vec<OutlineEntry>,
}

/// At most this many links per page and entries in a table of contents, and this deep (a
/// hostile PDF can make its bookmarks go round in circles).
const MAX_LINKS: usize = 2000;
const MAX_ENTRIES: usize = 5000;
const MAX_DEPTH: usize = 12;

impl PdfDoc {
    /// The links on `page` that go somewhere (a page of this PDF or a URI).
    pub fn page_links(&self, page: u32) -> Result<Vec<PdfLink>> {
        let _g = lock();
        let pages = self.doc.pages().len() as u32;
        self.with_page(page, |p| {
            let to_display = display_to_user(p)?.invert();
            let mut out = Vec::new();
            for link in p.links().iter() {
                let Ok(r) = link.rect() else { continue };
                let rect = to_display.map_rect(r.left().value as f64, r.bottom().value as f64, r.right().value as f64, r.top().value as f64);
                if rect.is_empty() || !rect.width().is_finite() || !rect.height().is_finite() {
                    continue;
                }
                let to_page = |i: i32| u32::try_from(i).ok().filter(|&i| i < pages).map(LinkTarget::Page);
                let target = match link.destination() {
                    Some(d) => d.page_index().ok().and_then(to_page),
                    None => link.action().and_then(|a| {
                        a.as_local_destination_action()
                            .and_then(|l| l.destination().ok())
                            .and_then(|d| d.page_index().ok())
                            .and_then(to_page)
                            .or_else(|| a.as_uri_action().and_then(|u| u.uri().ok()).map(|u| u.trim().to_string()).filter(|u| !u.is_empty() && u.len() < 4096).map(LinkTarget::Uri))
                    }),
                };
                if let Some(target) = target {
                    out.push(PdfLink { rect, target });
                    if out.len() >= MAX_LINKS {
                        break;
                    }
                }
            }
            Ok(out)
        })
    }

    /// The table of contents (empty when the PDF has none).
    pub fn outline(&self) -> Vec<OutlineEntry> {
        let _g = lock();
        let pages = self.doc.pages().len() as u32;
        let mut budget = MAX_ENTRIES;
        walk(self.doc.bookmarks().root(), pages, 0, &mut budget)
    }
}

/// The bookmark and its siblings, with their children.
fn walk(first: Option<PdfBookmark<'_>>, pages: u32, depth: usize, budget: &mut usize) -> Vec<OutlineEntry> {
    let mut out = Vec::new();
    let mut next = first;
    while let Some(b) = next {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        let index = match b.destination() {
            Some(d) => d.page_index().ok(),
            None => b.action().and_then(|a| a.as_local_destination_action().and_then(|l| l.destination().ok()).and_then(|d| d.page_index().ok())),
        };
        let page = index.and_then(|i| u32::try_from(i).ok()).filter(|&i| i < pages);
        let title: String = b.title().unwrap_or_default().chars().filter(|c| !c.is_control()).take(200).collect();
        let children = if depth + 1 < MAX_DEPTH { walk(b.first_child(), pages, depth + 1, budget) } else { Vec::new() };
        out.push(OutlineEntry { title: title.trim().to_string(), page, children });
        next = b.next_sibling();
    }
    out
}
