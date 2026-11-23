//! Headless screenshots of the screens outside the editor in every design (never opens a
//! window): library, settings, export, the page panel, a turned page, the outline with
//! headings, search results and a found spot lighting up, a page with coordinate systems and
//! the dialog for one (on the GPU renderer if there is one: the software one draws no ink).
//!
//!   cargo run --profile fast -p n0te-desktop --example styles -- <out-dir> [theme ids…]

#[path = "../tests/harness/mod.rs"]
mod harness;

use std::path::PathBuf;

use harness::Harness;
use iced::Size;
use n0te_core::store::Library;
use n0te_desktop::app::Message;
use n0te_desktop::{App, demo, theme};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().map(PathBuf::from).expect("out dir");
    let wanted: Vec<String> = args.collect();
    std::fs::create_dir_all(&out).unwrap();
    let root = std::env::temp_dir().join(format!("n0te-styles-{}", std::process::id()));
    let lib = Library::open(&root).unwrap();
    let id = demo::save_into(&lib);
    mark_up(&lib, &id);
    let (plot_sheet, plot_id) = plot_page(&lib, &id);
    for (title, folder, cover) in [("Lineare Algebra", "WS26", "initial"), ("Analysis I", "WS26", "dots"), ("Physik Praktikum", "WS26", "blue"), ("Ideen", "", "stripes")] {
        let nb = lib.create_notebook(title).unwrap();
        nb.set_cover(cover).unwrap();
        nb.set_folder(folder).unwrap();
        lib.save_notebook(&nb).unwrap();
    }
    let mut h = Harness::new(App::new(&root), "tiny-skia", Size::new(1440.0, 900.0), 1.0).expect("renderer");
    let mut gpu = Harness::new(App::new(&root), "wgpu", Size::new(1440.0, 900.0), 1.0);
    for t in theme::PALETTES {
        if !wanted.is_empty() && !wanted.iter().any(|w| w == t.id) {
            continue;
        }
        h.message(Message::SetTheme(t.id));
        h.message(Message::Back);
        h.events(&[]);
        h.screenshot(out.join(format!("{}-1-library.png", t.id)));
        h.message(Message::StartCreate);
        h.message(Message::NewTitle("Analysis II".into()));
        h.events(&[]);
        h.screenshot(out.join(format!("{}-1b-create.png", t.id)));
        h.message(Message::CancelCreate);
        h.message(Message::ShowSettings(true));
        h.events(&[]);
        h.screenshot(out.join(format!("{}-2-settings.png", t.id)));
        h.message(Message::ShowSettings(false));
        h.message(Message::Open(id.clone()));
        h.events(&[]);
        h.message(Message::ShowExport(true));
        h.events(&[]);
        h.screenshot(out.join(format!("{}-3-export.png", t.id)));
        h.message(Message::ShowExport(false));
        // The page panel, with a size of its own being typed; then the page turned.
        h.message(Message::ShowPage(true));
        h.events(&[]);
        h.screenshot(out.join(format!("{}-4-page.png", t.id)));
        h.message(Message::CustomSize(true));
        h.events(&[]);
        h.screenshot(out.join(format!("{}-4b-custom.png", t.id)));
        h.message(Message::ShowPage(false));
        h.message(Message::RotatePage);
        h.events(&[]);
        h.screenshot(out.join(format!("{}-5-turned.png", t.id)));
        h.message(Message::RotatePage);
        // Search: results in the sidebar, then the first hit shown and lit up; the outline with
        // the headings again.
        h.message(Message::SearchQuery("Grenzwert".into()));
        h.events(&[]);
        h.screenshot(out.join(format!("{}-6-search.png", t.id)));
        h.message(Message::SearchSubmit);
        h.events(&[]);
        h.screenshot(out.join(format!("{}-6b-found.png", t.id)));
        h.message(Message::SearchClear);
        h.events(&[]);
        h.screenshot(out.join(format!("{}-7-headings.png", t.id)));
        // Coordinate systems on squared paper, then the dialog for one.
        let g = match gpu.as_mut() {
            Some(g) => g,
            None => &mut h,
        };
        g.message(Message::SetTheme(t.id));
        g.message(Message::Open(id.clone()));
        g.events(&[]);
        g.message(Message::Jump(plot_sheet.clone()));
        g.events(&[]);
        g.screenshot(out.join(format!("{}-8-plots.png", t.id)));
        g.message(Message::EditPlot(plot_sheet.clone(), plot_id.clone()));
        g.events(&[]);
        g.screenshot(out.join(format!("{}-8b-plot-dialog.png", t.id)));
        g.message(Message::Plot(n0te_desktop::plots::Msg::Expr(1, "2sin x +".into())));
        g.events(&[]);
        g.screenshot(out.join(format!("{}-8c-plot-mistake.png", t.id)));
        g.message(Message::Plot(n0te_desktop::plots::Msg::Cancel));
    }
    let _ = std::fs::remove_dir_all(root);
}

