//! The input language of a construction, as in GeoGebra: definitions (`A = (1, 2)`,
//! `f(x) = x^2`, `g: y = 2x + 1`, `a = 3`, `c: x^2 + y^2 = 4`), commands in German or English
//! with round or square brackets (`Schneide(f, g)`, `Intersect[f, g]`), points `(1, 2)` and
//! the school notation `(1|2)`, arithmetic on numbers, points and vectors, comparisons for
//! `Wenn`/`If`. Names are read against the construction's objects, so `2a` is 2·a when there is
//! an `a`, and `ab` is a·b when there is no `ab`.

use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Add,
    Sub,
    Mul,
    Div,
    Pow,
}

pub use super::expr::Cmp;

#[derive(Debug, Clone, PartialEq)]
pub enum Node {
    Num(f64),
    /// One of the free variables x, y, t.
    Var(char),
    /// An object of the construction (or a constant: pi, e).
    Name(String),
    Neg(Box<Node>),
    Bin(Op, Box<Node>, Box<Node>),
    /// `name(args)`: an object's function (f(2), f'(x)), a built-in (sin x) or a command
    /// (Gerade(A, B)); `.1` counts primes.
    Call(String, u8, Vec<Node>),
    /// (a, b) or (a|b).
    Point(Box<Node>, Box<Node>),
    /// a < b ≤ c …
    Chain(Vec<Node>, Vec<Cmp>),
    Text(String),
    List(Vec<Node>),
}

/// What a line of input says.
#[derive(Debug, Clone, PartialEq)]
pub struct Input {
    /// The name it gives (None: the construction names it).
    pub name: Option<String>,
    /// `f(x) = …`: the function's variable.
    pub param: Option<char>,
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    Expr(Node),
    /// An equation `left = right` in x and y (a line, a circle, any curve).
    Equation(Node, Node),
}

/// Built-in functions (lower case, as typed).
pub const FUNCS: &[&str] = &[
    "arcsin", "arccos", "arctan", "asin", "acos", "atan", "sinh", "cosh", "tanh", "sin", "cos", "tan", "cot", "exp", "ln", "lg", "log", "sqrt", "wurzel",
    "abs", "betrag", "sgn", "sign", "floor", "ceil", "round", "x", "y",
];

/// Commands, German and English (GeoGebra's names); matched without regard to case.
pub const COMMANDS: &[(&str, &str)] = &[
    ("punkt", "point"),
    ("gerade", "line"),
    ("strecke", "segment"),
    ("strahl", "ray"),
    ("vektor", "vector"),
    ("kreis", "circle"),
    ("vieleck", "polygon"),
    ("mittelpunkt", "midpoint"),
    ("schneide", "intersect"),
    ("nullstelle", "root"),
    ("extremum", "extremum"),
    ("wendepunkt", "inflectionpoint"),
    ("ableitung", "derivative"),
    ("integral", "integral"),
    ("integralzwischen", "integralbetween"),
    ("tangente", "tangent"),
    ("senkrechte", "perpendicularline"),
    ("parallele", "parallel"),
    ("mittelsenkrechte", "perpendicularbisector"),
    ("winkelhalbierende", "anglebisector"),
    ("abstand", "distance"),
    ("länge", "length"),
    ("steigung", "slope"),
    ("winkel", "angle"),
    ("funktion", "function"),
    ("wenn", "if"),
    ("kurve", "curve"),
    ("text", "text"),
    ("schieberegler", "slider"),
    ("spiegle", "reflect"),
    ("verschiebe", "translate"),
    ("drehe", "rotate"),
    ("fläche", "area"),
    ("min", "min"),
    ("max", "max"),
];

/// The English name of a command typed in either language, or None.
pub fn command(name: &str) -> Option<&'static str> {
    let n = name.to_lowercase();
    let n = n.replace("ae", "ä").replace("laenge", "länge");
    COMMANDS.iter().find(|(de, en)| *de == n || *en == n || de.replace('ä', "ae") == n).map(|(_, en)| *en)
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Ident(String),
    Str(String),
    Prime,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Open,
    Close,
    LBrack,
    RBrack,
    LBrace,
    RBrace,
    Comma,
    Bar,
    Colon,
    Cmp(Cmp),
    Degree,
}

