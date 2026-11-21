//! Editing calls for the tablet app (kept apart from lib.rs so parallel work merges cleanly).

use crate::{N0teError, NotebookHandle, SheetHandle, lock};

type R<T> = Result<T, N0teError>;

fn floats(bytes: &[u8]) -> Vec<f32> {
    bytes.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().expect("4 bytes"))).collect()
}

#[uniffi::export]
impl SheetHandle {
    /// Undoes this device's last edit on the sheet; true if something changed.
    pub fn undo(&self) -> R<bool> {
        Ok(lock(&self.sheet).undo()?)
    }
    pub fn redo(&self) -> R<bool> {
        Ok(lock(&self.sheet).redo()?)
    }
    pub fn can_undo(&self) -> bool {
        lock(&self.sheet).can_undo()
    }
    pub fn can_redo(&self) -> bool {
        lock(&self.sheet).can_redo()
    }
    /// Stroke ids under the eraser at (x, y) with the radius (sheet units).
    pub fn hit_test(&self, x: f32, y: f32, radius: f32) -> Vec<String> {
        lock(&self.sheet).hit_test(x, y, radius)
    }
    /// A text page's Markdown body.
    pub fn page_text(&self) -> String {
        lock(&self.sheet).page_text()
    }
    pub fn set_page_text(&self, markdown: String) -> R<()> {
        Ok(lock(&self.sheet).set_page_text(&markdown)?)
    }
    /// The sheet's text boxes (typed on the laptop), top to bottom.
    pub fn text_boxes(&self) -> Vec<crate::FfiTextBox> {
        lock(&self.sheet)
            .text_boxes()
            .into_iter()
            .map(|t| crate::FfiTextBox { id: t.id, x: t.x, y: t.y, w: t.w, size: t.size, text: t.text })
            .collect()
    }
    /// Text boxes plus a text page's body as a box in the margins of a `page_width` page.
    pub fn text_boxes_with_body(&self, page_width: f32) -> Vec<crate::FfiTextBox> {
        lock(&self.sheet)
            .text_boxes_with_body(page_width)
            .into_iter()
            .map(|t| crate::FfiTextBox { id: t.id, x: t.x, y: t.y, w: t.w, size: t.size, text: t.text })
            .collect()
    }
    /// Stretches strokes and images about (ax, ay) by kx across and ky down (pen width kept).
    pub fn stretch(&self, ids: Vec<String>, ax: f32, ay: f32, kx: f32, ky: f32) -> R<()> {
        Ok(lock(&self.sheet).stretch(&ids, ax, ay, kx, ky)?)
    }
    /// Top and bottom of what lies below the line at `y` (what [`SheetHandle::shift_below`] moves).
    pub fn below(&self, y: f32) -> crate::FfiBelow {
        let b = lock(&self.sheet).below(y);
        crate::FfiBelow { top: b.top, bottom: b.bottom }
    }
    /// Moves everything below the line at `y` down by `dy` (up if negative), one undo step;
    /// returns how many objects moved.
    pub fn shift_below(&self, y: f32, dy: f32) -> R<u32> {
        Ok(lock(&self.sheet).shift_below(y, dy)? as u32)
    }
    /// Places a stored blob as an image; returns its object id.
    pub fn add_image(&self, blob: String, x: f32, y: f32, w: f32, h: f32) -> R<String> {
        Ok(lock(&self.sheet).add_image(&blob, x, y, w, h)?)
    }
    pub fn images(&self) -> Vec<crate::FfiImage> {
        lock(&self.sheet).images().into_iter().map(|i| crate::FfiImage { id: i.id, blob: i.blob, x: i.x, y: i.y, w: i.w, h: i.h }).collect()
    }
    pub fn place_image(&self, id: String, x: f32, y: f32, w: f32, h: f32) -> R<()> {
        Ok(lock(&self.sheet).place_image(&id, x, y, w, h)?)
    }

