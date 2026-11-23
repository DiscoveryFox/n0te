//! One sheet's content. Objects live in the map "objects", keyed by object id. A stroke is
//! immutable: one binary value (brush, bounds, encoded samples), so loading a sheet is a single
//! pass with no nested containers. Erasing deletes the key; recolouring or moving writes a new
//! value under the same key.

use std::sync::Mutex;

use loro::{Container, ExportMode, LoroDoc, LoroMap, LoroText, LoroValue, UndoManager, UpdateOptions, ValueOrContainer, VersionVector};

use crate::error::{Error, Result};
use crate::ids;
use crate::ink::{self, Brush, InkPoint, Rect};
use crate::plot::{LabelMode, LineStyle, Obj, Plot, PointStyle, Slider, Snap, Style};

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
/// An image on a page: a blob (JPEG or PNG bytes in the library's blob store, synced like PDFs)
/// placed with its top-left at (x, y) and w × h page points. Apps that predate images skip it.
#[derive(Debug, Clone, PartialEq)]
pub struct ImageObj {
    pub id: String,
    pub blob: String,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

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

/// What sticks out of a page of a given size ([`Sheet::outside`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Outside {
    pub strokes: usize,
    pub images: usize,
    pub texts: usize,
}

impl Outside {
    pub fn is_empty(&self) -> bool {
        self.strokes + self.images + self.texts == 0
    }
    pub fn add(&mut self, o: Outside) {
        self.strokes += o.strokes;
        self.images += o.images;
        self.texts += o.texts;
    }
    /// "3 Striche, 1 Bild und 2 Textfelder" (empty when nothing sticks out).
    pub fn describe(&self) -> String {
        let n = |k: usize, one: &str, many: &str| match k {
            0 => None,
            1 => Some(format!("1 {one}")),
            k => Some(format!("{k} {many}")),
        };
        let parts: Vec<String> = [n(self.strokes, "Strich", "Striche"), n(self.images, "Bild", "Bilder"), n(self.texts, "Textfeld", "Textfelder")].into_iter().flatten().collect();
        match parts.len() {
            0 => String::new(),
            1 => parts[0].clone(),
            _ => format!("{} und {}", parts[..parts.len() - 1].join(", "), parts[parts.len() - 1]),
        }
    }
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
        // What the handwriting recogniser writes is no edit of the user's: undo skips it.
        undo.add_exclude_origin_prefix(INDEX_ORIGIN);
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
            let Some(runs) = cut_stroke(&s.points, s.brush.width, path) else { continue };
            changed += 1;
            self.objects.delete(&s.id)?;
            for run in runs {
                let id = ids::new_id(self.doc.peer_id());
                self.objects.insert(&id, LoroValue::Binary(encode_stroke(&s.brush, &run).into()))?;
            }
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

