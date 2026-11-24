//! Coordinate systems on the laptop: constructions as in GeoGebra. Their look comes from the
//! core as ink ([`n0te_core::plot::render`]: axes, numbers, curves, points, drawn as if by hand)
//! and is drawn like strokes, worked out once per plot value. A click on one opens the
//! "Koordinatensystem" panel ([`Editor`]): the algebra view (one row per object as GeoGebra
//! lists it, the value with how it came about under it; a click edits its definition, the dot
//! shows or hides it, a slider drags), the input line with the tablet's commands (and the
//! commands that fit what is being typed, as GeoGebra suggests them), the properties of the
//! selected object, the look of the system ("Grafik") and a live preview (the selected object
//! bolder; a click selects an object, points and sliders drag). Drawing and tapping tools are
//! the tablet's. The palette and the top bar insert a new, empty one ([`fresh`]).

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::f64::consts::FRAC_PI_2;
use std::rc::Rc;

use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::graphics::mesh::Mesh;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Widget, tree};
use iced::advanced::{Clipboard, Shell, mouse};
use iced::widget::{Column, Row, Space, button, canvas, column, container, row, scrollable, slider, svg, text, text_input, toggler, tooltip};
use iced::keyboard::{self, key::Named};
use iced::{Alignment, Border, Color, Element, Event, Length, Padding, Point, Rectangle, Size, Task, Theme, Vector};

use n0te_core::ink::{Brush, InkPoint, Rect};
use n0te_core::notebook::Background;
use n0te_core::plot::lang::{self, Body, Node};
use n0te_core::plot::{self, Kind, LabelMode, LineStyle, MARK_CROSSINGS, MARK_EXTREMA, MARK_TURNS, MARK_ZEROS, Obj, PALETTE, Plot, PointStyle, Slider, Snap, Val};

use crate::i18n::{Lang, lang, tr, trf};
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
        // Typing and dragging in the panel make a new value each time: keep the cache small.
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
    // On squared paper it keeps to the squares (and points snap to them), like on the tablet.
    p.align = squares;
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

/// A number as it is written in a field here: "-6", "2,5", "0,125".
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

/// A number field of the panel: what it says, and the exact value behind it while it still
/// says what the panel wrote there (a stored -2π doesn't come back as -6,2832).
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

// --- the algebra view ----------------------------------------------------------------------

/// An object as the algebra view lists it, like GeoGebra: "f(x) = 0,5x² − 3", "A = (1 | 2)",
/// "g: y = 2x + 1", "c: (x − 1)² + y² = 4", "a = 2", "f(x) = x², 0 ≤ x ≤ 2", "a(b) = 3b",
/// "c: r = 1 + cos(θ)"; exact fractions as ¹⁄₃; an object with a mistake as typed. `known`:
/// the plot's names.
pub fn listing(o: &Obj, v: &Result<Val, String>, known: &HashSet<String>) -> String {
    let Ok(v) = v else { return o.def.trim().to_string() };
    match v {
        Val::Text(t) => format!("{} = „{t}“", o.name()),
        _ => plot::row_text(o, v, known, false),
    }
}

/// What an object is, as the properties name it.
pub fn kind_name(kind: Option<Kind>, slider: bool) -> &'static str {
    if slider {
        return tr("Schieberegler");
    }
    match kind {
        Some(Kind::Number) => tr("Zahl"),
        Some(Kind::Bool) => tr("Wahrheitswert"),
        Some(Kind::Point) => tr("Punkt"),
        Some(Kind::Points) => tr("Punkte"),
        Some(Kind::Vector) => tr("Vektor"),
        Some(Kind::Function) => tr("Funktion"),
        Some(Kind::Line) => tr("Gerade"),
        Some(Kind::Lines) => tr("Geraden"),
        Some(Kind::Segment) => tr("Strecke"),
        Some(Kind::Ray) => tr("Strahl"),
        Some(Kind::Circle) => tr("Kreis"),
        Some(Kind::Polygon) => tr("Vieleck"),
        Some(Kind::Curve) => tr("Kurve"),
        Some(Kind::Implicit) => tr("Kurve"),
        Some(Kind::Angle) => tr("Winkel"),
        Some(Kind::Area) => tr("Fläche"),
        Some(Kind::Text) => tr("Text"),
        None => tr("mit einem Fehler"),
    }
}

/// Kinds drawn with a line (they have a line width and style).
fn has_line(k: Option<Kind>) -> bool {
    matches!(k, Some(Kind::Function | Kind::Line | Kind::Lines | Kind::Segment | Kind::Ray | Kind::Circle | Kind::Polygon | Kind::Curve | Kind::Implicit | Kind::Vector | Kind::Angle))
}

/// Kinds that can be filled (hatched).
fn has_fill(k: Option<Kind>) -> bool {
    matches!(k, Some(Kind::Polygon | Kind::Circle | Kind::Angle | Kind::Area))
}

/// Whether an object is a plain number (`a = 2`): it can become a slider.
fn plain_number(o: &Obj) -> bool {
    let known = HashSet::from([o.name()]);
    match lang::parse_input(&o.def, &known) {
        Ok(input) => match input.body {
            Body::Expr(Node::Num(_)) => true,
            Body::Expr(Node::Neg(n)) => matches!(*n, Node::Num(_)),
            _ => false,
        },
        Err(_) => false,
    }
}

/// Whether object `i` is one that drags: a slider, a free point, a point on an object.
fn movable(p: &Plot, i: usize) -> bool {
    let o = &p.objs[i];
    o.style.slider.is_some() || plot::free_point(&o.def) || plot::moved_on(&o.def, 0.0).is_some()
}

/// Whether object `i` can be dragged in the preview: [`movable`] and not fixed.
fn draggable(p: &Plot, i: usize) -> bool {
    !p.objs[i].style.fixed && movable(p, i)
}

/// "g", "g und S", "g, h und S".
fn and_list(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => trf("{} und {}", &[&rest.join(", "), last]),
    }
}

/// The command buttons under the input line: label, what it inserts, what the help line then
/// says.
pub const QUICK: [(&str, &str, &str); 10] = [
    ("Punkt", "Punkt()", "Punkt(f, 1): ein Punkt auf f, der sich daran entlang ziehen lässt · ein freier Punkt: (1|2)"),
    ("Gerade", "Gerade()", "Gerade(A, B): die Gerade durch A und B · oder als Gleichung: g: y = 2x + 1"),
    ("Kreis", "Kreis()", "Kreis(M, 3): um M mit Radius 3 · Kreis(M, A): durch A · Kreis(A, B, C): durch drei Punkte"),
    ("Schneide", "Schneide()", "Schneide(f, g): wo sich f und g schneiden · Schneide(f, g, 1): nur der erste Schnittpunkt"),
    ("Nullstelle", "Nullstelle()", "Nullstelle(f): die Nullstellen von f im Bild · Nullstelle(f, 0, 3): zwischen 0 und 3"),
    ("Extremum", "Extremum()", "Extremum(f): die Hoch- und Tiefpunkte von f · Wendepunkt(f): die Wendepunkte"),
    ("Tangente", "Tangente()", "Tangente(1, f): die Tangente an f bei x = 1 · Tangente(A, f) · Tangente(A, Kreis)"),
    ("Integral", "Integral()", "Integral(f, 0, 2): die Fläche unter f von 0 bis 2 · IntegralZwischen(f, g, 0, 2)"),
    ("Ableitung", "Ableitung()", "Ableitung(f): die Ableitung von f · f'(x) geht auch"),
    ("Schieberegler", "Schieberegler()", "Schieberegler(0, 10, 0.5): eine Zahl zum Ziehen, von 0 bis 10 in Schritten von 0,5 · oder einfach a = 2"),
];

/// What the help line says before a command button is pressed.
pub const HELP: &str = "Zum Beispiel f(x) = x^2 − 3, A = (1|2) oder a = 2. Befehle schlägt die Zeile beim Tippen vor.";

/// The commands the input line suggests while one is typed, like GeoGebra's: the German name,
/// the English one (the core takes both), example arguments, what it makes. The core's
/// [`lang::COMMANDS`], written as GeoGebra writes them.
pub const COMMANDS: [(&str, &str, &str, &str); 35] = [
    ("Punkt", "Point", "f, 1", "ein Punkt auf f, zum Ziehen"),
    ("Gerade", "Line", "A, B", "durch zwei Punkte"),
    ("Strecke", "Segment", "A, B", "von A nach B"),
    ("Strahl", "Ray", "A, B", "ab A durch B"),
    ("Vektor", "Vector", "A, B", "von A nach B"),
    ("Kreis", "Circle", "M, 3", "um M mit Radius 3"),
    ("Vieleck", "Polygon", "A, B, C", "durch seine Ecken"),
    ("Mittelpunkt", "Midpoint", "A, B", "zwischen A und B"),
    ("Schneide", "Intersect", "f, g", "die Schnittpunkte"),
    ("Nullstelle", "Root", "f", "die Nullstellen"),
    ("Extremum", "Extremum", "f", "die Hoch- und Tiefpunkte"),
    ("Wendepunkt", "InflectionPoint", "f", "die Wendepunkte"),
    ("Ableitung", "Derivative", "f", "die Ableitung"),
    ("Integral", "Integral", "f, 0, 2", "die Fläche unter f"),
    ("IntegralZwischen", "IntegralBetween", "f, g, 0, 2", "die Fläche zwischen f und g"),
    ("Tangente", "Tangent", "1, f", "an f bei x = 1"),
    ("Senkrechte", "PerpendicularLine", "A, g", "durch A senkrecht zu g"),
    ("Parallele", "Parallel", "A, g", "durch A parallel zu g"),
    ("Mittelsenkrechte", "PerpendicularBisector", "A, B", "der Strecke von A nach B"),
    ("Winkelhalbierende", "AngleBisector", "A, B, C", "des Winkels bei B"),
    ("Abstand", "Distance", "A, B", "zwischen Punkten, Punkt und Gerade"),
    ("Länge", "Length", "A, B", "einer Strecke"),
    ("Steigung", "Slope", "g", "einer Geraden"),
    ("Winkel", "Angle", "A, B, C", "bei B"),
    ("Funktion", "Function", "f, 0, 2", "f nur von 0 bis 2"),
    ("Wenn", "If", "x < 0, -x, x", "je nach Bedingung"),
    ("Kurve", "Curve", "cos(t), sin(t), t, 0, 2π", "in Parameterform"),
    ("Text", "Text", "\"…\"", "ein Text im Bild"),
    ("Schieberegler", "Slider", "-5, 5, 0.1", "eine Zahl zum Ziehen"),
    ("Spiegle", "Reflect", "A, g", "an einer Geraden oder einem Punkt"),
    ("Verschiebe", "Translate", "A, v", "um einen Vektor"),
    ("Drehe", "Rotate", "A, 90°, M", "um einen Punkt"),
    ("Fläche", "Area", "A, B, C", "eines Vielecks"),
    ("Min", "Min", "a, b", "die kleinere Zahl"),
    ("Max", "Max", "a, b", "die größere Zahl"),
];

/// A command by its German name as the app shows it: GeoGebra's English name when the app
/// speaks English ("Schneide" → "Intersect"), else as it is.
pub fn command_name(de: &'static str) -> &'static str {
    match lang() {
        Lang::En => COMMANDS.iter().find(|c| c.0 == de).map_or(de, |c| c.1),
        Lang::De => de,
    }
}

/// How many suggestions show at most.
const SUGGESTIONS: usize = 5;

/// A command suggested under the input line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Suggestion {
    /// As shown and inserted: the German name, or the English one when that is being typed.
    pub name: &'static str,
    /// The German name (the command buttons go by it).
    pub de: &'static str,
    pub args: &'static str,
    pub about: &'static str,
}

impl Suggestion {
    /// How it goes: "Schneide(f, g)".
    pub fn usage(&self) -> String {
        format!("{}({})", self.name, self.args)
    }
}

/// The commands starting with `typed` (at least two letters, case and "ae" for "ä" don't
/// matter), the German names first (the English ones when the app speaks English), an exact
/// match before the others.
pub fn suggest(typed: &str) -> Vec<Suggestion> {
    let t = typed.to_lowercase();
    if t.chars().count() < 2 {
        return Vec::new();
    }
    let starts = |name: &str| {
        let n = name.to_lowercase();
        n.starts_with(&t) || n.replace('ä', "ae").starts_with(&t)
    };
    let english = lang() == Lang::En;
    let mut found: Vec<(bool, bool, usize, Suggestion)> = Vec::new();
    for (k, &(de, en, args, about)) in COMMANDS.iter().enumerate() {
        let (first, second) = if english { (en, de) } else { (de, en) };
        let name = if starts(first) {
            first
        } else if starts(second) {
            second
        } else {
            continue;
        };
        found.push((name.to_lowercase() != t, name != first, k, Suggestion { name, de, args, about }));
    }
    found.sort_by_key(|f| (f.0, f.1, f.2));
    found.into_iter().take(SUGGESTIONS).map(|f| f.3).collect()
}

/// What a function marks by itself, as the panel names it.
pub fn mark_names() -> [(u8, &'static str); 4] {
    [(MARK_ZEROS, tr("Nullstellen")), (MARK_EXTREMA, tr("Hoch- und Tiefpunkte")), (MARK_CROSSINGS, tr("Schnittpunkte")), (MARK_TURNS, tr("Wendepunkte"))]
}

/// The label modes, as the panel names them.
pub fn label_modes() -> [(LabelMode, &'static str); 5] {
    [(LabelMode::Off, tr("Aus")), (LabelMode::Name, "Name"), (LabelMode::NameValue, tr("Name & Wert")), (LabelMode::Value, tr("Wert")), (LabelMode::Caption, tr("Beschriftung"))]
}

/// The input line (focus).
pub fn input_id() -> iced::widget::Id {
    iced::widget::Id::new("n0te-plot-input")
}

/// The field of the row being edited.
pub fn edit_id() -> iced::widget::Id {
    iced::widget::Id::new("n0te-plot-edit")
}

/// An object's slider in the algebra view.
pub fn slider_id(obj: &str) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-plot-slider-{obj}"))
}

/// An object's dot (shown / hidden) in the algebra view.
pub fn dot_id(obj: &str) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-plot-dot-{obj}"))
}

/// The preview.
pub fn preview_id() -> iced::widget::Id {
    iced::widget::Id::new("n0te-plot-preview")
}

/// Puts the cursor into field `id`: at `at` characters, else at the end.
pub fn focus<M: Send + 'static>(id: iced::widget::Id, at: Option<usize>) -> Task<M> {
    let place = match at {
        Some(n) => iced::widget::operation::move_cursor_to(id.clone(), n),
        None => iced::widget::operation::move_cursor_to_end(id.clone()),
    };
    iced::widget::operation::focus(id).chain(place)
}

/// Puts the cursor into field `id` with characters `from` … `to` selected (typing replaces them).
pub fn select<M: Send + 'static>(id: iced::widget::Id, from: usize, to: usize) -> Task<M> {
    iced::widget::operation::focus(id.clone()).chain(iced::widget::operation::select_range(id, from, to))
}

// --- the panel -----------------------------------------------------------------------------

/// A row of the algebra view being edited: the object, what the field says, what is wrong.
#[derive(Debug, Clone, PartialEq)]
pub struct RowEdit {
    pub id: String,
    pub text: String,
    pub error: Option<String>,
}

/// What the right side shows under the preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// The selected object's properties.
    Object,
    /// The coordinate system's look.
    Graphics,
}