    pub fn add_text_box(&self, x: f32, y: f32, w: f32, size: f32, text: String) -> R<String> {
        Ok(lock(&self.sheet).add_text_box(x, y, w, size, &text)?)
    }
    pub fn set_text(&self, id: String, text: String) -> R<()> {
        Ok(lock(&self.sheet).set_text(&id, &text)?)
    }
    pub fn place_text_box(&self, id: String, x: f32, y: f32, w: f32, size: f32) -> R<()> {
        Ok(lock(&self.sheet).place_text_box(&id, x, y, w, size)?)
    }
    /// Strokes the path (packed LE f32 x,y pairs) passes within `radius` of.
    pub fn hit_path(&self, path: Vec<u8>, radius: f32) -> Vec<String> {
        lock(&self.sheet).hit_path(&pairs(&path), radius)
    }
    /// Area eraser along the path (packed LE f32 x, y, radius triples): one undo step.
    /// Returns how many strokes changed.
    pub fn erase_area(&self, path: Vec<u8>) -> R<u32> {
        let t: Vec<(f32, f32, f32)> = floats(&path).chunks_exact(3).map(|c| (c[0], c[1], c[2])).collect();
        Ok(lock(&self.sheet).erase_area(&t)? as u32)
    }
    /// The strokes' brush and samples (copy, cut, moving to another sheet).
    pub fn stroke_data(&self, ids: Vec<String>) -> R<Vec<crate::FfiStrokeData>> {
        let s = lock(&self.sheet);
        let mut out = Vec::new();
        for id in ids {
            if let Some(st) = s.stroke(&id)? {
                out.push(crate::FfiStrokeData { brush: st.brush.into(), samples: crate::pack_points(&st.points) });
            }
        }
        Ok(out)
    }
    /// Adds strokes, scaled by `scale` about (ox, oy) and moved by (dx, dy), as one undo step;
    /// returns them as stored.
    pub fn add_strokes(&self, items: Vec<crate::FfiStrokeData>, dx: f32, dy: f32, scale: f32, ox: f32, oy: f32) -> R<Vec<crate::FfiStroke>> {
        let k = if scale.is_finite() && scale > 0.0 { scale } else { 1.0 };
        let s = lock(&self.sheet);
        let data: Vec<(n0te_core::ink::Brush, Vec<n0te_core::ink::InkPoint>)> = items
            .into_iter()
            .map(|it| {
                let mut pts = crate::points_of(&it.samples);
                for p in &mut pts {
                    p.x = ox + (p.x - ox) * k + dx;
                    p.y = oy + (p.y - oy) * k + dy;
                }
                let mut b: n0te_core::ink::Brush = it.brush.into();
                b.width *= k;
                (b, pts)
            })
            .collect();
        let ids = s.add_strokes(&data)?;
        let mut out = Vec::new();
        for id in ids {
            if let Some(st) = s.stroke(&id)? {
                out.push(crate::to_ffi_stroke(st));
            }
        }
        Ok(out)
    }
    /// Stroke ids inside the lasso polygon (packed LE f32 x,y pairs).
    pub fn lasso(&self, polygon: Vec<u8>) -> Vec<String> {
        lock(&self.sheet).lasso(&floats(&polygon))
    }
    pub fn stroke_count(&self) -> u32 {
        lock(&self.sheet).len() as u32
    }
}

#[uniffi::export]
impl NotebookHandle {
    pub fn set_title(&self, title: String) -> R<()> {
        Ok(lock(&self.nb).set_title(&title)?)
    }
    pub fn set_node_title(&self, node: String, title: String) -> R<()> {
        Ok(lock(&self.nb).set_title_of(&node, &title)?)
    }
    /// Changes the paper of a sheet (main or side sheet).
    pub fn set_paper(&self, node: String, background: crate::FfiBackground, spacing: f32) -> R<()> {
        Ok(lock(&self.nb).set_paper(&node, background.into(), spacing)?)
    }
    /// What would stick out if the sheets of `nodes` had `width` × `height` (before turning or
    /// shrinking them): counts over all of them, and on how many pages. Open sheets count with
    /// their unsaved strokes.
    pub fn outside(&self, nodes: Vec<String>, width: f32, height: f32) -> R<crate::FfiOutside> {
        let ids: Vec<String> = {
            let nb = lock(&self.nb);
            let mut by_node = std::collections::HashMap::new();
            for row in nb.outline(true) {
                if let n0te_core::notebook::OutlineRow::Sheet { node, sheet_id, left, right, .. } = row {
                    by_node.insert(node, sheet_id);
                    for s in left.into_iter().chain(right) {
                        by_node.insert(s.node, s.sheet_id);
                    }
                }
            }
            nodes.iter().filter_map(|n| by_node.get(n).cloned()).collect()
        };
        let mut total = n0te_core::sheet::Outside::default();
        let mut pages = 0;
        for id in ids {
            let o = match self.open_sheet_handle(&id) {
                Some(h) => lock(&h.sheet).outside(width, height),
                None => self.lib.inner.load_sheet(&self.id(), &id)?.outside(width, height),
            };
            if !o.is_empty() {
                pages += 1;
                total.add(o);
            }
        }
        Ok(crate::FfiOutside { strokes: total.strokes as u32, images: total.images as u32, texts: total.texts as u32, pages, text: total.describe() })
    }

