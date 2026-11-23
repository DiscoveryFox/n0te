//! Coordinate systems with function graphs on the laptop. Their look comes from the core as ink
//! ([`n0te_core::plot::render`]: axes, numbers, curves, drawn as if by hand) and is drawn like
//! strokes, worked out once per plot value. A click on one opens the "Koordinatensystem" dialog
//! ([`Editor`]: functions with their line, area and derivative; what is marked; the axes; a
//! live preview); the palette and the top bar insert a new one ([`fresh`]).

use std::cell::RefCell;
use std::collections::HashMap;
use std::f64::consts::FRAC_PI_2;
use std::rc::Rc;

use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::graphics::mesh::Mesh;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Widget};
use iced::advanced::mouse;
use iced::widget::{Row, Space, button, canvas, column, container, row, scrollable, svg, text, text_input, toggler, tooltip};
use iced::{Alignment, Border, Color, Element, Length, Padding, Point, Rectangle, Size, Theme, Vector};

use n0te_core::ink::{Brush, InkPoint, Rect};
use n0te_core::notebook::Background;
use n0te_core::plot::{self, Area, Curve, LineStyle, MARK_CROSSINGS, MARK_EXTREMA, MARK_TURNS, MARK_ZEROS, Plot, PlotFn};

use crate::theme;

/// The pen colours, like the tablet's: black, blue, red, green, orange, purple.
pub const INKS: [u32; 6] = [0xFF11_1111, 0xFF1E_5BD8, 0xFFD7_1921, 0xFF13_8A47, 0xFFE0_8A00, 0xFF7B_3FD0];
/// Axes, ticks and numbers: nearly black.
pub const AXES: u32 = 0xFF20_2428;
/// Pen width of the curves of a new plot.
pub const WIDTH: f32 = 1.2;

/// A plot's look: (brush, points in page points) per stroke.
pub type Look = Rc<Vec<(Brush, Vec<InkPoint>)>>;

thread_local! {
    /// Looks by plot value (a plot's hand is seeded by its id, so a value always looks the same).
    static LOOKS: RefCell<HashMap<String, Look>> = RefCell::new(HashMap::new());
}

/// How `p` looks as ink, worked out once per value.
pub fn look(p: &Plot) -> Look {
    let key = format!("{p:?}");
    LOOKS.with(|m| {
        let mut m = m.borrow_mut();
        if let Some(l) = m.get(&key) {
            return l.clone();
        }
        // Typing in the dialog makes a new value per key: keep the cache small.
        if m.len() >= 256 {
            m.clear();
        }
        let l: Look = Rc::new(plot::render(p));
        m.insert(key, l.clone());
        l
    })
}

/// Plots as solid meshes in page units (None: nothing to draw), clipped to `clip`.
pub fn meshes(plots: &[Plot], clip: Rectangle) -> Option<Vec<Mesh>> {
    let looks: Vec<Look> = plots.iter().map(look).collect();
    let m = crate::pages::ink_meshes_of(looks.iter().flat_map(|l| l.iter().map(|(b, p)| (p.as_slice(), b))), 1, clip);
    (!m.is_empty()).then_some(m)
}

/// Where a look's ink lies (page points), with the plot's box.
fn look_bounds(p: &Plot, l: &[(Brush, Vec<InkPoint>)]) -> Rect {
    l.iter().fold(Rect::new(p.x, p.y, p.x + p.w, p.y + p.h), |r, (b, pts)| r.union(&n0te_core::sheet::bounds_of(pts, b.width)))
}

/// The colour for a new function: the pen colours after black, one after the other.
pub fn next_color(used: &[u32]) -> u32 {
    let inks = &INKS[1..];
    inks.iter().copied().find(|c| !used.contains(c)).unwrap_or(inks[used.len() % inks.len()])
}

/// The next pen colour after `c` (a colour not among them: blue).
pub fn cycle(c: u32) -> u32 {
    INKS.iter().position(|&k| k == c).map(|i| INKS[(i + 1) % INKS.len()]).unwrap_or(INKS[1])
}

/// A new, empty coordinate system around (`cx`, `cy`) on a page `pw` × `ph` (sheet points),
/// like the tablet's: on squared paper 12 × 8 squares with a square as the unit, snapped to the
/// squares (the axes on lines); else 240 × 160 pt with 20 pt to the unit and a grid of its own.
/// Smaller on a small page, always on it.
pub fn fresh(cx: f32, cy: f32, pw: f32, ph: f32, background: Background, spacing: f32) -> Plot {
    let squares = background == Background::Grid && spacing > 2.0;
    let unit = if squares { spacing } else { 20.0 };
    let (mut cols, mut rows) = (12u32, 8u32);
    while cols > 4 && cols as f32 * unit > pw * 0.85 {
        cols -= 2;
    }
    while rows > 4 && rows as f32 * unit > ph * 0.6 {
        rows -= 2;
    }
    let (w, h) = (cols as f32 * unit, rows as f32 * unit);
    let fit = |v: f32, size: f32, page: f32| {
        let v = v.clamp(0.0, (page - size).max(0.0));
        if squares {
            // On a line, and still on the page.
            let last = ((page - size).max(0.0) / unit).floor() * unit;
            ((v / unit).round() * unit).clamp(0.0, last)
        } else {
            v
        }
    };
    let (x, y) = (fit(cx - w / 2.0, w, pw), fit(cy - h / 2.0, h, ph));
    let mut p = Plot::centered(x, y, w, h, unit, AXES, WIDTH);
    p.grid = !squares;
    p
}

/// A typed number: "2,5", "-3", "−1.5", also "2π", "pi/2", "e" (a term without x).
pub fn number(s: &str) -> Option<f64> {
    let t = s.trim().replace('−', "-");
    if t.is_empty() {
        return None;
    }
    if let Ok(v) = t.replace(',', ".").parse::<f64>() {
        return v.is_finite().then_some(v);
    }
    let e = plot::parse(&t).ok()?;
    let (a, b) = (e.eval(0.0), e.eval(1.0));
    (a.is_finite() && a == b).then_some(a)
}

/// A number as it is written here: "-6", "2,5", "0,125".
pub fn fmt_num(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 && v.abs() < 1e15 {
        return format!("{}", v.round() as i64);
    }
    let s = format!("{v:.4}");
    s.trim_end_matches('0').trim_end_matches('.').replace('.', ",")
}

/// A number on an x axis in steps of π: "-2π", "π/2", "3π/2" (others as [`fmt_num`]).
pub fn fmt_pi(v: f64) -> String {
    let h = v / FRAC_PI_2;
    let k = h.round();
    if (h - k).abs() > 1e-9 || k.abs() > 1e6 {
        return fmt_num(v);
    }
    let sign = if k < 0.0 { "-" } else { "" };
    match k.abs() as u64 {
        0 => "0".into(),
        2 => format!("{sign}π"),
        1 => format!("{sign}π/2"),
        a if a % 2 == 0 => format!("{sign}{}π", a / 2),
        a => format!("{sign}{a}π/2"),
    }
}

/// The name of the `i`-th function: f, g, h, k, …
pub fn name(i: usize) -> String {
    plot::fn_name(i)
}

/// Most functions a plot has here: one per name (f, g, h, k, p, q, r, s), so each can be used
/// by the others.
pub const MAX_FNS: usize = 8;

/// The id of the `i`-th function's field (focus).
pub fn expr_id(i: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-plot-fn-{i}"))
}

/// A number field of the dialog: what it says, and the exact value behind it while it still
/// says what the dialog wrote there (a stored -2π doesn't come back as -6,2832).
#[derive(Debug, Clone, Default)]
pub struct NumField {
    pub text: String,
    exact: Option<(String, f64)>,
}

