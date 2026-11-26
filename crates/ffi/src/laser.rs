//! The laser pointer's look for the tablet (core `laser`): what to draw each frame, and the
//! smooth playback of other devices' pointers.

use std::sync::Mutex;

use n0te_core::laser::{self, Look, Replay, Style};

/// What to draw for one pointer now: the trail's `core` and `glow` as four-cornered pieces
/// (x, y of four corners in page points, then alpha: 9 numbers each; fill each, they meet
/// without overlapping); the `dot` as x, y, alpha (empty when there is none); `gone`: forget it.
#[derive(Debug, Clone, uniffi::Record)]
pub struct FfiLaserLook {
    pub core: Vec<f32>,
    pub glow: Vec<f32>,
    pub dot: Vec<f32>,
    pub gone: bool,
}

/// At `px` screen pixels per page point.
fn ffi(l: Look, px: f32) -> FfiLaserLook {
    let core = l.quads(px, 1.0).into_iter().flatten().collect();
    let glow = l.quads(px, 3.0).into_iter().flatten().collect();
    let dot = l.dot.map(|d| vec![d.0, d.1, d.2]).unwrap_or_default();
    FfiLaserLook { core, glow, dot, gone: l.gone }
}

fn triples(v: &[f32]) -> Vec<(f32, f32, f32)> {
    v.chunks_exact(3).map(|c| (c[0], c[1], c[2])).collect()
}

/// This device's pointer: its samples (x, y, t triples), `lifted` the time the pen lifted
/// (negative while it is down), the trail setting ("short", "long", "keep"), `px` screen
/// pixels per page point.
#[uniffi::export]
pub fn laser_look(samples: Vec<f32>, now: f32, lifted: f32, trail: String, px: f32) -> FfiLaserLook {
    ffi(laser::ribbon(&triples(&samples), now, (lifted >= 0.0).then_some(lifted), Style::named(&trail)), px)
}

/// The colour (0xAARRGGBB) of a pointer: red for the presenter, a steady colour per guest.
#[uniffi::export]
pub fn laser_colour(who: String, guest: bool) -> u32 {
    laser::colour(&who, guest)
}

/// Another device's pointer, played back smoothly on this device's clock.
#[derive(uniffi::Object, Default)]
pub struct LaserReplay {
    inner: Mutex<Replay>,
}

#[uniffi::export]
impl LaserReplay {
    #[uniffi::constructor]
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self::default())
    }
    /// A burst of samples (x, y, t on the pointing device's clock) received at `now`.
    pub fn add(&self, samples: Vec<f32>, down: bool, now: f32) {
        crate::lock(&self.inner).add(&triples(&samples), down, now);
    }
    pub fn look(&self, now: f32, trail: String, px: f32) -> FfiLaserLook {
        ffi(crate::lock(&self.inner).look(now, Style::named(&trail)), px)
    }
}