/// Reads a line of input. `known`: the names of the construction's objects (and of the one
/// being defined, so `a = a + 1` reads).
pub fn parse_input(src: &str, known: &HashSet<String>) -> Result<Input, String> {
    let src = src.trim();
    if src.is_empty() {
        return Err("Leer".into());
    }
    // name: …  (a label in front; not inside brackets).
    let (label, rest) = split_label(src);
    // The first top-level "=" (not part of <=, >=).
    let eq = top_level_eq(rest);
    let mut known = known.clone();
    if let Some(l) = &label {
        known.insert(l.clone());
    }
    match eq {
        None => Ok(Input { name: label, param: None, body: Body::Expr(parse_expr(rest, &known)?) }),
        Some(at) => {
            let (lhs, rhs) = (rest[..at].trim(), rest[at + 1..].trim());
            if rhs.is_empty() {
                return Err("Rechts vom = fehlt noch etwas".into());
            }
            // f(x) = …, f(t) = …: a function (also over a label's name).
            if let Some((name, var)) = fn_head(lhs) {
                known.insert(name.clone());
                return Ok(Input { name: Some(name), param: Some(var), body: Body::Expr(parse_expr(rhs, &known)?) });
            }
            // a = …: a definition, unless the left side is x or y (then: an equation).
            if is_ident(lhs) && !matches!(lhs, "x" | "y") {
                if label.is_some() {
                    return Err("Zwei Namen für ein Objekt".into());
                }
                known.insert(lhs.to_string());
                return Ok(Input { name: Some(lhs.to_string()), param: None, body: Body::Expr(parse_expr(rhs, &known)?) });
            }
            Ok(Input { name: label, param: None, body: Body::Equation(parse_expr(lhs, &known)?, parse_expr(rhs, &known)?) })
        }
    }
}

/// Reads an expression on its own (in the names `known`).
pub fn parse_expr(src: &str, known: &HashSet<String>) -> Result<Node, String> {
    let toks = lex(src, known)?;
    if toks.is_empty() {
        return Err("Leer".into());
    }
    let mut p = P { toks, i: 0, abs: 0, sep: false };
    let n = p.chain()?;
    if p.i < p.toks.len() {
        return Err(match p.toks[p.i] {
            Tok::Close | Tok::RBrack => "Eine Klammer zu viel".into(),
            Tok::Bar => "Ein Betragsstrich zu viel".into(),
            Tok::Comma => "Ein Komma zu viel (Dezimalzahlen mit Punkt oder Komma ohne Leerzeichen: 0,5)".into(),
            _ => "Da stimmt etwas nicht".into(),
        });
    }
    Ok(n)
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    c.next().is_some_and(|f| f.is_alphabetic()) && s.chars().all(|c| c.is_alphabetic() || c.is_ascii_digit() || c == '_' || c == '\'')
}

/// "name:" at the start (outside brackets): the label and the rest.
fn split_label(src: &str) -> (Option<String>, &str) {
    if let Some(at) = src.find(':') {
        let head = src[..at].trim();
        if is_ident(head) && !src[..at].contains(['(', '[']) {
            return (Some(head.to_string()), src[at + 1..].trim());
        }
    }
    (None, src)
}

