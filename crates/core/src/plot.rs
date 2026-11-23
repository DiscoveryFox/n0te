//! Coordinate systems with function graphs. A plot is an object on a page — a box, the ranges
//! it shows, its functions — and the core works out how it looks as ink: axes, ticks, the
//! numbers (in a little hand-written stroke font), the curves. So every app and the PDF export
//! draw it the same way, with the pen's feel rather than a printout's, and it stays editable.

use crate::ink::{Brush, InkPoint, KIND_PEN};

/// One function of a plot: what was typed (`x^2 - 1`, `f(x) = 2sin x`) and its ink.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotFn {
    pub expr: String,
    pub color: u32,
}

/// A coordinate system on a page: its box (page points), the ranges shown, its functions.
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
}

impl Plot {
    /// A plot in the box with ranges of `unit` page points per unit (equal on both axes),
    /// the origin in the middle: the plot's frame on squared paper when `unit` is the squares.
    pub fn centered(x: f32, y: f32, w: f32, h: f32, unit: f32, color: u32, width: f32) -> Plot {
        let (hx, hy) = ((w / unit / 2.0) as f64, (h / unit / 2.0) as f64);
        Plot { id: String::new(), x, y, w, h, x_min: -hx, x_max: hx, y_min: -hy, y_max: hy, grid: false, clean: false, color, width, fns: Vec::new() }
    }

