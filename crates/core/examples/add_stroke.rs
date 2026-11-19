//! Dev tool: adds one stroke to a sheet of a library (as another process would).
//! `cargo run -p n0te-core --example add_stroke -- <library> <notebook> <sheet>`
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let lib = n0te_core::store::Library::open_as(&a[0], "tool").expect("open");
    let sheet = lib.load_sheet(&a[1], &a[2]).expect("sheet");
    let pts: Vec<_> = (0..30).map(|i| n0te_core::ink::InkPoint { x: 100.0 + i as f32 * 4.0, y: 300.0 + (i as f32 * 0.3).sin() * 20.0, pressure: 0.6, t_ms: i * 10 }).collect();
    let id = sheet.add_stroke(n0te_core::ink::Brush { width: 2.0, thinning: 0.5, color: 0xFFD71921, kind: 0 }, &pts).expect("stroke");
    lib.save_sheet(&a[1], &a[2], &sheet).expect("save");
    println!("{id}");
}