/// What the panel does.
#[derive(Debug, Clone)]
pub enum Msg {
    /// The input line typed.
    Input(String),
    /// Where the input line's cursor is now (characters), after a key or click it took.
    Cursor(usize),
    /// A suggestion taken (index into [`Editor::suggestions`]; Tab takes the first).
    Complete(usize),
    /// Enter in the input line: a new object.
    InputSubmit,
    /// A command button (index into [`QUICK`]).
    Insert(usize),
    /// A row clicked: it is selected and its definition edited.
    Edit(String),
    EditText(String),
    /// Enter in a row: its definition changed.
    EditSubmit,
    /// The dot of a row: shown or hidden.
    Visible(String),
    /// A slider of the algebra view moved.
    Slider(String, f64),
    /// The × of a row: deleted (asked first when others depend on it).
    AskRemove(String),
    /// Deleted with everything that depends on it.
    Remove(String),
    /// Not deleted after all.
    KeepIt,
    /// An object clicked in the preview: selected.
    Pick(String),
    /// An object dragged in the preview to a page point.
    Drag(String, f32, f32),
    Tab(Tab),
    // The selected object's properties.
    Color(u32),
    Width(f32),
    Line(LineStyle),
    Point(PointStyle),
    Size(f32),
    Fill(f32),
    Label(LabelMode),
    Caption(String),
    Shown(bool),
    Fixed(bool),
    Aux(bool),
    /// What a function marks by itself ([`plot::MARK_ZEROS`] …) on or off.
    Special(u8, bool),
    /// Coordinates at those points.
    Coords(bool),
    /// A plain number as a slider, or not.
    MakeSlider(bool),
    /// A slider field (0 from, 1 to, 2 step) typed.
    SliderField(usize, String),
    /// A domain field of the selected function (0 from, 1 to) typed (empty: open that way).
    DomainField(usize, String),
    /// The selected function defined everywhere again.
    Everywhere,
    /// The selected row's switch: as a curve of its letters (true) or a number with a slider.
    AsCurve(bool),
    /// An end of a function's domain (0 from, 1 to) dragged in the preview to a page point.
    DragEnd(String, u8, f32, f32),
    /// An axis grabbed in the preview at a page position along it (x for the x axis, y for y).
    AxisGrab(plot::Axis, f32),
    /// The grabbed axis dragged to a page position: only it stretches.
    AxisTo(f32),
    /// The axis let go.
    AxisLet,
    // The coordinate system.
    /// Range field (0 x from, 1 x to, 2 y from, 3 y to) typed.
    Range(usize, String),
    /// The ranges back to those the panel opened with.
    ResetView,
    FitY,
    EqualUnits,
    /// The x axis in steps of π.
    Pi(bool),
    XName(String),
    YName(String),
    Axes(bool),
    Grid(bool),
    Minor(bool),
    Snap(Snap),
    Clean(bool),
    Interactive(bool),
    /// Esc: hides the suggestions, leaves a row being edited (or the question), else like
    /// Abbrechen.
    Escape,
    Delete,
    Cancel,
    Ok,
}

/// What the app does after a panel message.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    None,
    /// Put the cursor in a field (at that many characters, else at the end): see [`focus`].
    Focus(iced::widget::Id, Option<usize>),
    /// Put the cursor in a field with those characters selected: see [`select`].
    Select(iced::widget::Id, usize, usize),
    Cancel,
    Delete,
    /// Store this plot (one step).
    Apply(Plot),
}

/// The "Koordinatensystem" panel for one plot. It edits a copy of the stored plot (the draft):
/// every change shows at once in the preview, OK stores it in one step, Abbrechen forgets it.
pub struct Editor {
    pub sheet_id: String,
    /// The draft: always drawable (the ranges take typed values only once they work).
    pub plot: Plot,
    /// The plot as it was when the panel opened.
    pub original: Plot,
    /// What the input line says, and what is wrong with it after Enter.
    pub input: String,
    pub input_error: Option<String>,
    /// Where the input line's cursor is (characters; None: at the end).
    pub cursor: Option<usize>,
    /// Whether commands may be suggested: the input line was typed in last.
    pub suggesting: bool,
    /// What the help line says (the last command button's usage; None: [`HELP`]).
    pub hint: Option<&'static str>,
    /// The selected object (its properties show).
    pub selected: Option<String>,
    pub editing: Option<RowEdit>,
    /// An object asked about before it is deleted with the objects that depend on it.
    pub confirm: Option<String>,
    pub tab: Tab,
    /// The range fields: x from, x to, y from, y to.
    pub ranges: [NumField; 4],
    /// The selected slider's fields: from, to, step.
    pub slider_fields: [NumField; 3],
    /// The selected function's domain fields (from, to; empty: open that way) and what is
    /// wrong with them.
    pub domain_fields: [String; 2],
    pub domain_error: Option<String>,
    /// An axis being stretched in the preview: which, where it was grabbed, the plot then.
    axis_drag: Option<(plot::Axis, f32, Plot)>,
    /// The page's paper (for the preview).
    pub paper: (Background, f32),
    preview: RefCell<Option<(Plot, canvas::Cache)>>,
}

impl Editor {
    pub fn new(sheet_id: &str, plot: Plot, paper: (Background, f32)) -> Editor {
        let ranges = [NumField::of(plot.x_min, plot.pi), NumField::of(plot.x_max, plot.pi), NumField::of(plot.y_min, false), NumField::of(plot.y_max, false)];
        Editor {
            sheet_id: sheet_id.to_string(),
            original: plot.clone(),
            plot,
            input: String::new(),
            input_error: None,
            cursor: None,
            suggesting: false,
            hint: None,
            selected: None,
            editing: None,
            confirm: None,
            tab: Tab::Graphics,
            ranges,
            slider_fields: Default::default(),
            domain_fields: Default::default(),
            domain_error: None,
            axis_drag: None,
            paper,
            preview: RefCell::new(None),
        }
    }

    /// The domain fields as the selected function's domain says.
    fn show_domain(&mut self) {
        let d = self.selected.as_deref().and_then(|id| self.plot.domain(id));
        let field = |e: plot::End| e.map(|(v, _)| fmt_num(v)).unwrap_or_default();
        self.domain_fields = match d {
            Some((lo, hi)) => [field(lo), field(hi)],
            None => Default::default(),
        };
        self.domain_error = None;
    }

    fn index(&self, id: &str) -> Option<usize> {
        self.plot.objs.iter().position(|o| o.id == id)
    }

    /// The selected object's index.
    pub fn selected_index(&self) -> Option<usize> {
        self.selected.as_deref().and_then(|id| self.index(id))
    }

    fn select(&mut self, id: Option<String>) {
        if self.selected != id {
            self.selected = id;
            let s = self.selected_index().and_then(|i| self.plot.objs[i].style.slider);
            self.slider_fields = match s {
                Some(s) => [NumField::of(s.min, false), NumField::of(s.max, false), NumField::of(s.step, false)],
                None => Default::default(),
            };
            self.show_domain();
        }
        if self.selected.is_some() {
            self.tab = Tab::Object;
        }
    }

    /// The names of objects (by id).
    fn names_of(&self, ids: &[String]) -> Vec<String> {
        ids.iter().filter_map(|id| self.index(id)).map(|i| self.plot.objs[i].name()).collect()
    }

    /// The input line as a new object; false if it has a mistake (said under it). Empty: fine.
    fn take_input(&mut self) -> bool {
        let t = self.input.trim();
        if t.is_empty() {
            self.input_error = None;
            return true;
        }
        match self.plot.add(t) {
            Ok(p) => {
                let id = p.objs.last().map(|o| o.id.clone());
                self.plot = p;
                self.input.clear();
                self.cursor = None;
                self.input_error = None;
                self.hint = None;
                self.select(id);
                true
            }
            Err(e) => {
                self.input_error = Some(e);
                false
            }
        }
    }

    /// The draft with object `id` defined as `text`, checked like GeoGebra checks a
    /// redefinition: the name stays when none is typed (an equation gets it as a label), a new
    /// name must be free and nothing may depend on the old one, and the objects that worked
    /// must still work.
    pub fn redefine(&self, id: &str, text: &str) -> Result<Plot, String> {
        let i = self.index(id).ok_or(tr("Dieses Objekt gibt es nicht mehr"))?;
        let old = self.plot.objs[i].name();
        let text = text.trim();
        let text = match plot::name_of(text) {
            // "y = 2x + 1" without a name: "g: y = 2x + 1" (the core would write "g = y = …").
            None if text.contains('=') => format!("{old}: {text}"),
            _ => text.to_string(),
        };
        if let Some(new) = plot::name_of(&text)
            && new != old
        {
            if self.plot.objs.iter().any(|o| o.name() == new) {
                return Err(trf("Den Namen {} gibt es schon", &[&new]));
            }
            let deps = self.names_of(&self.plot.dependents(id));
            if !deps.is_empty() {
                return Err(if deps.len() == 1 { trf("Umbenennen geht nicht: {} hängt von {} ab", &[&and_list(&deps), &old]) } else { trf("Umbenennen geht nicht: {} hängen von {} ab", &[&and_list(&deps), &old]) });
            }
        }
        let before = self.plot.errors();
        let p = self.plot.set_def(id, &text)?;
        if let Some(j) = p.errors().iter().zip(&before).position(|(now, was)| now.is_some() && was.is_none()) {
            return Err(trf("Dann stimmt {} nicht mehr: {}", &[&p.objs[j].name(), &p.errors()[j].clone().unwrap_or_default()]));
        }
        Ok(p)
    }

    /// The row being edited taken into the draft (when changed); false if it has a mistake
    /// (said under it, the row stays open).
    fn finish_edit(&mut self) -> bool {
        let Some(e) = self.editing.clone() else { return true };
        let Some(i) = self.index(&e.id) else {
            self.editing = None;
            return true;
        };
        if e.text.trim() == self.plot.objs[i].def.trim() {
            self.editing = None;
            return true;
        }
        match self.redefine(&e.id, &e.text) {
            Ok(p) => {
                self.plot = p;
                self.editing = None;
                self.show_domain();
                true
            }
            Err(err) => {
                self.editing = Some(RowEdit { error: Some(err), ..e });
                false
            }
        }
    }

    fn remove(&mut self, id: &str) {
        self.plot = self.plot.remove(id);
        self.confirm = None;
        if self.selected.as_deref().is_some_and(|s| self.index(s).is_none()) {
            self.select(None);
        }
        if self.editing.as_ref().is_some_and(|e| self.index(&e.id).is_none()) {
            self.editing = None;
        }
    }

    /// The selected object's style, to change it.
    fn style_mut(&mut self) -> Option<&mut plot::Style> {
        let i = self.selected_index()?;
        Some(&mut self.plot.objs[i].style)
    }

    /// A row's text field forgets what it says when its object changes elsewhere (dragged).
    fn moved(&mut self, id: &str) {
        if self.editing.as_ref().is_some_and(|e| e.id == id) {
            self.editing = None;
        }
    }

    /// The word at the input line's cursor (a run of letters), in characters: where it starts,
    /// the cursor, where it ends.
    fn word(&self) -> (usize, usize, usize) {
        let chars: Vec<char> = self.input.chars().collect();
        let at = self.cursor.unwrap_or(chars.len()).min(chars.len());
        let start = chars[..at].iter().rposition(|c| !c.is_alphabetic()).map_or(0, |i| i + 1);
        let end = chars[at..].iter().position(|c| !c.is_alphabetic()).map_or(chars.len(), |i| at + i);
        (start, at, end)
    }

    /// The commands suggested for what is being typed at the cursor, like GeoGebra's (none
    /// while the input line isn't being typed in).
    pub fn suggestions(&self) -> Vec<Suggestion> {
        if !self.suggesting {
            return Vec::new();
        }
        let (start, at, _) = self.word();
        suggest(&self.input.chars().skip(start).take(at - start).collect::<String>())
    }

    /// Suggestion `k` in place of the word at the cursor: its usage with the example arguments
    /// selected (typing replaces them), or just its name when brackets follow already.
    fn complete(&mut self, k: usize) -> Reply {
        let Some(s) = self.suggestions().get(k).copied() else { return Reply::None };
        let (start, _, end) = self.word();
        let chars: Vec<char> = self.input.chars().collect();
        let (before, after): (String, String) = (chars[..start].iter().collect(), chars[end..].iter().collect());
        let brackets = after.starts_with(['(', '[']);
        let insert = if brackets { s.name.to_string() } else { s.usage() };
        self.input = format!("{before}{insert}{after}");
        self.input_error = None;
        self.suggesting = false;
        self.hint = QUICK.iter().find(|q| q.0 == s.de).map(|q| q.2);
        // Just after the opening bracket.
        let inside = start + s.name.chars().count() + 1;
        if brackets {
            self.cursor = Some(inside);
            return Reply::Focus(input_id(), Some(inside));
        }
        // A text's quotes stay.
        let (from, to) = match s.args.strip_prefix('"').and_then(|a| a.strip_suffix('"')) {
            Some(a) => (inside + 1, inside + 1 + a.chars().count()),
            None => (inside, inside + s.args.chars().count()),
        };
        self.cursor = Some(to);
        Reply::Select(input_id(), from, to)
    }

    /// Whether the ranges (as typed) are not those the panel opened with.
    pub fn view_changed(&self) -> bool {
        let o = &self.original;
        [o.x_min, o.x_max, o.y_min, o.y_max].iter().zip(&self.ranges).any(|(v, f)| f.value() != Some(*v))
    }

