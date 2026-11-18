//! Notebook-level PDF operations shared by the apps (Android through n0te-ffi, the laptop app
//! directly): import a PDF as sheets, and turn a notebook's sheets into export pages.

use std::collections::{HashMap, HashSet};

use n0te_core::ink::{self, Brush, InkPoint, Rect};
use n0te_core::notebook::{Background, Notebook, OutlineRow, SheetSpec};
use n0te_core::sheet;
use n0te_core::store::{Library, blob_id};

use crate::worker::{ExportSheet, PdfWorker, SheetSource, priority};
use crate::{Error, PageSize, Result};

/// Margin around the strokes of a board (infinite sheet) when it is exported as a page.
pub const BOARD_MARGIN: f32 = 24.0;
/// Largest exported page side in points (PDF's limit); bigger boards are scaled down.
const MAX_PAGE: f32 = 14_400.0;

/// A sheet created by [`import_pdf`].
#[derive(Debug, Clone, PartialEq)]
pub struct ImportedSheet {
    pub node: String,
    pub sheet_id: String,
    /// Page index in the PDF (0-based).
    pub page: u32,
    pub size: PageSize,
}

/// Imports a PDF: checks that pdfium can open it, stores the bytes as a blob and adds one sheet
/// per page, `SheetSpec { width, height: page size, pdf: Some((blob, page)) }`, starting at
/// `index` under `parent` (`None` = main column, else a group node). The document stays open on
/// the worker under its blob id, so the first tiles need no reload. The notebook is not saved.
pub fn import_pdf(lib: &Library, nb: &Notebook, worker: &PdfWorker, bytes: Vec<u8>, parent: Option<&str>, index: usize) -> Result<Vec<ImportedSheet>> {
    let blob = blob_id(&bytes);
    let (key, copy) = (blob.clone(), bytes.clone());
    let sizes = worker.call(priority::INTERACTIVE, move |docs| docs.insert(&key, copy)?.page_sizes())??;
    if sizes.is_empty() {
        return Err(Error::Invalid("the PDF has no pages".into()));
    }
    lib.put_blob(&bytes)?;
    let mut out = Vec::with_capacity(sizes.len());
    for (i, size) in sizes.into_iter().enumerate() {
        let spec = SheetSpec { width: size.width, height: size.height, background: Background::Blank, spacing: 20.0, pdf: Some((blob.clone(), i as u32)) };
        let (node, sheet_id) = nb.add_sheet(parent, index.saturating_add(i), &spec)?;
        out.push(ImportedSheet { node, sheet_id, page: i as u32, size });
    }
    Ok(out)
}

/// The pages for exporting `nb`: every sheet in reading order (groups expanded; per main sheet
/// its left side sheets outermost first, the sheet, its right side sheets), or only the
/// `selected` sheet ids, still in reading order (an id not in the notebook is an error).
///
/// PDF sheets become their PDF page, fixed-size sheets blank pages of their size, boards a page
/// around their strokes ([`BOARD_MARGIN`]); empty boards are left out. `strokes` returns a
/// sheet's strokes (apps prefer an open, possibly unsaved sheet over the stored one).
pub fn export_sheets(nb: &Notebook, selected: Option<&[String]>, mut strokes: impl FnMut(&str) -> Result<Vec<sheet::Stroke>>) -> Result<Vec<ExportSheet>> {
    let mut specs: HashMap<String, SheetSpec> = HashMap::new();
    for row in nb.outline(true) {
        if let OutlineRow::Sheet { sheet_id, spec, left, right, .. } = row {
            specs.insert(sheet_id, spec);
            for s in left.into_iter().chain(right) {
                specs.insert(s.sheet_id, s.spec);
            }
        }
    }
    let order = nb.sheet_order();
    let ids: Vec<String> = match selected {
        None => order,
        Some(sel) => {
            if let Some(missing) = sel.iter().find(|s| !specs.contains_key(s.as_str())) {
                return Err(Error::Invalid(format!("no sheet {missing} in this notebook")));
            }
            let want: HashSet<&str> = sel.iter().map(String::as_str).collect();
            order.into_iter().filter(|id| want.contains(id.as_str())).collect()
        }
    };
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let spec = &specs[&id];
        let mut ink: Vec<(Vec<InkPoint>, Brush)> = strokes(&id)?.into_iter().map(|s| (s.points, s.brush)).collect();
        let source = match &spec.pdf {
            Some((blob, page)) => SheetSource::Pdf { doc: blob.clone(), page: *page },
            None if !spec.is_board() => SheetSource::Blank { width: spec.width, height: spec.height },
            None => match board_page(&mut ink) {
                Some((width, height)) => SheetSource::Blank { width, height },
                None => continue,
            },
        };
        out.push(ExportSheet { source, strokes: ink });
    }
    Ok(out)
}

