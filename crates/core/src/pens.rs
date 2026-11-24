//! Pens as the tablet app offers them: pen types, colour palettes, width presets, the marker,
//! and the fountain pen's character (speed, nib, taper) baked into the samples' pressure as
//! they are written. Shared by every app that writes ink (the Android app has the same rules
//! in Kotlin: N0te.kt `PenType`/`Tools`, Settings.kt `Palettes`, Fountain.kt), so a stroke
//! looks the same whoever wrote it.

use crate::ink::{Brush, InkPoint, KIND_MARKER, KIND_PEN, KIND_PENCIL};

/// Pen types: how width follows pressure, how opaque the ink is, how it is drawn (kind).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PenType {
    /// Füller: the line follows the pressure (and speed, nib, taper; see [`Fountain`]).
    Fountain,
    /// Kugelschreiber: an even line.
    Ballpoint,
    /// Bleistift: graphite, soft at the edge, darker with pressure.
    Pencil,
    /// Pinsel: strongly pressure dependent.
    Brush,
}

impl PenType {
    pub const ALL: [PenType; 4] = [PenType::Fountain, PenType::Ballpoint, PenType::Pencil, PenType::Brush];

    /// Width factor on the chosen width preset.
    pub fn width_mul(self) -> f32 {
        match self {
            PenType::Fountain => 1.0,
            PenType::Ballpoint => 0.8,
            PenType::Pencil => 1.25,
            PenType::Brush => 1.8,
        }
    }
    /// How much pressure changes the width (the fountain pen's comes from its options).
    pub fn thinning(self) -> f32 {
        match self {
            PenType::Fountain => 0.65,
            PenType::Ballpoint => 0.12,
            PenType::Pencil => 0.2,
            PenType::Brush => 0.95,
        }
    }
    /// Ink opacity 0..=255 (the pencil's comes from the pencil settings).
    pub fn alpha(self) -> u8 {
        match self {
            PenType::Pencil => 0xF0,
            _ => 0xFF,
        }
    }
    pub fn kind(self) -> u8 {
        match self {
            PenType::Pencil => KIND_PENCIL,
            _ => KIND_PEN,
        }
    }
    /// Stable name for settings files.
    pub fn key(self) -> &'static str {
        match self {
            PenType::Fountain => "FOUNTAIN",
            PenType::Ballpoint => "BALLPOINT",
            PenType::Pencil => "PENCIL",
            PenType::Brush => "BRUSH",
        }
    }
    pub fn from_key(s: &str) -> Option<PenType> {
        PenType::ALL.into_iter().find(|p| p.key() == s)
    }
}

/// Width presets in page points (the toolbar's four).
pub const WIDTHS: [f32; 4] = [1.2, 2.0, 3.2, 5.0];
/// Range a width preset can be set to.
pub const MIN_WIDTH: f32 = 0.4;
pub const MAX_WIDTH: f32 = 14.0;
/// The marker: this much wider than the width preset, this opaque.
pub const MARKER_WIDTH_MUL: f32 = 5.0;
pub const MARKER_ALPHA: u8 = 0x66;
/// Default pencil opacity (settings: 0..1; never below 40/255).
pub const PENCIL_OPACITY: f32 = 0.85;

/// Colour palettes (six colours each, 0xAARRGGBB), with their names.
pub const PALETTE_NAMES: [&str; 6] = ["Standard", "Pastell", "Neon", "Erde", "Grau", "Textmarker"];
pub const PALETTES: [[u32; 6]; 6] = [
    [0xFF111111, 0xFF1E5BD8, 0xFFD71921, 0xFF138A47, 0xFFE08A00, 0xFF7B3FD0],
    [0xFF6B7FD7, 0xFF8FC1E3, 0xFFF2A2B1, 0xFF9ED9A0, 0xFFF6D186, 0xFFC3A6E8],
    [0xFF00E5FF, 0xFFFF2D95, 0xFF39FF14, 0xFFFFEA00, 0xFFFF6B00, 0xFFB026FF],
    [0xFF5B3A29, 0xFF8C6239, 0xFFA3B18A, 0xFF588157, 0xFFBC6C25, 0xFF3A5A40],
    [0xFF000000, 0xFF333333, 0xFF555555, 0xFF777777, 0xFF999999, 0xFFBBBBBB],
    [0xFFFFE600, 0xFF7CFC00, 0xFF00E1FF, 0xFFFF6EC7, 0xFFFFA500, 0xFFB388FF],
];