    pub fn update(&mut self, msg: Msg) -> Reply {
        // Suggestions show while the input line is being typed in; anything else hides them.
        if !matches!(msg, Msg::Input(_) | Msg::Cursor(_) | Msg::Complete(_) | Msg::Escape) {
            self.suggesting = false;
        }
        match msg {
            Msg::Input(s) => {
                self.input = s;
                self.input_error = None;
                self.suggesting = true;
                // Until the line says where its cursor is.
                self.cursor = None;
            }
            Msg::Cursor(at) => self.cursor = Some(at),
            Msg::Complete(k) => return self.complete(k),
            Msg::InputSubmit => {
                let added = !self.input.trim().is_empty();
                // The cursor stays in the line for the next object.
                if self.take_input() && added {
                    return Reply::Focus(input_id(), None);
                }
            }
            Msg::Insert(k) => {
                if let Some(&(label, insert, usage)) = QUICK.get(k) {
                    // In English the button's English name: "Point()" (the core takes both).
                    let name = command_name(label);
                    if name == label {
                        self.input.push_str(insert);
                    } else {
                        self.input.push_str(&format!("{name}()"));
                    }
                    self.input_error = None;
                    self.hint = Some(usage);
                    // Between the brackets.
                    let at = self.input.chars().count() - 1;
                    self.cursor = Some(at);
                    return Reply::Focus(input_id(), Some(at));
                }
            }
            Msg::Edit(id) => {
                if self.editing.as_ref().is_some_and(|e| e.id == id) {
                    return Reply::None;
                }
                if !self.finish_edit() {
                    return Reply::None;
                }
                let Some(i) = self.index(&id) else { return Reply::None };
                self.confirm = None;
                self.editing = Some(RowEdit { id: id.clone(), text: self.plot.objs[i].def.clone(), error: None });
                self.select(Some(id));
                return Reply::Focus(edit_id(), None);
            }
            Msg::EditText(s) => {
                if let Some(e) = &mut self.editing {
                    e.text = s;
                    e.error = None;
                }
            }
            Msg::EditSubmit => {
                self.finish_edit();
            }
            Msg::Visible(id) => {
                if let Some(i) = self.index(&id) {
                    let s = &mut self.plot.objs[i].style;
                    s.visible = !s.visible;
                }
            }
            Msg::Slider(id, v) => {
                if let Some(p) = self.plot.set_slider(&id, v) {
                    self.plot = p;
                    self.moved(&id);
                }
            }
            Msg::AskRemove(id) => {
                if self.plot.dependents(&id).is_empty() {
                    self.remove(&id);
                } else {
                    self.confirm = Some(id);
                }
            }
            Msg::Remove(id) => self.remove(&id),
            Msg::KeepIt => self.confirm = None,
            Msg::Pick(id) => {
                if self.index(&id).is_some() {
                    self.select(Some(id));
                }
            }
            Msg::Drag(id, x, y) => {
                let Some(i) = self.index(&id) else { return Reply::None };
                let moved = match (plot::slider_track(&self.plot, i), self.plot.objs[i].style.slider) {
                    (Some((x0, x1, _)), Some(s)) => {
                        let t = ((x - x0) / (x1 - x0).max(1e-3)).clamp(0.0, 1.0) as f64;
                        self.plot.set_slider(&id, s.min + t * (s.max - s.min))
                    }
                    _ => {
                        let (px, py) = self.plot.to_plot(x, y);
                        self.plot.drag(&id, px, py)
                    }
                };
                if let Some(p) = moved {
                    self.plot = p;
                    self.moved(&id);
                }
            }
            Msg::Tab(t) => self.tab = t,
            Msg::Color(c) => {
                if let Some(s) = self.style_mut() {
                    s.color = c;
                }
            }
            Msg::Width(w) => {
                if let Some(s) = self.style_mut() {
                    s.width = (w * 10.0).round() / 10.0;
                }
            }
            Msg::Line(l) => {
                if let Some(s) = self.style_mut() {
                    s.line = l;
                }
            }
            Msg::Point(p) => {
                if let Some(s) = self.style_mut() {
                    s.point = p;
                }
            }
            Msg::Size(v) => {
                if let Some(s) = self.style_mut() {
                    s.size = (v * 10.0).round() / 10.0;
                }
            }
            Msg::Fill(v) => {
                if let Some(s) = self.style_mut() {
                    s.fill = ((v * 100.0).round() / 100.0).clamp(0.0, 1.0);
                }
            }
            Msg::Label(l) => {
                if let Some(s) = self.style_mut() {
                    s.label = l;
                }
            }
            Msg::Caption(c) => {
                if let Some(s) = self.style_mut() {
                    s.caption = c;
                }
            }
            Msg::Shown(on) => {
                if let Some(s) = self.style_mut() {
                    s.visible = on;
                }
            }
            Msg::Fixed(on) => {
                if let Some(s) = self.style_mut() {
                    s.fixed = on;
                }
            }
            Msg::Aux(on) => {
                if let Some(s) = self.style_mut() {
                    s.aux = on;
                }
            }
            Msg::Special(bit, on) => {
                if let Some(s) = self.style_mut() {
                    if on {
                        s.special |= bit;
                    } else {
                        s.special &= !bit;
                    }
                }
            }
            Msg::Coords(on) => {
                if let Some(s) = self.style_mut() {
                    s.coords = on;
                }
            }
            Msg::MakeSlider(on) => {
                let Some(i) = self.selected_index() else { return Reply::None };
                if on && self.plot.objs[i].style.slider.is_none() && plain_number(&self.plot.objs[i]) {
                    // A range around it, like the core gives a typed number.
                    let v = match self.plot.values()[i] {
                        Ok(Val::Num(v)) => v,
                        _ => 1.0,
                    };
                    let r = (v.abs() * 2.0).max(5.0);
                    let step = if v.fract() == 0.0 { 0.1 } else { 0.01 };
                    self.plot.objs[i].style.slider = Some(Slider { min: -r, max: r, step });
                    self.slider_fields = [NumField::of(-r, false), NumField::of(r, false), NumField::of(step, false)];
                } else if !on {
                    self.plot.objs[i].style.slider = None;
                }
            }
            Msg::SliderField(k, s) if k < 3 => {
                self.slider_fields[k].text = s;
                if self.slider_error().is_none()
                    && let Some(i) = self.selected_index()
                    && self.plot.objs[i].style.slider.is_some()
                {
                    let v = |k: usize| self.slider_fields[k].value().unwrap_or(0.0);
                    let id = self.plot.objs[i].id.clone();
                    self.plot.objs[i].style.slider = Some(Slider { min: v(0), max: v(1), step: v(2) });
                    // The value stays in the new range.
                    if let Ok(Val::Num(now)) = self.plot.values()[i]
                        && (now < v(0) || now > v(1))
                        && let Some(p) = self.plot.set_slider(&id, now)
                    {
                        self.plot = p;
                    }
                }
            }
            Msg::DomainField(k, s) if k < 2 => {
                self.domain_fields[k] = s;
                let Some(id) = self.selected.clone() else { return Reply::None };
                let read = |t: &str| if t.trim().is_empty() { Some(None) } else { number(t).map(Some) };
                match (read(&self.domain_fields[0]), read(&self.domain_fields[1])) {
                    (Some(lo), Some(hi)) => match self.plot.set_domain(&id, lo, hi) {
                        Ok(p) => {
                            self.plot = p;
                            self.domain_error = None;
                            self.moved(&id);
                        }
                        Err(e) => self.domain_error = Some(e),
                    },
                    _ => self.domain_error = Some(tr("Das ist keine Zahl").into()),
                }
            }
            Msg::Everywhere => {
                if let Some(id) = self.selected.clone()
                    && let Ok(p) = self.plot.set_domain(&id, None, None)
                {
                    self.plot = p;
                    self.moved(&id);
                    self.show_domain();
                }
            }
            Msg::AsCurve(curve) => {
                let Some(id) = self.selected.clone() else { return Reply::None };
                let r = if curve { self.plot.as_curve(&id) } else { self.plot.as_sliders(&id) };
                if let Ok(p) = r {
                    self.plot = p;
                    self.moved(&id);
                    let s = self.selected_index().and_then(|i| self.plot.objs[i].style.slider);
                    self.slider_fields = s.map(|s| [NumField::of(s.min, false), NumField::of(s.max, false), NumField::of(s.step, false)]).unwrap_or_default();
                    self.show_domain();
                }
            }
            Msg::DragEnd(id, side, x, y) => {
                let (px, _) = self.plot.to_plot(x, y);
                if let Some(p) = self.plot.drag_domain_end(&id, side, px) {
                    self.plot = p;
                    self.moved(&id);
                    if self.selected.as_deref() == Some(id.as_str()) {
                        self.show_domain();
                    } else {
                        self.select(Some(id));
                    }
                }
            }
            Msg::AxisGrab(axis, at) => self.axis_drag = Some((axis, at, self.plot.clone())),
            Msg::AxisTo(to) => {
                if let Some((axis, from, start)) = &self.axis_drag {
                    let p = start.axis_dragged(*axis, *from, to);
                    (self.plot.x_min, self.plot.x_max, self.plot.y_min, self.plot.y_max) = (p.x_min, p.x_max, p.y_min, p.y_max);
                    let pi = self.plot.pi;
                    self.ranges = [NumField::of(p.x_min, pi), NumField::of(p.x_max, pi), NumField::of(p.y_min, false), NumField::of(p.y_max, false)];
                }
            }
            Msg::AxisLet => self.axis_drag = None,
            Msg::Range(i, s) if i < 4 => {
                self.ranges[i].text = s;
                self.take_range(i / 2);
            }
            Msg::ResetView => {
                let o = &self.original;
                (self.plot.x_min, self.plot.x_max, self.plot.y_min, self.plot.y_max) = (o.x_min, o.x_max, o.y_min, o.y_max);
                self.ranges = [NumField::of(o.x_min, self.plot.pi), NumField::of(o.x_max, self.plot.pi), NumField::of(o.y_min, false), NumField::of(o.y_max, false)];
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
            Msg::Axes(on) => self.plot.axes = on,
            Msg::Grid(on) => self.plot.grid = on,
            Msg::Minor(on) => self.plot.minor = on,
            Msg::Snap(s) => self.plot.snap = s,
            Msg::Clean(on) => self.plot.clean = on,
            Msg::Interactive(on) => self.plot.interactive = on,
            Msg::Escape => {
                // The suggestions first (the line keeps its cursor: see [`InputLine`]).
                if !self.suggestions().is_empty() {
                    self.suggesting = false;
                } else if self.editing.take().is_none() && self.confirm.take().is_none() {
                    return Reply::Cancel;
                }
            }
            Msg::Ok => {
                // What is still typed counts, as if Enter was pressed.
                if !self.finish_edit() || !self.take_input() {
                    return Reply::None;
                }
                if let Ok(p) = self.result() {
                    return Reply::Apply(p);
                }
            }
            Msg::Delete => return Reply::Delete,
            Msg::Cancel => return Reply::Cancel,
            Msg::SliderField(..) | Msg::Range(..) | Msg::DomainField(..) => {}
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

    /// An axis' range (0: x, 1: y) as typed goes into the draft once it works (the preview
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

    /// What is wrong with an axis' range (0: x, 1: y), if anything.
    pub fn range_error(&self, axis: usize) -> Option<String> {
        let (a, b) = (self.ranges[axis * 2].value(), self.ranges[axis * 2 + 1].value());
        let n = if axis == 0 { "x" } else { "y" };
        match (a, b) {
            (None, _) => Some(trf("{} von: keine Zahl", &[&n])),
            (_, None) => Some(trf("{} bis: keine Zahl", &[&n])),
            (Some(a), Some(b)) if b <= a => Some(trf("{} bis muss größer sein als {} von", &[&n, &n])),
            _ => None,
        }
    }

    /// What is wrong with the selected slider's fields, if anything.
    pub fn slider_error(&self) -> Option<String> {
        let v: Vec<Option<f64>> = self.slider_fields.iter().map(|f| f.value()).collect();
        match (v[0], v[1], v[2]) {
            (None, _, _) => Some(tr("von: keine Zahl").into()),
            (_, None, _) => Some(tr("bis: keine Zahl").into()),
            (_, _, None) => Some(tr("Schritt: keine Zahl").into()),
            (Some(a), Some(b), _) if b <= a => Some(tr("bis muss größer sein als von").into()),
            (_, _, Some(s)) if s < 0.0 => Some(tr("Der Schritt darf nicht kleiner als 0 sein").into()),
            _ => None,
        }
    }

    /// The plot to store, or why not (OK stays off).
    pub fn result(&self) -> Result<Plot, String> {
        for axis in 0..2 {
            if let Some(e) = self.range_error(axis) {
                return Err(e);
            }
        }
        let mut p = self.plot.clone();
        let v = |i: usize| self.ranges[i].value().unwrap_or(0.0);
        (p.x_min, p.x_max, p.y_min, p.y_max) = (v(0), v(1), v(2), v(3));
        // Names typed here lose their spaces; the stored ones stay as they are.
        if p.x_name != self.original.x_name {
            p.x_name = p.x_name.trim().to_string();
        }
        if p.y_name != self.original.y_name {
            p.y_name = p.y_name.trim().to_string();
        }
        if !p.is_valid() {
            return Err(tr("Dieser Bereich lässt sich nicht zeichnen").into());
        }
        Ok(p)
    }

    // --- view ----------------------------------------------------------------------------

    pub fn view(&self) -> Element<'_, Msg> {
        let vals = self.plot.values();
        let tab = |label: &str, t: Tab| button(theme::btn_small(label)).padding(theme::pad()).on_press(Msg::Tab(t)).style(theme::tab(self.tab == t));
        let side: Element<'_, Msg> = match self.tab {
            Tab::Object => self.properties(&vals),
            Tab::Graphics => self.graphics(),
        };
        let right = column![
            container(Preview { plot: &self.plot, drawn: highlighted(&self.plot, self.selected_index()), paper: self.paper, cache: &self.preview, size: Size::new(RIGHT, 250.0) }).id(preview_id()),
            row![tab(tr("Eigenschaften"), Tab::Object), tab(tr("Grafik"), Tab::Graphics)].spacing(6),
            scrollable(container(side).padding(Padding { right: 12.0, ..Padding::ZERO })).style(theme::scroll_dark).height(Length::Shrink),
        ]
        .spacing(12)
        .width(Length::Fixed(RIGHT));
        // The objects scroll when they don't fit; the input line under them always shows.
        let gutter = Padding { right: 14.0, ..Padding::ZERO };
        let left = Footed {
            parts: [scrollable(container(self.algebra(&vals)).padding(gutter)).style(theme::scroll_dark).width(Length::Fill).height(Length::Shrink).into(), container(self.input_line()).padding(gutter).into()],
            gap: 10.0,
        };
        // The properties scroll too; the buttons stay.
        let body = row![left, Space::new().width(22), right];
        let result = self.result();
        // Why OK is off, when the Grafik side isn't showing it.
        let why = if self.tab == Tab::Graphics { None } else { result.as_ref().err().cloned() };
        let col = column![
            theme::heading(tr("Koordinatensystem"), 22.0),
            crate::widgets::leave_room(body, 80.0),
            container(Space::new()).height(1).width(Length::Fill).style(theme::hairline),
            row![
                button(theme::btn(tr("Koordinatensystem löschen"))).padding(theme::pad()).on_press(Msg::Delete).style(theme::ghost),
                Space::new().width(Length::Fill),
                text(why.unwrap_or_default()).size(12).color(theme::p().accent),
                Space::new().width(12),
                button(theme::btn(tr("Abbrechen"))).padding(theme::pad()).on_press(Msg::Cancel).style(theme::ghost),
                Space::new().width(8),
                button(theme::btn("OK")).padding(theme::pad()).on_press_maybe(result.is_ok().then_some(Msg::Ok)).style(theme::accent_button),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(16);
        let card = container(col).padding(24).width(Length::Fill).max_width(1040).style(theme::panel);
        // Full size, so clicks next to the panel don't reach the pages under it.
        iced::widget::mouse_area(container(card).padding(Padding { top: 56.0, right: 16.0, bottom: 16.0, left: 16.0 }).center_x(Length::Fill).height(Length::Fill)).interaction(mouse::Interaction::Idle).into()
    }

    /// The algebra view: the objects, helpers last.
    fn algebra(&self, vals: &[Result<Val, String>]) -> Element<'_, Msg> {
        let objs = &self.plot.objs;
        let mut order: Vec<usize> = (0..objs.len()).collect();
        order.sort_by(|&a, &b| objs[a].style.aux.cmp(&objs[b].style.aux).then(objs[a].order.total_cmp(&objs[b].order)));
        let known: HashSet<String> = objs.iter().map(|o| o.name()).collect();
        let mut col = column![theme::caption("Algebra")].spacing(4);
        if objs.is_empty() {
            col = col.push(container(text(tr("Noch leer. Unten eingeben, z. B. f(x) = x^2 oder A = (1|2), und Enter.")).size(13).color(theme::p().muted)).padding([6, 4]));
        }
        let mut helpers = false;
        for i in order {
            if objs[i].style.aux && !helpers {
                helpers = true;
                col = col.push(container(theme::caption(tr("Hilfsobjekte"))).padding(Padding { top: 8.0, ..Padding::ZERO }));
            }
            col = col.push(self.row(i, &vals[i], &known));
        }
        col.into()
    }

    /// The input line, what is wrong with it, and under it the command buttons with the help
    /// line, or the commands that fit what is being typed.
    fn input_line(&self) -> Element<'_, Msg> {
        let suggestions = self.suggestions();
        let field = text_input(tr("Eingabe …"), &self.input).id(input_id()).on_input(Msg::Input).on_submit(Msg::InputSubmit).font(theme::MONO).padding(9).size(14).style(theme::field);
        let mut col = column![InputLine { field: field.into(), value: &self.input, suggesting: !suggestions.is_empty() }].spacing(4);
        if let Some(e) = &self.input_error {
            col = col.push(container(text(e.clone()).size(12.5).color(theme::p().accent)).padding([2, 4]));
        }
        // While a command is being typed, the commands that fit take the place of the buttons.
        if !suggestions.is_empty() {
            return col.push(Space::new().height(2)).push(suggestion_list(&suggestions)).into();
        }
        let quick = Row::with_children(QUICK.iter().enumerate().map(|(k, &(label, _, usage))| {
            let b = button(theme::btn_small(command_name(label))).padding([3, 8]).on_press(Msg::Insert(k)).style(theme::secondary);
            tooltip(b, container(text(tr(usage)).size(12)).padding([3, 8]).max_width(320).style(theme::chip), tooltip::Position::Bottom).gap(4).into()
        }))
        .spacing(5)
        .wrap()
        .vertical_spacing(5);
        col.push(Space::new().height(4)).push(quick).push(container(text(tr(self.hint.unwrap_or(HELP))).size(12).color(theme::p().muted)).padding([2, 2])).into()
    }

    /// One object of the algebra view: its dot, its value with how it came about under it (or
    /// its definition being edited), × while selected; a slider under it; what is wrong; the
    /// question before deleting.
    fn row(&self, i: usize, v: &Result<Val, String>, known: &HashSet<String>) -> Element<'_, Msg> {
        let o = &self.plot.objs[i];
        let id = o.id.clone();
        let selected = self.selected.as_deref() == Some(o.id.as_str());
        let editing = self.editing.as_ref().filter(|e| e.id == o.id);
        let body: Element<'_, Msg> = match editing {
            Some(e) => text_input("", &e.text).id(edit_id()).on_input(Msg::EditText).on_submit(Msg::EditSubmit).font(theme::MONO).padding([6, 8]).size(14).style(theme::field).into(),
            None => {
                let mut t = text(listing(o, v, known)).font(theme::SANS).size(15).wrapping(text::Wrapping::WordOrGlyph);
                if v.is_err() {
                    t = t.color(theme::p().muted);
                }
                let mut lines = column![t].spacing(1);
                if let Some(d) = row_definition(o, v, known) {
                    lines = lines.push(text(d).font(theme::SANS).size(12).color(theme::p().muted).wrapping(text::Wrapping::WordOrGlyph));
                }
                button(lines).width(Length::Fill).padding([6, 8]).on_press(Msg::Edit(id.clone())).style(theme::row(selected)).into()
            }
        };
        let deps = self.names_of(&self.plot.dependents(&o.id));
        // Only the selected row can be deleted (fewer ×); the others keep its room.
        let x: Element<'_, Msg> = if selected {
            let tip = if deps.is_empty() { tr("Löschen").to_string() } else { trf("Löschen, mit {}", &[&and_list(&deps)]) };
            tooltip(button(text("×").size(16)).padding([2, 9]).on_press(Msg::AskRemove(id.clone())).style(theme::ghost), container(text(tip).size(12)).padding([3, 8]).style(theme::chip), tooltip::Position::Left).gap(4).into()
        } else {
            Space::new().into()
        };
        let mut cell = column![row![dot(o), body, container(x).center_x(Length::Fixed(DELETE))].spacing(6).align_y(Alignment::Center)].spacing(4);
        let indent = |e: Element<'static, Msg>| -> Element<'static, Msg> { container(e).padding(Padding { left: 36.0, right: DELETE + 8.0, ..Padding::ZERO }).into() };
        if let (Some(s), Ok(Val::Num(value)), None) = (o.style.slider, v, editing) {
            let (lo, hi) = (s.min.min(s.max), s.max.max(s.min));
            let step = if s.step > 0.0 { s.step } else { ((hi - lo) / 200.0).max(1e-9) };
            let sid = id.clone();
            let bar = slider(lo..=hi, value.clamp(lo, hi), move |v| Msg::Slider(sid.clone(), v)).step(step).style(theme::slider_style);
            let small = |v: f64| text(plot::fmt_num(v)).size(11).color(theme::p().muted);
            cell = cell.push(indent(row![small(lo), container(bar).id(slider_id(&id)).width(Length::Fill), small(hi)].spacing(8).align_y(Alignment::Center).into()));
        }
        if let Some(e) = editing.and_then(|e| e.error.clone()).or_else(|| v.as_ref().err().cloned()) {
            cell = cell.push(indent(text(e).size(12.5).color(theme::p().accent).into()));
        }
        if self.confirm.as_deref() == Some(o.id.as_str()) {
            let ask = row![
                text(if deps.len() == 1 { trf("Mit {} geht auch {}.", &[&o.name(), &and_list(&deps)]) } else { trf("Mit {} gehen auch {}.", &[&o.name(), &and_list(&deps)]) }).size(12.5).width(Length::Fill),
                button(theme::btn_small(tr("Alle löschen"))).padding([3, 9]).on_press(Msg::Remove(id.clone())).style(theme::secondary),
                button(theme::btn_small(tr("Behalten"))).padding([3, 9]).on_press(Msg::KeepIt).style(theme::ghost),
            ]
            .spacing(8)
            .align_y(Alignment::Center);
            cell = cell.push(indent(container(ask).padding([6, 10]).style(theme::group).into()));
        }
        cell.into()
    }

    /// The selected object's properties.
    fn properties(&self, vals: &[Result<Val, String>]) -> Element<'_, Msg> {
        let muted = |s: &str| text(s.to_string()).size(12.5).color(theme::p().muted);
        let Some(i) = self.selected_index() else {
            return muted(tr("Klicke links auf ein Objekt (oder in der Vorschau): hier stehen dann seine Farbe, Linie, Beschriftung …")).into();
        };
        let o = &self.plot.objs[i];
        let s = &o.style;
        let kind = vals[i].as_ref().ok().map(Val::kind);
        let section = |label: &str, content: Element<'static, Msg>| -> Element<'static, Msg> { column![theme::caption(label), content].spacing(7).into() };
        let switch = |on: bool, label: &'static str, f: fn(bool) -> Msg| toggler(on).label(label).on_toggle(f).size(16).text_size(13).spacing(8).style(theme::switch);
        let value = |s: String| text(s).size(12).font(theme::MONO).color(theme::p().muted).width(Length::Fixed(40.0));
        let mut col = column![
            row![text(o.name()).font(theme::SANS_BOLD).size(18).color(theme::p().text), text(kind_name(kind, s.slider.is_some())).size(12.5).color(theme::p().muted)]
                .spacing(10)
                .align_y(Alignment::Center)
        ]
        .spacing(16);
        col = col.push(section(tr("Farbe"), swatches(s.color)));
        if has_line(kind) {
            let w = if s.width > 0.0 { s.width } else { self.plot.width };
            let styles = row![line_style(LineStyle::Solid, s.line, s.color), line_style(LineStyle::Dashed, s.line, s.color), line_style(LineStyle::Dotted, s.line, s.color)].spacing(4);
            let width = row![slider(0.3..=6.0, w.clamp(0.3, 6.0), Msg::Width).step(0.1f32).style(theme::slider_style), value(plot::fmt_num(w as f64))].spacing(10).align_y(Alignment::Center);
            col = col.push(section(tr("Linie"), column![styles, width].spacing(8).into()));
        }
        if matches!(kind, Some(Kind::Point | Kind::Points)) {
            let size = if s.size > 0.0 { s.size } else { point_size(&self.plot) };
            let styles = row![point_style(PointStyle::Dot, s.point, s.color), point_style(PointStyle::Cross, s.point, s.color), point_style(PointStyle::Ring, s.point, s.color)].spacing(4);
            let size = row![slider(1.0..=8.0, size.clamp(1.0, 8.0), Msg::Size).step(0.1f32).style(theme::slider_style), value(plot::fmt_num(size as f64))].spacing(10).align_y(Alignment::Center);
            col = col.push(section(tr("Punkt"), column![styles, size].spacing(8).into()));
        }
        if has_fill(kind) {
            let fill = row![slider(0.0..=1.0, s.fill.clamp(0.0, 1.0), Msg::Fill).step(0.05f32).style(theme::slider_style), value(format!("{} %", (s.fill * 100.0).round()))].spacing(10).align_y(Alignment::Center);
            col = col.push(section(tr("Füllung"), fill.into()));
        }
        let labels = Row::with_children(label_modes().iter().map(|&(mode, label)| button(theme::btn_small(label)).padding([3, 8]).on_press(Msg::Label(mode)).style(theme::option(s.label == mode)).into())).spacing(5).wrap().vertical_spacing(5);
        let mut label = column![labels].spacing(8);
        if s.label == LabelMode::Caption {
            label = label.push(text_input(tr("Text der Beschriftung"), &s.caption).on_input(Msg::Caption).padding(7).size(13).style(theme::field));
        }
        col = col.push(section(tr("Beschriftung"), label.into()));
        if kind == Some(Kind::Function) {
            let mut marks = column![].spacing(8);
            for (bit, label) in mark_names() {
                marks = marks.push(toggler(s.special & bit != 0).label(label).on_toggle(move |on| Msg::Special(bit, on)).size(16).text_size(13).spacing(8).style(theme::switch));
            }
            marks = marks.push(switch(s.coords, tr("Koordinaten dazu"), Msg::Coords));
            col = col.push(section(tr("Markieren"), marks.into()));
            // Where it is defined (f(x) = x², 0 ≤ x ≤ 2); empty: open that way. The ends drag in the preview.
            let field = |k: usize, label: &'static str| {
                column![theme::caption(label), text_input(tr("offen"), &self.domain_fields[k]).on_input(move |t| Msg::DomainField(k, t)).font(theme::MONO).padding(6).size(13).width(Length::Fixed(74.0)).style(theme::field)].spacing(4)
            };
            let mut fields = row![field(0, tr("von")), field(1, tr("bis"))].spacing(10).align_y(Alignment::End);
            if self.domain_fields.iter().any(|f| !f.trim().is_empty()) {
                fields = fields.push(button(theme::btn_small(tr("Überall"))).padding([3, 9]).on_press(Msg::Everywhere).style(theme::secondary));
            }
            let mut domain = column![fields].spacing(6);
            if let Some(e) = &self.domain_error {
                domain = domain.push(text(e.clone()).size(12.5).color(theme::p().accent));
            }
            col = col.push(section(tr("Definitionsbereich"), domain.into()));
        }
        // a = 3b: a curve of its letters, or a number with a slider.
        if let Some(curve) = self.plot.curve_switch(&o.id) {
            let choice = |on: bool, label: &'static str, to: bool| button(theme::btn_small(label)).padding([3, 8]).on_press(Msg::AsCurve(to)).style(theme::option(on));
            col = col.push(section(tr("Zeichnen"), row![choice(curve, tr("als Kurve"), true), choice(!curve, tr("als Schieberegler"), false)].spacing(5).into()));
        }
        if s.slider.is_some() {
            let field = |k: usize, label: &'static str| {
                column![theme::caption(label), text_input("", &self.slider_fields[k].text).on_input(move |t| Msg::SliderField(k, t)).font(theme::MONO).padding(6).size(13).width(Length::Fixed(74.0)).style(theme::field)].spacing(4)
            };
            let mut range = column![row![field(0, tr("von")), field(1, tr("bis")), field(2, tr("Schritt"))].spacing(10)].spacing(6);
            if let Some(e) = self.slider_error() {
                range = range.push(text(e).size(12.5).color(theme::p().accent));
            }
            range = range.push(switch(true, tr("Als Schieberegler"), Msg::MakeSlider));
            col = col.push(section(tr("Schieberegler"), range.into()));
        } else if kind == Some(Kind::Number) && plain_number(o) {
            col = col.push(switch(false, tr("Als Schieberegler"), Msg::MakeSlider));
        }
        let mut last = column![switch(s.visible, tr("Sichtbar"), Msg::Shown)].spacing(8);
        // Only what drags can be held in place.
        if movable(&self.plot, i) || s.fixed {
            last = last.push(switch(s.fixed, tr("Fixiert (lässt sich nicht ziehen)"), Msg::Fixed));
        }
        col.push(last.push(switch(s.aux, tr("Hilfsobjekt"), Msg::Aux))).into()
    }

    /// The coordinate system: ranges, axes, grid, Punktfang, the look.
    fn graphics(&self) -> Element<'_, Msg> {
        let switch = |on: bool, label: &'static str, f: fn(bool) -> Msg| toggler(on).label(label).on_toggle(f).size(16).text_size(13).spacing(8).style(theme::switch);
        let section = |label: &str, content: Element<'static, Msg>| -> Element<'static, Msg> { column![theme::caption(label), content].spacing(7).into() };
        let num = |i: usize| text_input("", &self.ranges[i].text).on_input(move |s| Msg::Range(i, s)).font(theme::MONO).padding(6).size(13).width(Length::Fixed(82.0)).style(theme::field);
        let axis_row = |axis: usize| {
            row![
                text(if axis == 0 { "x" } else { "y" }).font(theme::MONO_BOLD).size(14).color(theme::p().text).width(Length::Fixed(16.0)),
                theme::caption(tr("von")),
                num(axis * 2),
                theme::caption(tr("bis")),
                num(axis * 2 + 1),
            ]
            .spacing(7)
            .align_y(Alignment::Center)
        };
        let small = |label: &str, msg: Option<Msg>| button(theme::btn_small(label)).padding(theme::pad()).on_press_maybe(msg).style(theme::secondary);
        let mut range = column![
            axis_row(0),
            axis_row(1),
            row![small(tr("y passend"), self.plot.fit_y().is_some().then_some(Msg::FitY)), small(tr("Gleiche Einheiten"), Some(Msg::EqualUnits))].spacing(8),
            switch(self.plot.pi, tr("x in π"), Msg::Pi),
        ]
        .spacing(8);
        for axis in 0..2 {
            if let Some(e) = self.range_error(axis) {
                range = range.push(text(e).size(12.5).color(theme::p().accent));
            }
        }
        // Back to the ranges the panel opened with: next to the heading, once they changed.
        let reset: Element<'_, Msg> = if self.view_changed() {
            button(theme::btn_small(tr("Ansicht zurücksetzen"))).padding([2, 8]).on_press(Msg::ResetView).style(theme::ghost).into()
        } else {
            Space::new().into()
        };
        let heading = row![theme::caption(tr("Bereich")), Space::new().width(Length::Fill), reset].align_y(Alignment::Center).height(Length::Fixed(20.0));
        let name = |value: &str, hint: &'static str, f: fn(String) -> Msg| text_input(hint, value).on_input(f).font(theme::MONO).padding(6).size(13).width(Length::Fixed(82.0)).style(theme::field);
        // What only matters with the axes (their names) or the grid (the finer one) shows with them.
        let mut axes = column![switch(self.plot.axes, tr("Achsen"), Msg::Axes)].spacing(8);
        if self.plot.axes {
            axes = axes.push(
                row![
                    theme::caption(tr("Namen")),
                    Space::new().width(4),
                    text("x").font(theme::MONO_BOLD).size(14).color(theme::p().text),
                    name(&self.plot.x_name, "x", Msg::XName),
                    text("y").font(theme::MONO_BOLD).size(14).color(theme::p().text),
                    name(&self.plot.y_name, "y", Msg::YName),
                ]
                .spacing(7)
                .align_y(Alignment::Center),
            );
        }
        let mut grid = column![switch(self.plot.grid, tr("Raster"), Msg::Grid)].spacing(8);
        if self.plot.grid {
            grid = grid.push(switch(self.plot.minor, tr("Feines Raster dazwischen"), Msg::Minor));
        }
        let snap = Row::with_children([(Snap::Auto, tr("automatisch")), (Snap::Grid, tr("am Raster")), (Snap::Off, tr("aus"))].map(|(s, label)| button(theme::btn_small(label)).padding([3, 8]).on_press(Msg::Snap(s)).style(theme::option(self.plot.snap == s)).into())).spacing(5);
        let page = column![switch(self.plot.clean, tr("Wie mit dem Lineal gezeichnet"), Msg::Clean), switch(self.plot.interactive, tr("Punkte und Schieberegler ziehen"), Msg::Interactive)].spacing(8);
        column![
            column![heading, range].spacing(7),
            section(tr("Achsen"), axes.into()),
            section(tr("Raster"), grid.into()),
            section(tr("Punktfang"), snap.into()),
            section(tr("Auf der Seite"), page.into()),
        ]
        .spacing(16)
        .into()
    }
}

