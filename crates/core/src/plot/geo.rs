//! A construction, as in GeoGebra: objects defined by what was typed (`A = (1, 2)`,
//! `f(x) = x^2`, `g = Gerade(A, B)`, `S = Schneide(f, g)`), each with its style, worked out in
//! the order they depend on each other. Free points and sliders hold their value in their
//! definition, so dragging one rewrites it and everything that depends on it follows.

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;

use super::LineStyle;
use super::expr::{Cmp, Expr, Func};
use super::lang::{self, Body, Input, Node, Op};
use super::points::zeros_of;

pub type Pt = (f64, f64);

/// What an object is, once worked out.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Num(f64),
    Bool(bool),
    Point(Pt),
    /// Several points (Schneide, Nullstelle …).
    Points(Vec<Pt>),
    /// An arrow from `from` by `d`.
    Vector { from: Pt, d: Pt },
    /// y = f(x) (or of t).
    Func(Expr),
    /// a·x + b·y = c.
    Line { a: f64, b: f64, c: f64 },
    /// Several lines (two tangents from a point to a circle).
    Lines(Vec<(f64, f64, f64)>),
    Segment(Pt, Pt),
    Ray(Pt, Pt),
    Circle(Pt, f64),
    Polygon(Vec<Pt>),
    /// (x(t), y(t)) for t in [t0, t1] (t is the expression's x).
    Curve(Expr, Expr, f64, f64),
    /// F(x, y) = 0.
    Implicit(Expr),
    /// At `at`, from the direction `from` (radians) turning by `sweep` counter-clockwise.
    Angle { at: Pt, from: f64, sweep: f64 },
    /// ∫ (f − g) over [a, b] with its value; hatched between them.
    Area { f: Expr, g: Option<Expr>, a: f64, b: f64, value: f64 },
    Text(String),
}

/// The kind of an object (for names, styles, the algebra view).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Number,
    Bool,
    Point,
    Points,
    Vector,
    Function,
    Line,
    Lines,
    Segment,
    Ray,
    Circle,
    Polygon,
    Curve,
    Implicit,
    Angle,
    Area,
    Text,
}

impl Val {
    pub fn kind(&self) -> Kind {
        match self {
            Val::Num(_) => Kind::Number,
            Val::Bool(_) => Kind::Bool,
            Val::Point(_) => Kind::Point,
            Val::Points(_) => Kind::Points,
            Val::Vector { .. } => Kind::Vector,
            Val::Func(_) => Kind::Function,
            Val::Line { .. } => Kind::Line,
            Val::Lines(_) => Kind::Lines,
            Val::Segment(..) => Kind::Segment,
            Val::Ray(..) => Kind::Ray,
            Val::Circle(..) => Kind::Circle,
            Val::Polygon(_) => Kind::Polygon,
            Val::Curve(..) => Kind::Curve,
            Val::Implicit(_) => Kind::Implicit,
            Val::Angle { .. } => Kind::Angle,
            Val::Area { .. } => Kind::Area,
            Val::Text(_) => Kind::Text,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PointStyle {
    #[default]
    Dot,
    Cross,
    Ring,
}

/// What an object's label says (GeoGebra's "Beschriftung anzeigen").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelMode {
    #[default]
    Off,
    Name,
    /// Name and value: "f(x) = x² − 1", "A = (1 | 2)" — the formula.
    NameValue,
    Value,
    Caption,
}

/// A slider's range and step (a number shown as a slider).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Slider {
    pub min: f64,
    pub max: f64,
    pub step: f64,
}

/// How an object is drawn and labelled.
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub color: u32,
    /// Pen width (0: the plot's).
    pub width: f32,
    pub line: LineStyle,
    pub point: PointStyle,
    /// Point size in points (0: the default).
    pub size: f32,
    /// Fill of circles, polygons, areas: 0 (none) … 1 (dense hatching).
    pub fill: f32,
    pub label: LabelMode,
    pub caption: String,
    pub visible: bool,
    /// A helper (GeoGebra's "Hilfsobjekt"): not listed first, drawn only when visible.
    pub aux: bool,
    /// Can't be dragged.
    pub fixed: bool,
    /// A number shown as a slider.
    pub slider: Option<Slider>,
    /// Functions: zeros, highs and lows … marked by themselves (MARK_* bits).
    pub special: u8,
    /// Coordinates at those points.
    pub coords: bool,
    /// Texts and sliders: where in the box (0…1 across and down).
    pub at: Option<(f32, f32)>,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            color: 0xFF20_2428,
            width: 0.0,
            line: LineStyle::Solid,
            point: PointStyle::Dot,
            size: 0.0,
            fill: 0.0,
            label: LabelMode::Off,
            caption: String::new(),
            visible: true,
            aux: false,
            fixed: false,
            slider: None,
            special: 0,
            coords: false,
            at: None,
        }
    }
}

/// One object of a construction: its definition as typed (always with its name in front),
/// its style, where it stands in the algebra view.
#[derive(Debug, Clone, PartialEq)]
pub struct Obj {
    pub id: String,
    pub def: String,
    pub style: Style,
    pub order: f64,
}

impl Obj {
    /// The object's name: what stands before "(x) =", "=" or ":".
    pub fn name(&self) -> String {
        name_of(&self.def).unwrap_or_default()
    }
}

/// The name a definition gives (None: it gives none).
pub fn name_of(def: &str) -> Option<String> {
    let d = def.trim();
    let colon = d.find(':');
    let eq = d.find('=');
    let cut = match (colon, eq) {
        (Some(c), Some(e)) => c.min(e),
        (Some(c), None) => c,
        (None, Some(e)) => e,
        (None, None) => return None,
    };
    let head = d[..cut].trim();
    let head = head.split('(').next().unwrap_or("").trim();
    let ok = head.chars().next().is_some_and(|c| c.is_alphabetic()) && head.chars().all(|c| c.is_alphanumeric() || c == '_');
    (ok && !matches!(head, "x" | "y")).then(|| head.to_string())
}

/// The definition with its name changed to `to`.
pub fn renamed(def: &str, to: &str) -> String {
    match name_of(def) {
        Some(n) => {
            let at = def.find(&n).unwrap_or(0);
            format!("{}{}{}", &def[..at], to, &def[at + n.len()..])
        }
        None => format!("{to} = {}", def.trim()),
    }
}

/// The plot's view as the construction sees it (for commands that look over it).
#[derive(Debug, Clone, Copy)]
pub struct ViewRange {
    pub x0: f64,
    pub x1: f64,
    pub y0: f64,
    pub y1: f64,
}

/// Works out every object, in the order they depend on each other: its value or what is wrong
/// (German). Results are in the objects' order.
pub fn evaluate(objs: &[Obj], view: ViewRange) -> Vec<Result<Val, String>> {
    let names: Vec<String> = objs.iter().map(|o| o.name()).collect();
    let known: HashSet<String> = names.iter().filter(|n| !n.is_empty()).cloned().collect();
    let mut index: HashMap<String, usize> = HashMap::new();
    for (i, n) in names.iter().enumerate() {
        if n.is_empty() {
            continue;
        }
        index.entry(n.clone()).or_insert(i);
    }
    let parsed: Vec<Result<Input, String>> = objs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            if names[i].is_empty() {
                return Err("Ohne Namen".into());
            }
            if index.get(&names[i]) != Some(&i) {
                return Err(format!("Den Namen {} gibt es schon", names[i]));
            }
            lang::parse_input(&o.def, &known)
        })
        .collect();
    let mut out: Vec<Option<Result<Val, String>>> = vec![None; objs.len()];
    let mut env = Env { vals: HashMap::new(), view };
    // Depth-first in dependency order; a loop is a mistake for everyone in it.
    fn visit(i: usize, objs: &[Obj], names: &[String], parsed: &[Result<Input, String>], index: &HashMap<String, usize>, out: &mut Vec<Option<Result<Val, String>>>, env: &mut Env, stack: &mut Vec<usize>) {
        if out[i].is_some() {
            return;
        }
        if stack.contains(&i) {
            out[i] = Some(Err(format!("{} hängt von sich selbst ab", names[i])));
            return;
        }
        let r = match &parsed[i] {
            Err(e) => Err(e.clone()),
            Ok(input) => {
                let mut deps = Vec::new();
                refs_of(input, &mut deps);
                stack.push(i);
                let mut bad = None;
                for d in deps {
                    if d == names[i] {
                        continue;
                    }
                    if let Some(&j) = index.get(&d) {
                        visit(j, objs, names, parsed, index, out, env, stack);
                        if let Some(Err(e)) = &out[j] {
                            bad = Some(if e.contains("hängt von sich selbst ab") { e.clone() } else { format!("{} hat noch einen Fehler", names[j]) });
                        }
                    }
                }
                stack.pop();
                match bad {
                    Some(e) => Err(e),
                    None => env.define(input, &objs[i].style),
                }
            }
        };
        if let Ok(v) = &r {
            env.vals.insert(names[i].clone(), v.clone());
        }
        out[i] = Some(r);
    }
    let mut stack = Vec::new();
    for i in 0..objs.len() {
        visit(i, objs, &names, &parsed, &index, &mut out, &mut env, &mut stack);
    }
    out.into_iter().map(|r| r.unwrap_or_else(|| Err("?".into()))).collect()
}

/// The names an input refers to.
fn refs_of(input: &Input, out: &mut Vec<String>) {
    match &input.body {
        Body::Expr(n) => node_refs(n, out),
        Body::Equation(l, r) => {
            node_refs(l, out);
            node_refs(r, out);
        }
    }
}

fn node_refs(n: &Node, out: &mut Vec<String>) {
    match n {
        Node::Name(s) => out.push(s.clone()),
        Node::Call(s, _, args) => {
            out.push(s.clone());
            args.iter().for_each(|a| node_refs(a, out));
        }
        Node::Neg(a) => node_refs(a, out),
        Node::Bin(_, a, b) | Node::Point(a, b) => {
            node_refs(a, out);
            node_refs(b, out);
        }
        Node::Chain(items, _) | Node::List(items) => items.iter().for_each(|a| node_refs(a, out)),
        Node::Num(_) | Node::Var(_) | Node::Text(_) => {}
    }
}

struct Env {
    vals: HashMap<String, Val>,
    view: ViewRange,
}

fn bx(e: Expr) -> Box<Expr> {
    Box::new(e)
}

fn func_of(name: &str) -> Option<Func> {
    Some(match name.to_lowercase().as_str() {
        "sin" => Func::Sin,
        "cos" => Func::Cos,
        "tan" => Func::Tan,
        "cot" => Func::Cot,
        "asin" | "arcsin" => Func::Asin,
        "acos" | "arccos" => Func::Acos,
        "atan" | "arctan" => Func::Atan,
        "sinh" => Func::Sinh,
        "cosh" => Func::Cosh,
        "tanh" => Func::Tanh,
        "exp" => Func::Exp,
        "ln" => Func::Ln,
        "log" | "lg" => Func::Log,
        "sqrt" | "wurzel" => Func::Sqrt,
        "abs" | "betrag" => Func::Abs,
        "sgn" | "sign" => Func::Sgn,
        "floor" => Func::Floor,
        "ceil" => Func::Ceil,
        "round" => Func::Round,
        _ => return None,
    })
}

/// Whether a node depends on x, y or t (outside commands, which look after themselves).
fn free(n: &Node) -> bool {
    match n {
        Node::Var(_) => true,
        Node::Num(_) | Node::Name(_) | Node::Text(_) => false,
        Node::Neg(a) => free(a),
        Node::Bin(_, a, b) | Node::Point(a, b) => free(a) || free(b),
        Node::Chain(items, _) | Node::List(items) => items.iter().any(free),
        Node::Call(name, _, args) => (lang::command(name).is_none() || name.eq_ignore_ascii_case("wenn") || name.eq_ignore_ascii_case("if")) && args.iter().any(free),
    }
}

fn num(v: &Val) -> Result<f64, String> {
    match v {
        Val::Num(x) => Ok(*x),
        Val::Angle { sweep, .. } => Ok(*sweep),
        Val::Area { value, .. } => Ok(*value),
        _ => Err("Hier gehört eine Zahl hin".into()),
    }
}

fn pt(v: &Val) -> Result<Pt, String> {
    match v {
        Val::Point(p) => Ok(*p),
        Val::Points(ps) if ps.len() == 1 => Ok(ps[0]),
        Val::Points(ps) if ps.is_empty() => Err("Diesen Punkt gibt es nicht".into()),
        _ => Err("Hier gehört ein Punkt hin".into()),
    }
}

fn line_of(v: &Val) -> Option<(f64, f64, f64)> {
    match v {
        Val::Line { a, b, c } => Some((*a, *b, *c)),
        Val::Segment(p, q) | Val::Ray(p, q) => Some(through(*p, *q)),
        Val::Vector { from, d } => Some(through(*from, (from.0 + d.0, from.1 + d.1))),
        _ => None,
    }
}

/// The line through p and q: a·x + b·y = c.
fn through(p: Pt, q: Pt) -> (f64, f64, f64) {
    let (a, b) = (q.1 - p.1, p.0 - q.0);
    (a, b, a * p.0 + b * p.1)
}

