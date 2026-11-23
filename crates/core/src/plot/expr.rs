//! Functions as people type them, read into expressions: implicit products, `sin x`, `|x|`,
//! the German decimal comma, `x²`, `√`, `π`, references to the plot's other functions (`f(x)`,
//! `f'(x)`, `g(2x)`), vertical lines (`x = 2`), and LaTeX from the handwriting recogniser.

/// A built-in function.
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
    Round,
}

/// A comparison (in conditions: Wenn, a domain).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cmp {
    Lt,
    Le,
    Gt,
    Ge,
    Eq,
}

impl Cmp {
    pub fn holds(self, a: f64, b: f64) -> bool {
        match self {
            Cmp::Lt => a < b,
            Cmp::Le => a <= b + 1e-12 * (1.0 + b.abs()),
            Cmp::Gt => a > b,
            Cmp::Ge => a + 1e-12 * (1.0 + b.abs()) >= b,
            Cmp::Eq => (a - b).abs() <= 1e-9 * (1.0 + a.abs().max(b.abs())),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Num(f64),
    X,
    /// The second variable (equations in x and y: circles, any curve).
    Y,
    /// If the chain `items[0] ops[0] items[1] …` holds, the first, else the second (NaN: undefined).
    If(Vec<Expr>, Vec<Cmp>, Box<Expr>, Box<Expr>),
    Neg(Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Pow(Box<Expr>, Box<Expr>),
    Call(Func, Box<Expr>),
    /// The plot's function number `.0`, derived `.1` times, at `.2` (resolved before drawing).
    Ref(usize, u8, Box<Expr>),
}

fn b(e: Expr) -> Box<Expr> {
    Box::new(e)
}

impl Expr {
    /// The value at `x` (NaN outside the domain, ±∞ at poles; NaN for an unresolved reference).
    pub fn eval(&self, x: f64) -> f64 {
        self.eval2(x, f64::NAN)
    }

    /// The value at (x, y).
    pub fn eval2(&self, x: f64, y: f64) -> f64 {
        match self {
            Expr::Num(v) => *v,
            Expr::X => x,
            Expr::Y => y,
            Expr::If(items, ops, a, b) => {
                let vals: Vec<f64> = items.iter().map(|e| e.eval2(x, y)).collect();
                if vals.iter().all(|v| v.is_finite()) && ops.iter().enumerate().all(|(i, op)| op.holds(vals[i], vals[i + 1])) {
                    a.eval2(x, y)
                } else {
                    b.eval2(x, y)
                }
            }
            Expr::Neg(a) => -a.eval2(x, y),
            Expr::Add(a, b) => a.eval2(x, y) + b.eval2(x, y),
            Expr::Sub(a, b) => a.eval2(x, y) - b.eval2(x, y),
            Expr::Mul(a, b) => a.eval2(x, y) * b.eval2(x, y),
            Expr::Div(a, b) => a.eval2(x, y) / b.eval2(x, y),
            Expr::Pow(a, b) => pow(a.eval2(x, y), b.eval2(x, y)),
            Expr::Ref(..) => f64::NAN,
            Expr::Call(f, a) => {
                let v = a.eval2(x, y);
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
                    Func::Round => v.round(),
                }
            }
        }
    }

    /// Whether it depends on y.
    pub fn has_y(&self) -> bool {
        match self {
            Expr::Y => true,
            Expr::Num(_) | Expr::X => false,
            Expr::If(items, _, a, b) => items.iter().any(|e| e.has_y()) || a.has_y() || b.has_y(),
            Expr::Neg(a) | Expr::Call(_, a) | Expr::Ref(_, _, a) => a.has_y(),
            Expr::Add(a, b) | Expr::Sub(a, b) | Expr::Mul(a, b) | Expr::Div(a, b) | Expr::Pow(a, b) => a.has_y() || b.has_y(),
        }
    }

    /// Whether it depends on x.
    pub fn has_x(&self) -> bool {
        match self {
            Expr::X => true,
            Expr::Num(_) | Expr::Y => false,
            Expr::If(items, _, a, b) => items.iter().any(|e| e.has_x()) || a.has_x() || b.has_x(),
            Expr::Neg(a) | Expr::Call(_, a) => a.has_x(),
            Expr::Ref(_, _, a) => a.has_x(),
            Expr::Add(a, b) | Expr::Sub(a, b) | Expr::Mul(a, b) | Expr::Div(a, b) | Expr::Pow(a, b) => a.has_x() || b.has_x(),
        }
    }

    /// `self` with x replaced by `by` (f(2x) from f).
    pub fn at(&self, by: &Expr) -> Expr {
        match self {
            Expr::X => by.clone(),
            Expr::Y => Expr::Y,
            Expr::If(items, ops, a, c) => Expr::If(items.iter().map(|e| e.at(by)).collect(), ops.clone(), b(a.at(by)), b(c.at(by))),
            Expr::Num(v) => Expr::Num(*v),
            Expr::Neg(a) => Expr::Neg(b(a.at(by))),
            Expr::Add(l, r) => Expr::Add(b(l.at(by)), b(r.at(by))),
            Expr::Sub(l, r) => Expr::Sub(b(l.at(by)), b(r.at(by))),
            Expr::Mul(l, r) => Expr::Mul(b(l.at(by)), b(r.at(by))),
            Expr::Div(l, r) => Expr::Div(b(l.at(by)), b(r.at(by))),
            Expr::Pow(l, r) => Expr::Pow(b(l.at(by)), b(r.at(by))),
            Expr::Call(f, a) => Expr::Call(*f, b(a.at(by))),
            Expr::Ref(i, n, a) => Expr::Ref(*i, *n, b(a.at(by))),
        }
    }

    /// The derivative (by x), a little simplified.
    pub fn derive(&self) -> Expr {
        use Expr::*;
        let d = match self {
            Num(_) | Y => Num(0.0),
            X => Num(1.0),
            If(items, ops, a, c) => If(items.clone(), ops.clone(), b(a.derive()), b(c.derive())),
            Neg(a) => Neg(b(a.derive())),
            Add(l, r) => Add(b(l.derive()), b(r.derive())),
            Sub(l, r) => Sub(b(l.derive()), b(r.derive())),
            Mul(l, r) => Add(b(Mul(b(l.derive()), r.clone())), b(Mul(l.clone(), b(r.derive())))),
            Div(l, r) => Div(b(Sub(b(Mul(b(l.derive()), r.clone())), b(Mul(l.clone(), b(r.derive()))))), b(Pow(r.clone(), b(Num(2.0))))),
            Pow(l, r) if !r.has_x() => Mul(b(Mul(r.clone(), b(Pow(l.clone(), b(Sub(r.clone(), b(Num(1.0)))))))), b(l.derive())),
            Pow(l, r) if !l.has_x() => Mul(b(Mul(b(self.clone()), b(Call(Func::Ln, l.clone())))), b(r.derive())),
            // a^b = e^(b ln a): a^b · (b' ln a + b a'/a).
            Pow(l, r) => Mul(b(self.clone()), b(Add(b(Mul(b(r.derive()), b(Call(Func::Ln, l.clone())))), b(Div(b(Mul(r.clone(), b(l.derive()))), l.clone()))))),
            Ref(i, n, a) => Mul(b(Ref(*i, n + 1, a.clone())), b(a.derive())),
            Call(f, a) => {
                let da = a.derive();
                let sq = |e: Expr| Pow(b(e), b(Num(2.0)));
                let inner = match f {
                    Func::Sin => Call(Func::Cos, a.clone()),
                    Func::Cos => Neg(b(Call(Func::Sin, a.clone()))),
                    Func::Tan => Div(b(Num(1.0)), b(sq(Call(Func::Cos, a.clone())))),
                    Func::Cot => Neg(b(Div(b(Num(1.0)), b(sq(Call(Func::Sin, a.clone())))))),
                    Func::Asin => Div(b(Num(1.0)), b(Call(Func::Sqrt, b(Sub(b(Num(1.0)), b(sq((**a).clone()))))))),
                    Func::Acos => Neg(b(Div(b(Num(1.0)), b(Call(Func::Sqrt, b(Sub(b(Num(1.0)), b(sq((**a).clone()))))))))),
                    Func::Atan => Div(b(Num(1.0)), b(Add(b(Num(1.0)), b(sq((**a).clone()))))),
                    Func::Sinh => Call(Func::Cosh, a.clone()),
                    Func::Cosh => Call(Func::Sinh, a.clone()),
                    Func::Tanh => Div(b(Num(1.0)), b(sq(Call(Func::Cosh, a.clone())))),
                    Func::Exp => Call(Func::Exp, a.clone()),
                    Func::Ln => Div(b(Num(1.0)), a.clone()),
                    Func::Log => Div(b(Num(1.0)), b(Mul(a.clone(), b(Num(std::f64::consts::LN_10))))),
                    Func::Sqrt => Div(b(Num(1.0)), b(Mul(b(Num(2.0)), b(Call(Func::Sqrt, a.clone()))))),
                    Func::Abs => Call(Func::Sgn, a.clone()),
                    Func::Sgn | Func::Floor | Func::Ceil | Func::Round => Num(0.0),
                };
                Mul(b(inner), b(da))
            }
        };
        d.simplified()
    }

    /// Constants folded, ×1, +0, ×0 and the like taken out.
    pub fn simplified(&self) -> Expr {
        use Expr::*;
        let num = |e: &Expr| if let Num(v) = e { Some(*v) } else { None };
        match self {
            Neg(a) => match a.simplified() {
                Num(v) => Num(-v),
                Neg(inner) => *inner,
                s => Neg(b(s)),
            },
            Add(l, r) => {
                let (l, r) = (l.simplified(), r.simplified());
                match (num(&l), num(&r)) {
                    (Some(x), Some(y)) => Num(x + y),
                    (Some(z), _) if z == 0.0 => r,
                    (_, Some(z)) if z == 0.0 => l,
                    _ => Add(b(l), b(r)),
                }
            }
            Sub(l, r) => {
                let (l, r) = (l.simplified(), r.simplified());
                match (num(&l), num(&r)) {
                    (Some(x), Some(y)) => Num(x - y),
                    (_, Some(z)) if z == 0.0 => l,
                    (Some(z), _) if z == 0.0 => Neg(b(r)).simplified(),
                    _ => Sub(b(l), b(r)),
                }
            }
            Mul(l, r) => {
                let (l, r) = (l.simplified(), r.simplified());
                match (num(&l), num(&r)) {
                    (Some(x), Some(y)) => Num(x * y),
                    (Some(z), _) | (_, Some(z)) if z == 0.0 => Num(0.0),
                    (Some(o), _) if o == 1.0 => r,
                    (_, Some(o)) if o == 1.0 => l,
                    // The number in front: x·3 → 3x; 0,25·(3x²) → 0,75x².
                    (None, Some(k)) => Mul(b(Num(k)), b(l)).simplified(),
                    (Some(k), None) => match r {
                        Mul(ref a, ref c) if num(a).is_some() => Mul(b(Num(k * num(a).unwrap())), c.clone()).simplified(),
                        Neg(ref a) => Mul(b(Num(-k)), a.clone()).simplified(),
                        _ => Mul(b(Num(k)), b(r)),
                    },
                    _ => match (&l, &r) {
                        // (a·u)·v → a·(u·v): the number stays in front.
                        (Mul(a, u), _) if num(a).is_some() => Mul(a.clone(), b(Mul(u.clone(), b(r.clone())))).simplified(),
                        (_, Mul(a, v)) if num(a).is_some() => Mul(a.clone(), b(Mul(b(l.clone()), v.clone()))).simplified(),
                        _ => Mul(b(l), b(r)),
                    },
                }
            }
            Div(l, r) => {
                let (l, r) = (l.simplified(), r.simplified());
                match (num(&l), num(&r)) {
                    (Some(x), Some(y)) if y != 0.0 => Num(x / y),
                    (Some(z), _) if z == 0.0 => Num(0.0),
                    (_, Some(o)) if o == 1.0 => l,
                    _ => Div(b(l), b(r)),
                }
            }
            Pow(l, r) => {
                let (l, r) = (l.simplified(), r.simplified());
                match (num(&l), num(&r)) {
                    (Some(x), Some(y)) => Num(pow(x, y)),
                    (_, Some(o)) if o == 1.0 => l,
                    (_, Some(z)) if z == 0.0 => Num(1.0),
                    _ => Pow(b(l), b(r)),
                }
            }
            Call(f, a) => match a.simplified() {
                Num(v) if !matches!(f, Func::Floor | Func::Ceil | Func::Sgn | Func::Round) => Num(Call(*f, b(Num(v))).eval(0.0)),
                s => Call(*f, b(s)),
            },
            If(items, ops, a, c) => If(items.iter().map(|e| e.simplified()).collect(), ops.clone(), b(a.simplified()), b(c.simplified())),
            Ref(i, n, a) => Ref(*i, *n, b(a.simplified())),
            Num(_) | X | Y => self.clone(),
        }
    }

    /// References replaced by what they stand for (`defs`: the resolved functions by number).
    pub fn resolve(&self, defs: &dyn Fn(usize) -> Option<Expr>) -> Result<Expr, usize> {
        use Expr::*;
        Ok(match self {
            Num(_) | X | Y => self.clone(),
            If(items, ops, a, c) => If(items.iter().map(|e| e.resolve(defs)).collect::<Result<_, _>>()?, ops.clone(), b(a.resolve(defs)?), b(c.resolve(defs)?)),
            Neg(a) => Neg(b(a.resolve(defs)?)),
            Add(l, r) => Add(b(l.resolve(defs)?), b(r.resolve(defs)?)),
            Sub(l, r) => Sub(b(l.resolve(defs)?), b(r.resolve(defs)?)),
            Mul(l, r) => Mul(b(l.resolve(defs)?), b(r.resolve(defs)?)),
            Div(l, r) => Div(b(l.resolve(defs)?), b(r.resolve(defs)?)),
            Pow(l, r) => Pow(b(l.resolve(defs)?), b(r.resolve(defs)?)),
            Call(f, a) => Call(*f, b(a.resolve(defs)?)),
            Ref(i, n, a) => {
                let mut f = defs(*i).ok_or(*i)?;
                for _ in 0..*n {
                    f = f.derive();
                }
                f.at(&a.resolve(defs)?)
            }
        })
    }

    /// Which functions it refers to.
    pub fn refs(&self, out: &mut Vec<usize>) {
        match self {
            Expr::Num(_) | Expr::X | Expr::Y => {}
            Expr::If(items, _, a, c) => {
                items.iter().for_each(|e| e.refs(out));
                a.refs(out);
                c.refs(out);
            }
            Expr::Neg(a) | Expr::Call(_, a) => a.refs(out),
            Expr::Ref(i, _, a) => {
                out.push(*i);
                a.refs(out);
            }
            Expr::Add(l, r) | Expr::Sub(l, r) | Expr::Mul(l, r) | Expr::Div(l, r) | Expr::Pow(l, r) => {
                l.refs(out);
                r.refs(out);
            }
        }
    }
}

/// `a` to the power of `b`; an odd root of a negative number is real (x^(1/3) for x < 0).
pub(crate) fn pow(a: f64, b: f64) -> f64 {
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

/// What one line of a plot draws: a graph y = f(x), or a vertical line x = c.
#[derive(Debug, Clone, PartialEq)]
pub enum Curve {
    Of(Expr),
    AtX(f64),
}

/// The functions' names by their place in the plot: f, g, h, k, p, q, r, s.
pub const NAMES_OF_FNS: [char; 8] = ['f', 'g', 'h', 'k', 'p', 'q', 'r', 's'];

/// The name of the plot's function number `i`.
pub fn fn_name(i: usize) -> String {
    NAMES_OF_FNS.get(i).map(|c| c.to_string()).unwrap_or_else(|| format!("f{}", i + 1))
}

/// Whether `expr` can be drawn on its own: Ok, or what is wrong (German, for the app).
pub fn check(expr: &str) -> Result<(), String> {
    parse_line(expr, 0).map(|_| ())
}

/// Reads a function on its own (no references to other functions).
pub fn parse(src: &str) -> Result<Expr, String> {
    match parse_line(src, 0)? {
        Curve::Of(e) => Ok(e),
        Curve::AtX(_) => Err("Eine senkrechte Linie ist keine Funktion".into()),
    }
}

/// Reads one line of a plot with `fns` functions in it (their names can be used: `f(x) + 1`,
/// `f'(x)`). `x = 2` is a vertical line; `f(x) = …` / `y = …` count from their right side.
pub fn parse_line(src: &str, fns: usize) -> Result<Curve, String> {
    let src = src.trim();
    if let Some((lhs, rhs)) = src.split_once('=') {
        let lhs = lhs.trim();
        if lhs.eq_ignore_ascii_case("x") {
            let e = parse_expr(rhs, fns)?;
            if e.has_x() {
                return Err("Bei x = … darf rechts kein x stehen".into());
            }
            let v = e.eval(0.0);
            return if v.is_finite() { Ok(Curve::AtX(v)) } else { Err("Diese Zahl gibt es nicht".into()) };
        }
        return Ok(Curve::Of(parse_expr(rhs, fns)?));
    }
    Ok(Curve::Of(parse_expr(src, fns)?))
}

fn parse_expr(src: &str, fns: usize) -> Result<Expr, String> {
    let toks = lex(src, fns)?;
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

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Name(&'static str),
    /// One of the plot's functions (by number).
    Fn(usize),
    Prime,
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

fn lex(src: &str, fns: usize) -> Result<Vec<Tok>, String> {
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
            '\'' | '′' | '’' => out.push(Tok::Prime),
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
                // A run of letters is read as known names one after the other ("sinx" = sin x);
                // a single letter that names one of the plot's functions refers to it.
                let mut rest = word.as_str();
                while !rest.is_empty() {
                    if let Some(name) = NAMES.iter().find(|n| rest.starts_with(**n)) {
                        out.push(Tok::Name(name));
                        rest = &rest[name.len()..];
                        continue;
                    }
                    let first = rest.chars().next().unwrap_or(' ');
                    if let Some(k) = NAMES_OF_FNS.iter().position(|n| *n == first)
                        && k < fns
                    {
                        out.push(Tok::Fn(k));
                        rest = &rest[first.len_utf8()..];
                        continue;
                    }
                    let cut = (1..rest.len()).find(|&k| rest.is_char_boundary(k) && NAMES.iter().any(|n| rest[k..].starts_with(*n))).unwrap_or(rest.len());
                    let bad: String = rest[..cut].chars().take(8).collect();
                    return Err(format!("„{bad}“ kenne ich nicht (nur x als Variable)"));
                }
                continue;
            }
            other => return Err(format!("Zeichen „{other}“ kenne ich nicht")),
        }
        i += 1;
    }
    Ok(out)
}

/// `src` with its references to the plot's functions renamed: function `k` becomes `map(k)`
/// (None: left as it is). Letters are read exactly as [`parse_line`] reads them, so `sin`,
/// `sqrt` or `sgn` never change, only the names of functions.
pub fn rename_refs(src: &str, fns: usize, map: &dyn Fn(usize) -> Option<usize>) -> String {
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_alphabetic() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && chars[i].is_alphabetic() {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        let lower = word.to_lowercase();
        // Walk the run as the lexer does; keep the original spelling of what isn't renamed.
        let mut at = 0;
        let orig: Vec<char> = word.chars().collect();
        let low: Vec<char> = lower.chars().collect();
        while at < low.len() {
            let rest: String = low[at..].iter().collect();
            if let Some(name) = NAMES.iter().find(|n| rest.starts_with(**n)) {
                let n = name.chars().count();
                out.extend(&orig[at..at + n]);
                at += n;
                continue;
            }
            if let Some(k) = NAMES_OF_FNS.iter().position(|c| *c == low[at])
                && k < fns
                && let Some(to) = map(k)
            {
                out.push_str(&fn_name(to));
            } else {
                out.push(orig[at]);
            }
            at += 1;
        }
    }
    out
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
                    e = Expr::Add(b(e), b(self.term()?));
                }
                Some(Tok::Minus) => {
                    self.i += 1;
                    e = Expr::Sub(b(e), b(self.term()?));
                }
                _ => return Ok(e),
            }
        }
    }

    /// Whether the next token starts a value right after one (implicit multiplication).
    fn starts_value(&self) -> bool {
        match self.peek() {
            Some(Tok::Num(_)) | Some(Tok::Name(_)) | Some(Tok::Fn(_)) | Some(Tok::Open) => true,
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
                    e = Expr::Mul(b(e), b(self.unary()?));
                }
                Some(Tok::Slash) => {
                    self.i += 1;
                    e = Expr::Div(b(e), b(self.unary()?));
                }
                _ if self.starts_value() => e = Expr::Mul(b(e), b(self.power()?)),
                _ => return Ok(e),
            }
        }
    }

    fn unary(&mut self) -> Result<Expr, String> {
        match self.peek() {
            Some(Tok::Minus) => {
                self.i += 1;
                Ok(Expr::Neg(b(self.unary()?)))
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
            return Ok(Expr::Pow(b(base), b(exp)));
        }
        Ok(base)
    }

    /// A bracket's content after its "(" (the bracket closed).
    fn bracket(&mut self) -> Result<Expr, String> {
        let a = self.expr()?;
        if self.peek() != Some(&Tok::Close) {
            return Err("Eine Klammer „)“ fehlt".into());
        }
        self.i += 1;
        Ok(a)
    }

    fn atom(&mut self) -> Result<Expr, String> {
        let Some(t) = self.peek().cloned() else { return Err("Da fehlt noch etwas am Ende".into()) };
        self.i += 1;
        match t {
            Tok::Num(v) => Ok(Expr::Num(v)),
            Tok::Name("x") => Ok(Expr::X),
            Tok::Name("pi") => Ok(Expr::Num(std::f64::consts::PI)),
            Tok::Name("e") => Ok(Expr::Num(std::f64::consts::E)),
            Tok::Fn(k) => {
                // f(…), f'(…), f''(…); a bare f is f(x).
                let mut n = 0u8;
                while self.peek() == Some(&Tok::Prime) {
                    self.i += 1;
                    n += 1;
                }
                if self.peek() == Some(&Tok::Open) {
                    self.i += 1;
                    let a = self.bracket()?;
                    return Ok(Expr::Ref(k, n, b(a)));
                }
                Ok(Expr::Ref(k, n, b(Expr::X)))
            }
            Tok::Name(n) => {
                let f = func_of(n).ok_or_else(|| format!("„{n}“ kenne ich nicht"))?;
                // sin(…) takes the bracket; sin x, sin 2x, sin x² take what follows up to
                // the next operator (sin x cos x = sin(x)·cos(x)).
                if self.peek() == Some(&Tok::Open) {
                    self.i += 1;
                    let a = self.bracket()?;
                    return Ok(Expr::Call(f, b(a)));
                }
                if self.peek().is_none() {
                    return Err(format!("Hinter „{n}“ fehlt noch etwas"));
                }
                let mut a = if self.peek() == Some(&Tok::Minus) {
                    self.i += 1;
                    Expr::Neg(b(self.power()?))
                } else {
                    self.power()?
                };
                while matches!(self.peek(), Some(Tok::Num(_)) | Some(Tok::Name("x")) | Some(Tok::Name("pi")) | Some(Tok::Name("e"))) {
                    a = Expr::Mul(b(a), b(self.power()?));
                }
                Ok(Expr::Call(f, b(a)))
            }
            Tok::Open => self.bracket(),
            Tok::Bar => {
                self.abs += 1;
                let e = self.expr()?;
                self.abs -= 1;
                if self.peek() != Some(&Tok::Bar) {
                    return Err("Ein Betragsstrich „|“ fehlt".into());
                }
                self.i += 1;
                Ok(Expr::Call(Func::Abs, b(e)))
            }
            Tok::Close if self.i >= 2 && self.toks[self.i - 2] == Tok::Open => Err("In der Klammer fehlt noch etwas".into()),
            Tok::Close => Err("Eine Klammer „)“ zu viel".into()),
            Tok::Prime => Err("Ein Strich ' gehört hinter einen Funktionsnamen (f')".into()),
            Tok::Plus | Tok::Minus | Tok::Star | Tok::Slash | Tok::Caret => Err("Zwei Rechenzeichen hintereinander".into()),
        }
    }
}

