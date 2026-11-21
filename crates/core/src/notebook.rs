//! The notebook index: a tree of groups and sheets.
//!
//! - The main column is the ordered children of the root; a **group** is a collapsible node
//!   whose children are more groups or sheets (e.g. "Wöchentliche Aufgaben" > "Week01").
//! - A **sheet** node's own children are its **side sheets**, each marked `left` or `right`
//!   (ordered outward from the main sheet).
//! - Each sheet's content lives in its own document ([`crate::sheet::Sheet`]), loaded lazily.

use loro::{ExportMode, LoroDoc, LoroMap, LoroTree, LoroValue, TreeID, TreeParentId, ValueOrContainer, VersionVector};

use crate::error::{Error, Result};
use crate::ids;

/// What a sheet looks like. `width`/`height` 0 = an infinite board.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetSpec {
    pub width: f32,
    pub height: f32,
    pub background: Background,
    /// Pattern spacing in page units (line distance, grid box size, dot distance).
    pub spacing: f32,
    /// For PDF pages: the blob (BLAKE3 hex) and page index of the background.
    pub pdf: Option<(String, u32)>,
    /// Blank space inserted into a PDF page (sorted): at `.0` page points down the PDF as
    /// printed, `.1` points of empty paper, the rest of the PDF below. `height` includes them.
    pub gaps: Vec<(f32, f32)>,
}

/// A stretch of the background and where it lies on the page: from `src` (page points down
/// the background as printed) for `len`, drawn from `dst` down the page.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Band {
    pub src: f32,
    pub len: f32,
    pub dst: f32,
}

impl SheetSpec {
    pub fn a4(background: Background) -> Self {
        SheetSpec { width: 595.0, height: 842.0, background, spacing: 20.0, pdf: None, gaps: Vec::new() }
    }
    pub fn board() -> Self {
        SheetSpec { width: 0.0, height: 0.0, background: Background::Dots, spacing: 24.0, pdf: None, gaps: Vec::new() }
    }
    pub fn is_board(&self) -> bool {
        self.width <= 0.0 || self.height <= 0.0
    }
    /// The background's own height (a PDF page as printed), without the space inserted into it.
    pub fn base_height(&self) -> f32 {
        self.height - self.gaps.iter().map(|g| g.1).sum::<f32>()
    }
    /// How the background lies on the page: one band without gaps, one more per gap (a renderer
    /// draws each band of a `width` × [`SheetSpec::base_height`] background moved down to `dst`).
    pub fn bands(&self) -> Vec<Band> {
        let mut out = Vec::with_capacity(self.gaps.len() + 1);
        let (mut src, mut shift) = (0.0, 0.0);
        for &(at, h) in &self.gaps {
            out.push(Band { src, len: at - src, dst: src + shift });
            shift += h;
            src = at;
        }
        out.push(Band { src, len: self.base_height() - src, dst: src + shift });
        out
    }
}

/// What lies below a line on a page (its top and bottom; none if nothing does).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Below {
    pub top: Option<f32>,
    pub bottom: Option<f32>,
}

/// Space inserted into or taken out of a page: how far what lies below the line moves, and
/// the page's height and gaps after.
#[derive(Debug, Clone, PartialEq)]
pub struct SpacePlan {
    pub dy: f32,
    pub height: f32,
    pub gaps: Vec<(f32, f32)>,
}

/// Room kept under the lowest thing on a page that grew to keep it.
const SPACE_MARGIN: f32 = 12.0;