/// The ink opacity a pen type writes with (`pencil_opacity`: the pencil setting, 0..1).
pub fn ink_alpha(pen: PenType, pencil_opacity: f32) -> u8 {
    if pen.kind() == KIND_PENCIL { (pencil_opacity * 255.0).clamp(40.0, 255.0) as u8 } else { pen.alpha() }
}

/// The brush for writing with `pen` in `color` (alpha ignored) at width preset `width`.
pub fn pen_brush(pen: PenType, color: u32, width: f32, fountain: &FountainOptions, pencil_opacity: f32) -> Brush {
    let thinning = if pen == PenType::Fountain { fountain.thinning() } else { pen.thinning() };
    Brush { width: width * pen.width_mul(), thinning, color: (color & 0x00FF_FFFF) | ((ink_alpha(pen, pencil_opacity) as u32) << 24), kind: pen.kind() }
}

/// The marker's brush (see-through, wide, no pressure).
pub fn marker_brush(color: u32, width: f32) -> Brush {
    Brush { width: width * MARKER_WIDTH_MUL, thinning: 0.0, color: (color & 0x00FF_FFFF) | ((MARKER_ALPHA as u32) << 24), kind: KIND_MARKER }
}

/// The fountain pen's nib.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nib {
    Round,
    /// Oblique, 45°.
    Oblique,
    /// Broad / italic, 30°.
    Flat,
}

impl Nib {
    pub fn key(self) -> &'static str {
        match self {
            Nib::Round => "round",
            Nib::Oblique => "oblique",
            Nib::Flat => "flat",
        }
    }
    pub fn from_key(s: &str) -> Nib {
        match s {
            "oblique" => Nib::Oblique,
            "flat" => Nib::Flat,
            _ => Nib::Round,
        }
    }
}

/// The fountain pen's options (the pen menu).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FountainOptions {
    /// How much pressure changes the width (the brush's thinning), 0..1.
    pub sensitivity: f32,
    /// How much a quick stroke thins the line, 0..1.
    pub speed: f32,
    pub nib: Nib,
    /// Strokes start and lift off thin.
    pub taper: bool,
}

impl Default for FountainOptions {
    fn default() -> Self {
        FountainOptions { sensitivity: 0.65, speed: 0.5, nib: Nib::Round, taper: true }
    }
}

impl FountainOptions {
    /// The brush's thinning: a shaped nib needs the full range of pressure to show its angle.
    pub fn thinning(&self) -> f32 {
        if self.nib == Nib::Round { self.sensitivity } else { self.sensitivity.max(0.9) }
    }
}

const TAPER_START: f32 = 5.0;
const TAPER_END: f32 = 7.0;

