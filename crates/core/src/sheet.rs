//! One sheet's content. Objects live in the map "objects", keyed by object id. A stroke is
//! immutable: one binary value (brush, bounds, encoded samples), so loading a sheet is a single
//! pass with no nested containers. Erasing deletes the key; recolouring or moving writes a new
//! value under the same key.

use std::sync::Mutex;

use loro::{Container, ExportMode, LoroDoc, LoroMap, LoroText, LoroValue, UndoManager, UpdateOptions, ValueOrContainer, VersionVector};

use crate::error::{Error, Result};
use crate::ids;
use crate::ink::{self, Brush, InkPoint, Rect};

#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    pub id: String,
    pub brush: Brush,
    pub bounds: Rect,
    pub points: Vec<InkPoint>,
}

/// Typed text on a page: top-left (x, y) and width in page points, height follows the text.
/// Stored as a map in the objects map (`kind: "text"`, collaborative `text`), so two devices
/// typing in the same box merge character by character. Apps that predate text boxes skip it.
#[derive(Debug, Clone, PartialEq)]
pub struct TextBox {
    pub id: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    /// Font size in page points.
    pub size: f32,
    /// Markdown.
    pub text: String,
}

/// Id of the text page body when it is handed out as a [`TextBox`].
pub const BODY_ID: &str = "__body";

pub struct Sheet {
    doc: LoroDoc,
    objects: LoroMap,
    /// Undo/redo of this device's own edits only (a peer's edits are never undone here).
    undo: Mutex<UndoManager>,
}

const STROKE_V1: u8 = 1;

impl Sheet {
    pub fn create(peer: u64) -> Result<Self> {
        Self::from_doc(LoroDoc::new(), peer)
    }

    pub fn load(peer: u64, snapshot: &[u8]) -> Result<Self> {
        let doc = LoroDoc::new();
        doc.import(snapshot)?;
        Self::from_doc(doc, peer)
    }

    fn from_doc(doc: LoroDoc, peer: u64) -> Result<Self> {
        doc.set_peer_id(peer)?;
        let objects = doc.get_map("objects");
        let mut undo = UndoManager::new(&doc);
        undo.set_merge_interval(0); // every committed edit (stroke, erase gesture, lasso move) is one step
        undo.set_max_undo_steps(500);
        Ok(Sheet { doc, objects, undo: Mutex::new(undo) })
    }

    fn um(&self) -> std::sync::MutexGuard<'_, UndoManager> {
        self.undo.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Undoes this device's last edit on the sheet. Returns whether anything changed.
    pub fn undo(&self) -> Result<bool> {
        Ok(self.um().undo()?)
    }
    pub fn redo(&self) -> Result<bool> {
        Ok(self.um().redo()?)
    }
    pub fn can_undo(&self) -> bool {
        self.um().can_undo()
    }
    pub fn can_redo(&self) -> bool {
        self.um().can_redo()
    }

    /// Ids of strokes within `radius` of (x, y) (eraser), measured to the stroke's centre line
    /// plus half its width.
    pub fn hit_test(&self, x: f32, y: f32, radius: f32) -> Vec<String> {
        let area = Rect::new(x - radius, y - radius, x + radius, y + radius);
        self.strokes_in(Some(area))
            .into_iter()
            .filter(|s| {
                let r = radius + s.brush.width * 0.5;
                match s.points.len() {
                    0 => false,
                    1 => (s.points[0].x - x).hypot(s.points[0].y - y) <= r,
                    _ => s.points.windows(2).any(|w| seg_dist(x, y, w[0].x, w[0].y, w[1].x, w[1].y) <= r),
                }
            })
            .map(|s| s.id)
            .collect()
    }

    /// Ids of strokes the lasso (closed polygon, x,y pairs) encloses: at least 60 % of their
    /// samples inside.
    pub fn lasso(&self, polygon: &[f32]) -> Vec<String> {
        if polygon.len() < 6 {
            return Vec::new();
        }
        let mut bb = Rect::EMPTY;
        for xy in polygon.chunks_exact(2) {
            bb = bb.include(xy[0], xy[1]);
        }
        self.strokes_in(Some(bb))
            .into_iter()
            .filter(|s| {
                let inside = s.points.iter().filter(|p| point_in_polygon(p.x, p.y, polygon)).count();
                inside * 10 >= s.points.len() * 6
            })
            .map(|s| s.id)
            .collect()
    }

