//! Coordinate systems with function graphs. A plot is an object on a page — a box, the ranges
//! it shows, its functions, what to mark — and the core works out how it looks as ink: axes,
//! ticks, numbers and names in a little hand-written stroke font, curves, hatched areas,
//! marked points. So every app and the PDF export draw it the same way, with the pen's feel
//! rather than a printout's, and it stays editable.

mod expr;
mod font;
mod points;
mod render;

pub use expr::{Curve, Expr, Func, NAMES_OF_FNS, check, fn_name, latex_to_expr, parse, parse_line, rename_refs};
pub use font::{glyph, text_width};
pub use points::{PointKind, SpecialPoint, area, special_points, zeros_of};
pub use render::{curve, nice_step, pi_label, render, tick_label};

/// How a function's line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LineStyle {
    #[default]
    Solid,
    Dashed,
    Dotted,
}

impl LineStyle {
    pub fn code(self) -> &'static str {
        match self {
            LineStyle::Solid => "",
            LineStyle::Dashed => "dash",
            LineStyle::Dotted => "dot",
        }
    }
    pub fn from_code(s: &str) -> LineStyle {
        match s {
            "dash" => LineStyle::Dashed,
            "dot" => LineStyle::Dotted,
            _ => LineStyle::Solid,
        }
    }
}

/// A hatched area under a function: over [from, to], down to the x axis or to another of the
/// plot's functions (by number).
#[derive(Debug, Clone, PartialEq)]
pub struct Area {
    pub from: f64,
    pub to: f64,
    pub to_fn: Option<usize>,
}

/// One function of a plot: what was typed (`x^2 - 1`, `f(x) = 2sin x`, `f'(x)`, `x = 2`), its
/// ink and line, and an area to hatch under it.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotFn {
    pub expr: String,
    pub color: u32,
    pub style: LineStyle,
    pub area: Option<Area>,
}

impl PlotFn {
    pub fn new(expr: &str, color: u32) -> PlotFn {
        PlotFn { expr: expr.to_string(), color, style: LineStyle::Solid, area: None }
    }
}

/// A point marked by hand: its name and where it is (plot units).
#[derive(Debug, Clone, PartialEq)]
pub struct PlotPoint {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

/// What a plot marks on its functions by itself (bits of [`Plot::marks`]).
pub const MARK_ZEROS: u8 = 1;
pub const MARK_EXTREMA: u8 = 2;
pub const MARK_CROSSINGS: u8 = 4;
pub const MARK_TURNS: u8 = 8;

/// A coordinate system on a page: its box (page points), the ranges shown, its functions and
/// what it marks and writes.
#[derive(Debug, Clone, PartialEq)]
pub struct Plot {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub x_min: f64,
    pub x_max: f64,
    pub y_min: f64,
    pub y_max: f64,
    /// Light lines at every tick.
    pub grid: bool,
    /// Ruler-straight instead of drawn by hand.
    pub clean: bool,
    /// Ink of the axes, ticks and numbers.
    pub color: u32,
    /// Pen width of the curves (axes a little thinner).
    pub width: f32,
    pub fns: Vec<PlotFn>,
    /// Each function's name (f, g, …) written at its curve.
    pub names: bool,
    /// The x axis in steps of π (π/2, π, 3π/2 …).
    pub pi: bool,
    /// The axes' names (empty: x and y).
    pub x_name: String,
    pub y_name: String,
    /// What it marks by itself ([`MARK_ZEROS`] …).
    pub marks: u8,
    /// Coordinates written at marked points, the size of hatched areas.
    pub coords: bool,
    /// Points marked by hand.
    pub points: Vec<PlotPoint>,
}

impl Plot {
    /// A plot in the box with ranges of `unit` page points per unit (equal on both axes),
    /// the origin in the middle: the plot's frame on squared paper when `unit` is the squares.
    pub fn centered(x: f32, y: f32, w: f32, h: f32, unit: f32, color: u32, width: f32) -> Plot {
        let (hx, hy) = ((w / unit / 2.0) as f64, (h / unit / 2.0) as f64);
        Plot {
            id: String::new(),
            x,
            y,
            w,
            h,
            x_min: -hx,
            x_max: hx,
            y_min: -hy,
            y_max: hy,
            grid: false,
            clean: false,
            color,
            width,
            fns: Vec::new(),
            names: false,
            pi: false,
            x_name: String::new(),
            y_name: String::new(),
            marks: 0,
            coords: false,
            points: Vec::new(),
        }
    }