    /// Changes a sheet's size in page points (landscape, another format, a size of its own).
    /// Content keeps its place; PDF pages and boards refuse.
    pub fn set_size(&self, node: String, width: f32, height: f32) -> R<()> {
        Ok(lock(&self.nb).set_size(&node, width, height)?)
    }
    /// A sheet's height and gaps after space went in or out (also to undo that): `gaps` pairs.
    pub fn reshape(&self, node: String, height: f32, gaps: Vec<f32>) -> R<()> {
        let g: Vec<(f32, f32)> = gaps.chunks_exact(2).map(|c| (c[0], c[1])).collect();
        Ok(lock(&self.nb).reshape(&node, height, &g)?)
    }
    pub fn folder(&self) -> String {
        lock(&self.nb).folder()
    }
    /// Library cover: "" / "page" (first page) or a preset name.
    pub fn cover(&self) -> String {
        lock(&self.nb).cover()
    }
    pub fn set_cover(&self, cover: String) -> R<()> {
        Ok(lock(&self.nb).set_cover(&cover)?)
    }
    pub fn set_folder(&self, folder: String) -> R<()> {
        Ok(lock(&self.nb).set_folder(&folder)?)
    }
    pub fn sheet_count(&self) -> u32 {
        lock(&self.nb).sheet_count() as u32
    }
    /// All sheet ids in reading order (side sheets next to their main sheet): the export order.
    pub fn sheet_order(&self) -> Vec<String> {
        lock(&self.nb).sheet_order()
    }
}

fn pairs(b: &[u8]) -> Vec<(f32, f32)> {
    floats(b).chunks_exact(2).map(|c| (c[0], c[1])).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{FfiBrush, NoteLibrary};
    use std::sync::Arc;

    #[test]
    fn undo_and_lasso_through_ffi() {
        let dir = std::env::temp_dir().join(format!("n0te-ffi-edit-{}", n0te_core::ids::new_id(5)));
        let lib = NoteLibrary::open(dir.to_string_lossy().into_owned()).unwrap();
        let nb = lib.create_notebook("x".into()).unwrap();
        let spec = crate::FfiSheetSpec { width: 595.0, height: 842.0, background: crate::FfiBackground::Blank, spacing: 20.0, pdf_blob: None, pdf_page: 0, gaps: vec![] };
        let ids = nb.add_sheet(None, 0, spec).unwrap();
        let sheet: Arc<SheetHandle> = nb.clone().open_sheet(ids[1].clone()).unwrap();
        let pack = |v: &[f32]| v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>();
        let s = sheet.add_stroke(FfiBrush { width: 2.0, thinning: 0.5, color: 0xFF000000, kind: 0 }, pack(&[10.0, 10.0, 0.5, 0.0, 20.0, 10.0, 0.5, 5.0])).unwrap();
        assert_eq!(sheet.hit_test(15.0, 10.0, 2.0), vec![s.id.clone()]);
        assert_eq!(sheet.lasso(pack(&[0.0, 0.0, 30.0, 0.0, 30.0, 30.0, 0.0, 30.0])), vec![s.id.clone()]);
        assert!(sheet.undo().unwrap());
        assert_eq!(sheet.stroke_count(), 0);
        assert!(sheet.redo().unwrap());
        assert_eq!(sheet.stroke_count(), 1);
        nb.set_title("Analysis".into()).unwrap();
        assert_eq!(nb.title(), "Analysis");
        assert_eq!(nb.outside(vec![ids[0].clone()], 842.0, 595.0).unwrap().pages, 0, "the stroke at the top fits");
        assert_eq!(nb.outside(vec![ids[0].clone()], 15.0, 15.0).unwrap().text, "1 Strich");
        nb.set_size(ids[0].clone(), 842.0, 595.0).unwrap();
        assert!(nb.set_size(ids[0].clone(), 1.0, 595.0).is_err());
        assert_eq!(nb.sheet_order(), vec![ids[1].clone()]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod kind_tests {
    use crate::{FfiBrush, FfiBackground, FfiSheetSpec, NoteLibrary};

    #[test]
    fn pencil_kind_survives_the_ffi() {
        let dir = std::env::temp_dir().join(format!("n0te-ffi-kind-{}", n0te_core::ids::new_id(6)));
        let lib = NoteLibrary::open(dir.to_string_lossy().into_owned()).unwrap();
        let nb = lib.create_notebook("k".into()).unwrap();
        let ids = nb.add_sheet(None, 0, FfiSheetSpec { width: 595.0, height: 842.0, background: FfiBackground::Blank, spacing: 20.0, pdf_blob: None, pdf_page: 0, gaps: vec![] }).unwrap();
        let sheet = nb.open_sheet(ids[1].clone()).unwrap();
        let pack = |v: &[f32]| v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>();
        let s = sheet.add_stroke(FfiBrush { width: 2.0, thinning: 0.3, color: 0xC8333333, kind: 1 }, pack(&[1.0, 1.0, 0.5, 0.0, 9.0, 9.0, 0.5, 4.0])).unwrap();
        assert_eq!(s.brush.kind, 1);
        assert_eq!(sheet.all_strokes()[0].brush.kind, 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
