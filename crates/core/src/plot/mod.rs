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

pub use expr::{Cmp, Curve, Expr, Func, NAMES_OF_FNS, check, fn_name, latex_to_expr, parse, parse_line, rename_refs};
pub use font::{glyph, text_width};
pub use geo::{Kind, LabelMode, Obj, PointStyle, Slider, Style, Val, ViewRange, evaluate, fmt_expr, fmt_num, format_val, free_point, moved_on, moved_point, name_of, new_object, next_name, renamed};
pub use points::{PointKind, SpecialPoint, area, special_points, zeros_of};
pub use render::{curve, hit, nice_step, pi_label, render, slider_track, tick_label};

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
    pub fn is_valid(&self) -> bool {
        let fin = |v: f64| v.is_finite();
        fin(self.x_min) && fin(self.x_max) && fin(self.y_min) && fin(self.y_max) && self.x_max > self.x_min && self.y_max > self.y_min && self.w > 1.0 && self.h > 1.0
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
    pub fn add(&self, input: &str) -> Result<Plot, String> {
        if let Some(split) = self.split_points(input)? {
            return Ok(split);
        }
        self.add_one(input)
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
    pub fn set_def(&self, id: &str, def: &str) -> Result<Plot, String> {
        let mut p = self.clone();
        let i = p.objs.iter().position(|o| o.id == id).ok_or("Dieses Objekt gibt es nicht mehr")?;
        let old = p.objs[i].name();
        let def = def.trim();
        let known: std::collections::HashSet<String> = self.objs.iter().map(|o| o.name()).collect();
        let def = match name_of(def) {
            Some(_) => def.to_string(),
            // y = 3x without a name: an equation, labelled.
            None if lang::parse_input(def, &known).is_ok_and(|i| matches!(i.body, lang::Body::Equation(..))) => format!("{old}: {def}"),
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
        let v = value.clamp(s.min.min(s.max), s.max.max(s.min));
        let v = if s.step > 0.0 { s.min + ((v - s.min) / s.step).round() * s.step } else { v };
        let mut p = self.clone();
        p.objs[i].def = format!("{} = {}", self.objs[i].name(), geo::fmt_plain((v * 1e9).round() / 1e9));
        Some(p)
    }
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

    #[test]
    fn points_typed_by_hand() {
        let pts = parse_points("A(1|2); B(-1|0,5) C = (pi;-3)").unwrap();
        assert_eq!(pts.len(), 3);
        assert_eq!((pts[1].name.as_str(), pts[1].x, pts[1].y), ("B", -1.0, 0.5));
        assert!(parse_points("A(1,2)").is_err());
        assert_eq!(format_num(2.0 / 3.0, 2), "0,67");
    }
}
