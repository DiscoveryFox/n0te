//! Coordinate systems with function graphs on the laptop. Their look comes from the core as ink
//! ([`n0te_core::plot::render`]: axes, numbers, curves, drawn as if by hand) and is drawn like
//! strokes, worked out once per plot value. A click on one opens the "Koordinatensystem" dialog
//! ([`Editor`]: functions, ranges, grid, ruler-straight, a live preview); the palette and the top
//! bar insert a new one ([`fresh`]).

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::graphics::mesh::Mesh;
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer::{self, Renderer as _};
use iced::advanced::widget::{self, Widget};
use iced::advanced::mouse;
use iced::widget::{Space, button, canvas, column, container, row, text, text_input, toggler};
use iced::{Alignment, Border, Color, Element, Length, Padding, Point, Rectangle, Size, Theme, Vector};

use n0te_core::ink::{Brush, InkPoint, Rect};
use n0te_core::notebook::Background;
use n0te_core::plot::{self, Plot, PlotFn};

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

/// The name of the `i`-th function: f, g, h, k, …
pub fn name(i: usize) -> String {
    const NAMES: [&str; 8] = ["f", "g", "h", "k", "p", "q", "r", "s"];
    NAMES.get(i).map(|n| n.to_string()).unwrap_or_else(|| format!("f{}", i + 1))
}

/// The id of the `i`-th function's field (focus).
pub fn expr_id(i: usize) -> iced::widget::Id {
    iced::widget::Id::from(format!("n0te-plot-fn-{i}"))
}

