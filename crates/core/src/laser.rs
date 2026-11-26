//! How a laser pointer looks, the same on every device: a soft glowing dot at the pen and a
//! trail behind it that is smooth (a curve through the samples, not a zigzag of them), gets
//! thinner and fainter towards its end and fades out as a whole when the pen lifts.
//!
//! Renderers keep the samples (x, y in page points, t in ms on their own clock) and ask
//! [`ribbon`] each frame for what to draw: segments with a width (screen pixels, so the
//! pointer looks the same at every zoom) and an opacity, plus the dot.

/// Which pointer colour: the presenter's (own devices and the owner) or a guest's.
pub const OWN: u32 = 0xFFFF3B30;
/// Guests' pointers, by who points.
pub const GUESTS: [u32; 6] = [0xFF0A84FF, 0xFF30D158, 0xFFBF5AF2, 0xFFFF9F0A, 0xFF64D2FF, 0xFFFF375F];

/// The colour of a pointer: red for the presenter, else one of the guest colours, always the
/// same for the same guest.
pub fn colour(who: &str, guest: bool) -> u32 {
    if !guest {
        return OWN;
    }
    let h = who.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3));
    GUESTS[(h % GUESTS.len() as u64) as usize]
}

/// How long the trail is and how it fades.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    /// How long a point stays in the trail (ms); infinite: the whole way while the pen is down.
    pub trail_ms: f32,
    /// How long the trail takes to fade out once the pen lifted (ms).
    pub fade_ms: f32,
    /// Trail width at the dot (screen px); it thins to nothing at the end.
    pub width: f32,
    /// The dot's radius (screen px); its glow reaches three times as far.
    pub dot: f32,
}

impl Style {
    /// The short trail (the default): the last moment of the pen's way.
    pub const SHORT: Style = Style { trail_ms: 650.0, fade_ms: 450.0, width: 4.0, dot: 4.5 };
    /// A few seconds of it.
    pub const LONG: Style = Style { trail_ms: 4000.0, fade_ms: 1500.0, width: 4.0, dot: 4.5 };
    /// All of it while the pen is down.
    pub const KEEP: Style = Style { trail_ms: f32::INFINITY, fade_ms: 2000.0, width: 4.0, dot: 4.5 };

    /// By the setting's name ("short", "long", "keep").
    pub fn named(name: &str) -> Style {
        match name {
            "long" => Style::LONG,
            "keep" => Style::KEEP,
            _ => Style::SHORT,
        }
    }
}

/// One piece of the trail: from (x0, y0) to (x1, y1) in page points, `width` screen px wide
/// (round ends), `alpha` 0..1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub width: f32,
    pub alpha: f32,
}

/// What to draw for one pointer now.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Look {
    /// The trail, oldest piece first. Draw each twice: a glow 3 × as wide at a quarter of the
    /// opacity, then the core.
    pub pieces: Vec<Piece>,
    /// The dot at the pen (page points) with its opacity, while the pen is down or fading.
    pub dot: Option<(f32, f32, f32)>,
    /// Nothing left to draw: the renderer can forget this pointer.
    pub gone: bool,
}