impl NumField {
    /// `v`, written as a number (or in steps of π).
    pub fn of(v: f64, pi: bool) -> NumField {
        let text = if pi { fmt_pi(v) } else { fmt_num(v) };
        NumField { exact: Some((text.clone(), v)), text }
    }

    /// What it says, as a number (None: not one).
    pub fn value(&self) -> Option<f64> {
        match &self.exact {
            Some((t, v)) if *t == self.text => Some(*v),
            _ => number(&self.text),
        }
    }
}

/// The names the core reads in a run of letters before the functions' letters, in its order
/// (the core's lexer: known names one after the other, then single letters of functions).
const KNOWN: &[&str] = &[
    "arcsin", "arccos", "arctan", "betrag", "wurzel", "floor", "asin", "acos", "atan", "sinh", "cosh", "tanh", "sqrt", "ceil", "sign", "sin", "cos",
    "tan", "cot", "exp", "abs", "sgn", "log", "ln", "lg", "pi", "e", "x",
];

/// `expr` (a function of a plot with `fns` functions) with the names of the functions after
/// `removed` one back (`h(x)` → `g(x)`), read the way the core reads them.
pub fn shift_refs(expr: &str, removed: usize, fns: usize) -> String {
    let chars: Vec<char> = expr.chars().collect();
    let mut out = String::with_capacity(expr.len());
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
        let run = &chars[start..i];
        let lower = |k: usize| run[k].to_lowercase().next().unwrap_or(run[k]);
        let mut j = 0;
        while j < run.len() {
            if let Some(n) = KNOWN.iter().find(|n| j + n.len() <= run.len() && n.chars().enumerate().all(|(o, c)| lower(j + o) == c)) {
                out.extend(&run[j..j + n.len()]);
                j += n.len();
                continue;
            }
            match plot::NAMES_OF_FNS.iter().position(|&c| c == lower(j)) {
                Some(k) if k < fns => {
                    out.push(plot::NAMES_OF_FNS[if k > removed { k - 1 } else { k }]);
                    j += 1;
                }
                // Not a name: the core says what is wrong; the rest stays as it is.
                _ => {
                    out.extend(&run[j..]);
                    j = run.len();
                }
            }
        }
    }
    out
}

/// Takes function `i` out; the others keep meaning the same: their references to later ones
/// follow (`h(x)` → `g(x)`), so do areas between functions (an area to the one taken out goes
/// to the x axis).
pub fn remove_fn(fns: &mut Vec<PlotFn>, i: usize) {
    if i >= fns.len() {
        return;
    }
    let n = fns.len();
    fns.remove(i);
    for f in fns.iter_mut() {
        f.expr = shift_refs(&f.expr, i, n);
        if let Some(a) = &mut f.area {
            a.to_fn = match a.to_fn {
                Some(j) if j == i => None,
                Some(j) if j > i => Some(j - 1),
                other => other,
            };
        }
    }
}

/// What the dialog does.
#[derive(Debug, Clone)]
pub enum Msg {
    Expr(usize, String),
    /// The swatch: the next pen colour.
    Color(usize),
    /// How a function's line is drawn.
    Style(usize, LineStyle),
    /// "Fläche" on or off.
    Area(usize, bool),
    /// An area field (0 from, 1 to) typed.
    AreaRange(usize, usize, String),
    /// What an area reaches to: the x axis (None) or another function.
    AreaTo(usize, Option<usize>),
    /// "Ableitung": a new row with the function's derivative.
    Derive(usize),
    RemoveFn(usize),
    AddFn,
    /// Range field (0 x from, 1 x to, 2 y from, 3 y to) typed.
    Range(usize, String),
    FitY,
    EqualUnits,
    /// The x axis in steps of π.
    Pi(bool),
    XName(String),
    YName(String),
    Grid(bool),
    Clean(bool),
    /// The functions' names at their curves.
    Names(bool),
    /// What is marked by itself ([`plot::MARK_ZEROS`] …) on or off.
    Mark(u8, bool),
    /// Coordinates at the marked points.
    Coords(bool),
    /// The points marked by hand, as typed.
    Points(String),
    /// A section (0 points, 1 axes) folded or unfolded.
    Fold(usize),
    /// Enter in a field: like OK.
    Submit,
    Delete,
    Cancel,
    Ok,
}

/// What the app does after a dialog message.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    None,
    /// Put the cursor in function field `i` (a new one).
    Focus(usize),
    Cancel,
    Delete,
    /// Store this plot (one step).
    Apply(Plot),
}

/// What a plot marks by itself, as the dialog names it.
pub const MARKS: [(u8, &str); 4] = [(MARK_ZEROS, "Nullstellen"), (MARK_EXTREMA, "Hoch- und Tiefpunkte"), (MARK_CROSSINGS, "Schnittpunkte"), (MARK_TURNS, "Wendepunkte")];

/// The "Koordinatensystem" dialog for one plot. It edits a copy of the stored plot, so what it
/// doesn't show stays as it was.
pub struct Editor {
    pub sheet_id: String,
    /// The plot as it is being edited: always drawable (numbers take typed values only once
    /// they work), functions as typed.
    pub plot: Plot,
    /// The plot as it was when the dialog opened.
    pub original: Plot,
    /// The range fields: x from, x to, y from, y to.
    pub ranges: [NumField; 4],
    /// Each function's area fields (from, to), kept while its area is off (to switch it back).
    pub areas: Vec<[NumField; 2]>,
    /// What the field of points marked by hand says.
    pub points: String,
    /// What it said when the dialog opened.
    points_shown: String,
    /// Which sections are open: points, axes.
    pub open: [bool; 2],
    /// The page's paper (for the preview).
    pub paper: (Background, f32),
    preview: RefCell<Option<(Plot, canvas::Cache)>>,
}

impl Editor {
    /// The dialog for `plot` on `sheet_id`; a plot without functions gets an empty one to type.
    pub fn new(sheet_id: &str, plot: Plot, paper: (Background, f32)) -> Editor {
        let original = plot.clone();
        let mut plot = plot;
        if plot.fns.is_empty() {
            plot.fns.push(PlotFn::new("", next_color(&[])));
        }
        let ranges = [NumField::of(plot.x_min, plot.pi), NumField::of(plot.x_max, plot.pi), NumField::of(plot.y_min, false), NumField::of(plot.y_max, false)];
        let areas = plot.fns.iter().map(|f| f.area.as_ref().map(|a| [NumField::of(a.from, plot.pi), NumField::of(a.to, plot.pi)]).unwrap_or_default()).collect();
        let points = plot::format_points(&plot.points);
        // The points open when the plot marks something: else they wait folded.
        let open = [plot.marks != 0 || plot.coords || !plot.points.is_empty(), true];
        Editor { sheet_id: sheet_id.to_string(), plot, original, ranges, areas, points_shown: points.clone(), points, open, paper, preview: RefCell::new(None) }
    }

