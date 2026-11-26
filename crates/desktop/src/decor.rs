//! A design's decorations (snow, fairy lights, stars, a grid, petals, leaves, scanlines) behind
//! the library, and still in the design cards. The same pictures as the tablet's (Decor.kt):
//! the same pseudo-random placement, sizes in logical px where the tablet has dp.
//!
//! Moving decorations ask for their own redraws (about 30 a second) without a message, so the
//! app's view isn't rebuilt for them; a window that isn't drawn (minimised) stops asking.

use std::time::{Duration, Instant};

use iced::mouse;
use iced::widget::canvas::{self, Action, Cache, Event, Frame, Geometry, Path, Stroke, path::arc::Elliptical};
use iced::{Color, Element, Length, Point, Radians, Rectangle, Renderer, Size, Theme, Vector};
use n0te_core::design::{Decor, DecorKind};

use crate::theme::{self, Palette};

/// The moment a still decoration shows (snowflakes mid-air, stars half lit).
const STILL_T: f32 = 7.3;
/// Time between frames of moving decorations.
const FRAME: Duration = Duration::from_millis(33);

/// The current design's decorations as a layer for the library (none: nothing to draw, or
/// switched off in the settings).
pub fn layer<'a, M: 'a>(show: bool, motion: bool) -> Option<Element<'a, M>> {
    let pal = theme::p();
    if !show || pal.decor.is_empty() {
        return None;
    }
    let moving = motion && pal.decor.iter().any(|d| d.kind.animated());
    Some(canvas::Canvas::new(Layer { pal, moving }).width(Length::Fill).height(Length::Fill).into())
}

