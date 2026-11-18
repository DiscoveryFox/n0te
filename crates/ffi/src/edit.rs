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
    /// All sheet ids in reading order (side sheets next to their main sheet): the export order.
    pub fn sheet_order(&self) -> Vec<String> {
        lock(&self.nb).sheet_order()
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
        let spec = crate::FfiSheetSpec { width: 595.0, height: 842.0, background: crate::FfiBackground::Blank, spacing: 20.0, pdf_blob: None, pdf_page: 0 };
        let ids = nb.add_sheet(None, 0, spec).unwrap();
        let sheet: Arc<SheetHandle> = nb.clone().open_sheet(ids[1].clone()).unwrap();
        let pack = |v: &[f32]| v.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>();
        let s = sheet.add_stroke(FfiBrush { width: 2.0, thinning: 0.5, color: 0xFF000000 }, pack(&[10.0, 10.0, 0.5, 0.0, 20.0, 10.0, 0.5, 5.0])).unwrap();
        assert_eq!(sheet.hit_test(15.0, 10.0, 2.0), vec![s.id.clone()]);
        assert_eq!(sheet.lasso(pack(&[0.0, 0.0, 30.0, 0.0, 30.0, 30.0, 0.0, 30.0])), vec![s.id.clone()]);
        assert!(sheet.undo().unwrap());
        assert_eq!(sheet.stroke_count(), 0);
        assert!(sheet.redo().unwrap());
        assert_eq!(sheet.stroke_count(), 1);
        nb.set_title("Analysis".into()).unwrap();
        assert_eq!(nb.title(), "Analysis");
        assert_eq!(nb.sheet_order(), vec![ids[1].clone()]);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
