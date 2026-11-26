//! Ink geometry with the strokes that break naive code: no points, one, two, the same point
//! again and again, NaN and ±∞ from a confused digitizer, coordinates far off the page, zero
//! widths. Shapes, arrows, snapping, outlines, the ink encoding, handwriting lines, search, the
//! page layout: none may panic or loop forever.

mod common;

use std::time::{Duration, Instant};

use common::{Rng, Watchdog};
use n0te_core::arrows::{self, LineEnd, LineStroke};
use n0te_core::ink::{self, Brush, InkPoint, Rect};
use n0te_core::notebook::{Background, Notebook, SheetSpec, Side};
use n0te_core::sheet::{self, Sheet, Stroke};
use n0te_core::{format, inklines, layout, search, shapes, snap};

/// A stroke of one of the troublesome kinds.
fn stroke(r: &mut Rng) -> Vec<InkPoint> {
    let n = match r.below(6) {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 3 + r.below(10),
        _ => 10 + r.below(200),
    };
    let kind = r.below(7);
    let (x0, y0) = (r.range(-500.0, 1500.0), r.range(-500.0, 1500.0));
    let (dx, dy) = (r.range(-5.0, 5.0), r.range(-5.0, 5.0));
    (0..n)
        .map(|i| {
            let i_f = i as f32;
            let (x, y) = match kind {
                0 => (x0, y0),                                                       // the same point
                1 => (x0 + dx * i_f, y0 + dy * i_f),                                 // a straight line
                2 => (x0 + 50.0 * (i_f * 0.1).cos(), y0 + 50.0 * (i_f * 0.1).sin()), // a circle
                3 => (r.nasty_f32(), r.nasty_f32()),                                 // anything
                4 => (x0 + r.range(-1.0, 1.0), y0 + r.range(-1.0, 1.0)),            // a tremble
                5 => (x0 + dx * i_f * 1e6, y0),                                      // huge
                _ => (if i % 7 == 3 { f32::NAN } else { x0 + i_f }, y0 + (i_f * 0.3).sin() * 20.0),
            };
            let pressure = if r.chance(5) { r.nasty_f32() } else { r.range(0.0, 1.0) };
            let t_ms = if r.chance(5) { r.u64() as u32 } else { i as u32 * 8 };
            InkPoint { x, y, pressure, t_ms }
        })
        .collect()
}

fn brush(r: &mut Rng) -> Brush {
    if r.chance(20) {
        Brush { width: r.nasty_f32(), thinning: r.nasty_f32(), color: r.u64() as u32, kind: r.u64() as u8 }
    } else {
        Brush { width: r.range(0.5, 8.0), thinning: r.range(0.0, 1.0), color: 0xFF00_0000, kind: r.below(3) as u8 }
    }
}

/// Runs one case, named for the watchdog (a hang ends the run with the case printed).
fn timed(w: &Watchdog, what: impl Fn() -> String, f: impl FnOnce()) {
    w.case(what());
    f();
}

#[test]
fn outlines_meshes_and_the_ink_encoding_take_any_stroke() {
    let mut r = Rng::new(21);
    let w = Watchdog::new(Duration::from_secs(10));
    for _ in 0..600 {
        let pts = stroke(&mut r);
        let b = brush(&mut r);
        timed(&w, || format!("{pts:?}"), || {
            let poly = ink::outline(&pts, &b);
            assert_eq!(poly.len() % 2, 0);
            let (mut xy, mut idx) = (Vec::new(), Vec::new());
            ink::outline_mesh(&pts, &b, &mut xy, &mut idx);
            assert!(idx.iter().all(|&i| (i as usize) < xy.len() / 2), "mesh indices stay inside");
            let _ = ink::pencil_layers(&pts, &b);
            let enc = ink::encode_points(&pts);
            let back = ink::decode_points(&enc).expect("what we encode decodes");
            assert_eq!(back.len(), pts.len());
            let _ = ink::bounds(&pts, &b);
            let _ = sheet::bounds_of(&pts, b.width);
            let path: Vec<(f32, f32, f32)> = stroke(&mut r).iter().map(|p| (p.x, p.y, r.nasty_f32())).collect();
            let _ = sheet::cut_stroke(&pts, b.width, &path);
        });
    }
}