    pub fn update(&mut self, msg: Msg) -> Reply {
        let n = self.plot.fns.len();
        match msg {
            Msg::Expr(i, s) => {
                if let Some(f) = self.plot.fns.get_mut(i) {
                    f.expr = s;
                }
            }
            Msg::Color(i) => {
                if let Some(f) = self.plot.fns.get_mut(i) {
                    f.color = cycle(f.color);
                }
            }
            Msg::Style(i, style) => {
                if let Some(f) = self.plot.fns.get_mut(i) {
                    f.style = style;
                }
            }
            Msg::Area(i, on) if i < n => {
                if on {
                    self.area_on(i);
                } else {
                    self.plot.fns[i].area = None;
                }
            }
            Msg::AreaRange(i, k, s) if i < n && k < 2 => {
                self.areas[i][k].text = s;
                if let (Some(a), Some(b)) = (self.areas[i][0].value(), self.areas[i][1].value())
                    && b > a
                    && let Some(area) = &mut self.plot.fns[i].area
                {
                    (area.from, area.to) = (a, b);
                }
            }
            Msg::AreaTo(i, to) => {
                if let Some(area) = self.plot.fns.get_mut(i).and_then(|f| f.area.as_mut()) {
                    area.to_fn = to.filter(|&j| j != i && j < n);
                }
            }
            Msg::Derive(i) => {
                if self.can_derive(i, &self.plot.curves()) {
                    let f = PlotFn { style: LineStyle::Dashed, ..PlotFn::new(&format!("{}'(x)", name(i)), self.plot.fns[i].color) };
                    self.plot.fns.push(f);
                    self.areas.push(Default::default());
                }
            }
            Msg::RemoveFn(i) if i < n => {
                remove_fn(&mut self.plot.fns, i);
                self.areas.remove(i);
                if self.plot.fns.is_empty() {
                    self.plot.fns.push(PlotFn::new("", next_color(&[])));
                    self.areas.push(Default::default());
                }
            }
            Msg::AddFn if n < MAX_FNS => {
                let used: Vec<u32> = self.plot.fns.iter().map(|f| f.color).collect();
                self.plot.fns.push(PlotFn::new("", next_color(&used)));
                self.areas.push(Default::default());
                return Reply::Focus(n);
            }
            Msg::Range(i, s) if i < 4 => {
                self.ranges[i].text = s;
                self.take_range(i / 2);
            }
            Msg::FitY => {
                if let Some((lo, hi)) = self.plot.fit_y() {
                    self.set_y(lo, hi);
                }
            }
            Msg::EqualUnits => {
                let (lo, hi) = self.plot.equal_units();
                self.set_y(lo, hi);
            }
            Msg::Pi(on) => {
                self.plot.pi = on;
                // Whole π/2 at both ends, written in π.
                let (lo, hi) = self.plot.pi_range();
                if on && lo.is_finite() && hi > lo {
                    (self.plot.x_min, self.plot.x_max) = (lo, hi);
                    self.ranges[0] = NumField::of(lo, true);
                    self.ranges[1] = NumField::of(hi, true);
                }
            }
            Msg::XName(s) => self.plot.x_name = s,
            Msg::YName(s) => self.plot.y_name = s,
            Msg::Grid(on) => self.plot.grid = on,
            Msg::Clean(on) => self.plot.clean = on,
            Msg::Names(on) => self.plot.names = on,
            Msg::Mark(bit, on) => {
                if on {
                    self.plot.marks |= bit;
                } else {
                    self.plot.marks &= !bit;
                }
            }
            Msg::Coords(on) => self.plot.coords = on,
            Msg::Points(s) => {
                if s == self.points_shown {
                    self.plot.points = self.original.points.clone();
                } else if let Ok(v) = plot::parse_points(&s) {
                    self.plot.points = v;
                }
                self.points = s;
            }
            Msg::Fold(k) => {
                if let Some(o) = self.open.get_mut(k) {
                    *o = !*o;
                }
            }
            Msg::Submit | Msg::Ok => {
                if let Ok(p) = self.result() {
                    return Reply::Apply(p);
                }
            }
            Msg::Delete => return Reply::Delete,
            Msg::Cancel => return Reply::Cancel,
            Msg::Area(..) | Msg::AreaRange(..) | Msg::RemoveFn(_) | Msg::AddFn | Msg::Range(..) => {}
        }
        Reply::None
    }

    fn set_y(&mut self, lo: f64, hi: f64) {
        if lo.is_finite() && hi.is_finite() && hi > lo {
            self.plot.y_min = lo;
            self.plot.y_max = hi;
            self.ranges[2] = NumField::of(lo, false);
            self.ranges[3] = NumField::of(hi, false);
        }
    }

    /// An axis' range (0: x, 1: y) as typed goes into the plot once it works (the preview
    /// follows); the other axis stays as it is.
    fn take_range(&mut self, axis: usize) {
        if let (Some(a), Some(b)) = (self.ranges[axis * 2].value(), self.ranges[axis * 2 + 1].value())
            && b > a
        {
            if axis == 0 {
                (self.plot.x_min, self.plot.x_max) = (a, b);
            } else {
                (self.plot.y_min, self.plot.y_max) = (a, b);
            }
        }
    }

    /// Switches function `i`'s area on: where its fields say, else between its first two zeros
    /// in view, else over the middle half of the x range; down to the x axis.
    fn area_on(&mut self, i: usize) {
        let (from, to) = match (self.areas[i][0].value(), self.areas[i][1].value()) {
            (Some(a), Some(b)) if b > a => (a, b),
            _ => {
                let (lo, hi) = (self.plot.x_min, self.plot.x_max);
                let snap = |v: f64| if (v - v.round()).abs() < 1e-9 { v.round() } else { v };
                let zeros = match self.plot.curves().get(i) {
                    Some(Ok(Curve::Of(e))) => plot::zeros_of(&|x| e.eval(x), lo, hi),
                    _ => Vec::new(),
                };
                let (a, b) = if zeros.len() >= 2 && zeros[1] > zeros[0] {
                    (snap(zeros[0]), snap(zeros[1]))
                } else {
                    let q = (hi - lo) / 4.0;
                    if q >= 1.0 { ((lo + q).round(), (hi - q).round()) } else { (lo + q, hi - q) }
                };
                self.areas[i] = [NumField::of(a, self.plot.pi), NumField::of(b, self.plot.pi)];
                (a, b)
            }
        };
        self.plot.fns[i].area = Some(Area { from, to, to_fn: None });
    }

    /// Whether function `i` can get a row with its derivative: it is one, and there is a name
    /// left.
    fn can_derive(&self, i: usize, curves: &[Result<Curve, String>]) -> bool {
        self.plot.fns.len() < MAX_FNS && matches!(curves.get(i), Some(Ok(Curve::Of(_))))
    }

    /// What is wrong with an axis' range (0: x, 1: y), if anything.
    pub fn range_error(&self, axis: usize) -> Option<String> {
        let (a, b) = (self.ranges[axis * 2].value(), self.ranges[axis * 2 + 1].value());
        let n = if axis == 0 { "x" } else { "y" };
        match (a, b) {
            (None, _) => Some(format!("{n} von: keine Zahl")),
            (_, None) => Some(format!("{n} bis: keine Zahl")),
            (Some(a), Some(b)) if b <= a => Some(format!("{n} bis muss größer sein als {n} von")),
            _ => None,
        }
    }

    /// What is wrong with function `i`, the others it uses taken into account (empty ones
    /// are left out, not wrong).
    pub fn fn_error(&self, i: usize) -> Option<String> {
        self.plot.errors().get(i).cloned().flatten()
    }

    /// What is wrong with function `i`'s area, if it has one.
    pub fn area_error(&self, i: usize) -> Option<String> {
        self.area_problem(i, &self.plot.curves())
    }