impl Look {
    /// The trail as four-cornered pieces that join without overlapping (so faint ones don't
    /// add up where they meet): corners x, y (page points) in order around, then the alpha. At
    /// `px` screen pixels per page point; `widen` scales the width (3 for the glow).
    pub fn quads(&self, px: f32, widen: f32) -> Vec<[f32; 9]> {
        let n = self.pieces.len();
        if n == 0 || !(px > 0.0) {
            return Vec::new();
        }
        // Vertices along the trail, each with its half width (page points) and a normal that
        // halves the angle between the pieces meeting there.
        let mut verts: Vec<(f32, f32)> = self.pieces.iter().map(|p| (p.x0, p.y0)).collect();
        verts.push((self.pieces[n - 1].x1, self.pieces[n - 1].y1));
        let half = |i: usize| {
            let w = match (i.checked_sub(1).map(|k| self.pieces[k].width), self.pieces.get(i).map(|p| p.width)) {
                (Some(a), Some(b)) => (a + b) * 0.5,
                (a, b) => a.or(b).unwrap_or(0.0),
            };
            w * widen * 0.5 / px
        };
        let dir = |i: usize| {
            let p = &self.pieces[i];
            let (dx, dy) = (p.x1 - p.x0, p.y1 - p.y0);
            let l = dx.hypot(dy);
            if l < 1e-6 { None } else { Some((dx / l, dy / l)) }
        };
        let mut last = (1.0f32, 0.0f32);
        let normals: Vec<(f32, f32)> = (0..=n)
            .map(|i| {
                let a = i.checked_sub(1).and_then(dir);
                let b = if i < n { dir(i) } else { None };
                let d = match (a, b) {
                    (Some(a), Some(b)) => {
                        let (x, y) = (a.0 + b.0, a.1 + b.1);
                        let l = x.hypot(y);
                        if l < 1e-3 { b } else { (x / l, y / l) }
                    }
                    (Some(a), None) => a,
                    (None, Some(b)) => b,
                    (None, None) => last,
                };
                last = d;
                (-d.1, d.0)
            })
            .collect();
        let corner = |i: usize, side: f32| {
            let (nx, ny) = normals[i];
            let h = half(i) * side;
            (verts[i].0 + nx * h, verts[i].1 + ny * h)
        };
        (0..n)
            .map(|i| {
                let (l0, l1, r1, r0) = (corner(i, 1.0), corner(i + 1, 1.0), corner(i + 1, -1.0), corner(i, -1.0));
                [l0.0, l0.1, l1.0, l1.1, r1.0, r1.1, r0.0, r0.1, self.pieces[i].alpha]
            })
            .collect()
    }
}

/// The pointer's look at `now` from its samples (x, y, t; oldest first), `lifted` the time the
/// pen lifted (None while it is down).
pub fn ribbon(samples: &[(f32, f32, f32)], now: f32, lifted: Option<f32>, style: Style) -> Look {
    let fade = match lifted {
        None => 1.0,
        Some(t) => (1.0 - (now - t) / style.fade_ms.max(1.0)).clamp(0.0, 1.0),
    };
    let live: Vec<(f32, f32, f32)> = samples.iter().copied().filter(|p| p.0.is_finite() && p.1.is_finite() && p.2.is_finite() && now - p.2 <= style.trail_ms && p.2 <= now + 50.0).collect();
    if fade <= 0.0 || live.is_empty() {
        return Look { gone: fade <= 0.0 || samples.is_empty() || lifted.is_some(), ..Look::default() };
    }
    let curve = smooth(&live);
    let mut pieces = Vec::with_capacity(curve.len());
    for w in curve.windows(2) {
        let (a, b) = (w[0], w[1]);
        // Age of the piece: 0 at the pen, 1 at the end of the trail.
        let age = if style.trail_ms.is_finite() { ((now - (a.2 + b.2) * 0.5) / style.trail_ms).clamp(0.0, 1.0) } else { 0.0 };
        let left = 1.0 - age;
        pieces.push(Piece { x0: a.0, y0: a.1, x1: b.0, y1: b.1, width: style.width * left.powf(0.6).max(0.15), alpha: fade * left.powf(1.4) });
    }
    let last = live[live.len() - 1];
    Look { pieces, dot: Some((last.0, last.1, fade)), gone: false }
}

