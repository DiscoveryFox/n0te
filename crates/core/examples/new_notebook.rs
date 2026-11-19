//! Dev tool: a new notebook with one lined A4 page and a stroke on it; prints its id.
//! `cargo run -p n0te-core --example new_notebook -- <library> <title>`
use n0te_core::notebook::{Background, SheetSpec};
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let lib = n0te_core::store::Library::open_as(&a[0], "tool").expect("open");
    let nb = lib.create_notebook(&a[1]).expect("notebook");
    let (_, sheet_id) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Lines)).expect("sheet");
    lib.save_notebook(&nb).expect("save");
    let sheet = lib.load_sheet(&nb.id(), &sheet_id).expect("load");
    let pts: Vec<_> = (0..40).map(|i| n0te_core::ink::InkPoint { x: 80.0 + i as f32 * 6.0, y: 120.0 + (i as f32 * 0.4).sin() * 15.0, pressure: 0.6, t_ms: i * 10 }).collect();
    sheet.add_stroke(n0te_core::ink::Brush { width: 2.0, thinning: 0.5, color: 0xFF111114, kind: 0 }, &pts).expect("stroke");
    lib.save_sheet(&nb.id(), &sheet_id, &sheet).expect("save sheet");
    println!("{}", nb.id());
}
