//! Every exported call with the arguments a buggy or racing caller can send: NaN and ±∞ from
//! the pen, byte arrays of odd length, ids of nothing (or of `../`), empty lists, indexes far
//! past the end. A Rust panic is an exception in the tablet app (and a crash when nothing
//! catches it), so all of them must come back as values or errors.

use std::sync::Arc;

use n0te_ffi::*;

struct Rng(u64);
impl Rng {
    fn u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.u64() % n as u64) as usize
    }
    fn f(&mut self) -> f32 {
        match self.below(10) {
            0 => f32::NAN,
            1 => f32::INFINITY,
            2 => f32::NEG_INFINITY,
            3 => f32::MAX,
            4 => -f32::MAX,
            5 => 0.0,
            _ => (self.u64() % 2000) as f32 - 1000.0,
        }
    }
    fn d(&mut self) -> f64 {
        match self.below(8) {
            0 => f64::NAN,
            1 => f64::INFINITY,
            2 => -1e300,
            3 => 0.0,
            _ => (self.u64() % 200) as f64 / 10.0 - 10.0,
        }
    }
    fn floats(&mut self, max: usize) -> Vec<u8> {
        let n = self.below(max + 1);
        let mut out: Vec<u8> = (0..n).flat_map(|_| self.f().to_le_bytes()).collect();
        // Sometimes a length that is no multiple of 4 (or 16).
        if self.below(4) == 0 {
            out.push(7);
        }
        out
    }
    fn id(&mut self) -> String {
        ["", "..", "../../etc/passwd", "x/y", "nope", "0@0", "18446744073709551615@4294967295", "ä", &"a".repeat(200), "\0"][self.below(10)].to_string()
    }
}