    /// Whether the ranges and the box can be drawn.
    pub fn is_valid(&self) -> bool {
        let fin = |v: f64| v.is_finite();
        fin(self.x_min) && fin(self.x_max) && fin(self.y_min) && fin(self.y_max) && self.x_max > self.x_min && self.y_max > self.y_min && self.w > 1.0 && self.h > 1.0
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
        for f in &self.fns {
            let Ok(e) = parse(&f.expr) else { continue };
            for i in 0..=400 {
                let x = self.x_min + (self.x_max - self.x_min) * i as f64 / 400.0;
                let y = e.eval(x);
                if y.is_finite() {
                    ys.push(y);
                }
            }
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
}

// --- expressions -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Func {
    Sin,
    Cos,
    Tan,
    Cot,
    Asin,
    Acos,
    Atan,
    Sinh,
    Cosh,
    Tanh,
    Exp,
    Ln,
    Log,
    Sqrt,
    Abs,
    Sgn,
    Floor,
    Ceil,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    X,
    Neg(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
    Call(Func, Box<Expr>),
}

impl Expr {
    /// The value at `x` (NaN outside the domain, ±∞ at poles).
    pub fn eval(&self, x: f64) -> f64 {
        match self {
            Expr::Num(v) => *v,
            Expr::X => x,
            Expr::Neg(a) => -a.eval(x),
            Expr::Add(a, b) => a.eval(x) + b.eval(x),
            Expr::Sub(a, b) => a.eval(x) - b.eval(x),
            Expr::Mul(a, b) => a.eval(x) * b.eval(x),
            Expr::Div(a, b) => a.eval(x) / b.eval(x),
            Expr::Pow(a, b) => pow(a.eval(x), b.eval(x)),
            Expr::Call(f, a) => {
                let v = a.eval(x);
                match f {
                    Func::Sin => v.sin(),
                    Func::Cos => v.cos(),
                    Func::Tan => v.tan(),
                    Func::Cot => 1.0 / v.tan(),
                    Func::Asin => v.asin(),
                    Func::Acos => v.acos(),
                    Func::Atan => v.atan(),
                    Func::Sinh => v.sinh(),
                    Func::Cosh => v.cosh(),
                    Func::Tanh => v.tanh(),
                    Func::Exp => v.exp(),
                    Func::Ln => v.ln(),
                    Func::Log => v.log10(),
                    Func::Sqrt => v.sqrt(),
                    Func::Abs => v.abs(),
                    Func::Sgn => {
                        if v > 0.0 {
                            1.0
                        } else if v < 0.0 {
                            -1.0
                        } else {
                            0.0
                        }
                    }
                    Func::Floor => v.floor(),
                    Func::Ceil => v.ceil(),
                }
            }
        }
    }
}

/// `a` to the power of `b`; an odd root of a negative number is real (x^(1/3) for x < 0).
fn pow(a: f64, b: f64) -> f64 {
    if a < 0.0 && b.fract() != 0.0 {
        // An exponent p/q with q odd: −|a|^(p/q) or |a|^(p/q).
        if let Some((n, d)) = as_fraction(b)
            && d % 2 == 1
        {
            let r = a.abs().powf(b);
            return if n % 2 == 0 { r } else { -r };
        }
        return f64::NAN;
    }
    a.powf(b)
}

fn as_fraction(v: f64) -> Option<(i64, i64)> {
    for d in 1..=12i64 {
        let n = v * d as f64;
        if (n - n.round()).abs() < 1e-9 {
            return Some((n.round() as i64, d));
        }
    }
    None
}

/// Whether `expr` can be drawn: Ok, or what is wrong (German, for the app).
pub fn check(expr: &str) -> Result<(), String> {
    parse(expr).map(|_| ())
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Name(&'static str),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Open,
    Close,
    Bar,
}

const NAMES: &[&str] = &[
    "arcsin", "arccos", "arctan", "betrag", "wurzel", "floor", "asin", "acos", "atan", "sinh", "cosh", "tanh", "sqrt", "ceil", "sign", "sin", "cos",
    "tan", "cot", "exp", "abs", "sgn", "log", "ln", "lg", "pi", "e", "x",
];

/// Reads what was typed: `x^2`, `2x`, `3sin x`, `sin(2x)^2`, `|x|`, `√x`, `e^-x`, `0,5x`,
/// `f(x) = …` (only the right side counts).
pub fn parse(src: &str) -> Result<Expr, String> {
    let body = src.rsplit('=').next().unwrap_or(src);
    let toks = lex(body)?;
    if toks.is_empty() {
        return Err("Leer".into());
    }
    let mut p = Parser { toks, i: 0, abs: 0 };
    let e = p.expr()?;
    if p.i < p.toks.len() {
        return Err(match p.toks[p.i] {
            Tok::Close => "Eine Klammer „)“ zu viel".into(),
            Tok::Bar => "Ein Betragsstrich zu viel".into(),
            _ => "Da stimmt etwas nicht".into(),
        });
    }
    Ok(e)
}

fn lex(src: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' => {}
            '0'..='9' | '.' | ',' => {
                let start = i;
                let mut s = String::new();
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == ',') {
                    // A comma is a decimal point only between digits ("0,5"); a dot anywhere.
                    if chars[i] == ',' && !(i > start && i + 1 < chars.len() && chars[i + 1].is_ascii_digit()) {
                        return Err("Komma nur in Zahlen (0,5)".into());
                    }
                    s.push(if chars[i] == ',' { '.' } else { chars[i] });
                    i += 1;
                }
                out.push(Tok::Num(s.parse::<f64>().map_err(|_| format!("„{s}“ ist keine Zahl"))?));
                continue;
            }
            '+' => out.push(Tok::Plus),
            '-' | '−' | '–' => out.push(Tok::Minus),
            '*' | '·' | '×' | '⋅' => {
                if c == '*' && chars.get(i + 1) == Some(&'*') {
                    out.push(Tok::Caret);
                    i += 1;
                } else {
                    out.push(Tok::Star);
                }
            }
            '/' | '÷' | ':' => out.push(Tok::Slash),
            '^' => out.push(Tok::Caret),
            '(' | '[' | '{' => out.push(Tok::Open),
            ')' | ']' | '}' => out.push(Tok::Close),
            '|' => out.push(Tok::Bar),
            'π' => out.push(Tok::Name("pi")),
            '√' => out.push(Tok::Name("sqrt")),
            '²' | '³' => {
                out.push(Tok::Caret);
                out.push(Tok::Num(if c == '²' { 2.0 } else { 3.0 }));
            }
            c if c.is_alphabetic() => {
                let start = i;
                while i < chars.len() && chars[i].is_alphabetic() {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect::<String>().to_lowercase();
                // A run of letters is read as known names one after the other ("sinx" = sin x).
                let mut rest = word.as_str();
                while !rest.is_empty() {
                    let Some(name) = NAMES.iter().find(|n| rest.starts_with(**n)) else {
                        let cut = (1..rest.len()).find(|&k| rest.is_char_boundary(k) && NAMES.iter().any(|n| rest[k..].starts_with(*n))).unwrap_or(rest.len());
                        let bad: String = rest[..cut].chars().take(8).collect();
                        return Err(format!("„{bad}“ kenne ich nicht (nur x als Variable)"));
                    };
                    out.push(Tok::Name(name));
                    rest = &rest[name.len()..];
                }
                continue;
            }
            other => return Err(format!("Zeichen „{other}“ kenne ich nicht")),
        }
        i += 1;
    }
    Ok(out)
}

struct Parser {
    toks: Vec<Tok>,
    i: usize,
    /// Open absolute-value bars (a bar after a value closes one when there is one open).
    abs: usize,
}

fn func_of(name: &str) -> Option<Func> {
    Some(match name {
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
        _ => return None,
    })
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i)
    }

    fn expr(&mut self) -> Result<Expr, String> {
        let mut e = self.term()?;
        loop {
            match self.peek() {
                Some(Tok::Plus) => {
                    self.i += 1;
                    e = Expr::Add(Box::new(e), Box::new(self.term()?));
                }
                Some(Tok::Minus) => {
                    self.i += 1;
                    e = Expr::Sub(Box::new(e), Box::new(self.term()?));
                }
                _ => return Ok(e),
            }
        }
    }

    /// Whether the next token starts a value right after one (implicit multiplication).
    fn starts_value(&self) -> bool {
        match self.peek() {
            Some(Tok::Num(_)) | Some(Tok::Name(_)) | Some(Tok::Open) => true,
            Some(Tok::Bar) => self.abs == 0,
            _ => false,
        }
    }

    fn term(&mut self) -> Result<Expr, String> {
        let mut e = self.unary()?;
        loop {
            match self.peek() {
                Some(Tok::Star) => {
                    self.i += 1;
                    e = Expr::Mul(Box::new(e), Box::new(self.unary()?));
                }
                Some(Tok::Slash) => {
                    self.i += 1;
                    e = Expr::Div(Box::new(e), Box::new(self.unary()?));
                }
                _ if self.starts_value() => e = Expr::Mul(Box::new(e), Box::new(self.power()?)),
                _ => return Ok(e),
            }
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.i += 1;
                Ok(Expr::Neg(Box::new(self.unary()?)))
            }
            Some(Tok::Plus) => {
                self.i += 1;
                self.unary()
            }
            _ => self.power(),
        }
    }

    fn power(&mut self) -> Result<Expr, String> {
        let base = self.atom()?;
        if self.peek() == Some(&Tok::Caret) {
            self.i += 1;
            let exp = self.unary()?;
            return Ok(Expr::Pow(Box::new(base), Box::new(exp)));
        }
        Ok(base)
    }

    fn atom(&mut self) -> Result<Expr, String> {
        let Some(t) = self.peek().cloned() else { return Err("Da fehlt noch etwas am Ende".into()) };
        self.i += 1;
        match t {
            Tok::Num(v) => Ok(Expr::Num(v)),
            Tok::Name("x") => Ok(Expr::X),
            Tok::Name("pi") => Ok(Expr::Num(std::f64::consts::PI)),
            Tok::Name("e") => Ok(Expr::Num(std::f64::consts::E)),
            Tok::Name(n) => {
                let f = func_of(n).ok_or_else(|| format!("„{n}“ kenne ich nicht"))?;
                // sin(…) takes the bracket; sin x, sin 2x, sin x² take what follows up to
                // the next operator (sin x cos x = sin(x)·cos(x)).
                if self.peek() == Some(&Tok::Open) {
                    self.i += 1;
                    let a = self.expr()?;
                    if self.peek() != Some(&Tok::Close) {
                        return Err("Eine Klammer „)“ fehlt".into());
                    }
                    self.i += 1;
                    return Ok(Expr::Call(f, Box::new(a)));
                }
                if self.peek().is_none() {
                    return Err(format!("Hinter „{n}“ fehlt noch etwas"));
                }
                let mut a = if self.peek() == Some(&Tok::Minus) {
                    self.i += 1;
                    Expr::Neg(Box::new(self.power()?))
                } else {
                    self.power()?
                };
                while matches!(self.peek(), Some(Tok::Num(_)) | Some(Tok::Name("x")) | Some(Tok::Name("pi")) | Some(Tok::Name("e"))) {
                    a = Expr::Mul(Box::new(a), Box::new(self.power()?));
                }
                Ok(Expr::Call(f, Box::new(a)))
            }
            Tok::Open => {
                let e = self.expr()?;
                if self.peek() != Some(&Tok::Close) {
                    return Err("Eine Klammer „)“ fehlt".into());
                }
                self.i += 1;
                Ok(e)
            }
            Tok::Bar => {
                self.abs += 1;
                let e = self.expr()?;
                self.abs -= 1;
                if self.peek() != Some(&Tok::Bar) {
                    return Err("Ein Betragsstrich „|“ fehlt".into());
                }
                self.i += 1;
                Ok(Expr::Call(Func::Abs, Box::new(e)))
            }
            Tok::Close if self.i >= 2 && self.toks[self.i - 2] == Tok::Open => Err("In der Klammer fehlt noch etwas".into()),
            Tok::Close => Err("Eine Klammer „)“ zu viel".into()),
            Tok::Plus | Tok::Minus | Tok::Star | Tok::Slash | Tok::Caret => Err("Zwei Rechenzeichen hintereinander".into()),
        }
    }
}

