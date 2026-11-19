//! Headless screenshots of the n0te laptop app (never opens a window):
//!
//!   cargo run --profile fast -p n0te-desktop --example headless -- [out-dir]
//!
//! Builds the demo notebook in a temporary library and saves PNGs of the library, the
//! notebook at 100 % and zoomed, and with a group folded. `N0TE_BACKEND=tiny-skia|wgpu`.

#[path = "../tests/harness/mod.rs"]
mod harness;

use std::path::PathBuf;

use harness::Harness;
use iced::Size;
use n0te_core::store::Library;
use n0te_desktop::app::{Message, Screen};
use n0te_desktop::{App, demo};

fn main() {
    let out = std::env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../spikes/desktop-shots"));
    std::fs::create_dir_all(&out).unwrap();
    let root = std::env::temp_dir().join(format!("n0te-shots-{}", std::process::id()));
    let lib = Library::open(&root).unwrap();
    let id = demo::save_into(&lib);
    let second = lib.create_notebook("Lineare Algebra").unwrap();
    second.set_cover("initial").unwrap();
    second.set_folder("WS26").unwrap();
    lib.save_notebook(&second).unwrap();
    let backend = std::env::var("N0TE_BACKEND").unwrap_or_else(|_| "wgpu".into());
    let mut h = Harness::new(App::new(&root), &backend, Size::new(1600.0, 900.0), 1.2)
        .or_else(|| Harness::new(App::new(&root), "tiny-skia", Size::new(1600.0, 900.0), 1.2))
        .expect("renderer");
    println!("backend: {}", h.backend());
    h.screenshot(out.join("m2-01-library.png"));
    h.message(Message::Open(id));
    h.events(&[]);
    h.events(&[]);
    h.screenshot(out.join("m2-02-notebook.png"));
    h.message(Message::Zoom(2.2));
    h.events(&[]);
    h.screenshot(out.join("m2-03-zoom.png"));
    h.message(Message::Zoom(0.3));
    h.events(&[]);
    h.screenshot(out.join("m2-04-overview.png"));
    if let Screen::Notebook(o) = &h.app.screen
        && let Some(g) = o.pages.headers.first()
    {
        let node = g.node.clone();
        h.message(Message::Fold(node));
        h.events(&[]);
        h.screenshot(out.join("m2-05-folded.png"));
    }
    println!("screenshots in {}", out.display());
    let _ = std::fs::remove_dir_all(root);
}