struct Dir(std::path::PathBuf);
impl Dir {
    fn new(tag: &str) -> Self {
        let d = std::env::temp_dir().join(format!("n0te-ffi-hostile-{tag}-{}-{}", std::process::id(), n0te_core::ids::new_id(5)));
        std::fs::create_dir_all(&d).unwrap();
        Dir(d)
    }
    fn s(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
}
impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn spec(r: &mut Rng) -> FfiSheetSpec {
    let gaps = (0..r.below(5)).map(|_| r.f()).collect();
    FfiSheetSpec {
        width: r.f(),
        height: r.f(),
        background: [FfiBackground::Blank, FfiBackground::Lines, FfiBackground::Grid, FfiBackground::Dots, FfiBackground::Cornell, FfiBackground::Text][r.below(6)],
        spacing: r.f(),
        pdf_blob: if r.below(2) == 0 { Some(r.id()) } else { None },
        pdf_page: r.u64() as u32,
        gaps,
    }
}

fn brush(r: &mut Rng) -> FfiBrush {
    FfiBrush { width: r.f(), thinning: r.f(), color: r.u64() as u32, kind: r.u64() as u8 }
}

#[test]
fn free_functions_take_any_numbers() {
    let mut r = Rng(1);
    for _ in 0..1500 {
        let _ = stroke_outline(brush(&mut r), r.floats(64));
        let _ = stroke_layers(FfiBrush { kind: 1, ..brush(&mut r) }, r.floats(64));
        let _ = background_bands(spec(&mut r));
        let below = FfiBelow { top: Some(r.f()), bottom: if r.below(2) == 0 { Some(r.f()) } else { None } };
        let _ = plan_space(spec(&mut r), r.f(), r.f(), below);
        let _ = plan_space_x(spec(&mut r), r.f(), r.f(), below);
        let _ = describe_page_size(r.f(), r.f());
        let _ = page_format_id(r.f(), r.f());
        let _ = pdf::pdf_page_pixels(r.f(), r.f(), r.f());
        let _ = plot_from_latex(["\\frac{", "}}}", "x^", "\\sqrt[", "ä\\", ""][r.below(6)].to_string());
        let _ = plot_from_handwriting(["X2", "ä²", "))", "x22222", ""][r.below(5)].to_string());
    }
}

fn plot(r: &mut Rng) -> FfiPlot {
    let mut p = FfiPlot {
        id: "p".into(),
        x: 10.0,
        y: 10.0,
        w: 200.0,
        h: 150.0,
        x_min: -5.0,
        x_max: 5.0,
        y_min: -5.0,
        y_max: 5.0,
        axes: true,
        grid: true,
        minor: true,
        clean: false,
        color: 0xFF000000,
        width: 1.2,
        pi: r.below(2) == 0,
        x_name: String::new(),
        y_name: String::new(),
        snap: r.u64() as u8,
        align: true,
        interactive: true,
        objs: vec![],
    };
    for line in ["A = (1, 2)", "f(x) = x^2 - 1", "a = Schieberegler(0, 5, 0.5)", "g(x) = f(x) für 0 ≤ x ≤ 2", "T1 = Text(\"ä\")"] {
        p = plot_add(p, line.into()).unwrap();
    }
    if r.below(2) == 0 {
        p.x = r.f();
        p.y = r.f();
        p.w = r.f();
        p.h = r.f();
        p.x_min = r.d();
        p.x_max = r.d();
        p.y_min = r.d();
        p.y_max = r.d();
        p.width = r.f();
    }
    if r.below(3) == 0 {
        for o in p.objs.iter_mut() {
            o.style.at = (0..r.below(4)).map(|_| r.f()).collect();
            o.style.size = r.f();
            o.style.width = r.f();
            o.style.line = r.u64() as u8;
            o.style.label = r.u64() as u8;
            o.style.point = r.u64() as u8;
            if r.below(2) == 0 {
                o.style.slider = Some(FfiSlider { min: r.d(), max: r.d(), step: r.d() });
            }
        }
    }
    p
}

#[test]
fn plot_calls_take_any_plot() {
    let mut r = Rng(2);
    for _ in 0..15 {
        let p = plot(&mut r);
        let id = if r.below(4) == 0 { r.id() } else { p.objs[r.below(p.objs.len())].id.clone() };
        let _ = plot_strokes(p.clone());
        let _ = plot_info(p.clone());
        let _ = plot_add(p.clone(), r.id());
        let _ = plot_add_point_on(p.clone(), id.clone(), r.d(), r.d());
        let _ = plot_set_def(p.clone(), id.clone(), r.id());
        let _ = plot_remove(p.clone(), id.clone());
        let _ = plot_dependents(p.clone(), id.clone());
        let _ = plot_drag(p.clone(), id.clone(), r.d(), r.d());
        let _ = plot_set_slider(p.clone(), id.clone(), r.d());
        let _ = plot_hit(p.clone(), r.f(), r.f(), r.f());
        let _ = plot_to_plot(p.clone(), r.f(), r.f());
        let _ = plot_to_page(p.clone(), r.d(), r.d());
        let _ = plot_snapped(p.clone(), r.d(), r.d());
        let _ = plot_slider_track(p.clone(), id.clone());
        let _ = plot_fit_y(p.clone());
        let _ = plot_input_latex(p.clone(), "f(x) = \\frac{1}{x}".into());
        let _ = plot_pi_range(p.clone());
        let _ = plot_equal_units(p.clone());
        let _ = plot_set_domain(p.clone(), id.clone(), Some(r.d()), Some(r.d()));
        let _ = plot_domain_end_at(p.clone(), r.f(), r.f(), r.f());
        let _ = plot_drag_domain_end(p.clone(), id.clone(), r.u64() as u8, r.d());
        let _ = plot_axis_at(p.clone(), r.f(), r.f(), r.f());
        let _ = plot_axis_dragged(p.clone(), r.u64() as u8, r.f(), r.f());
        let _ = plot_as_curve(p.clone(), id.clone());
        let _ = plot_as_sliders(p.clone(), id.clone());
    }
}

#[test]
fn library_notebook_and_sheet_calls_take_any_arguments() {
    let dir = Dir::new("lib");
    let lib = NoteLibrary::open(dir.s()).unwrap();
    let mut r = Rng(3);
    let nb = lib.clone().create_notebook("ä".into()).unwrap();
    let a4 = FfiSheetSpec { width: 595.0, height: 842.0, background: FfiBackground::Grid, spacing: 20.0, pdf_blob: None, pdf_page: 0, gaps: vec![] };
    let ids = nb.add_sheet(None, 0, a4.clone()).unwrap();
    let side = nb.add_side_sheet(ids[0].clone(), true, u32::MAX, a4.clone()).unwrap();
    let group = nb.add_group(None, u32::MAX, "G".into()).unwrap();
    let sheet = nb.clone().open_sheet(ids[1].clone()).unwrap();
    let samples: Vec<u8> = (0..20).flat_map(|i| [i as f32 * 3.0, 50.0, 0.5, i as f32 * 8.0]).flat_map(|v| v.to_le_bytes()).collect();
    let good = sheet.add_stroke(FfiBrush { width: 2.0, thinning: 0.5, color: 0xFF000000, kind: 0 }, samples).unwrap();

    let base_plot = plot(&mut r);
    for _ in 0..120 {
        let mut pl = base_plot.clone();
        (pl.x, pl.w, pl.x_min, pl.y_max) = (r.f(), r.f(), r.d(), r.d());
        // Library.
        let _ = lib.clone().load_notebook(r.id());
        let _ = lib.blob_path(r.id());
        let _ = lib.export_bundle(r.id(), dir.0.join("x.n0te").to_string_lossy().into_owned());
        let _ = lib.import_bundle(dir.0.join("missing.n0te").to_string_lossy().into_owned());
        let _ = lib.recent_sheets(r.u64() as u32);
        let _ = lib.change_stamp(r.id());
        // Notebook.
        let node = match r.below(5) {
            0 => ids[0].clone(),
            1 => side[0].clone(),
            2 => group.clone(),
            _ => r.id(),
        };
        let _ = nb.add_group(Some(node.clone()), r.u64() as u32, r.id());
        let _ = nb.add_sheet(Some(node.clone()), r.u64() as u32, spec(&mut r));
        let _ = nb.add_side_sheet(node.clone(), r.below(2) == 0, r.u64() as u32, spec(&mut r));
        let _ = nb.move_node(node.clone(), Some(group.clone()), r.u64() as u32);
        let _ = nb.move_node(group.clone(), Some(node.clone()), r.u64() as u32);
        let _ = nb.set_collapsed(node.clone(), true);
        let _ = nb.set_node_title(node.clone(), r.id());
        let _ = nb.set_paper(node.clone(), FfiBackground::Dots, r.f());
        let _ = nb.set_size(node.clone(), r.f(), r.f());
        let gaps: Vec<f32> = (0..r.below(5)).map(|_| r.f()).collect();
        let _ = nb.reshape(node.clone(), r.f(), gaps);
        let _ = nb.set_marked(node.clone(), true);
        let _ = nb.outside(vec![node.clone(), ids[0].clone()], r.f(), r.f());
        let _ = nb.set_tags(vec![r.id(), "\u{1f}".into()]);
        let _ = nb.set_folder(r.id());
        let _ = nb.outline(true);
        let _ = nb.headings();
        let _ = nb.search(r.id(), None);
        let _ = nb.marked();
        let _ = nb.sheet_order();
        let _ = nb.clone().open_sheet(r.id());
        // Sheet.
        let some_ids = vec![good.id.clone(), r.id()];
        let _ = sheet.strokes_in(r.f(), r.f(), r.f(), r.f());
        let _ = sheet.hit_test(r.f(), r.f(), r.f());
        let _ = sheet.hit_path(r.floats(12), r.f());
        let _ = sheet.erase_area(r.floats(18));
        let _ = sheet.lasso(r.floats(12));
        let _ = sheet.add_stroke(brush(&mut r), r.floats(32));
        let _ = sheet.recolor(some_ids.clone(), r.u64() as u32);
        let _ = sheet.transform(some_ids.clone(), r.f(), r.f(), r.f(), r.f(), r.f());
        let _ = sheet.spin(some_ids.clone(), r.f(), r.f(), r.f(), r.f());
        let _ = sheet.stretch(some_ids.clone(), r.f(), r.f(), r.f(), r.f());
        let _ = sheet.below(r.f());
        let _ = sheet.shift_below(r.f(), r.f());
        let _ = sheet.right_of(r.f());
        let _ = sheet.shift_right_of(r.f(), r.f());
        let _ = sheet.add_image(r.id(), r.f(), r.f(), r.f(), r.f());
        let _ = sheet.place_image(r.id(), r.f(), r.f(), r.f(), r.f());
        let _ = sheet.add_text_box(r.f(), r.f(), r.f(), r.f(), r.id());
        let _ = sheet.text_boxes_with_body(r.f());
        let _ = sheet.set_page_text(r.id());
        let _ = sheet.add_heading(r.id(), some_ids.clone());
        let _ = sheet.set_heading_text(r.id(), r.id());
        let line = FfiIndexLine { key: r.id(), text: r.id(), alts: vec![r.id()], strokes: some_ids.clone(), x0: r.f(), y0: r.f(), x1: r.f(), y1: r.f() };
        let _ = sheet.set_index(vec![line], vec![r.id()]);
        let _ = sheet.index_lines();
        let _ = sheet.headings();
        let _ = sheet.stroke_data(some_ids.clone());
        let item = FfiStrokeData { brush: brush(&mut r), samples: r.floats(40) };
        let _ = sheet.add_strokes(vec![item], r.f(), r.f(), r.f(), r.f(), r.f());
        let _ = sheet.strokes_by_id(some_ids.clone());
        let _ = sheet.images();
        let _ = sheet.stroke_boxes();
        let _ = sheet.undo();
        let _ = sheet.redo();
        let _ = sheet.add_plot(pl.clone());
        let _ = sheet.set_plot(pl);
    }
    // Still a working notebook afterwards: it saves, loads and draws.
    nb.save().unwrap();
    sheet.save().unwrap();
    let again = lib.clone().load_notebook(nb.id()).unwrap();
    let _ = again.outline(true);
    let reopened = again.clone().open_sheet(ids[1].clone()).unwrap();
    let _ = reopened.all_strokes();
    let _ = Arc::strong_count(&reopened);
}