// --- sampling ---------------------------------------------------------------------------------

/// The graph of `e` in page points, clipped to the plot's box: pieces wherever it is defined
/// and doesn't jump (poles, steps).
pub fn curve(e: &Expr, p: &Plot) -> Vec<Vec<(f32, f32)>> {
    if !p.is_valid() {
        return Vec::new();
    }
    let (x0, x1) = (p.x_min, p.x_max);
    let sx = p.w as f64 / (x1 - x0);
    let sy = p.h as f64 / (p.y_max - p.y_min);
    let to_page = |x: f64, y: f64| (p.x as f64 + (x - x0) * sx, p.y as f64 + (p.y_max - y) * sy);
    let n = ((p.w / 3.0) as usize).clamp(48, 400);
    // Samples (x, y) and where the curve breaks (None).
    let mut pts: Vec<Option<(f64, f64)>> = Vec::new();
    /// How finely to sample: page scale, how far off the straight line a piece may be, the
    /// largest step that is still a line at the finest level, the y band that is seen.
    struct Ctx {
        sx: f64,
        sy: f64,
        tol: f64,
        jump: f64,
        lo: f64,
        hi: f64,
    }
    let margin = (p.y_max - p.y_min) * 0.05;
    let cx = Ctx { sx, sy, tol: 0.12, jump: (p.h as f64 * 0.35).min(1.5), lo: p.y_min - margin, hi: p.y_max + margin };
    fn refine(e: &Expr, a: (f64, f64), b: (f64, f64), depth: u32, out: &mut Vec<Option<(f64, f64)>>, c: &Ctx) {
        let (ax, ay) = a;
        let (bx, by) = b;
        let mx = (ax + bx) / 2.0;
        let my = e.eval(mx);
        let (fa, fb, fm) = (ay.is_finite(), by.is_finite(), my.is_finite());
        if depth >= 12 {
            // Can't split further: a jump this steep is a pole or a step, not a line.
            if fa && fb && ((ay - by) * c.sy).abs() < c.jump {
                out.push(Some(b));
            } else {
                out.push(None);
                if fb {
                    out.push(Some(b));
                }
            }
            return;
        }
        let split = if fa && fb && fm {
            // Bent, or long where it is seen (dense points draw a smooth pen line).
            let seen = !(ay < c.lo && by < c.lo) && !(ay > c.hi && by > c.hi);
            ((my - (ay + by) / 2.0) * c.sy).abs() > c.tol || (seen && ((bx - ax) * c.sx).hypot((by - ay) * c.sy) > 4.0)
        } else {
            // Where it starts or stops being defined: find the edge.
            fa || fb || fm
        };
        if split {
            refine(e, a, (mx, my), depth + 1, out, c);
            refine(e, (mx, my), b, depth + 1, out, c);
        } else if fb {
            out.push(Some(b));
        } else {
            out.push(None);
        }
    }
    let first = (x0, e.eval(x0));
    pts.push(first.1.is_finite().then_some(first));
    let mut prev = first;
    for i in 1..=n {
        let x = x0 + (x1 - x0) * i as f64 / n as f64;
        let cur = (x, e.eval(x));
        refine(e, prev, cur, 0, &mut pts, &cx);
        prev = cur;
    }
    // Into page points, clipped to the box (a little over the edges for the pen's round end).
    let (top, bottom) = (p.y as f64 - 0.5, (p.y + p.h) as f64 + 0.5);
    let mut out: Vec<Vec<(f32, f32)>> = Vec::new();
    let mut cur: Vec<(f32, f32)> = Vec::new();
    let mut last: Option<(f64, f64)> = None;
    let flush = |cur: &mut Vec<(f32, f32)>, out: &mut Vec<Vec<(f32, f32)>>| {
        if cur.len() >= 2 {
            out.push(std::mem::take(cur));
        } else {
            cur.clear();
        }
    };
    for s in pts {
        let Some((x, y)) = s else {
            flush(&mut cur, &mut out);
            last = None;
            continue;
        };
        let q = to_page(x, y);
        if let Some(pq) = last {
            // The segment pq → q, clipped to the band top..bottom.
            let (mut ax, mut ay, mut bx, mut by) = (pq.0, pq.1, q.0, q.1);
            let inside = |y: f64| y >= top && y <= bottom;
            if !(ay < top && by < top) && !(ay > bottom && by > bottom) {
                let cut = |ax: f64, ay: f64, bx: f64, by: f64, yy: f64| ax + (bx - ax) * (yy - ay) / (by - ay);
                let a_in = inside(ay);
                if !a_in {
                    let yy = if ay < top { top } else { bottom };
                    ax = cut(ax, ay, bx, by, yy);
                    ay = yy;
                }
                if !inside(by) {
                    let yy = if by < top { top } else { bottom };
                    bx = cut(pq.0, pq.1, bx, by, yy);
                    by = yy;
                }
                if !a_in {
                    flush(&mut cur, &mut out);
                    cur.push((ax as f32, ay as f32));
                } else if cur.is_empty() {
                    cur.push((ax as f32, ay as f32));
                }
                cur.push((bx as f32, by as f32));
                if !inside(q.1) {
                    flush(&mut cur, &mut out);
                }
            } else {
                flush(&mut cur, &mut out);
            }
        } else if q.1 >= top && q.1 <= bottom {
            cur.push((q.0 as f32, q.1 as f32));
        }
        last = Some(q);
    }
    flush(&mut cur, &mut out);
    out
}