fn meet(l: (f64, f64, f64), m: (f64, f64, f64)) -> Option<Pt> {
    let det = l.0 * m.1 - l.1 * m.0;
    if det.abs() < 1e-12 {
        return None;
    }
    Some(((l.2 * m.1 - l.1 * m.2) / det, (l.0 * m.2 - l.2 * m.0) / det))
}

fn line_circle(l: (f64, f64, f64), c: Pt, r: f64) -> Vec<Pt> {
    let (a, b, k) = l;
    let n = a.hypot(b);
    if n < 1e-12 {
        return vec![];
    }
    let d = (a * c.0 + b * c.1 - k) / n;
    if d.abs() > r + 1e-12 {
        return vec![];
    }
    let foot = (c.0 - a / n * d, c.1 - b / n * d);
    let h = (r * r - d * d).max(0.0).sqrt();
    let dir = (-b / n, a / n);
    if h < 1e-12 {
        return vec![foot];
    }
    vec![(foot.0 - dir.0 * h, foot.1 - dir.1 * h), (foot.0 + dir.0 * h, foot.1 + dir.1 * h)]
}

fn circle_circle(c1: Pt, r1: f64, c2: Pt, r2: f64) -> Vec<Pt> {
    let d = (c2.0 - c1.0).hypot(c2.1 - c1.1);
    if d < 1e-12 || d > r1 + r2 + 1e-12 || d < (r1 - r2).abs() - 1e-12 {
        return vec![];
    }
    let a = (r1 * r1 - r2 * r2 + d * d) / (2.0 * d);
    let h = (r1 * r1 - a * a).max(0.0).sqrt();
    let m = (c1.0 + a * (c2.0 - c1.0) / d, c1.1 + a * (c2.1 - c1.1) / d);
    if h < 1e-12 {
        return vec![m];
    }
    let off = (-(c2.1 - c1.1) / d * h, (c2.0 - c1.0) / d * h);
    vec![(m.0 + off.0, m.1 + off.1), (m.0 - off.0, m.1 - off.1)]
}

/// Whether p (on the line of the object) lies on the segment or ray itself.
fn on_part(v: &Val, p: Pt) -> bool {
    let along = |a: Pt, b: Pt| {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let l2 = dx * dx + dy * dy;
        if l2 < 1e-24 { 0.0 } else { ((p.0 - a.0) * dx + (p.1 - a.1) * dy) / l2 }
    };
    match v {
        Val::Segment(a, b) => (-1e-9..=1.0 + 1e-9).contains(&along(*a, *b)),
        Val::Ray(a, b) => along(*a, *b) >= -1e-9,
        Val::Vector { from, d } => (-1e-9..=1.0 + 1e-9).contains(&along(*from, (from.0 + d.0, from.1 + d.1))),
        _ => true,
    }
}

/// The circle through three points.
fn circumcircle(a: Pt, b: Pt, c: Pt) -> Option<(Pt, f64)> {
    let d = 2.0 * (a.0 * (b.1 - c.1) + b.0 * (c.1 - a.1) + c.0 * (a.1 - b.1));
    if d.abs() < 1e-12 {
        return None;
    }
    let s = |p: Pt| p.0 * p.0 + p.1 * p.1;
    let ux = (s(a) * (b.1 - c.1) + s(b) * (c.1 - a.1) + s(c) * (a.1 - b.1)) / d;
    let uy = (s(a) * (c.0 - b.0) + s(b) * (a.0 - c.0) + s(c) * (b.0 - a.0)) / d;
    Some(((ux, uy), (a.0 - ux).hypot(a.1 - uy)))
}

/// The value of a function's expression where it is finite; None elsewhere.
fn at(f: &Expr, x: f64) -> Option<f64> {
    let y = f.eval(x);
    y.is_finite().then_some(y)
}

fn reflect_pt(p: Pt, over: &Val) -> Result<Pt, String> {
    match over {
        Val::Point(c) => Ok((2.0 * c.0 - p.0, 2.0 * c.1 - p.1)),
        _ => {
            let (a, b, c) = line_of(over).ok_or("Spiegeln an einem Punkt oder einer Geraden")?;
            let n2 = a * a + b * b;
            let d = (a * p.0 + b * p.1 - c) / n2;
            Ok((p.0 - 2.0 * a * d, p.1 - 2.0 * b * d))
        }
    }
}

/// `v` with every point moved by `f` (reflect, translate, rotate).
fn map_val(v: &Val, f: &dyn Fn(Pt) -> Pt) -> Result<Val, String> {
    Ok(match v {
        Val::Point(p) => Val::Point(f(*p)),
        Val::Points(ps) => Val::Points(ps.iter().map(|p| f(*p)).collect()),
        Val::Segment(p, q) => Val::Segment(f(*p), f(*q)),
        Val::Ray(p, q) => Val::Ray(f(*p), f(*q)),
        Val::Polygon(ps) => Val::Polygon(ps.iter().map(|p| f(*p)).collect()),
        Val::Circle(c, r) => {
            let (c2, e) = (f(*c), f((c.0 + r, c.1)));
            Val::Circle(c2, (e.0 - c2.0).hypot(e.1 - c2.1))
        }
        Val::Vector { from, d } => {
            let (a, b) = (f(*from), f((from.0 + d.0, from.1 + d.1)));
            Val::Vector { from: a, d: (b.0 - a.0, b.1 - a.1) }
        }
        Val::Line { a, b, c } => {
            // Two points of the line, mapped.
            let (p, dir) = if b.abs() > 1e-12 { ((0.0, c / b), (b, -a)) } else { ((c / a, 0.0), (b, -a)) };
            let (p2, q2) = (f(p), f((p.0 + dir.0, p.1 + dir.1)));
            let (a, b, c) = through(p2, q2);
            Val::Line { a, b, c }
        }
        _ => return Err("Das lässt sich (noch) nicht abbilden".into()),
    })
}

impl Env {
    fn get(&self, name: &str) -> Result<Val, String> {
        if let Some(v) = self.vals.get(name) {
            return Ok(v.clone());
        }
        match name {
            "pi" => Ok(Val::Num(PI)),
            "e" => Ok(Val::Num(std::f64::consts::E)),
            _ => Err(format!("„{name}“ gibt es nicht")),
        }
    }

    /// What a line of input defines.
    fn define(&mut self, input: &Input, style: &Style) -> Result<Val, String> {
        let _ = style;
        if input.param.is_some() {
            let Body::Expr(n) = &input.body else { return Err("Eine Funktion braucht rechts einen Ausdruck".into()) };
            return Ok(Val::Func(self.fexpr(n)?.simplified()));
        }
        match &input.body {
            Body::Expr(n) => self.eval(n),
            Body::Equation(l, r) => self.equation(l, r),
        }
    }

    /// An equation in x and y: a line, y = f(x), x = c, or any curve.
    fn equation(&mut self, l: &Node, r: &Node) -> Result<Val, String> {
        let (le, re) = (self.fexpr(l)?, self.fexpr(r)?);
        let f = Expr::Sub(bx(le.clone()), bx(re.clone())).simplified();
        // Linear in x and y: a line.
        let v = |x: f64, y: f64| f.eval2(x, y);
        let f0 = v(0.0, 0.0);
        let (a, b) = (v(1.0, 0.0) - f0, v(0.0, 1.0) - f0);
        let linear = [(2.0, 3.0), (-1.7, 0.4), (5.0, -2.5), (0.3, 7.0)].iter().all(|&(x, y)| {
            let w = v(x, y);
            w.is_finite() && (w - (a * x + b * y + f0)).abs() <= 1e-9 * (1.0 + w.abs() + f0.abs())
        });
        if linear && f0.is_finite() && (a.abs() > 1e-12 || b.abs() > 1e-12) {
            return Ok(Val::Line { a, b, c: -f0 });
        }
        // x² + y² + Dx + Ey + F = 0 (times a number): a circle.
        if let Some((c, r)) = circle_of(&f) {
            return Ok(Val::Circle(c, r));
        }
        // y = f(x): a function.
        if matches!(l, Node::Var('y')) && !re.has_y() {
            return Ok(Val::Func(re.simplified()));
        }
        if !f.has_x() && !f.has_y() {
            return Err("Diese Gleichung hat kein x und kein y".into());
        }
        Ok(Val::Implicit(f))
    }

    /// A node as an expression in x (and y): numbers and functions of the construction put in.
    fn fexpr(&mut self, n: &Node) -> Result<Expr, String> {
        Ok(match n {
            Node::Num(v) => Expr::Num(*v),
            Node::Var('y') => Expr::Y,
            Node::Var(_) => Expr::X,
            Node::Name(s) => match self.get(s)? {
                Val::Func(e) => e,
                Val::Num(v) => Expr::Num(v),
                Val::Angle { sweep, .. } => Expr::Num(sweep),
                Val::Area { value, .. } => Expr::Num(value),
                _ => return Err(format!("{s} ist keine Zahl und keine Funktion")),
            },
            Node::Neg(a) => Expr::Neg(bx(self.fexpr(a)?)),
            Node::Bin(op, a, c) => {
                let (a, c) = (bx(self.fexpr(a)?), bx(self.fexpr(c)?));
                match op {
                    Op::Add => Expr::Add(a, c),
                    Op::Sub => Expr::Sub(a, c),
                    Op::Mul => Expr::Mul(a, c),
                    Op::Div => Expr::Div(a, c),
                    Op::Pow => Expr::Pow(a, c),
                }
            }
            Node::Call(name, primes, args) => {
                let lower = name.to_lowercase();
                // x(A), y(A): a point's coordinates; else x·(…).
                if (lower == "x" || lower == "y") && args.len() == 1 {
                    let probe = if free(&args[0]) { Err(String::new()) } else { self.eval(&args[0]) };
                    if let Ok(Val::Point(p)) = probe {
                        return Ok(Expr::Num(if lower == "x" { p.0 } else { p.1 }));
                    }
                    let v = if lower == "x" { Expr::X } else { Expr::Y };
                    return Ok(Expr::Mul(bx(v), bx(self.fexpr(&args[0])?)));
                }
                if lower == "wenn" || lower == "if" {
                    if args.len() < 2 {
                        return Err("Wenn(Bedingung, dann, sonst)".into());
                    }
                    let (items, ops) = self.condition(&args[0])?;
                    let then = self.fexpr(&args[1])?;
                    let other = if args.len() > 2 { self.fexpr(&args[2])? } else { Expr::Num(f64::NAN) };
                    return Ok(Expr::If(items, ops, bx(then), bx(other)));
                }
                if let Some(Val::Func(f)) = self.vals.get(name).cloned() {
                    let mut f = f;
                    for _ in 0..*primes {
                        f = f.derive();
                    }
                    let arg = args.first().map(|a| self.fexpr(a)).transpose()?.unwrap_or(Expr::X);
                    return Ok(f.at(&arg));
                }
                if let Some(func) = func_of(name) {
                    let a = args.first().ok_or("Hier fehlt etwas in der Klammer")?;
                    return Ok(Expr::Call(func, bx(self.fexpr(a)?)));
                }
                if let Some(Val::Num(v)) = self.vals.get(name).cloned() {
                    // a(x + 1): a times the bracket.
                    let arg = args.first().ok_or("Hier fehlt etwas in der Klammer")?;
                    return Ok(Expr::Mul(bx(Expr::Num(v)), bx(self.fexpr(arg)?)));
                }
                match self.eval(n)? {
                    Val::Func(e) => e,
                    Val::Num(v) => Expr::Num(v),
                    other => return Err(format!("{name}(…) ergibt hier keine Zahl ({:?})", other.kind())),
                }
            }
            Node::Point(..) => return Err("Ein Punkt kann kein Teil einer Funktion sein".into()),
            Node::Chain(..) => return Err("Ein Vergleich gehört in Wenn(…)".into()),
            Node::Text(_) => return Err("Ein Text kann kein Teil einer Funktion sein".into()),
            Node::List(_) => return Err("Eine Liste kann kein Teil einer Funktion sein".into()),
        })
    }

    fn condition(&mut self, n: &Node) -> Result<(Vec<Expr>, Vec<Cmp>), String> {
        match n {
            Node::Chain(items, ops) => Ok((items.iter().map(|i| self.fexpr(i)).collect::<Result<_, _>>()?, ops.clone())),
            _ => Err("Die Bedingung braucht einen Vergleich (x < 2)".into()),
        }
    }

