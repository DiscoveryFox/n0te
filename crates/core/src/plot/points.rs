//! Special points of a plot's functions: zeros, highs and lows, turning points, where two
//! graphs cross. Found numerically on the shown x range (the derivatives exactly, from the
//! expressions), poles told apart from zeros.

use super::expr::Expr;
use super::Plot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointKind {
    /// Where a graph meets the x axis.
    Zero,
    /// A high point (local maximum).
    High,
    /// A low point (local minimum).
    Low,
    /// Where the bend changes (Wendepunkt).
    Turn,
    /// Where two graphs cross.
    Cross,
}

/// One point found on the plot's functions (in plot units), with the function(s) it is on.
#[derive(Debug, Clone, PartialEq)]
pub struct SpecialPoint {
    pub kind: PointKind,
    pub x: f64,
    pub y: f64,
    pub on: usize,
    /// The other function (where two cross).
    pub with: Option<usize>,
}

/// Where `f` is zero on [a, b]: sign changes on a fine grid refined by bisection (a pole, where
/// the values run off instead of shrinking, is not a zero), plus exact zeros on the grid.
pub fn zeros_of(f: &dyn Fn(f64) -> f64, a: f64, b: f64) -> Vec<f64> {
    let n = 1600;
    let tol = (b - a) * 1e-13;
    let xs: Vec<f64> = (0..=n).map(|i| a + (b - a) * i as f64 / n as f64).collect();
    let fs: Vec<f64> = xs.iter().map(|&x| f(x)).collect();
    // A value on the grid that is nothing next to its neighbours (sin at π: a rounding rest) is
    // a zero right there.
    let on_grid: Vec<bool> = (0..=n)
        .map(|i| {
            let v = fs[i];
            if v == 0.0 {
                return true;
            }
            let near = |j: Option<usize>| j.and_then(|j| fs.get(j)).filter(|w| w.is_finite()).map_or(0.0, |w| w.abs());
            v.is_finite() && v.abs() <= 1e-12 * (near(i.checked_sub(1)) + near(Some(i + 1)))
        })
        .collect();
    let mut out: Vec<f64> = Vec::new();
    let push = |x: f64, out: &mut Vec<f64>| {
        if out.last().is_none_or(|l| (x - l).abs() > (b - a) * 1e-7) {
            out.push(x);
        }
    };
    for i in 0..=n {
        if on_grid[i] {
            push(xs[i], &mut out);
            continue;
        }
        if i == n || on_grid[i + 1] {
            continue;
        }
        let (x0, x1, f0, f1) = (xs[i], xs[i + 1], fs[i], fs[i + 1]);
        if !(f0.is_finite() && f1.is_finite() && f0.signum() != f1.signum()) {
            continue;
        }
        let (mut lo, mut hi, mut flo) = (x0, x1, f0);
        for _ in 0..80 {
            let mid = (lo + hi) / 2.0;
            let fm = f(mid);
            if !fm.is_finite() {
                break;
            }
            if fm.signum() == flo.signum() {
                lo = mid;
                flo = fm;
            } else {
                hi = mid;
            }
            if hi - lo < tol {
                break;
            }
        }
        let r = (lo + hi) / 2.0;
        // A real zero: the value there is tiny next to the values around it (at a pole it runs
        // off, at a step it stays as big).
        let fr = f(r);
        if fr.is_finite() && (fr == 0.0 || fr.abs() <= 1e-6 * f0.abs().max(f1.abs())) {
            push(r, &mut out);
        }
    }
    out
}

/// The special points of `kinds` on the plot's functions (`curves`, resolved), inside the box.
pub fn special_points(p: &Plot, curves: &[Option<Expr>], zeros: bool, extrema: bool, turns: bool, crossings: bool) -> Vec<SpecialPoint> {
    let mut out = Vec::new();
    let (a, b) = (p.x_min, p.x_max);
    let seen = |x: f64, y: f64| y.is_finite() && y >= p.y_min - 1e-9 && y <= p.y_max + 1e-9 && x >= a && x <= b;
    let span = (p.y_max - p.y_min).abs().max(1e-12);
    for (i, e) in curves.iter().enumerate() {
        let Some(e) = e else { continue };
        let f = |x: f64| e.eval(x);
        let d1 = e.derive();
        let mut found_zero: Vec<f64> = Vec::new();
        if zeros {
            for x in zeros_of(&f, a, b) {
                found_zero.push(x);
                out.push(SpecialPoint { kind: PointKind::Zero, x, y: 0.0, on: i, with: None });
            }
        }
        if extrema || zeros {
            for x in zeros_of(&|x| d1.eval(x), a, b) {
                let (l, r) = (d1.eval(x - (b - a) * 1e-5), d1.eval(x + (b - a) * 1e-5));
                let y = f(x);
                if !y.is_finite() || !l.is_finite() || !r.is_finite() {
                    continue;
                }
                let kind = if l > 0.0 && r < 0.0 {
                    PointKind::High
                } else if l < 0.0 && r > 0.0 {
                    PointKind::Low
                } else {
                    continue;
                };
                // Touching the axis (x² at 0) is a zero too.
                if zeros && y.abs() < span * 1e-6 && !found_zero.iter().any(|z| (z - x).abs() < (b - a) * 1e-4) {
                    out.push(SpecialPoint { kind: PointKind::Zero, x, y: 0.0, on: i, with: None });
                }
                if extrema && seen(x, y) {
                    out.push(SpecialPoint { kind, x, y, on: i, with: None });
                }
            }
        }
        if turns {
            let d2 = d1.derive();
            for x in zeros_of(&|x| d2.eval(x), a, b) {
                let (l, r) = (d2.eval(x - (b - a) * 1e-5), d2.eval(x + (b - a) * 1e-5));
                let y = f(x);
                if l.is_finite() && r.is_finite() && l.signum() != r.signum() && seen(x, y) {
                    out.push(SpecialPoint { kind: PointKind::Turn, x, y, on: i, with: None });
                }
            }
        }
        if crossings {
            for (j, g) in curves.iter().enumerate().skip(i + 1) {
                let Some(g) = g else { continue };
                for x in zeros_of(&|x| e.eval(x) - g.eval(x), a, b) {
                    let y = f(x);
                    if seen(x, y) {
                        out.push(SpecialPoint { kind: PointKind::Cross, x, y, on: i, with: Some(j) });
                    }
                }
            }
        }
    }
    out.retain(|s| seen(s.x, s.y));
    out
}