    /// Adds a finished stroke; returns its id.
    pub fn add_stroke(&self, brush: Brush, points: &[InkPoint]) -> Result<String> {
        if points.is_empty() {
            return Err(Error::Invalid("empty stroke".into()));
        }
        let id = ids::new_id(self.doc.peer_id());
        self.objects.insert(&id, LoroValue::Binary(encode_stroke(&brush, points).into()))?;
        self.doc.commit();
        Ok(id)
    }

    /// Adds several strokes as one edit (one undo step): paste, moving strokes between sheets.
    pub fn add_strokes(&self, items: &[(Brush, Vec<InkPoint>)]) -> Result<Vec<String>> {
        let mut out = Vec::with_capacity(items.len());
        for (brush, points) in items {
            if points.is_empty() {
                continue;
            }
            let id = ids::new_id(self.doc.peer_id());
            self.objects.insert(&id, LoroValue::Binary(encode_stroke(brush, points).into()))?;
            out.push(id);
        }
        self.doc.commit();
        Ok(out)
    }

    /// Ids of strokes the path (x, y pairs) passes within `radius` of (scribble-to-delete).
    pub fn hit_path(&self, path: &[(f32, f32)], radius: f32) -> Vec<String> {
        let Some(area) = path_bounds(path, radius) else { return Vec::new() };
        self.strokes_in(Some(area))
            .into_iter()
            .filter(|s| {
                let r = radius + s.brush.width * 0.5;
                s.points.iter().any(|p| dist_to_path(p.x, p.y, path) <= r)
            })
            .map(|s| s.id)
            .collect()
    }

    /// Area eraser: removes only the parts of strokes under the eraser path (x, y, radius per
    /// point: the tip may grow with speed); a stroke cut in the middle becomes two. One edit
    /// (one undo step). Returns how many strokes changed.
    pub fn erase_area(&self, path: &[(f32, f32, f32)]) -> Result<usize> {
        let rmax = path.iter().map(|p| p.2).fold(0.0f32, f32::max);
        let xy: Vec<(f32, f32)> = path.iter().map(|p| (p.0, p.1)).collect();
        let Some(area) = path_bounds(&xy, rmax) else { return Ok(0) };
        let mut changed = 0;
        for s in self.strokes_in(Some(area)) {
            let extra = s.brush.width * 0.35;
            let pts = resample(&s.points, 0.75);
            let keep: Vec<bool> = pts.iter().map(|p| !under_tip(p.x, p.y, path, extra)).collect();
            if keep.iter().all(|&k| k) {
                continue;
            }
            changed += 1;
            self.objects.delete(&s.id)?;
            let mut run: Vec<InkPoint> = Vec::new();
            let flush = |run: &mut Vec<InkPoint>| -> Result<()> {
                // Keep pieces that are more than a speck.
                if run.len() >= 2 || (run.len() == 1 && s.points.len() == 1) {
                    let id = ids::new_id(self.doc.peer_id());
                    self.objects.insert(&id, LoroValue::Binary(encode_stroke(&s.brush, run).into()))?;
                }
                run.clear();
                Ok(())
            };
            for (p, k) in pts.iter().zip(&keep) {
                if *k {
                    run.push(*p);
                } else {
                    flush(&mut run)?;
                }
            }
            flush(&mut run)?;
        }
        self.doc.commit();
        Ok(changed)
    }

    /// Adds a text box; returns its id.
    pub fn add_text_box(&self, x: f32, y: f32, w: f32, size: f32, text: &str) -> Result<String> {
        let id = ids::new_id(self.doc.peer_id());
        let m = self.objects.insert_container(&id, LoroMap::new())?;
        m.insert("kind", "text")?;
        m.insert("x", x as f64)?;
        m.insert("y", y as f64)?;
        m.insert("w", w.max(20.0) as f64)?;
        m.insert("size", size.clamp(4.0, 200.0) as f64)?;
        let t = m.insert_container("text", LoroText::new())?;
        t.insert(0, text)?;
        self.doc.commit();
        Ok(id)
    }

    fn text_map(&self, id: &str) -> Option<LoroMap> {
        match self.objects.get(id) {
            Some(ValueOrContainer::Container(Container::Map(m))) if map_str(&m, "kind").as_deref() == Some("text") => Some(m),
            _ => None,
        }
    }