    fn eval(&mut self, n: &Node) -> Result<Val, String> {
        if let Node::Call(name, primes, args) = n
            && let Some(cmd) = lang::command(name)
            && !self.vals.contains_key(name)
            && func_of(name).is_none()
        {
            return self.command(cmd, *primes, args);
        }
        if free(n) {
            if matches!(n, Node::Chain(..)) {
                return Err("Ein Vergleich gehört in Wenn(…)".into());
            }
            return Ok(Val::Func(self.fexpr(n)?.simplified()));
        }
        Ok(match n {
            Node::Num(v) => Val::Num(*v),
            Node::Var(_) => unreachable!("free"),
            Node::Name(s) => self.get(s)?,
            Node::Text(t) => Val::Text(t.clone()),
            Node::Neg(a) => arith(Op::Mul, Val::Num(-1.0), self.eval(a)?)?,
            Node::Bin(op, a, c) => {
                let (a, c) = (self.eval(a)?, self.eval(c)?);
                arith(*op, a, c)?
            }
            Node::Point(a, c) => Val::Point((num(&self.eval(a)?)?, num(&self.eval(c)?)?)),
            Node::Chain(items, ops) => {
                let vals: Vec<f64> = items.iter().map(|i| self.eval(i).and_then(|v| num(&v))).collect::<Result<_, _>>()?;
                Val::Bool(ops.iter().enumerate().all(|(i, op)| op.holds(vals[i], vals[i + 1])))
            }
            Node::List(items) => {
                let mut ps = Vec::new();
                for i in items {
                    ps.push(pt(&self.eval(i)?)?);
                }
                Val::Points(ps)
            }
            Node::Call(name, primes, args) => {
                let lower = name.to_lowercase();
                if (lower == "x" || lower == "y") && args.len() == 1 {
                    let p = pt(&self.eval(&args[0])?)?;
                    return Ok(Val::Num(if lower == "x" { p.0 } else { p.1 }));
                }
                match self.vals.get(name).cloned() {
                    Some(Val::Func(f)) => {
                        let mut f = f;
                        for _ in 0..*primes {
                            f = f.derive();
                        }
                        let x = num(&self.eval(args.first().ok_or("f(…) braucht eine Zahl")?)?)?;
                        Val::Num(f.eval(x))
                    }
                    Some(Val::Num(v)) => {
                        let a = self.eval(args.first().ok_or("Hier fehlt etwas in der Klammer")?)?;
                        arith(Op::Mul, Val::Num(v), a)?
                    }
                    Some(_) => return Err(format!("{name} ist keine Funktion")),
                    None => match func_of(name) {
                        Some(func) => {
                            let x = num(&self.eval(args.first().ok_or("Hier fehlt etwas in der Klammer")?)?)?;
                            Val::Num(Expr::Call(func, bx(Expr::Num(x))).eval(0.0))
                        }
                        None => return Err(format!("„{name}“ kenne ich nicht")),
                    },
                }
            }
        })
    }

    fn vals(&mut self, args: &[Node]) -> Result<Vec<Val>, String> {
        args.iter().map(|a| self.eval(a)).collect()
    }

    fn func(&mut self, n: &Node) -> Result<Expr, String> {
        match self.eval(n)? {
            Val::Func(f) => Ok(f),
            Val::Num(v) => Ok(Expr::Num(v)),
            Val::Line { a, b, c } if b.abs() > 1e-12 => Ok(Expr::Add(bx(Expr::Mul(bx(Expr::Num(-a / b)), bx(Expr::X))), bx(Expr::Num(c / b)))),
            _ => Err("Hier gehört eine Funktion hin".into()),
        }
    }

