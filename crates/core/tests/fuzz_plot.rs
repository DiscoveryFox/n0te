//! The construction language (GeoGebra-style input, LaTeX from the recogniser, handwriting read
//! as text) and everything that works on its objects, fed random and hostile input: nothing may
//! panic, hang or overflow the stack (a panic is an exception in the tablet app, a stack
//! overflow kills it). Deterministic: the same cases every run.

mod common;

use std::collections::HashSet;
use std::time::{Duration, Instant};

use common::{Rng, Watchdog};

thread_local! {
    static WATCHDOG: Watchdog = Watchdog::new(Duration::from_secs(10));
}
use n0te_core::plot::{self, Plot};

const TOKENS: &[&str] = &[
    "x", "y", "t", "a", "b", "c", "f", "g", "h", "A", "B", "C", "P", "k", "θ", "φ", "e", "pi", "(", ")", "[", "]", "{", "}", "|", ",", ";", ":", "=", "<", ">",
    "≤", "≥", "<=", "+", "-", "−", "*", "·", "/", "^", "²", "³", "√", "π", "'", "°", "0", "1", "2", "3.5", "0,5", "1e308", "1e-308", "99999999999999999999",
    "-1", " ", "  ", "sin", "cos", "tan", "cot", "ln", "lg", "log", "sqrt", "abs", "exp", "floor", "round", "sgn", "Schneide", "Intersect", "Gerade", "Line",
    "Kreis", "Circle", "Strecke", "Strahl", "Vieleck", "Polygon", "Tangente", "Nullstelle", "Root", "Extremum", "Wendepunkt", "Integral",
    "IntegralZwischen", "Ableitung", "Derivative", "Wenn", "If", "Kurve", "Curve", "Punkt", "Vektor", "Winkel", "Text", "Mittelpunkt", "Abstand",
    "Länge", "Laenge", "Steigung", "Senkrechte", "Parallele", "Mittelsenkrechte", "Winkelhalbierende", "Spiegle", "Verschiebe", "Drehe", "Fläche",
    "Min", "Max", "Funktion", "Schieberegler", "\"", "\"Hallo\"", "für", "for", "(1,2)", "(1|2)", "(0, 0)", "f(x) =", "g(x)=", "A =", "a =", "c:", "r =",
    "f'(x)", "f''", "x^2", "1/x", "x/0", "0/0", "ä", "ö", "ü", "ß", "€", "😀", "\u{1f}", "\u{1e}", "\u{1d}", "\n", "\t", "\\", "_", "&", "#", "%", "!",
];

const LATEX: &[&str] = &[
    "\\frac", "\\sqrt", "{", "}", "[", "]", "x", "^", "_", "2", "\\cdot", "\\sin", "\\left(", "\\right)", "\\pi", "\\", "\\,", "\\|", "&", "\n", "a", "+",
    "-", "=", "f(x)", "\\mathrm", "\\text{", "\\vert", "\\e", "\\unknown", "ä", " ", "$",
];

fn random_input(r: &mut Rng, tokens: &[&str], max: usize) -> String {
    let n = 1 + r.below(max);
    (0..n).map(|_| *r.pick(tokens)).collect::<Vec<_>>().join(if r.chance(30) { " " } else { "" })
}

fn base() -> Plot {
    Plot::centered(100.0, 100.0, 240.0, 160.0, 20.0, 0xFF22_2222, 1.2)
}