    /// Replaces a text box's text (as a minimal edit, so concurrent typing elsewhere survives).
    pub fn set_text(&self, id: &str, text: &str) -> Result<()> {
        let m = self.text_map(id).ok_or_else(|| Error::NotFound(format!("text box {id}")))?;
        let t = match m.get("text") {
            Some(ValueOrContainer::Container(Container::Text(t))) => t,
            _ => m.insert_container("text", LoroText::new())?,
        };
        t.update(text, UpdateOptions::default()).map_err(|e| Error::Invalid(format!("{e:?}")))?;
        self.doc.commit();
        Ok(())
    }

    /// Moves / resizes a text box (any of position, width, font size).
    pub fn place_text_box(&self, id: &str, x: f32, y: f32, w: f32, size: f32) -> Result<()> {
        let m = self.text_map(id).ok_or_else(|| Error::NotFound(format!("text box {id}")))?;
        m.insert("x", x as f64)?;
        m.insert("y", y as f64)?;
        m.insert("w", w.max(20.0) as f64)?;
        m.insert("size", size.clamp(4.0, 200.0) as f64)?;
        self.doc.commit();
        Ok(())
    }

    /// A text page's body (Markdown); empty for other sheets.
    pub fn page_text(&self) -> String {
        self.doc.get_text("body").to_string()
    }

    /// Replaces a text page's body as a minimal edit (concurrent typing elsewhere survives).
    pub fn set_page_text(&self, markdown: &str) -> Result<()> {
        self.doc.get_text("body").update(markdown, UpdateOptions::default()).map_err(|e| Error::Invalid(format!("{e:?}")))?;
        self.doc.commit();
        Ok(())
    }

    /// Text boxes plus, for a text page with a body, the body as a box inside the page's
    /// margins (`page_width` points wide) — what a renderer or the PDF export draws.
    pub fn text_boxes_with_body(&self, page_width: f32) -> Vec<TextBox> {
        let mut out = self.text_boxes();
        let body = self.page_text();
        if !body.trim().is_empty() {
            out.insert(0, TextBox { id: BODY_ID.into(), x: 56.0, y: 64.0, w: (page_width - 112.0).max(100.0), size: 12.0, text: body });
        }
        out
    }

    /// All text boxes, top to bottom.
    pub fn text_boxes(&self) -> Vec<TextBox> {
        let mut out = Vec::new();
        self.objects.for_each(|k, v| {
            if let ValueOrContainer::Container(Container::Map(m)) = v
                && map_str(&m, "kind").as_deref() == Some("text")
            {
                let f = |key: &str, d: f64| map_f64(&m, key).unwrap_or(d) as f32;
                let text = match m.get("text") {
                    Some(ValueOrContainer::Container(Container::Text(t))) => t.to_string(),
                    _ => String::new(),
                };
                out.push(TextBox { id: k.to_string(), x: f("x", 0.0), y: f("y", 0.0), w: f("w", 200.0), size: f("size", 14.0), text });
            }
        });
        out.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
        out
    }

    /// Removes objects (erase, delete selection). Unknown ids are ignored.
    pub fn remove(&self, ids: &[String]) -> Result<()> {
        for id in ids {
            if self.objects.get(id).is_some() {
                self.objects.delete(id)?;
            }
        }
        self.doc.commit();
        Ok(())
    }

    /// Recolours strokes (lasso).
    pub fn recolor(&self, ids: &[String], color: u32) -> Result<()> {
        for id in ids {
            if let Some(mut s) = self.stroke(id)? {
                s.brush.color = color;
                self.objects.insert(id, LoroValue::Binary(encode_stroke(&s.brush, &s.points).into()))?;
            }
        }
        self.doc.commit();
        Ok(())
    }

    /// Moves and scales strokes about (ox, oy) (lasso move/resize).
    pub fn transform(&self, ids: &[String], dx: f32, dy: f32, scale: f32, ox: f32, oy: f32) -> Result<()> {
        if !(scale.is_finite() && scale > 0.0) {
            return Err(Error::Invalid("scale must be positive".into()));
        }
        for id in ids {
            if let Some(mut s) = self.stroke(id)? {
                for p in &mut s.points {
                    p.x = ox + (p.x - ox) * scale + dx;
                    p.y = oy + (p.y - oy) * scale + dy;
                }
                s.brush.width *= scale;
                self.objects.insert(id, LoroValue::Binary(encode_stroke(&s.brush, &s.points).into()))?;
            }
        }
        self.doc.commit();
        Ok(())
    }