    /// A command (its English name), with its arguments as typed.
    fn command(&mut self, cmd: &str, primes: u8, args: &[Node]) -> Result<Val, String> {
        let _ = primes;
        let n = args.len();
        let wrong = |usage: &str| Err::<Val, String>(format!("So: {usage}"));
        let view = self.view;
        let span = (view.x1 - view.x0).abs().max(1e-9);
        let (lo, hi) = (view.x0 - span * 0.5, view.x1 + span * 0.5);
        match cmd {
            "point" => {
                // Punkt(f, x0): on a function at x0; Punkt(Kreis, w): on a circle at angle w.
                if n != 2 {
                    return wrong("Punkt(f, 1) oder Punkt(Kreis, Winkel)");
                }
                let obj = self.eval(&args[0])?;
                let t = num(&self.eval(&args[1])?)?;
                match obj {
                    Val::Func(f) => Ok(Val::Point((t, f.eval(t)))),
                    Val::Circle(c, r) => Ok(Val::Point((c.0 + r * t.cos(), c.1 + r * t.sin()))),
                    Val::Segment(p, q) => Ok(Val::Point((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t))),
                    Val::Line { a, b, c } => {
                        let n2 = a * a + b * b;
                        let p0 = (a * c / n2, b * c / n2);
                        let d = (-b / n2.sqrt(), a / n2.sqrt());
                        Ok(Val::Point((p0.0 + d.0 * t, p0.1 + d.1 * t)))
                    }
                    _ => wrong("Punkt(f, 1)"),
                }
            }
            "line" => {
                if n != 2 {
                    return wrong("Gerade(A, B) oder Gerade(A, g) (parallel)");
                }
                let v = self.vals(args)?;
                let p = pt(&v[0])?;
                if let Ok(q) = pt(&v[1]) {
                    if (p.0 - q.0).abs() + (p.1 - q.1).abs() < 1e-12 {
                        return Err("Die Punkte liegen aufeinander".into());
                    }
                    let (a, b, c) = through(p, q);
                    return Ok(Val::Line { a, b, c });
                }
                if let Some((a, b, _)) = line_of(&v[1]) {
                    return Ok(Val::Line { a, b, c: a * p.0 + b * p.1 });
                }
                wrong("Gerade(A, B)")
            }
            "parallel" => {
                if n != 2 {
                    return wrong("Parallele(A, g)");
                }
                let v = self.vals(args)?;
                let p = pt(&v[0])?;
                let (a, b, _) = line_of(&v[1]).ok_or("Parallele zu einer Geraden")?;
                Ok(Val::Line { a, b, c: a * p.0 + b * p.1 })
            }
            "segment" => {
                let v = self.vals(args)?;
                if n == 2 {
                    let p = pt(&v[0])?;
                    if let Ok(q) = pt(&v[1]) {
                        return Ok(Val::Segment(p, q));
                    }
                    if let Ok(l) = num(&v[1]) {
                        return Ok(Val::Segment(p, (p.0 + l, p.1)));
                    }
                }
                wrong("Strecke(A, B)")
            }
            "ray" => {
                let v = self.vals(args)?;
                if n == 2 {
                    return Ok(Val::Ray(pt(&v[0])?, pt(&v[1])?));
                }
                wrong("Strahl(A, B)")
            }
            "vector" => {
                let v = self.vals(args)?;
                match n {
                    1 => {
                        let p = pt(&v[0])?;
                        Ok(Val::Vector { from: (0.0, 0.0), d: p })
                    }
                    2 => {
                        let (p, q) = (pt(&v[0])?, pt(&v[1])?);
                        Ok(Val::Vector { from: p, d: (q.0 - p.0, q.1 - p.1) })
                    }
                    _ => wrong("Vektor(A, B)"),
                }
            }
            "circle" => {
                let v = self.vals(args)?;
                match n {
                    2 => {
                        let m = pt(&v[0])?;
                        if let Ok(q) = pt(&v[1]) {
                            return Ok(Val::Circle(m, (q.0 - m.0).hypot(q.1 - m.1)));
                        }
                        let r = num(&v[1])?;
                        if r <= 0.0 {
                            return Err("Der Radius muss größer als 0 sein".into());
                        }
                        Ok(Val::Circle(m, r))
                    }
                    3 => {
                        let (c, r) = circumcircle(pt(&v[0])?, pt(&v[1])?, pt(&v[2])?).ok_or("Die drei Punkte liegen auf einer Geraden")?;
                        Ok(Val::Circle(c, r))
                    }
                    _ => wrong("Kreis(M, 3), Kreis(M, A) oder Kreis(A, B, C)"),
                }
            }
            "polygon" => {
                let v = self.vals(args)?;
                let mut ps = Vec::new();
                for x in &v {
                    match x {
                        Val::Points(more) => ps.extend(more),
                        other => ps.push(pt(other)?),
                    }
                }
                if ps.len() < 3 {
                    return wrong("Vieleck(A, B, C, …)");
                }
                Ok(Val::Polygon(ps))
            }
            "midpoint" => {
                let v = self.vals(args)?;
                match (n, v.first()) {
                    (2, _) => {
                        let (p, q) = (pt(&v[0])?, pt(&v[1])?);
                        Ok(Val::Point(((p.0 + q.0) / 2.0, (p.1 + q.1) / 2.0)))
                    }
                    (1, Some(Val::Segment(p, q))) => Ok(Val::Point(((p.0 + q.0) / 2.0, (p.1 + q.1) / 2.0))),
                    (1, Some(Val::Circle(c, _))) => Ok(Val::Point(*c)),
                    _ => wrong("Mittelpunkt(A, B)"),
                }
            }
            "intersect" => {
                if n < 2 {
                    return wrong("Schneide(f, g)");
                }
                let v = self.vals(&args[..2])?;
                let mut ps = intersections(&v[0], &v[1], lo, hi)?;
                ps.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
                if n >= 3 {
                    let k = num(&self.eval(&args[2])?)? as usize;
                    return ps.get(k.saturating_sub(1)).map(|p| Val::Point(*p)).ok_or_else(|| format!("Es gibt keinen {k}. Schnittpunkt"));
                }
                Ok(if ps.len() == 1 { Val::Point(ps[0]) } else { Val::Points(ps) })
            }
            "root" | "extremum" | "inflectionpoint" => {
                if n == 0 {
                    return wrong("Nullstelle(f)");
                }
                let f = self.func(&args[0])?;
                // Nullstelle(f, x0): the one nearest to x0 (in a wide range around the view).
                let near = if n == 2 { Some(num(&self.eval(&args[1])?)?) } else { None };
                let (a, b) = if n >= 3 {
                    (num(&self.eval(&args[1])?)?, num(&self.eval(&args[2])?)?)
                } else if let Some(x0) = near {
                    (x0.min(view.x0) - span, x0.max(view.x1) + span)
                } else {
                    (view.x0, view.x1)
                };
                let pts: Vec<Pt> = match cmd {
                    "root" => zeros_of(&|x| f.eval(x), a, b).into_iter().map(|x| (x, 0.0)).collect(),
                    "extremum" => {
                        let d = f.derive();
                        zeros_of(&|x| d.eval(x), a, b)
                            .into_iter()
                            .filter(|&x| {
                                let h = (b - a) * 1e-5;
                                let (l, r) = (d.eval(x - h), d.eval(x + h));
                                l.is_finite() && r.is_finite() && l.signum() != r.signum()
                            })
                            .filter_map(|x| at(&f, x).map(|y| (x, y)))
                            .collect()
                    }
                    _ => {
                        let d2 = f.derive().derive();
                        zeros_of(&|x| d2.eval(x), a, b)
                            .into_iter()
                            .filter(|&x| {
                                let h = (b - a) * 1e-5;
                                let (l, r) = (d2.eval(x - h), d2.eval(x + h));
                                l.is_finite() && r.is_finite() && l.signum() != r.signum()
                            })
                            .filter_map(|x| at(&f, x).map(|y| (x, y)))
                            .collect()
                    }
                };
                if let Some(x0) = near {
                    return pts.iter().min_by(|p, q| (p.0 - x0).abs().total_cmp(&(q.0 - x0).abs())).map(|p| Val::Point(*p)).ok_or_else(|| "Dort gibt es keinen solchen Punkt".to_string());
                }
                Ok(if pts.len() == 1 { Val::Point(pts[0]) } else { Val::Points(pts) })
            }
            "derivative" => {
                if n == 0 {
                    return wrong("Ableitung(f)");
                }
                let mut f = self.func(&args[0])?;
                let k = if n >= 2 { num(&self.eval(&args[1])?)? as usize } else { 1 };
                for _ in 0..k.max(1) {
                    f = f.derive();
                }
                Ok(Val::Func(f))
            }
            "integral" | "integralbetween" => {
                let (f, g, ai) = if cmd == "integral" {
                    if n != 3 {
                        return wrong("Integral(f, a, b)");
                    }
                    (self.func(&args[0])?, None, 1)
                } else {
                    if n != 4 {
                        return wrong("IntegralZwischen(f, g, a, b)");
                    }
                    (self.func(&args[0])?, Some(self.func(&args[1])?), 2)
                };
                let (a, b) = (num(&self.eval(&args[ai])?)?, num(&self.eval(&args[ai + 1])?)?);
                let value = simpson(&|x| f.eval(x) - g.as_ref().map_or(0.0, |g| g.eval(x)), a, b);
                Ok(Val::Area { f, g, a, b, value })
            }
            "tangent" => {
                if n != 2 {
                    return wrong("Tangente(A, f) oder Tangente(2, f)");
                }
                let v = self.vals(args)?;
                match &v[1] {
                    Val::Func(f) => {
                        let x0 = match &v[0] {
                            Val::Num(x) => *x,
                            other => pt(other)?.0,
                        };
                        let (y0, m) = (f.eval(x0), f.derive().eval(x0));
                        if !y0.is_finite() || !m.is_finite() {
                            return Err("Dort hat die Funktion keine Tangente".into());
                        }
                        Ok(Val::Line { a: -m, b: 1.0, c: y0 - m * x0 })
                    }
                    Val::Circle(c, r) => {
                        let p = pt(&v[0])?;
                        let d = (p.0 - c.0).hypot(p.1 - c.1);
                        if (d - r).abs() < 1e-9 * (1.0 + r) {
                            let (a, b) = (p.0 - c.0, p.1 - c.1);
                            return Ok(Val::Line { a, b, c: a * p.0 + b * p.1 });
                        }
                        if d < *r {
                            return Err("Der Punkt liegt im Kreis".into());
                        }
                        let base = (p.1 - c.1).atan2(p.0 - c.0);
                        let th = (r / d).acos();
                        let lines = [base + th, base - th]
                            .iter()
                            .map(|w| {
                                let t = (c.0 + r * w.cos(), c.1 + r * w.sin());
                                through(p, t)
                            })
                            .collect();
                        Ok(Val::Lines(lines))
                    }
                    _ => wrong("Tangente(A, f)"),
                }
            }
            "perpendicularline" => {
                if n != 2 {
                    return wrong("Senkrechte(A, g)");
                }
                let v = self.vals(args)?;
                let p = pt(&v[0])?;
                let (a, b, _) = line_of(&v[1]).ok_or("Senkrechte zu einer Geraden")?;
                let (a2, b2) = (-b, a);
                Ok(Val::Line { a: a2, b: b2, c: a2 * p.0 + b2 * p.1 })
            }
            "perpendicularbisector" => {
                let v = self.vals(args)?;
                let (p, q) = match (n, v.first()) {
                    (2, _) => (pt(&v[0])?, pt(&v[1])?),
                    (1, Some(Val::Segment(p, q))) => (*p, *q),
                    _ => return wrong("Mittelsenkrechte(A, B)"),
                };
                let m = ((p.0 + q.0) / 2.0, (p.1 + q.1) / 2.0);
                let (a, b) = (q.0 - p.0, q.1 - p.1);
                Ok(Val::Line { a, b, c: a * m.0 + b * m.1 })
            }
            "anglebisector" => {
                if n != 3 {
                    return wrong("Winkelhalbierende(A, B, C)");
                }
                let v = self.vals(args)?;
                let (a, b, c) = (pt(&v[0])?, pt(&v[1])?, pt(&v[2])?);
                let unit = |p: Pt| {
                    let l = (p.0 - b.0).hypot(p.1 - b.1).max(1e-12);
                    ((p.0 - b.0) / l, (p.1 - b.1) / l)
                };
                let (u, w) = (unit(a), unit(c));
                let d = (u.0 + w.0, u.1 + w.1);
                let d = if d.0.hypot(d.1) < 1e-12 { (-u.1, u.0) } else { d };
                let (la, lb, lc) = through(b, (b.0 + d.0, b.1 + d.1));
                Ok(Val::Line { a: la, b: lb, c: lc })
            }
            "distance" => {
                if n != 2 {
                    return wrong("Abstand(A, B) oder Abstand(A, g)");
                }
                let v = self.vals(args)?;
                let p = pt(&v[0])?;
                if let Ok(q) = pt(&v[1]) {
                    return Ok(Val::Num((p.0 - q.0).hypot(p.1 - q.1)));
                }
                if let Some((a, b, c)) = line_of(&v[1]) {
                    return Ok(Val::Num((a * p.0 + b * p.1 - c).abs() / a.hypot(b)));
                }
                if let Val::Circle(m, r) = v[1] {
                    return Ok(Val::Num(((p.0 - m.0).hypot(p.1 - m.1) - r).abs()));
                }
                wrong("Abstand(A, B)")
            }
            "length" => {
                let v = self.vals(args)?;
                match (n, v.first()) {
                    (1, Some(Val::Segment(p, q))) => Ok(Val::Num((p.0 - q.0).hypot(p.1 - q.1))),
                    (1, Some(Val::Vector { d, .. })) => Ok(Val::Num(d.0.hypot(d.1))),
                    (1, Some(Val::Circle(_, r))) => Ok(Val::Num(2.0 * PI * r)),
                    (1, Some(Val::Polygon(ps))) => Ok(Val::Num(ps.iter().zip(ps.iter().cycle().skip(1)).map(|(a, b)| (a.0 - b.0).hypot(a.1 - b.1)).sum())),
                    (2, _) => {
                        let (p, q) = (pt(&v[0])?, pt(&v[1])?);
                        Ok(Val::Num((p.0 - q.0).hypot(p.1 - q.1)))
                    }
                    _ => wrong("Länge(Strecke)"),
                }
            }
            "slope" => {
                let v = self.vals(args)?;
                let (a, b, _) = v.first().and_then(line_of).ok_or("Steigung(g)")?;
                if b.abs() < 1e-12 {
                    return Err("Eine senkrechte Gerade hat keine Steigung".into());
                }
                Ok(Val::Num(-a / b))
            }
            "angle" => {
                let v = self.vals(args)?;
                match n {
                    3 => {
                        let (a, b, c) = (pt(&v[0])?, pt(&v[1])?, pt(&v[2])?);
                        let from = (a.1 - b.1).atan2(a.0 - b.0);
                        let to = (c.1 - b.1).atan2(c.0 - b.0);
                        Ok(Val::Angle { at: b, from, sweep: (to - from).rem_euclid(2.0 * PI) })
                    }
                    2 => {
                        let (l, m) = (line_of(&v[0]).ok_or("Winkel(g, h)")?, line_of(&v[1]).ok_or("Winkel(g, h)")?);
                        let at = meet(l, m).ok_or("Die Geraden sind parallel")?;
                        let (d1, d2) = ((-l.1).atan2(l.0).rem_euclid(PI), (-m.1).atan2(m.0).rem_euclid(PI));
                        let from = (l.0).atan2(-l.1);
                        let _ = (d1, d2);
                        let to = (m.0).atan2(-m.1);
                        Ok(Val::Angle { at, from, sweep: (to - from).rem_euclid(PI) })
                    }
                    _ => wrong("Winkel(A, B, C)"),
                }
            }
            "function" => {
                if n != 3 {
                    return wrong("Funktion(f, a, b)");
                }
                let f = self.func(&args[0])?;
                let (a, b) = (num(&self.eval(&args[1])?)?, num(&self.eval(&args[2])?)?);
                Ok(Val::Func(Expr::If(vec![Expr::Num(a), Expr::X, Expr::Num(b)], vec![Cmp::Le, Cmp::Le], bx(f), bx(Expr::Num(f64::NAN)))))
            }
            "if" => {
                if n < 2 {
                    return wrong("Wenn(Bedingung, dann, sonst)");
                }
                if args.iter().any(free) {
                    let e = self.fexpr(&Node::Call("Wenn".into(), 0, args.to_vec()))?;
                    return Ok(Val::Func(e));
                }
                match self.eval(&args[0])? {
                    Val::Bool(true) => self.eval(&args[1]),
                    Val::Bool(false) if n > 2 => self.eval(&args[2]),
                    Val::Bool(false) => Err("Die Bedingung gilt nicht".into()),
                    _ => Err("Die Bedingung braucht einen Vergleich".into()),
                }
            }
            "curve" => {
                if n != 5 {
                    return wrong("Kurve(cos(t), sin(t), t, 0, 2π)");
                }
                let (fx, fy) = (self.fexpr(&args[0])?, self.fexpr(&args[1])?);
                let (a, b) = (num(&self.eval(&args[3])?)?, num(&self.eval(&args[4])?)?);
                Ok(Val::Curve(fx.simplified(), fy.simplified(), a, b))
            }
            "text" => {
                if n == 0 {
                    return wrong("Text(\"…\")");
                }
                let v = self.eval(&args[0])?;
                Ok(Val::Text(match v {
                    Val::Text(t) => t,
                    other => format_val(&other),
                }))
            }
            "slider" => {
                if n < 2 {
                    return wrong("Schieberegler(-5, 5, 0.1)");
                }
                let (a, b) = (num(&self.eval(&args[0])?)?, num(&self.eval(&args[1])?)?);
                Ok(Val::Num(if a <= 1.0 && 1.0 <= b { 1.0 } else { a }))
            }
            "reflect" => {
                if n != 2 {
                    return wrong("Spiegle(A, g) oder Spiegle(A, P)");
                }
                let v = self.vals(args)?;
                let over = v[1].clone();
                map_val(&v[0], &|p| reflect_pt(p, &over).unwrap_or(p))
            }
            "translate" => {
                if n != 2 {
                    return wrong("Verschiebe(A, v)");
                }
                let v = self.vals(args)?;
                let d = match &v[1] {
                    Val::Vector { d, .. } => *d,
                    other => pt(other)?,
                };
                map_val(&v[0], &|p| (p.0 + d.0, p.1 + d.1))
            }
            "rotate" => {
                if !(2..=3).contains(&n) {
                    return wrong("Drehe(A, 90°, M)");
                }
                let v = self.vals(args)?;
                let w = num(&v[1])?;
                let c = if n == 3 { pt(&v[2])? } else { (0.0, 0.0) };
                let (s, k) = w.sin_cos();
                map_val(&v[0], &|p| (c.0 + (p.0 - c.0) * k - (p.1 - c.1) * s, c.1 + (p.0 - c.0) * s + (p.1 - c.1) * k))
            }
            "area" => {
                let v = self.vals(args)?;
                let ps: Vec<Pt> = match (n, v.first()) {
                    (1, Some(Val::Polygon(ps))) => ps.clone(),
                    (1, Some(Val::Circle(_, r))) => return Ok(Val::Num(PI * r * r)),
                    _ => v.iter().map(pt).collect::<Result<_, _>>()?,
                };
                if ps.len() < 3 {
                    return wrong("Fläche(Vieleck)");
                }
                let s: f64 = ps.iter().zip(ps.iter().cycle().skip(1)).map(|(a, b)| a.0 * b.1 - b.0 * a.1).sum();
                Ok(Val::Num(s.abs() / 2.0))
            }
            "min" | "max" => {
                let v = self.vals(args)?;
                let xs: Vec<f64> = v.iter().map(num).collect::<Result<_, _>>()?;
                if xs.is_empty() {
                    return wrong("Min(a, b)");
                }
                Ok(Val::Num(if cmd == "min" { xs.iter().cloned().fold(f64::INFINITY, f64::min) } else { xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max) }))
            }
            other => Err(format!("{other}: kommt noch")),
        }
    }
}

/// A circle's centre and radius if F(x, y) = a(x² + y²) + d·x + e·y + k with a ≠ 0.
fn circle_of(f: &Expr) -> Option<(Pt, f64)> {
    let v = |x: f64, y: f64| f.eval2(x, y);
    let c0 = v(0.0, 0.0);
    let (fx1, fxm, fy1, fym) = (v(1.0, 0.0), v(-1.0, 0.0), v(0.0, 1.0), v(0.0, -1.0));
    let (ax, dx) = ((fx1 + fxm - 2.0 * c0) / 2.0, (fx1 - fxm) / 2.0);
    let (ay, ey) = ((fy1 + fym - 2.0 * c0) / 2.0, (fy1 - fym) / 2.0);
    let bxy = (v(1.0, 1.0) - v(1.0, -1.0) - v(-1.0, 1.0) + v(-1.0, -1.0)) / 4.0;
    if ![c0, ax, dx, ay, ey, bxy].iter().all(|x| x.is_finite()) || ax.abs() < 1e-12 {
        return None;
    }
    let model = |x: f64, y: f64| ax * x * x + ay * y * y + bxy * x * y + dx * x + ey * y + c0;
    let fits = [(2.0, 3.0), (-1.7, 0.4), (5.0, -2.5), (0.3, 7.0), (-3.3, -4.1)].iter().all(|&(x, y)| {
        let w = v(x, y);
        w.is_finite() && (w - model(x, y)).abs() <= 1e-9 * (1.0 + w.abs())
    });
    if !fits || (ax - ay).abs() > 1e-9 * ax.abs() || bxy.abs() > 1e-9 * ax.abs() {
        return None;
    }
    let (cx, cy) = (-dx / (2.0 * ax), -ey / (2.0 * ax));
    let r2 = cx * cx + cy * cy - c0 / ax;
    (r2 > 0.0).then(|| ((cx, cy), r2.sqrt()))
}

