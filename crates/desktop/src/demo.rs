//! A demo notebook built with n0te-core: ~30 A4 pages in groups, a few side pages, and fake
//! cursive handwriting (100-400 strokes of ~120 samples per page). Deterministic, so the
//! measurements are repeatable.

use std::f32::consts::{PI, TAU};

use n0te_core::ink::{Brush, InkPoint};
use n0te_core::notebook::{Background, Notebook, SheetSpec, Side};
use n0te_core::sheet::Sheet;

pub const PEER: u64 = 0x6e30_7465;

pub const INK_BLACK: u32 = 0xFF111114;
pub const INK_BLUE: u32 = 0xFF2246A8;
pub const INK_RED: u32 = 0xFFD71921;

/// A small deterministic PRNG (splitmix64), so the demo needs no extra crates.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    /// Uniform in 0..1.
    pub fn f(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }
    pub fn range(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.f()
    }
    pub fn chance(&mut self, p: f32) -> bool {
        self.f() < p
    }
}

/// One cursive "word": a prolate-cycloid scribble with ascenders/descenders, slant, jitter
/// and pressure, `samples` points long, starting at `x0` on `baseline`.
pub fn word(rng: &mut Rng, x0: f32, baseline: f32, width: f32, xh: f32, samples: usize) -> Vec<InkPoint> {
    let letters = (width / (xh * 0.62)).max(1.0);
    // Per-letter height profile: 1 = x-height, >1 ascender, <0 descender.
    let n_letters = letters.ceil() as usize + 2;
    let profile: Vec<f32> = (0..n_letters)
        .map(|_| match rng.f() {
            p if p < 0.18 => rng.range(1.9, 2.3),
            p if p < 0.28 => -rng.range(0.8, 1.1),
            _ => rng.range(0.85, 1.1),
        })
        .collect();
    let phase0 = rng.range(0.0, TAU);
    let slant = rng.range(0.18, 0.3);
    let loop_r = xh * rng.range(0.28, 0.4);
    let mut out = Vec::with_capacity(samples);
    let (mut jx, mut jy) = (0.0f32, 0.0f32);
    for i in 0..samples {
        let t = i as f32 / (samples - 1) as f32;
        let phase = t * letters * TAU;
        let k = (phase / TAU).floor() as usize;
        let frac = (phase / TAU).fract();
        // Smoothly blend the letter heights so strokes do not jump.
        let a = profile[k.min(n_letters - 1)];
        let b = profile[(k + 1).min(n_letters - 1)];
        let s = frac * frac * (3.0 - 2.0 * frac);
        let h = a + (b - a) * s;
        let up = 0.5 - 0.5 * (phase + phase0).cos();
        let yoff = if h >= 0.0 { -xh * h * up } else { -xh * up + (-h) * xh * up * 1.6 };
        jx = jx * 0.8 + rng.range(-0.08, 0.08);
        jy = jy * 0.8 + rng.range(-0.08, 0.08);
        let y = baseline + yoff + jy;
        let x = x0 + t * width + loop_r * (phase + phase0).sin() + (baseline - y) * slant + jx;
        let pressure = (0.35 + 0.35 * (t * PI).sin() + rng.range(-0.05, 0.05)).clamp(0.05, 1.0);
        out.push(InkPoint { x, y, pressure, t_ms: i as u32 * 7 });
    }
    out
}

fn line_stroke(x0: f32, y0: f32, x1: f32, y1: f32, samples: usize, rng: &mut Rng) -> Vec<InkPoint> {
    (0..samples)
        .map(|i| {
            let t = i as f32 / (samples - 1) as f32;
            let wob = (t * PI * 3.0).sin() * 0.4 + rng.range(-0.1, 0.1);
            let (dx, dy) = (x1 - x0, y1 - y0);
            let len = (dx * dx + dy * dy).sqrt().max(1e-3);
            InkPoint { x: x0 + dx * t - dy / len * wob, y: y0 + dy * t + dx / len * wob, pressure: 0.55, t_ms: i as u32 * 6 }
        })
        .collect()
}

fn curve(x0: f32, x1: f32, f: impl Fn(f32) -> f32, samples: usize) -> Vec<InkPoint> {
    (0..samples)
        .map(|i| {
            let t = i as f32 / (samples - 1) as f32;
            let x = x0 + (x1 - x0) * t;
            InkPoint { x, y: f(t), pressure: 0.5 + 0.2 * (t * PI).sin(), t_ms: i as u32 * 6 }
        })
        .collect()
}