    pub fn stroke(&self, id: &str) -> Result<Option<Stroke>> {
        match self.objects.get(id) {
            Some(ValueOrContainer::Value(LoroValue::Binary(b))) => decode_stroke(id, &b).map(Some),
            _ => Ok(None),
        }
    }

    /// All strokes (undecodable entries from a newer app version are skipped).
    pub fn strokes(&self) -> Vec<Stroke> {
        self.strokes_in(None)
    }

    /// Strokes whose bounds intersect `area` (None = all). Only the header is read for the
    /// bounds test, so culling a large sheet is cheap.
    pub fn strokes_in(&self, area: Option<Rect>) -> Vec<Stroke> {
        let mut out = Vec::new();
        self.objects.for_each(|k, v| {
            if let ValueOrContainer::Value(LoroValue::Binary(b)) = v {
                if let Some(a) = area
                    && let Some(bb) = stroke_bounds(&b)
                    && !bb.intersects(&a)
                {
                    return;
                }
                if let Ok(s) = decode_stroke(k, &b) {
                    out.push(s);
                }
            }
        });
        out
    }

    pub fn len(&self) -> usize {
        self.objects.len()
    }
    pub fn is_empty(&self) -> bool {
        self.objects.is_empty()
    }

    pub fn snapshot(&self) -> Result<Vec<u8>> {
        Ok(self.doc.export(ExportMode::Snapshot)?)
    }
    pub fn version(&self) -> Vec<u8> {
        self.doc.oplog_vv().encode()
    }
    pub fn updates_since(&self, version: &[u8]) -> Result<Vec<u8>> {
        let vv = VersionVector::decode(version).map_err(|e| Error::Format(e.to_string()))?;
        Ok(self.doc.export(ExportMode::updates(&vv))?)
    }
    pub fn import(&self, bytes: &[u8]) -> Result<()> {
        self.doc.import(bytes)?;
        Ok(())
    }
}

fn map_str(m: &LoroMap, key: &str) -> Option<String> {
    match m.get(key) {
        Some(ValueOrContainer::Value(LoroValue::String(s))) => Some(s.to_string()),
        _ => None,
    }
}

fn map_f64(m: &LoroMap, key: &str) -> Option<f64> {
    match m.get(key) {
        Some(ValueOrContainer::Value(LoroValue::Double(d))) => Some(d),
        Some(ValueOrContainer::Value(LoroValue::I64(i))) => Some(i as f64),
        _ => None,
    }
}

fn path_bounds(path: &[(f32, f32)], radius: f32) -> Option<Rect> {
    if path.is_empty() {
        return None;
    }
    let mut bb = Rect::EMPTY;
    for &(x, y) in path {
        bb = bb.include(x, y);
    }
    Some(bb.inflate(radius + 20.0))
}

/// Whether (x, y) lies under an eraser path whose tip radius varies along it (+ `extra`).
fn under_tip(x: f32, y: f32, path: &[(f32, f32, f32)], extra: f32) -> bool {
    match path.len() {
        0 => false,
        1 => (x - path[0].0).hypot(y - path[0].1) <= path[0].2 + extra,
        _ => path.windows(2).any(|w| seg_dist(x, y, w[0].0, w[0].1, w[1].0, w[1].1) <= w[0].2.max(w[1].2) + extra),
    }
}

fn dist_to_path(x: f32, y: f32, path: &[(f32, f32)]) -> f32 {
    match path.len() {
        0 => f32::INFINITY,
        1 => (x - path[0].0).hypot(y - path[0].1),
        _ => path.windows(2).map(|w| seg_dist(x, y, w[0].0, w[0].1, w[1].0, w[1].1)).fold(f32::INFINITY, f32::min),
    }
}

/// Samples at most `step` apart (pressure and time interpolated), so a cut lands where the
/// eraser is and not at the next input sample.
fn resample(points: &[InkPoint], step: f32) -> Vec<InkPoint> {
    let mut out = Vec::with_capacity(points.len() * 2);
    for (i, p) in points.iter().enumerate() {
        if i > 0 {
            let a = points[i - 1];
            let d = (p.x - a.x).hypot(p.y - a.y);
            let n = (d / step).floor() as usize;
            for k in 1..n.min(400) {
                let t = k as f32 / n as f32;
                out.push(InkPoint {
                    x: a.x + (p.x - a.x) * t,
                    y: a.y + (p.y - a.y) * t,
                    pressure: a.pressure + (p.pressure - a.pressure) * t,
                    t_ms: a.t_ms + ((p.t_ms as f32 - a.t_ms as f32) * t) as u32,
                });
            }
        }
        out.push(*p);
    }
    out
}

