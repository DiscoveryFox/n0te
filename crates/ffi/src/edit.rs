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
    /// Every stroke's id, box and kind (no points: cheap).
    pub fn stroke_boxes(&self) -> Vec<crate::FfiStrokeBox> {
        lock(&self.sheet).stroke_boxes().into_iter().map(|(id, r, kind)| crate::FfiStrokeBox { id, x0: r.x0, y0: r.y0, x1: r.x1, y1: r.y1, kind }).collect()
    }
    /// The lines of handwriting as read, where they are now.
    pub fn index_lines(&self) -> Vec<crate::FfiIndexLine> {
        lock(&self.sheet).index_lines().into_iter().map(Into::into).collect()
    }
    /// The keys of all stored lines (also those whose strokes are gone).
    pub fn index_keys(&self) -> Vec<String> {
        lock(&self.sheet).index_keys()
    }
    /// Lines read (`put`, by key) and lines gone (`drop`); undo leaves this alone.
    pub fn set_index(&self, put: Vec<crate::FfiIndexLine>, drop: Vec<String>) -> R<()> {
        let put: Vec<n0te_core::sheet::IndexLine> = put.into_iter().map(Into::into).collect();
        Ok(lock(&self.sheet).set_index(&put, &drop)?)
    }
    /// Marks strokes as a heading (one undo step); returns its id.
    pub fn add_heading(&self, text: String, strokes: Vec<String>) -> R<String> {
        Ok(lock(&self.sheet).add_heading(&text, &strokes)?)
    }
    pub fn set_heading_text(&self, id: String, text: String) -> R<()> {
        Ok(lock(&self.sheet).set_heading_text(&id, &text)?)
    }
    pub fn remove_heading(&self, id: String) -> R<()> {
        Ok(lock(&self.sheet).remove_heading(&id)?)
    }
    /// The headings, top to bottom, where their strokes are now.
    pub fn headings(&self) -> Vec<crate::FfiHeading> {
        lock(&self.sheet)
            .headings()
            .into_iter()
            .map(|h| crate::FfiHeading { id: h.id, text: h.text, strokes: h.strokes, x0: h.x0, y0: h.y0, x1: h.x1, y1: h.y1 })
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
    /// Left and right edge of what lies right of the line at `x` (what [`SheetHandle::shift_right_of`] moves).
    pub fn right_of(&self, x: f32) -> crate::FfiBelow {
        let b = lock(&self.sheet).beyond(n0te_core::notebook::Axis::Right, x);
        crate::FfiBelow { top: b.top, bottom: b.bottom }
    }
    /// Moves everything right of the line at `x` right by `dx` (left if negative), one undo
    /// step; returns how many objects moved.
    pub fn shift_right_of(&self, x: f32, dx: f32) -> R<u32> {
        Ok(lock(&self.sheet).shift_beyond(n0te_core::notebook::Axis::Right, x, dx)? as u32)
    }
    /// Places a stored blob as an image; returns its object id.
    pub fn add_image(&self, blob: String, x: f32, y: f32, w: f32, h: f32) -> R<String> {
        Ok(lock(&self.sheet).add_image(&blob, x, y, w, h)?)
    }
    /// The page's images and then its plots (as images without a blob, see `FfiImage::plot`).
    pub fn images(&self) -> Vec<crate::FfiImage> {
        let s = lock(&self.sheet);
        let mut out: Vec<crate::FfiImage> = s.images().into_iter().map(|i| crate::FfiImage { id: i.id, blob: i.blob, x: i.x, y: i.y, w: i.w, h: i.h, plot: None }).collect();
        out.extend(s.plots().into_iter().map(|p| crate::FfiImage { id: p.id.clone(), blob: String::new(), x: p.x, y: p.y, w: p.w, h: p.h, plot: Some(p.into()) }));
        out
    }
    /// Puts a coordinate system on the page; returns its id.
    pub fn add_plot(&self, plot: crate::FfiPlot) -> R<String> {
        Ok(lock(&self.sheet).add_plot(&plot.into())?)
    }
    /// Changes a plot (by its id) in one step.
    pub fn set_plot(&self, plot: crate::FfiPlot) -> R<()> {
        Ok(lock(&self.sheet).set_plot(&plot.into())?)
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
        let mut stored = Vec::with_capacity(ids.len());
        for id in ids {
            if let Some(st) = s.stroke(&id)? {
                stored.push(st);
            }
        }
        // The outlines without holding the sheet.
        drop(s);
        Ok(crate::to_ffi_all(stored))
    }
    /// The strokes `ids` as stored now, with their outlines (after a move, a turn, a new
    /// colour: only those are drawn anew); ids of no stroke are left out.
    pub fn strokes_by_id(&self, ids: Vec<String>) -> R<Vec<crate::FfiStroke>> {
        let s = lock(&self.sheet);
        let mut stored = Vec::with_capacity(ids.len());
        for id in &ids {
            if let Some(st) = s.stroke(id)? {
                stored.push(st);
            }
        }
        drop(s);
        Ok(crate::to_ffi_all(stored))
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

    /// The pages marked to find again (bookmarks), as outline nodes in outline order.
    pub fn marked(&self) -> Vec<String> {
        lock(&self.nb).marked()
    }

    /// Marks a page (outline node) to find again, or not. Save the notebook after.
    pub fn set_marked(&self, node: String, on: bool) -> R<()> {
        Ok(lock(&self.nb).set_marked(&node, on)?)
    }

    /// The notebook's tags, in their order.
    pub fn tags(&self) -> Vec<String> {
        lock(&self.nb).tags()
    }

    /// Sets the tags (trimmed, repeats left out). Save the notebook after.
    pub fn set_tags(&self, tags: Vec<String>) -> R<()> {
        Ok(lock(&self.nb).set_tags(&tags)?)
    }

    /// Pinned: first in the library.
    pub fn pinned(&self) -> bool {
        lock(&self.nb).pinned()
    }

    pub fn set_pinned(&self, pinned: bool) -> R<()> {
        Ok(lock(&self.nb).set_pinned(pinned)?)
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

// --- scribbling out (core scribble: the same rules on every device) ----------------------------

/// Something on the page a scribble may cover, as the tablet draws it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiScribbleTarget {
    pub id: String,
    /// A stroke's outline (LE f32 x, y pairs of the filled polygon), or with `boxed` the box
    /// of a picture or graph (x0, y0, x1, y1).
    pub shape: Vec<u8>,
    pub boxed: bool,
}

/// What a scribble deletes: whole objects, and longer strokes it cuts a piece out of along
/// `path` (LE f32 x, y, radius triples). Both empty: the gesture is ink.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiScribble {
    pub delete: Vec<String>,
    pub cut: Vec<String>,
    pub path: Vec<u8>,
}

/// Whether a stroke (packed samples) moves like a scribble at all: cheap, before gathering
/// what it might cover.
#[uniffi::export]
pub fn scribble_moves(samples: Vec<u8>) -> bool {
    n0te_core::scribble::motion(&crate::points_of(&samples)).is_some()
}

/// What a stroke (packed samples, drawn `width` wide) scribbles out of `targets`.
#[uniffi::export]
pub fn scribble_judge(samples: Vec<u8>, width: f32, targets: Vec<FfiScribbleTarget>) -> FfiScribble {
    use n0te_core::scribble::{Target, judge};
    let targets: Vec<Target> = targets
        .into_iter()
        .map(|t| {
            let f = floats(&t.shape);
            if t.boxed && f.len() >= 4 {
                Target::boxed(t.id, n0te_core::ink::Rect::new(f[0], f[1], f[2], f[3]))
            } else {
                Target::outline(t.id, &f)
            }
        })
        .collect();
    let v = judge(&crate::points_of(&samples), width, &targets);
    let path = v.path.iter().flat_map(|&(x, y, r)| [x, y, r]).flat_map(f32::to_le_bytes).collect();
    FfiScribble { delete: v.delete, cut: v.cut, path }
}

#[uniffi::export]
impl SheetHandle {
    /// Applies a scribble's verdict: deletes and cuts as one undo step. How many objects changed.
    pub fn scribble_out(&self, delete: Vec<String>, cut: Vec<String>, path: Vec<u8>) -> R<u32> {
        let path: Vec<(f32, f32, f32)> = floats(&path).chunks_exact(3).map(|c| (c[0], c[1], c[2])).collect();
        let v = n0te_core::scribble::Verdict { delete, cut, path };
        Ok(lock(&self.sheet).scribble_out(&v)? as u32)
    }
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

    #[test]
    fn scribbling_out_through_ffi_with_outlines() {
        let dir = std::env::temp_dir().join(format!("n0te-ffi-scribble-{}", n0te_core::ids::new_id(5)));
        let lib = NoteLibrary::open(dir.to_string_lossy().into_owned()).unwrap();
        let nb = lib.create_notebook("x".into()).unwrap();
        let spec = crate::FfiSheetSpec { width: 595.0, height: 842.0, background: crate::FfiBackground::Blank, spacing: 20.0, pdf_blob: None, pdf_page: 0, gaps: vec![] };
        let ids = nb.add_sheet(None, 0, spec).unwrap();
        let sheet: Arc<SheetHandle> = nb.clone().open_sheet(ids[1].clone()).unwrap();
        let pack = |v: &[f32]| v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>();
        let brush = FfiBrush { width: 1.5, thinning: 0.5, color: 0xFF000000, kind: 0 };
        // A letter-sized stroke, and a quick zigzag over it (x, y, pressure, t).
        let letter = sheet.add_stroke(brush, pack(&[50.0, 45.0, 0.5, 0.0, 50.0, 50.0, 0.5, 20.0, 51.0, 55.0, 0.5, 40.0])).unwrap();
        let mut zig = Vec::new();
        for i in 0..12 {
            zig.extend_from_slice(&[if i % 2 == 0 { 43.0 } else { 57.0 }, 43.0 + i as f32 * 1.3, 0.5, i as f32 * 30.0]);
        }
        let zig = pack(&zig);
        assert!(scribble_moves(zig.clone()));
        let target = FfiScribbleTarget { id: letter.id.clone(), shape: letter.outline.clone(), boxed: false };
        let picture = FfiScribbleTarget { id: "pic".into(), shape: pack(&[200.0, 0.0, 240.0, 30.0]), boxed: true };
        let v = scribble_judge(zig.clone(), 1.5, vec![target, picture]);
        assert_eq!((v.delete.clone(), v.cut.len()), (vec![letter.id.clone()], 0));
        assert_eq!(sheet.scribble_out(v.delete, v.cut, v.path).unwrap(), 1);
        assert_eq!(sheet.stroke_count(), 0);
        assert!(sheet.undo().unwrap());
        assert_eq!(sheet.stroke_count(), 1);
        // Nothing under it: the same gesture is ink.
        assert!(scribble_judge(zig, 1.5, vec![]).delete.is_empty());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn many_strokes_pasted_come_back_in_order_and_only_the_moved_ones_reload() {
        let dir = std::env::temp_dir().join(format!("n0te-ffi-many-{}", n0te_core::ids::new_id(5)));
        let lib = NoteLibrary::open(dir.to_string_lossy().into_owned()).unwrap();
        let nb = lib.create_notebook("x".into()).unwrap();
        let spec = crate::FfiSheetSpec { width: 595.0, height: 842.0, background: crate::FfiBackground::Blank, spacing: 20.0, pdf_blob: None, pdf_page: 0, gaps: vec![] };
        let ids = nb.add_sheet(None, 0, spec).unwrap();
        let sheet: Arc<SheetHandle> = nb.clone().open_sheet(ids[1].clone()).unwrap();
        let pack = |v: &[f32]| v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>();
        // Enough to be outlined on several threads; each one a little further right.
        let items: Vec<crate::FfiStrokeData> = (0..700)
            .map(|i| {
                let x = i as f32 * 0.5;
                crate::FfiStrokeData { brush: FfiBrush { width: 2.0, thinning: 0.5, color: 0xFF000000, kind: 0 }, samples: pack(&[x, 10.0, 0.5, 0.0, x + 3.0, 20.0, 0.5, 8.0, x + 6.0, 12.0, 0.5, 16.0]) }
            })
            .collect();
        let added = sheet.add_strokes(items, 0.0, 0.0, 1.0, 0.0, 0.0).unwrap();
        assert_eq!(added.len(), 700);
        assert!(added.windows(2).all(|w| w[0].bounds[0] < w[1].bounds[0]), "in the order given");
        assert!(added.iter().all(|s| !s.outline.is_empty() && s.ends.len() == 9));
        // The same outlines as one at a time.
        let one = sheet.strokes_by_id(vec![added[123].id.clone()]).unwrap();
        assert_eq!(one[0].outline, added[123].outline);
        // Moved: only those come back, moved; an unknown id is left out.
        let moved: Vec<String> = added[100..400].iter().map(|s| s.id.clone()).collect();
        sheet.transform(moved.clone(), 50.0, 0.0, 1.0, 0.0, 0.0).unwrap();
        let mut ask = moved.clone();
        ask.push("nope".into());
        let back = sheet.strokes_by_id(ask).unwrap();
        assert_eq!(back.iter().map(|s| s.id.clone()).collect::<Vec<_>>(), moved);
        assert!((back[0].bounds[0] - added[100].bounds[0] - 50.0).abs() < 0.01);
        assert_eq!(sheet.all_strokes().len(), 700);
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