/// Moves a board's strokes onto a page around them and returns the page size (`None` if the
/// board is empty).
fn board_page(strokes: &mut [(Vec<InkPoint>, Brush)]) -> Option<(f32, f32)> {
    let b = strokes.iter().map(|(p, br)| ink::bounds(p, br)).filter(|r| !r.is_empty()).fold(Rect::EMPTY, |u, r| u.include(r.x0, r.y0).include(r.x1, r.y1));
    if b.is_empty() {
        return None;
    }
    let (w, h) = (b.x1 - b.x0 + 2.0 * BOARD_MARGIN, b.y1 - b.y0 + 2.0 * BOARD_MARGIN);
    let k = (MAX_PAGE / w.max(h)).min(1.0);
    for (pts, brush) in strokes.iter_mut() {
        for p in pts.iter_mut() {
            p.x = (p.x - b.x0 + BOARD_MARGIN) * k;
            p.y = (p.y - b.y0 + BOARD_MARGIN) * k;
        }
        brush.width *= k;
    }
    Some((w * k, h * k))
}

#[cfg(test)]
mod tests {
    use super::*;
    use n0te_core::notebook::Side;

    fn stroke(x: f32, y: f32) -> sheet::Stroke {
        let points = vec![InkPoint { x, y, pressure: 0.5, t_ms: 0 }, InkPoint { x: x + 10.0, y, pressure: 0.5, t_ms: 8 }];
        let brush = Brush { width: 2.0, thinning: 0.0, color: 0xFF000000 };
        sheet::Stroke { id: String::new(), bounds: ink::bounds(&points, &brush), brush, points }
    }

    #[test]
    fn export_order_selection_and_boards() {
        let nb = Notebook::create(1, "x").unwrap();
        let pdf = SheetSpec { pdf: Some(("b".repeat(64), 3)), ..SheetSpec::a4(Background::Blank) };
        let (main, s_main) = nb.add_sheet(None, 0, &pdf).unwrap();
        let (_, s_right) = nb.add_side_sheet(&main, Side::Right, 0, &SheetSpec::a4(Background::Lines)).unwrap();
        let (_, s_left) = nb.add_side_sheet(&main, Side::Left, 0, &SheetSpec::a4(Background::Grid)).unwrap();
        let g = nb.add_group(None, 1, "G").unwrap();
        let (_, s_board) = nb.add_sheet(Some(&g), 0, &SheetSpec::board()).unwrap();
        let (_, s_empty) = nb.add_sheet(Some(&g), 1, &SheetSpec::board()).unwrap();
        nb.set_collapsed(&g, true).unwrap(); // collapsed groups are exported too

        let ink = |id: &str| -> Result<Vec<sheet::Stroke>> { Ok(if id == s_board { vec![stroke(-500.0, 1000.0), stroke(-400.0, 1200.0)] } else { vec![] }) };
        let all = export_sheets(&nb, None, ink).unwrap();
        assert_eq!(all.len(), 4, "the empty board is left out");
        assert_eq!(all[0].source, SheetSource::Blank { width: 595.0, height: 842.0 }); // left side sheet first
        assert_eq!(all[1].source, SheetSource::Pdf { doc: "b".repeat(64), page: 3 });
        let SheetSource::Blank { width, height } = all[3].source else { panic!() };
        // Board: strokes span x -500..-390, y 1000..1200 (+ brush and margin).
        assert!((width - (110.0 + 4.0 + 2.0 * BOARD_MARGIN)).abs() < 0.01 && (height - (200.0 + 4.0 + 2.0 * BOARD_MARGIN)).abs() < 0.01, "{width}x{height}");
        let p = all[3].strokes[0].0[0];
        assert!((p.x - (BOARD_MARGIN + 2.0)).abs() < 0.01 && (p.y - (BOARD_MARGIN + 2.0)).abs() < 0.01, "{p:?}");

        let some = export_sheets(&nb, Some(&[s_right.clone(), s_left.clone(), s_empty]), ink).unwrap();
        assert_eq!(some.len(), 2);
        assert_eq!(some[0].source, all[0].source);
        assert!(export_sheets(&nb, Some(&["nope".to_string()]), ink).is_err());
        let _ = s_main;
    }
}