    /// Whether the ranges and the box can be drawn.
    pub fn is_valid(&self) -> bool {
        let fin = |v: f64| v.is_finite();
        fin(self.x_min) && fin(self.x_max) && fin(self.y_min) && fin(self.y_max) && self.x_max > self.x_min && self.y_max > self.y_min && self.w > 1.0 && self.h > 1.0
    }

    /// Each function read, references to the others resolved (`g(x) = f'(x) + 1`): what it
    /// draws, or what is wrong with it (German).
    pub fn curves(&self) -> Vec<Result<Curve, String>> {
        let n = self.fns.len();
        let parsed: Vec<Result<Curve, String>> = self.fns.iter().map(|f| if f.expr.trim().is_empty() { Err("Leer".into()) } else { parse_line(&f.expr, n) }).collect();
        // Resolved one after the other, each the first time something needs it.
        let mut done: Vec<Option<Result<Curve, String>>> = vec![None; n];
        fn resolve(i: usize, parsed: &[Result<Curve, String>], done: &mut Vec<Option<Result<Curve, String>>>, stack: &mut Vec<usize>) -> Result<Curve, String> {
            if let Some(r) = &done[i] {
                return r.clone();
            }
            if stack.contains(&i) {
                return Err(format!("{} verweist auf sich selbst (über {})", fn_name(i), stack.iter().map(|&k| fn_name(k)).collect::<Vec<_>>().join(", ")));
            }
            let r = match &parsed[i] {
                Err(e) => Err(e.clone()),
                Ok(Curve::AtX(v)) => Ok(Curve::AtX(*v)),
                Ok(Curve::Of(e)) => {
                    let mut refs = Vec::new();
                    e.refs(&mut refs);
                    stack.push(i);
                    let mut defs: Vec<Option<Expr>> = vec![None; parsed.len()];
                    let mut err = None;
                    for &j in &refs {
                        if j >= parsed.len() {
                            err = Some(format!("Es gibt kein {}", fn_name(j)));
                            break;
                        }
                        match resolve(j, parsed, done, stack) {
                            Ok(Curve::Of(g)) => defs[j] = Some(g),
                            Ok(Curve::AtX(_)) => {
                                err = Some(format!("{} ist eine senkrechte Linie", fn_name(j)));
                                break;
                            }
                            Err(e) if e.contains("verweist auf sich") => {
                                err = Some(e);
                                break;
                            }
                            Err(_) => {
                                err = Some(format!("{} hat noch einen Fehler", fn_name(j)));
                                break;
                            }
                        }
                    }
                    stack.pop();
                    match err {
                        Some(e) => Err(e),
                        None => e.resolve(&|j| defs.get(j).cloned().flatten()).map(|r| Curve::Of(r.simplified())).map_err(|j| format!("{} hat noch einen Fehler", fn_name(j))),
                    }
                }
            };
            done[i] = Some(r.clone());
            r
        }
        let mut stack = Vec::new();
        (0..n).map(|i| resolve(i, &parsed, &mut done, &mut stack)).collect()
    }

    /// What is wrong with each function (None: fine; empty ones are fine too).
    pub fn errors(&self) -> Vec<Option<String>> {
        self.curves().into_iter().zip(&self.fns).map(|(c, f)| if f.expr.trim().is_empty() { None } else { c.err() }).collect()
    }

    /// The y range for the same units as across (keeping the middle of the current one).
    pub fn equal_units(&self) -> (f64, f64) {
        let per_pt = (self.x_max - self.x_min) / self.w as f64;
        let half = per_pt * self.h as f64 / 2.0;
        let mid = (self.y_min + self.y_max) / 2.0;
        (mid - half, mid + half)
    }