/// A smooth curve through the samples: Catmull-Rom, four steps between two samples (samples
/// closer than half a point merge first, so a resting pen doesn't make knots).
fn smooth(pts: &[(f32, f32, f32)]) -> Vec<(f32, f32, f32)> {
    let mut p: Vec<(f32, f32, f32)> = Vec::with_capacity(pts.len());
    for &q in pts {
        match p.last_mut() {
            Some(l) if (q.0 - l.0).hypot(q.1 - l.1) < 0.5 => l.2 = q.2,
            _ => p.push(q),
        }
    }
    if p.len() < 3 {
        return p;
    }
    let n = p.len();
    let mut out = Vec::with_capacity(n * 4);
    for i in 0..n - 1 {
        let (p0, p1, p2, p3) = (p[i.saturating_sub(1)], p[i], p[i + 1], p[(i + 2).min(n - 1)]);
        for k in 0..4 {
            let t = k as f32 / 4.0;
            let (t2, t3) = (t * t, t * t * t);
            let c = |a: f32, b: f32, c: f32, d: f32| 0.5 * (2.0 * b + (c - a) * t + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2 + (3.0 * b - a - 3.0 * c + d) * t3);
            out.push((c(p0.0, p1.0, p2.0, p3.0), c(p0.1, p1.1, p2.1, p3.1), p1.2 + (p2.2 - p1.2) * t));
        }
    }
    out.push(p[n - 1]);
    out
}

/// Remote pointers arrive in bursts (every 30 ms or so, sometimes later): their samples carry
/// the pointing device's clock. A [`Replay`] maps them onto this device's clock a little late,
/// so the pointer moves as smoothly as it was drawn instead of jumping from burst to burst.
#[derive(Debug, Clone, Default)]
pub struct Replay {
    /// Local time minus remote time, settled on the first burst.
    offset: Option<f32>,
    /// The samples on the local clock.
    pub samples: Vec<(f32, f32, f32)>,
    /// When the pen lifted (local clock).
    pub lifted: Option<f32>,
    last_remote: f32,
}

/// How late remote pointers are shown (ms): enough to bridge a late burst.
pub const DELAY_MS: f32 = 90.0;

impl Replay {
    /// Takes a burst received at `now` (local clock); `down` false: the pen lifted after it.
    pub fn add(&mut self, burst: &[(f32, f32, f32)], down: bool, now: f32) {
        let fresh: Vec<(f32, f32, f32)> = burst.iter().copied().filter(|p| p.0.is_finite() && p.1.is_finite() && p.2.is_finite() && (self.offset.is_none() || p.2 > self.last_remote)).collect();
        if let Some(&(_, _, t)) = fresh.last() {
            // A new stroke after a lift, or a clock far off: start over.
            let off = now + DELAY_MS - t;
            if self.offset.is_none_or(|o| self.lifted.is_some() || (o - off).abs() > 1000.0) {
                self.offset = Some(off);
                self.samples.clear();
                self.lifted = None;
            }
            let o = self.offset.unwrap_or(off);
            self.samples.extend(fresh.iter().map(|&(x, y, t)| (x, y, t + o)));
            self.last_remote = t;
        }
        if !down {
            self.lifted = Some(self.samples.last().map_or(now, |s| s.2.max(now)));
        }
        // Keep it bounded (a pointer held down for minutes).
        if self.samples.len() > 4000 {
            self.samples.drain(..self.samples.len() - 4000);
        }
    }