fn simpson(f: &dyn Fn(f64) -> f64, a: f64, b: f64) -> f64 {
    let n = 2000;
    let h = (b - a) / n as f64;
    let v = |x: f64| {
        let y = f(x);
        if y.is_finite() { y } else { 0.0 }
    };
    let mut s = v(a) + v(b);
    for i in 1..n {
        s += v(a + h * i as f64) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    s * h / 3.0
}

/// Where two objects meet (in the x range lo … hi for graphs).
fn intersections(a: &Val, b: &Val, lo: f64, hi: f64) -> Result<Vec<Pt>, String> {
    use Val::*;
    let ps: Vec<Pt> = match (a, b) {
        (Func(f), Func(g)) => zeros_of(&|x| f.eval(x) - g.eval(x), lo, hi).into_iter().filter_map(|x| at(f, x).map(|y| (x, y))).collect(),
        (Func(f), other) | (other, Func(f)) if line_of(other).is_some() => {
            let (la, lb, lc) = line_of(other).unwrap();
            let ps: Vec<Pt> = if lb.abs() < 1e-12 {
                let x = lc / la;
                at(f, x).map(|y| vec![(x, y)]).unwrap_or_default()
            } else {
                zeros_of(&|x| f.eval(x) - (lc - la * x) / lb, lo, hi).into_iter().filter_map(|x| at(f, x).map(|y| (x, y))).collect()
            };
            ps.into_iter().filter(|p| on_part(other, *p)).collect()
        }
        (Func(f), Circle(c, r)) | (Circle(c, r), Func(f)) => {
            let (c, r) = (*c, *r);
            let d = |x: f64| f.eval(x).map_or_nan(|y| (x - c.0).hypot(y - c.1) - r);
            zeros_of(&d, c.0 - r, c.0 + r).into_iter().filter_map(|x| at(f, x).map(|y| (x, y))).collect()
        }
        (Circle(c1, r1), Circle(c2, r2)) => circle_circle(*c1, *r1, *c2, *r2),
        (Circle(c, r), other) | (other, Circle(c, r)) if line_of(other).is_some() => line_circle(line_of(other).unwrap(), *c, *r).into_iter().filter(|p| on_part(other, *p)).collect(),
        (x, y) if line_of(x).is_some() && line_of(y).is_some() => meet(line_of(x).unwrap(), line_of(y).unwrap()).into_iter().filter(|p| on_part(x, *p) && on_part(y, *p)).collect(),
        (Implicit(f), Func(g)) | (Func(g), Implicit(f)) => zeros_of(&|x| g.eval(x).map_or_nan(|y| f.eval2(x, y)), lo, hi).into_iter().filter_map(|x| at(g, x).map(|y| (x, y))).collect(),
        (Implicit(f), other) | (other, Implicit(f)) if line_of(other).is_some() => {
            let (la, lb, lc) = line_of(other).unwrap();
            let n2 = la * la + lb * lb;
            let o = (la * lc / n2, lb * lc / n2);
            let d = (-lb / n2.sqrt(), la / n2.sqrt());
            let span = (hi - lo).abs() * 2.0 + o.0.abs() + o.1.abs();
            zeros_of(&|t| f.eval2(o.0 + d.0 * t, o.1 + d.1 * t), -span, span).into_iter().map(|t| (o.0 + d.0 * t, o.1 + d.1 * t)).filter(|p| on_part(other, *p)).collect()
        }
        (Polygon(ps), other) | (other, Polygon(ps)) => {
            let mut out = Vec::new();
            for (p, q) in ps.iter().zip(ps.iter().cycle().skip(1)) {
                out.extend(intersections(&Segment(*p, *q), other, lo, hi)?);
            }
            out
        }
        _ => return Err("Diese beiden kann ich (noch) nicht schneiden".into()),
    };
    Ok(ps)
}

trait NanMap {
    fn map_or_nan(self, f: impl Fn(f64) -> f64) -> f64;
}

impl NanMap for f64 {
    fn map_or_nan(self, f: impl Fn(f64) -> f64) -> f64 {
        if self.is_finite() { f(self) } else { f64::NAN }
    }
}

/// Numbers and points on an object of the construction: + − · ÷ ^.
fn arith(op: Op, a: Val, b: Val) -> Result<Val, String> {
    use Val::*;
    let f = |x: f64, y: f64| match op {
        Op::Add => x + y,
        Op::Sub => x - y,
        Op::Mul => x * y,
        Op::Div => x / y,
        Op::Pow => super::expr::pow(x, y),
    };
    let as_num = |v: &Val| match v {
        Num(x) => Some(*x),
        Angle { sweep, .. } => Some(*sweep),
        Area { value, .. } => Some(*value),
        _ => None,
    };
    if let (Some(x), Some(y)) = (as_num(&a), as_num(&b)) {
        return Ok(Num(f(x, y)));
    }
    // A function with a number or a function: a function (in the operands' order).
    let as_fn = |v: &Val| match v {
        Func(e) => Some(e.clone()),
        Num(x) => Some(Expr::Num(*x)),
        _ => None,
    };
    if (matches!(a, Func(_)) || matches!(b, Func(_)))
        && let (Some(l), Some(r)) = (as_fn(&a), as_fn(&b))
    {
        let (l, r) = (bx(l), bx(r));
        return Ok(Func(
            match op {
                Op::Add => Expr::Add(l, r),
                Op::Sub => Expr::Sub(l, r),
                Op::Mul => Expr::Mul(l, r),
                Op::Div => Expr::Div(l, r),
                Op::Pow => Expr::Pow(l, r),
            }
            .simplified(),
        ));
    }
    Ok(match (op, a, b) {
        (Op::Add, Point(p), Point(q)) => Point((p.0 + q.0, p.1 + q.1)),
        (Op::Sub, Point(p), Point(q)) => Vector { from: (0.0, 0.0), d: (p.0 - q.0, p.1 - q.1) },
        (Op::Mul, Point(p), Point(q)) => Num(p.0 * q.0 + p.1 * q.1),
        (Op::Add, Point(p), Vector { d, .. }) | (Op::Add, Vector { d, .. }, Point(p)) => Point((p.0 + d.0, p.1 + d.1)),
        (Op::Sub, Point(p), Vector { d, .. }) => Point((p.0 - d.0, p.1 - d.1)),
        (Op::Add, Vector { from, d }, Vector { d: e, .. }) => Vector { from, d: (d.0 + e.0, d.1 + e.1) },
        (Op::Sub, Vector { from, d }, Vector { d: e, .. }) => Vector { from, d: (d.0 - e.0, d.1 - e.1) },
        (Op::Mul, Num(k), Point(p)) | (Op::Mul, Point(p), Num(k)) => Point((k * p.0, k * p.1)),
        (Op::Div, Point(p), Num(k)) => Point((p.0 / k, p.1 / k)),
        (Op::Mul, Num(k), Vector { from, d }) | (Op::Mul, Vector { from, d }, Num(k)) => Vector { from, d: (k * d.0, k * d.1) },
        (Op::Div, Vector { from, d }, Num(k)) => Vector { from, d: (d.0 / k, d.1 / k) },
        (Op::Mul, Num(k), Segment(p, q)) => Segment(p, (p.0 + (q.0 - p.0) * k, p.1 + (q.1 - p.1) * k)),
        _ => return Err("Diese Rechnung geht mit diesen Objekten nicht".into()),
    })
}

// --- writing values out -------------------------------------------------------------------------

/// A number as shown: German decimal comma, up to 2 places (like GeoGebra), no trailing zeros.
pub fn fmt_num(v: f64) -> String {
    if !v.is_finite() {
        return "undefiniert".into();
    }
    let v = if v.abs() < 5e-3 { 0.0 } else { v };
    let s = format!("{v:.2}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    s.replace('.', ",").replace('-', "−")
}

fn fmt_pt(p: Pt) -> String {
    format!("({} | {})", fmt_num(p.0), fmt_num(p.1))
}

/// A value as the algebra view shows it.
pub fn format_val(v: &Val) -> String {
    match v {
        Val::Num(x) => fmt_num(*x),
        Val::Bool(b) => if *b { "wahr" } else { "falsch" }.into(),
        Val::Point(p) => fmt_pt(*p),
        Val::Points(ps) if ps.is_empty() => "keine".into(),
        Val::Points(ps) => ps.iter().map(|p| fmt_pt(*p)).collect::<Vec<_>>().join(", "),
        Val::Vector { d, .. } => format!("({} | {})", fmt_num(d.0), fmt_num(d.1)),
        Val::Func(e) => fmt_expr(e),
        Val::Line { a, b, c } => fmt_line(*a, *b, *c),
        Val::Lines(ls) => ls.iter().map(|l| fmt_line(l.0, l.1, l.2)).collect::<Vec<_>>().join(", "),
        Val::Segment(p, q) | Val::Ray(p, q) => fmt_num((p.0 - q.0).hypot(p.1 - q.1)),
        Val::Circle(c, r) => {
            let part = |v: &str, k: f64| if k.abs() < 1e-12 { format!("{v}²") } else if k > 0.0 { format!("({v} − {})²", fmt_num(k)) } else { format!("({v} + {})²", fmt_num(-k)) };
            format!("{} + {} = {}", part("x", c.0), part("y", c.1), fmt_num(r * r))
        }
        Val::Polygon(ps) => {
            let s: f64 = ps.iter().zip(ps.iter().cycle().skip(1)).map(|(a, b)| a.0 * b.1 - b.0 * a.1).sum();
            fmt_num(s.abs() / 2.0)
        }
        Val::Curve(fx, fy, a, b) => format!("({}, {}), {} ≤ t ≤ {}", fmt_expr(fx).replace('x', "t"), fmt_expr(fy).replace('x', "t"), fmt_num(*a), fmt_num(*b)),
        Val::Implicit(f) => format!("{} = 0", fmt_expr(f)),
        Val::Angle { sweep, .. } => format!("{}°", fmt_num(sweep.to_degrees())),
        Val::Area { value, .. } => fmt_num(*value),
        Val::Text(t) => t.clone(),
    }
}

fn fmt_line(a: f64, b: f64, c: f64) -> String {
    if b.abs() < 1e-12 {
        return format!("x = {}", fmt_num(c / a));
    }
    // y = m x + n
    let (m, n) = (-a / b, c / b);
    let mx = if m.abs() < 1e-12 {
        String::new()
    } else if (m - 1.0).abs() < 1e-12 {
        "x".into()
    } else if (m + 1.0).abs() < 1e-12 {
        "−x".into()
    } else {
        format!("{}x", fmt_num(m))
    };
    match (mx.is_empty(), n.abs() < 1e-12) {
        (true, _) => format!("y = {}", fmt_num(n)),
        (false, true) => format!("y = {mx}"),
        (false, false) if n > 0.0 => format!("y = {mx} + {}", fmt_num(n)),
        (false, false) => format!("y = {mx} − {}", fmt_num(-n)),
    }
}

// --- the same as LaTeX (the algebra list sets it like GeoGebra) -----------------------------------

/// A number in LaTeX: German decimal comma (without the space LaTeX puts after a comma).
pub fn latex_num(v: f64) -> String {
    fmt_num(v).replace('−', "-").replace(',', "{,}")
}

/// A name in LaTeX: one letter in italics (Greek ones too), longer ones as a word, S_1 with
/// its index.
pub fn latex_name(n: &str) -> String {
    let (base, sub) = match n.split_once('_') {
        Some((b, s)) => (b, Some(s)),
        None => (n, None),
    };
    let greek = |c: char| -> Option<&'static str> {
        Some(match c {
            'α' => "\\alpha",
            'β' => "\\beta",
            'γ' => "\\gamma",
            'δ' => "\\delta",
            'ε' => "\\varepsilon",
            'θ' => "\\theta",
            'λ' => "\\lambda",
            'μ' => "\\mu",
            'π' => "\\pi",
            'σ' => "\\sigma",
            'φ' => "\\varphi",
            'ω' => "\\omega",
            _ => return None,
        })
    };
    let mut cs = base.chars();
    let b = match (cs.next(), cs.next()) {
        (Some(c), None) => greek(c).map(String::from).unwrap_or_else(|| c.to_string()),
        _ => format!("\\mathit{{{}}}", base.chars().map(|c| greek(c).map(|g| format!("{g} ")).unwrap_or_else(|| c.to_string())).collect::<String>()),
    };
    match sub {
        Some(s) => format!("{b}_{{{s}}}"),
        None => b,
    }
}

fn latex_text(t: &str) -> String {
    let mut out = String::from("\\text{");
    for c in t.chars() {
        match c {
            '\\' => out.push_str("\\backslash "),
            '{' | '}' | '$' | '%' | '#' | '&' | '_' => {
                out.push('\\');
                out.push(c);
            }
            '^' | '~' => out.push(' '),
            _ => out.push(c),
        }
    }
    out.push('}');
    out
}

/// (inner): plain brackets, growing ones only around something tall (a fraction, a root).
fn brackets(inner: &str) -> String {
    if inner.contains("\\frac") || inner.contains("\\sqrt") || inner.contains("pmatrix") {
        format!("\\left({inner}\\right)")
    } else {
        format!("({inner})")
    }
}

/// An expression in LaTeX: fractions stacked, powers raised, roots drawn; `var` for x.
pub fn latex_expr(e: &Expr, var: &str) -> String {
    fn prec(e: &Expr) -> u8 {
        match e {
            Expr::Add(..) | Expr::Sub(..) => 1,
            Expr::Mul(..) => 2,
            Expr::Neg(_) => 3,
            Expr::Div(..) | Expr::Pow(..) => 4,
            Expr::Num(v) if *v < 0.0 => 3,
            _ => 5,
        }
    }
    fn go(e: &Expr, var: &str, out: &mut String) {
        let wrap = |e: &Expr, min: u8, out: &mut String| {
            if prec(e) < min {
                let mut inner = String::new();
                go(e, var, &mut inner);
                out.push_str(&brackets(&inner));
            } else {
                go(e, var, out);
            }
        };
        match e {
            Expr::Num(v) => {
                if (*v - std::f64::consts::PI).abs() < 1e-12 {
                    out.push_str("\\pi ")
                } else if (*v - std::f64::consts::E).abs() < 1e-12 {
                    out.push('e')
                } else {
                    out.push_str(&latex_num(*v))
                }
            }
            Expr::X => out.push_str(var),
            Expr::Y => out.push('y'),
            Expr::Neg(a) => {
                out.push('-');
                wrap(a, 3, out);
            }
            Expr::Add(a, b) => {
                go(a, var, out);
                match &**b {
                    Expr::Neg(inner) => {
                        out.push_str(" - ");
                        wrap(inner, 2, out);
                    }
                    Expr::Num(v) if *v < 0.0 => {
                        out.push_str(" - ");
                        out.push_str(&latex_num(-v));
                    }
                    _ => {
                        out.push_str(" + ");
                        wrap(b, 1, out);
                    }
                }
            }
            Expr::Sub(a, b) => {
                go(a, var, out);
                out.push_str(" - ");
                wrap(b, 2, out);
            }
            Expr::Mul(a, b) => {
                if let Expr::Num(v) = **a
                    && v == -1.0
                {
                    out.push('-');
                    wrap(b, 3, out);
                    return;
                }
                let implicit = matches!(**a, Expr::Num(_)) && !matches!(**b, Expr::Num(_));
                wrap(a, 2, out);
                out.push_str(if implicit { " " } else { " \\cdot " });
                wrap(b, 3, out);
            }
            Expr::Div(a, b) => {
                out.push_str("\\frac{");
                go(a, var, out);
                out.push_str("}{");
                go(b, var, out);
                out.push('}');
            }
            Expr::Pow(a, b) => {
                wrap(a, 5, out);
                out.push_str("^{");
                go(b, var, out);
                out.push('}');
            }
            Expr::Call(f, a) => {
                let (open, close) = match f {
                    Func::Sqrt => ("\\sqrt{", "}"),
                    Func::Abs => ("\\left|", "\\right|"),
                    Func::Exp => ("e^{", "}"),
                    Func::Floor => ("\\left\\lfloor ", "\\right\\rfloor "),
                    Func::Ceil => ("\\left\\lceil ", "\\right\\rceil "),
                    _ => ("", ""),
                };
                if !open.is_empty() {
                    out.push_str(open);
                    go(a, var, out);
                    out.push_str(close);
                    return;
                }
                out.push_str(match f {
                    Func::Sin => "\\sin",
                    Func::Cos => "\\cos",
                    Func::Tan => "\\tan",
                    Func::Cot => "\\cot",
                    Func::Asin => "\\arcsin",
                    Func::Acos => "\\arccos",
                    Func::Atan => "\\arctan",
                    Func::Sinh => "\\sinh",
                    Func::Cosh => "\\cosh",
                    Func::Tanh => "\\tanh",
                    Func::Ln => "\\ln",
                    Func::Log => "\\lg",
                    Func::Sgn => "\\mathrm{sgn}",
                    _ => "\\mathrm{round}",
                });
                let mut inner = String::new();
                go(a, var, &mut inner);
                out.push_str(&brackets(&inner));
            }
            Expr::If(items, ops, a, b) => {
                out.push_str("\\text{Wenn}\\left(");
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(match ops[i - 1] {
                            Cmp::Lt => " < ",
                            Cmp::Le => " \\le ",
                            Cmp::Gt => " > ",
                            Cmp::Ge => " \\ge ",
                            Cmp::Eq => " = ",
                        });
                    }
                    go(it, var, out);
                }
                out.push_str(", ");
                go(a, var, out);
                if !matches!(**b, Expr::Num(v) if v.is_nan()) {
                    out.push_str(", ");
                    go(b, var, out);
                }
                out.push_str("\\right)");
            }
            Expr::Ref(..) => out.push('?'),
        }
    }
    let mut s = String::new();
    go(e, var, &mut s);
    s
}