    /// A y range that shows the functions over the x range (outliers like poles left out),
    /// rounded to the ticks; None if there is nothing to show.
    pub fn fit_y(&self) -> Option<(f64, f64)> {
        let mut ys = Vec::new();
        for e in points::graphs(&self.curves()).into_iter().flatten() {
            for i in 0..=400 {
                let x = self.x_min + (self.x_max - self.x_min) * i as f64 / 400.0;
                let y = e.eval(x);
                if y.is_finite() {
                    ys.push(y);
                }
            }
        }
        for p in &self.points {
            ys.push(p.y);
        }
        if ys.is_empty() {
            return None;
        }
        ys.sort_by(f64::total_cmp);
        let at = |q: f64| ys[((ys.len() - 1) as f64 * q).round() as usize];
        let (mut lo, mut hi) = (at(0.03), at(0.97));
        // The x axis stays in view when the curves come near it.
        let span = (hi - lo).max(1e-9);
        if lo > 0.0 && lo < span * 0.6 {
            lo = 0.0;
        }
        if hi < 0.0 && -hi < span * 0.6 {
            hi = 0.0;
        }
        if hi - lo < 1e-9 {
            lo -= 1.0;
            hi += 1.0;
        }
        let pad = (hi - lo) * 0.08;
        let step = nice_step((hi - lo) * 1.16, 8.0);
        Some((((lo - pad) / step).floor() * step, ((hi + pad) / step).ceil() * step))
    }

    /// The plot without its function number `i`: the ones after it move up a name (h becomes
    /// g), references to them follow, areas down to it go down to the x axis.
    pub fn remove_fn(&self, i: usize) -> Plot {
        let mut p = self.clone();
        if i >= p.fns.len() {
            return p;
        }
        let n = p.fns.len();
        p.fns.remove(i);
        let map = |k: usize| (k > i).then(|| k - 1);
        for f in &mut p.fns {
            f.expr = rename_refs(&f.expr, n, &map);
            if let Some(a) = &mut f.area {
                a.to_fn = match a.to_fn {
                    Some(j) if j == i => None,
                    Some(j) if j > i => Some(j - 1),
                    other => other,
                };
            }
        }
        p
    }