/// A tick step of 1, 2 or 5 × 10^k that cuts `range` into about `count` parts.
pub fn nice_step(range: f64, count: f64) -> f64 {
    let raw = (range / count.max(1.0)).abs().max(1e-12);
    let mag = 10f64.powf(raw.log10().floor());
    for m in [1.0, 2.0, 5.0, 10.0] {
        if m * mag >= raw * 0.999 {
            return m * mag;
        }
    }
    10.0 * mag
}

/// A tick's number as written: German decimal comma, as many places as the step needs.
pub fn tick_label(v: f64, step: f64) -> String {
    let places = if step >= 0.999 { 0 } else { (-step.log10()).ceil().max(0.0) as usize };
    let v = if v.abs() < step * 1e-6 { 0.0 } else { v };
    let s = format!("{v:.places$}");
    let s = if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s };
    s.replace('.', ",")
}

// --- the look: ink ------------------------------------------------------------------------------

/// Small random numbers, the same for the same plot every time (its look doesn't flicker).
struct Rng(u64);

impl Rng {
    fn new(seed: &str, salt: u64) -> Rng {
        let mut h = 0xcbf2_9ce4_8422_2325u64 ^ salt.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        for b in seed.bytes() {
            h = (h ^ b as u64).wrapping_mul(0x100_0000_01b3);
        }
        Rng(h)
    }
    /// −1 … 1.
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^= z >> 31;
        (z >> 11) as f32 / (1u64 << 53) as f32 * 2.0 - 1.0
    }
}

/// How the hand draws: a line wanders a little sideways and the pressure breathes along it,
/// lightest where the pen lands and lifts.
struct Hand {
    clean: bool,
    rng: Rng,
}

impl Hand {
    fn pressure(&mut self, along: &[f32], total: f32) -> Vec<f32> {
        if self.clean {
            return vec![0.5; along.len()];
        }
        let (ph, wl) = (self.rng.next() * 3.0, 60.0 + self.rng.next() * 25.0);
        along
            .iter()
            .map(|&d| {
                let ends = (d.min(total - d) / 2.5).clamp(0.0, 1.0);
                (0.5 + 0.07 * (d / wl * std::f32::consts::TAU + ph).sin()) * (0.72 + 0.28 * ends)
            })
            .collect()
    }

    /// The points along `path` with the hand's pressure (and, when `wobble` > 0, its sway).
    fn stroke(&mut self, path: &[(f32, f32)], wobble: f32) -> Vec<InkPoint> {
        let mut along = Vec::with_capacity(path.len());
        let mut d = 0.0;
        for (i, p) in path.iter().enumerate() {
            if i > 0 {
                d += (p.0 - path[i - 1].0).hypot(p.1 - path[i - 1].1);
            }
            along.push(d);
        }
        let pr = self.pressure(&along, d);
        let (a1, l1, p1) = (self.rng.next(), 70.0 + self.rng.next() * 30.0, self.rng.next() * 3.0);
        let (a2, l2, p2) = (self.rng.next(), 23.0 + self.rng.next() * 8.0, self.rng.next() * 3.0);
        let sway = if self.clean { 0.0 } else { wobble };
        path.iter()
            .enumerate()
            .map(|(i, &(x, y))| {
                let (nx, ny) = if path.len() < 2 {
                    (0.0, 0.0)
                } else {
                    let (a, b) = (path[i.saturating_sub(1)], path[(i + 1).min(path.len() - 1)]);
                    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                    let l = dx.hypot(dy).max(1e-6);
                    (-dy / l, dx / l)
                };
                let t = along[i];
                // The sway fades out at the ends (where the pen aims).
                let env = (t.min(d - t) / 6.0).clamp(0.0, 1.0);
                let off = sway * env * (0.65 * a1.signum() * (t / l1 * std::f32::consts::TAU + p1).sin() + 0.35 * a2 * (t / l2 * std::f32::consts::TAU + p2).sin());
                InkPoint { x: x + nx * off, y: y + ny * off, pressure: pr[i], t_ms: i as u32 * 8 }
            })
            .collect()
    }