fn latex_line(a: f64, b: f64, c: f64) -> String {
    fmt_line(a, b, c).replace('−', "-").replace(',', "{,}")
}

/// A value in LaTeX (vectors as columns, angles with °, texts as text).
pub fn latex_val(v: &Val) -> String {
    let pt = |p: Pt| format!("({} \\mid {})", latex_num(p.0), latex_num(p.1));
    match v {
        Val::Num(x) => latex_num(*x),
        Val::Bool(b) => latex_text(if *b { "wahr" } else { "falsch" }),
        Val::Point(p) => pt(*p),
        Val::Points(ps) if ps.is_empty() => latex_text("keine"),
        Val::Points(ps) => ps.iter().map(|p| pt(*p)).collect::<Vec<_>>().join(",\\ "),
        Val::Vector { d, .. } => format!("\\begin{{pmatrix}}{} \\\\ {}\\end{{pmatrix}}", latex_num(d.0), latex_num(d.1)),
        Val::Func(e) => latex_expr(e, "x"),
        Val::Line { a, b, c } => latex_line(*a, *b, *c),
        Val::Lines(ls) => ls.iter().map(|l| latex_line(l.0, l.1, l.2)).collect::<Vec<_>>().join(",\\ "),
        Val::Circle(c, r) => {
            let part = |v: &str, k: f64| {
                if k.abs() < 1e-12 {
                    format!("{v}^{{2}}")
                } else if k > 0.0 {
                    format!("({v} - {})^{{2}}", latex_num(k))
                } else {
                    format!("({v} + {})^{{2}}", latex_num(-k))
                }
            };
            format!("{} + {} = {}", part("x", c.0), part("y", c.1), latex_num(r * r))
        }
        Val::Curve(fx, fy, a, b) => format!("\\left({}, {}\\right),\\ {} \\le t \\le {}", latex_expr(fx, "t"), latex_expr(fy, "t"), latex_num(*a), latex_num(*b)),
        Val::Implicit(f) => format!("{} = 0", latex_expr(f, "x")),
        Val::Angle { sweep, .. } => format!("{}^{{\\circ}}", latex_num(sweep.to_degrees())),
        Val::Text(t) => latex_text(t),
        _ => format_val(v).replace('−', "-").replace(',', "{,}"),
    }
}

/// An object's row in the algebra list, as LaTeX: f(x) = …, g: y = …, A = (1 | 2).
pub fn latex_row(name: &str, v: &Val) -> String {
    let n = latex_name(name);
    match v {
        Val::Func(_) => format!("{n}(x) = {}", latex_val(v)),
        Val::Line { .. } | Val::Lines(_) | Val::Circle(..) | Val::Implicit(_) | Val::Curve(..) => format!("{n}{{:}}\\; {}", latex_val(v)),
        Val::Text(_) => latex_val(v),
        _ => format!("{n} = {}", latex_val(v)),
    }
}

/// An expression as written: 0,5x² − 3, 2 sin(x), x^(1/3).
pub fn fmt_expr(e: &Expr) -> String {
    fn prec(e: &Expr) -> u8 {
        match e {
            Expr::Add(..) | Expr::Sub(..) => 1,
            Expr::Mul(..) | Expr::Div(..) => 2,
            Expr::Neg(_) => 3,
            Expr::Pow(..) => 4,
            Expr::Num(v) if *v < 0.0 => 3,
            _ => 5,
        }
    }
    fn sup(n: f64) -> Option<&'static str> {
        match n {
            v if v == 2.0 => Some("²"),
            v if v == 3.0 => Some("³"),
            _ => None,
        }
    }
    fn go(e: &Expr, out: &mut String) {
        let wrap = |e: &Expr, min: u8, out: &mut String| {
            if prec(e) < min {
                out.push('(');
                go(e, out);
                out.push(')');
            } else {
                go(e, out);
            }
        };
        match e {
            Expr::Num(v) => {
                if (*v - std::f64::consts::PI).abs() < 1e-12 {
                    out.push('π')
                } else if (*v - std::f64::consts::E).abs() < 1e-12 {
                    out.push('e')
                } else {
                    out.push_str(&fmt_num(*v))
                }
            }
            Expr::X => out.push('x'),
            Expr::Y => out.push('y'),
            Expr::Neg(a) => {
                out.push('−');
                wrap(a, 3, out);
            }
            Expr::Add(a, b) => {
                go(a, out);
                match &**b {
                    Expr::Neg(inner) => {
                        out.push_str(" − ");
                        wrap(inner, 2, out);
                    }
                    Expr::Num(v) if *v < 0.0 => {
                        out.push_str(" − ");
                        out.push_str(&fmt_num(-v));
                    }
                    _ => {
                        out.push_str(" + ");
                        wrap(b, 1, out);
                    }
                }
            }
            Expr::Sub(a, b) => {
                go(a, out);
                out.push_str(" − ");
                wrap(b, 2, out);
            }
            Expr::Mul(a, b) => {
                // 2x, 0,5x², 3 sin(x): a number in front of something else goes without a dot.
                let implicit = matches!(**a, Expr::Num(_)) && !matches!(**b, Expr::Num(_));
                if let Expr::Num(v) = **a
                    && v == -1.0
                {
                    out.push('−');
                    wrap(b, 3, out);
                    return;
                }
                wrap(a, 2, out);
                out.push_str(if implicit { "" } else { " · " });
                if implicit && matches!(**b, Expr::Call(..)) {
                    out.push(' ');
                }
                wrap(b, 3, out);
            }
            Expr::Div(a, b) => {
                wrap(a, 2, out);
                out.push_str(" / ");
                wrap(b, 3, out);
            }
            Expr::Pow(a, b) => {
                wrap(a, 5, out);
                match **b {
                    Expr::Num(n) if sup(n).is_some() => out.push_str(sup(n).unwrap()),
                    _ => {
                        out.push('^');
                        wrap(b, 5, out);
                    }
                }
            }
            Expr::Call(f, a) => {
                let name = match f {
                    Func::Sin => "sin",
                    Func::Cos => "cos",
                    Func::Tan => "tan",
                    Func::Cot => "cot",
                    Func::Asin => "arcsin",
                    Func::Acos => "arccos",
                    Func::Atan => "arctan",
                    Func::Sinh => "sinh",
                    Func::Cosh => "cosh",
                    Func::Tanh => "tanh",
                    Func::Exp => "exp",
                    Func::Ln => "ln",
                    Func::Log => "lg",
                    Func::Sqrt => "√",
                    Func::Abs => "abs",
                    Func::Sgn => "sgn",
                    Func::Floor => "floor",
                    Func::Ceil => "ceil",
                    Func::Round => "round",
                };
                if *f == Func::Abs {
                    out.push('|');
                    go(a, out);
                    out.push('|');
                    return;
                }
                out.push_str(name);
                out.push('(');
                go(a, out);
                out.push(')');
            }
            Expr::If(items, ops, a, b) => {
                out.push_str("Wenn(");
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(match ops[i - 1] {
                            Cmp::Lt => " < ",
                            Cmp::Le => " ≤ ",
                            Cmp::Gt => " > ",
                            Cmp::Ge => " ≥ ",
                            Cmp::Eq => " = ",
                        });
                    }
                    go(it, out);
                }
                out.push_str(", ");
                go(a, out);
                if !matches!(**b, Expr::Num(v) if v.is_nan()) {
                    out.push_str(", ");
                    go(b, out);
                }
                out.push(')');
            }
            Expr::Ref(..) => out.push('?'),
        }
    }
    let mut s = String::new();
    go(e, &mut s);
    s
}

// --- names --------------------------------------------------------------------------------------

/// The next free name for a new object of `kind`, as GeoGebra names them: points A, B, C …,
/// functions and lines f, g, h …, circles c, d …, numbers a, b …, vectors u, v, w, angles α …,
/// polygons vieleck1 …, texts text1 ….
pub fn next_name(kind: Kind, taken: &HashSet<String>) -> String {
    let series: Vec<String> = match kind {
        Kind::Point | Kind::Points => ('A'..='Z').map(|c| c.to_string()).collect(),
        Kind::Vector => ["u", "v", "w"].iter().map(|s| s.to_string()).collect(),
        Kind::Function | Kind::Line | Kind::Lines | Kind::Segment | Kind::Ray => ["f", "g", "h", "p", "q", "r", "s"].iter().map(|s| s.to_string()).collect(),
        Kind::Circle | Kind::Implicit | Kind::Curve => ["c", "d", "k", "m"].iter().map(|s| s.to_string()).collect(),
        Kind::Number | Kind::Bool | Kind::Area => ["a", "b", "d", "j", "n"].iter().map(|s| s.to_string()).collect(),
        Kind::Angle => ["α", "β", "γ", "δ", "ε", "ζ", "η", "θ"].iter().map(|s| s.to_string()).collect(),
        Kind::Polygon => (1..100).map(|i| format!("vieleck{i}")).collect(),
        Kind::Text => (1..100).map(|i| format!("text{i}")).collect(),
    };
    if let Some(n) = series.iter().find(|n| !taken.contains(*n)) {
        return n.clone();
    }
    for i in 1..1000 {
        for base in &series {
            let n = format!("{base}_{i}");
            if !taken.contains(&n) {
                return n;
            }
        }
    }
    format!("obj{}", taken.len())
}

