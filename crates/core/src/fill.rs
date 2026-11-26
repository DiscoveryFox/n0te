//! The paint bucket: the area a tap fills, bounded by ink. The ink (its outlines, so a thick
//! line is a thick wall) goes onto a grid, gaps narrower than a few points close, the area
//! reachable from the tap is found and its outline traced, holes included (an island of ink
//! in it stays clear). If the area runs out to the edge it isn't closed and nothing is filled.
//! The result is the point list of a [`crate::ink::KIND_FILL`] stroke: the outline with its
//! holes joined in by zero-width links (filled with the non-zero rule).

use std::collections::HashMap;

use crate::ink::Rect;

/// The area a tap at (x, y) fills: everything reachable from it inside `area` without crossing
/// `walls` (filled outline polygons, x, y pairs) or gaps in them narrower than 2 × `gap`. None
/// when it reaches the edge of `area` (not closed) or the tap is on ink.
pub fn region(walls: &[Vec<f32>], x: f32, y: f32, area: Rect, gap: f32) -> Option<Vec<(f32, f32)>> {
    if area.is_empty() || !area.contains(x, y) || !(area.width() * area.height()).is_finite() {
        return None;
    }
    let cell = (area.width().max(area.height()) / 600.0).max(0.5);
    let w = (area.width() / cell).ceil() as usize + 2;
    let h = (area.height() / cell).ceil() as usize + 2;
    // Cell (i, j) covers x0 + (i - 1) * cell … ; a ring of free cells all round.
    let (x0, y0) = (area.x0 - cell, area.y0 - cell);
    let mut ink = vec![false; w * h];
    for poly in walls {
        let pts: Vec<(f32, f32)> = poly.chunks_exact(2).map(|c| (c[0], c[1])).filter(|p| p.0.is_finite() && p.1.is_finite()).collect();
        if pts.len() < 3 {
            continue;
        }
        let (py0, py1) = pts.iter().fold((f32::MAX, f32::MIN), |a, p| (a.0.min(p.1), a.1.max(p.1)));
        let j0 = (((py0 - y0) / cell).floor().max(0.0)) as usize;
        let j1 = (((py1 - y0) / cell).ceil() as usize).min(h - 1);
        for j in j0..=j1 {
            let cy = y0 + (j as f32 + 0.5) * cell;
            for (a, b) in row_spans(&pts, cy) {
                let i0 = (((a - x0) / cell).round().max(0.0)) as usize;
                let i1 = (((b - x0) / cell).round() as usize).min(w);
                for i in i0..i1.max(i0 + 1).min(w) {
                    ink[j * w + i] = true;
                }
            }
        }
    }
    // Close gaps: ink grows by `gap`.
    let free: Vec<bool> = ink.iter().map(|&i| !i).collect();
    let to_ink = crate::scribble::edt(&free, w, h);
    let wall: Vec<bool> = to_ink.iter().map(|&d| d * cell <= gap).collect();
    let (ti, tj) = (((x - x0) / cell) as usize, ((y - y0) / cell) as usize);
    if ti >= w || tj >= h || wall[tj * w + ti] {
        return None;
    }
    // The area from the tap; reaching the border ring means it isn't closed.
    let mut filled = vec![false; w * h];
    let mut stack = vec![(ti, tj)];
    filled[tj * w + ti] = true;
    while let Some((i, j)) = stack.pop() {
        if i == 0 || j == 0 || i + 1 >= w || j + 1 >= h {
            return None;
        }
        for (ni, nj) in [(i - 1, j), (i + 1, j), (i, j - 1), (i, j + 1)] {
            let k = nj * w + ni;
            if !filled[k] && !wall[k] {
                filled[k] = true;
                stack.push((ni, nj));
            }
        }
    }
    // Back under the ink by what the gap took (the ink is drawn over the fill).
    let outside: Vec<bool> = filled.iter().map(|&f| !f).collect();
    let to_fill = crate::scribble::edt(&outside, w, h);
    let grown: Vec<bool> = to_fill.iter().map(|&d| d * cell <= gap + cell).collect();
    let loops = trace(&grown, w, h);
    let loops: Vec<Vec<(f32, f32)>> = loops
        .into_iter()
        .map(|l| simplify(&l.iter().map(|&(i, j)| (x0 + i as f32 * cell, y0 + j as f32 * cell)).collect::<Vec<_>>(), cell * 0.75))
        .filter(|l| l.len() >= 3)
        .collect();
    if loops.is_empty() {
        return None;
    }
    Some(join(&loops))
}