pub struct Stroke {
    pub brush: Brush,
    pub points: Vec<InkPoint>,
}

fn pen(color: u32, width: f32) -> Brush {
    Brush { width, thinning: 0.55, color, kind: 0 }
}

/// Fake handwriting for one page: a headline, then lines of words, sometimes a diagram.
/// Returns exactly `target` strokes (or slightly fewer if the page is full).
pub fn page_strokes(rng: &mut Rng, spec: &SheetSpec, target: usize, with_diagram: bool) -> Vec<Stroke> {
    let mut out = Vec::with_capacity(target);
    let margin_l = if spec.background == Background::Cornell { 150.0 } else { 52.0 };
    let margin_r = spec.width - 36.0;
    let line_h = match spec.background {
        Background::Grid | Background::Dots => spec.spacing,
        _ => spec.spacing,
    };
    // Dense pages use a smaller hand so 400 words fit.
    let dense = target > 300;
    let xh = if dense { 4.2 } else { 6.4 };
    let line_h = if dense { line_h * 0.7 } else { line_h };
    let mut y = 64.0 + line_h * 2.0;

    // Headline + red underline.
    let mut x = margin_l;
    for _ in 0..rng.range(2.0, 4.0) as usize {
        let w = rng.range(50.0, 90.0);
        out.push(Stroke { brush: pen(INK_BLACK, 2.6), points: word(rng, x, y, w, 10.0, 120) });
        x += w + rng.range(14.0, 20.0);
    }
    out.push(Stroke { brush: pen(INK_RED, 2.2), points: line_stroke(margin_l - 4.0, y + 8.0, x - 8.0, y + 7.0, 120, rng) });
    y += line_h * 2.0;

    let diagram_at = if with_diagram { Some(rng.range(0.3, 0.6)) } else { None };
    let mut drew_diagram = false;
    while out.len() < target && y < spec.height - 48.0 {
        // Diagram block: axes, a curve and a boxed formula.
        if let Some(at) = diagram_at
            && !drew_diagram
            && y > spec.height * at
        {
            drew_diagram = true;
            let (ox, oy, w, h) = (margin_l + 20.0, y + 150.0, 220.0, 140.0);
            out.push(Stroke { brush: pen(INK_BLACK, 1.6), points: line_stroke(ox - 10.0, oy, ox + w, oy, 120, rng) });
            out.push(Stroke { brush: pen(INK_BLACK, 1.6), points: line_stroke(ox, oy + 10.0, ox, oy - h, 120, rng) });
            out.push(Stroke { brush: pen(INK_BLUE, 2.0), points: curve(ox + 5.0, ox + w - 10.0, |t| oy - h * 0.9 * (1.0 - (2.0 * t - 1.0).powi(2)), 160) });
            out.push(Stroke { brush: pen(INK_RED, 1.8), points: curve(ox + 5.0, ox + w - 10.0, |t| oy - h * 0.45 - h * 0.35 * (t * TAU * 1.5).sin(), 160) });
            let bx = ox + w + 40.0;
            out.push(Stroke { brush: pen(INK_BLACK, 2.0), points: word(rng, bx + 12.0, oy - h * 0.55, 120.0, 8.0, 120) });
            let r = [(bx, oy - h * 0.8), (bx + 150.0, oy - h * 0.8), (bx + 150.0, oy - h * 0.35), (bx, oy - h * 0.35)];
            for i in 0..4 {
                let (a, b) = (r[i], r[(i + 1) % 4]);
                out.push(Stroke { brush: pen(INK_RED, 1.4), points: line_stroke(a.0, a.1, b.0, b.1, 40, rng) });
            }
            y += h + 60.0;
            continue;
        }
        let mut x = margin_l + if rng.chance(0.15) { 18.0 } else { 0.0 };
        while x < margin_r - 30.0 && out.len() < target {
            let w = if dense { rng.range(16.0, 44.0) } else { rng.range(18.0, 64.0) }.min(margin_r - x);
            let color = if rng.chance(0.06) { INK_BLUE } else { INK_BLACK };
            out.push(Stroke { brush: pen(color, if dense { 1.4 } else { 1.8 }), points: word(rng, x, y, w, xh, 120) });
            x += w + rng.range(6.0, 13.0);
        }
        y += line_h;
    }
    out
}