/// The width of the right side (preview, properties).
const RIGHT: f32 = 360.0;

/// The room for a row's × (only the selected row shows it).
const DELETE: f32 = 32.0;

/// How an object came about, said under its value in the algebra view ("Schneide(f, c, 1)",
/// "f'(x)"): only where the value doesn't say it all (not for free objects, sliders, or objects
/// with a mistake, whose definition is the row itself).
fn row_definition(o: &Obj, v: &Result<Val, String>, known: &HashSet<String>) -> Option<String> {
    if v.is_err() || o.style.slider.is_some() {
        return None;
    }
    plot::definition(&o.def, known)
}

/// The size of a point without a size of its own, as the core draws it.
fn point_size(p: &Plot) -> f32 {
    ((p.w.min(p.h) * 0.045).clamp(5.0, 8.0) * 0.42).max(2.2)
}

/// The plot as the preview draws it: the selected object bolder (its lines 2.2 times as wide,
/// its points 1.4 times as big), so it is easy to find. The draft itself stays as it is.
fn highlighted(p: &Plot, selected: Option<usize>) -> Plot {
    let mut q = p.clone();
    if let Some(o) = selected.and_then(|i| q.objs.get_mut(i)) {
        let s = &mut o.style;
        s.width = (if s.width > 0.0 { s.width } else { p.width }) * 2.2;
        s.size = (if s.size > 0.0 { s.size } else { point_size(p) }) * 1.4;
    }
    q
}

