//! Export: a new PDF made of pages from source PDFs and blank pages, with vector ink on top.
//!
//! Two steps: pdfium assembles the pages ([`PdfEngine::assemble`]: imports pages with their
//! content, resources and annotations, adds blank pages), then n0te's own writer appends the ink
//! ([`crate::add_ink`]). Only the first step holds the pdfium lock.

use std::fmt::Write as _;

use n0te_core::ink::{self, Brush, InkPoint};
use pdfium_render::prelude::{
    PdfColor, PdfDocument, PdfPageContentRegenerationStrategy, PdfPageObjectsCommon, PdfPagePaperSize, PdfPagePathObject, PdfPoints,
};

use crate::{Affine, Error, PdfDoc, PdfEngine, Result, display_to_user, lock};

/// Where an exported page comes from.
#[derive(Clone, Copy)]
pub enum PageSource<'a> {
    /// Page `page` (0-based) of an open PDF, imported with its content, resources and annotations.
    Pdf { doc: &'a PdfDoc, page: u32 },
    /// An empty white page of the given size in points.
    Blank { width: f32, height: f32 },
}

/// One ink stroke in page coordinates (points, top-left origin of the displayed page).
#[derive(Clone, Copy)]
pub struct Stroke<'a> {
    pub points: &'a [InkPoint],
    pub brush: Brush,
}

/// One page of an export and the strokes drawn on top of it.
#[derive(Clone, Copy)]
pub struct ExportPage<'a> {
    pub source: PageSource<'a>,
    pub strokes: &'a [Stroke<'a>],
}

/// Largest blank page accepted, in points (PDF's own limit is 14 400).
const MAX_PAGE: f32 = 14_400.0;

impl PdfEngine {
    /// Builds a new PDF from `pages` and returns its bytes: [`PdfEngine::assemble`] followed by
    /// [`crate::add_ink`].
    ///
    /// Each stroke becomes one filled path (the [`ink::outline`] polygon, non-zero winding) in the
    /// brush colour; alpha below 255 becomes a transparency graphics state. The ink of all pages
    /// is in the optional content group "n0te ink" (see [`crate::ink_writer`]).
    pub fn export(&self, pages: &[ExportPage]) -> Result<Vec<u8>> {
        let sources: Vec<PageSource> = pages.iter().map(|p| p.source).collect();
        let bytes = self.assemble(&sources)?;
        let strokes: Vec<&[Stroke]> = pages.iter().map(|p| p.strokes).collect();
        crate::add_ink(bytes, &strokes)
    }

    /// The pdfium half of an export: a new PDF with `pages` in order and no ink.
    ///
    /// Pages from the same source document are imported in one pdfium call, so resources they
    /// share (fonts, images) are copied once and not once per page. Cost is roughly linear in the
    /// size of what is copied (a few ms for 50 slides).
    pub fn assemble(&self, pages: &[PageSource]) -> Result<Vec<u8>> {
        let _g = lock();
        Ok(self.assemble_locked(pages)?.save_to_bytes()?)
    }

    fn assemble_locked(&self, pages: &[PageSource]) -> Result<PdfDocument<'static>> {
        let pdfium = &self.e.pdfium;