    /// The look at `now` (local clock): only what has "happened" by then.
    pub fn look(&self, now: f32, style: Style) -> Look {
        let shown: Vec<(f32, f32, f32)> = self.samples.iter().copied().filter(|s| s.2 <= now).collect();
        let lifted = self.lifted.filter(|&t| t <= now);
        if shown.is_empty() {
            return Look { gone: self.samples.is_empty() && self.lifted.is_some(), ..Look::default() };
        }
        ribbon(&shown, now, lifted, style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(n: usize, t0: f32) -> Vec<(f32, f32, f32)> {
        (0..n).map(|i| (i as f32 * 5.0, (i as f32 * 0.7).sin() * 4.0, t0 + i as f32 * 16.0)).collect()
    }

    #[test]
    fn the_trail_thins_and_fades_towards_its_end_and_the_dot_is_at_the_pen() {
        let s = line(30, 0.0);
        let now = s[29].2;
        let l = ribbon(&s, now, None, Style::SHORT);
        assert!(l.pieces.len() > 60, "smooth: several pieces between two samples");
        let (first, last) = (l.pieces[0], *l.pieces.last().unwrap());
        assert!(first.width < last.width && first.alpha < last.alpha, "{first:?} {last:?}");
        assert_eq!(l.dot.map(|d| (d.0, d.1)), Some((s[29].0, s[29].1)));
        assert!(!l.gone);
    }

    #[test]
    fn old_points_leave_the_trail_and_a_lift_fades_it_out() {
        let s = line(30, 0.0);
        // 650 ms later only the newest samples are left.
        let later = ribbon(&s, s[29].2 + 500.0, None, Style::SHORT);
        assert!(later.pieces.len() < ribbon(&s, s[29].2, None, Style::SHORT).pieces.len());
        let lift = s[29].2;
        let half = ribbon(&s, lift + 225.0, Some(lift), Style::SHORT);
        assert!((half.dot.unwrap().2 - 0.5).abs() < 0.01, "half faded");
        let done = ribbon(&s, lift + 500.0, Some(lift), Style::SHORT);
        assert!(done.gone && done.pieces.is_empty());
        // Kept: nothing ages while the pen is down.
        let keep = ribbon(&s, s[29].2 + 60_000.0, None, Style::KEEP);
        assert!(keep.pieces.iter().all(|p| p.alpha == 1.0));
    }

    #[test]
    fn a_remote_pointer_plays_back_smoothly_on_the_local_clock() {
        let mut r = Replay::default();
        // The remote clock is far from ours; bursts of two samples every 32 ms.
        let remote: Vec<(f32, f32, f32)> = (0..20).map(|i| (i as f32 * 3.0, 0.0, 90_000.0 + i as f32 * 16.0)).collect();
        for (k, burst) in remote.chunks(2).enumerate() {
            r.add(burst, true, 1000.0 + k as f32 * 32.0 + 16.0);
        }
        // At first only part of it has "happened"; later all of it.
        let early = r.look(1000.0 + 16.0 + DELAY_MS + 40.0, Style::SHORT);
        let late = r.look(1000.0 + 20.0 * 16.0 + DELAY_MS, Style::SHORT);
        assert!(early.dot.unwrap().0 < late.dot.unwrap().0);
        assert_eq!(late.dot.unwrap().0, 57.0);
        // A duplicate burst (it came twice, directly and passed on) adds nothing.
        let n = r.samples.len();
        r.add(&remote[..4], true, 1500.0);
        assert_eq!(r.samples.len(), n);
        // The lift fades it out; then it's gone.
        r.add(&[], false, 1500.0);
        assert!(r.look(5000.0, Style::SHORT).gone);
    }

    #[test]
    fn the_trail_is_drawn_as_pieces_that_meet_without_overlapping() {
        let s = line(30, 0.0);
        let l = ribbon(&s, s[29].2, None, Style::SHORT);
        let q = l.quads(2.0, 1.0);
        assert_eq!(q.len(), l.pieces.len());
        for w in q.windows(2) {
            // This piece's far corners are the next one's near corners.
            assert_eq!((w[0][2], w[0][3], w[0][4], w[0][5]), (w[1][0], w[1][1], w[1][6], w[1][7]));
        }
        // At the pen the trail is `width` screen px wide: 2 page points at 2 px per point.
        let last = q[q.len() - 1];
        let w = (last[2] - last[4]).hypot(last[3] - last[5]);
        assert!((w - 2.0).abs() < 0.3, "{w}");
        assert!(l.quads(0.0, 1.0).is_empty());
    }

    #[test]
    fn guests_get_their_own_steady_colours() {
        assert_eq!(colour("anyone", false), OWN);
        let c = colour("guest-1", true);
        assert_ne!(c, OWN);
        assert_eq!(c, colour("guest-1", true));
    }

    #[test]
    fn hostile_samples_draw_nothing_bad() {
        let bad = [(f32::NAN, 0.0, 0.0), (0.0, f32::INFINITY, 1.0), (1.0, 1.0, f32::NAN)];
        let l = ribbon(&bad, 10.0, None, Style::SHORT);
        assert!(l.pieces.is_empty() && l.dot.is_none());
        let mut r = Replay::default();
        r.add(&bad, true, 0.0);
        assert!(r.samples.is_empty());
    }
}