    fn area_problem(&self, i: usize, curves: &[Result<Curve, String>]) -> Option<String> {
        let f = self.plot.fns.get(i)?;
        let area = f.area.as_ref()?;
        if f.expr.trim().is_empty() {
            return None;
        }
        if matches!(curves.get(i), Some(Ok(Curve::AtX(_)))) {
            return Some("Eine senkrechte Linie hat keine Fläche".into());
        }
        match (self.areas[i][0].value(), self.areas[i][1].value()) {
            (None, _) => return Some("Fläche von: keine Zahl".into()),
            (_, None) => return Some("Fläche bis: keine Zahl".into()),
            (Some(a), Some(b)) if b <= a => return Some("Fläche: bis muss größer sein als von".into()),
            _ => {}
        }
        let j = area.to_fn?;
        let g = name(j);
        match (self.plot.fns.get(j), curves.get(j)) {
            (Some(h), _) if h.expr.trim().is_empty() => Some(format!("Fläche bis {g}: {g} ist leer")),
            (Some(_), Some(Ok(Curve::AtX(_)))) => Some(format!("Fläche bis {g}: {g} ist eine senkrechte Linie")),
            (Some(_), Some(Err(_))) => Some(format!("Fläche bis {g}: {g} hat noch einen Fehler")),
            (Some(_), Some(Ok(_))) => None,
            _ => Some(format!("Fläche: es gibt kein {g}")),
        }
    }

    /// What is wrong with the points typed by hand (unchanged ones are fine).
    pub fn points_error(&self) -> Option<String> {
        if self.points == self.points_shown {
            return None;
        }
        plot::parse_points(&self.points).err()
    }

    /// The plot to store, or why not (OK stays off).
    pub fn result(&self) -> Result<Plot, String> {
        for axis in 0..2 {
            if let Some(e) = self.range_error(axis) {
                return Err(e);
            }
        }
        let curves = self.plot.curves();
        for (i, e) in self.plot.errors().into_iter().enumerate() {
            if let Some(e) = e.or_else(|| self.area_problem(i, &curves)) {
                return Err(format!("{}(x): {e}", name(i)));
            }
        }
        if let Some(e) = self.points_error() {
            return Err(e);
        }
        let mut p = self.plot.clone();
        let v = |i: usize| self.ranges[i].value().unwrap_or(0.0);
        (p.x_min, p.x_max, p.y_min, p.y_max) = (v(0), v(1), v(2), v(3));
        for (f, t) in p.fns.iter_mut().zip(&self.areas) {
            if let (Some(area), Some(a), Some(b)) = (f.area.as_mut(), t[0].value(), t[1].value()) {
                (area.from, area.to) = (a, b);
            }
        }
        // Empty rows are left out; the functions after them keep what they refer to.
        for i in (0..p.fns.len()).rev() {
            if p.fns[i].expr.trim().is_empty() {
                remove_fn(&mut p.fns, i);
            }
        }
        for f in &mut p.fns {
            f.expr = f.expr.trim().to_string();
        }
        p.x_name = p.x_name.trim().to_string();
        p.y_name = p.y_name.trim().to_string();
        if !p.is_valid() {
            return Err("Dieser Bereich lässt sich nicht zeichnen".into());
        }
        Ok(p)
    }