/// A new object from a line of input: its definition with a name (given or chosen), checked
/// against the construction (`objs`). A slider (Schieberegler(…) or a plain number `a = 3`)
/// gets its range.
pub fn new_object(input: &str, objs: &[Obj], view: ViewRange) -> Result<(String, Style), String> {
    let mut taken: HashSet<String> = objs.iter().map(|o| o.name()).collect();
    let known = taken.clone();
    let parsed = lang::parse_input(input, &known)?;
    let mut style = Style::default();
    let given = parsed.name.clone();
    if let Some(n) = &given
        && taken.contains(n)
    {
        return Err(format!("Den Namen {n} gibt es schon"));
    }
    // Work it out in the construction to know its kind (and catch mistakes).
    let probe_name = given.clone().unwrap_or_else(|| "zzneuesobjekt".into());
    let def = match &given {
        Some(_) => input.trim().to_string(),
        None => format!("{probe_name} = {}", input.trim()),
    };
    // A plain equation without a name gets a label instead of "= …".
    let def = if given.is_none() && matches!(parsed.body, Body::Equation(..)) { format!("{probe_name}: {}", input.trim()) } else { def };
    let mut all = objs.to_vec();
    all.push(Obj { id: String::new(), def: def.clone(), style: Style::default(), order: f64::MAX });
    taken.insert(probe_name.clone());
    let val = evaluate(&all, view).pop().unwrap()?;
    let kind = val.kind();
    let name = given.unwrap_or_else(|| {
        taken.remove(&probe_name);
        next_name(kind, &taken)
    });
    let mut def = renamed(&def, &name);
    // Schieberegler(min, max, step) → a number with that range.
    if let Node::Call(cmd, _, args) = match &parsed.body {
        Body::Expr(n) => n,
        _ => &Node::Num(0.0),
    } && lang::command(cmd) == Some("slider")
    {
        let nums: Vec<f64> = args.iter().filter_map(|a| if let Node::Num(v) = a { Some(*v) } else if let Node::Neg(b) = a { if let Node::Num(v) = **b { Some(-v) } else { None } } else { None }).collect();
        let (min, max) = (nums.first().copied().unwrap_or(-5.0), nums.get(1).copied().unwrap_or(5.0));
        let step = nums.get(2).copied().unwrap_or(0.1);
        style.slider = Some(Slider { min, max, step });
        def = format!("{name} = {}", fmt_plain(num(&val).unwrap_or(1.0)));
    } else if kind == Kind::Number && matches!(&parsed.body, Body::Expr(Node::Num(_)) | Body::Expr(Node::Neg(_))) {
        // A plain number: a slider around it, like GeoGebra.
        let v = num(&val).unwrap_or(1.0);
        let r = (v.abs() * 2.0).max(5.0);
        style.slider = Some(Slider { min: -r, max: r, step: if v.fract() == 0.0 { 0.1 } else { 0.01 } });
    }
    // Points show their name, like in GeoGebra.
    if matches!(kind, Kind::Point | Kind::Points) {
        style.label = LabelMode::Name;
    }
    if kind == Kind::Angle {
        style.label = LabelMode::Value;
    }
    Ok((def, style))
}