/// Everything the apps do with a plot after an edit: values, the algebra rows, ink, hits, drags.
fn exercise(p: &Plot) {
    let vals = p.values();
    let known: HashSet<String> = p.objs.iter().map(|o| o.name()).collect();
    for (o, v) in p.objs.iter().zip(&vals) {
        if let Ok(v) = v {
            let _ = plot::format_val(v);
            let _ = plot::row_text(o, v, &known, false);
            let _ = plot::row_text(o, v, &known, true);
            let _ = plot::row_latex(o, v, &known);
            if let plot::Val::Func(e) = v {
                let _ = plot::domain_parts(e);
                let _ = plot::fmt_expr(e);
                let _ = plot::latex_expr(e, "x");
            }
        }
        let _ = plot::definition(&o.def, &known);
        let _ = p.curve_switch(&o.id);
        let _ = p.dependents(&o.id);
        let _ = p.drag(&o.id, 1.5, -2.0);
        let _ = p.set_slider(&o.id, 3.0);
        let _ = plot::moved_on(&o.def, 0.5);
        let _ = plot::free_point(&o.def);
    }
    let ink = plot::render(p);
    for (brush, pts) in &ink {
        assert!(brush.width.is_finite(), "ink of {:?}", p.objs);
        let _ = pts.len();
    }
    let _ = plot::hit(p, 200.0, 180.0, 6.0);
    let _ = p.fit_y();
    let _ = p.pi_range();
    let _ = p.equal_units();
    let _ = p.domain_end_at(150.0, 150.0, 5.0);
    let _ = p.axis_dragged(plot::Axis::X, 120.0, 140.0);
    let (lo, hi) = (p.x_min, p.x_max);
    let _ = plot::nice_step(hi - lo, 8.0);
    let _ = plot::tick_label(lo, (hi - lo) / 8.0);
}

/// Runs `f` and fails if it takes longer than `limit`; the watchdog ends a run that hangs,
/// with the case printed.
fn timed(what: &str, limit: Duration, f: impl FnOnce()) {
    WATCHDOG.with(|w| w.case(what));
    let t = Instant::now();
    f();
    let took = t.elapsed();
    assert!(took < limit, "{what} took {took:?}");
}

#[test]
fn random_construction_input_never_panics_or_hangs() {
    let mut r = Rng::new(1);
    let limit = Duration::from_secs(5);
    for case in 0..150 {
        let mut p = base();
        // A few lines in a row: later ones can refer to earlier ones (and to themselves).
        for _ in 0..1 + r.below(4) {
            let input = random_input(&mut r, TOKENS, 10);
            timed(&format!("case {case}: add {input:?} to {:?}", p.objs.iter().map(|o| &o.def).collect::<Vec<_>>()), limit, || {
                if let Ok(next) = p.add(&input) {
                    p = next;
                }
            });
        }
        let defs: Vec<String> = p.objs.iter().map(|o| o.def.clone()).collect();
        timed(&format!("case {case}: exercise {defs:?}"), limit, || exercise(&p));
        if let Some(o) = p.objs.first() {
            let input = random_input(&mut r, TOKENS, 8);
            timed(&format!("case {case}: set_def {input:?} on {defs:?}"), limit, || {
                if let Ok(q) = p.set_def(&o.id, &input) {
                    exercise(&q);
                }
            });
            let _ = p.remove(&o.id);
        }
    }
}

#[test]
fn constructions_people_type_work_after_any_mix() {
    // Real lines in random order and combination: whatever they make, it must evaluate.
    let lines = [
        "A = (1, 2)", "B = (3, -1)", "f(x) = x^2 - 1", "g(x) = sin(x)", "S = Schneide(f, g)", "a = Schieberegler(0, 5, 0.5)", "h(x) = a x + 1",
        "c: x^2 + y^2 = 4", "k = Kreis(A, B)", "l = Gerade(A, B)", "T = Tangente(A, f)", "N = Nullstelle(f)", "E = Extremum(f)", "W = Wendepunkt(g)",
        "F = Integral(f, -1, 1)", "G = IntegralZwischen(f, g, 0, 1)", "p(x) = f'(x)", "q(x) = Wenn(x < 0, -x, x)", "r = 1 + cos θ", "v = Vektor(A, B)",
        "w = Winkel(A, (0,0), B)", "s = Strecke(A, B)", "P = Vieleck(A, B, (0, 3))", "M = Mittelpunkt(A, B)", "d = Abstand(A, B)",
        "K = Kurve(cos(t), sin(t), t, 0, 2π)", "u(x) = x² für 0 ≤ x ≤ 2", "T1 = Text(\"Hallo\")", "b = 4 + 3a", "y = m x + c", "f(x) = f(x) + 1",
        "a = b", "b = a", "Z = Spiegle(A, l)", "Q = Drehe(A, 90°, B)", "m = Steigung(l)",
    ];
    let mut r = Rng::new(2);
    for case in 0..60 {
        let mut p = base();
        for _ in 0..2 + r.below(8) {
            let line = *r.pick(&lines);
            timed(&format!("case {case}: {line}"), Duration::from_secs(5), || {
                if let Ok(next) = p.add(line) {
                    p = next;
                }
            });
        }
        timed(&format!("case {case}: exercise"), Duration::from_secs(5), || exercise(&p));
    }
}

