//! Coordinate systems with constructions, as in GeoGebra. A plot is an object on a page — a
//! box, the ranges it shows, its look — holding objects (points, lines, circles, functions,
//! sliders, texts …) defined the way GeoGebra defines them. The core works out how it all looks
//! as ink: axes, ticks, numbers and names in a little hand-written stroke font, curves, hatched
//! areas, marked points. So every app and the PDF export draw it the same way, with the pen's
//! feel rather than a printout's, and it stays editable.

mod expr;
mod font;
pub mod geo;
pub mod lang;
mod points;
mod render;

pub use expr::{Cmp, Curve, Expr, Func, NAMES_OF_FNS, check, fn_name, handwritten_input, latex_to_expr, parse, parse_line, rename_refs};
pub use font::{glyph, layout, text_width};
pub use geo::{End, Kind, LabelMode, Obj, PointStyle, Slider, Style, Val, ViewRange, bounds_of, definition, domain_ends, domain_parts, evaluate, exact_fraction, fmt_expr, fmt_expr_in, fmt_num, format_val, free_point, latex_expr, latex_input, latex_row, latex_val, moved_on, moved_point, name_of, new_object, next_name, renamed, row_latex, row_text, with_domain};
pub use points::{PointKind, SpecialPoint, area, special_points, zeros_of};
pub use render::{curve, hit, nice_step, pi_label, render, slider_track, tick_label};

use std::collections::HashSet;

/// How a line is drawn.
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

/// A point marked by hand: its name and where it is (plot units).
#[derive(Debug, Clone, PartialEq)]
pub struct PlotPoint {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

/// What a function marks by itself (bits of [`Style::special`]).
pub const MARK_ZEROS: u8 = 1;
pub const MARK_EXTREMA: u8 = 2;
pub const MARK_CROSSINGS: u8 = 4;
pub const MARK_TURNS: u8 = 8;

/// Where new and dragged points land (GeoGebra's "Punktfang").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Snap {
    /// On the grid when the grid shows (or the plot keeps to squared paper) and the point is
    /// near a grid point.
    #[default]
    Auto,
    /// Always on the grid's points.
    Grid,
    Off,
}

impl Snap {
    pub fn code(self) -> &'static str {
        match self {
            Snap::Auto => "",
            Snap::Grid => "grid",
            Snap::Off => "off",
        }
    }
    pub fn from_code(s: &str) -> Snap {
        match s {
            "grid" => Snap::Grid,
            "off" => Snap::Off,
            _ => Snap::Auto,
        }
    }
}

/// A coordinate system on a page: its box (page points), the ranges shown, its look, its
/// objects.
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
    pub axes: bool,
    /// Lines at every tick.
    pub grid: bool,
    /// Finer lines between them.
    pub minor: bool,
    /// Ruler-straight instead of drawn by hand.
    pub clean: bool,
    /// Ink of the axes, ticks and numbers.
    pub color: u32,
    /// Pen width of the objects (axes a little thinner).
    pub width: f32,
    /// The x axis in steps of π (π/2, π, 3π/2 …).
    pub pi: bool,
    /// The axes' names (empty: x and y).
    pub x_name: String,
    pub y_name: String,
    pub snap: Snap,
    /// The box keeps to the paper's squares (moved, made bigger); points snap to them too.
    pub align: bool,
    /// Points and sliders can be dragged on the page itself.
    pub interactive: bool,
    pub objs: Vec<Obj>,
}