/// The commands suggested under the input line, in a frame: each as it goes ("Schneide(f, g)")
/// with what it makes. A click takes one; Tab takes the first, which is marked.
fn suggestion_list<'a>(list: &[Suggestion]) -> Element<'a, Msg> {
    // The usages as a column: the font is a mono one (each character 0.61 of its size wide).
    let width = list.iter().map(|s| s.usage().chars().count()).max().unwrap_or(0) as f32 * 13.5 * 0.61 + 4.0;
    let rows = list.iter().enumerate().map(|(k, s)| {
        let usage = row![text(s.name).font(theme::MONO_BOLD).size(13.5).color(theme::p().text), text(format!("({})", s.args)).font(theme::MONO).size(13.5).color(theme::p().muted)].width(Length::Fixed(width));
        let mut line = row![usage, Space::new().width(14), text(tr(s.about)).size(12.5).color(theme::p().muted).width(Length::Fill)].align_y(Alignment::Center);
        if k == 0 {
            // The key that takes it, as a key cap.
            let cap = |_: &Theme| container::Style { text_color: Some(theme::p().muted), border: Border { radius: 4.0.into(), width: 1.0, color: theme::p().border }, ..Default::default() };
            line = line.push(container(text("Tab").font(theme::MONO).size(11)).padding([1, 6]).style(cap));
        }
        button(line).width(Length::Fill).padding([5, 8]).on_press(Msg::Complete(k)).style(theme::row(k == 0)).into()
    });
    container(Column::with_children(rows).spacing(2))
        .padding(4)
        .width(Length::Fill)
        .style(|_| container::Style { border: Border { radius: theme::s().card.min(10.0).into(), width: 1.0, color: theme::p().border }, ..Default::default() })
        .into()
}

/// A row's dot: filled while the object shows, hollow while hidden (like GeoGebra), in its ink
/// on a bit of paper (so dark inks show on dark designs too).
fn dot<'a>(o: &Obj) -> Element<'a, Msg> {
    let (c, shown) = (crate::pages::argb(o.style.color), o.style.visible);
    let disc = container(Space::new().width(10).height(10)).style(move |_| container::Style {
        background: shown.then(|| c.into()),
        border: Border { radius: 5.0.into(), width: if shown { 0.0 } else { 1.6 }, color: c },
        ..Default::default()
    });
    let paper = container(disc).center(18).style(|_| container::Style {
        background: Some(theme::p().paper.into()),
        border: Border { radius: 9.0.into(), width: 1.0, color: theme::p().border },
        ..Default::default()
    });
    let b = button(paper).padding(4).on_press(Msg::Visible(o.id.clone())).style(theme::ghost);
    let tip = if shown { tr("Ausblenden") } else { tr("Zeigen") };
    container(tooltip(b, container(text(tip).size(12)).padding([3, 8]).style(theme::chip), tooltip::Position::Top).gap(4)).id(dot_id(&o.id)).into()
}

/// The colours to pick: the pen colours and the construction colours (the object's own first
/// when it is none of them), on a strip of paper.
fn swatches<'a>(current: u32) -> Element<'a, Msg> {
    let mut all: Vec<u32> = INKS.iter().chain(PALETTE.iter()).copied().collect();
    if !all.contains(&current) {
        all.insert(0, current);
    }
    let swatch = |c: u32| -> Element<'a, Msg> {
        let (col, on) = (crate::pages::argb(c), c == current);
        let disc = container(Space::new().width(16).height(16)).style(move |_| container::Style {
            background: Some(col.into()),
            border: Border { radius: 8.0.into(), width: 1.0, color: theme::alpha(Color::BLACK, 0.18) },
            ..Default::default()
        });
        button(disc)
            .padding(3)
            .on_press(Msg::Color(c))
            .style(move |_, status| button::Style {
                background: None,
                border: Border { radius: 12.0.into(), width: 2.0, color: if on { theme::p().accent } else if matches!(status, button::Status::Hovered | button::Status::Pressed) { Color::from_rgb8(0xC8, 0xC8, 0xCE) } else { Color::TRANSPARENT } },
                ..Default::default()
            })
            .into()
    };
    let strip = Row::with_children(all.into_iter().map(swatch)).spacing(1).wrap().vertical_spacing(1);
    container(strip)
        .padding(5)
        .style(|_| container::Style { background: Some(theme::p().paper.into()), border: Border { radius: theme::s().card.min(10.0).into(), width: 1.0, color: theme::p().border }, ..Default::default() })
        .into()
}

/// One of the line styles to pick: a piece of the line (the chosen one in the object's ink),
/// its name over it.
fn line_style<'a>(style: LineStyle, current: LineStyle, ink: u32) -> Element<'a, Msg> {
    let (glyph, tip) = match style {
        LineStyle::Solid => (SOLID, tr("durchgezogen")),
        LineStyle::Dashed => (DASHED, tr("gestrichelt")),
        LineStyle::Dotted => (DOTTED, tr("gepunktet")),
    };
    pick(glyph, tip, style == current, ink, Msg::Line(style))
}

/// One of the point styles to pick.
fn point_style<'a>(style: PointStyle, current: PointStyle, ink: u32) -> Element<'a, Msg> {
    let (glyph, tip) = match style {
        PointStyle::Dot => (DOT, tr("Punkt")),
        PointStyle::Cross => (CROSS, tr("Kreuz")),
        PointStyle::Ring => (RING, tr("Kreis")),
    };
    pick(glyph, tip, style == current, ink, Msg::Point(style))
}

/// A small glyph button on paper (the chosen one in the object's ink, with the accent frame).
fn pick<'a>(glyph: &'static [u8], tip: &'static str, on: bool, ink: u32, msg: Msg) -> Element<'a, Msg> {
    let color = if on { crate::pages::argb(ink) } else { theme::PAPER_MUTED };
    let g = svg(svg::Handle::from_memory(glyph)).width(24).height(12).style(move |_, _| svg::Style { color: Some(color) });
    // The paper follows the frame's corners (theme::option's).
    let b = button(container(g).padding([4, 6]).style(|_| container::Style { background: Some(theme::p().paper.into()), border: Border { radius: (theme::s().card.min(14.0) - 2.0).max(0.0).into(), ..Default::default() }, ..Default::default() }))
        .padding(2)
        .on_press(msg)
        .style(theme::option(on));
    tooltip(b, container(text(tip).size(12)).padding([3, 8]).style(theme::chip), tooltip::Position::Top).gap(4).into()
}

// Pieces of each line and point style on a 24 × 12 grid.
const SOLID: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 12"><path d="M2 6H22" stroke="#000" stroke-width="2.2" stroke-linecap="round"/></svg>"##;
const DASHED: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 12"><path d="M2 6H22" stroke="#000" stroke-width="2.2" stroke-linecap="round" stroke-dasharray="4.5 4"/></svg>"##;
const DOTTED: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 12"><path d="M2.5 6H22" stroke="#000" stroke-width="2.4" stroke-linecap="round" stroke-dasharray="0.1 4.4"/></svg>"##;
const DOT: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 12"><circle cx="12" cy="6" r="3.6" fill="#000"/></svg>"##;
const CROSS: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 12"><path d="M8.5 2.5L15.5 9.5M15.5 2.5L8.5 9.5" stroke="#000" stroke-width="2" stroke-linecap="round"/></svg>"##;
const RING: &[u8] = br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 12"><circle cx="12" cy="6" r="3.6" fill="none" stroke="#000" stroke-width="1.8"/></svg>"##;

// --- the input line ------------------------------------------------------------------------

/// The input line's text field with what the suggestions need: after every key or click it
/// takes, it says where its cursor is ([`Msg::Cursor`]: the suggestions follow the word there);
/// while suggestions show, Tab takes the first and Esc only hides them (the field keeps its
/// cursor; the panel hears that Esc through the [`crate::widgets::on_escape`] around it).
struct InputLine<'a> {
    field: Element<'a, Msg>,
    /// What the field says.
    value: &'a str,
    suggesting: bool,
}

/// The state iced keeps for a text field.
type FieldState = iced::widget::text_input::State<<iced::Renderer as iced::advanced::text::Renderer>::Paragraph>;

/// What the input line's field was typed to since the panel last drew it: (what it said then,
/// what it says now). Several events can come before the panel draws again.
#[derive(Default)]
struct Typed(Option<(String, String)>);

impl<'a> Widget<Msg, Theme, iced::Renderer> for InputLine<'a> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Typed>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Typed::default())
    }

    fn children(&self) -> Vec<widget::Tree> {
        vec![widget::Tree::new(&self.field)]
    }

    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(std::slice::from_ref(&self.field));
    }

    fn size(&self) -> Size<Length> {
        self.field.as_widget().size()
    }

    fn layout(&mut self, tree: &mut widget::Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        self.field.as_widget_mut().layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(&mut self, tree: &mut widget::Tree, layout: Layout<'_>, renderer: &iced::Renderer, operation: &mut dyn widget::Operation) {
        self.field.as_widget_mut().operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn update(&mut self, tree: &mut widget::Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &iced::Renderer, clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, Msg>, viewport: &Rectangle) {
        let focused = tree.children[0].state.downcast_ref::<FieldState>().is_focused();
        if focused
            && self.suggesting
            && !shell.is_event_captured()
            && let Event::Keyboard(keyboard::Event::KeyPressed { key: keyboard::Key::Named(key @ (Named::Tab | Named::Escape)), modifiers, .. }) = event
            && (*key == Named::Escape || modifiers.is_empty())
        {
            if *key == Named::Tab {
                shell.publish(Msg::Complete(0));
            }
            shell.capture_event();
            return;
        }
        // What the field publishes (to know what it says after this event).
        let mut published = Vec::new();
        let mut inner = Shell::new(&mut published);
        self.field.as_widget_mut().update(&mut tree.children[0], event, layout, cursor, renderer, clipboard, &mut inner, viewport);
        let took = inner.is_event_captured();
        let typed: RefCell<Option<String>> = RefCell::new(None);
        shell.merge(inner, |m| {
            if let Msg::Input(s) = &m {
                *typed.borrow_mut() = Some(s.clone());
            }
            m
        });
        let since = tree.state.downcast_mut::<Typed>();
        if let Some(t) = typed.into_inner() {
            since.0 = Some((self.value.to_string(), t));
        }
        let now = match &since.0 {
            Some((then, now)) if then == self.value => now.as_str(),
            _ => self.value,
        };
        let field = tree.children[0].state.downcast_ref::<FieldState>();
        if took && field.is_focused() {
            let at = match field.cursor().state(&iced::widget::text_input::Value::new(now)) {
                iced::widget::text_input::cursor::State::Index(i) => i,
                iced::widget::text_input::cursor::State::Selection { end, .. } => end,
            };
            shell.publish(Msg::Cursor(at));
        }
    }

    fn mouse_interaction(&self, tree: &widget::Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &iced::Renderer) -> mouse::Interaction {
        self.field.as_widget().mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }

    fn draw(&self, tree: &widget::Tree, renderer: &mut iced::Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle) {
        self.field.as_widget().draw(&tree.children[0], renderer, theme, style, layout, cursor, viewport);
    }

    fn overlay<'b>(&'b mut self, tree: &'b mut widget::Tree, layout: Layout<'b>, renderer: &iced::Renderer, viewport: &Rectangle, translation: Vector) -> Option<iced::advanced::overlay::Element<'b, Msg, Theme, iced::Renderer>> {
        self.field.as_widget_mut().overlay(&mut tree.children[0], layout, renderer, viewport, translation)
    }
}

impl<'a> From<InputLine<'a>> for Element<'a, Msg> {
    fn from(w: InputLine<'a>) -> Self {
        Element::new(w)
    }
}

/// Two parts one above the other, where the lower one (the input line and what is under it)
/// always shows: it is laid out first, and the upper one (the objects, scrolling) gets the
/// height that is left.
struct Footed<'a> {
    parts: [Element<'a, Msg>; 2],
    gap: f32,
}

impl<'a> Widget<Msg, Theme, iced::Renderer> for Footed<'a> {
    fn children(&self) -> Vec<widget::Tree> {
        self.parts.iter().map(widget::Tree::new).collect()
    }

    fn diff(&self, tree: &mut widget::Tree) {
        tree.diff_children(&self.parts);
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Shrink)
    }

    fn layout(&mut self, tree: &mut widget::Tree, renderer: &iced::Renderer, limits: &layout::Limits) -> layout::Node {
        let max = limits.max();
        let [top, foot] = &mut self.parts;
        let foot = foot.as_widget_mut().layout(&mut tree.children[1], renderer, &layout::Limits::new(Size::ZERO, max));
        let room = (max.height - foot.size().height - self.gap).max(0.0);
        let top = top.as_widget_mut().layout(&mut tree.children[0], renderer, &layout::Limits::new(Size::ZERO, Size::new(max.width, room)));
        let (t, f) = (top.size(), foot.size());
        let size = limits.resolve(Length::Fill, Length::Shrink, Size::new(t.width.max(f.width), t.height + self.gap + f.height));
        layout::Node::with_children(size, vec![top, foot.move_to(Point::new(0.0, t.height + self.gap))])
    }

    fn operate(&mut self, tree: &mut widget::Tree, layout: Layout<'_>, renderer: &iced::Renderer, operation: &mut dyn widget::Operation) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            for ((part, tree), layout) in self.parts.iter_mut().zip(&mut tree.children).zip(layout.children()) {
                part.as_widget_mut().operate(tree, layout, renderer, operation);
            }
        });
    }

    fn update(&mut self, tree: &mut widget::Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, renderer: &iced::Renderer, clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, Msg>, viewport: &Rectangle) {
        for ((part, tree), layout) in self.parts.iter_mut().zip(&mut tree.children).zip(layout.children()) {
            part.as_widget_mut().update(tree, event, layout, cursor, renderer, clipboard, shell, viewport);
        }
    }

    fn mouse_interaction(&self, tree: &widget::Tree, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle, renderer: &iced::Renderer) -> mouse::Interaction {
        self.parts.iter().zip(&tree.children).zip(layout.children()).map(|((part, tree), layout)| part.as_widget().mouse_interaction(tree, layout, cursor, viewport, renderer)).max().unwrap_or_default()
    }

    fn draw(&self, tree: &widget::Tree, renderer: &mut iced::Renderer, theme: &Theme, style: &renderer::Style, layout: Layout<'_>, cursor: mouse::Cursor, viewport: &Rectangle) {
        for ((part, tree), layout) in self.parts.iter().zip(&tree.children).zip(layout.children()) {
            part.as_widget().draw(tree, renderer, theme, style, layout, cursor, viewport);
        }
    }

    fn overlay<'b>(&'b mut self, tree: &'b mut widget::Tree, layout: Layout<'b>, renderer: &iced::Renderer, viewport: &Rectangle, translation: Vector) -> Option<iced::advanced::overlay::Element<'b, Msg, Theme, iced::Renderer>> {
        iced::advanced::overlay::from_children(&mut self.parts, tree, layout, renderer, viewport, translation)
    }
}