/// Where a row at `y` is inside a polygon (non-zero rule).
fn row_spans(poly: &[(f32, f32)], y: f32) -> Vec<(f32, f32)> {
    let n = poly.len();
    let mut cross: Vec<(f32, i32)> = Vec::new();
    for i in 0..n {
        let (a, b) = (poly[i], poly[(i + 1) % n]);
        if (a.1 <= y && b.1 > y) || (b.1 <= y && a.1 > y) {
            cross.push((a.0 + (y - a.1) / (b.1 - a.1) * (b.0 - a.0), if b.1 > a.1 { 1 } else { -1 }));
        }
    }
    cross.sort_by(|p, q| p.0.total_cmp(&q.0));
    let (mut wn, mut start, mut out) = (0, 0.0, Vec::new());
    for (x, d) in cross {
        let was = wn;
        wn += d;
        if was == 0 && wn != 0 {
            start = x;
        } else if was != 0 && wn == 0 {
            out.push((start, x));
        }
    }
    out
}

/// The outlines of the set cells, along cell edges: outer ones one way round, holes the other
/// (so the non-zero rule fills the area and leaves the holes).
fn trace(mask: &[bool], w: usize, h: usize) -> Vec<Vec<(i32, i32)>> {
    let set = |i: i32, j: i32| i >= 0 && j >= 0 && (i as usize) < w && (j as usize) < h && mask[j as usize * w + i as usize];
    let mut edges: Vec<((i32, i32), (i32, i32))> = Vec::new();
    for j in 0..h as i32 {
        for i in 0..w as i32 {
            if !set(i, j) {
                continue;
            }
            if !set(i - 1, j) {
                edges.push(((i, j), (i, j + 1)));
            }
            if !set(i, j + 1) {
                edges.push(((i, j + 1), (i + 1, j + 1)));
            }
            if !set(i + 1, j) {
                edges.push(((i + 1, j + 1), (i + 1, j)));
            }
            if !set(i, j - 1) {
                edges.push(((i + 1, j), (i, j)));
            }
        }
    }
    let mut from: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (k, e) in edges.iter().enumerate() {
        from.entry(e.0).or_default().push(k);
    }
    let mut used = vec![false; edges.len()];
    let mut loops = Vec::new();
    for start in 0..edges.len() {
        if used[start] {
            continue;
        }
        let mut l = Vec::new();
        let mut k = start;
        loop {
            used[k] = true;
            l.push(edges[k].0);
            let next = from.get(&edges[k].1).and_then(|v| v.iter().copied().find(|&n| !used[n]));
            match next {
                Some(n) => k = n,
                None => break,
            }
        }
        loops.push(l);
    }
    loops
}

/// A closed outline with points on straight runs dropped (Douglas–Peucker, `eps` points).
fn simplify(l: &[(f32, f32)], eps: f32) -> Vec<(f32, f32)> {
    if l.len() < 4 {
        return l.to_vec();
    }
    // Split the loop at its first point and the point farthest from it.
    let far = (1..l.len()).max_by(|&a, &b| dist(l[0], l[a]).total_cmp(&dist(l[0], l[b]))).unwrap_or(1);
    let mut out = dp(&l[..=far], eps);
    let mut back: Vec<(f32, f32)> = l[far..].to_vec();
    back.push(l[0]);
    let second = dp(&back, eps);
    out.pop();
    out.extend_from_slice(&second[..second.len() - 1]);
    out
}

fn dp(p: &[(f32, f32)], eps: f32) -> Vec<(f32, f32)> {
    if p.len() < 3 {
        return p.to_vec();
    }
    let (a, b) = (p[0], p[p.len() - 1]);
    let (mut idx, mut dmax) = (0, 0.0);
    for (i, &q) in p.iter().enumerate().take(p.len() - 1).skip(1) {
        let d = seg_dist(q, a, b);
        if d > dmax {
            (idx, dmax) = (i, d);
        }
    }
    if dmax <= eps {
        return vec![a, b];
    }
    let mut left = dp(&p[..=idx], eps);
    let right = dp(&p[idx..], eps);
    left.pop();
    left.extend(right);
    left
}

fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn seg_dist(q: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 < 1e-9 { 0.0 } else { (((q.0 - a.0) * dx + (q.1 - a.1) * dy) / l2).clamp(0.0, 1.0) };
    dist(q, (a.0 + dx * t, a.1 + dy * t))
}

