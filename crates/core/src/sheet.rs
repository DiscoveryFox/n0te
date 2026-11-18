//! One sheet's content. Objects live in the map "objects", keyed by object id. A stroke is
//! immutable: one binary value (brush, bounds, encoded samples), so loading a sheet is a single
//! pass with no nested containers. Erasing deletes the key; recolouring or moving writes a new
//! value under the same key.

use std::sync::Mutex;

use loro::{ExportMode, LoroDoc, LoroMap, LoroValue, UndoManager, ValueOrContainer, VersionVector};

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