    /// A straight line from `a` to `b`, sampled for the hand's sway.
    fn line(&mut self, a: (f32, f32), b: (f32, f32), wobble: f32) -> Vec<InkPoint> {
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        let n = ((len / 3.0) as usize).max(1);
        let path: Vec<(f32, f32)> = (0..=n).map(|i| (a.0 + (b.0 - a.0) * i as f32 / n as f32, a.1 + (b.1 - a.1) * i as f32 / n as f32)).collect();
        self.stroke(&path, wobble)
    }
}

/// The stroke font of the numbers: each glyph a few lines in a box 1 high (top at 0), with
/// its width. Lowercase letters sit on the line at 1 like the digits.
fn glyph(c: char) -> Option<(f32, Vec<Vec<(f32, f32)>>)> {
    use std::f32::consts::PI;
    // An arc of the ellipse about (cx, cy), angles in degrees, counter-clockwise (y up).
    fn arc(cx: f32, cy: f32, rx: f32, ry: f32, a0: f32, a1: f32) -> Vec<(f32, f32)> {
        let n = (((a1 - a0).abs() / 12.0).ceil() as usize).max(2);
        (0..=n)
            .map(|i| {
                let a = (a0 + (a1 - a0) * i as f32 / n as f32) * PI / 180.0;
                (cx + rx * a.cos(), cy - ry * a.sin())
            })
            .collect()
    }
    fn bez(a: (f32, f32), c: (f32, f32), b: (f32, f32)) -> Vec<(f32, f32)> {
        (0..=10)
            .map(|i| {
                let t = i as f32 / 10.0;
                let u = 1.0 - t;
                (u * u * a.0 + 2.0 * u * t * c.0 + t * t * b.0, u * u * a.1 + 2.0 * u * t * c.1 + t * t * b.1)
            })
            .collect()
    }
    let join = |mut a: Vec<(f32, f32)>, b: Vec<(f32, f32)>| {
        a.extend(b);
        a
    };
    Some(match c {
        '0' => (0.56, vec![arc(0.28, 0.5, 0.25, 0.5, 90.0, 450.0)]),
        '1' => (0.4, vec![vec![(0.04, 0.22), (0.26, 0.0), (0.26, 1.0)]]),
        '2' => (0.56, vec![join(arc(0.27, 0.27, 0.24, 0.25, 160.0, -25.0), vec![(0.03, 1.0), (0.54, 1.0)])]),
        '3' => (0.54, vec![join(arc(0.26, 0.25, 0.22, 0.24, 150.0, -90.0), arc(0.26, 0.735, 0.25, 0.265, 90.0, -150.0))]),
        '4' => (0.58, vec![vec![(0.4, 1.0), (0.4, 0.0), (0.03, 0.68), (0.56, 0.68)]]),
        '5' => (0.54, vec![join(vec![(0.48, 0.0), (0.12, 0.0), (0.1, 0.43)], arc(0.27, 0.69, 0.24, 0.31, 125.0, -145.0))]),
        '6' => (0.56, vec![join(bez((0.46, 0.03), (0.07, 0.1), (0.06, 0.6)), arc(0.29, 0.7, 0.23, 0.3, 172.0, 532.0))]),
        '7' => (0.54, vec![vec![(0.02, 0.0), (0.52, 0.0), (0.2, 1.0)]]),
        '8' => (0.56, vec![arc(0.28, 0.25, 0.2, 0.24, 270.0, 630.0), arc(0.28, 0.73, 0.25, 0.27, 90.0, 450.0)]),
        '9' => (0.56, vec![join(arc(0.27, 0.3, 0.23, 0.3, 0.0, 360.0), bez((0.5, 0.3), (0.5, 0.85), (0.14, 0.99)))]),
        '-' => (0.42, vec![vec![(0.06, 0.56), (0.36, 0.56)]]),
        ',' => (0.22, vec![vec![(0.12, 0.88), (0.12, 0.96), (0.05, 1.14)]]),
        '.' => (0.2, vec![vec![(0.09, 0.95), (0.11, 0.97)]]),
        'x' => (0.5, vec![vec![(0.04, 0.45), (0.46, 1.0)], vec![(0.46, 0.45), (0.04, 1.0)]]),
        'y' => (0.52, vec![vec![(0.04, 0.45), (0.26, 0.96)], vec![(0.49, 0.45), (0.27, 0.98), (0.14, 1.26)]]),
        _ => return None,
    })
}

/// `path` with points at most `step` apart and every corner (a turn of over 25°) tripled:
/// the core's smoothing rounds the corners of a sparse path away (a 4 would lose its point).
fn dense(path: &[(f32, f32)], step: f32) -> Vec<(f32, f32)> {
    let mut out = Vec::with_capacity(path.len() * 2);
    for (i, &q) in path.iter().enumerate() {
        if i > 0 {
            let p = path[i - 1];
            let n = ((q.0 - p.0).hypot(q.1 - p.1) / step).ceil().max(1.0) as usize;
            for k in 1..n {
                let t = k as f32 / n as f32;
                out.push((p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t));
            }
        }
        out.push(q);
        if i > 0 && i + 1 < path.len() {
            let (p, r) = (path[i - 1], path[i + 1]);
            let (a, b) = ((q.0 - p.0, q.1 - p.1), (r.0 - q.0, r.1 - q.1));
            let (la, lb) = (a.0.hypot(a.1), b.0.hypot(b.1));
            if la > 1e-6 && lb > 1e-6 && (a.0 * b.0 + a.1 * b.1) / (la * lb) < 25f32.to_radians().cos() {
                out.push(q);
                out.push(q);
            }
        }
    }
    out
}