/// The demo notebook layout: (title, background, strokes, side pages right, side pages left).
struct PagePlan {
    title: String,
    bg: Background,
    strokes: usize,
    diagram: bool,
    right: Vec<Background>,
    left: Vec<Background>,
}

fn plan(title: &str, bg: Background, strokes: usize) -> PagePlan {
    PagePlan { title: title.into(), bg, strokes, diagram: false, right: vec![], left: vec![] }
}

pub struct DemoNotebook {
    pub notebook: Notebook,
    /// (sheet id, sheet) for every main and side sheet.
    pub sheets: Vec<(String, Sheet)>,
    /// The sheet id of the densest page (400 strokes) for measurements.
    pub dense_sheet: String,
}

/// Builds the demo notebook (~30 main A4 pages). Takes a few hundred ms in release builds.
pub fn notebook() -> DemoNotebook {
    let mut rng = Rng::new(0x5EED);
    let nb = Notebook::create(PEER, "Analysis I · WS 26/27").expect("notebook");
    let mut sheets = Vec::new();
    let mut dense_sheet = String::new();

    let bgs = [Background::Grid, Background::Lines, Background::Dots];
    let add = |nb: &Notebook, parent: Option<&str>, idx: usize, p: PagePlan, rng: &mut Rng, sheets: &mut Vec<(String, Sheet)>| -> String {
        let spec = SheetSpec::a4(p.bg);
        let (node, sid) = nb.add_sheet(parent, idx, &spec).expect("sheet");
        nb.set_title_of(&node, &p.title).expect("title");
        let sheet = Sheet::create(PEER).expect("sheet doc");
        for s in page_strokes(rng, &spec, p.strokes, p.diagram) {
            sheet.add_stroke(s.brush, &s.points).expect("stroke");
        }
        sheets.push((sid.clone(), sheet));
        for (i, bg) in p.right.iter().enumerate() {
            let spec = SheetSpec::a4(*bg);
            let (_, sid) = nb.add_side_sheet(&node, Side::Right, i, &spec).expect("side");
            let sheet = Sheet::create(PEER).expect("sheet doc");
            let n = 60 + (rng.f() * 120.0) as usize;
            for s in page_strokes(rng, &spec, n, i == 0) {
                sheet.add_stroke(s.brush, &s.points).expect("stroke");
            }
            sheets.push((sid, sheet));
        }
        for (i, bg) in p.left.iter().enumerate() {
            let spec = SheetSpec::a4(*bg);
            let (_, sid) = nb.add_side_sheet(&node, Side::Left, i, &spec).expect("side");
            let sheet = Sheet::create(PEER).expect("sheet doc");
            for s in page_strokes(rng, &spec, 90, false) {
                sheet.add_stroke(s.brush, &s.points).expect("stroke");
            }
            sheets.push((sid, sheet));
        }
        sid
    };

    add(&nb, None, 0, plan("Deckblatt", Background::Blank, 100), &mut rng, &mut sheets);

    let lectures = nb.add_group(None, 9, "Vorlesungen").expect("group");
    for i in 0..12 {
        let mut p = plan(&format!("V{:02} · {}", i + 1, LECTURES[i]), bgs[i % 3], 140 + (i * 23) % 220);
        p.diagram = i % 3 == 1;
        match i {
            1 => p.right = vec![Background::Grid],
            4 => p.right = vec![Background::Blank, Background::Dots],
            6 => p.left = vec![Background::Cornell],
            7 => p.strokes = 400,
            8 => p.right = vec![Background::Lines],
            _ => {}
        }
        let sid = add(&nb, Some(&lectures), i, p, &mut rng, &mut sheets);
        if i == 7 {
            dense_sheet = sid;
        }
    }

    let weekly = nb.add_group(None, 9, "Wöchentliche Aufgaben").expect("group");
    for w in 0..3 {
        let week = nb.add_group(Some(&weekly), w, &format!("Week{:02}", w + 1)).expect("group");
        for k in 0..3 {
            let p = plan(&format!("Blatt {} · Aufgabe {}", w + 1, k + 1), Background::Lines, 110 + (w * 3 + k) * 17);
            add(&nb, Some(&week), k, p, &mut rng, &mut sheets);
        }
        if w == 2 {
            nb.set_collapsed(&week, true).expect("collapse");
        }
    }

    let exam = nb.add_group(None, 9, "Klausurvorbereitung").expect("group");
    for k in 0..5 {
        let mut p = plan(&format!("Probeklausur {}", k + 1), if k % 2 == 0 { Background::Dots } else { Background::Grid }, 180 + k * 30);
        p.diagram = k == 2;
        if k == 1 {
            p.right = vec![Background::Grid];
        }
        add(&nb, Some(&exam), k, p, &mut rng, &mut sheets);
    }

    for k in 0..3 {
        add(&nb, None, 99, plan(&format!("Formelsammlung {}", k + 1), Background::Grid, 260), &mut rng, &mut sheets);
    }

    DemoNotebook { notebook: nb, sheets, dense_sheet }
}