/// A number as written into a definition (dot decimals, so it reads back exactly).
pub fn fmt_plain(v: f64) -> String {
    let s = format!("{v:.6}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    if s == "-0" { "0".into() } else { s }
}

/// Whether an object's definition is a free point (A = (1, 2)): it can be dragged anywhere.
pub fn free_point(def: &str) -> bool {
    let Some(name) = name_of(def) else { return false };
    let Ok(input) = lang::parse_input(def, &HashSet::from([name])) else { return false };
    let plain = |n: &Node| matches!(n, Node::Num(_)) || matches!(n, Node::Neg(b) if matches!(**b, Node::Num(_)));
    matches!(input.body, Body::Expr(Node::Point(ref a, ref b)) if plain(a) && plain(b))
}

/// A free point's definition moved to (x, y).
pub fn moved_point(def: &str, x: f64, y: f64) -> String {
    let name = name_of(def).unwrap_or_else(|| "A".into());
    format!("{name} = ({}, {})", fmt_plain(x), fmt_plain(y))
}

/// How an object came about, written like GeoGebra lists it under the value ("Schneide(f, c, 1)",
/// "f'(x)", "k sin(x)"); None for a free object (a number, a point, a function or equation of
/// its own), where the value says it all. `known`: the names of the construction.
pub fn definition(def: &str, known: &HashSet<String>) -> Option<String> {
    let input = lang::parse_input(def, known).ok()?;
    let own = input.name.clone().unwrap_or_default();
    let mut refs = Vec::new();
    refs_of(&input, &mut refs);
    let command = match &input.body {
        Body::Expr(Node::Call(c, _, _)) => lang::command(c).filter(|c| !matches!(*c, "text" | "slider")),
        _ => None,
    };
    if command.is_none() && !refs.iter().any(|r| *r != own && known.contains(r)) {
        return None;
    }
    Some(match &input.body {
        Body::Expr(n) => pretty(n, 0),
        Body::Equation(l, r) => format!("{} = {}", pretty(l, 0), pretty(r, 0)),
    })
}

/// A parsed input written out for reading: German decimal comma, ² ³, implicit products
/// (2x, k sin(x)), √, π, (a | b).
fn pretty(n: &Node, ctx: u8) -> String {
    let atom = |n: &Node| matches!(n, Node::Num(_) | Node::Var(_) | Node::Name(_) | Node::Call(..) | Node::Point(..) | Node::List(_) | Node::Text(_));
    let (s, prec) = match n {
        Node::Num(v) => {
            let t = fmt_plain(*v).replace('.', ",");
            (if t.starts_with('-') { t.replacen('-', "−", 1) } else { t }, 5)
        }
        Node::Var(c) => (c.to_string(), 5),
        Node::Name(s) => (if s == "pi" { "π".into() } else { s.clone() }, 5),
        Node::Neg(a) => (format!("−{}", pretty(a, 3)), 3),
        Node::Bin(Op::Add, a, b) => match &**b {
            Node::Neg(c) => (format!("{} − {}", pretty(a, 1), pretty(c, 2)), 1),
            _ => (format!("{} + {}", pretty(a, 1), pretty(b, 1)), 1),
        },
        Node::Bin(Op::Sub, a, b) => (format!("{} − {}", pretty(a, 1), pretty(b, 2)), 1),
        Node::Bin(Op::Mul, a, b) => {
            let (l, r) = (pretty(a, 2), pretty(b, 2));
            let letter_first = r.starts_with(|c: char| c.is_alphabetic() || c == '(' || c == '√' || c == 'π');
            let tight = matches!(**a, Node::Num(_)) && letter_first && matches!(**b, Node::Var(_) | Node::Bin(Op::Pow, ..) | Node::Name(_));
            let spaced = matches!(**a, Node::Num(_) | Node::Name(_) | Node::Var(_)) && letter_first && (atom(b) || matches!(**b, Node::Bin(Op::Pow, ..)));
            (if tight { format!("{l}{r}") } else if spaced { format!("{l} {r}") } else { format!("{l} · {r}") }, 2)
        }
        Node::Bin(Op::Div, a, b) => (format!("{} / {}", pretty(a, 2), pretty(b, 3)), 2),
        Node::Bin(Op::Pow, a, b) => {
            let base = pretty(a, 5);
            (match **b {
                Node::Num(e) if e == 2.0 => format!("{base}²"),
                Node::Num(e) if e == 3.0 => format!("{base}³"),
                _ => format!("{base}^{}", pretty(b, 5)),
            }, 4)
        }
        Node::Call(name, primes, args) => {
            let list = args.iter().map(|a| pretty(a, 0)).collect::<Vec<_>>().join(", ");
            (match name.to_lowercase().as_str() {
                "sqrt" => format!("√({list})"),
                "abs" => format!("|{list}|"),
                _ => format!("{name}{}({list})", "'".repeat(*primes as usize)),
            }, 5)
        }
        Node::Point(a, b) => (format!("({} | {})", pretty(a, 0), pretty(b, 0)), 5),
        Node::Chain(items, cmps) => {
            let mut out = pretty(&items[0], 1);
            for (c, it) in cmps.iter().zip(&items[1..]) {
                let sym = match c {
                    Cmp::Lt => "<",
                    Cmp::Le => "≤",
                    Cmp::Gt => ">",
                    Cmp::Ge => "≥",
                    Cmp::Eq => "=",
                };
                out = format!("{out} {sym} {}", pretty(it, 1));
            }
            (out, 0)
        }
        Node::Text(t) => (format!("\"{t}\""), 5),
        Node::List(items) => (format!("{{{}}}", items.iter().map(|a| pretty(a, 0)).collect::<Vec<_>>().join(", ")), 5),
    };
    if prec < ctx { format!("({s})") } else { s }
}

/// A line being typed, set as LaTeX for the preview under the input (GeoGebra shows what it
/// will be while typing): fractions stacked, powers raised, roots drawn, commands upright.
/// None while it doesn't read yet. `known`: the construction's names.
pub fn latex_input(src: &str, known: &HashSet<String>) -> Option<String> {
    let input = lang::parse_input(src, known).ok()?;
    let body = match &input.body {
        Body::Expr(n) => tex(n, 0),
        Body::Equation(l, r) => format!("{}={}", tex(l, 0), tex(r, 0)),
    };
    Some(match (&input.name, input.param) {
        (Some(n), Some(p)) => format!("{}({p})={body}", latex_name(n)),
        (Some(n), None) => format!("{}{{:}}\\;{body}", latex_name(n)),
        _ => body,
    })
}

/// A parsed input as LaTeX (see [`latex_input`]); `ctx`: the binding around it (brackets when weaker).
fn tex(n: &Node, ctx: u8) -> String {
    let (s, prec) = match n {
        Node::Num(v) => (latex_num(*v), if *v < 0.0 { 3 } else { 5 }),
        Node::Var(c) => (c.to_string(), 5),
        Node::Name(s) => (match s.as_str() {
            "pi" => "\\pi".into(),
            "e" => "e".into(),
            _ => latex_name(s),
        }, 5),
        Node::Neg(a) => (format!("-{}", tex(a, 3)), 3),
        Node::Bin(Op::Add, a, b) => match &**b {
            Node::Neg(c) => (format!("{}-{}", tex(a, 1), tex(c, 2)), 1),
            _ => (format!("{}+{}", tex(a, 1), tex(b, 1)), 1),
        },
        Node::Bin(Op::Sub, a, b) => (format!("{}-{}", tex(a, 1), tex(b, 2)), 1),
        Node::Bin(Op::Mul, a, b) => {
            let (l, r) = (tex(a, 2), tex(b, 2));
            // 2x, k sin(x): no dot; 2 · 3: a dot.
            let dot = matches!(**b, Node::Num(_)) || r.starts_with(|c: char| c.is_ascii_digit() || c == '-');
            (if dot { format!("{l}\\cdot {r}") } else { format!("{l}\\,{r}") }, 2)
        }
        Node::Bin(Op::Div, a, b) => (format!("\\frac{{{}}}{{{}}}", tex(a, 0), tex(b, 0)), 4),
        Node::Bin(Op::Pow, a, b) => (format!("{}^{{{}}}", tex(a, 5), tex(b, 0)), 4),
        Node::Call(name, primes, args) => {
            let list = args.iter().map(|a| tex(a, 0)).collect::<Vec<_>>().join(",\\,");
            let lower = name.to_lowercase();
            (match lower.as_str() {
                "sqrt" => format!("\\sqrt{{{list}}}"),
                "nroot" if args.len() == 2 => format!("\\sqrt[{}]{{{}}}", tex(&args[1], 0), tex(&args[0], 0)),
                "abs" => format!("\\left|{list}\\right|"),
                "sin" | "cos" | "tan" | "ln" | "log" | "exp" => format!("\\{lower}{}", brackets(&list)),
                _ if lang::command(name).is_some() => format!("\\mathrm{{{name}}}{}", brackets(&list)),
                _ => format!("{}{}{}", latex_name(name), "'".repeat(*primes as usize), brackets(&list)),
            }, 5)
        }
        Node::Point(a, b) => (format!("\\left({}\\,\\middle|\\,{}\\right)", tex(a, 0), tex(b, 0)), 5),
        Node::Chain(items, cmps) => {
            let mut out = tex(&items[0], 1);
            for (c, it) in cmps.iter().zip(&items[1..]) {
                let sym = match c {
                    Cmp::Lt => "<",
                    Cmp::Le => "\\le ",
                    Cmp::Gt => ">",
                    Cmp::Ge => "\\ge ",
                    Cmp::Eq => "=",
                };
                out = format!("{out}{sym}{}", tex(it, 1));
            }
            (out, 0)
        }
        Node::Text(t) => (latex_text(t), 5),
        Node::List(items) => (format!("\\left\\{{{}\\right\\}}", items.iter().map(|a| tex(a, 0)).collect::<Vec<_>>().join(",\\,")), 5),
    };
    if prec < ctx { brackets(&s) } else { s }
}

/// A point on an object (Punkt(f, 1.5)): its definition with the place on the object changed.
pub fn moved_on(def: &str, t: f64) -> Option<String> {
    let eq = def.find('=')?;
    let body = def[eq + 1..].trim();
    let open = body.find(['(', '['])?;
    if lang::command(body[..open].trim()) != Some("point") {
        return None;
    }
    let comma = def.rfind(',')?;
    let close = if body.ends_with(']') { "]" } else { ")" };
    Some(format!("{}, {}{close}", &def[..comma], fmt_plain(t)))
}

#[cfg(test)]
mod tests {
    #[test]
    fn what_is_typed_is_set_as_latex_while_typing() {
        let known: std::collections::HashSet<String> = ["f", "A", "B"].iter().map(|s| s.to_string()).collect();
        let t = |s: &str| super::latex_input(s, &known);
        assert_eq!(t("f(x) = x^2/3").as_deref(), Some("f(x)=\\frac{x^{2}}{3}"));
        assert_eq!(t("g(x)=sqrt(x+1)").as_deref(), Some("g(x)=\\sqrt{x+1}"));
        assert!(t("Gerade(A, B)").unwrap().contains("\\mathrm{Gerade}"));
        assert!(t("2x+3").unwrap().starts_with("2\\,x+3"));
        // Not readable yet (half typed): nothing.
        assert_eq!(t("f(x) = (x+"), None);
    }

    #[test]
    fn rows_are_set_as_latex() {
        let p = |defs: &[&str]| {
            let objs: Vec<Obj> = defs.iter().enumerate().map(|(i, d)| Obj { id: i.to_string(), def: d.to_string(), style: Style::default(), order: i as f64 }).collect();
            let vals = evaluate(&objs, ViewRange { x0: -5.0, x1: 5.0, y0: -5.0, y1: 5.0 });
            objs.iter().zip(vals).map(|(o, v)| latex_row(&o.name(), &v.unwrap())).collect::<Vec<_>>()
        };
        let rows = p(&["f(x) = 0.25x^3 - 2x", "g(x) = 1/x + sqrt(x)", "A = (1, -2.5)", "u = Vektor((0,0), (3, 4))", "h: y = 0.5x - 1", "α = Winkel((1,0), (0,0), (0,1))", "S_1 = (2, 3)", "t = Text(\"a_b {x}\")", "vieleck1 = Vieleck((0,0), (1,0), (0,1))"]);
        assert_eq!(rows[0], "f(x) = 0{,}25 x^{3} - 2 x");
        assert_eq!(rows[1], "g(x) = \\frac{1}{x} + \\sqrt{x}");
        assert_eq!(rows[2], "A = (1 \\mid -2{,}5)");
        assert_eq!(rows[3], "u = \\begin{pmatrix}3 \\\\ 4\\end{pmatrix}");
        assert_eq!(rows[4], "h{:}\\; y = 0{,}5x - 1");
        assert_eq!(rows[5], "\\alpha = 90^{\\circ}");
        assert_eq!(rows[6], "S_{1} = (2 \\mid 3)");
        assert_eq!(rows[7], "\\text{a\\_b \\{x\\}}");
        assert_eq!(rows[8], "\\mathit{vieleck1} = 0{,}5");
    }

    #[test]
    fn definitions_show_how_dependent_objects_came_about() {
        let known: HashSet<String> = ["f", "g", "c", "A", "B", "k", "p", "h"].iter().map(|s| s.to_string()).collect();
        let d = |s: &str| definition(s, &known);
        assert_eq!(d("A = (1, 1)"), None, "free: the value says it");
        assert_eq!(d("f(x) = 0.25x^3-2x"), None);
        assert_eq!(d("g: y = 2x + 1"), None);
        assert_eq!(d("k = 4.5"), None);
        assert_eq!(d("t = Text(\"Hallo\")"), None);
        assert_eq!(d("B = Schneide(f,c, 1)").as_deref(), Some("Schneide(f, c, 1)"));
        assert_eq!(d("g(x) = f'(x)").as_deref(), Some("f'(x)"));
        assert_eq!(d("p(x)=k*sin(x)").as_deref(), Some("k sin(x)"));
        assert_eq!(d("h(x) = 0.5f(x) + k x^2 - 1").as_deref(), Some("0,5 f(x) + k x² − 1"));
        assert_eq!(d("c = Kreis((0,0), 3)").as_deref(), Some("Kreis((0 | 0), 3)"), "a command, even without objects");
        assert_eq!(d("h = Gerade(A, B)").as_deref(), Some("Gerade(A, B)"));
        assert_eq!(d("p(x) = sqrt(x - k) / (2 + k)").as_deref(), Some("√(x − k) / (2 + k)"));
    }

    use super::*;

    fn obj(def: &str) -> Obj {
        Obj { id: def.into(), def: def.into(), style: Style::default(), order: 0.0 }
    }

    const VIEW: ViewRange = ViewRange { x0: -6.0, x1: 6.0, y0: -4.0, y1: 4.0 };

    fn vals(defs: &[&str]) -> Vec<Result<Val, String>> {
        evaluate(&defs.iter().map(|d| obj(d)).collect::<Vec<_>>(), VIEW)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn points_lines_circles_and_what_they_make() {
        let v = vals(&["A = (1, 2)", "B = (4, 6)", "g = Gerade(A, B)", "s = Strecke(A, B)", "c = Kreis(A, 5)", "M = Mittelpunkt(A, B)", "l = Länge(s)", "P = Schneide(c, g)", "h = Senkrechte(A, g)"]);
        assert!(v.iter().all(|r| r.is_ok()), "{v:?}");
        let Ok(Val::Line { a, b, c }) = &v[2] else { panic!() };
        assert!(close(a * 1.0 + b * 2.0, *c) && close(a * 4.0 + b * 6.0, *c));
        assert_eq!(v[5], Ok(Val::Point((2.5, 4.0))));
        assert_eq!(v[6], Ok(Val::Num(5.0)));
        // The circle about A with radius 5 meets the line AB at B and at A − (3, 4).
        let Ok(Val::Points(ps)) = &v[7] else { panic!("{:?}", v[7]) };
        assert_eq!(ps.len(), 2);
        assert!(ps.iter().any(|p| close(p.0, 4.0) && close(p.1, 6.0)) && ps.iter().any(|p| close(p.0, -2.0) && close(p.1, -2.0)));
        // The perpendicular through A: slope −3/4.
        let Ok(Val::Line { a, b, .. }) = &v[8] else { panic!() };
        assert!(close(-a / b, -0.75));
    }

    #[test]
    fn functions_sliders_and_analysis() {
        let v = vals(&["a = 2", "f(x) = a x^2 - 2", "g: y = x", "S = Schneide(f, g)", "N = Nullstelle(f)", "T = Extremum(f)", "f1 = Ableitung(f)", "t = Tangente(1, f)", "A_1 = Integral(f, 0, 1)"]);
        assert!(v.iter().all(|r| r.is_ok()), "{v:?}");
        let Ok(Val::Func(f)) = &v[1] else { panic!() };
        assert!(close(f.eval(3.0), 16.0));
        assert!(matches!(v[2], Ok(Val::Line { .. })), "y = x is a line");
        let Ok(Val::Points(s)) = &v[3] else { panic!("{:?}", v[3]) };
        assert_eq!(s.len(), 2);
        let Ok(Val::Points(n)) = &v[4] else { panic!() };
        assert!(close(n[1].0, 1.0) && close(n[0].0, -1.0));
        assert_eq!(v[5], Ok(Val::Point((0.0, -2.0))));
        let Ok(Val::Line { a, b, c }) = &v[7] else { panic!() };
        // Tangent at 1: slope 4 through (1, 0).
        assert!(close(-a / b, 4.0) && close(c / b, -4.0));
        let Ok(Val::Area { value, .. }) = &v[8] else { panic!() };
        assert!((value - (2.0 / 3.0 - 2.0)).abs() < 1e-9);
        // A function on a domain; Wenn in a function.
        let v = vals(&["f(x) = Funktion(x^2, 0, 2)", "g(x) = Wenn(x < 0, -x, x^2)"]);
        let Ok(Val::Func(f)) = &v[0] else { panic!() };
        assert!(f.eval(-1.0).is_nan() && close(f.eval(1.0), 1.0));
        let Ok(Val::Func(g)) = &v[1] else { panic!() };
        assert!(close(g.eval(-3.0), 3.0) && close(g.eval(3.0), 9.0));
    }

    #[test]
    fn equations_become_lines_functions_or_curves() {
        let v = vals(&["g: 2x + 3y = 6", "h: x = 2", "k: y = x^2", "c: x^2 + y^2 = 4", "d: (x - 1)^2 + (y + 2)^2 = 9", "e2: x^2 + 4y^2 = 4", "S = Schneide(e2, m)", "m: y = 0.5x"]);
        assert!(matches!(v[0], Ok(Val::Line { .. })));
        let Ok(Val::Line { a, b, c }) = v[1].clone() else { panic!() };
        assert!(b.abs() < 1e-12 && close(c / a, 2.0));
        assert!(matches!(v[2], Ok(Val::Func(_))));
        assert_eq!(v[3], Ok(Val::Circle((0.0, 0.0), 2.0)), "a circle's equation is a circle");
        let Ok(Val::Circle(m, r)) = v[4] else { panic!("{:?}", v[4]) };
        assert!(close(m.0, 1.0) && close(m.1, -2.0) && close(r, 3.0));
        let Ok(Val::Implicit(f)) = &v[5] else { panic!("{:?}", v[5]) };
        assert!(close(f.eval2(2.0, 0.0), 0.0));
        let Ok(Val::Points(s)) = &v[6] else { panic!("{:?}", v[6]) };
        assert_eq!(s.len(), 2, "an ellipse and a line meet twice");
        assert!(s.iter().all(|p| (p.0 * p.0 + 4.0 * p.1 * p.1 - 4.0).abs() < 1e-6));
    }

    #[test]
    fn mistakes_and_loops_are_named() {
        let v = vals(&["A = (1, 2)", "B = Gerade(A, A)", "f(x) = g(x) + 1", "g(x) = f(x)", "h = q + 1", "A = (3, 3)"]);
        assert_eq!(v[1], Err("Die Punkte liegen aufeinander".into()));
        assert!(v[2].as_ref().unwrap_err().contains("hängt von sich selbst ab"), "{:?}", v[2]);
        assert!(v[4].is_err());
        assert_eq!(v[5], Err("Den Namen A gibt es schon".into()));
    }

    #[test]
    fn new_objects_get_names_and_sliders() {
        let objs = vec![obj("A = (1, 2)"), obj("f(x) = x^2")];
        let (def, style) = new_object("(3, 4)", &objs, VIEW).unwrap();
        assert_eq!(def, "B = (3, 4)");
        assert_eq!(style.label, LabelMode::Name);
        assert_eq!(new_object("x^3", &objs, VIEW).unwrap().0, "g = x^3");
        assert_eq!(new_object("y = 2x + 1", &objs, VIEW).unwrap().0, "g: y = 2x + 1");
        let (def, style) = new_object("Schieberegler(-3, 3, 0.5)", &objs, VIEW).unwrap();
        assert_eq!(def, "a = 1");
        assert_eq!(style.slider, Some(Slider { min: -3.0, max: 3.0, step: 0.5 }));
        assert!(new_object("k = 2", &objs, VIEW).unwrap().1.slider.is_some(), "a plain number is a slider");
        assert!(new_object("A = (0, 0)", &objs, VIEW).is_err());
        assert!(new_object("Gerade(A)", &objs, VIEW).is_err());
        assert!(free_point("A = (1, -2)") && !free_point("M = Mittelpunkt(A, B)"));
        assert_eq!(moved_point("A = (1, 2)", 1.5, -0.25), "A = (1.5, -0.25)");
        assert_eq!(moved_on("P = Punkt(f, 1)", 2.5).unwrap(), "P = Punkt(f, 2.5)");
        assert_eq!(renamed("f(x) = x^2", "g"), "g(x) = x^2");
    }

    #[test]
    fn values_are_written_like_in_school() {
        assert_eq!(format_val(&Val::Point((1.5, -2.0))), "(1,5 | −2)");
        let f = lang::parse_expr("0.5x^2 - 3", &HashSet::new()).unwrap();
        let mut env = Env { vals: HashMap::new(), view: VIEW };
        let e = env.fexpr(&f).unwrap().simplified();
        assert_eq!(fmt_expr(&e), "0,5x² − 3");
        assert_eq!(fmt_line(-2.0, 1.0, 1.0), "y = 2x + 1");
        assert_eq!(fmt_line(1.0, 0.0, 2.0), "x = 2");
        assert_eq!(format_val(&Val::Circle((1.0, -2.0), 3.0)), "(x − 1)² + (y + 2)² = 9");
        assert_eq!(format_val(&Val::Angle { at: (0.0, 0.0), from: 0.0, sweep: PI / 2.0 }), "90°");
    }
}