fn smooth(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// The fountain pen running over one stroke: [`Fountain::pressure`] for each new sample,
/// [`Fountain::taper_end`] at pen-up.
#[derive(Debug, Clone)]
pub struct Fountain {
    pub options: FountainOptions,
    n: u32,
    last: (f32, f32, f32),
    length: f32,
    speed: f32,
    dir: (f32, f32),
}

impl Fountain {
    pub fn new(options: FountainOptions) -> Self {
        Fountain { options, n: 0, last: (0.0, 0.0, 0.0), length: 0.0, speed: 0.0, dir: (1.0, 0.0) }
    }

    /// The pressure to store for a new sample at (x, y) page points, pen pressure `p`, time `t` ms.
    pub fn pressure(&mut self, x: f32, y: f32, p: f32, t: f32) -> f32 {
        let o = self.options;
        let mut f = 1.0f32;
        if self.n > 0 {
            let (dx, dy) = (x - self.last.0, y - self.last.1);
            let d = dx.hypot(dy);
            self.length += d;
            let dt = (t - self.last.2).max(1.0);
            self.speed = self.speed * 0.7 + (d / dt) * 0.3;
            if d > 0.05 {
                self.dir = (self.dir.0 * 0.6 + dx / d * 0.4, self.dir.1 * 0.6 + dy / d * 0.4);
            }
        }
        self.n += 1;
        self.last = (x, y, t);
        // Speed (pt/ms): from a calm 0.15 to a quick 1.2 the line thins by up to 45 %.
        f *= 1.0 - o.speed * 0.45 * ((self.speed - 0.15) / 1.05).clamp(0.0, 1.0);
        // Nib: widest across the nib's edge, thinnest along it.
        if o.nib != Nib::Round {
            let (angle, thin) = if o.nib == Nib::Flat { ((-30.0f32).to_radians(), 0.22) } else { ((-45.0f32).to_radians(), 0.45) };
            let dir = self.dir.1.atan2(self.dir.0);
            f *= thin + (1.0 - thin) * (dir - angle).sin().abs();
        }
        if o.taper {
            f *= 0.45 + 0.55 * smooth(0.0, TAPER_START, self.length);
        }
        (p * f).clamp(0.03, 1.0)
    }

    /// At pen-up: the stroke lifts off thin over its last few points.
    pub fn taper_end(&self, points: &mut [InkPoint]) {
        if !self.options.taper || points.len() < 3 {
            return;
        }
        let mut len = 0.0f32;
        for i in (1..points.len()).rev() {
            let k = 0.4 + 0.6 * smooth(0.0, TAPER_END, len);
            if k >= 0.999 {
                break;
            }
            points[i].pressure = (points[i].pressure * k).max(0.03);
            len += (points[i].x - points[i - 1].x).hypot(points[i].y - points[i - 1].y);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f32, y: f32, p: f32, t: u32) -> InkPoint {
        InkPoint { x, y, pressure: p, t_ms: t }
    }

    #[test]
    fn brushes_match_the_tablet() {
        let f = FountainOptions::default();
        let b = pen_brush(PenType::Fountain, 0xFF1E5BD8, 2.0, &f, PENCIL_OPACITY);
        assert_eq!((b.width, b.thinning, b.color, b.kind), (2.0, 0.65, 0xFF1E5BD8, KIND_PEN));
        let b = pen_brush(PenType::Pencil, 0xFF111111, 2.0, &f, PENCIL_OPACITY);
        assert_eq!((b.width, b.kind, b.color >> 24), (2.5, KIND_PENCIL, 216));
        let b = pen_brush(PenType::Ballpoint, 0x12345678, 3.2, &f, PENCIL_OPACITY);
        assert_eq!(b.color, 0xFF345678);
        let m = marker_brush(0xFFFFE600, 2.0);
        assert_eq!((m.width, m.thinning, m.color, m.kind), (10.0, 0.0, 0x66FFE600, KIND_MARKER));
        let shaped = FountainOptions { nib: Nib::Flat, sensitivity: 0.3, ..f };
        assert_eq!(shaped.thinning(), 0.9);
    }

    #[test]
    fn fountain_tapers_and_thins_when_quick() {
        let mut slow = Fountain::new(FountainOptions::default());
        let mut quick = Fountain::new(FountainOptions::default());
        let mut ps = Vec::new();
        let mut pq = Vec::new();
        for i in 0..40 {
            ps.push(slow.pressure(i as f32 * 0.5, 0.0, 0.8, i as f32 * 10.0));
            pq.push(quick.pressure(i as f32 * 20.0, 0.0, 0.8, i as f32 * 10.0));
        }
        // Starts thin (taper), then full.
        assert!(ps[0] < ps[39] * 0.6, "{} vs {}", ps[0], ps[39]);
        // A quick stroke is thinner than a slow one.
        assert!(pq[39] < ps[39] * 0.8, "{} vs {}", pq[39], ps[39]);
        let mut pts: Vec<InkPoint> = (0..20).map(|i| pt(i as f32, 0.0, 0.8, i * 10)).collect();
        slow.taper_end(&mut pts);
        assert!(pts[19].pressure < 0.8 * 0.5 && pts[0].pressure == 0.8);
    }

    #[test]
    fn oblique_nib_follows_direction() {
        let o = FountainOptions { nib: Nib::Oblique, taper: false, speed: 0.0, ..Default::default() };
        let along = {
            // Along the nib (-45°: up and to the right on screen, y down).
            let mut f = Fountain::new(o);
            (0..20).map(|i| f.pressure(i as f32, -(i as f32), 1.0, i as f32 * 10.0)).last().unwrap()
        };
        let across = {
            let mut f = Fountain::new(o);
            (0..20).map(|i| f.pressure(i as f32, i as f32, 1.0, i as f32 * 10.0)).last().unwrap()
        };
        assert!(along < 0.6 && across > 0.95, "along {along} across {across}");
    }
}