    /// What the points section marks, said in a line (when it is folded).
    fn points_summary(&self) -> String {
        let mut parts: Vec<String> = MARKS.iter().filter(|(bit, _)| self.plot.marks & bit != 0).map(|(_, s)| s.to_string()).collect();
        match self.plot.points.len() {
            0 => {}
            1 => parts.push("1 eigener Punkt".into()),
            k => parts.push(format!("{k} eigene Punkte")),
        }
        if parts.is_empty() { "nichts markiert".into() } else { parts.join(", ") }
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let curves = self.plot.curves();
        let errors = self.plot.errors();
        let muted = |s: String| text(s).size(12).color(theme::p().muted);
        let error = |s: String| text(s).size(12).color(theme::p().accent);
        let label_w = 54.0;
        // Under the field: past the swatch and the name.
        let under = |e: Element<'static, Msg>| -> Element<'static, Msg> { container(e).padding(Padding { left: label_w + 42.0, ..Padding::ZERO }).into() };
        let n = self.plot.fns.len();

        let mut fns = column![theme::caption("Funktionen")].spacing(10);
        for (i, f) in self.plot.fns.iter().enumerate() {
            let c = crate::pages::argb(f.color);
            let swatch = button(container(Space::new().width(16).height(16)).style(move |_| container::Style {
                background: Some(c.into()),
                border: Border { radius: 8.0.into(), width: 1.0, color: theme::alpha(Color::BLACK, 0.18) },
                ..Default::default()
            }))
            .padding(5)
            .on_press(Msg::Color(i))
            .style(theme::ghost);
            let field = text_input(if i == 0 { "z. B. x^2 - 1" } else { "" }, &f.expr)
                .id(expr_id(i))
                .on_input(move |s| Msg::Expr(i, s))
                .on_submit(Msg::Submit)
                .font(theme::MONO)
                .padding(8)
                .size(14)
                .style(theme::field);
            let mut cell = column![
                row![
                    swatch,
                    text(format!("{}(x) =", name(i))).font(theme::MONO).size(13).color(theme::p().muted).width(Length::Fixed(label_w)),
                    field,
                    button(text("×").size(15)).padding([3, 9]).on_press(Msg::RemoveFn(i)).style(theme::ghost),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
            ]
            .spacing(5);
            if let Some(e) = &errors[i] {
                cell = cell.push(under(error(e.clone()).into()));
            }
            // The line, the area, the derivative.
            let derive = self.can_derive(i, &curves);
            let controls = row![
                line_style(i, LineStyle::Solid, f.style, c),
                line_style(i, LineStyle::Dashed, f.style, c),
                line_style(i, LineStyle::Dotted, f.style, c),
                Space::new().width(14),
                toggler(f.area.is_some()).label("Fläche").on_toggle(move |on| Msg::Area(i, on)).size(14).text_size(12.5).spacing(6).style(theme::switch),
                Space::new().width(10),
                button(theme::btn_small("Ableitung").color_maybe((!derive).then(|| theme::alpha(theme::p().muted, 0.45))))
                    .padding([3, 9])
                    .on_press_maybe(derive.then_some(Msg::Derive(i)))
                    .style(theme::ghost),
            ]
            .spacing(3)
            .align_y(Alignment::Center);
            cell = cell.push(under(controls.into()));
            if let Some(area) = &f.area {
                let num = |k: usize| {
                    text_input("", &self.areas[i][k].text).on_input(move |s| Msg::AreaRange(i, k, s)).on_submit(Msg::Submit).font(theme::MONO).padding([4, 7]).size(13).width(Length::Fixed(62.0)).style(theme::field)
                };
                let mut line: Vec<Element<'_, Msg>> = vec![theme::caption("von").into(), num(0).into(), theme::caption("bis").into(), num(1).into(), Space::new().width(4).into()];
                line.push(choice("bis zur x-Achse".into(), area.to_fn.is_none(), Msg::AreaTo(i, None)));
                for j in (0..n).filter(|&j| j != i) {
                    line.push(choice(format!("bis {}", name(j)), area.to_fn == Some(j), Msg::AreaTo(i, Some(j))));
                }
                let line = Row::with_children(line).spacing(5).align_y(Alignment::Center).wrap().vertical_spacing(6);
                cell = cell.push(container(line).padding(Padding { left: label_w + 42.0, ..Padding::ZERO }));
                if let Some(e) = self.area_problem(i, &curves) {
                    cell = cell.push(under(error(e).into()));
                }
            }
            fns = fns.push(cell);
        }
        fns = fns.push(
            row![
                button(theme::btn_small("Funktion hinzufügen")).padding(theme::pad()).on_press_maybe((n < MAX_FNS).then_some(Msg::AddFn)).style(theme::secondary),
                Space::new().width(12),
                muted("x^2, sin x, √x, |x|, e^-x, f'(x), x = 2".into()),
            ]
            .align_y(Alignment::Center),
        );

        let switch = |on: bool, label: &'static str, f: Box<dyn Fn(bool) -> Msg>| toggler(on).label(label).on_toggle(f).size(16).text_size(13).spacing(8).style(theme::switch);
        let fold = |k: usize, title: &str, summary: String| -> Element<'static, Msg> {
            let open = self.open[k];
            let mut head = row![text(if open { "▾" } else { "▸" }).font(theme::SANS_BOLD).size(12).color(theme::p().accent).width(16), theme::caption(title)].align_y(Alignment::Center);
            if !open {
                head = head.push(Space::new().width(10)).push(text(summary).size(12).color(theme::p().muted));
            }
            button(head).padding([3, 4]).on_press(Msg::Fold(k)).style(theme::row(false)).into()
        };

        // What is marked: by itself, and by hand.
        let mut points = column![fold(0, "Punkte", self.points_summary())].spacing(8);
        if self.open[0] {
            let mark = |k: usize| {
                let (bit, label) = MARKS[k];
                container(switch(self.plot.marks & bit != 0, label, Box::new(move |on| Msg::Mark(bit, on)))).width(Length::Fill)
            };
            points = points
                .push(row![mark(0), mark(1)].spacing(12))
                .push(row![mark(2), mark(3)].spacing(12))
                .push(switch(self.plot.coords, "Koordinaten zeigen", Box::new(Msg::Coords)))
                .push(
                    row![
                        theme::caption("Eigene Punkte"),
                        text_input("A(1|2); B(-1|0,5)", &self.points).on_input(Msg::Points).on_submit(Msg::Submit).font(theme::MONO).padding(7).size(13).style(theme::field),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                );
            if let Some(e) = self.points_error() {
                points = points.push(error(e));
            }
        }

        // The axes: ranges, steps of π, names, the look.
        let r = &self.ranges;
        let mut axes = column![fold(1, "Achsen", format!("x {} bis {}, y {} bis {}", r[0].text, r[1].text, r[2].text, r[3].text))].spacing(8);
        if self.open[1] {
            let num = |i: usize, hint: &'static str| text_input(hint, &self.ranges[i].text).on_input(move |s| Msg::Range(i, s)).on_submit(Msg::Submit).font(theme::MONO).padding(7).size(14).width(Length::Fixed(84.0)).style(theme::field);
            let axis_row = |axis: usize| {
                row![
                    text(if axis == 0 { "x" } else { "y" }).font(theme::MONO_BOLD).size(14).color(theme::p().text).width(Length::Fixed(18.0)),
                    theme::caption("von"),
                    num(axis * 2, "von"),
                    theme::caption("bis"),
                    num(axis * 2 + 1, "bis"),
                ]
                .spacing(8)
                .align_y(Alignment::Center)
            };
            let axis_name = |value: &str, hint: &'static str, f: fn(String) -> Msg| text_input(hint, value).on_input(f).on_submit(Msg::Submit).font(theme::MONO).padding(7).size(14).width(Length::Fixed(84.0)).style(theme::field);
            axes = axes
                .push(row![axis_row(0), Space::new().width(10), switch(self.plot.pi, "x in π", Box::new(Msg::Pi))].spacing(8).align_y(Alignment::Center))
                .push(
                    row![
                        axis_row(1),
                        Space::new().width(10),
                        button(theme::btn_small("y passend")).padding(theme::pad()).on_press_maybe(self.plot.fit_y().is_some().then_some(Msg::FitY)).style(theme::secondary),
                        button(theme::btn_small("Gleiche Einheiten")).padding(theme::pad()).on_press(Msg::EqualUnits).style(theme::secondary),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                );
            for axis in 0..2 {
                if let Some(e) = self.range_error(axis) {
                    axes = axes.push(error(e));
                }
            }
            axes = axes
                .push(
                    row![
                        container(theme::caption("Namen")).width(Length::Fixed(60.0)),
                        text("x").font(theme::MONO_BOLD).size(14).color(theme::p().text),
                        axis_name(&self.plot.x_name, "x", Msg::XName),
                        Space::new().width(6),
                        text("y").font(theme::MONO_BOLD).size(14).color(theme::p().text),
                        axis_name(&self.plot.y_name, "y", Msg::YName),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center),
                )
                .push(
                    Row::with_children([
                        switch(self.plot.grid, "Raster", Box::new(Msg::Grid)).into(),
                        switch(self.plot.clean, "Wie mit dem Lineal", Box::new(Msg::Clean)).into(),
                        switch(self.plot.names, "Namen an den Kurven", Box::new(Msg::Names)).into(),
                    ])
                    .spacing(22)
                    .wrap()
                    .vertical_spacing(8),
                );
        }

        let left = column![fns, points, axes].spacing(18).padding(Padding { right: 14.0, ..Padding::ZERO });
        let preview = column![theme::caption("Vorschau"), Preview { plot: &self.plot, paper: self.paper, cache: &self.preview, size: Size::new(300.0, 240.0) }].spacing(8);
        // The settings scroll when they don't fit; the preview and the buttons stay.
        let body = row![scrollable(left).style(theme::scroll_dark).width(Length::Fill).height(Length::Shrink), Space::new().width(20), preview];
        let result = self.result();
        // Why OK is off, when it is said in a folded section.
        let hidden = (!self.open[0]).then(|| self.points_error()).flatten().or_else(|| if self.open[1] { None } else { (0..2).find_map(|a| self.range_error(a)) });
        let col = column![
            theme::heading("Koordinatensystem", 22.0),
            crate::widgets::leave_room(body, 80.0),
            container(Space::new()).height(1).width(Length::Fill).style(theme::hairline),
            row![
                button(theme::btn("Löschen")).padding(theme::pad()).on_press(Msg::Delete).style(theme::ghost),
                Space::new().width(Length::Fill),
                muted(hidden.unwrap_or_default()),
                Space::new().width(12),
                button(theme::btn("Abbrechen")).padding(theme::pad()).on_press(Msg::Cancel).style(theme::ghost),
                Space::new().width(8),
                button(theme::btn("OK")).padding(theme::pad()).on_press_maybe(result.is_ok().then_some(Msg::Ok)).style(theme::accent_button),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(16);
        let card = container(col).padding(24).width(Length::Fill).max_width(900).style(theme::panel);
        // Full size, so clicks next to the dialog don't reach the pages under it.
        iced::widget::mouse_area(container(card).padding(Padding { top: 70.0, right: 16.0, bottom: 16.0, left: 16.0 }).center_x(Length::Fill).height(Length::Fill)).interaction(mouse::Interaction::Idle).into()
    }
}

/// A small choice (what an area reaches to): the chosen one carries the accent.
fn choice<'a>(label: String, on: bool, msg: Msg) -> Element<'a, Msg> {
    button(theme::btn_small(&label)).padding([3, 8]).on_press(msg).style(theme::option(on)).into()
}

/// One of a function's line styles to pick: a piece of the line (the chosen one in the
/// function's ink), its name over it.
fn line_style<'a>(i: usize, style: LineStyle, current: LineStyle, ink: Color) -> Element<'a, Msg> {
    let (glyph, tip) = match style {
        LineStyle::Solid => (SOLID, "durchgezogen"),
        LineStyle::Dashed => (DASHED, "gestrichelt"),
        LineStyle::Dotted => (DOTTED, "gepunktet"),
    };
    let color = if style == current { ink } else { theme::p().muted };
    let line = svg(svg::Handle::from_memory(glyph)).width(24).height(12).style(move |_, _| svg::Style { color: Some(color) });
    let b = button(line).padding([4, 6]).on_press(Msg::Style(i, style)).style(theme::option(current == style));
    tooltip(b, container(text(tip).size(12)).padding([3, 8]).style(theme::chip), tooltip::Position::Top).gap(4).into()
}

// A piece of each line style on a 24 × 12 grid.
const SOLID: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 12"><path d="M2 6H22" stroke="#000" stroke-width="2.2" stroke-linecap="round"/></svg>"##;
const DASHED: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 12"><path d="M2 6H22" stroke="#000" stroke-width="2.2" stroke-linecap="round" stroke-dasharray="4.5 4"/></svg>"##;
const DOTTED: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 12"><path d="M2.5 6H22" stroke="#000" stroke-width="2.4" stroke-linecap="round" stroke-dasharray="0.1 4.4"/></svg>"##;

/// The live preview: the plot on a piece of its paper, scaled to fit. Drawn as paths (both
/// renderers draw those; the software one skips meshes), redrawn when the plot changes.
struct Preview<'a> {
    plot: &'a Plot,
    paper: (Background, f32),
    cache: &'a RefCell<Option<(Plot, canvas::Cache)>>,
    size: Size,
}

impl<'a> Preview<'a> {
    fn paint(&self, frame: &mut canvas::Frame, size: Size) {
        let l = look(self.plot);
        let r = look_bounds(self.plot, &l);
        if r.width() <= 0.0 || r.height() <= 0.0 {
            return;
        }
        let pad = 15.0;
        let k = ((size.width - 2.0 * pad) / r.width()).min((size.height - 2.0 * pad) / r.height()).max(0.01);
        // The ink's middle in the preview's middle: page point (x, y) at o + (x, y) · k.
        let o = Vector::new(size.width / 2.0 - (r.x0 + r.x1) / 2.0 * k, size.height / 2.0 - (r.y0 + r.y1) / 2.0 * k);
        // A piece of the page's paper under it, lined up with the plot as on the page.
        let (bg, step) = (self.paper.0, self.paper.1.max(4.0));
        let (x0, x1, y0, y1) = (-o.x / k, (size.width - o.x) / k, -o.y / k, (size.height - o.y) / k);
        let first = |v: f32| (v / step).ceil() * step;
        if step * k >= 3.0 {
            let mut y = first(y0);
            while y <= y1 {
                if matches!(bg, Background::Grid | Background::Lines) {
                    frame.fill_rectangle(Point::new(0.0, o.y + y * k - 0.5), Size::new(size.width, 1.0), theme::PATTERN);
                }
                if bg == Background::Dots {
                    let mut x = first(x0);
                    while x <= x1 {
                        frame.fill(&canvas::Path::circle(Point::new(o.x + x * k, o.y + y * k), 1.1), theme::PATTERN_DOT);
                        x += step;
                    }
                }
                y += step;
            }
            if bg == Background::Grid {
                let mut x = first(x0);
                while x <= x1 {
                    frame.fill_rectangle(Point::new(o.x + x * k - 0.5, 0.0), Size::new(1.0, size.height), theme::PATTERN);
                    x += step;
                }
            }
        }
        // The ink: each stroke's outline (the core's, as for every stroke), filled.
        for (brush, pts) in l.iter() {
            let poly = n0te_core::ink::outline(pts, brush);
            if poly.len() < 6 {
                continue;
            }
            let path = canvas::Path::new(|p| {
                p.move_to(Point::new(o.x + poly[0] * k, o.y + poly[1] * k));
                for c in poly.chunks_exact(2).skip(1) {
                    p.line_to(Point::new(o.x + c[0] * k, o.y + c[1] * k));
                }
                p.close();
            });
            frame.fill(&path, crate::pages::argb(brush.color));
        }
    }
}

impl<'a, Message> Widget<Message, Theme, iced::Renderer> for Preview<'a> {
    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.size.width), Length::Fixed(self.size.height))
    }