/// Space inserted at `y` (`dy` > 0) or taken out (`dy` < 0) on a page, with `below` what lies
/// under the line. Nothing ever leaves its page: what is pushed past the bottom makes the page
/// longer; a PDF page splits at the line (the PDF below moves down with the ink, so notes stay
/// next to what they belong to) and grows by the space. Taking space out stops before what
/// lies below reaches the line, and on a PDF page it only takes out space inserted before
/// (the printed sheet never shrinks).
pub fn plan_space(spec: &SheetSpec, y: f32, dy: f32, below: Below) -> SpacePlan {
    let mut plan = SpacePlan { dy: 0.0, height: spec.height, gaps: spec.gaps.clone() };
    if !dy.is_finite() || dy == 0.0 {
        return plan;
    }
    let max = crate::format::MAX_PT;
    if spec.is_board() {
        plan.dy = if dy > 0.0 { dy } else { -(-dy).min(below.top.map_or(0.0, |t| (t - y).max(0.0))) };
        return plan;
    }
    let y = y.clamp(0.0, spec.height);
    // Where `y` is on a PDF page: inside a band (its place in the PDF as printed) or inside a
    // gap. The first band starts at the top, so a gap always lies between two bands.
    enum Place {
        Printed(f32),
        Gap(usize),
    }
    let place = || -> Place {
        let bands = spec.bands();
        for (k, b) in bands.iter().enumerate() {
            if y < b.dst {
                return Place::Gap(k - 1);
            }
            if y < b.dst + b.len || k + 1 == bands.len() {
                return Place::Printed(b.src + (y - b.dst).min(b.len));
            }
        }
        Place::Printed(spec.base_height())
    };
    if dy > 0.0 {
        if spec.pdf.is_some() {
            let d = dy.min(max - spec.height).max(0.0);
            match place() {
                Place::Gap(k) => plan.gaps[k].1 += d,
                Place::Printed(at) => match plan.gaps.iter_mut().find(|g| (g.0 - at).abs() < 0.01) {
                    Some(g) => g.1 += d,
                    None => {
                        plan.gaps.push((at, d));
                        plan.gaps.sort_by(|a, b| a.0.total_cmp(&b.0));
                    }
                },
            }
            plan.dy = d;
            plan.height = spec.height + d;
        } else {
            let mut d = dy;
            if let Some(bottom) = below.bottom {
                d = d.min(max - SPACE_MARGIN - bottom).max(0.0);
                plan.height = spec.height.max(bottom + d + SPACE_MARGIN).min(max);
            }
            plan.dy = d;
        }
    } else {
        let mut d = (-dy).min(below.top.map_or(f32::INFINITY, |t| (t - y).max(0.0)));
        if spec.pdf.is_some() {
            let room = match place() {
                Place::Gap(k) => (spec.bands()[k + 1].dst - y).max(0.0),
                Place::Printed(_) => 0.0,
            };
            d = d.min(room);
            if let Place::Gap(k) = place() {
                plan.gaps[k].1 -= d;
                if plan.gaps[k].1 < 0.5 {
                    plan.gaps.remove(k);
                }
            }
            plan.height = spec.height - d;
        } else if below.top.is_none() {
            d = 0.0; // nothing below to pull up
        }
        plan.dy = -d;
    }
    plan
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Background {
    Blank,
    Lines,
    Grid,
    Dots,
    Cornell,
    /// A flowing text page: its content is the sheet's Markdown body ([`crate::sheet::Sheet::page_text`]).
    /// Apps that predate it read it as blank paper.
    Text,
}

impl Background {
    pub fn as_str(&self) -> &'static str {
        match self {
            Background::Blank => "blank",
            Background::Lines => "lines",
            Background::Grid => "grid",
            Background::Dots => "dots",
            Background::Cornell => "cornell",
            Background::Text => "text",
        }
    }
    pub fn parse(s: &str) -> Background {
        match s {
            "lines" => Background::Lines,
            "grid" => Background::Grid,
            "dots" => Background::Dots,
            "cornell" => Background::Cornell,
            "text" => Background::Text,
            _ => Background::Blank,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

impl Side {
    fn as_str(&self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
        }
    }
}

/// A node id as a string ("<peer>@<counter>"), stable across devices.
pub type NodeId = String;

/// One row of the flattened notebook outline (what a page list or table of contents shows).
#[derive(Debug, Clone, PartialEq)]
pub enum OutlineRow {
    Group { node: NodeId, depth: u32, title: String, collapsed: bool, count: u32 },
    Sheet { node: NodeId, depth: u32, sheet_id: String, title: String, spec: SheetSpec, left: Vec<SideSheet>, right: Vec<SideSheet> },
}

#[derive(Debug, Clone, PartialEq)]
pub struct SideSheet {
    pub node: NodeId,
    pub sheet_id: String,
    pub spec: SheetSpec,
}

pub struct Notebook {
    doc: LoroDoc,
    tree: LoroTree,
    meta: LoroMap,
    /// Records the last [`Notebook::delete_undoable`] so it can be taken back.
    undo_delete: std::sync::Mutex<Option<loro::UndoManager>>,
}

const K_KIND: &str = "kind";
const K_TITLE: &str = "title";
const K_SHEET: &str = "sheet";
const K_SIDE: &str = "side";
const K_COLLAPSED: &str = "collapsed";
const K_W: &str = "w";
const K_H: &str = "h";
const K_BG: &str = "bg";
const K_SPACING: &str = "spacing";
const K_PDF: &str = "pdf";
const K_PDF_PAGE: &str = "pdf_page";
const K_GAPS: &str = "gaps";

fn gaps_text(gaps: &[(f32, f32)]) -> String {
    gaps.iter().map(|(a, h)| format!("{a}:{h}")).collect::<Vec<_>>().join(";")
}

fn parse_gaps(s: &str) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = s
        .split(';')
        .filter_map(|g| {
            let (a, h) = g.split_once(':')?;
            let (a, h) = (a.parse::<f32>().ok()?, h.parse::<f32>().ok()?);
            (a.is_finite() && h.is_finite() && h > 0.0).then_some((a, h))
        })
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out
}

impl Notebook {
    /// A new, empty notebook edited by `peer`.
    pub fn create(peer: u64, title: &str) -> Result<Self> {
        let nb = Self::from_doc(LoroDoc::new(), peer)?;
        nb.meta.insert("title", title)?;
        nb.meta.insert("id", ids::new_id(peer))?;
        nb.meta.insert("format", 1)?;
        nb.doc.commit();
        Ok(nb)
    }

    /// Opens a notebook from a snapshot (and any updates after it).
    pub fn load(peer: u64, snapshot: &[u8]) -> Result<Self> {
        let doc = LoroDoc::new();
        doc.import(snapshot)?;
        Self::from_doc(doc, peer)
    }

    fn from_doc(doc: LoroDoc, peer: u64) -> Result<Self> {
        doc.set_peer_id(peer)?;
        let tree = doc.get_tree("pages");
        tree.enable_fractional_index(0);
        let meta = doc.get_map("meta");
        Ok(Notebook { doc, tree, meta, undo_delete: std::sync::Mutex::new(None) })
    }

    pub fn id(&self) -> String {
        self.meta_str("id")
    }
    pub fn title(&self) -> String {
        self.meta_str("title")
    }
    pub fn set_title(&self, title: &str) -> Result<()> {
        self.meta.insert("title", title)?;
        self.doc.commit();
        Ok(())
    }

    /// Gives a copy of a notebook its own identity (restoring an old version as a new notebook).
    pub(crate) fn set_id(&self, id: &str) -> Result<()> {
        self.meta.insert("id", id)?;
        self.doc.commit();
        Ok(())
    }

    /// The library folder this notebook is filed under ("" = top level), e.g. "WS26".
    pub fn folder(&self) -> String {
        self.meta_str("folder")
    }
    pub fn set_folder(&self, folder: &str) -> Result<()> {
        self.meta.insert("folder", folder.trim())?;
        self.doc.commit();
        Ok(())
    }

    /// How the library shows the notebook: "" / "page" (first page), or a preset name.
    pub fn cover(&self) -> String {
        self.meta_str("cover")
    }
    pub fn set_cover(&self, cover: &str) -> Result<()> {
        self.meta.insert("cover", cover)?;
        self.doc.commit();
        Ok(())
    }

    /// Every sheet's spec (main and side sheets) by sheet id.
    pub fn sheet_specs(&self) -> std::collections::HashMap<String, SheetSpec> {
        let mut out = std::collections::HashMap::new();
        for row in self.outline(true) {
            if let OutlineRow::Sheet { sheet_id, spec, left, right, .. } = row {
                out.insert(sheet_id, spec);
                for s in left.into_iter().chain(right) {
                    out.insert(s.sheet_id, s.spec);
                }
            }
        }
        out
    }

    /// The PDF blobs the notebook's sheets use as backgrounds.
    pub fn pdf_blobs(&self) -> Vec<String> {
        let mut out = Vec::new();
        for row in self.outline(true) {
            if let OutlineRow::Sheet { spec, left, right, .. } = row {
                out.extend(spec.pdf.map(|p| p.0));
                for s in left.into_iter().chain(right) {
                    out.extend(s.spec.pdf.map(|p| p.0));
                }
            }
        }
        out
    }

    /// Number of sheets (main and side) in the notebook.
    pub fn sheet_count(&self) -> usize {
        self.sheet_order().len()
    }

    fn meta_str(&self, key: &str) -> String {
        value_str(self.meta.get(key))
    }

    /// Adds a group at `index` under `parent` (None = main column; index past the end appends).
    pub fn add_group(&self, parent: Option<&str>, index: usize, title: &str) -> Result<NodeId> {
        let p = self.parent_for_new(parent)?;
        let id = self.create_at(p, index)?;
        let m = self.tree.get_meta(id)?;
        m.insert(K_KIND, "group")?;
        m.insert(K_TITLE, title)?;
        m.insert(K_COLLAPSED, false)?;
        self.doc.commit();
        Ok(id.to_string())
    }

    /// Adds a main sheet at `index` under `parent` (None = main column, or a group).
    /// Returns (node, sheet id).
    pub fn add_sheet(&self, parent: Option<&str>, index: usize, spec: &SheetSpec) -> Result<(NodeId, String)> {
        let p = self.parent_for_new(parent)?;
        let id = self.create_at(p, index)?;
        let sheet_id = self.fill_sheet(id, spec, None)?;
        self.doc.commit();
        Ok((id.to_string(), sheet_id))
    }

    /// Adds a side sheet to the `side` of main sheet `main`, `index` steps outward (0 = next to it).
    pub fn add_side_sheet(&self, main: &str, side: Side, index: usize, spec: &SheetSpec) -> Result<(NodeId, String)> {
        let main_id = self.node(main)?;
        if self.kind(main_id) != "sheet" || self.side_of(main_id).is_some() {
            return Err(Error::Invalid("side sheets attach to a main sheet".into()));
        }
        // Position among all children so that the same-side order matches `index`.
        let children = self.tree.children(main_id).unwrap_or_default();
        let same: Vec<usize> = children.iter().enumerate().filter(|(_, c)| self.side_of(**c) == Some(side)).map(|(i, _)| i).collect();
        let pos = if index < same.len() { same[index] } else { children.len() };
        let id = self.tree.create_at(TreeParentId::Node(main_id), pos)?;
        let sheet_id = self.fill_sheet(id, spec, Some(side))?;
        self.doc.commit();
        Ok((id.to_string(), sheet_id))
    }

    fn fill_sheet(&self, id: TreeID, spec: &SheetSpec, side: Option<Side>) -> Result<String> {
        let m = self.tree.get_meta(id)?;
        let sheet_id = ids::new_id(self.doc.peer_id());
        m.insert(K_KIND, "sheet")?;
        m.insert(K_SHEET, sheet_id.as_str())?;
        m.insert(K_W, spec.width as f64)?;
        m.insert(K_H, spec.height as f64)?;
        m.insert(K_BG, spec.background.as_str())?;
        m.insert(K_SPACING, spec.spacing as f64)?;
        if let Some((blob, page)) = &spec.pdf {
            m.insert(K_PDF, blob.as_str())?;
            m.insert(K_PDF_PAGE, *page as i64)?;
            if !spec.gaps.is_empty() {
                m.insert(K_GAPS, gaps_text(&spec.gaps))?;
            }
        }
        if let Some(s) = side {
            m.insert(K_SIDE, s.as_str())?;
        }
        Ok(sheet_id)
    }

    /// Moves a node (group or main sheet) to `index` under `parent` (None = main column).
    pub fn move_node(&self, node: &str, parent: Option<&str>, index: usize) -> Result<()> {
        let id = self.node(node)?;
        if self.side_of(id).is_some() {
            return Err(Error::Invalid("move side sheets with move_side_sheet".into()));
        }
        let p = self.parent_for_new(parent)?;
        if let TreeParentId::Node(pid) = p
            && (pid == id || self.is_ancestor(id, pid))
        {
            return Err(Error::Invalid("a group cannot move into itself".into()));
        }
        // Moving within the same parent: the index refers to the list without the node.
        self.tree.mov_to(id, p, index.min(self.child_count(p).saturating_sub(self.same_parent(id, p) as usize)))?;
        self.doc.commit();
        Ok(())
    }

    pub fn set_collapsed(&self, group: &str, collapsed: bool) -> Result<()> {
        let id = self.node(group)?;
        if self.kind(id) != "group" {
            return Err(Error::Invalid("only groups collapse".into()));
        }
        self.tree.get_meta(id)?.insert(K_COLLAPSED, collapsed)?;
        self.doc.commit();
        Ok(())
    }

    pub fn set_title_of(&self, node: &str, title: &str) -> Result<()> {
        let id = self.node(node)?;
        self.tree.get_meta(id)?.insert(K_TITLE, title)?;
        self.doc.commit();
        Ok(())
    }

    /// Changes a sheet's paper (also side sheets). Size and PDF background stay.
    pub fn set_paper(&self, node: &str, background: Background, spacing: f32) -> Result<()> {
        let id = self.node(node)?;
        if self.kind(id) != "sheet" {
            return Err(Error::Invalid("only sheets have paper".into()));
        }
        let m = self.tree.get_meta(id)?;
        m.insert(K_BG, background.as_str())?;
        m.insert(K_SPACING, spacing as f64)?;
        self.doc.commit();
        Ok(())
    }

    /// Changes a sheet's size (turned to landscape, A5, a size of its own). What is on the page
    /// keeps its place (page points from the top-left corner), so turning it back brings
    /// everything back where it was. PDF pages keep the PDF's size and boards stay infinite.
    pub fn set_size(&self, node: &str, width: f32, height: f32) -> Result<()> {
        let id = self.node(node)?;
        if self.kind(id) != "sheet" {
            return Err(Error::Invalid("only sheets have a size".into()));
        }
        let spec = self.spec(id);
        if spec.pdf.is_some() || spec.is_board() {
            return Err(Error::Invalid("PDF pages and boards keep their size".into()));
        }
        let ok = |v: f32| v.is_finite() && (crate::format::MIN_PT..=crate::format::MAX_PT).contains(&v);
        if !ok(width) || !ok(height) {
            return Err(Error::Invalid(format!("page size {width}×{height} pt is out of range")));
        }
        let m = self.tree.get_meta(id)?;
        m.insert(K_W, width as f64)?;
        m.insert(K_H, height as f64)?;
        self.doc.commit();
        Ok(())
    }

    /// Space inserted into or taken out of a sheet (see [`plan_space`]): its new height and,
    /// for a PDF page, where its background splits. Also what undoing that puts back.
    pub fn reshape(&self, node: &str, height: f32, gaps: &[(f32, f32)]) -> Result<()> {
        let id = self.node(node)?;
        if self.kind(id) != "sheet" {
            return Err(Error::Invalid("only sheets have a size".into()));
        }
        let spec = self.spec(id);
        if spec.is_board() {
            return Err(Error::Invalid("boards have no height".into()));
        }
        let ok = height.is_finite() && (crate::format::MIN_PT..=crate::format::MAX_PT).contains(&height);
        let gaps_ok = gaps.iter().all(|g| g.0.is_finite() && g.1.is_finite() && g.1 > 0.0);
        if !ok || !gaps_ok || (spec.pdf.is_none() && !gaps.is_empty()) {
            return Err(Error::Invalid(format!("page height {height} pt or its gaps are out of range")));
        }
        let m = self.tree.get_meta(id)?;
        m.insert(K_H, height as f64)?;
        m.insert(K_GAPS, gaps_text(gaps))?;
        self.doc.commit();
        Ok(())
    }

    /// Deletes a node with everything under it (a sheet takes its side sheets along).
    pub fn delete(&self, node: &str) -> Result<()> {
        let id = self.node(node)?;
        self.tree.delete(id)?;
        self.doc.commit();
        Ok(())
    }

    /// Deletes like [`Notebook::delete`], and remembers it for [`Notebook::undo_delete`] (a page
    /// comes back with its side sheets and content; the sheets' own documents are untouched).
    pub fn delete_undoable(&self, node: &str) -> Result<()> {
        let id = self.node(node)?;
        let um = loro::UndoManager::new(&self.doc);
        self.tree.delete(id)?;
        self.doc.commit();
        *self.undo_delete.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(um);
        Ok(())
    }

    /// Takes back the last [`Notebook::delete_undoable`]; false when there is nothing to undo.
    pub fn undo_delete(&self) -> Result<bool> {
        let um = self.undo_delete.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
        let Some(mut um) = um else { return Ok(false) };
        let done = um.undo()?;
        self.doc.commit();
        Ok(done)
    }

    /// The flattened outline: groups and main sheets in reading order with their depth;
    /// children of collapsed groups are left out unless `expand_all`.
    pub fn outline(&self, expand_all: bool) -> Vec<OutlineRow> {
        let mut out = Vec::new();
        self.walk(TreeParentId::Root, 0, expand_all, &mut out);
        out
    }

    /// Every sheet id in reading order, side sheets right after their main sheet (left side
    /// first, from the outermost), independent of collapsed groups. This is the PDF export order.
    pub fn sheet_order(&self) -> Vec<String> {
        let mut out = Vec::new();
        for row in self.outline(true) {
            if let OutlineRow::Sheet { sheet_id, left, right, .. } = row {
                out.extend(left.iter().rev().map(|s| s.sheet_id.clone()));
                out.push(sheet_id);
                out.extend(right.iter().map(|s| s.sheet_id.clone()));
            }
        }
        out
    }

    fn walk(&self, parent: TreeParentId, depth: u32, expand_all: bool, out: &mut Vec<OutlineRow>) {
        for id in self.tree.children(parent).unwrap_or_default() {
            match self.kind(id).as_str() {
                "group" => {
                    let collapsed = self.meta_bool(id, K_COLLAPSED);
                    let count = self.count_sheets(id);
                    out.push(OutlineRow::Group { node: id.to_string(), depth, title: self.meta_string(id, K_TITLE), collapsed, count });
                    if !collapsed || expand_all {
                        self.walk(TreeParentId::Node(id), depth + 1, expand_all, out);
                    }
                }
                "sheet" => {
                    let (mut left, mut right) = (Vec::new(), Vec::new());
                    for c in self.tree.children(TreeParentId::Node(id)).unwrap_or_default() {
                        let s = SideSheet { node: c.to_string(), sheet_id: self.meta_string(c, K_SHEET), spec: self.spec(c) };
                        match self.side_of(c) {
                            Some(Side::Left) => left.push(s),
                            Some(Side::Right) => right.push(s),
                            None => {}
                        }
                    }
                    out.push(OutlineRow::Sheet {
                        node: id.to_string(),
                        depth,
                        sheet_id: self.meta_string(id, K_SHEET),
                        title: self.meta_string(id, K_TITLE),
                        spec: self.spec(id),
                        left,
                        right,
                    });
                }
                _ => {} // half-synced or unknown node: skip
            }
        }
    }

    fn count_sheets(&self, id: TreeID) -> u32 {
        let mut n = 0;
        for c in self.tree.children(TreeParentId::Node(id)).unwrap_or_default() {
            match self.kind(c).as_str() {
                "group" => n += self.count_sheets(c),
                "sheet" => n += 1,
                _ => {}
            }
        }
        n
    }

    fn spec(&self, id: TreeID) -> SheetSpec {
        let pdf = match self.meta_string(id, K_PDF) {
            s if s.is_empty() => None,
            blob => Some((blob, self.meta_f64(id, K_PDF_PAGE) as u32)),
        };
        SheetSpec {
            width: self.meta_f64(id, K_W) as f32,
            height: self.meta_f64(id, K_H) as f32,
            background: Background::parse(&self.meta_string(id, K_BG)),
            spacing: self.meta_f64(id, K_SPACING) as f32,
            pdf,
            gaps: parse_gaps(&self.meta_string(id, K_GAPS)),
        }
    }

    fn kind(&self, id: TreeID) -> String {
        self.meta_string(id, K_KIND)
    }
    fn side_of(&self, id: TreeID) -> Option<Side> {
        match self.meta_string(id, K_SIDE).as_str() {
            "left" => Some(Side::Left),
            "right" => Some(Side::Right),
            _ => None,
        }
    }
    fn meta_string(&self, id: TreeID, key: &str) -> String {
        self.tree.get_meta(id).map(|m| value_str(m.get(key))).unwrap_or_default()
    }
    fn meta_f64(&self, id: TreeID, key: &str) -> f64 {
        match self.tree.get_meta(id).ok().and_then(|m| m.get(key)) {
            Some(ValueOrContainer::Value(LoroValue::Double(v))) => v,
            Some(ValueOrContainer::Value(LoroValue::I64(v))) => v as f64,
            _ => 0.0,
        }
    }
    fn meta_bool(&self, id: TreeID, key: &str) -> bool {
        matches!(self.tree.get_meta(id).ok().and_then(|m| m.get(key)), Some(ValueOrContainer::Value(LoroValue::Bool(true))))
    }

    fn node(&self, s: &str) -> Result<TreeID> {
        let id = TreeID::try_from(s).map_err(|_| Error::NotFound(format!("node {s}")))?;
        if !self.tree.contains(id) || self.tree.is_node_deleted(&id).unwrap_or(true) {
            return Err(Error::NotFound(format!("node {s}")));
        }
        Ok(id)
    }

    fn parent_for_new(&self, parent: Option<&str>) -> Result<TreeParentId> {
        match parent {
            None => Ok(TreeParentId::Root),
            Some(p) => {
                let id = self.node(p)?;
                if self.kind(id) != "group" {
                    return Err(Error::Invalid("sheets and groups go into the main column or a group".into()));
                }
                Ok(TreeParentId::Node(id))
            }
        }
    }

    fn create_at(&self, parent: TreeParentId, index: usize) -> Result<TreeID> {
        let n = self.child_count(parent);
        Ok(self.tree.create_at(parent, index.min(n))?)
    }

    fn child_count(&self, parent: TreeParentId) -> usize {
        self.tree.children_num(parent).unwrap_or(0)
    }

    fn same_parent(&self, id: TreeID, parent: TreeParentId) -> bool {
        self.tree.parent(id) == Some(parent)
    }

    fn is_ancestor(&self, ancestor: TreeID, mut node: TreeID) -> bool {
        while let Some(TreeParentId::Node(p)) = self.tree.parent(node) {
            if p == ancestor {
                return true;
            }
            node = p;
        }
        false
    }

    // --- persistence and sync -------------------------------------------------------------

    pub fn snapshot(&self) -> Result<Vec<u8>> {
        Ok(self.doc.export(ExportMode::Snapshot)?)
    }

    /// The version this replica has (send it to a peer; it answers with [`Self::updates_since`]).
    pub fn version(&self) -> Vec<u8> {
        self.doc.oplog_vv().encode()
    }

    /// Everything a peer at `version` is missing.
    pub fn updates_since(&self, version: &[u8]) -> Result<Vec<u8>> {
        let vv = VersionVector::decode(version).map_err(|e| Error::Format(e.to_string()))?;
        Ok(self.doc.export(ExportMode::updates(&vv))?)
    }

    /// Merges updates or a snapshot from another replica.
    pub fn import(&self, bytes: &[u8]) -> Result<()> {
        self.doc.import(bytes)?;
        Ok(())
    }
}

fn value_str(v: Option<ValueOrContainer>) -> String {
    match v {
        Some(ValueOrContainer::Value(LoroValue::String(s))) => s.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(nb: &Notebook, all: bool) -> Vec<String> {
        nb.outline(all)
            .into_iter()
            .map(|r| match r {
                OutlineRow::Group { title, depth, collapsed, count, .. } => format!("{}G:{title}{}({count})", "  ".repeat(depth as usize), if collapsed { "+" } else { "" }),
                OutlineRow::Sheet { depth, left, right, .. } => format!("{}S[{}|{}]", "  ".repeat(depth as usize), left.len(), right.len()),
            })
            .collect()
    }

    fn pdf_spec() -> SheetSpec {
        SheetSpec { pdf: Some(("b".repeat(64), 0)), ..SheetSpec::a4(Background::Blank) }
    }

    #[test]
    fn inserted_space_makes_pages_longer_and_splits_pdf_pages() {
        let a4 = SheetSpec::a4(Background::Lines);
        // Room at the bottom: what lies below moves into it, the page keeps its size.
        let p = plan_space(&a4, 400.0, 100.0, Below { top: Some(420.0), bottom: Some(600.0) });
        assert_eq!((p.dy, p.height), (100.0, 842.0));
        // Pushed past the bottom: the page grows just enough (nothing goes to the next page).
        let p = plan_space(&a4, 400.0, 100.0, Below { top: Some(420.0), bottom: Some(800.0) });
        assert_eq!((p.dy, p.height), (100.0, 912.0));
        assert!(p.gaps.is_empty());
        // Taking space out stops where the ink below would reach the line.
        let p = plan_space(&a4, 300.0, -300.0, Below { top: Some(500.0), bottom: Some(600.0) });
        assert_eq!((p.dy, p.height), (-200.0, 842.0));
        assert_eq!(plan_space(&a4, 300.0, -50.0, Below::default()).dy, 0.0, "nothing below: nothing to pull up");

        // A PDF page splits where the space goes in and grows by it.
        let pdf = pdf_spec();
        let p = plan_space(&pdf, 300.0, 100.0, Below::default());
        assert_eq!((p.dy, p.height, p.gaps.clone()), (100.0, 942.0, vec![(300.0, 100.0)]));
        let one = SheetSpec { height: p.height, gaps: p.gaps, ..pdf.clone() };
        assert_eq!(one.base_height(), 842.0);
        assert_eq!(one.bands(), vec![Band { src: 0.0, len: 300.0, dst: 0.0 }, Band { src: 300.0, len: 542.0, dst: 400.0 }]);
        // More space inside the blank part widens that gap; further down, a new gap where the PDF is.
        let p = plan_space(&one, 350.0, 50.0, Below::default());
        assert_eq!(p.gaps, vec![(300.0, 150.0)]);
        let p = plan_space(&one, 500.0, 40.0, Below::default());
        assert_eq!((p.height, p.gaps), (982.0, vec![(300.0, 100.0), (400.0, 40.0)]));
        // Out again: only the blank space, and not past the ink below it.
        let p = plan_space(&one, 320.0, -200.0, Below { top: Some(460.0), bottom: Some(900.0) });
        assert_eq!((p.dy, p.height, p.gaps), (-80.0, 862.0, vec![(300.0, 20.0)]));
        let p = plan_space(&one, 300.0, -100.0, Below::default());
        assert_eq!((p.dy, p.height, p.gaps.is_empty()), (-100.0, 842.0, true), "all of it: the PDF is whole again");
        assert_eq!(plan_space(&one, 200.0, -50.0, Below::default()).dy, 0.0, "the printed sheet never shrinks");
    }

    #[test]
    fn reshape_keeps_height_and_gaps_and_refuses_gaps_on_plain_pages() {
        let nb = Notebook::create(1, "Platz").unwrap();
        let (pdf, _) = nb.add_sheet(None, 0, &pdf_spec()).unwrap();
        let (plain, _) = nb.add_sheet(None, 1, &SheetSpec::a4(Background::Grid)).unwrap();
        nb.reshape(&pdf, 942.0, &[(300.0, 100.0)]).unwrap();
        nb.reshape(&plain, 900.0, &[]).unwrap();
        assert!(nb.reshape(&plain, 900.0, &[(10.0, 5.0)]).is_err());
        let specs = nb.sheet_specs();
        let s = specs.values().find(|s| s.pdf.is_some()).unwrap();
        assert_eq!((s.height, s.gaps.clone(), s.base_height()), (942.0, vec![(300.0, 100.0)], 842.0));
        assert!(specs.values().any(|s| s.pdf.is_none() && s.height == 900.0));
        let bytes = nb.snapshot().unwrap();
        let again = Notebook::load(2, &bytes).unwrap();
        assert!(again.sheet_specs().values().any(|s| s.gaps == vec![(300.0, 100.0)]));
    }

    #[test]
    fn a_page_turns_to_landscape_or_takes_any_size_but_pdf_pages_and_boards_keep_theirs() {
        let nb = Notebook::create(1, "Formate").unwrap();
        let (s1, _) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Grid)).unwrap();
        let (side, _) = nb.add_side_sheet(&s1, Side::Right, 0, &SheetSpec::a4(Background::Blank)).unwrap();
        nb.set_size(&s1, 842.0, 595.0).unwrap();
        nb.set_size(&side, 300.0, 400.0).unwrap();
        let specs: Vec<_> = nb.sheet_specs().into_values().map(|s| (s.width, s.height, s.background)).collect();
        assert!(specs.contains(&(842.0, 595.0, Background::Grid)), "paper stays: {specs:?}");
        assert!(specs.contains(&(300.0, 400.0, Background::Blank)));
        assert!(nb.set_size(&s1, 10.0, 595.0).is_err(), "too small");
        assert!(nb.set_size(&s1, f32::NAN, 595.0).is_err());
        let (pdf, _) = nb.add_sheet(None, 1, &SheetSpec { pdf: Some(("b".repeat(64), 0)), ..SheetSpec::a4(Background::Blank) }).unwrap();
        let (board, _) = nb.add_sheet(None, 2, &SheetSpec::board()).unwrap();
        assert!(nb.set_size(&pdf, 842.0, 595.0).is_err());
        assert!(nb.set_size(&board, 842.0, 595.0).is_err());
        let g = nb.add_group(None, 3, "G").unwrap();
        assert!(nb.set_size(&g, 842.0, 595.0).is_err());
        // It syncs like every other change.
        let other = Notebook::load(2, &nb.snapshot().unwrap()).unwrap();
        assert!(other.sheet_specs().values().any(|s| (s.width, s.height) == (842.0, 595.0)));
    }

    #[test]
    fn paper_changes_after_creation_also_on_side_sheets() {
        let nb = Notebook::create(1, "p").unwrap();
        let g = nb.add_group(None, 0, "G").unwrap();
        let (s1, _) = nb.add_sheet(None, 1, &SheetSpec::a4(Background::Grid)).unwrap();
        let (side, _) = nb.add_side_sheet(&s1, Side::Left, 0, &SheetSpec::a4(Background::Blank)).unwrap();
        nb.set_paper(&s1, Background::Lines, 24.0).unwrap();
        nb.set_paper(&side, Background::Dots, 18.0).unwrap();
        assert!(nb.set_paper(&g, Background::Lines, 20.0).is_err(), "groups have no paper");
        let row = nb.outline(true).into_iter().find_map(|r| match r { OutlineRow::Sheet { spec, left, .. } => Some((spec, left)), _ => None }).unwrap();
        assert_eq!((row.0.background, row.0.spacing, row.0.width), (Background::Lines, 24.0, 595.0));
        assert_eq!((row.1[0].spec.background, row.1[0].spec.spacing), (Background::Dots, 18.0));
    }

    #[test]
    fn groups_sheets_and_side_sheets() {
        let nb = Notebook::create(1, "Mathe WS26").unwrap();
        let (s1, _) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Grid)).unwrap();
        let weekly = nb.add_group(None, 9, "Wöchentliche Aufgaben").unwrap();
        let w1 = nb.add_group(Some(&weekly), 0, "Week01").unwrap();
        nb.add_sheet(Some(&w1), 0, &SheetSpec::a4(Background::Lines)).unwrap();
        nb.add_sheet(Some(&w1), 1, &SheetSpec::board()).unwrap();
        nb.add_side_sheet(&s1, Side::Right, 0, &SheetSpec::a4(Background::Blank)).unwrap();
        nb.add_side_sheet(&s1, Side::Right, 5, &SheetSpec::a4(Background::Blank)).unwrap();
        nb.add_side_sheet(&s1, Side::Left, 0, &SheetSpec::a4(Background::Dots)).unwrap();
        assert_eq!(titles(&nb, false), vec!["S[1|2]", "G:Wöchentliche Aufgaben(2)", "  G:Week01(2)", "    S[0|0]", "    S[0|0]"]);
        nb.set_collapsed(&w1, true).unwrap();
        assert_eq!(titles(&nb, false), vec!["S[1|2]", "G:Wöchentliche Aufgaben(2)", "  G:Week01+(2)"]);
        assert_eq!(titles(&nb, true).len(), 5);
        assert_eq!(nb.sheet_order().len(), 6);
        // A side sheet of a side sheet, or a sheet inside a sheet, is refused.
        assert!(nb.add_sheet(Some(&s1), 0, &SheetSpec::board()).is_err());
        assert!(nb.move_node(&weekly, Some(&w1), 0).is_err());
    }

    #[test]
    fn a_deleted_page_comes_back_with_its_side_sheets() {
        let nb = Notebook::create(1, "x").unwrap();
        nb.add_sheet(None, 0, &SheetSpec::a4(Background::Grid)).unwrap();
        let (s2, sheet2) = nb.add_sheet(None, 1, &SheetSpec::a4(Background::Lines)).unwrap();
        nb.add_sheet(None, 2, &SheetSpec::a4(Background::Dots)).unwrap();
        let (side, _) = nb.add_side_sheet(&s2, Side::Right, 0, &SheetSpec::a4(Background::Blank)).unwrap();
        let before = titles(&nb, true);
        // A side sheet alone.
        nb.delete_undoable(&side).unwrap();
        assert_eq!(titles(&nb, true)[1], "S[0|0]");
        assert!(nb.undo_delete().unwrap());
        assert_eq!(titles(&nb, true), before);
        // A page with its side sheet, back in its place, with the same sheet.
        nb.delete_undoable(&s2).unwrap();
        assert_eq!(titles(&nb, true).len(), 2);
        assert!(nb.undo_delete().unwrap());
        assert_eq!(titles(&nb, true), before);
        assert_eq!(nb.sheet_order()[1], sheet2);
        // Only once.
        assert!(!nb.undo_delete().unwrap());
        // It survives a save and reload.
        let again = Notebook::load(1, &nb.snapshot().unwrap()).unwrap();
        assert_eq!(titles(&again, true), before);
    }

    #[test]
    fn concurrent_edits_converge() {
        let a = Notebook::create(1, "x").unwrap();
        let (s, _) = a.add_sheet(None, 0, &SheetSpec::a4(Background::Blank)).unwrap();
        let b = Notebook::load(2, &a.snapshot().unwrap()).unwrap();
        // Concurrently: A inserts before, B adds a side sheet and a group after.
        a.add_sheet(None, 0, &SheetSpec::board()).unwrap();
        b.add_side_sheet(&s, Side::Right, 0, &SheetSpec::a4(Background::Lines)).unwrap();
        b.add_group(None, 9, "Notes").unwrap();
        let (va, vb) = (a.version(), b.version());
        a.import(&b.updates_since(&va).unwrap()).unwrap();
        b.import(&a.updates_since(&vb).unwrap()).unwrap();
        assert_eq!(titles(&a, true), titles(&b, true));
        assert_eq!(titles(&a, true), vec!["S[0|0]", "S[0|1]", "G:Notes(0)"]);
    }

    #[test]
    fn moving_into_a_group_and_back() {
        let nb = Notebook::create(1, "x").unwrap();
        let g = nb.add_group(None, 0, "G").unwrap();
        let (s, _) = nb.add_sheet(None, 1, &SheetSpec::board()).unwrap();
        nb.move_node(&s, Some(&g), 0).unwrap();
        assert_eq!(titles(&nb, true), vec!["G:G(1)", "  S[0|0]"]);
        nb.move_node(&s, None, 0).unwrap();
        assert_eq!(titles(&nb, true), vec!["S[0|0]", "G:G(0)"]);
        nb.delete(&g).unwrap();
        assert_eq!(titles(&nb, true), vec!["S[0|0]"]);
    }
}