/// LaTeX from the handwriting recogniser as a function to type (`\frac{1}{2}x^{2}` →
/// `(1)/(2)x^(2)`); `f(x) =` / `y =` in front are kept for [`parse_line`] to drop.
pub fn latex_to_expr(latex: &str) -> Result<String, String> {
    let s = latex.trim().trim_matches('$').trim();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let out = latex_group(&chars, &mut i, None)?;
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if out.trim().is_empty() {
        return Err("Keine Formel erkannt".into());
    }
    Ok(out)
}

/// Reads LaTeX from `i` up to `end` (a closing brace) or the end.
fn latex_group(c: &[char], i: &mut usize, end: Option<char>) -> Result<String, String> {
    let mut out = String::new();
    while *i < c.len() {
        let ch = c[*i];
        if Some(ch) == end {
            *i += 1;
            return Ok(out);
        }
        match ch {
            '{' => {
                *i += 1;
                let g = latex_group(c, i, Some('}'))?;
                out.push('(');
                out.push_str(&g);
                out.push(')');
            }
            '\\' => {
                *i += 1;
                let start = *i;
                while *i < c.len() && c[*i].is_ascii_alphabetic() {
                    *i += 1;
                }
                let cmd: String = c[start..*i].iter().collect();
                if cmd.is_empty() {
                    // \, \; \! \  : spacing; \| : a bar.
                    if *i < c.len() {
                        if c[*i] == '|' {
                            out.push('|');
                        }
                        *i += 1;
                    }
                    continue;
                }
                let arg = |i: &mut usize| -> Result<String, String> {
                    while *i < c.len() && c[*i] == ' ' {
                        *i += 1;
                    }
                    if *i < c.len() && c[*i] == '{' {
                        *i += 1;
                        latex_group(c, i, Some('}'))
                    } else if *i < c.len() {
                        let ch = c[*i];
                        *i += 1;
                        Ok(ch.to_string())
                    } else {
                        Err("Da fehlt etwas in der Formel".into())
                    }
                };
                match cmd.as_str() {
                    "frac" | "dfrac" | "tfrac" => {
                        let (n, d) = (arg(i)?, arg(i)?);
                        out.push_str(&format!("(({n})/({d}))"));
                    }
                    "sqrt" => {
                        // \sqrt[n]{a}: the n-th root.
                        let mut root = None;
                        if *i < c.len() && c[*i] == '[' {
                            *i += 1;
                            root = Some(latex_group(c, i, Some(']'))?);
                        }
                        let a = arg(i)?;
                        match root {
                            Some(n) => out.push_str(&format!("(({a})^(1/({n})))")),
                            None => out.push_str(&format!("sqrt({a})")),
                        }
                    }
                    "sin" | "cos" | "tan" | "cot" | "ln" | "log" | "exp" | "sinh" | "cosh" | "tanh" | "arcsin" | "arccos" | "arctan" => {
                        out.push(' ');
                        out.push_str(&cmd);
                        out.push(' ');
                    }
                    "lg" => out.push_str(" log "),
                    "pi" => out.push_str(" pi "),
                    "cdot" | "times" | "ast" => out.push('*'),
                    "div" => out.push('/'),
                    "left" | "right" | "big" | "Big" | "bigl" | "bigr" | "Bigl" | "Bigr" | "mathrm" | "mathit" | "text" | "operatorname" | "displaystyle" | "quad" | "qquad" => {}
                    "vert" | "lvert" | "rvert" | "mid" => out.push('|'),
                    "e" => out.push_str(" e "),
                    other => return Err(format!("„\\{other}“ kann ich nicht zeichnen")),
                }
            }
            '_' => {
                // A subscript (x_0, f_1): names only, dropped.
                *i += 1;
                if *i < c.len() && c[*i] == '{' {
                    *i += 1;
                    latex_group(c, i, Some('}'))?;
                } else {
                    *i += 1;
                }
            }
            '^' => {
                *i += 1;
                out.push('^');
                if *i < c.len() && c[*i] == '{' {
                    *i += 1;
                    let g = latex_group(c, i, Some('}'))?;
                    out.push_str(&format!("({g})"));
                } else if *i < c.len() {
                    out.push(c[*i]);
                    *i += 1;
                }
            }
            '&' | '\n' => {
                *i += 1;
            }
            _ => {
                out.push(ch);
                *i += 1;
            }
        }
    }
    if end.is_some() {
        return Err("Eine Klammer „}“ fehlt in der Formel".into());
    }
    Ok(out)
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
        for bad in ["", "x +", "sin", "(x", "x)", "q*x", "x ** ", "|x", "f(x)"] {
            assert!(parse(bad).is_err(), "{bad} should not parse");
        }
        assert!(check("x^2").is_ok());
        assert_eq!(check("y = kx").unwrap_err(), "„k“ kenne ich nicht (nur x als Variable)");
        assert_eq!(check("2sin()").unwrap_err(), "In der Klammer fehlt noch etwas");
        assert_eq!(check("x)").unwrap_err(), "Eine Klammer „)“ zu viel");
        // A vertical line.
        assert_eq!(parse_line("x = 2,5", 0), Ok(Curve::AtX(2.5)));
        assert_eq!(parse_line("x=-pi/2", 0), Ok(Curve::AtX(-std::f64::consts::FRAC_PI_2)));
        assert!(parse_line("x = 2x", 0).is_err());
    }

    #[test]
    fn derivatives_are_right() {
        let num = |e: &Expr, x: f64| (e.eval(x + 1e-6) - e.eval(x - 1e-6)) / 2e-6;
        for f in ["x^3 - 2x", "sin(2x)", "e^(-x^2)", "ln x", "sqrt(x^2+1)", "x^x", "1/x", "tan x", "atan(x)", "2^x", "|x-1|", "x sin x", "cos(x)/x"] {
            let e = parse(f).unwrap();
            let d = e.derive();
            for x in [0.3, 1.1, 2.7] {
                let (a, n) = (d.eval(x), num(&e, x));
                assert!((a - n).abs() < 1e-4 * (1.0 + n.abs()), "{f} at {x}: {a} vs {n} ({d:?})");
            }
        }
        // Simplified: the derivative of x² is 2x, not 2·x^1·1; numbers come together in front.
        assert_eq!(parse("x^2").unwrap().derive(), Expr::Mul(b(Expr::Num(2.0)), b(Expr::X)));
        assert_eq!(parse("0.25x^3").unwrap().derive(), Expr::Mul(b(Expr::Num(0.75)), b(Expr::Pow(b(Expr::X), b(Expr::Num(2.0))))));
        assert_eq!(parse("x*3").unwrap().simplified(), Expr::Mul(b(Expr::Num(3.0)), b(Expr::X)));
        assert_eq!(parse("5").unwrap().derive(), Expr::Num(0.0));
    }

    #[test]
    fn functions_refer_to_each_other() {
        let Curve::Of(g) = parse_line("f(x) + 1", 2).unwrap() else { panic!() };
        assert_eq!(g, Expr::Add(b(Expr::Ref(0, 0, b(Expr::X))), b(Expr::Num(1.0))));
        let Curve::Of(d) = parse_line("2f''(3x)", 2).unwrap() else { panic!() };
        let f = parse("x^3").unwrap();
        let r = d.resolve(&|i| (i == 0).then(|| f.clone())).unwrap();
        // f'' = 6x, at 3x: 18x, times 2: 36x.
        assert!((r.eval(2.0) - 72.0).abs() < 1e-9);
        // An unknown function stays unknown.
        assert!(parse_line("h(x)", 2).is_err());
        assert_eq!(d.resolve(&|_| None), Err(0));
        // "s" names a function only where there are that many; sin and sqrt stay what they are.
        assert!(matches!(parse_line("sin x + sqrt x", 8), Ok(Curve::Of(_))));
    }

    #[test]
    fn references_are_renamed_not_words() {
        let down = |k: usize| (k > 1).then(|| k - 1);
        assert_eq!(rename_refs("h(x) + f'(2x) - sqrt(x) + sgn(h(x))", 3, &down), "g(x) + f'(2x) - sqrt(x) + sgn(g(x))");
        assert_eq!(rename_refs("2hx + sin h", 3, &down), "2gx + sin g");
        // A letter that names no function (too few of them) stays.
        assert_eq!(rename_refs("k(x)", 3, &down), "k(x)");
    }

    #[test]
    fn latex_from_handwriting_becomes_a_function() {
        let v = |l: &str, x: f64| {
            let s = latex_to_expr(l).unwrap_or_else(|e| panic!("{l}: {e}"));
            match parse_line(&s, 0).unwrap_or_else(|e| panic!("{l} → {s}: {e}")) {
                Curve::Of(e) => e.eval(x),
                Curve::AtX(c) => c,
            }
        };
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        assert!(close(v("f(x)=\\frac{1}{2}x^{2}-3", 2.0), -1.0));
        assert!(close(v("y = \\sqrt{x+1}", 3.0), 2.0));
        assert!(close(v("\\sin\\left(2x\\right)", 1.0), 2f64.sin()));
        assert!(close(v("2 \\cdot e^{-x}", 0.0), 2.0));
        assert!(close(v("\\sqrt[3]{x}", 8.0), 2.0));
        assert!(close(v("$x^2$", 3.0), 9.0));
        assert!(close(v("\\left| x \\right| - 1", -2.0), 1.0));
        assert!(close(v("x = 3", 0.0), 3.0));
        assert!(latex_to_expr("\\int x dx").is_err());
    }
}