/// Several outlines as one polygon: each in turn, linked through their first points and back
/// the same way (the links have no area).
fn join(loops: &[Vec<(f32, f32)>]) -> Vec<(f32, f32)> {
    let mut out = Vec::new();
    for l in loops {
        out.extend_from_slice(l);
        out.push(l[0]);
    }
    for l in loops.iter().rev().skip(1) {
        out.push(l[0]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ink::{Brush, InkPoint, KIND_FILL, outline, outline_mesh};

    /// A rectangle's outline as a wall `w` wide (x, y pairs of a filled polygon with a hole).
    fn frame(x0: f32, y0: f32, x1: f32, y1: f32, w: f32) -> Vec<f32> {
        let o = [(x0 - w, y0 - w), (x1 + w, y0 - w), (x1 + w, y1 + w), (x0 - w, y1 + w)];
        let i = [(x0 + w, y0 + w), (x0 + w, y1 - w), (x1 - w, y1 - w), (x1 - w, y0 + w)];
        let mut v = Vec::new();
        for p in o.iter().chain(&[o[0], i[0]]).chain(&i).chain(&[i[0], o[0]]) {
            v.extend_from_slice(&[p.0, p.1]);
        }
        v
    }

    fn winding(x: f32, y: f32, poly: &[(f32, f32)]) -> i32 {
        let n = poly.len();
        let mut w = 0;
        for i in 0..n {
            let (a, b) = (poly[i], poly[(i + 1) % n]);
            let cross = (b.0 - a.0) * (y - a.1) - (x - a.0) * (b.1 - a.1);
            if a.1 <= y && b.1 > y && cross > 0.0 {
                w += 1;
            } else if b.1 <= y && a.1 > y && cross < 0.0 {
                w -= 1;
            }
        }
        w
    }

    #[test]
    fn a_tap_inside_a_box_fills_the_box_and_leaves_an_island_clear() {
        let page = Rect::new(0.0, 0.0, 400.0, 400.0);
        let walls = vec![frame(100.0, 100.0, 300.0, 250.0, 1.0), frame(180.0, 160.0, 220.0, 190.0, 1.0)];
        let poly = region(&walls, 120.0, 120.0, page, 3.0).expect("closed");
        assert_ne!(winding(150.0, 200.0, &poly), 0, "inside the box");
        assert_eq!(winding(350.0, 350.0, &poly), 0, "outside");
        assert_eq!(winding(200.0, 175.0, &poly), 0, "the island's inside stays clear");
        // Under the ink but not past it.
        assert_ne!(winding(100.5, 200.0, &poly), 0);
        assert_eq!(winding(97.0, 200.0, &poly), 0);
        // As a stroke: drawn the same by every renderer.
        let pts: Vec<InkPoint> = poly.iter().map(|&(x, y)| InkPoint { x, y, pressure: 0.5, t_ms: 0 }).collect();
        let b = Brush { width: 1.0, thinning: 0.0, color: 0x55D71921, kind: KIND_FILL };
        assert!(outline(&pts, &b).len() >= 8);
        let (mut xy, mut idx) = (Vec::new(), Vec::new());
        outline_mesh(&pts, &b, &mut xy, &mut idx);
        assert!(idx.len() > 100);
        // Hatched: bars only inside.
        let hatched = outline(&pts, &b.with_dash(crate::ink::DASH_DASHED));
        assert!(hatched.len() > 40);
    }

    #[test]
    fn an_open_shape_or_a_tap_on_ink_fills_nothing_and_small_gaps_close() {
        let page = Rect::new(0.0, 0.0, 400.0, 400.0);
        assert!(region(&[], 200.0, 200.0, page, 3.0).is_none(), "open: runs out to the edge");
        let walls = vec![frame(100.0, 100.0, 300.0, 250.0, 1.0)];
        assert!(region(&walls, 100.0, 175.0, page, 3.0).is_none(), "on the ink");
        // A gap of 4 points in the frame (a hand-drawn box that didn't quite close) is closed.
        let mut gappy = vec![];
        let seg = |x0: f32, y0: f32, x1: f32, y1: f32| vec![x0, y0, x1, y0, x1, y1, x0, y1];
        gappy.push(seg(99.0, 99.0, 301.0, 101.0)); // top
        gappy.push(seg(99.0, 249.0, 301.0, 251.0)); // bottom
        gappy.push(seg(299.0, 99.0, 301.0, 251.0)); // right
        gappy.push(seg(99.0, 99.0, 101.0, 170.0)); // left, upper
        gappy.push(seg(99.0, 174.0, 101.0, 251.0)); // left, lower (gap 170–174)
        assert!(region(&gappy, 200.0, 175.0, page, 3.0).is_some());
        assert!(region(&gappy, 200.0, 175.0, page, 1.0).is_none(), "too small a gap setting: it leaks");
    }
}
