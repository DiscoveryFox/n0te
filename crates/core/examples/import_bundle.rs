//! Dev tool: imports a `.n0te` bundle (e.g. a history checkpoint) into a library as a copy.
//! `cargo run -p n0te-core --example import_bundle -- <library> <file.n0te>`
fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let lib = n0te_core::store::Library::open_as(&a[0], "tool").expect("open");
    let bytes = std::fs::read(&a[1]).expect("read");
    let id = lib.import_bundle_as_copy(&bytes, "Wiederhergestellt").expect("import");
    println!("{id}");
}