fn top_level_eq(s: &str) -> Option<usize> {
    let b = s.as_bytes();
    let mut depth = 0i32;
    for (i, &c) in b.iter().enumerate() {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'=' if depth == 0 => {
                let prev = if i > 0 { b[i - 1] } else { b' ' };
                let next = b.get(i + 1).copied().unwrap_or(b' ');
                if !matches!(prev, b'<' | b'>' | b'!' | b'=') && next != b'=' {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// "f(x)" / "g(t)" on the left of "=": the function's name and variable.
fn fn_head(lhs: &str) -> Option<(String, char)> {
    let open = lhs.find('(')?;
    let name = lhs[..open].trim();
    let inner = lhs[open + 1..].strip_suffix(')')?.trim();
    (is_ident(name) && !name.contains('\'') && matches!(inner, "x" | "t")).then(|| (name.to_string(), inner.chars().next().unwrap()))
}

/// Which commas are decimal commas: between digits, outside brackets, or inside a bracket that
/// separates with "|" ((0,5|2)).
fn decimal_commas(chars: &[char]) -> Vec<bool> {
    let n = chars.len();
    let mut dec = vec![false; n];
    // For each opening bracket, the number of bars at its own depth.
    let mut stack: Vec<(usize, usize)> = Vec::new();
    let mut bars_of = vec![0usize; n];
    let mut owner = vec![usize::MAX; n];
    for (i, &c) in chars.iter().enumerate() {
        match c {
            '(' | '[' | '{' => stack.push((i, 0)),
            ')' | ']' | '}' => {
                if let Some((o, k)) = stack.pop() {
                    bars_of[o] = k;
                }
            }
            '|' => {
                if let Some(top) = stack.last_mut() {
                    top.1 += 1;
                }
            }
            ',' => owner[i] = stack.last().map_or(usize::MAX, |t| t.0),
            _ => {}
        }
    }
    for i in 0..n {
        if chars[i] != ',' || i == 0 || i + 1 >= n || !chars[i - 1].is_ascii_digit() || !chars[i + 1].is_ascii_digit() {
            continue;
        }
        dec[i] = owner[i] == usize::MAX || bars_of[owner[i]] % 2 == 1;
    }
    dec
}

fn lex(src: &str, known: &HashSet<String>) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = src.chars().collect();
    let dec = decimal_commas(&chars);
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            ' ' | '\t' => {}
            '0'..='9' | '.' => {
                let mut s = String::new();
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.' || (chars[i] == ',' && dec[i])) {
                    s.push(if chars[i] == ',' { '.' } else { chars[i] });
                    i += 1;
                }
                out.push(Tok::Num(s.parse::<f64>().map_err(|_| format!("„{s}“ ist keine Zahl"))?));
                continue;
            }
            '"' => {
                let start = i + 1;
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    i += 1;
                }
                if i >= chars.len() {
                    return Err("Ein Anführungszeichen \" fehlt".into());
                }
                out.push(Tok::Str(chars[start..i].iter().collect()));
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
            '/' | '÷' => out.push(Tok::Slash),
            '^' => out.push(Tok::Caret),
            '(' => out.push(Tok::Open),
            ')' => out.push(Tok::Close),
            '[' => out.push(Tok::LBrack),
            ']' => out.push(Tok::RBrack),
            '{' => out.push(Tok::LBrace),
            '}' => out.push(Tok::RBrace),
            ',' | ';' => out.push(Tok::Comma),
            '|' => out.push(Tok::Bar),
            ':' => out.push(Tok::Colon),
            '°' => out.push(Tok::Degree),
            '\'' | '′' | '’' => out.push(Tok::Prime),
            '<' | '>' | '≤' | '≥' | '=' => {
                let next = chars.get(i + 1).copied();
                let (op, two) = match (c, next) {
                    ('<', Some('=')) => (Cmp::Le, true),
                    ('>', Some('=')) => (Cmp::Ge, true),
                    ('=', Some('=')) => (Cmp::Eq, true),
                    ('<', _) => (Cmp::Lt, false),
                    ('>', _) => (Cmp::Gt, false),
                    ('≤', _) => (Cmp::Le, false),
                    ('≥', _) => (Cmp::Ge, false),
                    _ => (Cmp::Eq, false),
                };
                out.push(Tok::Cmp(op));
                if two {
                    i += 1;
                }
            }
            'π' => out.push(Tok::Ident("pi".into())),
            '√' => out.push(Tok::Ident("sqrt".into())),
            '²' | '³' => {
                out.push(Tok::Caret);
                out.push(Tok::Num(if c == '²' { 2.0 } else { 3.0 }));
            }
            c if c.is_alphabetic() => {
                let start = i;
                // Letters, digits 0–9 and _ (not ² ³, which raise what comes before).
                while i < chars.len() && (chars[i].is_alphabetic() || chars[i].is_ascii_digit() || chars[i] == '_') {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                // The followed by "(" or "[": a call, so commands and functions come first.
                let calls = matches!(chars.get(i), Some('(') | Some('['));
                for t in split_word(&word, known, calls)? {
                    out.push(Tok::Ident(t));
                }
                continue;
            }
            other => return Err(format!("Zeichen „{other}“ kenne ich nicht")),
        }
        i += 1;
    }
    Ok(out)
}

/// A run of letters (and digits) read as names: the whole run when it is known (an object, a
/// command, a function, x/y/t, pi, e), else known names one after the other, longest first.
fn split_word(word: &str, known: &HashSet<String>, calls: bool) -> Result<Vec<String>, String> {
    let is_known = |w: &str| {
        known.contains(w) || FUNCS.contains(&w.to_lowercase().as_str()) || matches!(w, "x" | "y" | "t" | "pi" | "e") || (calls && command(w).is_some())
    };
    if is_known(word) || (calls && command(word).is_some()) {
        return Ok(vec![word.to_string()]);
    }
    let chars: Vec<char> = word.chars().collect();
    let mut out = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let mut found = None;
        for end in (at + 1..=chars.len()).rev() {
            let piece: String = chars[at..end].iter().collect();
            // A piece that ends the word may be a command or function being called.
            let last = end == chars.len();
            if known.contains(&piece) || FUNCS.contains(&piece.to_lowercase().as_str()) || matches!(piece.as_str(), "x" | "y" | "t" | "pi" | "e") || (last && calls && command(&piece).is_some()) {
                found = Some(end);
                break;
            }
            // Digits after letters belong to the name before them (a2: a·2 only if there is no a2).
        }
        let Some(end) = found else {
            // Digits on their own (a2 where only a exists): a number.
            if chars[at].is_ascii_digit() {
                let s: String = chars[at..].iter().take_while(|c| c.is_ascii_digit()).collect();
                at += s.len();
                out.push(s);
                continue;
            }
            let rest: String = chars[at..].iter().collect();
            let shown: String = rest.chars().take_while(|c| c.is_alphabetic()).take(10).collect();
            return Err(format!("„{shown}“ kenne ich nicht"));
        };
        out.push(chars[at..end].iter().collect());
        at = end;
    }
    Ok(out)
}