impl<'a> From<Footed<'a>> for Element<'a, Msg> {
    fn from(w: Footed<'a>) -> Self {
        Element::new(w)
    }
}

// --- the preview ---------------------------------------------------------------------------

/// The live preview: the plot on a piece of its paper, scaled to fit. Drawn as paths (both
/// renderers draw those; the software one skips meshes), redrawn when the plot changes. A click
/// on an object selects it; points and sliders drag.
struct Preview<'a> {
    /// The draft (where things are, what a click hits).
    plot: &'a Plot,
    /// What is drawn: the draft with the selected object bolder ([`highlighted`]).
    drawn: Plot,
    paper: (Background, f32),
    cache: &'a RefCell<Option<(Plot, canvas::Cache)>>,
    size: Size,
}

/// How page points map into the preview's inside of `size` (its bounds less 1 px each side):
/// page point (x, y) at o + (x, y) · k (None: nothing to show). The ink, scaled to fit.
pub fn preview_fit(plot: &Plot, size: Size) -> Option<(Vector, f32)> {
    let l = look(plot);
    let r = look_bounds(plot, &l);
    if r.width() <= 0.0 || r.height() <= 0.0 {
        return None;
    }
    let pad = 15.0;
    let k = ((size.width - 2.0 * pad) / r.width()).min((size.height - 2.0 * pad) / r.height()).max(0.01);
    // The ink's middle in the preview's middle.
    Some((Vector::new(size.width / 2.0 - (r.x0 + r.x1) / 2.0 * k, size.height / 2.0 - (r.y0 + r.y1) / 2.0 * k), k))
}

/// What is being dragged in the preview.
#[derive(Default)]
enum Dragging {
    #[default]
    Nothing,
    /// A point or a slider.
    Object(String),
    /// An end of a function's domain (0 from, 1 to).
    End(String, u8),
    /// An axis, stretched alone.
    /// An axis, stretched alone; with how the preview mapped page points when it was grabbed
    /// (the preview fits the ink again as it changes; the drag keeps to the first mapping).
    Axis(plot::Axis, Vector, f32),
}

impl<'a> Preview<'a> {
    /// How page points map into a preview of `size`: page point (x, y) at o + (x, y) · k (None:
    /// nothing to show).
    fn fit(&self, size: Size) -> Option<(Vector, f32)> {
        preview_fit(self.plot, size)
    }

    /// The page point under a screen point, and the scale.
    fn page_at(&self, inner: Rectangle, p: Point) -> Option<(f32, f32, f32)> {
        let (o, k) = self.fit(inner.size())?;
        Some(((p.x - inner.x - o.x) / k, (p.y - inner.y - o.y) / k, k))
    }

    /// The object under a screen point.
    fn object_at(&self, inner: Rectangle, p: Point) -> Option<usize> {
        let (x, y, k) = self.page_at(inner, p)?;
        plot::hit(self.plot, x, y, 7.0 / k)
    }

    fn paint(&self, frame: &mut canvas::Frame, size: Size) {
        let Some((o, k)) = self.fit(size) else { return };
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
        for (brush, pts) in look(&self.drawn).iter() {
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

impl<'a> Widget<Msg, Theme, iced::Renderer> for Preview<'a> {
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<Dragging>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Dragging::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fixed(self.size.width), Length::Fixed(self.size.height))
    }

    fn layout(&mut self, _tree: &mut widget::Tree, _renderer: &iced::Renderer, _limits: &layout::Limits) -> layout::Node {
        layout::Node::new(self.size)
    }

    fn update(&mut self, tree: &mut widget::Tree, event: &Event, layout: Layout<'_>, cursor: mouse::Cursor, _renderer: &iced::Renderer, _clipboard: &mut dyn Clipboard, shell: &mut Shell<'_, Msg>, _viewport: &Rectangle) {
        let inner = layout.bounds().shrink(1.0);
        let state = tree.state.downcast_mut::<Dragging>();
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let Some(p) = cursor.position_over(layout.bounds()) else { return };
                let Some((x, y, k)) = self.page_at(inner, p) else { return };
                // An end of a domain first (it lies on its curve), then objects, then the axes.
                if let Some((id, side)) = self.plot.domain_end_at(x, y, 7.0 / k) {
                    shell.publish(Msg::Pick(id.clone()));
                    *state = Dragging::End(id, side);
                } else if let Some(i) = self.object_at(inner, p) {
                    let id = self.plot.objs[i].id.clone();
                    shell.publish(Msg::Pick(id.clone()));
                    if draggable(self.plot, i) {
                        *state = Dragging::Object(id);
                    }
                } else if let Some(axis) = self.plot.axis_at(x, y, 6.0 / k) {
                    shell.publish(Msg::AxisGrab(axis, if axis == plot::Axis::X { x } else { y }));
                    *state = Dragging::Axis(axis, self.fit(inner.size()).map_or(Vector::ZERO, |f| f.0), k);
                } else {
                    return;
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let Some((x, y, _)) = self.page_at(inner, *position) else { return };
                match &*state {
                    Dragging::Object(id) => shell.publish(Msg::Drag(id.clone(), x, y)),
                    Dragging::End(id, side) => shell.publish(Msg::DragEnd(id.clone(), *side, x, y)),
                    Dragging::Axis(axis, o, k) => {
                        let (ax, ay) = ((position.x - inner.x - o.x) / k, (position.y - inner.y - o.y) / k);
                        shell.publish(Msg::AxisTo(if *axis == plot::Axis::X { ax } else { ay }))
                    }
                    Dragging::Nothing => return,
                }
                shell.capture_event();
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                match std::mem::take(state) {
                    Dragging::Nothing => return,
                    Dragging::Axis(..) => shell.publish(Msg::AxisLet),
                    _ => {}
                }
                shell.capture_event();
            }
            _ => {}
        }
    }

    fn mouse_interaction(&self, tree: &widget::Tree, layout: Layout<'_>, cursor: mouse::Cursor, _viewport: &Rectangle, _renderer: &iced::Renderer) -> mouse::Interaction {
        let axis_look = |a: plot::Axis| if a == plot::Axis::X { mouse::Interaction::ResizingHorizontally } else { mouse::Interaction::ResizingVertically };
        match tree.state.downcast_ref::<Dragging>() {
            Dragging::Nothing => {}
            Dragging::Axis(a, ..) => return axis_look(*a),
            _ => return mouse::Interaction::Grabbing,
        }
        let inner = layout.bounds().shrink(1.0);
        let Some(p) = cursor.position_over(layout.bounds()) else { return mouse::Interaction::None };
        let page = self.page_at(inner, p);
        if page.is_some_and(|(x, y, k)| self.plot.domain_end_at(x, y, 7.0 / k).is_some()) {
            return mouse::Interaction::ResizingHorizontally;
        }
        match self.object_at(inner, p) {
            Some(i) if draggable(self.plot, i) => mouse::Interaction::Grab,
            Some(_) => mouse::Interaction::Pointer,
            None => match page.and_then(|(x, y, k)| self.plot.axis_at(x, y, 6.0 / k)) {
                Some(a) => axis_look(a),
                None => mouse::Interaction::None,
            },
        }
    }

    fn draw(&self, _tree: &widget::Tree, renderer: &mut iced::Renderer, _theme: &Theme, _style: &renderer::Style, layout: Layout<'_>, _cursor: mouse::Cursor, _viewport: &Rectangle) {
        let b = layout.bounds();
        renderer.fill_quad(
            renderer::Quad { bounds: b, border: Border { color: theme::p().border, width: 1.0, radius: theme::s().card.min(6.0).into() }, ..Default::default() },
            theme::p().paper,
        );
        if !matches!(self.cache.borrow().as_ref(), Some((p, _)) if *p == self.drawn) {
            *self.cache.borrow_mut() = Some((self.drawn.clone(), canvas::Cache::new()));
        }
        let cache = self.cache.borrow();
        let Some((_, cache)) = cache.as_ref() else { return };
        let inner = b.shrink(1.0);
        let geometry = cache.draw(renderer, inner.size(), |frame| self.paint(frame, inner.size()));
        renderer.with_layer(inner, |r| r.with_translation(Vector::new(inner.x, inner.y), |r| r.draw_geometry(geometry)));
    }
}