#[test]
fn decoding_random_ink_bytes_never_panics() {
    let mut r = Rng::new(22);
    for _ in 0..5_000 {
        let n = r.below(64);
        let mut b = r.bytes(n);
        if !b.is_empty() && r.chance(50) {
            b[0] = 1; // the right version byte: the varints get read
        }
        let _ = ink::decode_points(&b);
    }
    // A count that claims more points than the bytes hold, varints of 10+ bytes, deltas that
    // overflow.
    assert!(ink::decode_points(&[1, 0xFF, 0xFF, 0xFF, 0xFF, 0x0F]).is_err());
    let mut over = vec![1, 3];
    for _ in 0..3 {
        over.extend_from_slice(&[0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01]);
        over.extend_from_slice(&[0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01]);
        over.push(128);
        over.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01]);
    }
    let _ = ink::decode_points(&over);
}

#[test]
fn shapes_arrows_and_snapping_take_any_stroke() {
    let mut r = Rng::new(23);
    let w = Watchdog::new(Duration::from_secs(10));
    let o = snap::Options::default();
    for _ in 0..500 {
        let pts = stroke(&mut r);
        let still = r.nasty_f32();
        let scale = if r.chance(20) { r.nasty_f32() } else { r.range(0.2, 4.0) };
        let others: Vec<Stroke> = (0..r.below(4))
            .map(|_| {
                let p = stroke(&mut r);
                let b = brush(&mut r);
                Stroke { id: String::new(), bounds: ink::bounds(&p, &b), brush: b, points: p }
            })
            .collect();
        let lines: Vec<LineStroke> = others.iter().map(LineStroke::of).collect();
        let desc = format!("{pts:?} still {still} scale {scale} lines {:?}", lines);
        let step = |name: &str| w.case(format!("{name}: {desc}"));
        timed(&w, || desc.clone(), || {
            step("shapes::recognize");
            let _ = shapes::recognize(&pts, still, true);
            step("shapes::detect");
            if let Some(s) = shapes::detect(&pts, still, r.chance(50)) {
                step("shapes::HeldShape::new");
                if let Some(mut h) = shapes::HeldShape::new(&s, r.nasty_f32(), r.nasty_f32()) {
                    let _ = h.square();
                    let _ = h.drag(r.nasty_f32(), r.nasty_f32(), r.nasty_f32());
                }
                step("snap::Held::new");
                if let Some(mut held) = snap::Held::new(&s, r.range(0.0, 500.0), r.range(0.0, 500.0)) {
                    for k in 0..5 {
                        let _ = held.pen_at(r.nasty_f32(), r.nasty_f32(), k * 100, scale, r.nasty_f32());
                    }
                }
            }
            step("shapes::trimmed");
            let _ = shapes::trimmed(&pts, still);
            step("shapes::rest_at");
            let _ = shapes::rest_at(&pts, still);
            step("arrows::from_stroke");
            let _ = arrows::from_stroke(&pts, r.chance(50));
            step("arrows::ends");
            let _ = arrows::ends(&pts);
            let end = LineEnd { x: r.nasty_f32(), y: r.nasty_f32(), dx: r.nasty_f32(), dy: r.nasty_f32() };
            let shaft = r.nasty_f32();
            step("arrows::head");
            let _ = arrows::head(&pts, end, shaft);
            step("arrows::closed_head");
            let _ = arrows::closed_head(&pts, end, shaft);
            step("arrows::barb");
            if let (Some(b1), Some(b2)) = (arrows::barb(&pts, end, shaft), arrows::barb(&stroke(&mut r), end, shaft)) {
                step("arrows::head_from_barbs");
                let _ = arrows::head_from_barbs(b1, b2, end, shaft, r.nasty_f32(), r.nasty_f32());
            }
            step("arrows::barb_from");
            let _ = arrows::barb_from(r.nasty_f32(), r.nasty_f32(), r.nasty_f32(), r.nasty_f32(), r.nasty_f32(), end, shaft);
            step("arrows::head_at_line_ends");
            let _ = arrows::head_at_line_ends(&pts, still, &lines, r.chance(50));
            step("arrows::barb_pair");
            let _ = arrows::barb_pair(&pts, still, &lines);
            step("snap::on_hold");
            let _ = snap::on_hold(snap::Hold::Shape, &pts, scale, &lines, o);
            step("snap::on_hold");
            let _ = snap::on_hold(snap::Hold::Arrow, &pts, scale, &lines, o);
            step("snap::on_lift");
            let _ = snap::on_lift(&pts, scale, r.u64() as u32, &lines, o);
            step("snap::HeldLine::new");
            if let Some(mut hl) = snap::HeldLine::new(&pts) {
                let _ = hl.free();
                for k in 0..5 {
                    let _ = hl.pen_at(r.nasty_f32(), r.nasty_f32(), k * 50, scale, r.nasty_f32());
                }
            }
            let mut rest = snap::Rest::new(r.nasty_f32(), r.nasty_f32(), 0);
            let _ = rest.moved(r.nasty_f32(), r.nasty_f32(), r.u64() as u32, scale);
            let _ = rest.held_ms(r.u64() as u32);
        });
    }
}