/// The letters a line of input uses that name nothing yet (`y = m x + c` → m, c; `b = 4 + 3a`
/// → a), in the order they come: GeoGebra makes sliders of them. Not x, y, t, e, π, the name
/// being defined, function or command names, or text in quotes. A letter with an index (`a_1`)
/// counts as one name.
pub fn unknown_letters(src: &str, known: &HashSet<String>) -> Vec<String> {
    let (label, rest) = split_label(src.trim());
    let mut known = known.clone();
    if let Some(l) = &label {
        known.insert(l.clone());
    }
    let mut scan = rest;
    if let Some(at) = top_level_eq(rest) {
        let lhs = rest[..at].trim();
        if let Some((name, _)) = fn_head(lhs) {
            known.insert(name);
            scan = &rest[at + 1..];
        } else if is_ident(lhs) && !matches!(lhs, "x" | "y") {
            known.insert(lhs.to_string());
            scan = &rest[at + 1..];
        }
    }
    let is_known = |w: &str, last_call: bool| {
        known.contains(w) || FUNCS.contains(&w.to_lowercase().as_str()) || matches!(w, "x" | "y" | "t" | "pi" | "e") || (last_call && command(w).is_some())
    };
    let chars: Vec<char> = scan.chars().collect();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    let mut quoted = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            quoted = !quoted;
        }
        if quoted || !c.is_alphabetic() {
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_alphabetic() || chars[i].is_ascii_digit() || chars[i] == '_') {
            i += 1;
        }
        let word = &chars[start..i];
        let calls = matches!(chars.get(i), Some('(') | Some('['));
        let whole: String = word.iter().collect();
        if is_known(&whole, calls) {
            continue;
        }
        // Read like the lexer does: the longest known piece first; a letter that is none starts a name.
        let mut at = 0;
        while at < word.len() {
            let found = (at + 1..=word.len()).rev().find(|&end| {
                let piece: String = word[at..end].iter().collect();
                is_known(&piece, calls && end == word.len())
            });
            if let Some(end) = found {
                at = end;
                continue;
            }
            if word[at].is_ascii_digit() || word[at] == '_' {
                at += 1;
                continue;
            }
            // a_1, a_n: the index belongs to the letter.
            let end = if word.get(at + 1) == Some(&'_') { word.len() } else { at + 1 };
            let name: String = word[at..end].iter().collect();
            if !out.contains(&name) {
                out.push(name);
            }
            at = end;
        }
    }
    out
}