    /// Places an image (a stored blob) on the page; returns its object id.
    pub fn add_image(&self, blob: &str, x: f32, y: f32, w: f32, h: f32) -> Result<String> {
        if !(w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()) {
            return Err(Error::Invalid("image size must be positive".into()));
        }
        let id = ids::new_id(self.doc.peer_id());
        let m = self.objects.insert_container(&id, LoroMap::new())?;
        m.insert("kind", "image")?;
        m.insert("blob", blob)?;
        // Stacking: the newest image lies on top (milliseconds since 1970).
        let z = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0);
        m.insert("z", z)?;
        for (k, v) in [("x", x), ("y", y), ("w", w), ("h", h)] {
            m.insert(k, v as f64)?;
        }
        self.doc.commit();
        Ok(id)
    }

    /// The page's images, bottom to top (the newest last).
    pub fn images(&self) -> Vec<ImageObj> {
        let mut out = Vec::new();
        let mut z: Vec<f64> = Vec::new();
        self.objects.for_each(|k, v| {
            if let ValueOrContainer::Container(Container::Map(m)) = v
                && map_str(&m, "kind").as_deref() == Some("image")
                && let Some(blob) = map_str(&m, "blob")
            {
                let f = |key: &str| map_f64(&m, key).unwrap_or(0.0) as f32;
                out.push(ImageObj { id: k.to_string(), blob, x: f("x"), y: f("y"), w: f("w"), h: f("h") });
                z.push(map_f64(&m, "z").unwrap_or(0.0));
            }
        });
        let mut order: Vec<usize> = (0..out.len()).collect();
        order.sort_by(|&a, &b| z[a].total_cmp(&z[b]).then(out[a].id.cmp(&out[b].id)));
        order.into_iter().map(|i| out[i].clone()).collect()
    }

    /// Moves or resizes an image.
    pub fn place_image(&self, id: &str, x: f32, y: f32, w: f32, h: f32) -> Result<()> {
        let Some(m) = self.image_map(id) else { return Err(Error::NotFound(format!("image {id}"))) };
        for (k, v) in [("x", x), ("y", y), ("w", w.max(1.0)), ("h", h.max(1.0))] {
            m.insert(k, v as f64)?;
        }
        self.doc.commit();
        Ok(())
    }

    fn image_map(&self, id: &str) -> Option<LoroMap> {
        match self.objects.get(id) {
            Some(ValueOrContainer::Container(Container::Map(m))) if map_str(&m, "kind").as_deref() == Some("image") => Some(m),
            _ => None,
        }
    }

    /// An image's or a plot's map: the objects that are a box on the page.
    fn boxed_map(&self, id: &str) -> Option<LoroMap> {
        match self.objects.get(id) {
            Some(ValueOrContainer::Container(Container::Map(m))) if matches!(map_str(&m, "kind").as_deref(), Some("image") | Some("plot")) => Some(m),
            _ => None,
        }
    }

    fn plot_map(&self, id: &str) -> Option<LoroMap> {
        match self.objects.get(id) {
            Some(ValueOrContainer::Container(Container::Map(m))) if map_str(&m, "kind").as_deref() == Some("plot") => Some(m),
            _ => None,
        }
    }

    // --- plots ------------------------------------------------------------------------------

    /// Puts a coordinate system on the page (its `id` is ignored); returns its id. Apps that
    /// predate plots skip it.
    pub fn add_plot(&self, p: &Plot) -> Result<String> {
        if !p.is_valid() {
            return Err(Error::Invalid("plot: empty box or range".into()));
        }
        let id = ids::new_id(self.doc.peer_id());
        let m = self.objects.insert_container(&id, LoroMap::new())?;
        m.insert("kind", "plot")?;
        let z = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0);
        m.insert("z", z)?;
        write_plot(&m, p)?;
        self.doc.commit();
        Ok(id)
    }

    /// Changes a plot (its box, ranges, functions …) in one step.
    pub fn set_plot(&self, p: &Plot) -> Result<()> {
        if !p.is_valid() {
            return Err(Error::Invalid("plot: empty box or range".into()));
        }
        let Some(m) = self.plot_map(&p.id) else { return Err(Error::NotFound(format!("plot {}", p.id))) };
        write_plot(&m, p)?;
        self.doc.commit();
        Ok(())
    }

    /// The page's plots, bottom to top.
    pub fn plots(&self) -> Vec<Plot> {
        let mut out: Vec<(f64, Plot)> = Vec::new();
        self.objects.for_each(|k, v| {
            if let ValueOrContainer::Container(Container::Map(m)) = v
                && map_str(&m, "kind").as_deref() == Some("plot")
            {
                out.push((map_f64(&m, "z").unwrap_or(0.0), read_plot(k, &m)));
            }
        });
        out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.id.cmp(&b.1.id)));
        out.into_iter().map(|(_, p)| p).collect()
    }

    /// The plots drawn as ink (for the PDF export: the same look as on screen).
    pub fn plot_strokes(&self) -> Vec<Stroke> {
        self.plots()
            .iter()
            .flat_map(|p| crate::plot::render(p))
            .map(|(brush, points)| Stroke { id: String::new(), bounds: bounds_of(&points, brush.width), brush, points })
            .collect()
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
            if let Some(m) = self.boxed_map(id) {
                // Images and plots move and scale with the selection.
                let f = |key: &str| map_f64(&m, key).unwrap_or(0.0) as f32;
                m.insert("x", (ox + (f("x") - ox) * scale + dx) as f64)?;
                m.insert("y", (oy + (f("y") - oy) * scale + dy) as f64)?;
                m.insert("w", (f("w") * scale) as f64)?;
                m.insert("h", (f("h") * scale) as f64)?;
                continue;
            }
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

    /// Stretches strokes and images about (ax, ay): across by `kx`, down by `ky` (a side of a
    /// selection dragged). The pen's width stays, so a stretched rectangle keeps its line.
    pub fn stretch(&self, ids: &[String], ax: f32, ay: f32, kx: f32, ky: f32) -> Result<()> {
        let ok = |k: f32| k.is_finite() && k.abs() > 1e-3;
        if !ok(kx) || !ok(ky) {
            return Err(Error::Invalid("bad stretch".into()));
        }
        for id in ids {
            if let Some(m) = self.boxed_map(id) {
                let f = |key: &str| map_f64(&m, key).unwrap_or(0.0) as f32;
                let (x0, x1) = (ax + (f("x") - ax) * kx, ax + (f("x") + f("w") - ax) * kx);
                let (y0, y1) = (ay + (f("y") - ay) * ky, ay + (f("y") + f("h") - ay) * ky);
                m.insert("x", x0.min(x1) as f64)?;
                m.insert("y", y0.min(y1) as f64)?;
                m.insert("w", (x1 - x0).abs().max(1.0) as f64)?;
                m.insert("h", (y1 - y0).abs().max(1.0) as f64)?;
                continue;
            }
            if let Some(mut s) = self.stroke(id)? {
                for p in &mut s.points {
                    p.x = ax + (p.x - ax) * kx;
                    p.y = ay + (p.y - ay) * ky;
                }
                self.objects.insert(id, LoroValue::Binary(encode_stroke(&s.brush, &s.points).into()))?;
            }
        }
        self.doc.commit();
        Ok(())
    }

    /// Turns and stretches strokes about the anchor (ax, ay): every point p becomes
    /// a + (re + i·im)·(p − a) in complex terms (angle and length of (re, im) are the rotation
    /// and the stretch). The width stays: dragging a line's end changes its length and angle,
    /// not its thickness.
    pub fn spin(&self, ids: &[String], ax: f32, ay: f32, re: f32, im: f32) -> Result<()> {
        if !(re.is_finite() && im.is_finite()) || re * re + im * im < 1e-6 {
            return Err(Error::Invalid("bad rotation".into()));
        }
        for id in ids {
            if let Some(mut s) = self.stroke(id)? {
                for p in &mut s.points {
                    let (dx, dy) = (p.x - ax, p.y - ay);
                    p.x = ax + re * dx - im * dy;
                    p.y = ay + im * dx + re * dy;
                }
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

    /// Top and bottom of everything that lies below the line at `y` (its middle under it):
    /// strokes, images and text boxes, what [`Sheet::shift_below`] moves.
    pub fn below(&self, y: f32) -> crate::notebook::Below {
        let mut b = crate::notebook::Below::default();
        let mut take = |y0: f32, y1: f32| {
            if (y0 + y1) / 2.0 >= y {
                b.top = Some(b.top.map_or(y0, |t: f32| t.min(y0)));
                b.bottom = Some(b.bottom.map_or(y1, |t: f32| t.max(y1)));
            }
        };
        self.objects.for_each(|_, v| {
            if let ValueOrContainer::Value(LoroValue::Binary(bytes)) = v
                && let Some(r) = stroke_bounds(&bytes)
            {
                take(r.y0, r.y1);
            }
        });
        for i in self.images() {
            take(i.y, i.y + i.h);
        }
        for p in self.plots() {
            take(p.y, p.y + p.h);
        }
        for t in self.text_boxes() {
            take(t.y, t.y + text_box_height(&t));
        }
        b
    }

    /// Moves everything below the line at `y` (its middle under it) down by `dy` (up when
    /// negative), in one step to undo: space inserted into or taken out of the page. Returns
    /// how many objects moved.
    pub fn shift_below(&self, y: f32, dy: f32) -> Result<usize> {
        if !dy.is_finite() {
            return Err(Error::Invalid("bad shift".into()));
        }
        let mut strokes = Vec::new();
        self.objects.for_each(|k, v| {
            if let ValueOrContainer::Value(LoroValue::Binary(bytes)) = v
                && let Some(r) = stroke_bounds(&bytes)
                && (r.y0 + r.y1) / 2.0 >= y
            {
                strokes.push(k.to_string());
            }
        });
        let mut n = 0;
        for id in &strokes {
            if let Some(mut s) = self.stroke(id)? {
                for p in &mut s.points {
                    p.y += dy;
                }
                self.objects.insert(id, LoroValue::Binary(encode_stroke(&s.brush, &s.points).into()))?;
                n += 1;
            }
        }
        for i in self.images() {
            if i.y + i.h / 2.0 >= y
                && let Some(m) = self.image_map(&i.id)
            {
                m.insert("y", (i.y + dy) as f64)?;
                n += 1;
            }
        }
        for p in self.plots() {
            if p.y + p.h / 2.0 >= y
                && let Some(m) = self.plot_map(&p.id)
            {
                m.insert("y", (p.y + dy) as f64)?;
                n += 1;
            }
        }
        for t in self.text_boxes() {
            if t.y + text_box_height(&t) / 2.0 >= y
                && let Some(m) = self.text_map(&t.id)
            {
                m.insert("y", (t.y + dy) as f64)?;
                n += 1;
            }
        }
        self.doc.commit();
        Ok(n)
    }

    // --- what the handwriting says, headings, search ------------------------------------------

    fn index_map(&self) -> LoroMap {
        self.doc.get_map("index")
    }

    fn headings_map(&self) -> LoroMap {
        self.doc.get_map("headings")
    }

    /// The boxes of the strokes still on the page, by id.
    fn boxes_by_id(&self) -> std::collections::HashMap<String, Rect> {
        let mut out = std::collections::HashMap::new();
        self.objects.for_each(|k, v| {
            if let ValueOrContainer::Value(LoroValue::Binary(b)) = v
                && let Some(r) = stroke_bounds(&b)
            {
                out.insert(k.to_string(), r);
            }
        });
        out
    }

    /// The box around `ids` (those still there), or None if none is.
    fn box_of(boxes: &std::collections::HashMap<String, Rect>, ids: &[String]) -> Option<[f32; 4]> {
        let mut b: Option<[f32; 4]> = None;
        for id in ids {
            if let Some(r) = boxes.get(id) {
                b = Some(match b {
                    None => [r.x0, r.y0, r.x1, r.y1],
                    Some(c) => [c[0].min(r.x0), c[1].min(r.y0), c[2].max(r.x1), c[3].max(r.y1)],
                });
            }
        }
        b
    }

    /// Every stroke's id, box and kind, without reading its points (cheap: for finding the
    /// lines of handwriting on a page).
    pub fn stroke_boxes(&self) -> Vec<(String, Rect, u8)> {
        let mut out = Vec::new();
        self.objects.for_each(|k, v| {
            if let ValueOrContainer::Value(LoroValue::Binary(b)) = v
                && let Some(r) = stroke_bounds(&b)
            {
                let kind = if b[0] == STROKE_V2 { b[HEADER] } else { ink::KIND_PEN };
                out.push((k.to_string(), r, kind));
            }
        });
        out
    }

    /// The lines of handwriting as the recogniser read them, where they are now (lines whose
    /// strokes are all gone are left out).
    pub fn index_lines(&self) -> Vec<IndexLine> {
        let boxes = self.boxes_by_id();
        let mut out = Vec::new();
        self.index_map().for_each(|k, v| {
            if let ValueOrContainer::Value(LoroValue::String(s)) = v {
                let mut parts = s.split(SEP_FIELD);
                let text = parts.next().unwrap_or("").to_string();
                let alts: Vec<String> = parts.next().unwrap_or("").split(SEP_ITEM).filter(|a| !a.is_empty()).map(str::to_string).collect();
                let strokes: Vec<String> = parts.next().unwrap_or("").split(',').filter(|a| !a.is_empty()).map(str::to_string).collect();
                if let Some(b) = Self::box_of(&boxes, &strokes) {
                    out.push(IndexLine { key: k.to_string(), text, alts, strokes, x0: b[0], y0: b[1], x1: b[2], y1: b[3] });
                }
            }
        });
        out.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
        out
    }

    /// The keys of all stored lines (also those whose strokes are gone: to drop them).
    pub fn index_keys(&self) -> Vec<String> {
        self.index_map().keys().map(|k| k.to_string()).collect()
    }

    /// Lines read (put, by key) and lines gone (`drop`), in one step that undo leaves alone.
    pub fn set_index(&self, put: &[IndexLine], drop: &[String]) -> Result<()> {
        let m = self.index_map();
        for k in drop {
            m.delete(k)?;
        }
        for l in put {
            let clean = |t: &str| t.replace([SEP_FIELD, SEP_ITEM], " ");
            let alts: Vec<String> = l.alts.iter().map(|a| clean(a)).collect();
            let v = format!("{}{SEP_FIELD}{}{SEP_FIELD}{}", clean(&l.text), alts.join(&SEP_ITEM.to_string()), l.strokes.join(","));
            m.insert(&l.key, v)?;
        }
        self.doc.set_next_commit_origin(INDEX_ORIGIN);
        self.doc.commit();
        Ok(())
    }

    /// Marks `strokes` as a heading reading `text` (one step to undo); returns its id.
    pub fn add_heading(&self, text: &str, strokes: &[String]) -> Result<String> {
        let id = ids::new_id(self.doc.peer_id());
        self.headings_map().insert(&id, format!("{}{SEP_FIELD}{}", text.replace(SEP_FIELD, " "), strokes.join(",")))?;
        self.doc.commit();
        Ok(id)
    }

    /// A heading's words changed (its strokes stay).
    pub fn set_heading_text(&self, id: &str, text: &str) -> Result<()> {
        let Some(h) = self.headings().into_iter().find(|h| h.id == id) else { return Err(Error::NotFound(format!("heading {id}"))) };
        self.headings_map().insert(id, format!("{}{SEP_FIELD}{}", text.replace(SEP_FIELD, " "), h.strokes.join(",")))?;
        self.doc.commit();
        Ok(())
    }

    /// No longer a heading (the strokes stay).
    pub fn remove_heading(&self, id: &str) -> Result<()> {
        self.headings_map().delete(id)?;
        self.doc.commit();
        Ok(())
    }

    /// The headings the user marked, top to bottom, where their strokes are now (a heading
    /// whose strokes are all gone is left out).
    pub fn headings(&self) -> Vec<Heading> {
        let boxes = self.boxes_by_id();
        let mut out = Vec::new();
        self.headings_map().for_each(|k, v| {
            if let ValueOrContainer::Value(LoroValue::String(s)) = v {
                let (text, rest) = s.split_once(SEP_FIELD).unwrap_or((s.as_str(), ""));
                let strokes: Vec<String> = rest.split(',').filter(|a| !a.is_empty()).map(str::to_string).collect();
                if let Some(b) = Self::box_of(&boxes, &strokes) {
                    out.push(Heading { id: k.to_string(), text: text.to_string(), strokes, x0: b[0], y0: b[1], x1: b[2], y1: b[3] });
                }
            }
        });
        out.sort_by(|a, b| a.y0.total_cmp(&b.y0).then(a.x0.total_cmp(&b.x0)));
        out
    }

    /// Where the words of `query` are on this page (a `page_width` page): headings, handwriting
    /// (also as the recogniser's second and third guess), text boxes and a text page's body.
    /// Best first, then top to bottom. PDF text is searched by the caller (it needs the PDF).
    pub fn search(&self, query: &str, page_width: f32) -> Vec<crate::search::Hit> {
        use crate::search::{Hit, HitKind, normalize, score, words};
        let w = words(query);
        let mut out = Vec::new();
        if w.is_empty() {
            return out;
        }
        // Strokes of headings that matched: their handwriting needn't be found a second time.
        let mut in_heading = std::collections::HashSet::new();
        for h in self.headings() {
            if let Some(slips) = score(&w, &normalize(&h.text)) {
                in_heading.extend(h.strokes.iter().cloned());
                out.push(Hit { kind: HitKind::Heading, text: h.text, x0: h.x0, y0: h.y0, x1: h.x1, y1: h.y1, slips });
            }
        }
        for l in self.index_lines() {
            if !l.strokes.is_empty() && l.strokes.iter().all(|s| in_heading.contains(s)) {
                continue;
            }
            let best = std::iter::once(&l.text).chain(l.alts.iter()).filter_map(|t| score(&w, &normalize(t))).min();
            if let Some(slips) = best {
                out.push(Hit { kind: HitKind::Ink, text: l.text, x0: l.x0, y0: l.y0, x1: l.x1, y1: l.y1, slips });
            }
        }
        for t in self.text_boxes_with_body(page_width) {
            // Line by line, so a long text page points at the line that matched.
            let line_h = t.size * 1.4;
            let per_line = (t.w / (t.size * 0.5)).max(1.0);
            let mut y = t.y;
            for line in t.text.lines() {
                let rows = (line.chars().count() as f32 / per_line).ceil().max(1.0);
                if let Some(slips) = score(&w, &normalize(line)) {
                    out.push(Hit { kind: HitKind::Text, text: line.trim().to_string(), x0: t.x, y0: y, x1: t.x + t.w, y1: y + rows * line_h, slips });
                }
                y += rows * line_h;
            }
        }
        // The objects of coordinate systems ("sin" finds f(x) = 2sin x, "Schneide" finds S).
        for p in self.plots() {
            for o in &p.objs {
                if let Some(slips) = score(&w, &normalize(&o.def)) {
                    out.push(Hit { kind: HitKind::Plot, text: o.def.trim().to_string(), x0: p.x, y0: p.y, x1: p.x + p.w, y1: p.y + p.h, slips });
                }
            }
        }
        out.sort_by(|a, b| a.slips.cmp(&b.slips).then(a.y0.total_cmp(&b.y0)));
        out
    }

    /// What would stick out of a `width` × `height` page (top-left at 0, 0): strokes, images
    /// and text boxes reaching past its edges (a little slack for the stroke's own width).
    /// Used to warn before a page is turned or made smaller.
    pub fn outside(&self, width: f32, height: f32) -> Outside {
        const SLACK: f32 = 2.0;
        let out_of = |x0: f32, y0: f32, x1: f32, y1: f32| x0 < -SLACK || y0 < -SLACK || x1 > width + SLACK || y1 > height + SLACK;
        let mut o = Outside::default();
        self.objects.for_each(|_, v| {
            if let ValueOrContainer::Value(LoroValue::Binary(b)) = v
                && let Some(r) = stroke_bounds(&b)
                && out_of(r.x0, r.y0, r.x1, r.y1)
            {
                o.strokes += 1;
            }
        });
        o.images = self.images().iter().filter(|i| out_of(i.x, i.y, i.x + i.w, i.y + i.h)).count()
            + self.plots().iter().filter(|p| out_of(p.x, p.y, p.x + p.w, p.y + p.h)).count();
        o.texts = self
            .text_boxes()
            .iter()
            .filter(|t| out_of(t.x, t.y, t.x + t.w, t.y + text_box_height(t)))
            .count();
        o
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

fn map_bool(m: &LoroMap, key: &str) -> Option<bool> {
    match m.get(key) {
        Some(ValueOrContainer::Value(LoroValue::Bool(b))) => Some(b),
        _ => None,
    }
}

/// A plot's fields into its map: the view as keys, each object as an entry of the map "objs"
/// (its id → "definition␟order␟style"), so two devices editing different objects merge.
fn write_plot(m: &LoroMap, p: &Plot) -> Result<()> {
    for (k, v) in [("x", p.x as f64), ("y", p.y as f64), ("w", p.w as f64), ("h", p.h as f64), ("x0", p.x_min), ("x1", p.x_max), ("y0", p.y_min), ("y1", p.y_max), ("color", p.color as f64), ("width", p.width as f64)] {
        m.insert(k, v)?;
    }
    for (k, v) in [("axes", p.axes), ("grid", p.grid), ("minor", p.minor), ("clean", p.clean), ("pi", p.pi), ("align", p.align), ("interactive", p.interactive)] {
        m.insert(k, v)?;
    }
    let clean = |s: &str| s.replace([SEP_FIELD, SEP_ITEM], " ");
    m.insert("xname", clean(&p.x_name))?;
    m.insert("yname", clean(&p.y_name))?;
    m.insert("snap", p.snap.code())?;
    let objs = m.ensure_mergeable_map("objs")?;
    let mut keep = std::collections::HashSet::new();
    for o in &p.objs {
        let v = format!("{}{SEP_FIELD}{}{SEP_FIELD}{}", clean(&o.def), o.order, encode_style(&o.style));
        keep.insert(o.id.clone());
        if map_str(&objs, &o.id).as_deref() != Some(v.as_str()) {
            objs.insert(&o.id, v)?;
        }
    }
    let mut gone = Vec::new();
    objs.for_each(|k, _| {
        if !keep.contains(k) {
            gone.push(k.to_string());
        }
    });
    for k in gone {
        objs.delete(&k)?;
    }
    Ok(())
}

const SEP_STYLE: char = '\u{1d}';

fn encode_style(s: &Style) -> String {
    let mut kv: Vec<String> = vec![format!("c={:08x}", s.color)];
    if s.width > 0.0 {
        kv.push(format!("w={}", s.width));
    }
    if s.line != LineStyle::Solid {
        kv.push(format!("l={}", s.line.code()));
    }
    match s.point {
        PointStyle::Dot => {}
        PointStyle::Cross => kv.push("p=cross".into()),
        PointStyle::Ring => kv.push("p=ring".into()),
    }
    if s.size > 0.0 {
        kv.push(format!("s={}", s.size));
    }
    if s.fill > 0.0 {
        kv.push(format!("f={}", s.fill));
    }
    kv.push(format!(
        "lb={}",
        match s.label {
            LabelMode::Off => "off",
            LabelMode::Name => "name",
            LabelMode::NameValue => "nv",
            LabelMode::Value => "val",
            LabelMode::Caption => "cap",
        }
    ));
    if !s.caption.is_empty() {
        kv.push(format!("cap={}", s.caption.replace([SEP_STYLE, SEP_FIELD, SEP_ITEM], " ")));
    }
    if !s.visible {
        kv.push("v=0".into());
    }
    if s.aux {
        kv.push("aux=1".into());
    }
    if s.fixed {
        kv.push("fix=1".into());
    }
    if let Some(sl) = s.slider {
        kv.push(format!("sl={},{},{}", sl.min, sl.max, sl.step));
    }
    if s.special != 0 {
        kv.push(format!("sp={}", s.special));
    }
    if s.coords {
        kv.push("co=1".into());
    }
    if let Some((x, y)) = s.at {
        kv.push(format!("at={x},{y}"));
    }
    kv.join(&SEP_STYLE.to_string())
}

fn decode_style(src: &str) -> Style {
    let mut s = Style::default();
    for kv in src.split(SEP_STYLE) {
        let Some((k, v)) = kv.split_once('=') else { continue };
        let nums = |v: &str| v.split(',').filter_map(|x| x.parse::<f64>().ok()).collect::<Vec<_>>();
        match k {
            "c" => s.color = u32::from_str_radix(v, 16).unwrap_or(s.color),
            "w" => s.width = v.parse().unwrap_or(0.0),
            "l" => s.line = LineStyle::from_code(v),
            "p" => {
                s.point = match v {
                    "cross" => PointStyle::Cross,
                    "ring" => PointStyle::Ring,
                    _ => PointStyle::Dot,
                }
            }
            "s" => s.size = v.parse().unwrap_or(0.0),
            "f" => s.fill = v.parse().unwrap_or(0.0),
            "lb" => {
                s.label = match v {
                    "name" => LabelMode::Name,
                    "nv" => LabelMode::NameValue,
                    "val" => LabelMode::Value,
                    "cap" => LabelMode::Caption,
                    _ => LabelMode::Off,
                }
            }
            "cap" => s.caption = v.to_string(),
            "v" => s.visible = v != "0",
            "aux" => s.aux = v == "1",
            "fix" => s.fixed = v == "1",
            "sl" => {
                let n = nums(v);
                if n.len() == 3 {
                    s.slider = Some(Slider { min: n[0], max: n[1], step: n[2] });
                }
            }
            "sp" => s.special = v.parse().unwrap_or(0),
            "co" => s.coords = v == "1",
            "at" => {
                let n = nums(v);
                if n.len() == 2 {
                    s.at = Some((n[0] as f32, n[1] as f32));
                }
            }
            _ => {}
        }
    }
    s
}

fn read_plot(id: &str, m: &LoroMap) -> Plot {
    let f = |k: &str, d: f64| map_f64(m, k).unwrap_or(d);
    let mut p = Plot {
        id: id.to_string(),
        x: f("x", 0.0) as f32,
        y: f("y", 0.0) as f32,
        w: f("w", 1.0) as f32,
        h: f("h", 1.0) as f32,
        x_min: f("x0", -5.0),
        x_max: f("x1", 5.0),
        y_min: f("y0", -5.0),
        y_max: f("y1", 5.0),
        axes: map_bool(m, "axes").unwrap_or(true),
        grid: map_bool(m, "grid").unwrap_or(false),
        minor: map_bool(m, "minor").unwrap_or(false),
        clean: map_bool(m, "clean").unwrap_or(false),
        color: f("color", 0xFF20_2428u32 as f64) as u32,
        width: f("width", 1.2) as f32,
        pi: map_bool(m, "pi").unwrap_or(false),
        x_name: map_str(m, "xname").unwrap_or_default(),
        y_name: map_str(m, "yname").unwrap_or_default(),
        snap: Snap::from_code(&map_str(m, "snap").unwrap_or_default()),
        align: map_bool(m, "align").unwrap_or(true),
        interactive: map_bool(m, "interactive").unwrap_or(false),
        objs: Vec::new(),
    };
    if let Some(ValueOrContainer::Container(Container::Map(objs))) = m.get("objs") {
        objs.for_each(|k, v| {
            if let ValueOrContainer::Value(LoroValue::String(s)) = v {
                let mut parts = s.split(SEP_FIELD);
                let def = parts.next().unwrap_or("").to_string();
                let order = parts.next().and_then(|o| o.parse().ok()).unwrap_or(0.0);
                let style = decode_style(parts.next().unwrap_or(""));
                p.objs.push(Obj { id: k.to_string(), def, style, order });
            }
        });
        p.objs.sort_by(|a, b| a.order.total_cmp(&b.order).then(a.id.cmp(&b.id)));
    } else {
        p.objs = legacy_objs(id, m);
    }
    p
}

/// A plot of n0te 0.22–0.23 (functions in one string, marks, points) as objects.
fn legacy_objs(id: &str, m: &LoroMap) -> Vec<Obj> {
    let names = ["f", "g", "h", "k", "p", "q", "r", "s"];
    let marks = map_f64(m, "marks").unwrap_or(0.0) as u8;
    let coords = map_bool(m, "coords").unwrap_or(false);
    let named = map_bool(m, "names").unwrap_or(false);
    let mut out = Vec::new();
    let mut order = 0.0;
    let mut next = |out: &mut Vec<Obj>, def: String, style: Style| {
        order += 1.0;
        out.push(Obj { id: format!("{id}-{}", out.len()), def, style, order });
    };
    let mut areas = Vec::new();
    for (i, item) in map_str(m, "fns").unwrap_or_default().split(SEP_ITEM).filter(|s| !s.is_empty()).enumerate() {
        let mut parts = item.split(SEP_FIELD);
        let expr = parts.next().unwrap_or("").trim().to_string();
        let color = parts.next().and_then(|c| u32::from_str_radix(c, 16).ok()).unwrap_or(0xFF20_2428);
        let line = LineStyle::from_code(parts.next().unwrap_or(""));
        let area = parts.next().filter(|a| !a.is_empty()).map(|a| a.split(';').map(|x| x.to_string()).collect::<Vec<_>>());
        let n = names.get(i).copied().unwrap_or("f");
        if expr.is_empty() {
            continue;
        }
        let def = match expr.split_once('=') {
            Some((lhs, rhs)) if lhs.trim().eq_ignore_ascii_case("x") => format!("{n}: x = {}", rhs.trim()),
            Some((_, rhs)) => format!("{n}(x) = {}", rhs.trim()),
            None => format!("{n}(x) = {expr}"),
        };
        let style = Style { color, line, special: marks, coords, label: if named { LabelMode::Name } else { LabelMode::Off }, ..Style::default() };
        next(&mut out, def, style);
        if let Some(a) = area
            && a.len() >= 2
        {
            areas.push((n.to_string(), a, color));
        }
    }
    for (k, (n, a, color)) in areas.into_iter().enumerate() {
        let to = a.get(2).and_then(|j| j.parse::<usize>().ok()).and_then(|j| names.get(j));
        let def = match to {
            Some(g) => format!("fläche{} = IntegralZwischen({n}, {g}, {}, {})", k + 1, a[0], a[1]),
            None => format!("fläche{} = Integral({n}, {}, {})", k + 1, a[0], a[1]),
        };
        let label = if coords { LabelMode::Value } else { LabelMode::Off };
        next(&mut out, def, Style { color, fill: 0.5, label, ..Style::default() });
    }
    for item in map_str(m, "pts").unwrap_or_default().split(SEP_ITEM).filter(|s| !s.is_empty()) {
        let v: Vec<&str> = item.split(SEP_FIELD).collect();
        if v.len() == 3 {
            let style = Style { color: 0xFF15_65C0, label: if coords { LabelMode::NameValue } else { LabelMode::Name }, ..Style::default() };
            next(&mut out, format!("{} = ({}, {})", v[0], v[1], v[2]), style);
        }
    }
    out
}

fn map_f64(m: &LoroMap, key: &str) -> Option<f64> {
    match m.get(key) {
        Some(ValueOrContainer::Value(LoroValue::Double(d))) => Some(d),
        Some(ValueOrContainer::Value(LoroValue::I64(i))) => Some(i as f64),
        _ => None,
    }
}

/// The area eraser on one stroke: the pieces that stay when the parts under the eraser path
/// (x, y, radius per dab) are cut away, or None if the path doesn't touch the stroke. Specks
/// are dropped. Shared by [`Sheet::erase_area`] and live previews of someone erasing.
pub fn cut_stroke(points: &[InkPoint], width: f32, path: &[(f32, f32, f32)]) -> Option<Vec<Vec<InkPoint>>> {
    let extra = width * 0.35;
    let pts = resample(points, 0.75);
    let keep: Vec<bool> = pts.iter().map(|p| !under_tip(p.x, p.y, path, extra)).collect();
    if keep.iter().all(|&k| k) {
        return None;
    }
    let mut runs = Vec::new();
    let mut run: Vec<InkPoint> = Vec::new();
    let mut flush = |run: &mut Vec<InkPoint>| {
        // Keep pieces that are more than a speck.
        if run.len() >= 2 || (run.len() == 1 && points.len() == 1) {
            runs.push(std::mem::take(run));
        }
        run.clear();
    };
    for (p, k) in pts.iter().zip(&keep) {
        if *k {
            run.push(*p);
        } else {
            flush(&mut run);
        }
    }
    flush(&mut run);
    Some(runs)
}

/// The bounds of a run of points drawn `width` wide (for strokes made on the fly).
pub fn bounds_of(points: &[InkPoint], width: f32) -> Rect {
    let xy: Vec<(f32, f32)> = points.iter().map(|p| (p.x, p.y)).collect();
    path_bounds(&xy, width / 2.0 + 1.0).unwrap_or(Rect::EMPTY)
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

/// Commits from the handwriting recogniser carry this origin (undo skips them).
const INDEX_ORIGIN: &str = "index";
/// Separators inside a stored index line / heading (control characters, never typed).
const SEP_FIELD: char = '\u{1f}';
const SEP_ITEM: char = '\u{1e}';

/// A line of handwriting as the recogniser read it: its best reading, the next guesses, the
/// strokes it is made of, and where they are now. `key` names the line (the app derives it
/// from the strokes, so a line read again replaces itself).
#[derive(Debug, Clone, PartialEq)]
pub struct IndexLine {
    pub key: String,
    pub text: String,
    pub alts: Vec<String>,
    pub strokes: Vec<String>,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// A heading the user marked: its words, the strokes it is made of, where they are now.
#[derive(Debug, Clone, PartialEq)]
pub struct Heading {
    pub id: String,
    pub text: String,
    pub strokes: Vec<String>,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// A text box's height follows its text: estimated from its lines and their length.
fn text_box_height(t: &TextBox) -> f32 {
    let per_line = (t.w / (t.size * 0.5)).max(1.0);
    let lines: f32 = t.text.lines().map(|l| (l.chars().count() as f32 / per_line).ceil().max(1.0)).sum::<f32>().max(1.0);
    lines * t.size * 1.4
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
    #[test]
    fn what_sticks_out_of_a_turned_page_is_counted() {
        use crate::ink::{Brush, InkPoint};
        let s = super::Sheet::create(1).unwrap();
        let p = |x: f32, y: f32| InkPoint { x, y, pressure: 0.5, t_ms: 0 };
        let brush = Brush { width: 2.0, thinning: 0.0, color: 0xFF000000, kind: 0 };
        // Near the top: fits portrait and landscape A4. Low on the page: only portrait.
        s.add_stroke(brush, &[p(100.0, 100.0), p(200.0, 120.0)]).unwrap();
        s.add_stroke(brush, &[p(100.0, 700.0), p(300.0, 760.0)]).unwrap();
        s.add_image("b", 400.0, 650.0, 100.0, 80.0).unwrap();
        s.add_text_box(56.0, 620.0, 200.0, 12.0, "Unten").unwrap();
        assert!(s.outside(595.0, 842.0).is_empty(), "everything is on the portrait page");
        let o = s.outside(842.0, 595.0);
        assert_eq!((o.strokes, o.images, o.texts), (1, 1, 1));
        assert_eq!(o.describe(), "1 Strich, 1 Bild und 1 Textfeld");
        let mut sum = o;
        sum.add(o);
        assert_eq!(sum.describe(), "2 Striche, 2 Bilder und 2 Textfelder");
    }

    use super::*;

    #[test]
    fn what_the_handwriting_says_is_searchable_and_not_undone() {
        let s = Sheet::create(1).unwrap();
        let p = |x: f32, y: f32| InkPoint { x, y, pressure: 0.5, t_ms: 0 };
        let brush = Brush { width: 2.0, thinning: 0.0, color: 0xFF000000, kind: 0 };
        let a = s.add_stroke(brush, &[p(100.0, 100.0), p(140.0, 110.0)]).unwrap();
        let b = s.add_stroke(brush, &[p(150.0, 100.0), p(190.0, 110.0)]).unwrap();
        let c = s.add_stroke(brush, &[p(100.0, 300.0), p(200.0, 310.0)]).unwrap();
        let line = |key: &str, text: &str, alts: &[&str], strokes: &[&String]| IndexLine {
            key: key.into(), text: text.into(), alts: alts.iter().map(|a| a.to_string()).collect(),
            strokes: strokes.iter().map(|s| s.to_string()).collect(), x0: 0.0, y0: 0.0, x1: 0.0, y1: 0.0,
        };
        s.set_index(&[line("l1", "Grenzwert der Folge", &["Grenzwert der Falge"], &[&a, &b]), line("l2", "Beweis", &[], &[&c])], &[]).unwrap();
        let lines = s.index_lines();
        assert_eq!(lines.len(), 2);
        assert!((lines[0].x0 - 99.0).abs() < 2.0 && lines[0].x1 > 189.0, "box from the strokes: {:?}", lines[0]);
        // Undo takes back the last stroke, not what the recogniser wrote.
        assert!(s.undo().unwrap());
        assert_eq!(s.index_lines().len(), 1, "the line of the undone stroke is gone with it, the other stays");
        s.redo().unwrap();
        // Headings: marked by the user, undoable, found first.
        let h = s.add_heading("Satz 3: Grenzwerte", &[c.clone()]).unwrap();
        let hits = s.search("grenzwert", 595.0);
        assert_eq!(hits.len(), 2);
        assert_eq!(hits.iter().filter(|h| h.kind == crate::search::HitKind::Heading).count(), 1);
        assert_eq!(s.search("Beweis", 595.0)[0].kind, crate::search::HitKind::Ink);
        assert!(s.search("Integral", 595.0).is_empty());
        s.set_heading_text(&h, "Satz 3").unwrap();
        assert_eq!(s.headings()[0].text, "Satz 3");
        assert!(s.undo().unwrap() && s.undo().unwrap());
        assert!(s.headings().is_empty());
        // Gone lines and typed text.
        s.set_index(&[], &["l2".into()]).unwrap();
        assert_eq!(s.index_lines().len(), 1);
        s.add_text_box(56.0, 400.0, 300.0, 12.0, "Übung zur Konvergenz").unwrap();
        assert_eq!(s.search("uebung", 595.0)[0].kind, crate::search::HitKind::Text);
        // Handwriting marked as a heading is found once, as the heading.
        s.add_heading("Grenzwert der Folge", &[a.clone(), b.clone()]).unwrap();
        let hits = s.search("folge", 595.0);
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].kind, crate::search::HitKind::Heading);
    }

    #[test]
    fn plots_are_boxes_that_keep_their_settings() {
        let s = Sheet::create(1).unwrap();
        let mut p = Plot::centered(100.0, 200.0, 240.0, 160.0, 20.0, 0xFF22_2222, 1.2);
        for i in ["A = (1, 2)", "f(x) = x^2 - 1", "g(x) = sin(x)", "S = Schneide(f, g)", "a = Schieberegler(0, 5, 0.5)"] {
            p = p.add(i).unwrap();
        }
        p.objs[1].style.line = LineStyle::Dashed;
        p.objs[1].style.special = crate::plot::MARK_ZEROS;
        p.objs[2].style.label = LabelMode::NameValue;
        p.objs[2].style.caption = "Welle; hoch".into();
        p.objs[0].style.at = Some((0.25, 0.5));
        p.grid = true;
        p.pi = true;
        p.x_name = "t".into();
        p.snap = Snap::Grid;
        p.interactive = true;
        let id = s.add_plot(&p).unwrap();
        let got = s.plots();
        assert_eq!(got.len(), 1);
        let mut want = p.clone();
        want.id = id.clone();
        assert_eq!(got[0], want);
        // Not an image (nothing to fetch for it), but a box like one: moved, stretched, shifted.
        assert!(s.images().is_empty());
        s.transform(&[id.clone()], 10.0, 0.0, 0.5, 100.0, 200.0).unwrap();
        let q = &s.plots()[0];
        assert_eq!((q.x, q.y, q.w, q.h), (110.0, 200.0, 120.0, 80.0));
        s.stretch(&[id.clone()], 110.0, 200.0, 2.0, 1.0).unwrap();
        assert_eq!(s.plots()[0].w, 240.0);
        s.shift_below(150.0, 30.0).unwrap();
        assert_eq!(s.plots()[0].y, 230.0);
        // Edited in one step, undone in one.
        let e = s.plots()[0].remove(&s.plots()[0].objs[1].id);
        assert_eq!(e.objs.len(), 3, "f goes and the intersections with it");
        s.set_plot(&e).unwrap();
        assert_eq!(s.plots()[0].objs.len(), 3);
        s.undo().unwrap();
        assert_eq!(s.plots()[0].objs.len(), 6);
        // Its objects are found (each intersection is a point of its own).
        let hits = s.search("Schneide", 595.0);
        assert_eq!(hits.len(), 2);
        assert_eq!((hits[0].kind, hits[0].text.as_str()), (crate::search::HitKind::Plot, "S_1 = Schneide(f, g, 1)"));
        // Drawn as ink for the export; survives a reload; gone with remove.
        assert!(s.plot_strokes().len() > 20);
        let b = Sheet::load(2, &s.snapshot().unwrap()).unwrap();
        assert_eq!(b.plots()[0].objs, s.plots()[0].objs);
        s.remove(&[id]).unwrap();
        assert!(s.plots().is_empty());
        let mut bad = p.clone();
        bad.x_max = bad.x_min;
        assert!(s.add_plot(&bad).is_err());
    }

    #[test]
    fn plots_of_0_23_become_objects() {
        let s = Sheet::create(1).unwrap();
        let m = s.objects.insert_container("old", LoroMap::new()).unwrap();
        m.insert("kind", "plot").unwrap();
        for (k, v) in [("x", 10.0), ("y", 20.0), ("w", 240.0), ("h", 160.0), ("x0", -6.0), ("x1", 6.0), ("y0", -4.0), ("y1", 4.0), ("marks", 1.0)] {
            m.insert(k, v).unwrap();
        }
        m.insert("coords", true).unwrap();
        m.insert("fns", format!("x^2 - 1{SEP_FIELD}ffd02020{SEP_FIELD}dash{SEP_FIELD}-1;1;{SEP_ITEM}f'(x){SEP_FIELD}ff2020d0{SEP_FIELD}{SEP_FIELD}{SEP_ITEM}x = 2{SEP_FIELD}ff202020")).unwrap();
        m.insert("pts", format!("A{SEP_FIELD}1{SEP_FIELD}2")).unwrap();
        s.doc.commit();
        let p = &s.plots()[0];
        let defs: Vec<&str> = p.objs.iter().map(|o| o.def.as_str()).collect();
        assert_eq!(defs, vec!["f(x) = x^2 - 1", "g(x) = f'(x)", "h: x = 2", "fläche1 = Integral(f, -1, 1)", "A = (1, 2)"]);
        assert!(p.errors().iter().all(|e| e.is_none()), "{:?}", p.errors());
        assert_eq!(p.objs[0].style.line, LineStyle::Dashed);
        assert_eq!(p.objs[0].style.special, crate::plot::MARK_ZEROS);
        assert!(p.objs[0].style.coords);
    }

    #[test]
    fn stretching_keeps_the_pen_width_and_one_side() {
        let s = Sheet::create(1).unwrap();
        let p = |x: f32, y: f32| InkPoint { x, y, pressure: 0.5, t_ms: 0 };
        let brush = Brush { width: 2.0, thinning: 0.0, color: 0xFF000000, kind: 0 };
        let r = s.add_stroke(brush, &[p(100.0, 100.0), p(200.0, 100.0), p(200.0, 150.0), p(100.0, 150.0), p(100.0, 100.0)]).unwrap();
        let img = s.add_image("b", 100.0, 200.0, 100.0, 50.0).unwrap();
        // The right side pulled out: twice as wide, the left side stays.
        s.stretch(&[r.clone(), img.clone()], 100.0, 0.0, 2.0, 1.0).unwrap();
        let st = s.stroke(&r).unwrap().unwrap();
        assert_eq!(st.points[1].x, 300.0);
        assert_eq!(st.points[0].x, 100.0);
        assert_eq!(st.points[2].y, 150.0);
        assert_eq!(st.brush.width, 2.0);
        let i = s.images().into_iter().find(|i| i.id == img).unwrap();
        assert_eq!((i.x, i.w, i.h), (100.0, 200.0, 50.0));
        assert!(s.stretch(&[r], 0.0, 0.0, 0.0, 1.0).is_err());
    }

    #[test]
    fn space_below_a_line_moves_everything_under_it_in_one_step() {
        let s = Sheet::create(1).unwrap();
        let p = |x: f32, y: f32| InkPoint { x, y, pressure: 0.5, t_ms: 0 };
        let brush = Brush { width: 2.0, thinning: 0.0, color: 0xFF000000, kind: 0 };
        let above = s.add_stroke(brush, &[p(100.0, 100.0), p(200.0, 120.0)]).unwrap();
        let under = s.add_stroke(brush, &[p(100.0, 400.0), p(200.0, 420.0)]).unwrap();
        // Mostly above the line: stays (its middle is above).
        let across = s.add_stroke(brush, &[p(300.0, 240.0), p(300.0, 310.0)]).unwrap();
        let img = s.add_image("b", 50.0, 500.0, 100.0, 80.0).unwrap();
        let txt = s.add_text_box(56.0, 600.0, 200.0, 12.0, "Unten").unwrap();
        let b = s.below(300.0);
        assert!((395.0..400.0).contains(&b.top.unwrap()), "top of the stroke under the line (with its width): {b:?}");
        assert!(b.bottom.unwrap() > 600.0);
        assert_eq!(s.shift_below(300.0, 50.0).unwrap(), 3);
        let y0 = |id: &str| s.stroke(id).unwrap().unwrap().points[0].y;
        assert_eq!((y0(&above), y0(&under), y0(&across)), (100.0, 450.0, 240.0));
        assert_eq!(s.images().iter().find(|i| i.id == img).unwrap().y, 550.0);
        assert_eq!(s.text_boxes().iter().find(|t| t.id == txt).unwrap().y, 650.0);
        // One undo puts it all back.
        assert!(s.undo().unwrap());
        assert_eq!((y0(&under), s.images()[0].y, s.text_boxes()[0].y), (400.0, 500.0, 600.0));
    }

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
    fn images_are_placed_moved_with_the_selection_and_removed() {
        let s = Sheet::create(5).unwrap();
        let id = s.add_image("ab12", 10.0, 20.0, 100.0, 50.0).unwrap();
        s.add_text_box(0.0, 0.0, 100.0, 12.0, "hi").unwrap();
        assert_eq!(s.images(), vec![ImageObj { id: id.clone(), blob: "ab12".into(), x: 10.0, y: 20.0, w: 100.0, h: 50.0 }]);
        // A later image lies on top (last).
        std::thread::sleep(std::time::Duration::from_millis(3));
        let top = s.add_image("cd34", 0.0, 0.0, 10.0, 10.0).unwrap();
        assert_eq!(s.images().last().map(|i| i.id.clone()), Some(top.clone()));
        s.remove(&[top]).unwrap();
        assert_eq!(s.text_boxes().len(), 1, "images are not text boxes");
        assert!(s.strokes().is_empty(), "nor strokes");
        s.transform(std::slice::from_ref(&id), 5.0, 0.0, 2.0, 10.0, 20.0).unwrap();
        let i = &s.images()[0];
        assert_eq!((i.x, i.y, i.w, i.h), (15.0, 20.0, 200.0, 100.0));
        s.place_image(&id, 0.0, 0.0, 30.0, 40.0).unwrap();
        assert_eq!((s.images()[0].w, s.images()[0].h), (30.0, 40.0));
        // Survives a round trip (sync) and goes with remove.
        let b = Sheet::load(6, &s.snapshot().unwrap()).unwrap();
        assert_eq!(b.images().len(), 1);
        s.remove(&[id]).unwrap();
        assert!(s.images().is_empty());
        assert!(s.add_image("x", 0.0, 0.0, 0.0, 10.0).is_err());
    }

    #[test]
    fn spin_turns_and_stretches_about_an_end_keeping_the_width() {
        let s = Sheet::create(8).unwrap();
        // A line from (10, 10) to (20, 10).
        let line: Vec<InkPoint> = (0..=10).map(|i| InkPoint { x: 10.0 + i as f32, y: 10.0, pressure: 0.5, t_ms: i * 5 }).collect();
        let a = s.add_stroke(Brush::default(), &line).unwrap();
        let w = s.stroke(&a).unwrap().unwrap().brush.width;
        // Its end dragged from (20, 10) to (10, 30): turned 90° and twice as long, about the start.
        let (ax, ay) = (10.0f32, 10.0f32);
        let (ox, oy) = (20.0 - ax, 10.0 - ay);
        let (nx, ny) = (10.0 - ax, 30.0 - ay);
        let d = ox * ox + oy * oy;
        let (re, im) = ((nx * ox + ny * oy) / d, (ny * ox - nx * oy) / d);
        s.spin(std::slice::from_ref(&a), ax, ay, re, im).unwrap();
        let st = s.stroke(&a).unwrap().unwrap();
        let (f, l) = (st.points[0], st.points[st.points.len() - 1]);
        assert!((f.x - 10.0).abs() < 0.05 && (f.y - 10.0).abs() < 0.05, "start stays: {f:?}");
        assert!((l.x - 10.0).abs() < 0.05 && (l.y - 30.0).abs() < 0.05, "end follows: {l:?}");
        assert_eq!(st.brush.width, w, "the width stays");
        assert!(s.spin(std::slice::from_ref(&a), 0.0, 0.0, 0.0, 0.0).is_err());
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