impl<'a> From<Preview<'a>> for Element<'a, Msg> {
    fn from(p: Preview<'a>) -> Self {
        Element::new(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paper() -> (Background, f32) {
        (Background::Blank, 22.0)
    }

    fn empty() -> Plot {
        let mut p = fresh(300.0, 400.0, 595.0, 842.0, Background::Blank, 22.0);
        p.id = "p".into();
        p
    }

    /// Types `s` into the input line and presses Enter.
    fn enter(e: &mut Editor, s: &str) {
        e.update(Msg::Input(s.into()));
        e.update(Msg::InputSubmit);
    }

    fn id_of(e: &Editor, name: &str) -> String {
        e.plot.objs.iter().find(|o| o.name() == name).unwrap_or_else(|| panic!("no {name}")).id.clone()
    }

    fn names(e: &Editor) -> Vec<String> {
        e.plot.objs.iter().map(|o| o.name()).collect()
    }

    /// The algebra view's rows, as listed.
    fn rows(p: &Plot) -> Vec<String> {
        let known: HashSet<String> = p.objs.iter().map(|o| o.name()).collect();
        p.objs.iter().zip(p.values()).map(|(o, v)| listing(o, &v, &known)).collect()
    }

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
    fn typing_definitions_and_commands_adds_named_objects_listed_like_geogebra() {
        let mut e = Editor::new("s", empty(), paper());
        assert_eq!(e.tab, Tab::Graphics, "nothing selected: the look of the system");
        enter(&mut e, "A = (1,2)");
        enter(&mut e, "x^2");
        enter(&mut e, "2x + 1");
        enter(&mut e, "Schneide(f, g)");
        enter(&mut e, "y = 0,5x - 1");
        enter(&mut e, "c: x^2 + y^2 = 4");
        enter(&mut e, "a = 3");
        assert_eq!(names(&e), vec!["A", "f", "g", "B", "C", "h", "c", "a"], "two intersections: two points");
        assert!(e.plot.errors().iter().all(|r| r.is_none()), "{:?}", e.plot.errors());
        assert_eq!(e.plot.objs[1].def, "f = x^2", "named like in GeoGebra");
        assert_eq!(rows(&e.plot), vec!["A = (1 | 2)", "f(x) = x²", "g(x) = 2x + 1", "B = (−0,41 | 0,17)", "C = (2,41 | 5,83)", "h: y = 0,5x − 1", "c: x² + y² = 4", "a = 3"]);
        assert!(e.plot.objs[7].style.slider.is_some(), "a plain number is a slider");
        // The input line is empty again, the new object selected, its properties showing.
        assert_eq!((e.input.as_str(), e.input_error.as_ref()), ("", None));
        assert_eq!(e.selected.as_deref(), Some(e.plot.objs[7].id.as_str()));
        assert_eq!(e.tab, Tab::Object);
        // Enter on an empty line: nothing; after a new object the cursor stays in the line.
        assert_eq!(e.update(Msg::InputSubmit), Reply::None);
        assert_eq!(e.plot.objs.len(), 8);
        e.update(Msg::Input("b = 1".into()));
        assert_eq!(e.update(Msg::InputSubmit), Reply::Focus(input_id(), None));
        e.update(Msg::AskRemove(e.plot.objs[8].id.clone()));
        // A command button puts the command into the line with the cursor between its brackets
        // and says how it goes.
        e.update(Msg::Input("S = ".into()));
        assert_eq!(e.update(Msg::Insert(3)), Reply::Focus(input_id(), Some(13)));
        assert_eq!(e.input, "S = Schneide()");
        assert_eq!(e.hint, Some(QUICK[3].2));
        e.update(Msg::Input("S = Schneide(h, c)".into()));
        e.update(Msg::InputSubmit);
        assert_eq!(e.plot.objs.last().unwrap().def, "S_2 = Schneide(h, c, 2)", "two intersections: S_1 and S_2");
        assert_eq!(e.hint, None, "the help line is back to the examples");
        // A function of t, a text, an angle are listed as GeoGebra lists them.
        let p = empty().add("k(t) = t^2").unwrap().add("Text(\"Hallo\")").unwrap().add("q: y = x^3").unwrap();
        assert_eq!(rows(&p), vec!["k(t) = t²", "text1 = „Hallo“", "q: y = x³"]);
        assert_eq!(rows(&empty().add("k(t) = exp(t) + t^2").unwrap()), vec!["k(t) = exp(t) + t²"], "t, not x, also next to words like exp");
    }

    #[test]
    fn a_mistake_is_said_under_the_line_and_nothing_is_added() {
        let mut e = Editor::new("s", empty(), paper());
        enter(&mut e, "A = (1, 2)");
        enter(&mut e, "Gerade(A)");
        assert_eq!(e.input_error.as_deref(), Some("So: Gerade(A, B) oder Gerade(A, g) (parallel)"));
        assert_eq!(e.input, "Gerade(A)", "what was typed stays, to fix it");
        assert_eq!(e.plot.objs.len(), 1);
        enter(&mut e, "A = (0, 0)");
        assert_eq!(e.input_error.as_deref(), Some("Den Namen A gibt es schon"));
        // Typing again: the message goes.
        e.update(Msg::Input("B = (0, 0)".into()));
        assert_eq!(e.input_error, None);
    }

    #[test]
    fn a_row_edits_its_definition() {
        let mut e = Editor::new("s", empty(), paper());
        enter(&mut e, "A = (1, 2)");
        enter(&mut e, "x^2");
        enter(&mut e, "T = Punkt(f, 1)");
        enter(&mut e, "Tangente(T, f)");
        let f = id_of(&e, "f");
        // A click: selected, its definition in the field, the cursor in it.
        assert_eq!(e.update(Msg::Edit(f.clone())), Reply::Focus(edit_id(), None));
        assert_eq!(e.editing, Some(RowEdit { id: f.clone(), text: "f = x^2".into(), error: None }));
        assert_eq!(e.selected.as_deref(), Some(f.as_str()));
        // A mistake: said, nothing changes, the row stays open.
        e.update(Msg::EditText("x^3 +".into()));
        e.update(Msg::EditSubmit);
        assert_eq!(e.editing.as_ref().and_then(|r| r.error.as_deref()), Some("Da fehlt noch etwas am Ende"));
        assert_eq!(e.plot.objs[1].def, "f = x^2");
        // Without a name it keeps its own; the tangent follows.
        e.update(Msg::EditText("x^3".into()));
        e.update(Msg::EditSubmit);
        assert_eq!(e.editing, None);
        assert_eq!(e.plot.objs[1].def, "f = x^3");
        assert_eq!(rows(&e.plot)[3], "g: y = 3x − 2");
        // An equation without a name gets it as a label (the core alone would make it wrong).
        let g = id_of(&e, "g");
        e.update(Msg::Edit(g.clone()));
        e.update(Msg::EditText("y = 2x".into()));
        e.update(Msg::EditSubmit);
        assert_eq!(e.plot.objs[3].def, "g: y = 2x");
        // A redefinition that would break what depends on it is refused, like in GeoGebra.
        let a = id_of(&e, "f");
        e.update(Msg::Edit(a.clone()));
        e.update(Msg::EditText("f = (1, 2)".into()));
        e.update(Msg::EditSubmit);
        assert!(e.editing.as_ref().and_then(|r| r.error.clone()).is_some_and(|m| m.starts_with("Dann stimmt T nicht mehr")), "{:?}", e.editing);
        // So is a new name while others use the old one; a free one without dependents is fine.
        e.update(Msg::EditText("k(x) = x^3".into()));
        e.update(Msg::EditSubmit);
        assert_eq!(e.editing.as_ref().and_then(|r| r.error.as_deref()), Some("Umbenennen geht nicht: T hängt von f ab"));
        e.update(Msg::Escape);
        assert_eq!(e.editing, None, "Esc leaves the row");
        let a = id_of(&e, "A");
        e.update(Msg::Edit(a.clone()));
        e.update(Msg::EditText("P = (1, 2)".into()));
        e.update(Msg::EditSubmit);
        assert_eq!(e.plot.objs[0].def, "P = (1, 2)");
        // Clicking another row takes what was typed in this one.
        e.update(Msg::Edit(a.clone()));
        e.update(Msg::EditText("P = (3, 2)".into()));
        e.update(Msg::Edit(g.clone()));
        assert_eq!(e.plot.objs[0].def, "P = (3, 2)");
        assert_eq!(e.editing.as_ref().map(|r| r.id.as_str()), Some(g.as_str()));
    }

    #[test]
    fn deleting_takes_what_depends_on_it_after_asking() {
        let mut e = Editor::new("s", empty(), paper());
        for s in ["A = (1, 2)", "B = (4, 6)", "Gerade(A, B)", "x^2", "Schneide(f, g)", "P = (0, 3)"] {
            enter(&mut e, s);
        }
        assert_eq!(names(&e), vec!["A", "B", "f", "g", "C", "D", "P"]);
        let a = id_of(&e, "A");
        assert_eq!(e.names_of(&e.plot.dependents(&a)), vec!["f", "C", "D"]);
        // Others depend on A: asked first.
        e.update(Msg::AskRemove(a.clone()));
        assert_eq!(e.confirm.as_deref(), Some(a.as_str()));
        assert_eq!(e.plot.objs.len(), 7);
        e.update(Msg::KeepIt);
        assert_eq!((e.confirm.as_ref(), e.plot.objs.len()), (None, 7));
        e.update(Msg::AskRemove(a.clone()));
        e.update(Msg::Escape);
        assert_eq!((e.confirm.as_ref(), e.plot.objs.len()), (None, 7), "Esc says no too");
        e.update(Msg::Edit(a.clone()));
        e.update(Msg::AskRemove(a.clone()));
        e.update(Msg::Remove(a.clone()));
        assert_eq!(names(&e), vec!["B", "g", "P"]);
        assert_eq!((e.selected.as_ref(), e.editing.as_ref()), (None, None), "it was selected and open");
        // Nothing depends on P: gone at once.
        e.update(Msg::AskRemove(id_of(&e, "P")));
        assert_eq!(names(&e), vec!["B", "g"]);
        assert_eq!(e.confirm, None);
    }

    #[test]
    fn sliders_drag_and_dots_show_or_hide() {
        let mut e = Editor::new("s", empty(), paper());
        enter(&mut e, "a = Schieberegler(0, 10, 0.5)");
        enter(&mut e, "f(x) = a x^2");
        let a = id_of(&e, "a");
        e.update(Msg::Slider(a.clone(), 3.37));
        assert_eq!(e.plot.objs[0].def, "a = 3.5", "on its steps");
        assert_eq!(rows(&e.plot)[1], "f(x) = 3,5x²", "the function follows");
        // The dot.
        e.update(Msg::Visible(a.clone()));
        assert!(!e.plot.objs[0].style.visible);
        e.update(Msg::Visible(a.clone()));
        assert!(e.plot.objs[0].style.visible);
        // Its range in the properties: the value stays inside.
        e.update(Msg::Edit(a.clone()));
        e.update(Msg::Escape);
        assert_eq!(e.slider_fields.iter().map(|f| f.text.as_str()).collect::<Vec<_>>(), vec!["0", "10", "0,5"]);
        e.update(Msg::SliderField(1, "2".into()));
        assert_eq!(e.plot.objs[0].style.slider, Some(Slider { min: 0.0, max: 2.0, step: 0.5 }));
        assert_eq!(e.plot.objs[0].def, "a = 2");
        e.update(Msg::SliderField(1, "-1".into()));
        assert_eq!(e.slider_error().as_deref(), Some("bis muss größer sein als von"));
        assert_eq!(e.plot.objs[0].style.slider.unwrap().max, 2.0, "the last range that worked");
        e.update(Msg::SliderField(1, "20".into()));
        e.update(Msg::SliderField(2, "1".into()));
        assert_eq!(e.plot.objs[0].style.slider, Some(Slider { min: 0.0, max: 20.0, step: 1.0 }));
        // Dragged in the preview along its track.
        let i = 0;
        let (x0, x1, y) = plot::slider_track(&e.plot, i).expect("a visible slider has a track");
        e.update(Msg::Drag(a.clone(), x0 + (x1 - x0) * 0.75, y));
        assert_eq!(e.plot.objs[0].def, "a = 15");
        // A number typed as a formula can't become a slider; a plain one can.
        enter(&mut e, "b = 2");
        enter(&mut e, "n = a + 1");
        let b = id_of(&e, "b");
        e.update(Msg::Pick(b.clone()));
        e.update(Msg::MakeSlider(false));
        assert_eq!(e.plot.objs[2].style.slider, None);
        e.update(Msg::MakeSlider(true));
        assert_eq!(e.plot.objs[2].style.slider, Some(Slider { min: -5.0, max: 5.0, step: 0.1 }));
        e.update(Msg::Pick(id_of(&e, "n")));
        e.update(Msg::MakeSlider(true));
        assert_eq!(e.plot.objs[3].style.slider, None);
    }

    #[test]
    fn points_drag_in_the_preview() {
        let mut e = Editor::new("s", empty(), paper());
        enter(&mut e, "A = (1, 2)");
        enter(&mut e, "x^2");
        enter(&mut e, "P = Punkt(f, 1)");
        e.plot.snap = Snap::Off;
        let (x, y) = e.plot.to_page(2.5, -1.25);
        e.update(Msg::Drag(id_of(&e, "A"), x, y));
        assert_eq!(e.plot.objs[0].def, "A = (2.5, -1.25)");
        let (x, y) = e.plot.to_page(-2.0, 0.0);
        e.update(Msg::Drag(id_of(&e, "P"), x, y));
        assert_eq!(e.plot.values()[2], Ok(Val::Point((-2.0, 4.0))), "along f");
        // Fixed: it stays; a function doesn't drag.
        e.update(Msg::Pick(id_of(&e, "A")));
        e.update(Msg::Fixed(true));
        e.update(Msg::Drag(id_of(&e, "A"), 0.0, 0.0));
        assert_eq!(e.plot.objs[0].def, "A = (2.5, -1.25)");
        assert!(!draggable(&e.plot, 0) && !draggable(&e.plot, 1) && draggable(&e.plot, 2));
    }

    #[test]
    fn properties_change_the_selected_objects_style() {
        let mut e = Editor::new("s", empty(), paper());
        enter(&mut e, "f(x) = x^2 - 1");
        enter(&mut e, "A = (1, 2)");
        enter(&mut e, "B = (3, 0)");
        enter(&mut e, "Vieleck(A, B, (0, 0))");
        let f = id_of(&e, "f");
        e.update(Msg::Pick(f.clone()));
        assert_eq!(e.tab, Tab::Object);
        e.update(Msg::Color(PALETTE[3]));
        e.update(Msg::Width(2.04));
        e.update(Msg::Line(LineStyle::Dashed));
        e.update(Msg::Label(LabelMode::Caption));
        e.update(Msg::Caption("Parabel".into()));
        e.update(Msg::Special(MARK_ZEROS, true));
        e.update(Msg::Special(MARK_EXTREMA, true));
        e.update(Msg::Special(MARK_ZEROS, false));
        e.update(Msg::Coords(true));
        e.update(Msg::Aux(true));
        e.update(Msg::Shown(false));
        let s = &e.plot.objs[0].style;
        assert_eq!((s.color, s.width, s.line, s.label, s.caption.as_str()), (PALETTE[3], 2.0, LineStyle::Dashed, LabelMode::Caption, "Parabel"));
        assert_eq!((s.special, s.coords, s.aux, s.visible), (MARK_EXTREMA, true, true, false));
        // A point: its look; nothing else changed.
        let a = id_of(&e, "A");
        e.update(Msg::Pick(a.clone()));
        e.update(Msg::Point(PointStyle::Cross));
        e.update(Msg::Size(4.0));
        e.update(Msg::Fixed(true));
        let s = &e.plot.objs[1].style;
        assert_eq!((s.point, s.size, s.fixed, s.label), (PointStyle::Cross, 4.0, true, LabelMode::Name));
        assert_eq!(e.plot.objs[2].style, empty().add("B = (3, 0)").unwrap().objs[0].style, "B as it came");
        // A polygon: its filling.
        e.update(Msg::Pick(e.plot.objs[3].id.clone()));
        assert_eq!(e.plot.objs[3].style.fill, 0.35, "filled from the start");
        e.update(Msg::Fill(0.8));
        assert_eq!(e.plot.objs[3].style.fill, 0.8);
        // Nothing selected: the properties do nothing.
        let before = e.plot.clone();
        e.select(None);
        e.update(Msg::Color(INKS[0]));
        assert_eq!(e.plot, before);
    }

    #[test]
    fn the_look_of_the_system() {
        let mut e = Editor::new("s", empty().add("x^2").unwrap(), paper());
        e.update(Msg::Range(1, "1".into()));
        e.update(Msg::Range(1, "10".into()));
        e.update(Msg::Range(0, "".into()));
        assert_eq!((e.plot.x_min, e.plot.x_max), (-6.0, 10.0), "a half-typed range keeps the last one that worked");
        assert!(e.range_error(0).is_some() && e.result().is_err());
        assert_eq!(e.update(Msg::Ok), Reply::None, "OK waits for a range that works");
        e.update(Msg::Range(0, "-2,5".into()));
        e.update(Msg::FitY);
        assert!(e.plot.y_min <= 0.0 && e.plot.y_max >= 60.0, "{:?}", (e.plot.y_min, e.plot.y_max));
        e.update(Msg::EqualUnits);
        let per_x = (e.plot.x_max - e.plot.x_min) / e.plot.w as f64;
        assert!(((e.plot.y_max - e.plot.y_min) / e.plot.h as f64 - per_x).abs() < 1e-9);
        e.update(Msg::Pi(true));
        let pi = std::f64::consts::PI;
        assert_eq!((e.plot.x_min, e.plot.x_max), (-pi, 3.5 * pi), "widened to whole π/2");
        assert_eq!((e.ranges[0].text.as_str(), e.ranges[1].text.as_str()), ("-π", "7π/2"));
        for m in [Msg::Axes(false), Msg::Grid(false), Msg::Minor(true), Msg::Snap(Snap::Grid), Msg::Clean(true), Msg::Interactive(true), Msg::XName(" t ".into()), Msg::YName("U".into())] {
            e.update(m);
        }
        let Reply::Apply(p) = e.update(Msg::Ok) else { panic!("OK: {:?}", e.result()) };
        assert_eq!((p.axes, p.grid, p.minor, p.snap, p.clean, p.interactive, p.pi), (false, false, true, Snap::Grid, true, true, true));
        assert_eq!((p.x_name.as_str(), p.y_name.as_str()), ("t", "U"));
        assert_eq!((p.x_min, p.x_max), (-pi, 3.5 * pi), "exactly, not as written");
    }

    #[test]
    fn ok_stores_the_draft_with_what_is_still_typed_and_abbrechen_forgets_it() {
        let mut e = Editor::new("s", empty(), paper());
        enter(&mut e, "A = (1, 2)");
        // Typed, not entered yet: OK takes it.
        e.update(Msg::Input("x^2".into()));
        let Reply::Apply(p) = e.update(Msg::Ok) else { panic!() };
        assert_eq!(p.objs.iter().map(|o| o.def.as_str()).collect::<Vec<_>>(), vec!["A = (1, 2)", "f = x^2"]);
        // A mistake still typed: OK says so and waits; also in a row.
        e.update(Msg::Input("Schneide(f)".into()));
        assert_eq!(e.update(Msg::Ok), Reply::None);
        assert_eq!(e.input_error.as_deref(), Some("So: Schneide(f, g)"));
        e.update(Msg::Input(String::new()));
        e.update(Msg::Edit(id_of(&e, "f")));
        e.update(Msg::EditText("x^".into()));
        assert_eq!(e.update(Msg::Ok), Reply::None);
        assert!(e.editing.as_ref().is_some_and(|r| r.error.is_some()));
        e.update(Msg::EditText("x^3".into()));
        let Reply::Apply(p) = e.update(Msg::Ok) else { panic!() };
        assert_eq!(p.objs[1].def, "f = x^3");
        // Abbrechen, Esc (with nothing open), Löschen.
        assert_eq!(e.update(Msg::Cancel), Reply::Cancel);
        assert_eq!(e.update(Msg::Escape), Reply::Cancel);
        assert_eq!(e.update(Msg::Delete), Reply::Delete);
    }

    /// A construction as the tablet makes it (with the core's `add`), with odd ranges, the x
    /// axis in π, named axes, styles changed by hand and a helper.
    fn tablet_plot() -> Plot {
        let mut p = fresh(300.0, 400.0, 595.0, 842.0, Background::Grid, 17.0);
        p.id = "t".into();
        for s in ["a = Schieberegler(-1, 2, 0.1)", "f(x) = 0.5x^2 - 2", "T = Punkt(f, a)", "Tangente(T, f)", "A = (-2, -1)", "B = (3, -1)", "C = (0.5, 2.5)", "Vieleck(A, B, C)", "Kreis(A, B, C)", "Mittelsenkrechte(A, B)", "q: y = x"] {
            p = p.add(s).unwrap();
        }
        let pi = std::f64::consts::PI;
        (p.x_min, p.x_max, p.y_min, p.y_max) = (-2.0 * pi, 2.5 * pi, -1.0 / 3.0, 2.0 / 3.0 + 1.0);
        p.pi = true;
        p.x_name = " t".into();
        p.y_name = "U".into();
        p.objs[1].style.special = MARK_ZEROS | MARK_EXTREMA;
        p.objs[1].style.coords = true;
        p.objs[9].style.aux = true;
        p.objs[9].style.line = LineStyle::Dotted;
        p.objs[8].style.label = LabelMode::Caption;
        p.objs[8].style.caption = "Umkreis".into();
        p
    }

    #[test]
    fn a_plot_from_the_tablet_comes_back_unchanged_when_only_ok_is_pressed() {
        let p = tablet_plot();
        assert!(p.errors().iter().all(|e| e.is_none()), "{:?}", p.errors());
        let mut e = Editor::new("s", p.clone(), (Background::Grid, 17.0));
        assert_eq!(e.ranges[0].text, "-2π");
        assert_eq!(e.ranges[1].text, "5π/2");
        assert_eq!(e.update(Msg::Ok), Reply::Apply(p.clone()), "exactly as stored");
        // Also after looking around: rows opened and left, tabs switched, a field typed and
        // typed back, objects selected.
        let mut e = Editor::new("s", p.clone(), (Background::Grid, 17.0));
        let _ = e.view();
        for o in p.objs.iter() {
            e.update(Msg::Edit(o.id.clone()));
            let _ = e.view();
        }
        e.update(Msg::EditSubmit);
        e.update(Msg::Tab(Tab::Graphics));
        e.update(Msg::Range(0, "-6".into()));
        e.update(Msg::Range(0, "-2π".into()));
        e.update(Msg::Pick(p.objs[0].id.clone()));
        e.update(Msg::SliderField(2, "0,5".into()));
        e.update(Msg::SliderField(2, "0,1".into()));
        e.update(Msg::AskRemove(p.objs[4].id.clone()));
        e.update(Msg::KeepIt);
        assert_eq!(e.update(Msg::Ok), Reply::Apply(p));
    }

    fn suggested(e: &Editor) -> Vec<&'static str> {
        e.suggestions().iter().map(|s| s.name).collect()
    }

    #[test]
    fn the_suggested_commands_are_the_cores_with_usages_that_read() {
        assert_eq!(COMMANDS.len(), lang::COMMANDS.len());
        let known: HashSet<String> = ["A", "B", "C", "M", "f", "g", "v", "a", "b"].map(String::from).into();
        for (&(de, en, args, about), &(core_de, core_en)) in COMMANDS.iter().zip(lang::COMMANDS) {
            assert_eq!((de.to_lowercase().as_str(), en.to_lowercase().as_str()), (core_de, core_en), "in the core's order");
            assert_eq!((lang::command(de), lang::command(en)), (Some(core_en), Some(core_en)));
            let usage = format!("{de}({args})");
            assert!(lang::parse_input(&usage, &known).is_ok(), "{usage} reads");
            assert!(!about.is_empty() && about.chars().count() <= 36, "{de}: {about}");
        }
        // Every command button is one of them.
        assert!(QUICK.iter().all(|q| COMMANDS.iter().any(|c| c.0 == q.0)));
    }

    #[test]
    fn typing_a_command_suggests_it_like_geogebra() {
        assert_eq!(suggest("s").len(), 0, "one letter: not yet");
        assert_eq!(suggest("sch").iter().map(|s| s.name).collect::<Vec<_>>(), vec!["Schneide", "Schieberegler"]);
        assert_eq!(suggest("Sch")[0].usage(), "Schneide(f, g)");
        assert_eq!(suggest("INTER").iter().map(|s| s.name).collect::<Vec<_>>(), vec!["Intersect"], "English too, written as GeoGebra writes it");
        assert_eq!(suggest("tang").iter().map(|s| s.name).collect::<Vec<_>>(), vec!["Tangente"], "one command once, German first");
        assert_eq!(suggest("laen").iter().map(|s| s.name).collect::<Vec<_>>(), vec!["Länge"], "ae for ä");
        assert_eq!(suggest("winkel").iter().map(|s| s.name).collect::<Vec<_>>(), vec!["Winkel", "Winkelhalbierende"], "what was typed first");
        assert_eq!(suggest("in").iter().map(|s| s.name).collect::<Vec<_>>(), vec!["Integral", "IntegralZwischen", "Intersect", "InflectionPoint"]);
        assert!(suggest("sin").is_empty() && suggest("x^").is_empty());

        let mut e = Editor::new("s", empty(), paper());
        e.update(Msg::Input("S = Sch".into()));
        assert_eq!(suggested(&e), vec!["Schneide", "Schieberegler"], "the word at the end, where the cursor is");
        // A click (or Tab) puts in the usage with the example arguments selected.
        assert_eq!(e.update(Msg::Complete(0)), Reply::Select(input_id(), 13, 17));
        assert_eq!(e.input, "S = Schneide(f, g)");
        assert_eq!(e.hint, Some(QUICK[3].2), "the help line says how it goes");
        assert!(suggested(&e).is_empty(), "and the suggestions go");
        // In the middle of the line: the word at the cursor.
        e.update(Msg::Input("Schneide(Ab)".into()));
        assert_eq!(suggested(&e), Vec::<&str>::new(), "the line ends with a bracket");
        e.update(Msg::Cursor(11));
        assert_eq!(suggested(&e), vec!["Ableitung", "Abstand"]);
        assert_eq!(e.update(Msg::Complete(0)), Reply::Select(input_id(), 19, 20));
        assert_eq!(e.input, "Schneide(Ableitung(f))");
        // Brackets already there: only the name; a text keeps its quotes.
        e.update(Msg::Input("Tan(1, f)".into()));
        e.update(Msg::Cursor(2));
        assert_eq!(suggested(&e), vec!["Tangente"], "what is before the cursor counts");
        assert_eq!(e.update(Msg::Complete(0)), Reply::Focus(input_id(), Some(9)));
        assert_eq!(e.input, "Tangente(1, f)");
        e.update(Msg::Input("t = te".into()));
        assert_eq!(e.update(Msg::Complete(0)), Reply::Select(input_id(), 10, 11));
        assert_eq!(e.input, "t = Text(\"…\")");
        // Esc hides them first, then leaves the panel.
        e.update(Msg::Input("Sch".into()));
        assert_eq!(e.update(Msg::Escape), Reply::None);
        assert!(suggested(&e).is_empty());
        assert_eq!(e.input, "Sch", "what was typed stays");
        assert_eq!(e.update(Msg::Escape), Reply::Cancel);
        // Anything else done in the panel hides them; typing brings them back.
        e.update(Msg::Input("Sch".into()));
        e.update(Msg::Tab(Tab::Graphics));
        assert!(suggested(&e).is_empty());
        assert_eq!(e.update(Msg::Complete(0)), Reply::None, "nothing to take");
        // Enter still adds what is typed.
        e.update(Msg::Input("Max(1, 2)".into()));
        e.update(Msg::Cursor(3));
        assert_eq!(suggested(&e), vec!["Max"]);
        e.update(Msg::InputSubmit);
        assert_eq!(e.plot.objs.last().map(|o| o.def.as_str()), Some("a = Max(1, 2)"));
        assert!(e.input.is_empty() && suggested(&e).is_empty());
    }

    #[test]
    fn rows_say_how_an_object_came_about_unless_its_value_says_it_all() {
        let mut e = Editor::new("s", empty(), paper());
        for s in ["a = Schieberegler(0, 5, 1)", "A = (1, 2)", "f(x) = a x^2", "g(x) = x + 2", "Schneide(f, g)", "h = Ableitung(f)", "k = 1"] {
            enter(&mut e, s);
        }
        // A mistake (as it can come from another device).
        e.plot.objs[7].def = "k = q + 1".into();
        let known: HashSet<String> = e.plot.objs.iter().map(|o| o.name()).collect();
        let vals = e.plot.values();
        let lines: Vec<(String, Option<String>)> = e.plot.objs.iter().zip(&vals).map(|(o, v)| (listing(o, v, &known), row_definition(o, v, &known))).collect();
        assert_eq!(
            lines,
            vec![
                ("a = 1".to_string(), None),
                ("A = (1 | 2)".to_string(), None),
                ("f(x) = x²".to_string(), Some("a x²".to_string())),
                ("g(x) = x + 2".to_string(), None),
                ("B = (−1 | 1)".to_string(), Some("Schneide(f, g, 1)".to_string())),
                ("C = (2 | 4)".to_string(), Some("Schneide(f, g, 2)".to_string())),
                ("h(x) = 2x".to_string(), Some("Ableitung(f)".to_string())),
                ("k = q + 1".to_string(), None),
            ],
            "a slider, a free point and a function of its own say it with their value; a mistake shows as typed"
        );
        assert!(vals[7].is_err());
    }

    #[test]
    fn the_selected_object_is_drawn_bolder_in_the_preview_only() {
        let mut e = Editor::new("s", empty(), paper());
        enter(&mut e, "f(x) = x^2 - 1");
        enter(&mut e, "A = (1, 2)");
        enter(&mut e, "g(x) = x");
        e.plot.objs[2].style.width = 2.0;
        let p = e.plot.clone();
        let size = point_size(&p);
        let width = |q: &Plot, color: u32| look(q).iter().filter(|(b, _)| b.color == color).map(|(b, _)| b.width).fold(0.0f32, f32::max);
        for (i, w, s) in [(0, p.width * 2.2, size * 1.4), (1, p.width * 2.2, size * 1.4), (2, 4.4, size * 1.4)] {
            let h = highlighted(&p, Some(i));
            assert!((h.objs[i].style.width - w).abs() < 1e-5 && (h.objs[i].style.size - s).abs() < 1e-5, "{:?}", h.objs[i].style);
            for j in (0..3).filter(|&j| j != i) {
                assert_eq!(h.objs[j], p.objs[j], "the others as they are");
            }
        }
        // The curve really is drawn wider; nothing selected: as it is.
        let solo = empty().add("f(x) = x^2 - 1").unwrap();
        let f = solo.objs[0].style.color;
        assert!(width(&highlighted(&solo, Some(0)), f) > width(&solo, f) * 2.0);
        assert_eq!(highlighted(&p, None), p);
        // Selecting changes nothing of what is stored.
        e.update(Msg::Pick(p.objs[0].id.clone()));
        assert_eq!(e.plot, p);
        let Reply::Apply(stored) = e.update(Msg::Ok) else { panic!() };
        assert_eq!(stored.objs, p.objs);
    }

    #[test]
    fn domains_curves_of_letters_and_fractions_in_the_panel() {
        let mut e = Editor::new("s", empty(), paper());
        enter(&mut e, "f(x) = x^2, 0 ≤ x ≤ 2");
        assert_eq!(rows(&e.plot), vec!["f(x) = x², 0 ≤ x ≤ 2"]);
        // The properties show the domain; what is typed there changes it.
        assert_eq!(e.domain_fields, ["0".to_string(), "2".to_string()]);
        e.update(Msg::DomainField(1, "3".into()));
        assert_eq!(e.plot.objs[0].def, "f(x) = x^2, 0 ≤ x ≤ 3");
        e.update(Msg::DomainField(0, "".into()));
        assert_eq!(e.plot.objs[0].def, "f(x) = x^2, x ≤ 3");
        e.update(Msg::DomainField(0, "5".into()));
        assert!(e.domain_error.is_some(), "from after to");
        assert_eq!(e.plot.objs[0].def, "f(x) = x^2, x ≤ 3");
        e.update(Msg::Everywhere);
        assert_eq!(e.plot.objs[0].def, "f(x) = x^2");
        assert_eq!(e.domain_fields, [String::new(), String::new()]);
        // An end dragged in the preview.
        e.update(Msg::DomainField(1, "2".into()));
        let id = id_of(&e, "f");
        let (x, y) = e.plot.to_page(1.0, 1.0);
        e.update(Msg::DragEnd(id, 1, x, y));
        assert_eq!(e.plot.objs[0].def, "f(x) = x^2, x ≤ 1");
        assert_eq!(e.domain_fields[1], "1");
        // a = 3b: a curve of the letters, the axes named after them; as sliders and back.
        enter(&mut e, "a = 3b");
        assert_eq!(rows(&e.plot)[1], "a(b) = 3b");
        assert_eq!((e.plot.x_name.as_str(), e.plot.y_name.as_str()), ("b", "a"));
        e.update(Msg::AsCurve(false));
        assert_eq!(rows(&e.plot)[1..], ["a = 3", "b = 1"]);
        e.update(Msg::AsCurve(true));
        assert_eq!(names(&e), ["f", "a"]);
        // More kinds; exact fractions as small digits.
        enter(&mut e, "r = 1 + cos θ");
        enter(&mut e, "(cos t, sin t)");
        enter(&mut e, "B = (1/3, 2/3)");
        let r = rows(&e.plot);
        assert_eq!(r[2], "c: r = 1 + cos(θ)");
        assert_eq!(r[3], "d: (cos(t), sin(t)), 0 ≤ t ≤ 2π");
        assert_eq!(r[4], "B = (¹⁄₃ | ²⁄₃)");
    }

    #[test]
    fn an_axis_stretches_alone_in_the_preview() {
        let mut e = Editor::new("s", empty().add("x^2").unwrap(), paper());
        let p = e.plot.clone();
        // The 2 on the x axis pulled to where the 1 was: twice as much x, the same y.
        e.update(Msg::AxisGrab(plot::Axis::X, p.to_page(2.0, 0.0).0));
        e.update(Msg::AxisTo(p.to_page(1.5, 0.0).0));
        e.update(Msg::AxisTo(p.to_page(1.0, 0.0).0));
        e.update(Msg::AxisLet);
        assert_eq!((e.plot.x_min, e.plot.x_max, e.plot.y_min, e.plot.y_max), (-12.0, 12.0, -4.0, 4.0));
        assert_eq!(e.ranges.iter().map(|f| f.text.as_str()).collect::<Vec<_>>(), vec!["-12", "12", "-4", "4"]);
        // The y axis: its 1 pulled up to where the 4 was — a quarter of the y.
        let q = e.plot.clone();
        e.update(Msg::AxisGrab(plot::Axis::Y, q.to_page(0.0, 1.0).1));
        e.update(Msg::AxisTo(q.to_page(0.0, 4.0).1));
        e.update(Msg::AxisLet);
        assert_eq!((e.plot.x_min, e.plot.x_max, e.plot.y_min, e.plot.y_max), (-12.0, 12.0, -1.0, 1.0));
        assert!(e.view_changed());
        // Moving the mouse with nothing grabbed changes nothing.
        let before = e.plot.clone();
        e.update(Msg::AxisTo(0.0));
        assert_eq!(e.plot, before);
    }

    #[test]
    fn the_view_goes_back_to_how_it_was() {
        let mut e = Editor::new("s", empty().add("x^2").unwrap(), paper());
        assert!(!e.view_changed());
        e.update(Msg::Range(0, "-10".into()));
        e.update(Msg::FitY);
        assert!(e.view_changed());
        e.update(Msg::ResetView);
        assert!(!e.view_changed());
        assert_eq!((e.plot.x_min, e.plot.x_max, e.plot.y_min, e.plot.y_max), (-6.0, 6.0, -4.0, 4.0));
        assert_eq!(e.ranges.iter().map(|f| f.text.as_str()).collect::<Vec<_>>(), vec!["-6", "6", "-4", "4"]);
        // Also from a half-typed range, and in π when the x axis is.
        e.update(Msg::Pi(true));
        e.update(Msg::Range(1, "".into()));
        assert!(e.view_changed() && e.result().is_err());
        e.update(Msg::ResetView);
        assert!(e.result().is_ok());
        assert_eq!((e.plot.x_min, e.plot.x_max), (-6.0, 6.0));
        assert!(e.plot.pi, "only the ranges go back");
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
        assert!(r.grid && r.color == AXES && r.width == WIDTH && r.objs.is_empty());
        // A small page: fewer squares, inside it.
        let s = fresh(50.0, 50.0, 200.0, 150.0, Background::Blank, 22.0);
        assert!(s.w <= 200.0 * 0.85 && s.x >= 0.0 && s.y >= 0.0 && s.x + s.w <= 200.0 && s.y + s.h <= 150.0, "{s:?}");
    }
}