        // 1. Staging document: one import per source document, then the blank pages.
        let mut staging = pdfium.create_new_pdf()?;
        let mut staged_at = vec![0u32; pages.len()];
        let mut sources: Vec<&PdfDoc> = Vec::new();
        for p in pages {
            if let PageSource::Pdf { doc, page } = *p {
                doc.check_page(page)?;
                if !sources.iter().any(|d| std::ptr::eq(*d, doc)) {
                    sources.push(doc);
                }
            }
        }
        let mut next = 0u32;
        for src in &sources {
            let mut range = String::new();
            for (i, p) in pages.iter().enumerate() {
                if let PageSource::Pdf { doc, page } = *p
                    && std::ptr::eq(doc, *src)
                {
                    let _ = write!(range, "{}{}", if range.is_empty() { "" } else { "," }, page + 1);
                    staged_at[i] = next;
                    next += 1;
                }
            }
            let at = staging.pages().len();
            staging.pages_mut().copy_pages_from_document(&src.doc, &range, at)?;
        }
        for (i, p) in pages.iter().enumerate() {
            if let PageSource::Blank { width, height } = *p {
                if !(width > 0.0 && height > 0.0 && width <= MAX_PAGE && height <= MAX_PAGE) {
                    return Err(Error::Invalid(format!("blank page of {width}x{height} pt")));
                }
                staging.pages_mut().create_page_at_end(PdfPagePaperSize::Custom(PdfPoints::new(width), PdfPoints::new(height)))?;
                staged_at[i] = next;
                next += 1;
            }
        }

        // 2. Put the pages in export order (one more import, again sharing resources), unless
        //    staging already has that order.
        if staged_at.iter().enumerate().all(|(i, &s)| s == i as u32) {
            return Ok(staging);
        }
        let mut out = pdfium.create_new_pdf()?;
        let order = staged_at.iter().map(|s| (s + 1).to_string()).collect::<Vec<_>>().join(",");
        out.pages_mut().copy_pages_from_document(&staging, &order, 0)?;
        Ok(out)
    }

    /// The M0 export: ink added as pdfium path objects. Kept only to compare speed and output
    /// with [`PdfEngine::export`] (bench and tests). pdfium's `FPDFPage_GenerateContent` walks
    /// the whole document for pages with an indirect `/Resources`, so this is
    /// O(inked pages × document size).
    #[doc(hidden)]
    pub fn export_via_page_objects(&self, pages: &[ExportPage]) -> Result<Vec<u8>> {
        let _g = lock();
        let sources: Vec<PageSource> = pages.iter().map(|p| p.source).collect();
        let out = self.assemble_locked(&sources)?;
        for (i, p) in pages.iter().enumerate() {
            if !p.strokes.is_empty() {
                draw_strokes(&out, i as u32, p.strokes)?;
            }
        }
        Ok(out.save_to_bytes()?)
    }
}

fn draw_strokes(doc: &PdfDocument<'static>, index: u32, strokes: &[Stroke]) -> Result<()> {
    let mut page = doc.pages().get(index as i32)?;
    // Regenerate the content stream once, not after every added object.
    page.set_content_regeneration_strategy(PdfPageContentRegenerationStrategy::Manual);
    let m = display_to_user(&page)?;
    for s in strokes {
        let poly = ink::outline(s.points, &s.brush);
        if poly.len() < 6 {
            continue;
        }
        let path = polygon_path(doc, &m, &poly, color(s.brush.color))?;
        page.objects_mut().add_path_object(path)?;
    }
    page.regenerate_content()?;
    Ok(())
}

fn polygon_path(doc: &PdfDocument<'static>, m: &Affine, poly: &[f32], fill: PdfColor) -> Result<PdfPagePathObject<'static>> {
    let round = |v: f64| ((v * 100.0).round() / 100.0) as f32;
    let pt = |i: usize| {
        let (x, y) = m.apply(poly[i] as f64, poly[i + 1] as f64);
        (PdfPoints::new(round(x)), PdfPoints::new(round(y)))
    };
    let (x0, y0) = pt(0);
    // Fill only, non-zero winding (pdfium-render's default fill mode).
    let mut path = PdfPagePathObject::new(doc, x0, y0, None, None, Some(fill))?;
    for i in (2..poly.len() - 1).step_by(2) {
        let (x, y) = pt(i);
        path.line_to(x, y)?;
    }
    path.close_path()?;
    Ok(path)
}

/// 0xAARRGGBB -> PdfColor.
fn color(argb: u32) -> PdfColor {
    PdfColor::new((argb >> 16) as u8, (argb >> 8) as u8, argb as u8, (argb >> 24) as u8)
}