#[test]
fn sheets_take_strokes_with_any_numbers() {
    let mut r = Rng::new(24);
    let s = Sheet::create(1).unwrap();
    for _ in 0..200 {
        let pts = stroke(&mut r);
        let b = brush(&mut r);
        if let Ok(id) = s.add_stroke(b, &pts) {
            let st = s.stroke(&id).unwrap().expect("what was added is there");
            assert!(st.points.iter().all(|p| p.x.is_finite() && p.y.is_finite()), "stored samples are numbers: {:?}", st.points);
            let bb = st.bounds;
            assert!(bb.is_empty() || (bb.x0.is_finite() && bb.y0.is_finite() && bb.x1.is_finite() && bb.y1.is_finite()), "bounds {bb:?}");
        }
        let (x, y, rad) = (r.nasty_f32(), r.nasty_f32(), r.nasty_f32());
        let _ = s.hit_test(x, y, rad);
        let poly: Vec<f32> = (0..r.below(12)).map(|_| r.nasty_f32()).collect();
        let _ = s.lasso(&poly);
        let path: Vec<(f32, f32)> = (0..r.below(6)).map(|_| (r.nasty_f32(), r.nasty_f32())).collect();
        let _ = s.hit_path(&path, rad);
        let dabs: Vec<(f32, f32, f32)> = path.iter().map(|&(x, y)| (x, y, r.nasty_f32())).collect();
        let _ = s.erase_area(&dabs);
        let ids: Vec<String> = s.strokes().into_iter().take(3).map(|s| s.id).collect();
        let _ = s.transform(&ids, r.nasty_f32(), r.nasty_f32(), r.nasty_f32(), r.nasty_f32(), r.nasty_f32());
        let _ = s.stretch(&ids, r.nasty_f32(), r.nasty_f32(), r.nasty_f32(), r.nasty_f32());
        let _ = s.spin(&ids, r.nasty_f32(), r.nasty_f32(), r.nasty_f32(), r.nasty_f32());
        let _ = s.shift_below(r.nasty_f32(), r.nasty_f32());
        let _ = s.below(r.nasty_f32());
        let _ = s.outside(r.nasty_f32(), r.nasty_f32());
        let _ = s.add_text_box(r.nasty_f32(), r.nasty_f32(), r.nasty_f32(), r.nasty_f32(), "ä\u{1f}x");
        let _ = s.add_image("b", r.nasty_f32(), r.nasty_f32(), r.nasty_f32(), r.nasty_f32());
        let _ = s.search("ä x", r.nasty_f32());
    }
    let copy = Sheet::load(2, &s.snapshot().unwrap()).unwrap();
    assert_eq!(copy.len(), s.len());
    let _ = s.strokes_in(Some(Rect::new(f32::NAN, 0.0, f32::INFINITY, 1.0)));
}

