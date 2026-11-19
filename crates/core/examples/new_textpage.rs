//! Dev tool: a new notebook with one text page holding the given Markdown; prints its id.
//! `cargo run -p n0te-core --example new_textpage -- <library> <title> <markdown>`
use n0te_core::notebook::{Background, SheetSpec};
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let lib = n0te_core::store::Library::open_as(&a[0], "tool").expect("open");
    let nb = lib.create_notebook(&a[1]).expect("notebook");
    let (_, sheet_id) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Text)).expect("sheet");
    lib.save_notebook(&nb).expect("save");
    let sheet = lib.load_sheet(&nb.id(), &sheet_id).expect("load");
    sheet.set_page_text(&a[2]).expect("text");
    lib.save_sheet(&nb.id(), &sheet_id, &sheet).expect("save sheet");
    println!("{}", nb.id());
}