/// `src` with every use of the name `old` changed to `new` (read like the lexer reads names:
/// `2a`, `a b`, `f'(x)`, inside commands; text in quotes stays as it is).
pub fn rename_name(src: &str, old: &str, new: &str, known: &HashSet<String>) -> String {
    let mut known = known.clone();
    known.insert(old.to_string());
    let chars: Vec<char> = src.chars().collect();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let mut quoted = false;
    while i < chars.len() {
        let c = chars[i];
        if c == '"' {
            quoted = !quoted;
        }
        if quoted || !c.is_alphabetic() {
            out.push(c);
            i += 1;
            continue;
        }
        let start = i;
        while i < chars.len() && (chars[i].is_alphabetic() || chars[i].is_ascii_digit() || chars[i] == '_') {
            i += 1;
        }
        let word: String = chars[start..i].iter().collect();
        let calls = matches!(chars.get(i), Some('(') | Some('['));
        match split_word(&word, &known, calls) {
            Ok(pieces) => {
                for p in pieces {
                    out.push_str(if p == old { new } else { &p });
                }
            }
            Err(_) => out.push_str(&word),
        }
    }
    out
}

struct P {
    toks: Vec<Tok>,
    i: usize,
    abs: usize,
    /// Inside "( … )", where a bar after a value separates (1|2) instead of opening |x|.
    sep: bool,
}