/// How wide `text` is at cap height `cap`.
fn text_width(text: &str, cap: f32) -> f32 {
    let gap = 0.16 * cap;
    let w: f32 = text.chars().filter_map(glyph).map(|(w, _)| w * cap + gap).sum();
    (w - gap).max(0.0)
}

/// `text` in the stroke font with its top-left at (x, y), each glyph a little different (the
/// hand's slant and size wander).
fn write(hand: &mut Hand, text: &str, x: f32, y: f32, cap: f32) -> Vec<Vec<InkPoint>> {
    let mut out = Vec::new();
    let mut ox = x;
    for c in text.chars() {
        let Some((w, lines)) = glyph(c) else { continue };
        let (slant, k, dy) = if hand.clean { (0.08, 1.0, 0.0) } else { (0.08 + hand.rng.next() * 0.05, 1.0 + hand.rng.next() * 0.04, hand.rng.next() * 0.03) };
        for l in lines {
            let path: Vec<(f32, f32)> = dense(&l, 0.06).iter().map(|&(gx, gy)| (ox + (gx + (1.0 - gy) * slant) * cap * k, y + (gy + dy) * cap * k)).collect();
            out.push(hand.stroke(&path, 0.0));
        }
        ox += w * cap + 0.16 * cap;
    }
    out
}