/// Colours given to new objects in turn (GeoGebra-like, on white paper).
pub const PALETTE: [u32; 6] = [0xFF15_65C0, 0xFFC6_2828, 0xFF2E_7D32, 0xFF6A_1B9A, 0xFFEF_6C00, 0xFF00_838F];

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
            axes: true,
            grid: false,
            minor: false,
            clean: false,
            color,
            width,
            pi: false,
            x_name: String::new(),
            y_name: String::new(),
            snap: Snap::Auto,
            align: false,
            interactive: false,
            objs: Vec::new(),
        }
    }

    /// Whether the ranges and the box can be drawn.
    /// Also a box that is a number and lies within reach: the box comes from another device or
    /// a drag as it is, and drawing walks along it in small steps (an infinite or far-off box
    /// never got to the end).
    pub fn is_valid(&self) -> bool {
        let fin = |v: f64| v.is_finite();
        let near = |v: f32| v.abs() <= 1e6;
        let size = |v: f32| v > 1.0 && v <= crate::format::MAX_PT;
        fin(self.x_min) && fin(self.x_max) && fin(self.y_min) && fin(self.y_max) && fin(self.x_max - self.x_min) && fin(self.y_max - self.y_min)
            && self.x_max > self.x_min && self.y_max > self.y_min && size(self.w) && size(self.h) && near(self.x) && near(self.y)
    }

    pub fn view(&self) -> ViewRange {
        ViewRange { x0: self.x_min, x1: self.x_max, y0: self.y_min, y1: self.y_max }
    }

    /// Every object worked out (in the objects' order).
    pub fn values(&self) -> Vec<Result<Val, String>> {
        evaluate(&self.objs, self.view())
    }

    /// What is wrong with each object (None: fine).
    pub fn errors(&self) -> Vec<Option<String>> {
        self.values().into_iter().map(|r| r.err()).collect()
    }

    /// The plot with a new object from a line of input (`A = (1, 2)`, `x^2`, `Schneide(f, g)`):
    /// named like in GeoGebra, coloured in turn. Several points (two intersections, three
    /// zeros) become several objects, A and B, each usable on its own. Err: what is wrong.
    ///
    /// Letters that name nothing yet become sliders first, like in GeoGebra (`y = m x + c`,
    /// `b = 4 + 3a`): each at 1, from −5 to 5 — only when the input then works.
    ///
    /// Two letters that name nothing yet and no x or y (`a = 3b`, `b = 4 + 3a`, `2a + 3b = 6`)
    /// make a curve instead: the right one across, the left one up, the axes named after them
    /// (see [`Plot::curve_switch`] to make sliders of them after all).
    pub fn add(&self, input: &str) -> Result<Plot, String> {
        let plain = |p: &Plot| -> Result<Plot, String> {
            if let Some(split) = p.split_points(input)? {
                return Ok(split);
            }
            p.add_one(input)
        };
        match plain(self) {
            Ok(p) => Ok(p),
            Err(e) => {
                if let Some(p) = self.curve_of_letters(input) {
                    return Ok(p);
                }
                match self.with_sliders_for(input) {
                    // Just a letter: its slider is all there is to it.
                    Some((p, letters)) if letters.len() == 1 && letters[0] == input.trim() => Ok(p),
                    Some((p, _)) => plain(&p).map_err(|_| e),
                    None => Err(e),
                }
            }
        }
    }

    /// `a = 3b` (or an equation like `2a + 3b = 6`) whose two letters name nothing yet, without
    /// x and y: the curve of one in the other — `a(b) = 3b`, the axes b across and a up (an
    /// equation: the letter only on the right across, else the first one). None otherwise.
    fn curve_of_letters(&self, input: &str) -> Option<Plot> {
        let known: HashSet<String> = self.objs.iter().map(|o| o.name()).collect();
        let (across, up, def) = self.letters_curve(input, &known)?;
        let mut p = self.add_one(&def).ok()?;
        p.name_axes(&across, &up);
        Some(p)
    }

    /// Object `id` changed to `a = 3b` (two letters that name nothing yet, its own old name
    /// counting as free): the same curve as typed (see [`Plot::curve_of_letters`]). None
    /// otherwise, and for an object others use (they keep a number).
    fn curve_of_letters_at(&self, id: &str, input: &str) -> Option<Plot> {
        if !self.dependents(id).is_empty() {
            return None;
        }
        let own = self.objs.iter().find(|o| o.id == id)?.name();
        let known: HashSet<String> = self.objs.iter().map(|o| o.name()).filter(|n| *n != own).collect();
        let (across, up, def) = self.letters_curve(input, &known)?;
        let mut p = self.set_def_plain(id, &def).ok()?;
        p.name_axes(&across, &up);
        Some(p)
    }

    /// For [`Plot::curve_of_letters`]: the letter across, the letter up and the curve's
    /// definition (`a(b) = 3b`, `2x + 3y = 6`), with [known] the names in use.
    fn letters_curve(&self, input: &str, known: &HashSet<String>) -> Option<(String, String, String)> {
        let letters = lang::unknown_letters(input, known);
        let letter = |s: &str| s.len() == 1 && s.chars().all(|c| c.is_ascii_lowercase() && !matches!(c, 'x' | 'y' | 't' | 'e'));
        let mut all = known.clone();
        all.extend(letters.iter().cloned());
        let parsed = lang::parse_input(input, &all).ok()?;
        if parsed.param.is_some() || parsed.polar || nodes_of(&parsed).iter().any(|n| has_var(n)) {
            return None;
        }
        match (&parsed.name, &parsed.body) {
            (Some(l), lang::Body::Expr(_)) if letters.len() == 1 && letter(l) && letter(&letters[0]) && !known.contains(l) && letters[0] != *l => {
                let r = letters[0].clone();
                let rest = input.trim();
                let rest = rest[lang_eq(rest)? + 1..].trim();
                Some((r.clone(), l.clone(), format!("{l}({r}) = {rest}")))
            }
            (_, lang::Body::Equation(lhs, rhs)) if letters.len() == 2 && letters.iter().all(|s| letter(s)) => {
                let side = |n: &lang::Node| {
                    let mut v = Vec::new();
                    collect_names(n, &mut v);
                    v
                };
                let (ln, rn) = (side(lhs), side(rhs));
                let only = |a: &[String], b: &[String]| letters.iter().filter(|l| a.contains(l) && !b.contains(l)).cloned().collect::<Vec<_>>();
                let (r_only, l_only) = (only(&rn, &ln), only(&ln, &rn));
                let (h, v) = if r_only.len() == 1 && l_only.len() == 1 { (r_only[0].clone(), l_only[0].clone()) } else { (letters[0].clone(), letters[1].clone()) };
                let to_x = lang::rename_name(input, &h, "x", &all);
                let def = lang::rename_name(&to_x, &v, "y", &all);
                Some((h, v, def))
            }
            _ => None,
        }
    }

    /// The axes named `across` and `up` where they have no names of their own yet.
    fn name_axes(&mut self, across: &str, up: &str) {
        if self.x_name.trim().is_empty() {
            self.x_name = across.to_string();
        }
        if self.y_name.trim().is_empty() {
            self.y_name = up.to_string();
        }
    }

    /// The row switch "als Kurve / als Schieberegler" of object `id`: Some(true) for a curve of
    /// a letter (`a(b) = 3b`), which can become a number with a slider; Some(false) for a
    /// number of one slider and no x, y (`b = 4 + 3a`), which can become the curve; None: no
    /// switch.
    pub fn curve_switch(&self, id: &str) -> Option<bool> {
        let o = self.objs.iter().find(|o| o.id == id)?;
        let shape = lang::shape(&o.def);
        if shape.polar {
            return None;
        }
        if let Some(p) = shape.param {
            return (p.is_ascii_lowercase() && !matches!(p, 'x' | 't')).then_some(true);
        }
        self.slider_of(id).map(|_| false)
    }

    /// For a number of one slider and no x, y (`b = 4 + 3a`): the slider's index.
    fn slider_of(&self, id: &str) -> Option<usize> {
        let i = self.objs.iter().position(|o| o.id == id)?;
        let known: HashSet<String> = self.objs.iter().map(|o| o.name()).collect();
        let input = lang::parse_input(&self.objs[i].def, &known).ok()?;
        let (Some(name), lang::Body::Expr(n), None) = (&input.name, &input.body, &input.domain) else { return None };
        if has_var(n) {
            return None;
        }
        let mut refs = Vec::new();
        collect_names(n, &mut refs);
        refs.retain(|r| r != name && known.contains(r));
        refs.sort();
        refs.dedup();
        let [r] = refs.as_slice() else { return None };
        let j = self.objs.iter().position(|o| o.name() == *r)?;
        let ok = self.objs[j].style.slider.is_some() && r.len() == 1 && r.chars().all(|c| c.is_ascii_lowercase() && !matches!(c, 'x' | 'y' | 't' | 'e'));
        (ok && matches!(self.values()[i], Ok(Val::Num(_)))).then_some(j)
    }

    /// `a(b) = 3b` as a number: `a = 3b`, with a slider b (made when there is none).
    pub fn as_sliders(&self, id: &str) -> Result<Plot, String> {
        let i = self.objs.iter().position(|o| o.id == id).ok_or("Dieses Objekt gibt es nicht mehr")?;
        let o = &self.objs[i];
        let var = lang::shape(&o.def).param.filter(|p| !matches!(p, 'x' | 't')).ok_or("Das ist keine Kurve aus Buchstaben")?;
        let (body, _) = lang::split_domain(&o.def);
        let rest = body[lang_eq(body).ok_or("?")? + 1..].trim();
        let name = o.name();
        let mut p = self.clone();
        if !self.objs.iter().any(|q| q.name() == var.to_string()) {
            p = p.add_one(&format!("{var} = 1"))?;
        }
        let mut p = p.set_def_plain(id, &format!("{name} = {rest}"))?;
        if p.x_name == var.to_string() && p.y_name == name {
            (p.x_name, p.y_name) = (String::new(), String::new());
        }
        Ok(p)
    }

    /// `b = 4 + 3a` (a a slider) as the curve `b(a) = 4 + 3a`; the slider goes when nothing else
    /// needs it.
    pub fn as_curve(&self, id: &str) -> Result<Plot, String> {
        let j = self.slider_of(id).ok_or("Das ist keine Zahl aus einem Schieberegler")?;
        let i = self.objs.iter().position(|o| o.id == id).ok_or("Dieses Objekt gibt es nicht mehr")?;
        let (name, var, slider) = (self.objs[i].name(), self.objs[j].name(), self.objs[j].id.clone());
        let def = self.objs[i].def.trim().to_string();
        let rest = def[lang_eq(&def).ok_or("?")? + 1..].trim();
        let mut p = self.set_def_plain(id, &format!("{name}({var}) = {rest}"))?;
        if p.dependents(&slider).is_empty() {
            p = p.remove(&slider);
        }
        p.name_axes(&var, &name);
        Ok(p)
    }

    /// The plot with a slider for each letter `input` uses that names nothing yet (None: there
    /// is none), and those letters.
    fn with_sliders_for(&self, input: &str) -> Option<(Plot, Vec<String>)> {
        let known: std::collections::HashSet<String> = self.objs.iter().map(|o| o.name()).collect();
        let letters = lang::unknown_letters(input, &known);
        if letters.is_empty() {
            return None;
        }
        let mut p = self.clone();
        for l in &letters {
            p = p.add_one(&format!("{l} = 1")).ok()?;
        }
        Some((p, letters))
    }

    /// Schneide/Nullstelle/Extremum/Wendepunkt with several results: one object per point
    /// (Schneide(f, g, 1), Schneide(f, g, 2); Nullstelle(f, x₀) nearest to each).
    fn split_points(&self, input: &str) -> Result<Option<Plot>, String> {
        let known: std::collections::HashSet<String> = self.objs.iter().map(|o| o.name()).collect();
        let Ok(parsed) = lang::parse_input(input, &known) else { return Ok(None) };
        let lang::Body::Expr(lang::Node::Call(cmd, _, args)) = &parsed.body else { return Ok(None) };
        let which = lang::command(cmd);
        let indexed = which == Some("intersect") && args.len() == 2;
        let nearest = matches!(which, Some("root") | Some("extremum") | Some("inflectionpoint")) && args.len() == 1;
        if !indexed && !nearest {
            return Ok(None);
        }
        let (def, _) = new_object(input, &self.objs, self.view())?;
        let mut probe = self.objs.clone();
        probe.push(Obj { id: String::new(), def: def.clone(), style: Style::default(), order: 0.0 });
        let Some(Ok(Val::Points(ps))) = evaluate(&probe, self.view()).pop() else { return Ok(None) };
        if ps.len() < 2 {
            return Ok(None);
        }
        // The call as typed (after "name ="), its closing bracket.
        let body = input.trim();
        let body = match &parsed.name {
            Some(_) => body[body.find('=').map(|i| i + 1).unwrap_or(0)..].trim(),
            None => body,
        };
        let (head, close) = (&body[..body.len() - 1], &body[body.len() - 1..]);
        let mut p = self.clone();
        for (k, q) in ps.iter().enumerate() {
            let extra = if indexed { format!("{}", k + 1) } else { geo::fmt_plain((q.0 * 1e6).round() / 1e6) };
            let call = format!("{head}, {extra}{close}");
            let line = match &parsed.name {
                Some(n) => format!("{n}_{} = {call}", k + 1),
                None => call,
            };
            p = p.add_one(&line)?;
        }
        Ok(Some(p))
    }

    fn add_one(&self, input: &str) -> Result<Plot, String> {
        let (def, mut style) = new_object(input, &self.objs, self.view())?;
        let mut p = self.clone();
        let kind = {
            let mut probe = p.objs.clone();
            probe.push(Obj { id: String::new(), def: def.clone(), style: style.clone(), order: 0.0 });
            evaluate(&probe, self.view()).pop().and_then(|r| r.ok()).map(|v| v.kind())
        };
        style.color = match kind {
            Some(Kind::Point) | Some(Kind::Points) if free_point(&def) => 0xFF15_65C0,
            Some(Kind::Point) | Some(Kind::Points) => 0xFF42_4242,
            Some(Kind::Number) | Some(Kind::Text) | Some(Kind::Bool) => 0xFF20_2428,
            Some(Kind::Angle) => 0xFF2E_7D32,
            Some(Kind::Polygon) => 0xFF8D_6E63,
            _ => {
                let used = p.objs.iter().filter(|o| PALETTE.contains(&o.style.color) && !free_point(&o.def)).count();
                PALETTE[used % PALETTE.len()]
            }
        };
        if matches!(kind, Some(Kind::Polygon) | Some(Kind::Angle) | Some(Kind::Area)) {
            style.fill = 0.35;
        }
        let order = p.objs.iter().map(|o| o.order).fold(0.0, f64::max) + 1.0;
        let id = crate::ids::new_id(order.to_bits() ^ p.objs.len() as u64);
        p.objs.push(Obj { id, def, style, order });
        Ok(p)
    }

    /// The plot with an object's definition changed. Without a name it keeps its old one; with
    /// a new name the objects that use it follow. Refused (Err, German) when the object or one
    /// that depends on it would be wrong afterwards.
    /// Letters that name nothing yet become sliders, as with [`Plot::add`] — and like there, two
    /// of them without x and y (`a = 3b`) make the curve `a(b) = 3b` instead.
    pub fn set_def(&self, id: &str, def: &str) -> Result<Plot, String> {
        match self.set_def_plain(id, def) {
            Ok(p) => Ok(p),
            Err(e) => {
                if let Some(p) = self.curve_of_letters_at(id, def) {
                    return Ok(p);
                }
                match self.with_sliders_for(def) {
                    Some((p, _)) => p.set_def_plain(id, def).map_err(|_| e),
                    None => Err(e),
                }
            }
        }
    }

    fn set_def_plain(&self, id: &str, def: &str) -> Result<Plot, String> {
        let mut p = self.clone();
        let i = p.objs.iter().position(|o| o.id == id).ok_or("Dieses Objekt gibt es nicht mehr")?;
        let old = p.objs[i].name();
        let def = def.trim();
        let known: std::collections::HashSet<String> = self.objs.iter().map(|o| o.name()).collect();
        let def = match name_of(def) {
            Some(_) => def.to_string(),
            // y = 3x without a name: an equation, labelled.
            None if lang::parse_input(def, &known).is_ok_and(|i| matches!(i.body, lang::Body::Equation(..)) || i.polar) => format!("{old}: {def}"),
            None => format!("{old} = {def}"),
        };
        let new = name_of(&def).unwrap_or_default();
        if new != old {
            if known.contains(&new) {
                return Err(format!("Den Namen {new} gibt es schon"));
            }
            for (j, o) in p.objs.iter_mut().enumerate() {
                if j != i {
                    o.def = lang::rename_name(&o.def, &old, &new, &known);
                }
            }
        }
        p.objs[i].def = def;
        let before = self.values();
        let after = p.values();
        if let Err(e) = &after[i] {
            return Err(e.clone());
        }
        for (j, (b, a)) in before.iter().zip(&after).enumerate() {
            if j != i && b.is_ok()
                && let Err(e) = a
            {
                return Err(format!("Dann stimmt {} nicht mehr: {e}", p.objs[j].name()));
            }
        }
        Ok(p)
    }

    /// The objects that depend on `id` (and on those, …), not counting itself.
    pub fn dependents(&self, id: &str) -> Vec<String> {
        let names: Vec<String> = self.objs.iter().map(|o| o.name()).collect();
        let known: std::collections::HashSet<String> = names.iter().cloned().collect();
        let refs: Vec<Vec<String>> = self
            .objs
            .iter()
            .map(|o| {
                let mut out = Vec::new();
                if let Ok(input) = lang::parse_input(&o.def, &known) {
                    let mut walk = vec![];
                    match &input.body {
                        lang::Body::Expr(n) => walk.push(n.clone()),
                        lang::Body::Equation(l, r) => {
                            walk.push(l.clone());
                            walk.push(r.clone());
                        }
                    }
                    walk.extend(input.domain.clone());
                    while let Some(n) = walk.pop() {
                        match n {
                            lang::Node::Name(s) => out.push(s),
                            lang::Node::Call(s, _, args) => {
                                out.push(s);
                                walk.extend(args);
                            }
                            lang::Node::Neg(a) => walk.push(*a),
                            lang::Node::Bin(_, a, b) | lang::Node::Point(a, b) => {
                                walk.push(*a);
                                walk.push(*b);
                            }
                            lang::Node::Chain(items, _) | lang::Node::List(items) => walk.extend(items),
                            _ => {}
                        }
                    }
                }
                out
            })
            .collect();
        let Some(start) = self.objs.iter().position(|o| o.id == id) else { return vec![] };
        let mut gone = vec![names[start].clone()];
        let mut out = Vec::new();
        loop {
            let before = out.len();
            for (i, r) in refs.iter().enumerate() {
                if !out.contains(&self.objs[i].id) && self.objs[i].id != id && r.iter().any(|n| gone.contains(n)) {
                    out.push(self.objs[i].id.clone());
                    gone.push(names[i].clone());
                }
            }
            if out.len() == before {
                return out;
            }
        }
    }

    /// The plot without an object and everything that depends on it (as GeoGebra deletes).
    pub fn remove(&self, id: &str) -> Plot {
        let gone = self.dependents(id);
        let mut p = self.clone();
        p.objs.retain(|o| o.id != id && !gone.contains(&o.id));
        p
    }

    /// The y range for the same units as across (keeping the middle of the current one).
    pub fn equal_units(&self) -> (f64, f64) {
        let per_pt = (self.x_max - self.x_min) / self.w as f64;
        let half = per_pt * self.h as f64 / 2.0;
        let mid = (self.y_min + self.y_max) / 2.0;
        (mid - half, mid + half)
    }

    /// A y range that shows the functions and points over the x range (outliers like poles left
    /// out), rounded to the ticks; None if there is nothing to show.
    pub fn fit_y(&self) -> Option<(f64, f64)> {
        let mut ys = Vec::new();
        for v in self.values().into_iter().flatten() {
            match v {
                Val::Func(e) => {
                    for i in 0..=400 {
                        let x = self.x_min + (self.x_max - self.x_min) * i as f64 / 400.0;
                        let y = e.eval(x);
                        if y.is_finite() {
                            ys.push(y);
                        }
                    }
                }
                Val::Point(p) => ys.push(p.1),
                Val::Points(ps) => ys.extend(ps.iter().map(|p| p.1)),
                _ => {}
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

    /// The x range for steps of π: the current one widened to whole π/2.
    pub fn pi_range(&self) -> (f64, f64) {
        let h = std::f64::consts::FRAC_PI_2;
        ((self.x_min / h).floor() * h, (self.x_max / h).ceil() * h)
    }

    /// Plot units per page point across and down.
    pub fn units_per_pt(&self) -> (f64, f64) {
        ((self.x_max - self.x_min) / self.w as f64, (self.y_max - self.y_min) / self.h as f64)
    }

    /// A page point (x, y) in plot units.
    pub fn to_plot(&self, x: f32, y: f32) -> (f64, f64) {
        let (ux, uy) = self.units_per_pt();
        (self.x_min + (x - self.x) as f64 * ux, self.y_max - (y - self.y) as f64 * uy)
    }

    /// A plot point in page points.
    pub fn to_page(&self, x: f64, y: f64) -> (f32, f32) {
        let (ux, uy) = self.units_per_pt();
        ((self.x as f64 + (x - self.x_min) / ux) as f32, (self.y as f64 + (self.y_max - y) / uy) as f32)
    }

    /// Where a point put down at (x, y) (plot units) lands with the plot's Punktfang: on the
    /// nearest grid point when it snaps (automatic: only when it is near one).
    pub fn snapped(&self, x: f64, y: f64) -> (f64, f64) {
        // Automatic: with a grid shown, or on squared paper the plot keeps to.
        let on = match self.snap {
            Snap::Off => false,
            Snap::Grid => true,
            Snap::Auto => self.grid || self.align,
        };
        if !on {
            return (x, y);
        }
        let (sx, _, _) = render::ticks_of(self.x_min, self.x_max, self.w, self.pi);
        let (sy, _, _) = render::ticks_of(self.y_min, self.y_max, self.h, false);
        let (gx, gy) = ((x / sx).round() * sx, (y / sy).round() * sy);
        let (ux, uy) = self.units_per_pt();
        if self.snap == Snap::Auto && ((gx - x) / ux).hypot((gy - y) / uy) > (sx / ux).min(sy / uy) * 0.3 {
            return (x, y);
        }
        (gx, gy)
    }

    /// The plot with an object dragged to (x, y) (plot units): a free point moves there
    /// (snapped), a point on an object moves along it to the nearest place. None if the object
    /// can't be dragged.
    pub fn drag(&self, id: &str, x: f64, y: f64) -> Option<Plot> {
        let i = self.objs.iter().position(|o| o.id == id)?;
        let o = &self.objs[i];
        if o.style.fixed {
            return None;
        }
        let mut p = self.clone();
        if free_point(&o.def) {
            let (sx, sy) = self.snapped(x, y);
            p.objs[i].def = moved_point(&o.def, sx, sy);
            return Some(p);
        }
        if moved_on(&o.def, 0.0).is_some() {
            let vals = self.values();
            let t = place_on(&self.objs, &vals, i, x, y)?;
            p.objs[i].def = moved_on(&o.def, t)?;
            return Some(p);
        }
        None
    }

    /// The plot with a new point on the object `on` (a function, line, segment or circle) at
    /// the place nearest to (x, y): `P = Punkt(f, 1.5)`, draggable along it.
    pub fn add_point_on(&self, on: &str, x: f64, y: f64) -> Result<Plot, String> {
        let i = self.objs.iter().position(|o| o.id == on).ok_or("Dieses Objekt gibt es nicht mehr")?;
        let name = self.objs[i].name();
        let vals = self.values();
        let t = match vals[i].as_ref().map_err(|e| e.clone())? {
            Val::Func(_) => x,
            Val::Circle(c, _) => (y - c.1).atan2(x - c.0),
            Val::Segment(p, q) => {
                let (dx, dy) = (q.0 - p.0, q.1 - p.1);
                (((x - p.0) * dx + (y - p.1) * dy) / (dx * dx + dy * dy).max(1e-24)).clamp(0.0, 1.0)
            }
            Val::Line { a, b, c } => {
                let n2 = a * a + b * b;
                let p0 = (a * c / n2, b * c / n2);
                let d = (-b / n2.sqrt(), a / n2.sqrt());
                (x - p0.0) * d.0 + (y - p0.1) * d.1
            }
            _ => return self.add(&format!("({}, {})", geo::fmt_plain(x), geo::fmt_plain(y))),
        };
        self.add(&format!("Punkt({name}, {})", geo::fmt_plain((t * 1e6).round() / 1e6)))
    }

    /// The plot with a slider set to `value` (kept in its range, on its steps).
    pub fn set_slider(&self, id: &str, value: f64) -> Option<Plot> {
        let i = self.objs.iter().position(|o| o.id == id)?;
        let s = self.objs[i].style.slider?;
        // A range from another device or an old version can be anything (f64::clamp panics on
        // NaN or reversed bounds).
        if !(s.min.is_finite() && s.max.is_finite() && value.is_finite()) {
            return None;
        }
        let v = value.clamp(s.min.min(s.max), s.max.max(s.min));
        let v = if s.step > 0.0 { s.min + ((v - s.min) / s.step).round() * s.step } else { v };
        let mut p = self.clone();
        p.objs[i].def = format!("{} = {}", self.objs[i].name(), geo::fmt_plain((v * 1e9).round() / 1e9));
        Some(p)
    }

    // --- a function's domain ---------------------------------------------------------------

    /// The domain of function `id`: from, to (each the number and whether it belongs to it;
    /// None: open that way). None: the object is no function.
    pub fn domain(&self, id: &str) -> Option<(End, End)> {
        let i = self.objs.iter().position(|o| o.id == id)?;
        match &self.values()[i] {
            Ok(Val::Func(e)) => Some(domain_parts(e).map_or((None, None), |(_, lo, hi)| (lo, hi))),
            _ => None,
        }
    }

    /// Function `id` defined only from `lo` to `hi` (None: open that way, both: everywhere),
    /// written into its definition as in school: `f(x) = x², 0 ≤ x ≤ 2`. An end keeps whether
    /// it belongs (≤ or <); a new one belongs.
    pub fn set_domain(&self, id: &str, lo: Option<f64>, hi: Option<f64>) -> Result<Plot, String> {
        let i = self.objs.iter().position(|o| o.id == id).ok_or("Dieses Objekt gibt es nicht mehr")?;
        let (old_lo, old_hi) = self.domain(id).ok_or("Nur eine Funktion hat einen Definitionsbereich")?;
        if let (Some(a), Some(b)) = (lo, hi)
            && b <= a
        {
            return Err("„bis“ muss größer sein als „von“".into());
        }
        let known: HashSet<String> = self.objs.iter().map(|o| o.name()).collect();
        let lo = lo.map(|v| (v, old_lo.is_none_or(|l| l.1)));
        let hi = hi.map(|v| (v, old_hi.is_none_or(|h| h.1)));
        let def = geo::with_domain(&self.objs[i].def, lo, hi, &known)?;
        self.set_def_plain(id, &def)
    }

    /// The end of a function's domain under page point (x, y) (within `tol` page points): the
    /// function's id and 0 (from) or 1 (to). Fixed and hidden functions have none to drag.
    pub fn domain_end_at(&self, x: f32, y: f32, tol: f32) -> Option<(String, u8)> {
        let vals = self.values();
        let mut best: Option<(f32, String, u8)> = None;
        for (o, v) in self.objs.iter().zip(&vals) {
            let Ok(Val::Func(e)) = v else { continue };
            if o.style.fixed || !o.style.visible {
                continue;
            }
            let Some((f, lo, hi)) = domain_parts(e) else { continue };
            for (side, end) in [(0u8, lo), (1, hi)] {
                let Some((bx, _)) = end else { continue };
                let by = f.eval(bx);
                if !by.is_finite() {
                    continue;
                }
                let (px, py) = self.to_page(bx, by);
                let d = (px - x).hypot(py - y);
                if d <= tol && best.as_ref().is_none_or(|b| d < b.0) {
                    best = Some((d, o.id.clone(), side));
                }
            }
        }
        best.map(|b| (b.1, b.2))
    }

    /// The plot with end `side` (0 from, 1 to) of function `id`'s domain dragged to x (plot
    /// units; on the grid's steps when points snap there). None: it can't go there.
    pub fn drag_domain_end(&self, id: &str, side: u8, x: f64) -> Option<Plot> {
        let (lo, hi) = self.domain(id)?;
        let on = match self.snap {
            Snap::Off => false,
            Snap::Grid => true,
            Snap::Auto => self.grid || self.align,
        };
        let (sx, _, _) = render::ticks_of(self.x_min, self.x_max, self.w, self.pi);
        let g = (x / sx).round() * sx;
        let x = if on && (self.snap == Snap::Grid || ((g - x) / sx).abs() < 0.3) { g } else { (x * 1e6).round() / 1e6 };
        let tiny = (self.x_max - self.x_min) * 1e-3;
        match side {
            0 if hi.is_none_or(|h| x < h.0 - tiny) => self.set_domain(id, Some(x), hi.map(|h| h.0)).ok(),
            1 if lo.is_none_or(|l| x > l.0 + tiny) => self.set_domain(id, lo.map(|l| l.0), Some(x)).ok(),
            _ => None,
        }
    }

    // --- stretching an axis ------------------------------------------------------------------

    /// Where the axes are drawn on the page: the y axis' x and the x axis' y (at 0, else at the
    /// box's edge nearest to it).
    pub fn axis_lines(&self) -> (f32, f32) {
        let ax = if self.x_min <= 0.0 && self.x_max >= 0.0 { self.to_page(0.0, 0.0).0 } else if self.x_max < 0.0 { self.x + self.w } else { self.x };
        let ay = if self.y_min <= 0.0 && self.y_max >= 0.0 { self.to_page(0.0, 0.0).1 } else if self.y_max < 0.0 { self.y } else { self.y + self.h };
        (ax, ay)
    }

    /// The axis under page point (x, y), within `tol` page points (its numbers count too); None
    /// elsewhere and where the two cross.
    pub fn axis_at(&self, x: f32, y: f32, tol: f32) -> Option<Axis> {
        if !self.axes || !self.is_valid() {
            return None;
        }
        let (ax, ay) = self.axis_lines();
        let cap = (self.w.min(self.h) * 0.045).clamp(5.0, 8.0);
        if (x - ax).abs() < tol * 1.5 && (y - ay).abs() < tol * 1.5 {
            return None;
        }
        let along_x = x >= self.x - tol && x <= self.x + self.w + tol;
        let along_y = y >= self.y - tol && y <= self.y + self.h + tol;
        let on_x = along_x && y >= ay - tol && y <= ay + tol + cap * 1.4;
        let on_y = along_y && x <= ax + tol && x >= ax - tol - cap * 2.5;
        match (on_x, on_y) {
            (true, false) => Some(Axis::X),
            (false, true) => Some(Axis::Y),
            (true, true) => Some(if (y - ay).abs() <= (x - ax).abs() { Axis::X } else { Axis::Y }),
            _ => None,
        }
    }

    /// The plot with `axis` grabbed at page position `from` and dragged to `to` (page x for the
    /// x axis, page y for the y axis), as GeoGebra stretches an axis: only that axis changes,
    /// the number grabbed follows the pen, 0 (or the end of the axis nearest to it) stays. The
    /// other axis keeps its units, so they may differ (y in hundreds, x in ones).
    pub fn axis_dragged(&self, axis: Axis, from: f32, to: f32) -> Plot {
        let (lo, hi) = match axis {
            Axis::X => (self.x_min, self.x_max),
            Axis::Y => (self.y_min, self.y_max),
        };
        if !self.is_valid() || !from.is_finite() || !to.is_finite() {
            return self.clone();
        }
        let at = 0f64.clamp(lo, hi);
        let pa = match axis {
            Axis::X => self.to_page(at, 0.0).0,
            Axis::Y => self.to_page(0.0, at).1,
        };
        let (df, dt) = ((from - pa) as f64, (to - pa) as f64);
        // Too near the fixed point to stretch from; never through it.
        let near = 4.0;
        if df.abs() < near {
            return self.clone();
        }
        let dt = if dt.signum() != df.signum() || dt.abs() < near { near * df.signum() } else { dt };
        let (l2, h2) = stretched(lo, hi, at, (df / dt).clamp(1e-3, 1e3));
        if !(h2 - l2 > 1e-9 && h2 - l2 < 1e12) {
            return self.clone();
        }
        let clean = |v: f64| (v * 1e9).round() / 1e9;
        let mut p = self.clone();
        match axis {
            Axis::X => (p.x_min, p.x_max) = (clean(l2), clean(h2)),
            Axis::Y => (p.y_min, p.y_max) = (clean(l2), clean(h2)),
        }
        p
    }
}

/// One of a plot's axes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
}

/// The range `lo` … `hi` stretched by `k` about `at` (k > 1: more of the axis shows; `at`
/// stays where it is).
pub fn stretched(lo: f64, hi: f64, at: f64, k: f64) -> (f64, f64) {
    (at + (lo - at) * k, at + (hi - at) * k)
}

/// The top-level "=" of a definition (not in <=, >=).
fn lang_eq(s: &str) -> Option<usize> {
    lang::top_level_eq(s)
}

/// The parts of an input: its body's sides and its domain.
fn nodes_of(i: &lang::Input) -> Vec<&lang::Node> {
    let mut out = match &i.body {
        lang::Body::Expr(n) => vec![n],
        lang::Body::Equation(l, r) => vec![l, r],
    };
    out.extend(i.domain.as_ref());
    out
}

/// Whether a node uses x, y, t (or another variable).
fn has_var(n: &lang::Node) -> bool {
    use lang::Node;
    match n {
        Node::Var(_) => true,
        Node::Num(_) | Node::Name(_) | Node::Text(_) => false,
        Node::Neg(a) => has_var(a),
        Node::Bin(_, a, b) | Node::Point(a, b) => has_var(a) || has_var(b),
        Node::Call(_, _, args) | Node::Chain(args, _) | Node::List(args) => args.iter().any(has_var),
    }
}

/// The names a node refers to.
fn collect_names(n: &lang::Node, out: &mut Vec<String>) {
    geo::node_refs(n, out);
}

/// For a point on an object (object `i`, Punkt(obj, t)): the t nearest to (x, y).
fn place_on(objs: &[Obj], vals: &[Result<Val, String>], i: usize, x: f64, y: f64) -> Option<f64> {
    let def = &objs[i].def;
    let eq = def.find('=')?;
    let body = &def[eq + 1..];
    let open = body.find(['(', '['])?;
    let target = body[open + 1..body.rfind(',')?].trim();
    let j = objs.iter().position(|o| o.name() == target)?;
    match vals.get(j)?.as_ref().ok()? {
        Val::Func(_) => Some(x),
        Val::Circle(c, _) => Some((y - c.1).atan2(x - c.0)),
        Val::Segment(p, q) => {
            let (dx, dy) = (q.0 - p.0, q.1 - p.1);
            let l2 = (dx * dx + dy * dy).max(1e-24);
            Some((((x - p.0) * dx + (y - p.1) * dy) / l2).clamp(0.0, 1.0))
        }
        Val::Line { a, b, c } => {
            let n2 = a * a + b * b;
            let p0 = (a * c / n2, b * c / n2);
            let d = (-b / n2.sqrt(), a / n2.sqrt());
            Some((x - p0.0) * d.0 + (y - p0.1) * d.1)
        }
        _ => None,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn plot() -> Plot {
        Plot::centered(100.0, 100.0, 240.0, 160.0, 20.0, 0xFF22_2222, 1.2)
    }

    #[test]
    fn objects_are_added_named_coloured_and_removed_with_what_depends_on_them() {
        let p = plot().add("A = (1, 2)").unwrap().add("(4, 6)").unwrap().add("Gerade(A, B)").unwrap().add("x^2").unwrap().add("Schneide(f, g)").unwrap();
        let names: Vec<String> = p.objs.iter().map(|o| o.name()).collect();
        assert_eq!(names, vec!["A", "B", "f", "g", "C", "D"], "{:?}", p.objs.iter().map(|o| &o.def).collect::<Vec<_>>());
        assert!(p.errors().iter().all(|e| e.is_none()), "{:?}", p.errors());
        assert_ne!(p.objs[2].style.color, p.objs[3].style.color, "lines and functions get colours in turn");
        assert_eq!(p.objs[0].style.color, 0xFF15_65C0, "free points are blue");
        // Removing A takes the line through it and the intersection with it.
        let q = p.remove(&p.objs[0].id);
        assert_eq!(q.objs.iter().map(|o| o.name()).collect::<Vec<_>>(), vec!["B", "g"]);
        // A wrong definition is refused, a right one taken (the name stays).
        assert!(p.set_def(&p.objs[3].id, "x^").is_err());
        let r = p.set_def(&p.objs[3].id, "x^2 + 1").unwrap();
        assert_eq!(r.objs[3].def, "g = x^2 + 1");
        assert!(p.set_def(&p.objs[3].id, "x^3").is_err(), "x³ meets the line once: the second point would be wrong");
    }

    #[test]
    fn several_points_become_several_objects_and_redefining_is_checked() {
        let p = plot().add("f(x) = x^2 - 4").unwrap().add("g(x) = x").unwrap().add("Schneide(f, g)").unwrap().add("Nullstelle(f)").unwrap();
        let defs: Vec<String> = p.objs.iter().map(|o| o.def.clone()).collect();
        assert_eq!(defs[2..], ["A = Schneide(f, g, 1)", "B = Schneide(f, g, 2)", "C = Nullstelle(f, -2)", "D = Nullstelle(f, 2)"], "{defs:?}");
        assert!(p.errors().iter().all(|e| e.is_none()), "{:?}", p.errors());
        let q = p.add("Gerade(A, B)").unwrap();
        assert!(q.errors().iter().all(|e| e.is_none()));
        // Named: S_1, S_2.
        let r = plot().add("f(x) = x^2 - 4").unwrap().add("g(x) = x").unwrap().add("S = Schneide(f, g)").unwrap();
        assert_eq!(r.objs[2].name(), "S_1");
        assert_eq!(r.objs[3].name(), "S_2");
        // An equation without a name keeps the old one; a change that breaks others is refused;
        // a new name is taken over where it is used.
        let id = q.objs[1].id.clone();
        assert_eq!(q.set_def(&id, "y = 3x").unwrap().objs[1].def, "g: y = 3x");
        let a = q.objs[2].id.clone();
        assert!(q.set_def(&a, "A = 3").unwrap_err().starts_with("Dann stimmt"), "a number can't be a line's point");
        let renamed = q.set_def(&a, "P = Schneide(f, g, 1)").unwrap();
        assert!(renamed.objs.iter().any(|o| o.def == "h = Gerade(P, B)"), "{:?}", renamed.objs.iter().map(|o| &o.def).collect::<Vec<_>>());
        assert!(q.set_def(&a, "B = (1, 1)").is_err(), "B is taken");
    }

    #[test]
    fn letters_that_name_nothing_become_sliders_like_in_geogebra() {
        let defs = |p: &Plot| p.objs.iter().map(|o| o.def.clone()).collect::<Vec<_>>();
        // b = 4 + 3a is a curve now (two letters, no x, y); as sliders: a slider a at 1
        // (−5 … 5), b follows it.
        let p = plot().add("b = 4 + 3a").unwrap();
        assert_eq!(defs(&p), ["b(a) = 4 + 3a"]);
        let p = p.as_sliders(&p.objs[0].id).unwrap();
        assert_eq!(defs(&p), ["b = 4 + 3a", "a = 1"]);
        let s = p.objs[1].style.slider.unwrap();
        assert_eq!((s.min, s.max, s.step), (-5.0, 5.0, 0.1));
        assert_eq!(p.values()[0], Ok(Val::Num(7.0)));
        assert_eq!(p.set_slider(&p.objs[1].id, 2.0).unwrap().values()[0], Ok(Val::Num(10.0)));
        // A line, a parabola in vertex form, a circle: every letter gets its slider, x and y none.
        assert_eq!(defs(&plot().add("y = m x + c").unwrap()), ["m = 1", "c = 1", "f: y = m x + c"]);
        let q = plot().add("f(x) = a(x - d)^2 + k").unwrap();
        assert_eq!(defs(&q)[..3], ["a = 1", "d = 1", "k = 1"]);
        assert!(q.errors().iter().all(|e| e.is_none()), "{:?}", q.errors());
        assert_eq!(plot().add("x^2 + y^2 = r^2").unwrap().objs.len(), 2);
        assert_eq!(defs(&plot().add("y = a_1 x").unwrap())[0], "a_1 = 1");
        // What exists is used; a letter alone is just its slider.
        assert_eq!(plot().add("a = 2").unwrap().add("g(x) = a x").unwrap().objs.len(), 2);
        assert_eq!(defs(&plot().add("a").unwrap()), ["a = 1"]);
        // A typing mistake stays a mistake (nothing is made).
        assert!(plot().add("Gerde(A, B)").is_err());
        assert!(plot().add("y = 2x +").is_err());
        // Editing: f(x) = x² becomes f(x) = k x², k is made.
        let p = plot().add("f(x) = x^2").unwrap();
        let q = p.set_def(&p.objs[0].id, "f(x) = k x^2").unwrap();
        assert_eq!(defs(&q), ["f(x) = k x^2", "k = 1"]);
        assert!(q.errors().iter().all(|e| e.is_none()));
    }

    #[test]
    fn points_drag_and_snap_sliders_step() {
        let mut p = plot().add("A = (1, 2)").unwrap().add("f(x) = x^2").unwrap().add("P = Punkt(f, 1)").unwrap().add("a = Schieberegler(0, 10, 0.5)").unwrap();
        // With the grid on and automatic snapping, near a grid point it lands on it.
        p.grid = true;
        let q = p.drag(&p.objs[0].id, 2.93, -1.04).unwrap();
        assert_eq!(q.objs[0].def, "A = (3, -1)");
        let q = p.drag(&p.objs[0].id, 2.5, -1.5).unwrap();
        assert_eq!(q.objs[0].def, "A = (2.5, -1.5)", "between grid points it stays where it is");
        p.snap = Snap::Off;
        assert_eq!(p.drag(&p.objs[0].id, 2.93, -1.04).unwrap().objs[0].def, "A = (2.93, -1.04)");
        // On f: along the curve.
        let q = p.drag(&p.objs[2].id, 2.0, 0.0).unwrap();
        assert_eq!(q.values()[2], Ok(Val::Point((2.0, 4.0))));
        // A slider: in range, on its steps.
        let q = p.set_slider(&p.objs[3].id, 3.37).unwrap();
        assert_eq!(q.objs[3].def, "a = 3.5");
        assert_eq!(p.set_slider(&p.objs[3].id, 99.0).unwrap().objs[3].def, "a = 10");
        // A fixed point doesn't move.
        let mut fixed = p.clone();
        fixed.objs[0].style.fixed = true;
        assert!(fixed.drag(&fixed.objs[0].id, 0.0, 0.0).is_none());
        let page = p.to_page(1.0, 2.0);
        let back = p.to_plot(page.0, page.1);
        assert!((back.0 - 1.0).abs() < 1e-5 && (back.1 - 2.0).abs() < 1e-5);
    }

    #[test]
    fn ranges_fit_the_functions() {
        let p = plot().add("x^2").unwrap();
        let (lo, hi) = p.fit_y().unwrap();
        assert!(lo <= 0.0 && lo >= -5.0 && hi >= 30.0 && hi <= 40.0, "{lo} {hi}");
        assert!(plot().fit_y().is_none());
        let mut p = plot();
        p.h = 120.0;
        assert_eq!(p.equal_units(), (-3.0, 3.0));
        let (a, b) = Plot { x_min: -5.0, x_max: 7.0, ..plot() }.pi_range();
        assert!((a + 2.0 * std::f64::consts::PI).abs() < 1e-12 && (b - 2.5 * std::f64::consts::PI).abs() < 1e-12);
    }

    fn func(p: &Plot, i: usize) -> Expr {
        match p.values()[i].clone() {
            Ok(Val::Func(e)) => e,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn functions_take_a_domain_written_as_in_school() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        // f(x) = x², 0 ≤ x ≤ 2: undefined outside, the ends belong to it.
        let p = plot().add("f(x) = x^2, 0 ≤ x ≤ 2").unwrap();
        let f = func(&p, 0);
        assert!(f.eval(-0.5).is_nan() && f.eval(2.5).is_nan());
        assert!(close(f.eval(0.0), 0.0) && close(f.eval(2.0), 4.0) && close(f.eval(1.0), 1.0));
        assert_eq!(domain_ends(&f), vec![(0.0, 0.0, true), (2.0, 4.0, true)]);
        // für / for, < and >, <= and >=, the domain in brackets after it.
        for (src, inside, outside) in [
            ("f(x) = x² für 0 < x < 2", 1.0, 0.0),
            ("g(x) = x^2 for x > 1", 3.0, 1.0),
            ("x^2 (x <= -1)", -2.0, 0.0),
            ("h(x) = 2x, -1 ≤ x", -1.0, -1.5),
            ("y = 2x + 1, 0 <= x <= 3", 3.0, 4.0),
        ] {
            let p = plot().add(src).unwrap_or_else(|e| panic!("{src}: {e}"));
            let f = func(&p, 0);
            assert!(f.eval(inside).is_finite() && f.eval(outside).is_nan(), "{src}");
        }
        let open = func(&plot().add("f(x) = x² für 0 < x < 2").unwrap(), 0);
        assert_eq!(domain_ends(&open), vec![(0.0, 0.0, false), (2.0, 4.0, false)]);
        // Decimal commas, points and Wenn are no domains.
        assert!(close(func(&plot().add("f(x) = 0,5x").unwrap(), 0).eval(2.0), 1.0));
        assert!(func(&plot().add("Wenn(x < 0, -x, x^2)").unwrap(), 0).eval(-2.0) == 2.0);
        assert!(plot().add("f(x) = x^2, 0").is_err());
        // A letter in it becomes a slider; the slider is what it depends on.
        let q = plot().add("f(x) = x^2, 0 ≤ x ≤ a").unwrap();
        assert_eq!(q.objs[0].def, "a = 1");
        assert_eq!(q.dependents(&q.objs[0].id), vec![q.objs[1].id.clone()]);
        assert!(func(&q, 1).eval(1.5).is_nan());
        // The row shows it like that; so does the preview while typing.
        let known: HashSet<String> = p.objs.iter().map(|o| o.name()).collect();
        assert_eq!(row_latex(&p.objs[0], &p.values()[0].clone().unwrap(), &known), "f(x) = x^{2},\\; 0 \\le x \\le 2");
        assert_eq!(geo::latex_input("f(x) = x^2 für 0 < x < 2", &HashSet::new()).as_deref(), Some("f(x)=x^{2},\\;0<x<2"));
        // Funktion(…) has its domain too.
        let r = plot().add("f(x) = Funktion(x^2, 0, 2)").unwrap();
        assert_eq!(r.domain(&r.objs[0].id), Some((Some((0.0, true)), Some((2.0, true)))));
    }

    #[test]
    fn a_domain_is_set_in_the_settings_and_its_ends_drag() {
        let p = plot().add("f(x) = x^2").unwrap();
        let id = p.objs[0].id.clone();
        assert_eq!(p.domain(&id), Some((None, None)));
        let q = p.set_domain(&id, Some(-1.0), Some(1.5)).unwrap();
        assert_eq!(q.objs[0].def, "f(x) = x^2, -1 ≤ x ≤ 1.5");
        assert_eq!(q.set_domain(&id, None, Some(2.0)).unwrap().objs[0].def, "f(x) = x^2, x ≤ 2");
        assert_eq!(q.set_domain(&id, None, None).unwrap().objs[0].def, "f(x) = x^2");
        assert!(q.set_domain(&id, Some(2.0), Some(1.0)).is_err());
        // An end keeps whether it belongs.
        let open = plot().add("f(x) = x^2 für 0 < x < 2").unwrap();
        assert_eq!(open.set_domain(&open.objs[0].id, Some(1.0), Some(3.0)).unwrap().objs[0].def, "f(x) = x^2, 1 < x < 3");
        // Funktion(x^2, 0, 2) is written the school way when its domain changes.
        let r = plot().add("f(x) = Funktion(x^2, 0, 2)").unwrap();
        assert_eq!(r.set_domain(&r.objs[0].id, Some(0.0), Some(3.0)).unwrap().objs[0].def, "f(x) = x², 0 ≤ x ≤ 3");
        // Only functions have one.
        let a = plot().add("A = (1, 2)").unwrap();
        assert!(a.domain(&a.objs[0].id).is_none() && a.set_domain(&a.objs[0].id, Some(0.0), None).is_err());
        // The end at (1,5 | 2,25) is found on the page and dragged; on the grid it snaps.
        let (ex, ey) = q.to_page(1.5, 2.25);
        assert_eq!(q.domain_end_at(ex + 1.0, ey, 6.0), Some((id.clone(), 1)));
        assert_eq!(q.domain_end_at(ex + 30.0, ey, 6.0), None);
        let mut g = q.clone();
        g.snap = Snap::Off;
        assert_eq!(g.drag_domain_end(&id, 1, 2.37).unwrap().objs[0].def, "f(x) = x^2, -1 ≤ x ≤ 2.37");
        g.grid = true;
        g.snap = Snap::Auto;
        assert_eq!(g.drag_domain_end(&id, 1, 2.93).unwrap().objs[0].def, "f(x) = x^2, -1 ≤ x ≤ 3");
        assert!(g.drag_domain_end(&id, 1, -2.0).is_none(), "not past the other end");
        // The graph marks the ends: a filled dot where it belongs, a ring where not.
        let mut d = plot().add("f(x) = 0.5x^2, -2 ≤ x < 2").unwrap();
        d.id = "d".into();
        let color = d.objs[0].style.color;
        let ink = render(&d);
        let near = |x: f64, y: f64| {
            let (px, py) = d.to_page(x, y);
            ink.iter().filter(|(b, pts)| b.color == color && pts.iter().all(|q| (q.x - px).hypot(q.y - py) < 4.0)).count()
        };
        assert!(near(-2.0, 2.0) >= 1 && near(2.0, 2.0) >= 1, "both ends marked");
    }

    #[test]
    fn an_axis_stretches_on_its_own() {
        assert_eq!(stretched(-5.0, 5.0, 0.0, 2.0), (-10.0, 10.0));
        assert_eq!(stretched(2.0, 6.0, 2.0, 0.5), (2.0, 4.0));
        let p = plot(); // x −6 … 6, y −4 … 4
        let (ax, ay) = p.axis_lines();
        assert_eq!(p.to_page(0.0, 0.0), (ax, ay));
        // Where the pen is: on the x axis (or its numbers), on the y axis, where they cross.
        assert_eq!(p.axis_at(p.to_page(3.0, 0.0).0, ay + 1.0, 4.0), Some(Axis::X));
        assert_eq!(p.axis_at(p.to_page(3.0, 0.0).0, ay + 7.0, 4.0), Some(Axis::X), "on its numbers");
        assert_eq!(p.axis_at(ax - 1.0, p.to_page(0.0, 2.0).1, 4.0), Some(Axis::Y));
        assert_eq!(p.axis_at(ax + 1.0, ay - 1.0, 4.0), None, "where they cross");
        assert_eq!(p.axis_at(p.to_page(3.0, 2.0).0, p.to_page(3.0, 2.0).1, 4.0), None);
        // The 2 on the x axis pulled to where the 1 was: x shows twice as much, y stays.
        let q = p.axis_dragged(Axis::X, p.to_page(2.0, 0.0).0, p.to_page(1.0, 0.0).0);
        assert_eq!((q.x_min, q.x_max, q.y_min, q.y_max), (-12.0, 12.0, -4.0, 4.0));
        // The 1 on the y axis pulled up to where the 2 was: y shows half as much.
        let r = p.axis_dragged(Axis::Y, p.to_page(0.0, 1.0).1, p.to_page(0.0, 2.0).1);
        assert_eq!((r.x_min, r.x_max, r.y_min, r.y_max), (-6.0, 6.0, -2.0, 2.0));
        // Never through 0 (it stays put), never from right at it.
        let s = p.axis_dragged(Axis::X, p.to_page(2.0, 0.0).0, p.to_page(-3.0, 0.0).0);
        assert!(s.x_max > s.x_min && s.x_max > 0.0 && s.x_min < 0.0);
        assert_eq!(p.axis_dragged(Axis::X, ax + 1.0, ax + 50.0), p);
        // Without 0 in view the end nearest to it stays.
        let far = Plot { x_min: 2.0, x_max: 8.0, ..plot() };
        let t = far.axis_dragged(Axis::X, far.to_page(5.0, 0.0).0, far.to_page(8.0, 0.0).0);
        assert!((t.x_min - 2.0).abs() < 1e-9 && (t.x_max - 5.0).abs() < 1e-9, "{t:?}");
        // Each axis picks its own round steps: y in hundreds, x in ones.
        let tall = Plot { y_min: -400.0, y_max: 400.0, ..plot() };
        let (sx, _, _) = render::ticks_of(tall.x_min, tall.x_max, tall.w, false);
        let (sy, _, ys) = render::ticks_of(tall.y_min, tall.y_max, tall.h, false);
        assert_eq!((sx, sy), (1.0, 100.0));
        assert!(ys.contains(&"300".to_string()));
    }

    #[test]
    fn more_kinds_are_drawn_when_typed() {
        let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
        // x y = 1: a curve of its own (a hyperbola), traced over the view.
        let mut p = plot().add("x y = 1").unwrap().add("x^2 + 4y^2 = 4").unwrap();
        p.id = "k".into();
        assert!(matches!(p.values()[0], Ok(Val::Implicit(_))) && matches!(p.values()[1], Ok(Val::Implicit(_))));
        let ink = render(&p);
        assert!(ink.iter().any(|(b, pts)| b.color == p.objs[0].style.color && pts.len() > 10));
        // (cos t, sin t): a curve in parameter form, a full turn; (t, t²) from −10 to 10;
        // a range written after it; Kurve(…) also without the t.
        let c = plot().add("(cos t, sin t)").unwrap().add("(t, t^2)").unwrap().add("(cos t, sin t), 0 ≤ t ≤ pi").unwrap().add("Kurve(t, 2t, 0, 1)").unwrap();
        let ranges: Vec<(f64, f64)> = c.values().into_iter().map(|v| match v {
            Ok(Val::Curve(_, _, a, b)) => (a, b),
            other => panic!("{other:?}"),
        }).collect();
        assert!(close(ranges[0].1, 2.0 * std::f64::consts::PI) && ranges[1] == (-10.0, 10.0) && close(ranges[2].1, std::f64::consts::PI) && ranges[3] == (0.0, 1.0));
        let known: HashSet<String> = c.objs.iter().map(|o| o.name()).collect();
        assert_eq!(row_latex(&c.objs[0], &c.values()[0].clone().unwrap(), &known), "c{:}\\; \\left(\\cos(t), \\sin(t)\\right),\\; 0 \\le t \\le 2\\pi ");
        // r = 1 + cos θ: a curve in polar coordinates (a cardioid: r = 2 at θ = 0).
        let r = plot().add("r = 1 + cos θ").unwrap().add("r = 2 theta").unwrap();
        let Ok(Val::Curve(fx, fy, a, b)) = r.values()[0].clone() else { panic!("{:?}", r.values()) };
        assert!(close(fx.eval(0.0), 2.0) && close(fy.eval(0.0), 0.0) && a == 0.0 && close(b, 2.0 * std::f64::consts::PI));
        assert!(matches!(r.values()[1], Ok(Val::Curve(..))));
        let known: HashSet<String> = r.objs.iter().map(|o| o.name()).collect();
        assert_eq!(row_text(&r.objs[0], &r.values()[0].clone().unwrap(), &known, false), "c: r = 1 + cos(θ)");
        assert!(row_latex(&r.objs[0], &r.values()[0].clone().unwrap(), &known).contains("\\theta"));
        // Without θ, r = 2 is a number as before.
        assert_eq!(plot().add("r = 2").unwrap().values()[0], Ok(Val::Num(2.0)));
    }

    #[test]
    fn two_letters_without_x_and_y_make_a_curve_with_a_switch_to_sliders() {
        let defs = |p: &Plot| p.objs.iter().map(|o| o.def.clone()).collect::<Vec<_>>();
        // a = 3b: b across, a up, the axes say so.
        let p = plot().add("a = 3b").unwrap();
        assert_eq!(defs(&p), ["a(b) = 3b"]);
        assert_eq!((p.x_name.as_str(), p.y_name.as_str()), ("b", "a"));
        assert_eq!(func(&p, 0).eval(2.0), 6.0);
        let known: HashSet<String> = p.objs.iter().map(|o| o.name()).collect();
        assert_eq!(row_latex(&p.objs[0], &p.values()[0].clone().unwrap(), &known), "a(b) = 3 b");
        assert_eq!(row_text(&p.objs[0], &p.values()[0].clone().unwrap(), &known, false), "a(b) = 3b");
        // The switch: as sliders (a slider b, a a number), and back.
        let id = p.objs[0].id.clone();
        assert_eq!(p.curve_switch(&id), Some(true));
        let s = p.as_sliders(&id).unwrap();
        assert_eq!(defs(&s), ["a = 3b", "b = 1"]);
        assert_eq!(s.values()[0], Ok(Val::Num(3.0)));
        assert_eq!((s.x_name.as_str(), s.y_name.as_str()), ("", ""));
        assert_eq!(s.curve_switch(&id), Some(false));
        assert_eq!(s.curve_switch(&s.objs[1].id), None, "the slider itself has none");
        let back = s.as_curve(&id).unwrap();
        assert_eq!(defs(&back), ["a(b) = 3b"], "the slider goes with it");
        assert_eq!((back.x_name.as_str(), back.y_name.as_str()), ("b", "a"));
        // An equation of two letters: the one on the right across (else the first one).
        let e = plot().add("2a + 3b = 6").unwrap();
        assert_eq!(defs(&e), ["f: 2x + 3y = 6"]);
        assert_eq!((e.x_name.as_str(), e.y_name.as_str()), ("a", "b"));
        let q = plot().add("u^2 = v").unwrap();
        assert_eq!((q.x_name.as_str(), q.y_name.as_str()), ("v", "u"));
        // Axis names already given stay.
        let mut named = plot();
        named.x_name = "t in s".into();
        assert_eq!(named.add("a = 3b").unwrap().x_name, "t in s");
        // Three letters, or x and y in it: sliders as before.
        assert_eq!(defs(&plot().add("a = 3b + c").unwrap()), ["b = 1", "c = 1", "a = 3b + c"]);
        assert_eq!(plot().add("f(x) = k x").unwrap().objs.len(), 2);
        // A number from a slider used elsewhere keeps the slider.
        let used = plot().add("k = 2").unwrap().add("b = 3k").unwrap().add("g(x) = k x").unwrap();
        let c = used.as_curve(&used.objs[1].id).unwrap();
        assert_eq!(defs(&c)[..2], ["k = 2", "b(k) = 3k"]);
    }

    #[test]
    fn editing_a_row_to_two_letters_makes_the_curve_as_typing_does() {
        let defs = |p: &Plot| p.objs.iter().map(|o| o.def.clone()).collect::<Vec<_>>();
        // A function edited to a = 3b: the curve a(b) = 3b in the same row, axes b and a.
        let p = plot().add("f(x) = x^2").unwrap();
        let id = p.objs[0].id.clone();
        let e = p.set_def(&id, "a = 3b").unwrap();
        assert_eq!(defs(&e), ["a(b) = 3b"]);
        assert_eq!(e.objs[0].id, id, "the same row");
        assert_eq!((e.x_name.as_str(), e.y_name.as_str()), ("b", "a"));
        assert_eq!(func(&e, 0).eval(2.0), 6.0);
        assert_eq!(e.curve_switch(&id), Some(true));
        // The row's own name counts as free: a = 2 edited to a = 3b.
        let n = plot().add("a = 2").unwrap();
        let e = n.set_def(&n.objs[0].id, "a = 3b").unwrap();
        assert_eq!(defs(&e), ["a(b) = 3b"]);
        // Without a name it keeps its own (a = 3b's way: b across, the row's name up).
        let e = p.set_def(&id, "u = v^2").unwrap();
        assert_eq!(defs(&e), ["u(v) = v^2"]);
        // An equation of two letters: an equation of x and y, the axes named after the letters.
        let e = p.set_def(&id, "2a + 3b = 6").unwrap();
        assert_eq!(defs(&e), ["f: 2x + 3y = 6"]);
        assert_eq!((e.x_name.as_str(), e.y_name.as_str()), ("a", "b"));
        // Editing the curve keeps it a curve.
        let c = plot().add("a = 3b").unwrap();
        let e = c.set_def(&c.objs[0].id, "a = 5b").unwrap();
        assert_eq!(defs(&e), ["a(b) = 5b"]);
        // A number others use stays a number: sliders as before.
        let used = plot().add("a = 2").unwrap().add("g(x) = a x").unwrap();
        let e = used.set_def(&used.objs[0].id, "a = 3b").unwrap();
        assert_eq!(defs(&e), ["a = 3b", "g(x) = a x", "b = 1"]);
        assert_eq!(e.values()[0], Ok(Val::Num(3.0)));
        // A slider already there: a number of it, as typing does.
        let s = plot().add("b = 4").unwrap().add("f(x) = x").unwrap();
        let e = s.set_def(&s.objs[1].id, "a = 3b").unwrap();
        assert_eq!(defs(&e), ["b = 4", "a = 3b"]);
        // Three letters, or x in it: sliders as before.
        let e = p.set_def(&id, "a = 3b + c").unwrap();
        assert_eq!(defs(&e), ["a = 3b + c", "b = 1", "c = 1"]);
    }

    #[test]
    fn fractions_stay_fractions() {
        let rows = |src: &[&str]| {
            let mut p = plot();
            for s in src {
                p = p.add(s).unwrap_or_else(|e| panic!("{s}: {e}"));
            }
            let known: HashSet<String> = p.objs.iter().map(|o| o.name()).collect();
            p.objs.iter().zip(p.values()).map(|(o, v)| row_latex(o, &v.unwrap(), &known)).collect::<Vec<_>>()
        };
        let r = rows(&["f(x) = 1/3 x^2", "g(x) = x/3", "A = (1/3, 2/3)", "h: y = x/3 + 2/3", "a = 0.25", "b = 2/7"]);
        assert_eq!(r[0], "f(x) = \\frac{1}{3} x^{2}");
        assert_eq!(r[1], "g(x) = \\frac{x}{3}");
        assert_eq!(r[2], "A = (\\frac{1}{3} \\mid \\frac{2}{3})");
        assert_eq!(r[3], "h{:}\\; y = \\frac{1}{3}x + \\frac{2}{3}");
        assert_eq!(r[4], "a = 0{,}25", "what two places say exactly stays a decimal");
        assert_eq!(r[5], "b = \\frac{2}{7}");
        // As text: small digits; written on the graph: stacked.
        assert_eq!(fmt_num(1.0 / 3.0), "¹⁄₃");
        assert_eq!(fmt_num(-5.0 / 6.0), "−⁵⁄₆");
        assert_eq!(fmt_num(0.5), "0,5");
        assert_eq!(exact_fraction(0.125), Some((1, 8)));
        assert_eq!(exact_fraction(2f64.sqrt()), None);
        assert_eq!(exact_fraction(2.00000004), None, "a whole number worked out numerically");
        assert_eq!(exact_fraction(1.0 / 3.0 + 1e-11), Some((1, 3)), "a third worked out numerically");
        let p = plot().add("g(x) = (x + 1)/3").unwrap();
        let known: HashSet<String> = p.objs.iter().map(|o| o.name()).collect();
        let label = row_text(&p.objs[0], &p.values()[0].clone().unwrap(), &known, true);
        assert!(label.starts_with("g(x) = \u{E000}x + 1\u{E001}3\u{E002}"), "{label:?}");
        assert_eq!(row_text(&p.objs[0], &p.values()[0].clone().unwrap(), &known, false), "g(x) = (x + 1) / 3");
        let laid = layout(&label);
        assert_eq!(laid.bars.len(), 1);
    }

    #[test]
    fn points_typed_by_hand() {
        let pts = parse_points("A(1|2); B(-1|0,5) C = (pi;-3)").unwrap();
        assert_eq!(pts.len(), 3);
        assert_eq!((pts[1].name.as_str(), pts[1].x, pts[1].y), ("B", -1.0, 0.5));
        assert!(parse_points("A(1,2)").is_err());
        assert_eq!(format_num(2.0 / 3.0, 2), "0,67");
    }
}