/// ∫ |f − g| over [a, b] (g = 0: the area between f and the x axis), Simpson's rule.
pub fn area(f: &Expr, g: Option<&Expr>, a: f64, b: f64) -> f64 {
    let n = 2000;
    let h = (b - a) / n as f64;
    let v = |x: f64| {
        let d = f.eval(x) - g.map_or(0.0, |g| g.eval(x));
        if d.is_finite() { d.abs() } else { 0.0 }
    };
    let mut s = v(a) + v(b);
    for i in 1..n {
        s += v(a + h * i as f64) * if i % 2 == 1 { 4.0 } else { 2.0 };
    }
    s * h / 3.0
}

#[cfg(test)]
mod tests {
    use super::super::expr::parse;
    use super::*;

    fn plot(fns: &[&str]) -> (Plot, Vec<Option<Expr>>) {
        let p = Plot::centered(0.0, 0.0, 240.0, 160.0, 20.0, 0, 1.0);
        (p, fns.iter().map(|f| Some(parse(f).unwrap())).collect())
    }

    fn near(v: f64, w: f64) -> bool {
        (v - w).abs() < 1e-6
    }

    #[test]
    fn zeros_extrema_turns_and_crossings() {
        // x³ − 3x: zeros at 0, ±√3; high at −1 (2), low at 1 (−2); turning point at 0.
        let (p, c) = plot(&["x^3 - 3x"]);
        let s = special_points(&p, &c, true, true, true, false);
        let of = |k: PointKind| s.iter().filter(|q| q.kind == k).map(|q| (q.x, q.y)).collect::<Vec<_>>();
        let z = of(PointKind::Zero);
        assert_eq!(z.len(), 3, "{z:?}");
        assert!(near(z[0].0, -3f64.sqrt()) && near(z[1].0, 0.0) && near(z[2].0, 3f64.sqrt()));
        assert_eq!(of(PointKind::High).len(), 1);
        assert!(near(of(PointKind::High)[0].0, -1.0) && near(of(PointKind::High)[0].1, 2.0));
        assert!(near(of(PointKind::Low)[0].0, 1.0) && near(of(PointKind::Low)[0].1, -2.0));
        assert!(near(of(PointKind::Turn)[0].0, 0.0));
        // x² touches the axis at 0: a zero (and a low).
        // Steeper zeros (and those of a derivative) are found too.
        let (p, c) = plot(&["0,25x^3 - 2x", "0,75x^2 - 2"]);
        let s = special_points(&p, &c, true, true, false, false);
        assert_eq!(s.iter().filter(|q| q.on == 0 && matches!(q.kind, PointKind::High | PointKind::Low)).count(), 2, "{s:?}");
        assert_eq!(s.iter().filter(|q| q.on == 1 && q.kind == PointKind::Zero).count(), 2, "{s:?}");
        let (p, c) = plot(&["x^2"]);
        let s = special_points(&p, &c, true, true, false, false);
        assert_eq!(s.iter().filter(|q| q.kind == PointKind::Zero).count(), 1);
        assert_eq!(s.iter().filter(|q| q.kind == PointKind::Low).count(), 1);
        // Zeros right on the sampling grid (a rounding rest there, not exactly 0) are found.
        let (p, c) = plot(&["x - 0,3", "sin x - cos x"]);
        let s = special_points(&p, &c, true, false, false, false);
        assert_eq!(s.iter().filter(|q| q.on == 0).count(), 1, "{s:?}");
        assert_eq!(s.iter().filter(|q| q.on == 1).count(), 4, "{s:?}");
        let near_pi = zeros_of(&|x: f64| x.sin(), -2.0 * std::f64::consts::PI, 2.0 * std::f64::consts::PI);
        assert_eq!(near_pi.len(), 5, "{near_pi:?}");
        // 1/x has no zero (the sign change at 0 is a pole).
        let (p, c) = plot(&["1/x", "tan x"]);
        let s = special_points(&p, &c, true, false, false, false);
        let tz: Vec<f64> = s.iter().filter(|q| q.on == 1).map(|q| q.x).collect();
        assert!(s.iter().all(|q| q.on == 1), "1/x has no zeros: {s:?}");
        // tan: zeros at −π, 0, π only (not at its poles ±π/2, ±3π/2).
        assert_eq!(tz.len(), 3, "{tz:?}");
        // Two lines cross once.
        let (p, c) = plot(&["x + 1", "-x + 3"]);
        let s = special_points(&p, &c, false, false, false, true);
        assert_eq!(s.len(), 1);
        assert!(near(s[0].x, 1.0) && near(s[0].y, 2.0) && s[0].with == Some(1));
        // Areas: ∫₀² x² = 8/3; between x and x² on [0, 1]: 1/6.
        assert!((area(&parse("x^2").unwrap(), None, 0.0, 2.0) - 8.0 / 3.0).abs() < 1e-9);
        assert!((area(&parse("x").unwrap(), Some(&parse("x^2").unwrap()), 0.0, 1.0) - 1.0 / 6.0).abs() < 1e-9);
    }
}

