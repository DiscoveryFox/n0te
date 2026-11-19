//! Lists a library's notebooks: title, pages, strokes, text boxes, PDFs (read only).
//! `cargo run -p n0te-core --example inspect -- <library dir>`
fn main() {
    let root = std::env::args().nth(1).expect("library dir");
    let lib = n0te_core::store::Library::open(&root).expect("open");
    for id in lib.notebook_ids().expect("ids") {
        let nb = lib.load_notebook(&id).expect("notebook");
        let sheets = lib.sheet_ids_on_disk(&id).unwrap_or_default();
        let (mut strokes, mut texts) = (0, 0);
        for s in &sheets {
            if let Ok(sh) = lib.load_sheet(&id, s) {
                strokes += sh.strokes().len();
                texts += sh.text_boxes().len();
            }
        }
        let rows = nb.outline(true).len();
        println!("{id}  {:?}  outline rows {rows}, sheets {}, strokes {strokes}, text boxes {texts}, pdfs {}", nb.title(), sheets.len(), nb.pdf_blobs().len());
    }
}