fn seg_dist(px: f32, py: f32, ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 < 1e-9 { 0.0 } else { (((px - ax) * dx + (py - ay) * dy) / len2).clamp(0.0, 1.0) };
    (px - (ax + t * dx)).hypot(py - (ay + t * dy))
}

/// Even-odd point-in-polygon test (x,y pairs).
fn point_in_polygon(x: f32, y: f32, poly: &[f32]) -> bool {
    let n = poly.len() / 2;
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi, xj, yj) = (poly[2 * i], poly[2 * i + 1], poly[2 * j], poly[2 * j + 1]);
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

// Stroke record v1: [1][width f32][thinning f32][color u32][x0 y0 x1 y1 f32][points…]
// Stroke record v2: [2][same as v1 up to the bounds][kind u8][points…] (only when kind != pen,
// so plain strokes stay readable by older apps).
const HEADER: usize = 1 + 4 + 4 + 4 + 16;
const STROKE_V2: u8 = 2;

fn encode_stroke(brush: &Brush, points: &[InkPoint]) -> Vec<u8> {
    let bb = ink::bounds(points, brush);
    let pts = ink::encode_points(points);
    let mut out = Vec::with_capacity(HEADER + 1 + pts.len());
    out.push(if brush.kind == ink::KIND_PEN { STROKE_V1 } else { STROKE_V2 });
    out.extend_from_slice(&brush.width.to_le_bytes());
    out.extend_from_slice(&brush.thinning.to_le_bytes());
    out.extend_from_slice(&brush.color.to_le_bytes());
    for v in [bb.x0, bb.y0, bb.x1, bb.y1] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    if brush.kind != ink::KIND_PEN {
        out.push(brush.kind);
    }
    out.extend_from_slice(&pts);
    out
}

fn f32_at(b: &[u8], i: usize) -> f32 {
    f32::from_le_bytes(b[i..i + 4].try_into().expect("4 bytes"))
}

fn known(b: &[u8]) -> bool {
    (b.len() >= HEADER && b[0] == STROKE_V1) || (b.len() > HEADER && b[0] == STROKE_V2)
}

fn stroke_bounds(b: &[u8]) -> Option<Rect> {
    known(b).then(|| Rect { x0: f32_at(b, 13), y0: f32_at(b, 17), x1: f32_at(b, 21), y1: f32_at(b, 25) })
}