#[test]
fn deep_nesting_is_refused_not_a_stack_overflow() {
    // Run with a small stack on purpose (like a worker thread on the tablet): input nested
    // thousands of levels deep must come back as an error, not take the app down.
    let deep = |open: &str, mid: &str, close: &str, n: usize| format!("{}{mid}{}", open.repeat(n), close.repeat(n));
    let cases = vec![
        deep("(", "x", ")", 20_000),
        // Under the length limit, still too deep (the nesting limit, not the length, stops these).
        deep("(", "x", ")", 450),
        deep("|", "x", "|", 450),
        format!("{}x", "-".repeat(900)),
        format!("x{}", "^x".repeat(450)),
        format!("{}x", "sin ".repeat(400)),
        deep("[", "x", "]", 20_000),
        deep("|", "x", "|", 20_000),
        format!("{}x", "-".repeat(50_000)),
        format!("{}x", "+".repeat(50_000)),
        format!("x{}", "^x".repeat(20_000)),
        format!("{}x", "sin ".repeat(20_000)),
        format!("{}x{}", "sin(".repeat(20_000), ")".repeat(20_000)),
        format!("f(x) = {}", deep("(", "x", ")", 20_000)),
        format!("{}1{}", "Wenn(x<0, ".repeat(5_000), ", 1)".repeat(5_000)),
        format!("({}1{})", "(1, ".repeat(10_000), ")".repeat(10_000)),
        format!("f{}(x)", "'".repeat(300)),
        format!("x{}", "²".repeat(20_000)),
    ];
    // 512 KB, half of an Android worker thread's stack, in optimised builds (what ships). An
    // unoptimised build (CI's `cargo test`) has frames several times larger, so it gets more:
    // the nesting limits are what's under test, not the debug build's frame sizes.
    let stack = if cfg!(debug_assertions) { 4 << 20 } else { 512 << 10 };
    let t = std::thread::Builder::new()
        .stack_size(stack)
        .spawn(move || {
            for input in &cases {
                let p = base();
                let t = Instant::now();
                if let Ok(q) = p.add(input) {
                    exercise(&q);
                }
                let _ = plot::lang::parse_input(input, &HashSet::new());
                let _ = plot::parse(input);
                let _ = plot::latex_input(input, &HashSet::new());
                let _ = plot::handwritten_input(input);
                assert!(t.elapsed() < Duration::from_secs(5), "{}…: {:?}", input.chars().take(20).collect::<String>(), t.elapsed());
            }
            // LaTeX groups.
            for input in [deep("{", "x", "}", 20_000), format!("{}x", "\\frac{1}{".repeat(10_000)), format!("{}2", "x^{".repeat(20_000)), format!("{}x", "\\sqrt[".repeat(10_000))] {
                let _ = plot::latex_to_expr(&input);
            }
        })
        .unwrap();
    t.join().expect("no panic, no stack overflow");
}

#[test]
fn a_function_of_itself_or_of_a_point_is_a_mistake_not_a_crash() {
    // Typed in the app, these recursed until the stack ran out.
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(|| {
            let with_point = base().add("A = (1, 2)").unwrap();
            for (p, line) in [(base(), "f(x) = f(x) + 1"), (base(), "f(x) = 2 f(x - 1)"), (with_point.clone(), "g(x) = A(x)"), (with_point, "g(x) = A(x) + x")] {
                match p.add(line) {
                    Ok(q) => assert!(q.errors().iter().any(|e| e.is_some()), "{line}: {:?}", q.errors()),
                    Err(e) => assert!(!e.is_empty()),
                }
            }
        })
        .unwrap()
        .join()
        .expect("no stack overflow");
}