    fn layout(&mut self, _tree: &mut widget::Tree, _renderer: &iced::Renderer, _limits: &layout::Limits) -> layout::Node {
        layout::Node::new(self.size)
    }

    fn draw(&self, _tree: &widget::Tree, renderer: &mut iced::Renderer, _theme: &Theme, _style: &renderer::Style, layout: Layout<'_>, _cursor: mouse::Cursor, _viewport: &Rectangle) {
        let b = layout.bounds();
        renderer.fill_quad(
            renderer::Quad { bounds: b, border: Border { color: theme::p().border, width: 1.0, radius: theme::s().card.min(6.0).into() }, ..Default::default() },
            theme::p().paper,
        );
        if !matches!(self.cache.borrow().as_ref(), Some((p, _)) if p == self.plot) {
            *self.cache.borrow_mut() = Some((self.plot.clone(), canvas::Cache::new()));
        }
        let cache = self.cache.borrow();
        let Some((_, cache)) = cache.as_ref() else { return };
        let inner = b.shrink(1.0);
        let geometry = cache.draw(renderer, inner.size(), |frame| self.paint(frame, inner.size()));
        renderer.with_layer(inner, |r| r.with_translation(Vector::new(inner.x, inner.y), |r| r.draw_geometry(geometry)));
    }
}