/// What the dialog does.
#[derive(Debug, Clone)]
pub enum Msg {
    Expr(usize, String),
    /// The swatch: the next pen colour.
    Color(usize),
    RemoveFn(usize),
    AddFn,
    /// Range field (0 x from, 1 x to, 2 y from, 3 y to) typed.
    Range(usize, String),
    FitY,
    EqualUnits,
    Grid(bool),
    Clean(bool),
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

/// The "Koordinatensystem" dialog for one plot.
pub struct Editor {
    pub sheet_id: String,
    /// The plot as it is being edited: always drawable (ranges take typed values only once they
    /// work), functions as typed.
    pub plot: Plot,
    /// The plot as it was when the dialog opened.
    pub original: Plot,
    /// What the range fields say: x from, x to, y from, y to.
    pub ranges: [String; 4],
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
            plot.fns.push(PlotFn { expr: String::new(), color: next_color(&[]) });
        }
        let ranges = [fmt_num(plot.x_min), fmt_num(plot.x_max), fmt_num(plot.y_min), fmt_num(plot.y_max)];
        Editor { sheet_id: sheet_id.to_string(), plot, original, ranges, paper, preview: RefCell::new(None) }
    }

    pub fn update(&mut self, msg: Msg) -> Reply {
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
            Msg::RemoveFn(i) => {
                if i < self.plot.fns.len() {
                    self.plot.fns.remove(i);
                }
            }
            Msg::AddFn => {
                let used: Vec<u32> = self.plot.fns.iter().map(|f| f.color).collect();
                self.plot.fns.push(PlotFn { expr: String::new(), color: next_color(&used) });
                return Reply::Focus(self.plot.fns.len() - 1);
            }
            Msg::Range(i, s) => {
                if i < 4 {
                    self.ranges[i] = s;
                    self.take_ranges();
                }
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
            Msg::Grid(on) => self.plot.grid = on,
            Msg::Clean(on) => self.plot.clean = on,
            Msg::Submit | Msg::Ok => {
                if let Ok(p) = self.result() {
                    return Reply::Apply(p);
                }
            }
            Msg::Delete => return Reply::Delete,
            Msg::Cancel => return Reply::Cancel,
        }
        Reply::None
    }

    fn set_y(&mut self, lo: f64, hi: f64) {
        if lo.is_finite() && hi.is_finite() && hi > lo {
            self.plot.y_min = lo;
            self.plot.y_max = hi;
            self.ranges[2] = fmt_num(lo);
            self.ranges[3] = fmt_num(hi);
        }
    }

    /// Ranges typed so far that work go into the plot (the preview follows).
    fn take_ranges(&mut self) {
        let v: Vec<Option<f64>> = self.ranges.iter().map(|s| number(s)).collect();
        if let (Some(a), Some(b)) = (v[0], v[1])
            && b > a
        {
            self.plot.x_min = a;
            self.plot.x_max = b;
        }
        if let (Some(a), Some(b)) = (v[2], v[3])
            && b > a
        {
            self.plot.y_min = a;
            self.plot.y_max = b;
        }
    }

    /// What is wrong with an axis' range (0: x, 1: y), if anything.
    pub fn range_error(&self, axis: usize) -> Option<String> {
        let (a, b) = (number(&self.ranges[axis * 2]), number(&self.ranges[axis * 2 + 1]));
        let n = if axis == 0 { "x" } else { "y" };
        match (a, b) {
            (None, _) => Some(format!("{n} von: keine Zahl")),
            (_, None) => Some(format!("{n} bis: keine Zahl")),
            (Some(a), Some(b)) if b <= a => Some(format!("{n} bis muss größer sein als {n} von")),
            _ => None,
        }
    }

    /// What is wrong with function `i` (empty ones are left out, not wrong).
    pub fn fn_error(&self, i: usize) -> Option<String> {
        let f = self.plot.fns.get(i)?;
        if f.expr.trim().is_empty() {
            return None;
        }
        plot::check(&f.expr).err()
    }

    /// The plot to store, or why not (OK stays off).
    pub fn result(&self) -> Result<Plot, String> {
        for axis in 0..2 {
            if let Some(e) = self.range_error(axis) {
                return Err(e);
            }
        }
        for i in 0..self.plot.fns.len() {
            if let Some(e) = self.fn_error(i) {
                return Err(format!("{}(x): {e}", name(i)));
            }
        }
        let mut p = self.plot.clone();
        p.fns.retain(|f| !f.expr.trim().is_empty());
        for f in &mut p.fns {
            f.expr = f.expr.trim().to_string();
        }
        let n = |i: usize| number(&self.ranges[i]).unwrap_or(0.0);
        (p.x_min, p.x_max, p.y_min, p.y_max) = (n(0), n(1), n(2), n(3));
        if !p.is_valid() {
            return Err("Dieser Bereich lässt sich nicht zeichnen".into());
        }
        Ok(p)
    }

    pub fn view(&self) -> Element<'_, Msg> {
        let muted = |s: String| text(s).size(12).color(theme::p().muted);
        let error = |s: String| text(s).size(12).color(theme::p().accent);
        let label_w = 54.0;

        let mut fns = column![theme::caption("Funktionen")].spacing(8);
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
            .spacing(3);
            if let Some(e) = self.fn_error(i) {
                cell = cell.push(row![Space::new().width(Length::Fixed(label_w + 42.0)), error(e)]);
            }
            fns = fns.push(cell);
        }
        fns = fns.push(
            row![
                button(theme::btn_small("Funktion hinzufügen")).padding(theme::pad()).on_press(Msg::AddFn).style(theme::secondary),
                Space::new().width(12),
                muted("2x, x^2, sin x, √x, |x|, e^-x, ln x, 0,5x".into()),
            ]
            .align_y(Alignment::Center),
        );

        let num = |i: usize, hint: &'static str| text_input(hint, &self.ranges[i]).on_input(move |s| Msg::Range(i, s)).on_submit(Msg::Submit).font(theme::MONO).padding(7).size(14).width(Length::Fixed(84.0)).style(theme::field);
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
        let mut ranges = column![
            theme::caption("Bereich"),
            axis_row(0),
            row![
                axis_row(1),
                Space::new().width(10),
                button(theme::btn_small("y passend")).padding(theme::pad()).on_press_maybe(self.plot.fit_y().is_some().then_some(Msg::FitY)).style(theme::secondary),
                button(theme::btn_small("Gleiche Einheiten")).padding(theme::pad()).on_press(Msg::EqualUnits).style(theme::secondary),
            ]
            .spacing(8)
            .align_y(Alignment::Center),
        ]
        .spacing(8);
        for axis in 0..2 {
            if let Some(e) = self.range_error(axis) {
                ranges = ranges.push(error(e));
            }
        }
        let switches = row![
            toggler(self.plot.grid).label("Raster").on_toggle(Msg::Grid).size(16).text_size(13).spacing(8).style(theme::switch),
            Space::new().width(22),
            toggler(self.plot.clean).label("Wie mit dem Lineal").on_toggle(Msg::Clean).size(16).text_size(13).spacing(8).style(theme::switch),
        ]
        .align_y(Alignment::Center);

        let preview = column![theme::caption("Vorschau"), Preview { plot: &self.plot, paper: self.paper, cache: &self.preview, size: Size::new(300.0, 240.0) }].spacing(8);
        let body = row![column![fns, ranges, switches].spacing(18).width(Length::Fill), Space::new().width(20), preview];
        let ok = self.result().is_ok();
        let col = column![
            theme::heading("Koordinatensystem", 22.0),
            body,
            container(Space::new()).height(1).width(Length::Fill).style(theme::hairline),
            row![
                button(theme::btn("Löschen")).padding(theme::pad()).on_press(Msg::Delete).style(theme::ghost),
                Space::new().width(Length::Fill),
                button(theme::btn("Abbrechen")).padding(theme::pad()).on_press(Msg::Cancel).style(theme::ghost),
                Space::new().width(8),
                button(theme::btn("OK")).padding(theme::pad()).on_press_maybe(ok.then_some(Msg::Ok)).style(theme::accent_button),
            ]
            .align_y(Alignment::Center),
        ]
        .spacing(16);
        let card = container(col).padding(24).width(Length::Fill).max_width(900).style(theme::panel);
        // Full size, so clicks next to the dialog don't reach the pages under it.
        iced::widget::mouse_area(container(card).padding(Padding { top: 70.0, right: 16.0, bottom: 16.0, left: 16.0 }).center_x(Length::Fill).height(Length::Fill)).interaction(mouse::Interaction::Idle).into()
    }
}

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
        assert_eq!(got.fns, vec![PlotFn { expr: "x^2 - 1".into(), color: INKS[1] }], "the empty one is left out");
        e.update(Msg::FitY);
        assert!(e.plot.y_min <= 0.0 && e.plot.y_max >= 60.0, "{:?}", (e.plot.y_min, e.plot.y_max));
        e.update(Msg::EqualUnits);
        let per_x = (e.plot.x_max - e.plot.x_min) / e.plot.w as f64;
        assert!(((e.plot.y_max - e.plot.y_min) / e.plot.h as f64 - per_x).abs() < 1e-9);
        assert_eq!(e.update(Msg::Cancel), Reply::Cancel);
    }
}
