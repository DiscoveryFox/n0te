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
    // A PDF notebook: slides as page backgrounds (tiles arrive from the worker).
    if let Some(pdf) = h.app.pdf.clone() {
        use n0te_pdf::pdfium_render::prelude::*;
        let bytes = pdf
            .worker
            .call(10, |docs| {
                docs.engine().with_raw(|p| {
                    let mut doc = p.create_new_pdf().unwrap();
                    let font = doc.fonts_mut().helvetica_bold();
                    for i in 0..3 {
                        let mut page = doc.pages_mut().create_page_at_end(PdfPagePaperSize::new_custom(PdfPoints::new(842.0), PdfPoints::new(474.0))).unwrap();
                        page.objects_mut().create_text_object(PdfPoints::new(60.0), PdfPoints::new(390.0), &format!("Kapitel {}: Folgen und Reihen", i + 1), font, PdfPoints::new(36.0)).unwrap();
                        page.objects_mut().create_path_object_rect(PdfRect::new_from_values(40.0, 40.0, 120.0, 800.0), None, None, Some(PdfColor::new(215, 25, 33, 255))).unwrap();
                    }
                    doc.save_to_bytes().unwrap()
                })
            })
            .unwrap();
        let file = root.join("Analysis_Folien.pdf");
        std::fs::write(&file, bytes).unwrap();
        h.message(Message::Back);
        h.message(Message::ImportPdf(file));
        for _ in 0..6 {
            h.events(&[]);
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        h.screenshot(out.join("m2-06-pdf.png"));
        // Text boxes on the slide: one rendered, one being edited.
        if let Screen::Notebook(o) = &h.app.screen {
            let sheet = o.pages.pages[0].placed.sheet_id.clone();
            h.message(Message::NewText(sheet.clone(), 470.0, 160.0));
            for (i, line) in ["## Merke", "- Folge **konvergiert**, wenn $|a_n - a| < \\epsilon$", "- Reihe: `Summe der Folgenglieder`"].iter().enumerate() {
                if i > 0 {
                    h.press(iced::keyboard::key::Named::Enter);
                }
                h.type_text(line);
            }
            h.message(Message::DoneText);
            h.message(Message::NewText(sheet, 60.0, 250.0));
            h.type_text("Frage an Prof: Gilt das auch für komplexe Folgen?");
        }
        h.events(&[]);
        // Formulas render in the background (local TeX): give them a moment.
        std::thread::sleep(std::time::Duration::from_millis(2500));
        h.events(&[]);
        h.screenshot(out.join("m2-07-text.png"));
        // Notes docked next to the slide.
        h.message(Message::DoneText);
        h.message(Message::ToggleNotes);
        for (i, line) in ["## Notizen zu Kapitel 1", "Grenzwert eindeutig, falls er existiert.", "- Beweis über Dreiecksungleichung", "- Übung 3 dazu ansehen"].iter().enumerate() {
            if i > 0 {
                h.press(iced::keyboard::key::Named::Enter);
            }
            h.type_text(line);
        }
        h.events(&[]);
        h.screenshot(out.join("m2-08-notes.png"));
        // A full text page.
        h.message(Message::NewTextPage);
        for (i, line) in ["# Mitschrift Vorlesung 8", "Heute: **Mittelwertsatz** und *Anwendungen*.", "```python", "def f(x): return x**2"].iter().enumerate() {
            if i > 0 {
                h.press(iced::keyboard::key::Named::Enter);
            }
            h.type_text(line);
        }
        // Out of the code block (Ctrl+Enter), then lists, a checkbox, formulas and a quote.
        h.message(Message::Blocks(n0te_desktop::blocks::Msg::ExitBlock(h.app.text_page.as_ref().and_then(|t| t.doc.focus).unwrap_or(0))));
        for (i, line) in ["1. Ableitung bilden", "Nullstellen finden", "", "[ ] Übung 4 rechnen", "", "Es gibt ein $\\xi \\in (a,b)$ mit $f'(\\xi) = \\frac{f(b)-f(a)}{b-a}$.", "$$"].iter().enumerate() {
            if i > 0 {
                h.press(iced::keyboard::key::Named::Enter);
            }
            h.type_text(line);
        }
        h.type_text("\\int_a^b f'(x)\\,dx = f(b) - f(a)");
        h.message(Message::Blocks(n0te_desktop::blocks::Msg::ExitBlock(h.app.text_page.as_ref().and_then(|t| t.doc.focus).unwrap_or(0))));
        h.type_text("> Merke: stetig auf [a,b], differenzierbar auf (a,b).");
        h.events(&[]);
        std::thread::sleep(std::time::Duration::from_millis(3000));
        h.events(&[]);
        h.screenshot(out.join("m2-09-textpage.png"));
    }
    // Command palette and the light theme.
    h.message(Message::CloseTextPage);
    h.message(Message::OpenPalette);
    h.type_text("pdf");
    h.events(&[]);
    h.screenshot(out.join("m2-10-palette.png"));
    h.message(Message::PaletteQuery(String::new()));
    h.app.palette = None;
    h.message(Message::SetTheme("light"));
    h.message(Message::Back);
    h.events(&[]);
    h.screenshot(out.join("m2-11-light.png"));
    h.message(Message::SetTheme("nothing"));
    println!("screenshots in {}", out.display());
    let _ = std::fs::remove_dir_all(root);
}