fn decode_stroke(id: &str, b: &[u8]) -> Result<Stroke> {
    if !known(b) {
        return Err(Error::Format("unknown stroke record".into()));
    }
    let (kind, at) = if b[0] == STROKE_V2 { (b[HEADER], HEADER + 1) } else { (ink::KIND_PEN, HEADER) };
    let brush = Brush { width: f32_at(b, 1), thinning: f32_at(b, 5), color: u32::from_le_bytes(b[9..13].try_into().expect("4 bytes")), kind };
    Ok(Stroke { id: id.to_string(), brush, bounds: stroke_bounds(b).expect("checked"), points: ink::decode_points(&b[at..])? })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pts(x: f32, n: usize) -> Vec<InkPoint> {
        (0..n).map(|i| InkPoint { x: x + i as f32, y: 10.0 + i as f32 * 0.5, pressure: 0.5, t_ms: i as u32 * 5 }).collect()
    }

    #[test]
    fn add_cull_erase_recolor_transform() {
        let s = Sheet::create(7).unwrap();
        let a = s.add_stroke(Brush::default(), &pts(0.0, 20)).unwrap();
        let b = s.add_stroke(Brush::default(), &pts(500.0, 20)).unwrap();
        assert_eq!(s.strokes().len(), 2);
        let visible = s.strokes_in(Some(Rect::new(0.0, 0.0, 100.0, 100.0)));
        assert_eq!(visible.len(), 1);
        assert_eq!(visible[0].id, a);
        s.recolor(std::slice::from_ref(&a), 0xFFD71921).unwrap();
        assert_eq!(s.stroke(&a).unwrap().unwrap().brush.color, 0xFFD71921);
        s.transform(std::slice::from_ref(&a), 10.0, 0.0, 2.0, 0.0, 0.0).unwrap();
        let moved = s.stroke(&a).unwrap().unwrap();
        assert!((moved.points[1].x - 12.0).abs() < 0.05, "{}", moved.points[1].x);
        assert!((moved.brush.width - 4.0).abs() < 1e-5);
        s.remove(&[b.clone(), "nope".into()]).unwrap();
        assert_eq!(s.len(), 1);
        assert!(s.stroke(&b).unwrap().is_none());
    }

    #[test]
    fn undo_redo_hit_test_and_lasso() {
        let s = Sheet::create(9).unwrap();
        let a = s.add_stroke(Brush::default(), &pts(0.0, 20)).unwrap();
        let b = s.add_stroke(Brush::default(), &pts(200.0, 20)).unwrap();
        assert_eq!(s.hit_test(10.0, 15.0, 3.0), vec![a.clone()]);
        assert!(s.hit_test(100.0, 100.0, 3.0).is_empty());
        let poly = [-10.0, -10.0, 60.0, -10.0, 60.0, 60.0, -10.0, 60.0];
        assert_eq!(s.lasso(&poly), vec![a.clone()]);

        s.remove(std::slice::from_ref(&a)).unwrap();
        assert_eq!(s.len(), 1);
        assert!(s.undo().unwrap());
        assert_eq!(s.len(), 2, "undo brings the erased stroke back");
        assert!(s.redo().unwrap());
        assert_eq!(s.len(), 1);
        assert!(s.undo().unwrap());
        assert!(s.undo().unwrap()); // undo adding b
        assert!(s.stroke(&b).unwrap().is_none());
        assert!(s.can_redo());
    }

    fn line(x0: f32, x1: f32, y: f32) -> Vec<InkPoint> {
        (0..=10).map(|i| InkPoint { x: x0 + (x1 - x0) * i as f32 / 10.0, y, pressure: 0.5, t_ms: i * 8 }).collect()
    }

    #[test]
    fn text_page_body_merges_and_shows_as_a_box() {
        let a = Sheet::create(12).unwrap();
        assert!(a.text_boxes_with_body(595.0).is_empty());
        a.set_page_text("# Mitschrift\n\nErster Absatz").unwrap();
        let b = Sheet::load(13, &a.snapshot().unwrap()).unwrap();
        a.set_page_text("# Mitschrift V08\n\nErster Absatz").unwrap();
        b.set_page_text("# Mitschrift\n\nErster Absatz\n\n- Punkt").unwrap();
        a.import(&b.updates_since(&a.version()).unwrap()).unwrap();
        assert_eq!(a.page_text(), "# Mitschrift V08\n\nErster Absatz\n\n- Punkt");
        let boxes = a.text_boxes_with_body(595.0);
        assert_eq!((boxes[0].id.as_str(), boxes[0].w), (BODY_ID, 483.0));
        assert!(a.strokes().is_empty() && a.text_boxes().is_empty(), "the body is neither a stroke nor a box");
    }

    #[test]
    fn text_boxes_store_edit_move_and_merge_concurrent_typing() {
        let a = Sheet::create(10).unwrap();
        a.add_stroke(Brush::default(), &pts(0.0, 5)).unwrap();
        let id = a.add_text_box(50.0, 80.0, 300.0, 14.0, "# Satz\nDer Mittelwertsatz").unwrap();
        assert_eq!(a.strokes().len(), 1, "text boxes are not strokes");
        let tb = &a.text_boxes()[0];
        assert_eq!((tb.x, tb.y, tb.w, tb.size, tb.text.as_str()), (50.0, 80.0, 300.0, 14.0, "# Satz\nDer Mittelwertsatz"));
        a.place_text_box(&id, 60.0, 90.0, 250.0, 16.0).unwrap();
        assert_eq!(a.text_boxes()[0].w, 250.0);
        // Another device types at the end while this one fixes the start: both survive.
        let b = Sheet::load(11, &a.snapshot().unwrap()).unwrap();
        a.set_text(&id, "# Satz 4.2\nDer Mittelwertsatz").unwrap();
        b.set_text(&id, "# Satz\nDer Mittelwertsatz der Differentialrechnung").unwrap();
        a.import(&b.updates_since(&a.version()).unwrap()).unwrap();
        assert_eq!(a.text_boxes()[0].text, "# Satz 4.2\nDer Mittelwertsatz der Differentialrechnung");
        a.remove(&[id]).unwrap();
        assert!(a.text_boxes().is_empty());
    }

    #[test]
    fn area_eraser_cuts_a_line_in_two_and_undoes_in_one_step() {
        let s = Sheet::create(4).unwrap();
        s.add_stroke(Brush { width: 2.0, ..Brush::default() }, &line(0.0, 100.0, 50.0)).unwrap();
        s.add_stroke(Brush::default(), &line(0.0, 100.0, 200.0)).unwrap(); // far away: untouched
        // A vertical swipe through x = 50.
        let changed = s.erase_area(&[(50.0, 0.0, 5.0), (50.0, 100.0, 5.0)]).unwrap();
        assert_eq!(changed, 1);
        let mut pieces: Vec<Stroke> = s.strokes().into_iter().filter(|st| st.bounds.y0 < 100.0).collect();
        pieces.sort_by(|a, b| a.bounds.x0.total_cmp(&b.bounds.x0));
        assert_eq!(pieces.len(), 2, "cut in the middle: two pieces");
        assert!(pieces[0].points.last().unwrap().x < 46.0 && pieces[1].points[0].x > 54.0, "gap where the eraser went");
        assert_eq!(s.len(), 3);
        assert!(s.undo().unwrap());
        assert_eq!(s.len(), 2, "one undo restores the whole line");
    }

    #[test]
    fn add_strokes_is_one_step_and_hit_path_finds_crossed_strokes() {
        let s = Sheet::create(5).unwrap();
        let ids = s.add_strokes(&[(Brush::default(), line(0.0, 50.0, 10.0)), (Brush::default(), line(0.0, 50.0, 80.0))]).unwrap();
        assert_eq!(ids.len(), 2);
        let crossed = s.hit_path(&[(25.0, 0.0), (25.0, 20.0)], 2.0);
        assert_eq!(crossed, vec![ids[0].clone()]);
        assert!(s.undo().unwrap());
        assert!(s.is_empty(), "both strokes were one edit");
    }

    #[test]
    fn pencil_kind_round_trips_and_pen_stays_v1() {
        let s = Sheet::create(3).unwrap();
        let pen = s.add_stroke(Brush::default(), &pts(0.0, 5)).unwrap();
        let pencil = s.add_stroke(Brush { kind: ink::KIND_PENCIL, ..Brush::default() }, &pts(50.0, 5)).unwrap();
        assert_eq!(s.stroke(&pen).unwrap().unwrap().brush.kind, ink::KIND_PEN);
        assert_eq!(s.stroke(&pencil).unwrap().unwrap().brush.kind, ink::KIND_PENCIL);
        assert_eq!(encode_stroke(&Brush::default(), &pts(0.0, 3))[0], STROKE_V1, "plain strokes keep the old format");
    }

    #[test]
    fn undo_never_reverts_a_peer() {
        let a = Sheet::create(1).unwrap();
        let b = Sheet::load(2, &a.snapshot().unwrap()).unwrap();
        b.add_stroke(Brush::default(), &pts(0.0, 5)).unwrap();
        a.import(&b.updates_since(&a.version()).unwrap()).unwrap();
        assert_eq!(a.len(), 1);
        assert!(!a.undo().unwrap(), "nothing of A's own to undo");
        assert_eq!(a.len(), 1);
    }

    #[test]
    fn two_devices_draw_and_erase_concurrently() {
        let a = Sheet::create(1).unwrap();
        let shared = a.add_stroke(Brush::default(), &pts(0.0, 10)).unwrap();
        let b = Sheet::load(2, &a.snapshot().unwrap()).unwrap();
        a.add_stroke(Brush::default(), &pts(50.0, 10)).unwrap();
        b.add_stroke(Brush::default(), &pts(90.0, 10)).unwrap();
        b.remove(std::slice::from_ref(&shared)).unwrap();
        let (va, vb) = (a.version(), b.version());
        a.import(&b.updates_since(&va).unwrap()).unwrap();
        b.import(&a.updates_since(&vb).unwrap()).unwrap();
        let ids = |s: &Sheet| {
            let mut v: Vec<String> = s.strokes().into_iter().map(|s| s.id).collect();
            v.sort();
            v
        };
        assert_eq!(ids(&a), ids(&b));
        assert_eq!(ids(&a).len(), 2);
    }
}