impl<'a, Message: 'a> From<Preview<'a>> for Element<'a, Message> {
    fn from(p: Preview<'a>) -> Self {
        Element::new(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_take_a_comma_and_pi() {
        assert_eq!(number("2,5"), Some(2.5));
        assert_eq!(number(" -3 "), Some(-3.0));
        assert_eq!(number("−1.5"), Some(-1.5));
        assert!((number("2π").unwrap() - std::f64::consts::TAU).abs() < 1e-12);
        assert!((number("-pi/2").unwrap() + std::f64::consts::FRAC_PI_2).abs() < 1e-12);
        assert_eq!(number("x"), None);
        assert_eq!(number(""), None);
        assert_eq!(number("abc"), None);
        assert_eq!(fmt_num(-6.0), "-6");
        assert_eq!(fmt_num(2.5), "2,5");
        assert_eq!(fmt_num(0.125), "0,125");
        assert_eq!(fmt_num(-0.0), "0");
        let pi = std::f64::consts::PI;
        for (v, s) in [(0.0, "0"), (pi, "π"), (-pi, "-π"), (2.0 * pi, "2π"), (-2.5 * pi, "-5π/2"), (pi / 2.0, "π/2"), (-pi / 2.0, "-π/2"), (1.5 * pi, "3π/2"), (2.0, "2")] {
            assert_eq!(fmt_pi(v), s);
            assert!((number(s).unwrap() - v).abs() < 1e-12, "{s} reads back");
        }
    }

    #[test]
    fn names_after_a_removed_function_move_back() {
        assert_eq!(shift_refs("sqrt(x) + abs(h(x))", 0, 3), "sqrt(x) + abs(g(x))");
        assert_eq!(shift_refs("2h''(3x) - G(x)", 0, 3), "2g''(3x) - f(x)");
        assert_eq!(shift_refs("h(x) = g(x) + sin x", 1, 3), "g(x) = g(x) + sin x", "the removed one's name stays (now an error)");
        assert_eq!(shift_refs("pi s(x) + sgn(x) + sinh x + exp(x)", 2, 8), "pi r(x) + sgn(x) + sinh x + exp(x)");
        assert_eq!(shift_refs("hx + xh", 0, 3), "gx + xg", "letters run together are read like the core does");
        assert_eq!(shift_refs("k(x) + zz(x)", 0, 3), "k(x) + zz(x)", "not names (yet): left alone");
        // The functions mean the same after one is taken out (areas too).
        let mut p = Plot::centered(0.0, 0.0, 240.0, 160.0, 20.0, AXES, WIDTH);
        p.fns = vec![PlotFn::new("x^2", INKS[1]), PlotFn::new("x + 1", INKS[2]), PlotFn::new("g(x) - f'(x)", INKS[3]), PlotFn::new("h(x) * g(x)", INKS[4])];
        p.fns[3].area = Some(Area { from: 0.0, to: 1.0, to_fn: Some(2) });
        p.fns[2].area = Some(Area { from: 0.0, to: 1.0, to_fn: Some(0) });
        let before: Vec<f64> = match &p.curves()[3] {
            Ok(Curve::Of(e)) => (0..5).map(|i| e.eval(i as f64)).collect(),
            c => panic!("{c:?}"),
        };
        let mut q = p.clone();
        q.fns.insert(1, PlotFn::new("", INKS[5]));
        for f in &mut q.fns[2..] {
            f.expr = f.expr.replace("h(", "k(").replace("g(", "h(");
        }
        q.fns[4].area.as_mut().unwrap().to_fn = Some(3);
        remove_fn(&mut q.fns, 1);
        assert_eq!(q.fns, p.fns);
        let after: Vec<f64> = match &q.curves()[3] {
            Ok(Curve::Of(e)) => (0..5).map(|i| e.eval(i as f64)).collect(),
            c => panic!("{c:?}"),
        };
        assert_eq!(before, after);
        // An area to the one taken out goes to the x axis.
        remove_fn(&mut q.fns, 0);
        assert_eq!(q.fns.iter().map(|f| f.area.as_ref().map(|a| a.to_fn)).collect::<Vec<_>>(), vec![None, Some(None), Some(Some(1))]);
    }

    /// A plot using everything the dialog edits and more: marks, coordinates, an area between
    /// two functions, a dashed line, own points, names, axes in π and named, an odd y range.
    fn full_plot() -> Plot {
        let mut p = fresh(300.0, 400.0, 595.0, 842.0, Background::Blank, 22.0);
        p.id = "p".into();
        let pi = std::f64::consts::PI;
        (p.x_min, p.x_max, p.y_min, p.y_max) = (-2.0 * pi, 2.5 * pi, -1.0 / 3.0, 2.0 / 3.0 + 1.0);
        p.fns = vec![PlotFn::new("sin x", INKS[1]), PlotFn { style: LineStyle::Dashed, area: Some(Area { from: -pi / 3.0, to: 1.234567, to_fn: Some(0) }), ..PlotFn::new("f'(x) + 0,5", INKS[2]) }];
        p.marks = plot::MARK_EXTREMA | plot::MARK_TURNS;
        p.coords = true;
        p.names = true;
        p.pi = true;
        p.grid = true;
        p.x_name = "t".into();
        p.y_name = "U".into();
        p.points = vec![plot::PlotPoint { name: "A".into(), x: 1.0 / 3.0, y: 0.5 }, plot::PlotPoint { name: "B".into(), x: -1.0, y: 0.0 }];
        p
    }

    #[test]
    fn a_plot_comes_back_unchanged_when_only_ok_is_pressed() {
        let p = full_plot();
        let mut e = Editor::new("s", p.clone(), (Background::Blank, 22.0));
        assert_eq!(e.ranges[0].text, "-2π");
        assert_eq!(e.ranges[1].text, "5π/2");
        assert_eq!(e.points, "A(0,3333|0,5); B(-1|0)");
        assert!(e.open[0], "the points section is open: the plot marks something");
        assert_eq!(e.update(Msg::Ok), Reply::Apply(p.clone()), "exactly as stored");
        // Also after looking around: sections folded, a field typed and typed back.
        let mut e = Editor::new("s", p.clone(), (Background::Blank, 22.0));
        e.update(Msg::Fold(0));
        e.update(Msg::Fold(1));
        e.update(Msg::Range(0, "-6".into()));
        e.update(Msg::Range(0, "-2π".into()));
        e.update(Msg::Points("A(1|2)".into()));
        e.update(Msg::Points("A(0,3333|0,5); B(-1|0)".into()));
        e.update(Msg::AreaRange(1, 1, "2".into()));
        e.update(Msg::AreaRange(1, 1, "1,2346".into()));
        assert_eq!(e.update(Msg::Ok), Reply::Apply(p));
        // A plot as the tablet makes it, untouched: the same.
        let mut q = fresh(300.0, 400.0, 595.0, 842.0, Background::Grid, 17.0);
        q.fns = vec![PlotFn::new("x^2", INKS[2])];
        assert_eq!(Editor::new("s", q.clone(), (Background::Grid, 17.0)).update(Msg::Ok), Reply::Apply(q));
    }

    #[test]
    fn marks_areas_derivatives_and_the_axes() {
        let mut p = fresh(300.0, 400.0, 595.0, 842.0, Background::Blank, 22.0);
        p.fns = vec![PlotFn::new("x^2 - 1", INKS[1]), PlotFn::new("2", INKS[2])];
        let mut e = Editor::new("s", p.clone(), (Background::Blank, 22.0));
        assert!(!e.open[0], "nothing marked: the points wait folded");
        e.update(Msg::Mark(plot::MARK_ZEROS, true));
        e.update(Msg::Mark(plot::MARK_CROSSINGS, true));
        e.update(Msg::Mark(plot::MARK_CROSSINGS, false));
        e.update(Msg::Coords(true));
        // An area: between the function's zeros, down to the x axis; then up to g, typed wider.
        e.update(Msg::Area(0, true));
        assert_eq!(e.plot.fns[0].area, Some(Area { from: -1.0, to: 1.0, to_fn: None }));
        assert_eq!((e.areas[0][0].text.as_str(), e.areas[0][1].text.as_str()), ("-1", "1"));
        e.update(Msg::AreaTo(0, Some(1)));
        e.update(Msg::AreaTo(0, Some(0)));
        assert_eq!(e.plot.fns[0].area.as_ref().unwrap().to_fn, None, "not to itself");
        e.update(Msg::AreaTo(0, Some(1)));
        e.update(Msg::AreaRange(0, 0, "-1,5".into()));
        e.update(Msg::AreaRange(0, 1, "".into()));
        assert_eq!(e.area_error(0).as_deref(), Some("Fläche bis: keine Zahl"));
        assert!(e.result().is_err());
        e.update(Msg::AreaRange(0, 1, "pi/2".into()));
        assert_eq!(e.area_error(0), None);
        // An area on a function without a curve says so.
        e.update(Msg::Area(1, true));
        assert_eq!(e.plot.fns[1].area.as_ref().map(|a| (a.from, a.to)), Some((-3.0, 3.0)), "no zeros: the middle half");
        e.update(Msg::Expr(1, "x = 2".into()));
        assert_eq!(e.area_error(1).as_deref(), Some("Eine senkrechte Linie hat keine Fläche"));
        assert_eq!(e.area_error(0).as_deref(), Some("Fläche bis g: g ist eine senkrechte Linie"));
        e.update(Msg::Expr(1, "2".into()));
        e.update(Msg::Area(1, false));
        // The derivative: a new row, dashed in the same ink, named after the function.
        e.update(Msg::Style(1, LineStyle::Dotted));
        e.update(Msg::Derive(0));
        assert_eq!(e.plot.fns[2], PlotFn { style: LineStyle::Dashed, ..PlotFn::new("f'(x)", INKS[1]) });
        e.update(Msg::Derive(2));
        assert_eq!(e.plot.fns[3].expr, "h'(x)");
        // The axes.
        e.update(Msg::Pi(true));
        let pi = std::f64::consts::PI;
        assert_eq!((e.plot.x_min, e.plot.x_max), (-2.0 * pi, 2.0 * pi), "widened to whole π/2");
        assert_eq!((e.ranges[0].text.as_str(), e.ranges[1].text.as_str()), ("-2π", "2π"));
        e.update(Msg::XName(" t ".into()));
        e.update(Msg::Names(true));
        let Reply::Apply(got) = e.update(Msg::Ok) else { panic!("OK: {:?}", e.result()) };
        assert_eq!(got.marks, plot::MARK_ZEROS);
        assert!(got.coords && got.names && got.pi);
        assert_eq!(got.x_name, "t");
        assert_eq!(got.fns[0].area, Some(Area { from: -1.5, to: pi / 2.0, to_fn: Some(1) }));
        assert_eq!((got.fns[1].style, got.fns[1].area.clone()), (LineStyle::Dotted, None));
        assert_eq!(got.fns.len(), 4);
        assert_eq!((got.x_min, got.x_max), (-2.0 * pi, 2.0 * pi));
        // Removing f: the others keep meaning the same (g's derivative row follows).
        e.update(Msg::RemoveFn(0));
        assert_eq!(e.plot.fns.iter().map(|f| f.expr.as_str()).collect::<Vec<_>>(), vec!["2", "f'(x)", "g'(x)"]);
        assert!(e.fn_error(1).is_none());
        // No more than eight: one per name.
        while e.plot.fns.len() < MAX_FNS {
            e.update(Msg::AddFn);
        }
        assert_eq!(e.update(Msg::AddFn), Reply::None);
        e.update(Msg::Derive(0));
        assert_eq!(e.plot.fns.len(), MAX_FNS);
        // Points by hand: a mistake keeps OK off and says why.
        e.update(Msg::Points("A(1|2); B(3".into()));
        assert!(e.points_error().is_some() && e.result().is_err());
        e.update(Msg::Points("A(1|2); B(-1|0,5)".into()));
        let Reply::Apply(got) = e.update(Msg::Ok) else { panic!("OK: {:?}", e.result()) };
        assert_eq!(got.points, vec![plot::PlotPoint { name: "A".into(), x: 1.0, y: 2.0 }, plot::PlotPoint { name: "B".into(), x: -1.0, y: 0.5 }]);
        assert_eq!(got.fns.len(), 3, "the empty rows are left out");
    }

    #[test]
    fn an_empty_row_in_the_middle_is_left_out_and_the_others_keep_their_meaning() {
        let mut p = fresh(300.0, 400.0, 595.0, 842.0, Background::Blank, 22.0);
        p.fns = vec![PlotFn::new("x^2", INKS[1])];
        let mut e = Editor::new("s", p, (Background::Blank, 22.0));
        e.update(Msg::AddFn);
        e.update(Msg::AddFn);
        e.update(Msg::Expr(2, "f(x) + 1".into()));
        e.update(Msg::AddFn);
        e.update(Msg::Expr(3, "h'(x)".into()));
        e.update(Msg::Area(3, true));
        e.update(Msg::AreaTo(3, Some(2)));
        let Reply::Apply(got) = e.update(Msg::Ok) else { panic!("OK: {:?}", e.result()) };
        assert_eq!(got.fns.iter().map(|f| f.expr.as_str()).collect::<Vec<_>>(), vec!["x^2", "f(x) + 1", "g'(x)"]);
        assert_eq!(got.fns[2].area.as_ref().unwrap().to_fn, Some(1));
        // An area to an empty row: said, not stored.
        e.update(Msg::AreaTo(3, Some(1)));
        assert_eq!(e.area_error(3).as_deref(), Some("Fläche bis g: g ist leer"));
        assert_eq!(e.update(Msg::Ok), Reply::None);
    }

    #[test]
    fn a_new_plot_sits_on_the_squares_of_squared_paper() {
        let p = fresh(300.0, 400.0, 595.0, 842.0, Background::Grid, 17.0);
        assert_eq!((p.w, p.h), (12.0 * 17.0, 8.0 * 17.0));
        assert_eq!((p.x % 17.0, p.y % 17.0), (0.0, 0.0));
        assert_eq!((p.x_min, p.x_max, p.y_min, p.y_max), (-6.0, 6.0, -4.0, 4.0));
        assert!(!p.grid, "the paper is its grid");
        // Near the edge: still on the page and on the squares.
        let q = fresh(590.0, 838.0, 595.0, 842.0, Background::Grid, 17.0);
        assert!(q.x + q.w <= 595.0 && q.y + q.h <= 842.0 && q.x % 17.0 == 0.0 && q.y % 17.0 == 0.0, "{q:?}");
        // Plain paper: 240 × 160 pt, 20 pt to the unit, a grid of its own, centred.
        let r = fresh(300.0, 400.0, 595.0, 842.0, Background::Blank, 22.0);
        assert_eq!((r.x, r.y, r.w, r.h, r.x_max, r.y_max), (180.0, 320.0, 240.0, 160.0, 6.0, 4.0));
        assert!(r.grid && r.color == AXES && r.width == WIDTH && r.fns.is_empty());
        // A small page: fewer squares, inside it.
        let s = fresh(50.0, 50.0, 200.0, 150.0, Background::Blank, 22.0);
        assert!(s.w <= 200.0 * 0.85 && s.x >= 0.0 && s.y >= 0.0 && s.x + s.w <= 200.0 && s.y + s.h <= 150.0, "{s:?}");
    }

    #[test]
    fn the_editor_keeps_the_last_good_range_and_refuses_bad_functions() {
        let mut p = fresh(300.0, 400.0, 595.0, 842.0, Background::Blank, 22.0);
        p.id = "p".into();
        let mut e = Editor::new("s", p.clone(), (Background::Blank, 22.0));
        assert_eq!(e.plot.fns.len(), 1, "an empty function to type");
        assert_eq!(e.plot.fns[0].color, INKS[1]);
        // Nothing typed: OK stores the plot without functions.
        assert_eq!(e.update(Msg::Ok), Reply::Apply(p.clone()));
        e.update(Msg::Expr(0, "x^2 +".into()));
        assert!(e.fn_error(0).is_some());
        assert_eq!(e.update(Msg::Ok), Reply::None, "OK refuses a function with a mistake");
        e.update(Msg::Expr(0, "x^2 - 1".into()));
        e.update(Msg::Range(1, "1".into()));
        e.update(Msg::Range(1, "10".into()));
        e.update(Msg::Range(0, "".into()));
        assert_eq!((e.plot.x_min, e.plot.x_max), (-6.0, 10.0), "a half-typed range keeps the last one that worked");
        assert!(e.range_error(0).is_some() && e.result().is_err());
        e.update(Msg::Range(0, "-2,5".into()));
        assert_eq!(e.update(Msg::AddFn), Reply::Focus(1));
        assert_eq!(e.plot.fns[1].color, INKS[2]);
        e.update(Msg::Color(1));
        assert_eq!(e.plot.fns[1].color, INKS[3]);
        let Reply::Apply(got) = e.update(Msg::Ok) else { panic!("OK") };
        assert_eq!((got.x_min, got.x_max), (-2.5, 10.0));
        assert_eq!(got.fns, vec![PlotFn::new("x^2 - 1", INKS[1])], "the empty one is left out");
        e.update(Msg::FitY);
        assert!(e.plot.y_min <= 0.0 && e.plot.y_max >= 60.0, "{:?}", (e.plot.y_min, e.plot.y_max));
        e.update(Msg::EqualUnits);
        let per_x = (e.plot.x_max - e.plot.x_min) / e.plot.w as f64;
        assert!(((e.plot.y_max - e.plot.y_min) / e.plot.h as f64 - per_x).abs() < 1e-9);
        assert_eq!(e.update(Msg::Cancel), Reply::Cancel);
    }
}