/// A page of squared paper at the end with two coordinate systems: (sheet, the first plot).
fn plot_page(lib: &Library, id: &str) -> (String, String) {
    use n0te_core::notebook::{Background, SheetSpec};
    use n0te_core::plot::PlotFn;
    use n0te_desktop::plots::{self, INKS};
    let nb = lib.load_notebook(id).unwrap();
    let (_, end) = n0te_desktop::pdf::position_after(&nb, None);
    let (node, sheet_id) = nb.add_sheet(None, end, &SheetSpec { spacing: 17.0, ..SheetSpec::a4(Background::Grid) }).unwrap();
    nb.set_title_of(&node, "Funktionen").unwrap();
    lib.save_notebook(&nb).unwrap();
    let sheet = lib.load_sheet(id, &sheet_id).unwrap();
    let mut p = plots::fresh(297.0, 250.0, 595.0, 842.0, Background::Grid, 17.0);
    p.fns = vec![PlotFn { expr: "0,5x^2 - 2".into(), color: INKS[1] }, PlotFn { expr: "2sin x".into(), color: INKS[2] }];
    let first = sheet.add_plot(&p).unwrap();
    let mut q = plots::fresh(297.0, 560.0, 595.0, 842.0, Background::Blank, 0.0);
    q.grid = false;
    q.clean = true;
    (q.y_min, q.y_max) = (-1.0, 7.0);
    q.fns = vec![PlotFn { expr: "e^-x".into(), color: INKS[3] }, PlotFn { expr: "|x| - 1".into(), color: INKS[4] }];
    sheet.add_plot(&q).unwrap();
    lib.save_sheet(id, &sheet_id, &sheet).unwrap();
    (sheet_id, first)
}

/// What the tablet adds that the demo lacks: headings and handwriting it read (on the demo's
/// strokes), a text box, and an untitled cover named by its heading.
fn mark_up(lib: &Library, id: &str) {
    use n0te_core::notebook::OutlineRow;
    use n0te_core::sheet::IndexLine;
    let nb = lib.load_notebook(id).unwrap();
    let pages: Vec<(String, String, String)> = nb
        .outline(true)
        .into_iter()
        .filter_map(|r| match r {
            OutlineRow::Sheet { node, sheet_id, title, .. } => Some((node, sheet_id, title)),
            OutlineRow::Group { .. } => None,
        })
        .collect();
    let mark = |title: &str, heads: &[&str], lines: &[&str], text: Option<&str>| {
        let Some((node, sheet_id, _)) = pages.iter().find(|p| p.2.starts_with(title)) else { return };
        let sheet = lib.load_sheet(id, sheet_id).unwrap();
        let mut strokes = sheet.stroke_boxes();
        strokes.sort_by(|a, b| a.1.y0.total_cmp(&b.1.y0));
        let mut rest = strokes.into_iter().map(|s| s.0);
        for h in heads {
            let ids: Vec<String> = rest.by_ref().take(3).collect();
            sheet.add_heading(h, &ids).unwrap();
            let _ = rest.by_ref().take(20).count();
        }
        let read: Vec<IndexLine> = lines
            .iter()
            .enumerate()
            .map(|(i, l)| IndexLine { key: format!("l{i}"), text: l.to_string(), alts: Vec::new(), strokes: rest.by_ref().take(6).collect(), x0: 0.0, y0: 0.0, x1: 0.0, y1: 0.0 })
            .collect();
        sheet.set_index(&read, &[]).unwrap();
        if let Some(t) = text {
            sheet.add_text_box(320.0, 120.0, 220.0, 12.0, t).unwrap();
        }
        lib.save_sheet(id, sheet_id, &sheet).unwrap();
        if title == "Deckblatt" {
            nb.set_title_of(node, "").unwrap();
        }
    };
    mark("Deckblatt", &["Analysis I"], &[], None);
    mark("V01", &["Abbildungen", "Injektiv und surjektiv"], &["Eine Abbildung ordnet jedem Element genau eines zu"], None);
    mark("V04", &["Satz 4.1: Grenzwerte sind eindeutig", "Beweis"], &["Der Grenzwert einer Folge ist eindeutig", "Grenzwert und Teilfolgen"], Some("Übung: Grenzwert von (1 + 1/n)^n berechnen"));
    lib.save_notebook(&nb).unwrap();
}