    /// The x range for steps of π: the current one widened to whole π/2.
    pub fn pi_range(&self) -> (f64, f64) {
        let h = std::f64::consts::FRAC_PI_2;
        ((self.x_min / h).floor() * h, (self.x_max / h).ceil() * h)
    }
}

/// A number as written: German decimal comma, at most `places` places, no trailing zeros.
pub fn format_num(v: f64, places: usize) -> String {
    let v = if v.abs() < 0.5 * 10f64.powi(-(places as i32)) { 0.0 } else { v };
    let s = format!("{v:.places$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    s.replace('.', ",")
}

/// Points typed by hand: `A(1|2); B(-1|0,5)` (also `A(1;2)`, `P = (1|2)`, names optional).
pub fn parse_points(src: &str) -> Result<Vec<PlotPoint>, String> {
    let mut out = Vec::new();
    let mut rest = src.trim();
    let mut k = 0;
    while !rest.is_empty() {
        let open = rest.find('(').ok_or_else(|| format!("Punkt „{}“: so schreiben: A(1|2)", rest.chars().take(12).collect::<String>()))?;
        let close = rest[open..].find(')').map(|c| c + open).ok_or("Eine Klammer „)“ fehlt bei einem Punkt")?;
        let name: String = rest[..open].trim().trim_end_matches('=').trim().trim_start_matches([';', ',']).trim().to_string();
        let inner = &rest[open + 1..close];
        let parts: Vec<&str> = inner.split(['|', ';']).collect();
        if parts.len() != 2 {
            return Err(format!("Punkt {}: x und y mit | trennen, z. B. (1|2)", if name.is_empty() { "?" } else { &name }));
        }
        let num = |s: &str| -> Result<f64, String> {
            match parse_line(s, 0) {
                Ok(Curve::Of(e)) if !e.has_x() => Ok(e.eval(0.0)),
                _ => Err(format!("„{}“ ist keine Zahl", s.trim())),
            }
        };
        k += 1;
        out.push(PlotPoint { name: if name.is_empty() { format!("P{k}") } else { name }, x: num(parts[0])?, y: num(parts[1])? });
        rest = rest[close + 1..].trim().trim_start_matches([';', ',']).trim();
    }
    Ok(out)
}

/// Points as they are typed: `A(1|2); B(-1|0,5)`.
pub fn format_points(points: &[PlotPoint]) -> String {
    points.iter().map(|p| format!("{}({}|{})", p.name, format_num(p.x, 4), format_num(p.y, 4))).collect::<Vec<_>>().join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plot(fns: &[&str]) -> Plot {
        let mut p = Plot::centered(100.0, 100.0, 240.0, 160.0, 20.0, 0xFF22_2222, 1.2);
        p.fns = fns.iter().map(|f| PlotFn::new(f, 0xFFD0_2020)).collect();
        p
    }

    #[test]
    fn functions_use_each_other_and_say_whats_wrong() {
        let p = plot(&["x^2", "f'(x) + 1", "g(x) - f(x)", "x = 2", "k(x)", ""]);
        let c = p.curves();
        let val = |i: usize, x: f64| match &c[i] {
            Ok(Curve::Of(e)) => e.eval(x),
            other => panic!("{i}: {other:?}"),
        };
        assert_eq!(val(1, 3.0), 7.0);
        assert_eq!(val(2, 3.0), 7.0 - 9.0);
        assert_eq!(c[3], Ok(Curve::AtX(2.0)));
        // k is the 4th: the vertical line; it can't be used as a function.
        assert_eq!(c[4], Err("k ist eine senkrechte Linie".into()));
        assert_eq!(p.errors()[5], None, "an empty line is no mistake");
        let loop_ = plot(&["g(x)", "f(x) + 1"]);
        assert!(loop_.errors()[0].as_ref().unwrap().contains("verweist auf sich"), "{:?}", loop_.errors());
        let broken = plot(&["x +", "f(x)"]);
        assert_eq!(broken.errors()[1], Some("f hat noch einen Fehler".into()));
    }

    #[test]
    fn removing_a_function_keeps_the_others_right() {
        let mut p = plot(&["x^2", "sin x", "f'(x) + k(x)", "2x"]);
        p.fns[2].area = Some(Area { from: 0.0, to: 1.0, to_fn: Some(3) });
        p.fns[3].area = Some(Area { from: 0.0, to: 1.0, to_fn: Some(1) });
        let q = p.remove_fn(1);
        assert_eq!(q.fns.iter().map(|f| f.expr.as_str()).collect::<Vec<_>>(), vec!["x^2", "f'(x) + h(x)", "2x"]);
        assert_eq!(q.fns[1].area.as_ref().unwrap().to_fn, Some(2));
        assert_eq!(q.fns[2].area.as_ref().unwrap().to_fn, None, "the area down to the removed one goes to the axis");
        assert!(q.errors().iter().all(|e| e.is_none()), "{:?}", q.errors());
    }

    #[test]
    fn ranges_fit_the_functions() {
        let mut p = plot(&["x^2"]);
        let (lo, hi) = p.fit_y().unwrap();
        assert!(lo <= 0.0 && lo >= -5.0 && hi >= 30.0 && hi <= 40.0, "{lo} {hi}");
        p.fns = vec![PlotFn::new("1/x", 0)];
        let (lo, hi) = p.fit_y().unwrap();
        assert!(lo > -20.0 && hi < 20.0, "{lo} {hi}");
        p.fns.clear();
        assert!(p.fit_y().is_none());
        let mut p = plot(&[]);
        p.h = 120.0;
        assert_eq!(p.equal_units(), (-3.0, 3.0));
        let (a, b) = Plot { x_min: -5.0, x_max: 7.0, ..plot(&[]) }.pi_range();
        assert!((a + 2.0 * std::f64::consts::PI).abs() < 1e-12 && (b - 2.5 * std::f64::consts::PI).abs() < 1e-12);
    }

    #[test]
    fn points_typed_by_hand() {
        let pts = parse_points("A(1|2); B(-1|0,5) C = (pi;-3)").unwrap();
        assert_eq!(pts.len(), 3);
        assert_eq!((pts[1].name.as_str(), pts[1].x, pts[1].y), ("B", -1.0, 0.5));
        assert!((pts[2].x - std::f64::consts::PI).abs() < 1e-12 && pts[2].name == "C");
        assert_eq!(parse_points("(2|3)").unwrap()[0].name, "P1");
        assert!(parse_points("A(1,2)").is_err());
        assert!(parse_points("A(1|x)").is_err());
        assert_eq!(format_points(&pts[..2]), "A(1|2); B(-1|0,5)");
        assert_eq!(parse_points("").unwrap(), vec![]);
        assert_eq!(format_num(2.0 / 3.0, 2), "0,67");
        assert_eq!(format_num(-0.001, 2), "0");
    }
}