/// `pal`'s decorations, still, for a design card.
pub fn still<'a, M: 'a>(pal: &'static Palette) -> Element<'a, M> {
    canvas::Canvas::new(Layer { pal, moving: false }).width(Length::Fill).height(Length::Fill).into()
}

struct Layer {
    pal: &'static Palette,
    moving: bool,
}

#[derive(Default)]
pub struct State {
    start: Option<Instant>,
    t: f32,
    /// Still pictures are drawn once per size.
    cache: Cache,
}

impl<M> canvas::Program<M> for Layer {
    type State = State;

    fn update(&self, state: &mut State, event: &Event, _bounds: Rectangle, _cursor: mouse::Cursor) -> Option<Action<M>> {
        if !self.moving {
            return None;
        }
        if let Event::Window(iced::window::Event::RedrawRequested(now)) = event {
            let start = *state.start.get_or_insert(*now);
            state.t = now.saturating_duration_since(start).as_secs_f32();
            return Some(Action::request_redraw_at(*now + FRAME));
        }
        None
    }

    fn draw(&self, state: &State, renderer: &Renderer, _theme: &Theme, bounds: Rectangle, _cursor: mouse::Cursor) -> Vec<Geometry> {
        if self.moving {
            let mut frame = Frame::new(renderer, bounds.size());
            draw(&mut frame, self.pal, state.t);
            vec![frame.into_geometry()]
        } else {
            vec![state.cache.draw(renderer, bounds.size(), |frame| draw(frame, self.pal, STILL_T))]
        }
    }
}

/// A stable pseudo-random number in 0 … 1 for item `i` (each `salt` another property); the
/// same as the tablet's `Decor.h` (32-bit wrapping arithmetic).
pub fn h(i: i32, salt: i32) -> f32 {
    let mut x = i.wrapping_mul(374_761_393).wrapping_add(salt.wrapping_mul(668_265_263));
    x = (x ^ ((x as u32 >> 13) as i32)).wrapping_mul(1_274_126_177);
    x ^= (x as u32 >> 16) as i32;
    (x & 0xFF_FFFF) as f32 / 16_777_215.0
}

/// How many items for an area: `base` on a 1200 × 800 screen, fewer on small ones.
pub fn count(base: f32, w: f32, h: f32) -> i32 {
    ((base * ((w * h) / (1200.0 * 800.0)).clamp(0.12, 1.6)) as i32).max(3)
}

fn rgb(c: u32, a: f32) -> Color {
    Color::from_rgba8((c >> 16) as u8, (c >> 8) as u8, c as u8, a.clamp(0.0, 1.0))
}

fn colors<'a>(d: &'a Decor, defaults: &'a [u32]) -> &'a [u32] {
    if d.colors.is_empty() { defaults } else { &d.colors }
}

/// Draws `pal`'s decorations at time `t` (seconds).
pub fn draw(frame: &mut Frame, pal: &Palette, t: f32) {
    let s = frame.size();
    if s.width <= 0.0 || s.height <= 0.0 {
        return;
    }
    for d in &pal.decor {
        match d.kind {
            DecorKind::Snow => snow(frame, d, t),
            DecorKind::Petals => falling(frame, d, t, false),
            DecorKind::Leaves => falling(frame, d, t, true),
            DecorKind::Stars => stars(frame, d, t),
            DecorKind::Lights => lights(frame, d, t, pal),
            DecorKind::Grid => grid(frame, d, pal),
            DecorKind::Scanlines => scanlines(frame, d, pal),
        }
    }
}

fn snow(frame: &mut Frame, d: &Decor, t: f32) {
    let Size { width: w, height: hh } = frame.size();
    let n = count(24.0 + 96.0 * d.amount, w, hh);
    let cols = colors(d, &[0xFFFFFF]);
    let pad = 12.0;
    for i in 0..n {
        let r = 1.1 + 2.8 * h(i, 3);
        let speed = 12.0 + 24.0 * h(i, 2) + r * 4.0;
        let y = (h(i, 4) * (hh + 2.0 * pad) + t * speed).rem_euclid(hh + 2.0 * pad) - pad;
        let x = h(i, 1) * w + (t * (0.4 + 0.5 * h(i, 8)) + h(i, 5) * 6.283).sin() * 14.0 * h(i, 6);
        frame.fill(&Path::circle(Point::new(x, y), r), rgb(cols[i as usize % cols.len()], 0.35 + 0.55 * h(i, 7)));
    }
}

fn falling(frame: &mut Frame, d: &Decor, t: f32, leaf: bool) {
    let Size { width: w, height: hh } = frame.size();
    let n = count(if leaf { 8.0 + 22.0 * d.amount } else { 10.0 + 28.0 * d.amount }, w, hh);
    let cols = if leaf { colors(d, &[0xC2410C, 0xD97706, 0xA16207]) } else { colors(d, &[0xF9A8C2, 0xFBCFE0]) };
    for i in 0..n {
        let s = if leaf { 9.0 + 6.0 * h(i, 3) } else { 6.0 + 4.0 * h(i, 3) };
        let pad = 2.0 * s;
        let speed = 10.0 + 18.0 * h(i, 2);
        let y = (h(i, 4) * (hh + 2.0 * pad) + t * speed).rem_euclid(hh + 2.0 * pad) - pad;
        let x = h(i, 1) * w + (t * (0.3 + 0.4 * h(i, 8)) + h(i, 5) * 6.283).sin() * 30.0 * h(i, 6);
        let spin = (20.0 + 60.0 * h(i, 10)) * if h(i, 11) > 0.5 { 1.0 } else { -1.0 };
        let angle = (h(i, 9) * 360.0 + t * spin).to_radians();
        let c = rgb(cols[i as usize % cols.len()], 0.55 + 0.35 * h(i, 7));
        let shape = if leaf {
            // Two arcs meeting in points, turned about the middle.
            let (sin, cos) = angle.sin_cos();
            let at = |dx: f32, dy: f32| Point::new(x + dx * cos - dy * sin, y + dx * sin + dy * cos);
            Path::new(|b| {
                b.move_to(at(-s / 2.0, 0.0));
                b.quadratic_curve_to(at(0.0, -s * 0.45), at(s / 2.0, 0.0));
                b.quadratic_curve_to(at(0.0, s * 0.45), at(-s / 2.0, 0.0));
                b.close();
            })
        } else {
            Path::new(|b| {
                b.ellipse(Elliptical {
                    center: Point::new(x, y),
                    radii: Vector::new(s / 2.0, s * 0.3),
                    rotation: Radians(angle),
                    start_angle: Radians(0.0),
                    end_angle: Radians(std::f32::consts::TAU),
                })
            })
        };
        frame.fill(&shape, c);
    }
}

fn stars(frame: &mut Frame, d: &Decor, t: f32) {
    let Size { width: w, height: hh } = frame.size();
    let n = count(36.0 + 110.0 * d.amount, w, hh);
    let cols = colors(d, &[0xFFFFFF]);
    for i in 0..n {
        let r = 0.5 + 1.3 * h(i, 3) * h(i, 3);
        let twinkle = 0.5 + 0.5 * (t * (0.6 + 1.8 * h(i, 4)) + h(i, 5) * 6.283).sin();
        let a = (0.2 + 0.8 * twinkle * (0.6 + 0.4 * h(i, 6))).clamp(0.0, 1.0);
        let o = Point::new(h(i, 1) * w, h(i, 2) * hh);
        let c = cols[i as usize % cols.len()];
        frame.fill(&Path::circle(o, r), rgb(c, a));
        if h(i, 7) > 0.9 {
            let l = r * 3.5;
            let sparkle = Stroke::default().with_color(rgb(c, a * 0.7)).with_width(0.8);
            frame.stroke(&Path::line(Point::new(o.x - l, o.y), Point::new(o.x + l, o.y)), sparkle);
            frame.stroke(&Path::line(Point::new(o.x, o.y - l), Point::new(o.x, o.y + l)), sparkle);
        }
    }
}

fn lights(frame: &mut Frame, d: &Decor, t: f32, pal: &Palette) {
    let w = frame.width();
    let cols = colors(d, &[0xF5C542, 0xD7263D, 0x3FA34D, 0x4A90E2]);
    let span = 160.0;
    let sag = 22.0 * (0.6 + 0.6 * d.amount);
    let top = 2.0;
    let segments = (w / span) as i32 + 1;
    let wire = Path::new(|b| {
        b.move_to(Point::new(0.0, top));
        for s in 0..segments {
            let x0 = s as f32 * span;
            b.quadratic_curve_to(Point::new(x0 + span / 2.0, top + 2.0 * sag), Point::new(x0 + span, top));
        }
    });
    frame.stroke(&wire, Stroke::default().with_color(theme::alpha(pal.muted, 0.7)).with_width(1.2));
    let drop = 5.0;
    let mut i = 0;
    for s in 0..segments {
        for u in [0.25f32, 0.5, 0.75] {
            let x0 = s as f32 * span;
            let x = (1.0 - u) * (1.0 - u) * x0 + 2.0 * u * (1.0 - u) * (x0 + span / 2.0) + u * u * (x0 + span);
            let y = top + 2.0 * u * (1.0 - u) * 2.0 * sag + drop;
            let glow = 0.55 + 0.45 * (t * 1.3 + i as f32 * 1.7).sin();
            let c = cols[i as usize % cols.len()];
            frame.fill(&Path::circle(Point::new(x, y), 11.0), rgb(c, 0.2 * glow));
            frame.fill(&Path::circle(Point::new(x, y), 4.0), rgb(c, 0.6 + 0.4 * glow));
            frame.fill_rectangle(Point::new(x - 1.5, y - drop + 1.0), Size::new(3.0, 3.0), pal.muted);
            i += 1;
        }
    }
}

fn grid(frame: &mut Frame, d: &Decor, pal: &Palette) {
    let Size { width: w, height: hh } = frame.size();
    let base = d.colors.first().map(|c| rgb(*c, 1.0)).unwrap_or(pal.text);
    let step = 24.0;
    let a = 0.05 + 0.06 * d.amount;
    let line = |k: i32| Stroke::default().with_color(theme::alpha(base, if k % 5 == 0 { a * 2.2 } else { a })).with_width(1.0);
    let (mut x, mut k) = (0.0, 0);
    while x <= w {
        frame.stroke(&Path::line(Point::new(x, 0.0), Point::new(x, hh)), line(k));
        x += step;
        k += 1;
    }
    let (mut y, mut k) = (0.0, 0);
    while y <= hh {
        frame.stroke(&Path::line(Point::new(0.0, y), Point::new(w, y)), line(k));
        y += step;
        k += 1;
    }
}

fn scanlines(frame: &mut Frame, d: &Decor, pal: &Palette) {
    let Size { width: w, height: hh } = frame.size();
    let c = d.colors.first().map(|c| rgb(*c, 1.0)).unwrap_or(pal.accent);
    let c = theme::alpha(c, 0.04 + 0.08 * d.amount);
    let mut y = 0.0;
    while y <= hh {
        frame.fill_rectangle(Point::new(0.0, y), Size::new(w, 1.0), c);
        y += 3.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scatter_matches_the_tablet() {
        // The same numbers as Decor.h on the tablet (DesignsTest pins them there), so both
        // draw the same sky.
        for (i, salt, want) in [(0, 1, 0.678_285_9), (7, 3, 0.247_704), (123_456, 11, 0.918_759_6), (-5, 2, 0.313_836_5)] {
            assert!((h(i, salt) - want).abs() < 1e-6, "h({i}, {salt}) = {}", h(i, salt));
        }
        assert_ne!(h(1, 1), h(2, 1));
        assert_eq!(h(5, 2), h(5, 2));
        assert_eq!(count(100.0, 1200.0, 800.0), 100);
        assert_eq!(count(100.0, 10.0, 10.0), 12);
        assert_eq!(count(100.0, 10_000.0, 10_000.0), 160);
    }
}