/// The plot as ink in page points, under-to-top: grid, axes with ticks and numbers, curves.
pub fn render(p: &Plot) -> Vec<(Brush, Vec<InkPoint>)> {
    let mut out: Vec<(Brush, Vec<InkPoint>)> = Vec::new();
    if !p.is_valid() {
        return out;
    }
    let mut hand = Hand { clean: p.clean, rng: Rng::new(&p.id, 1) };
    let thin = if p.clean { 0.0 } else { 0.35 };
    let w = p.width.clamp(0.3, 6.0);
    let axis = Brush { width: (w * 0.8).clamp(0.35, 3.0), thinning: thin, color: p.color, kind: KIND_PEN };
    let small = p.w.min(p.h);
    let cap = (small * 0.045).clamp(5.0, 8.0);
    let label = Brush { width: (cap * 0.13).clamp(0.35, w.max(0.5)), thinning: thin, color: p.color, kind: KIND_PEN };
    let grid = Brush { width: 0.3, thinning: 0.0, color: (p.color & 0x00FF_FFFF) | 0x4000_0000, kind: KIND_PEN };
    let (x0, y0, x1, y1) = (p.x, p.y, p.x + p.w, p.y + p.h);
    let px = |v: f64| (p.x as f64 + (v - p.x_min) / (p.x_max - p.x_min) * p.w as f64) as f32;
    let py = |v: f64| (p.y as f64 + (p.y_max - v) / (p.y_max - p.y_min) * p.h as f64) as f32;
    // Where the axes run: through 0, else along the nearer edge.
    let ax_y = if p.y_min <= 0.0 && p.y_max >= 0.0 { py(0.0) } else if p.y_max < 0.0 { y0 } else { y1 };
    let ax_x = if p.x_min <= 0.0 && p.x_max >= 0.0 { px(0.0) } else if p.x_max < 0.0 { x1 } else { x0 };
    let ticks = |lo: f64, hi: f64, len: f32| {
        let step = nice_step(hi - lo, (len / 14.0) as f64);
        let first = (lo / step).ceil() as i64;
        let last = (hi / step).floor() as i64;
        let vs: Vec<f64> = (first..=last).take(400).map(|i| i as f64 * step).collect();
        (step, vs)
    };
    let (sx, xs) = ticks(p.x_min, p.x_max, p.w);
    let (sy, ys) = ticks(p.y_min, p.y_max, p.h);
    if p.grid {
        for &v in &xs {
            out.push((grid, hand.line((px(v), y0), (px(v), y1), 0.15)));
        }
        for &v in &ys {
            out.push((grid, hand.line((x0, py(v)), (x1, py(v)), 0.15)));
        }
    }
    let wob = 0.35;
    // Axes, each with a head at its positive end.
    let head = (small * 0.035).clamp(3.5, 7.0);
    out.push((axis, hand.line((x0, ax_y), (x1 + head * 0.4, ax_y), wob)));
    out.push((axis, hand.line((ax_x, y1), (ax_x, y0 - head * 0.4), wob)));
    let tip_x = (x1 + head * 0.4, ax_y);
    let tip_y = (ax_x, y0 - head * 0.4);
    let sa = (26f32).to_radians();
    for (tip, dir) in [(tip_x, (1.0f32, 0.0f32)), (tip_y, (0.0, -1.0))] {
        for side in [-1.0f32, 1.0] {
            let (c, s) = (sa.cos(), sa.sin() * side);
            let back = (-dir.0 * c + dir.1 * s, -dir.1 * c - dir.0 * s);
            out.push((axis, hand.line(tip, (tip.0 + back.0 * head, tip.1 + back.1 * head), 0.0)));
        }
    }
    // Ticks and their numbers (every one that has room; 0 not where the other axis crosses).
    let tick = cap * 0.45;
    let zero_x = p.x_min < 0.0 && p.x_max > 0.0;
    let zero_y = p.y_min < 0.0 && p.y_max > 0.0;
    let gap = cap * 0.4;
    let x_labels: Vec<(f64, String)> = xs.iter().filter(|v| !(v.abs() < sx * 1e-6 && zero_y)).map(|&v| (v, tick_label(v, sx))).collect();
    let widest = x_labels.iter().map(|(_, t)| text_width(t, cap)).fold(0.0f32, f32::max);
    let step_pt = (sx / (p.x_max - p.x_min) * p.w as f64) as f32;
    let every_x = [1i64, 2, 5, 10, 20, 50].into_iter().find(|k| *k as f32 * step_pt >= widest + cap * 0.9).unwrap_or(100);
    let below = ax_y + tick + gap;
    for &v in &xs {
        let x = px(v);
        if (x - ax_x).abs() > 0.5 || !zero_x {
            out.push((axis, hand.line((x, ax_y - tick), (x, ax_y + tick), 0.0)));
        }
    }
    for (v, t) in &x_labels {
        let i = (v / sx).round() as i64;
        // No number at the axis' end: the head and the axis' name are there.
        if i % every_x != 0 || px(*v) > x1 - cap * 1.1 {
            continue;
        }
        let tw = text_width(t, cap);
        // A minus sign doesn't push the number off its tick.
        let shift = if t.starts_with('-') { text_width("-", cap) * 0.5 + 0.08 * cap } else { 0.0 };
        for s in write(&mut hand, t, px(*v) - tw / 2.0 - shift, below, cap) {
            out.push((label, s));
        }
    }
    let step_pt_y = (sy / (p.y_max - p.y_min) * p.h as f64) as f32;
    let every_y = [1i64, 2, 5, 10, 20, 50].into_iter().find(|k| *k as f32 * step_pt_y >= cap * 1.9).unwrap_or(100);
    for &v in &ys {
        let y = py(v);
        if (y - ax_y).abs() > 0.5 || !zero_y {
            out.push((axis, hand.line((ax_x - tick, y), (ax_x + tick, y), 0.0)));
        }
        let i = (v / sy).round() as i64;
        if i % every_y != 0 || (v.abs() < sy * 1e-6 && zero_x) || y < y0 + cap * 1.1 {
            continue;
        }
        let t = tick_label(v, sy);
        let tw = text_width(&t, cap);
        for s in write(&mut hand, &t, ax_x - tick - gap - tw, y - cap / 2.0, cap) {
            out.push((label, s));
        }
    }
    // The axes' names by their heads.
    // (x over the axis: the numbers are under it.)
    for s in write(&mut hand, "x", tip_x.0 - text_width("x", cap) - cap * 0.1, ax_y - tick - gap * 0.6 - cap, cap) {
        out.push((label, s));
    }
    for s in write(&mut hand, "y", tip_y.0 + tick + gap, tip_y.1 - cap * 0.3, cap) {
        out.push((label, s));
    }
    // The curves, in their own ink, on top.
    for f in &p.fns {
        let Ok(e) = parse(&f.expr) else { continue };
        let brush = Brush { width: w, thinning: thin, color: f.color, kind: KIND_PEN };
        for piece in curve(&e, p) {
            // Kinks (|x| at 0) stay sharp.
            out.push((brush, hand.stroke(&dense(&piece, 4.0), 0.0)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val(s: &str, x: f64) -> f64 {
        parse(s).unwrap_or_else(|e| panic!("{s}: {e}")).eval(x)
    }

    #[test]
    fn reads_what_people_type() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(close(val("x^2 - 1", 3.0), 8.0));
        assert!(close(val("2x", 3.0), 6.0));
        assert!(close(val("f(x) = 3sin x", 1.0), 3.0 * 1f64.sin()));
        assert!(close(val("sin 2x", 1.0), 2f64.sin()));
        assert!(close(val("sin x cos x", 1.0), 1f64.sin() * 1f64.cos()));
        assert!(close(val("sin(x)^2", 1.0), 1f64.sin().powi(2)));
        assert!(close(val("sin x^2", 2.0), 4f64.sin()));
        assert!(close(val("-x^2", 3.0), -9.0));
        assert!(close(val("e^-x", 1.0), (-1f64).exp()));
        assert!(close(val("2^x", 3.0), 8.0));
        assert!(close(val("2^3^2", 0.0), 512.0));
        assert!(close(val("0,5x", 4.0), 2.0));
        assert!(close(val("y = (x+1)(x-1)", 3.0), 8.0));
        assert!(close(val("|x| - 1", -3.0), 2.0));
        assert!(close(val("2|x-1|", -1.0), 4.0));
        assert!(close(val("||x|-2|", -1.0), 1.0));
        assert!(close(val("√x", 9.0), 3.0));
        assert!(close(val("sqrt(x)+1", 4.0), 3.0));
        assert!(close(val("x²", 3.0), 9.0));
        assert!(close(val("1/x", 4.0), 0.25));
        assert!(close(val("πx", 1.0), std::f64::consts::PI));
        assert!(close(val("2pix", 1.0), 2.0 * std::f64::consts::PI));
        assert!(close(val("ln(e)", 0.0), 1.0));
        assert!(close(val("x·3 − 1", 2.0), 5.0));
        assert!(close(val("x^(1/3)", -8.0), -2.0));
        assert!(close(val("sinx", 1.0), 1f64.sin()));
        assert!(val("ln x", -1.0).is_nan());
        assert!(val("sqrt x", -1.0).is_nan());
        for bad in ["", "x +", "sin", "(x", "x)", "q*x", "x ** ", "|x"] {
            assert!(parse(bad).is_err(), "{bad} should not parse");
        }
        assert!(check("x^2").is_ok());
        assert_eq!(check("y = kx").unwrap_err(), "„k“ kenne ich nicht (nur x als Variable)");
        assert_eq!(check("2sin()").unwrap_err(), "In der Klammer fehlt noch etwas");
        assert_eq!(check("x)").unwrap_err(), "Eine Klammer „)“ zu viel");
    }

    #[test]
    fn ticks_are_round_and_german() {
        assert_eq!(nice_step(10.0, 10.0), 1.0);
        assert_eq!(nice_step(12.0, 5.0), 5.0);
        assert_eq!(nice_step(1.0, 4.0), 0.5);
        assert_eq!(nice_step(0.3, 6.0), 0.05);
        assert_eq!(tick_label(0.5, 0.5), "0,5");
        assert_eq!(tick_label(-2.0, 1.0), "-2");
        assert_eq!(tick_label(1.25, 0.05), "1,25");
        assert_eq!(tick_label(3e-17, 0.1), "0");
    }

    fn plot(fns: &[&str]) -> Plot {
        let mut p = Plot::centered(100.0, 100.0, 240.0, 160.0, 20.0, 0xFF22_2222, 1.2);
        p.fns = fns.iter().map(|f| PlotFn { expr: f.to_string(), color: 0xFFD0_2020 }).collect();
        p
    }

    #[test]
    fn curves_stay_in_the_box_and_break_at_poles() {
        let p = plot(&[]);
        assert_eq!((p.x_min, p.x_max, p.y_min, p.y_max), (-6.0, 6.0, -4.0, 4.0));
        let inside = |pieces: &[Vec<(f32, f32)>]| pieces.iter().flatten().all(|&(x, y)| x >= 99.9 && x <= 340.1 && y >= 99.4 && y <= 260.6);
        // A parabola leaves through the top twice: one piece, inside the box.
        let par = curve(&parse("x^2 - 2").unwrap(), &p);
        assert_eq!(par.len(), 1, "{}", par.len());
        assert!(inside(&par));
        // Its lowest point is at (0, -2): page (220, 220).
        let low = par[0].iter().cloned().fold((0.0f32, 0.0f32), |a, b| if b.1 > a.1 { b } else { a });
        assert!((low.0 - 220.0).abs() < 1.0 && (low.1 - 220.0).abs() < 0.5, "{low:?}");
        // 1/x: two branches, never joined across the pole.
        let hyp = curve(&parse("1/x").unwrap(), &p);
        assert_eq!(hyp.len(), 2);
        assert!(inside(&hyp));
        assert!(hyp.iter().all(|piece| piece.iter().all(|q| q.0 < 220.0) || piece.iter().all(|q| q.0 > 220.0)));
        // tan: a branch between each pair of poles (-6..6 holds 4 poles → 5 pieces).
        let tan = curve(&parse("tan x").unwrap(), &p);
        assert_eq!(tan.len(), 5);
        // A step jumps: floor shows its steps -4 … 4 (the others are off the box).
        assert_eq!(curve(&parse("floor(x)").unwrap(), &p).len(), 9);
        // Only where it is defined: sqrt starts at 0.
        let sq = curve(&parse("sqrt x").unwrap(), &p);
        assert_eq!(sq.len(), 1);
        assert!((sq[0][0].0 - 220.0).abs() < 0.2, "{:?}", sq[0][0]);
        // Smooth: neighbouring points close on the page.
        let sin = curve(&parse("3 sin x").unwrap(), &p);
        assert!(sin[0].windows(2).all(|w| (w[0].0 - w[1].0).hypot(w[0].1 - w[1].1) < 6.0));
    }

    #[test]
    fn the_look_is_ink_in_the_box_and_the_same_every_time() {
        let mut p = plot(&["x^2/4 - 2", "sin x"]);
        p.id = "abc".into();
        let a = render(&p);
        assert_eq!(a, render(&p), "the same plot looks the same every time");
        assert!(a.len() > 20);
        // Everything near the box (numbers a little outside it at most).
        for (_, pts) in &a {
            for q in pts {
                assert!(q.x > 80.0 && q.x < 360.0 && q.y > 85.0 && q.y < 285.0, "{q:?}");
            }
        }
        // The curves in their colour, the rest in the plot's ink.
        assert!(a.iter().any(|(b, _)| b.color == 0xFFD0_2020));
        assert!(a.iter().filter(|(b, _)| b.color != 0xFFD0_2020).all(|(b, _)| b.color & 0xFFFFFF == 0x222222));
        // Drawn by hand: the x axis isn't ruler-straight, but stays within half a point.
        let axis = &a[0].1;
        let ys: Vec<f32> = axis.iter().map(|q| q.y).collect();
        let (lo, hi) = (ys.iter().cloned().fold(f32::MAX, f32::min), ys.iter().cloned().fold(f32::MIN, f32::max));
        assert!(hi - lo > 0.05 && hi - lo < 0.8, "{lo} {hi}");
        p.clean = true;
        let c = render(&p);
        let ys: Vec<f32> = c[0].1.iter().map(|q| q.y).collect();
        assert!(ys.iter().all(|y| (y - ys[0]).abs() < 1e-4), "clean is straight");
        // The grid adds a line per tick.
        p.grid = true;
        assert!(render(&p).len() > c.len() + 10);
        // Another id: another hand.
        let mut q = plot(&["x^2/4 - 2", "sin x"]);
        q.id = "xyz".into();
        assert_ne!(render(&q)[0].1, a[0].1);
    }

    #[test]
    fn numbers_are_written_with_all_their_glyphs() {
        for c in "0123456789-,.xy".chars() {
            let (w, lines) = glyph(c).unwrap();
            assert!(w > 0.1 && !lines.is_empty());
            for l in lines {
                assert!(l.iter().all(|&(x, y)| (-0.05..=0.65).contains(&x) && (-0.05..=1.3).contains(&y)), "{c}: {l:?}");
            }
        }
        assert!(text_width("-12,5", 10.0) > text_width("12", 10.0));
    }

    #[test]
    fn ranges_fit_the_functions() {
        let mut p = plot(&["x^2"]);
        let (lo, hi) = p.fit_y().unwrap();
        assert!(lo <= 0.0 && lo >= -5.0 && hi >= 30.0 && hi <= 40.0, "{lo} {hi}");
        // A pole doesn't blow the range up.
        p.fns = vec![PlotFn { expr: "1/x".into(), color: 0 }];
        let (lo, hi) = p.fit_y().unwrap();
        assert!(lo > -20.0 && hi < 20.0, "{lo} {hi}");
        p.fns.clear();
        assert!(p.fit_y().is_none());
        let mut p = plot(&[]);
        p.h = 120.0;
        assert_eq!(p.equal_units(), (-3.0, 3.0));
    }
}