impl P {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.i)
    }

    fn eat(&mut self, t: &Tok) -> bool {
        if self.peek() == Some(t) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    /// a < b < c, or just a.
    fn chain(&mut self) -> Result<Node, String> {
        let first = self.expr()?;
        let mut items = vec![first];
        let mut ops = Vec::new();
        while let Some(Tok::Cmp(op)) = self.peek().cloned() {
            self.i += 1;
            ops.push(op);
            items.push(self.expr()?);
        }
        Ok(if ops.is_empty() { items.pop().unwrap() } else { Node::Chain(items, ops) })
    }

    fn expr(&mut self) -> Result<Node, String> {
        let mut e = self.term()?;
        loop {
            if self.eat(&Tok::Plus) {
                e = Node::Bin(Op::Add, Box::new(e), Box::new(self.term()?));
            } else if self.eat(&Tok::Minus) {
                e = Node::Bin(Op::Sub, Box::new(e), Box::new(self.term()?));
            } else {
                return Ok(e);
            }
        }
    }

    fn starts_value(&self) -> bool {
        match self.peek() {
            Some(Tok::Num(_)) | Some(Tok::Ident(_)) | Some(Tok::Open) | Some(Tok::Str(_)) => true,
            Some(Tok::Bar) => self.abs == 0 && !self.sep,
            _ => false,
        }
    }

    fn term(&mut self) -> Result<Node, String> {
        let mut e = self.unary()?;
        loop {
            if self.eat(&Tok::Star) {
                e = Node::Bin(Op::Mul, Box::new(e), Box::new(self.unary()?));
            } else if self.eat(&Tok::Slash) {
                e = Node::Bin(Op::Div, Box::new(e), Box::new(self.unary()?));
            } else if self.starts_value() {
                e = Node::Bin(Op::Mul, Box::new(e), Box::new(self.power()?));
            } else {
                return Ok(e);
            }
        }
    }

    fn unary(&mut self) -> Result<Node, String> {
        if self.eat(&Tok::Minus) {
            return Ok(Node::Neg(Box::new(self.unary()?)));
        }
        if self.eat(&Tok::Plus) {
            return self.unary();
        }
        self.power()
    }

    fn power(&mut self) -> Result<Node, String> {
        let mut base = self.atom()?;
        // 90°: in radians.
        if self.eat(&Tok::Degree) {
            base = Node::Bin(Op::Mul, Box::new(base), Box::new(Node::Bin(Op::Div, Box::new(Node::Name("pi".into())), Box::new(Node::Num(180.0)))));
        }
        if self.eat(&Tok::Caret) {
            let exp = self.unary()?;
            return Ok(Node::Bin(Op::Pow, Box::new(base), Box::new(exp)));
        }
        Ok(base)
    }

    /// Arguments after "(" or "[" up to the matching bracket.
    fn args(&mut self, close: Tok) -> Result<Vec<Node>, String> {
        let saved = self.sep;
        self.sep = false;
        let r = self.args_in(close);
        self.sep = saved;
        r
    }

    fn args_in(&mut self, close: Tok) -> Result<Vec<Node>, String> {
        let mut out = Vec::new();
        if self.eat(&close) {
            return Ok(out);
        }
        loop {
            out.push(self.chain()?);
            if self.eat(&Tok::Comma) {
                continue;
            }
            if self.eat(&close) {
                return Ok(out);
            }
            return Err("Eine Klammer fehlt".into());
        }
    }

    fn atom(&mut self) -> Result<Node, String> {
        let Some(t) = self.peek().cloned() else { return Err("Da fehlt noch etwas am Ende".into()) };
        self.i += 1;
        match t {
            Tok::Num(v) => Ok(Node::Num(v)),
            Tok::Str(s) => Ok(Node::Text(s)),
            Tok::Ident(name) => {
                // x(A), y(A): a point's coordinates (or x times a bracket).
                if matches!(name.as_str(), "x" | "y") && self.peek() == Some(&Tok::Open) {
                    self.i += 1;
                    let args = self.args(Tok::Close)?;
                    return Ok(Node::Call(name, 0, args));
                }
                if matches!(name.as_str(), "x" | "y" | "t") {
                    return Ok(Node::Var(name.chars().next().unwrap()));
                }
                let mut primes = 0u8;
                while self.eat(&Tok::Prime) {
                    primes += 1;
                }
                let lower = name.to_lowercase();
                let is_func = FUNCS.contains(&lower.as_str());
                // name(…) / name[…]: a call (object function, built-in or command).
                if self.peek() == Some(&Tok::Open) || self.peek() == Some(&Tok::LBrack) {
                    let close = if self.eat(&Tok::Open) {
                        Tok::Close
                    } else {
                        self.i += 1;
                        Tok::RBrack
                    };
                    let args = self.args(close)?;
                    return Ok(Node::Call(name, primes, args));
                }
                if is_func {
                    // sin x, sin 2x, sin x²: the next value up to an operator.
                    if self.peek().is_none() {
                        return Err(format!("Hinter „{name}“ fehlt noch etwas"));
                    }
                    let mut a = if self.eat(&Tok::Minus) { Node::Neg(Box::new(self.power()?)) } else { self.power()? };
                    while matches!(self.peek(), Some(Tok::Num(_))) || matches!(self.peek(), Some(Tok::Ident(n)) if matches!(n.as_str(), "x" | "y" | "t" | "pi" | "e")) {
                        a = Node::Bin(Op::Mul, Box::new(a), Box::new(self.power()?));
                    }
                    return Ok(Node::Call(name, 0, vec![a]));
                }
                if primes > 0 {
                    return Ok(Node::Call(name, primes, vec![Node::Var('x')]));
                }
                Ok(Node::Name(name))
            }
            Tok::Open => {
                // (a), (a, b) a point, (a | b) a point.
                let saved = (self.abs, self.sep);
                self.abs = 0;
                self.sep = true;
                let a = self.chain()?;
                if self.eat(&Tok::Comma) || self.eat(&Tok::Bar) {
                    let b = self.chain()?;
                    (self.abs, self.sep) = saved;
                    if !self.eat(&Tok::Close) {
                        return Err("Eine Klammer „)“ fehlt".into());
                    }
                    return Ok(Node::Point(Box::new(a), Box::new(b)));
                }
                (self.abs, self.sep) = saved;
                if !self.eat(&Tok::Close) {
                    return Err("Eine Klammer „)“ fehlt".into());
                }
                Ok(a)
            }
            Tok::LBrace => {
                let items = self.args(Tok::RBrace)?;
                Ok(Node::List(items))
            }
            Tok::Bar => {
                self.abs += 1;
                let saved = self.sep;
                self.sep = false;
                let e = self.expr()?;
                self.sep = saved;
                self.abs -= 1;
                if !self.eat(&Tok::Bar) {
                    return Err("Ein Betragsstrich „|“ fehlt".into());
                }
                Ok(Node::Call("abs".into(), 0, vec![e]))
            }
            Tok::Close if self.i >= 2 && self.toks[self.i - 2] == Tok::Open => Err("In der Klammer fehlt noch etwas".into()),
            Tok::Close | Tok::RBrack | Tok::RBrace => Err("Eine Klammer zu viel".into()),
            Tok::Prime => Err("Ein Strich ' gehört hinter einen Funktionsnamen (f')".into()),
            Tok::Comma => Err("Ein Komma an der falschen Stelle".into()),
            Tok::Colon => Err("Ein Doppelpunkt nur hinter einem Namen (g: y = 2x)".into()),
            Tok::Cmp(_) => Err("Ein Vergleich ohne linke Seite".into()),
            Tok::Degree => Err("Das Gradzeichen gehört hinter eine Zahl (90°)".into()),
            Tok::LBrack => Err("Eine eckige Klammer nur hinter einem Befehl".into()),
            Tok::Plus | Tok::Minus | Tok::Star | Tok::Slash | Tok::Caret => Err("Zwei Rechenzeichen hintereinander".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn k(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn name(s: &str) -> Box<Node> {
        Box::new(Node::Name(s.into()))
    }

    #[test]
    fn definitions_read_like_in_geogebra() {
        let none = k(&[]);
        let i = parse_input("A = (1, 2)", &none).unwrap();
        assert_eq!(i.name.as_deref(), Some("A"));
        assert_eq!(i.body, Body::Expr(Node::Point(Box::new(Node::Num(1.0)), Box::new(Node::Num(2.0)))));
        // The school notation, with decimal commas.
        let i = parse_input("B = (0,5|-2)", &none).unwrap();
        assert_eq!(i.body, Body::Expr(Node::Point(Box::new(Node::Num(0.5)), Box::new(Node::Neg(Box::new(Node::Num(2.0)))))));
        // A function; an equation (line); a labelled equation; a number.
        let f = parse_input("f(x) = 0,5x^2 - 3", &none).unwrap();
        assert_eq!((f.name.as_deref(), f.param), (Some("f"), Some('x')));
        assert!(matches!(parse_input("y = 2x + 1", &none).unwrap().body, Body::Equation(..)));
        let g = parse_input("g: y = 2x + 1", &none).unwrap();
        assert_eq!(g.name.as_deref(), Some("g"));
        assert!(matches!(g.body, Body::Equation(..)));
        assert!(matches!(parse_input("x = 2", &none).unwrap().body, Body::Equation(..)));
        let a = parse_input("a = 3", &none).unwrap();
        assert_eq!((a.name.as_deref(), a.body), (Some("a"), Body::Expr(Node::Num(3.0))));
        // Unnamed.
        assert_eq!(parse_input("(3, 4)", &none).unwrap().name, None);
    }

    #[test]
    fn commands_in_both_languages_and_brackets() {
        let known = k(&["f", "g", "A", "B"]);
        let i = parse_input("S = Schneide(f, g)", &known).unwrap();
        assert_eq!(i.body, Body::Expr(Node::Call("Schneide".into(), 0, vec![Node::Name("f".into()), Node::Name("g".into())])));
        let i = parse_input("Intersect[f, g]", &known).unwrap();
        assert_eq!(i.body, Body::Expr(Node::Call("Intersect".into(), 0, vec![Node::Name("f".into()), Node::Name("g".into())])));
        assert_eq!(command("Schneide"), Some("intersect"));
        assert_eq!(command("intersect"), Some("intersect"));
        assert_eq!(command("Laenge"), Some("length"));
        assert_eq!(command("Gerade"), Some("line"));
        assert!(command("Quatsch").is_none());
        // A command's arguments: points, numbers with decimal commas inside (a|b) only.
        let i = parse_input("Kreis((1|2), 3)", &known).unwrap();
        assert!(matches!(i.body, Body::Expr(Node::Call(_, 0, ref a)) if a.len() == 2));
        let i = parse_input("Kreis((1,5), 3)", &known).unwrap();
        let Body::Expr(Node::Call(_, _, a)) = i.body else { panic!() };
        assert_eq!(a[0], Node::Point(Box::new(Node::Num(1.0)), Box::new(Node::Num(5.0))), "a comma in (a, b) separates");
    }

    #[test]
    fn superscripts_raise_what_is_before_them() {
        let known = k(&["f", "a"]);
        for src in ["g(x) = x² + 1", "h(x) = a·x³ − 2", "p(x) = (x − 1)²", "q(x) = √(x) + π"] {
            assert!(parse_input(src, &known).is_ok(), "{src}: {:?}", parse_input(src, &known));
        }
    }

    #[test]
    fn names_are_renamed_where_they_are_used() {
        let known = k(&["A", "B", "f", "a"]);
        assert_eq!(rename_name("g = Gerade(A, B)", "A", "P", &known), "g = Gerade(P, B)");
        assert_eq!(rename_name("h(x) = 2a x + f'(x)", "a", "k", &known), "h(x) = 2k x + f'(x)");
        assert_eq!(rename_name("t = Text(\"A und B\")", "A", "P", &known), "t = Text(\"A und B\")");
        // Names inside other words aren't touched (Abstand is a command, not A·bstand).
        assert_eq!(rename_name("d = Abstand(A, B)", "A", "P", &known), "d = Abstand(P, B)");
    }

    #[test]
    fn names_are_read_against_the_objects() {
        // 2a is 2·a; ab is a·b when there is no ab; f(2) a call; f'(x) a derivative.
        let known = k(&["a", "b", "f"]);
        assert_eq!(parse_expr("2a", &known).unwrap(), Node::Bin(Op::Mul, Box::new(Node::Num(2.0)), name("a")));
        assert_eq!(parse_expr("ab", &known).unwrap(), Node::Bin(Op::Mul, name("a"), name("b")));
        assert_eq!(parse_expr("f(2)", &known).unwrap(), Node::Call("f".into(), 0, vec![Node::Num(2.0)]));
        assert_eq!(parse_expr("f'(x)", &known).unwrap(), Node::Call("f".into(), 1, vec![Node::Var('x')]));
        assert_eq!(parse_expr("f'", &known).unwrap(), Node::Call("f".into(), 1, vec![Node::Var('x')]));
        assert!(parse_expr("q + 1", &known).is_err());
        // sin x keeps working; an object named "s" doesn't break sin.
        let known = k(&["s"]);
        assert_eq!(parse_expr("sin x", &known).unwrap(), Node::Call("sin".into(), 0, vec![Node::Var('x')]));
        // Conditions for Wenn.
        let c = parse_expr("0 < x <= 2", &k(&[])).unwrap();
        assert_eq!(c, Node::Chain(vec![Node::Num(0.0), Node::Var('x'), Node::Num(2.0)], vec![Cmp::Lt, Cmp::Le]));
        // Text and lists.
        assert_eq!(parse_expr("\"Hallo\"", &k(&[])).unwrap(), Node::Text("Hallo".into()));
        assert!(matches!(parse_expr("{1, 2, 3}", &k(&[])).unwrap(), Node::List(v) if v.len() == 3));
        // Absolute values still work inside points.
        assert!(matches!(parse_expr("(|-2|, 3)", &k(&[])).unwrap(), Node::Point(..)));
    }
}