#[test]
fn notebooks_and_layout_take_any_page_size() {
    let mut r = Rng::new(25);
    let nb = Notebook::create(1, "x").unwrap();
    for _ in 0..200 {
        let spec = SheetSpec {
            width: r.nasty_f32(),
            height: r.nasty_f32(),
            background: Background::Grid,
            spacing: r.nasty_f32(),
            pdf: if r.chance(30) { Some(("b".repeat(64), r.u64() as u32)) } else { None },
            gaps: (0..r.below(4)).map(|_| (r.nasty_f32(), r.nasty_f32())).collect(),
        };
        if let Ok((node, _)) = nb.add_sheet(None, r.below(10), &spec) {
            let _ = nb.add_side_sheet(&node, if r.chance(50) { Side::Left } else { Side::Right }, r.below(3), &spec);
            let _ = nb.reshape(&node, r.nasty_f32(), &[(r.nasty_f32(), r.nasty_f32())]);
            let _ = nb.set_size(&node, r.nasty_f32(), r.nasty_f32());
        }
        let _ = n0te_core::notebook::plan_space(&spec, r.nasty_f32(), r.nasty_f32(), n0te_core::notebook::Below { top: Some(r.nasty_f32()), bottom: Some(r.nasty_f32()) });
        let _ = n0te_core::notebook::plan_space_x(&spec, r.nasty_f32(), r.nasty_f32(), n0te_core::notebook::Below { top: Some(r.nasty_f32()), bottom: None });
        let _ = spec.bands();
        let _ = format::describe(spec.width, spec.height);
        let _ = format::find(spec.width, spec.height);
        let _ = layout::display_size(&spec);
    }
    // Whatever went in: what comes out can be planned on and laid out.
    for s in nb.sheet_specs().values() {
        let _ = n0te_core::notebook::plan_space(s, 100.0, 50.0, Default::default());
        let _ = n0te_core::notebook::plan_space(s, 100.0, -50.0, n0te_core::notebook::Below { top: Some(120.0), bottom: Some(300.0) });
    }
    let rows = nb.outline(true);
    let t = Instant::now();
    let l = layout::layout(&rows, &rows, layout::Options::default());
    assert!(t.elapsed() < Duration::from_secs(1));
    let _ = l;
}

#[test]
fn handwriting_lines_and_search_take_any_boxes_and_text() {
    let mut r = Rng::new(26);
    let texts = ["", " ", "ä", "Übung", "STRASSE", "straße", "ǅ", "İ", "ﬁ", "😀😀", "a\u{301}", "\u{1f}\u{1e}", "x".repeat(500).leak()];
    for _ in 0..1000 {
        let boxes: Vec<inklines::Box> = (0..r.below(40)).map(|k| inklines::Box::new(format!("s{k}"), r.nasty_f32(), r.nasty_f32(), r.nasty_f32(), r.nasty_f32())).collect();
        let lines = inklines::group(&boxes);
        for l in &lines {
            let _ = inklines::key(l);
        }
        if let Some(b) = boxes.first() {
            let _ = inklines::word(&boxes, &b.id);
        }
        let q = format!("{} {}", r.pick(&texts), r.pick(&texts));
        let w = search::words(&q);
        let _ = search::score(&w, &search::normalize(r.pick(&texts)));
        let _ = search::normalize(&q);
    }
}