const LECTURES: [&str; 12] = [
    "Mengen und Abbildungen",
    "Vollständige Induktion",
    "Reelle Zahlen",
    "Folgen und Grenzwerte",
    "Reihen",
    "Stetigkeit",
    "Differenzierbarkeit",
    "Mittelwertsatz",
    "Taylor-Entwicklung",
    "Integral (Riemann)",
    "Hauptsatz",
    "Uneigentliche Integrale",
];

/// The flowing text page of the demo, as (markdown-ish kind prefix, text) pairs.
pub fn text_page() -> Vec<(&'static str, &'static str)> {
    vec![
        ("#", "Mitschrift · Vorlesung 08"),
        ("", "Heute: Mittelwertsatz und seine Folgerungen. Der Satz von Rolle ist der Spezialfall f(a) = f(b); daraus folgt der allgemeine Mittelwertsatz durch Abziehen der Sekante."),
        ("##", "Satz (Mittelwertsatz)"),
        ("", "Sei f: [a, b] → ℝ stetig und auf (a, b) differenzierbar. Dann gibt es ein ξ ∈ (a, b) mit"),
        ("$$", "f'(\\xi) = \\frac{f(b) - f(a)}{b - a}"),
        ("-", "Geometrisch: Es gibt eine Tangente, die parallel zur Sekante ist."),
        ("-", "Folgerung: f' = 0 auf einem Intervall ⇒ f ist konstant."),
        ("-", "Folgerung: f' ≥ 0 ⇒ f ist monoton wachsend (Übung: Umkehrung gilt nicht streng)."),
        ("##", "Numerisch prüfen"),
        ("", "Kleines Skript für die Übung – findet ξ per Bisektion für f(x) = x³ auf [0, 2]:"),
        (
            "```rust",
            "fn xi(f: impl Fn(f64) -> f64, df: impl Fn(f64) -> f64, a: f64, b: f64) -> f64 {\n    let slope = (f(b) - f(a)) / (b - a);\n    let (mut lo, mut hi) = (a, b);\n    for _ in 0..60 {\n        let mid = 0.5 * (lo + hi);\n        if df(mid) < slope { lo = mid } else { hi = mid }\n    }\n    0.5 * (lo + hi) // ≈ 1.1547 für x³\n}",
        ),
        ("", "Ergebnis: ξ ≈ 1,1547 = 2/√3. Fragen für die Übung: Warum braucht man die Stetigkeit auf dem abgeschlossenen Intervall? Gegenbeispiel überlegen (Sprungstelle bei b)."),
        ("###", "Offene Punkte"),
        ("-", "Beweis von Rolle nochmal sauber aufschreiben (Extremwert im Inneren)."),
        ("-", "Tutorium Donnerstag: Aufgaben 3 und 4 vorrechnen – Übungsblatt liegt im Ordner „Wöchentliche Aufgaben“."),
        ("", ""),
    ]
}

/// Saves the demo notebook into a library (tests, screenshots); returns its id.
pub fn save_into(lib: &n0te_core::store::Library) -> String {
    let d = notebook();
    lib.save_notebook(&d.notebook).expect("save notebook");
    for (id, sheet) in &d.sheets {
        lib.save_sheet(&d.notebook.id(), id, sheet).expect("save sheet");
    }
    d.notebook.id()
}