#[test]
fn random_latex_and_handwriting_never_panic() {
    let mut r = Rng::new(3);
    let mut p = base();
    p = p.add("f(x) = x^2").unwrap();
    let known: HashSet<String> = p.objs.iter().map(|o| o.name()).collect();
    for _ in 0..1500 {
        let latex = random_input(&mut r, LATEX, 14);
        if let Ok(expr) = plot::latex_to_expr(&latex) {
            let _ = p.add(&expr);
        }
        let text = random_input(&mut r, TOKENS, 10);
        let hand = plot::handwritten_input(&text);
        let _ = plot::latex_input(&hand, &known);
        let _ = plot::lang::parse_input(&hand, &known);
        let _ = plot::parse(&text);
        let _ = plot::check(&text);
        let _ = plot::lang::unknown_letters(&text, &known);
        let _ = plot::name_of(&text);
        let _ = plot::renamed(&text, "q");
        let _ = plot::lang::rename_name(&text, "f", "g", &known);
        let _ = plot::rename_refs(&text, 3, &|k| Some(k + 1));
    }
}

#[test]
fn hostile_views_and_numbers_never_panic() {
    // The apps hand in plots as they got them (from another device, from the UI mid-drag).
    let mut r = Rng::new(4);
    let mut good = base();
    for l in ["A = (1, 2)", "f(x) = x^2 - 1", "a = Schieberegler(0, 5, 0.5)", "c: x^2 + y^2 = 4", "F = Integral(f, -1, 1)", "T1 = Text(\"x\")"] {
        good = good.add(l).unwrap();
    }
    let nasty64 = |r: &mut Rng| match r.below(8) {
        0 => f64::NAN,
        1 => f64::INFINITY,
        2 => -f64::INFINITY,
        3 => 1e300,
        4 => -1e-300,
        5 => 0.0,
        _ => (r.range(-50.0, 50.0)) as f64,
    };
    for _ in 0..80 {
        let mut p = good.clone();
        p.x = r.nasty_f32();
        p.y = r.nasty_f32();
        p.w = r.nasty_f32();
        p.h = r.nasty_f32();
        p.x_min = nasty64(&mut r);
        p.x_max = nasty64(&mut r);
        p.y_min = nasty64(&mut r);
        p.y_max = nasty64(&mut r);
        p.width = r.nasty_f32();
        if r.chance(30) {
            for o in p.objs.iter_mut() {
                o.style.size = r.nasty_f32();
                o.style.width = r.nasty_f32();
                o.style.fill = r.nasty_f32();
                o.order = nasty64(&mut r);
                if r.chance(30) {
                    o.style.at = Some((r.nasty_f32(), r.nasty_f32()));
                }
                if r.chance(30) {
                    o.style.slider = Some(plot::Slider { min: nasty64(&mut r), max: nasty64(&mut r), step: nasty64(&mut r) });
                }
            }
        }
        let t = Instant::now();
        exercise(&p);
        let (x, y) = (r.nasty_f32(), r.nasty_f32());
        let _ = p.to_plot(x, y);
        let _ = p.to_page(nasty64(&mut r), nasty64(&mut r));
        let _ = p.snapped(nasty64(&mut r), nasty64(&mut r));
        let _ = plot::hit(&p, x, y, r.nasty_f32());
        let _ = p.axis_at(x, y, r.nasty_f32());
        let _ = p.axis_dragged(plot::Axis::Y, r.nasty_f32(), r.nasty_f32());
        let _ = p.add_point_on(&p.objs[1].id.clone(), nasty64(&mut r), nasty64(&mut r));
        let _ = p.set_domain(&p.objs[1].id.clone(), Some(nasty64(&mut r)), Some(nasty64(&mut r)));
        let _ = p.drag_domain_end(&p.objs[1].id.clone(), r.below(3) as u8, nasty64(&mut r));
        for i in 0..p.objs.len() {
            let _ = plot::slider_track(&p, i);
        }
        let _ = plot::nice_step(nasty64(&mut r), nasty64(&mut r));
        let _ = plot::tick_label(nasty64(&mut r), nasty64(&mut r));
        let _ = plot::pi_label(r.u64() as i64, r.u64() as i64);
        let _ = plot::fmt_num(nasty64(&mut r));
        let _ = plot::exact_fraction(nasty64(&mut r));
        assert!(t.elapsed() < Duration::from_secs(5), "slow on {p:?}");
    }
}
