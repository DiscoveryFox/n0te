//! Where pages go in the notebook view, the same on every device: one column of main sheets
//! (top to bottom, group titles between them), side sheets left and right of their main sheet,
//! boards as landscape frames. Coordinates are page points, x = 0 is the column's centre.

use std::collections::HashMap;

use crate::ink::Rect;
use crate::notebook::{OutlineRow, SheetSpec};

pub const TOP: f32 = 40.0;
pub const HEADER_H: f32 = 34.0;
pub const A4_W: f32 = 595.0;
/// A board's frame in the column (the board itself is infinite).
pub const BOARD_W: f32 = 842.0;
pub const BOARD_H: f32 = 595.0;

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Between page rows.
    pub row_gap: f32,
    /// Between a sheet and its side sheets.
    pub side_gap: f32,
}

impl Default for Options {
    fn default() -> Self {
        Options { row_gap: 36.0, side_gap: 28.0 }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub sheet_id: String,
    pub node: String,
    /// The main sheet's node (itself for a main sheet).
    pub main_node: String,
    pub spec: SheetSpec,
    pub rect: Rect,
    /// 0 main sheet, -1, -2 … left, 1, 2 … right.
    pub side: i32,
    /// Page number over all pages (folded ones too); side sheets carry their main page's.
    pub number: u32,
}

impl Placed {
    pub fn is_board(&self) -> bool {
        self.spec.is_board()
    }
    /// "3", or "3 L1" / "3 R2" for side sheets.
    pub fn label(&self) -> String {
        match self.side {
            0 => self.number.to_string(),
            s if s < 0 => format!("{} L{}", self.number, -s),
            s => format!("{} R{}", self.number, s),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    pub node: String,
    pub title: String,
    pub depth: u32,
    pub collapsed: bool,
    pub count: u32,
    pub rect: Rect,
}

#[derive(Debug, Clone, Default)]
pub struct Layout {
    pub sheets: Vec<Placed>,
    pub headers: Vec<Header>,
    pub bounds: Rect,
    pub total_pages: u32,
}

/// Size of a sheet in the column (boards get their frame).
pub fn display_size(spec: &SheetSpec) -> (f32, f32) {
    if spec.is_board() { (BOARD_W, BOARD_H) } else { (spec.width, spec.height) }
}

/// Lays out the visible rows (`outline(false)`); `all` (`outline(true)`) numbers the pages.
pub fn layout(visible: &[OutlineRow], all: &[OutlineRow], opt: Options) -> Layout {
    let mut numbers = HashMap::new();
    for row in all {
        if let OutlineRow::Sheet { node, .. } = row {
            let n = numbers.len() as u32 + 1;
            numbers.insert(node.clone(), n);
        }
    }
    let mut out = Layout { bounds: Rect::EMPTY, total_pages: numbers.len() as u32, ..Default::default() };
    let mut y = TOP;
    let mut last = 0;
    for row in visible {
        match row {
            OutlineRow::Group { node, depth, title, collapsed, count } => {
                let left = -A4_W / 2.0 + *depth as f32 * 18.0;
                let r = Rect::new(left, y, A4_W / 2.0, y + HEADER_H);
                out.headers.push(Header { node: node.clone(), title: title.clone(), depth: *depth, collapsed: *collapsed, count: *count, rect: r });
                out.bounds = out.bounds.union(&r);
                y += HEADER_H + 10.0;
            }
            OutlineRow::Sheet { node, sheet_id, spec, left, right, .. } => {
                let number = numbers.get(node).copied().unwrap_or(last + 1);
                last = number;
                let (w, h) = display_size(spec);
                let main = Rect::new(-w / 2.0, y, w / 2.0, y + h);
                out.sheets.push(Placed { sheet_id: sheet_id.clone(), node: node.clone(), main_node: node.clone(), spec: spec.clone(), rect: main, side: 0, number });
                let mut row_h = h;
                let mut x = main.x0;
                for (i, s) in left.iter().enumerate() {
                    let (sw, sh) = display_size(&s.spec);
                    x -= opt.side_gap + sw;
                    out.sheets.push(Placed { sheet_id: s.sheet_id.clone(), node: s.node.clone(), main_node: node.clone(), spec: s.spec.clone(), rect: Rect::new(x, y, x + sw, y + sh), side: -(i as i32 + 1), number });
                    row_h = row_h.max(sh);
                }
                let mut x = main.x1;
                for (i, s) in right.iter().enumerate() {
                    let (sw, sh) = display_size(&s.spec);
                    out.sheets.push(Placed { sheet_id: s.sheet_id.clone(), node: s.node.clone(), main_node: node.clone(), spec: s.spec.clone(), rect: Rect::new(x + opt.side_gap, y, x + opt.side_gap + sw, y + sh), side: i as i32 + 1, number });
                    x += opt.side_gap + sw;
                    row_h = row_h.max(sh);
                }
                y += row_h + opt.row_gap;
            }
        }
    }
    for p in &out.sheets {
        out.bounds = out.bounds.union(&p.rect);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notebook::{Background, Notebook, Side};

    #[test]
    fn column_side_sheets_groups_and_stable_numbers() {
        let nb = Notebook::create(1, "L").unwrap();
        let (s1, _) = nb.add_sheet(None, 0, &SheetSpec::a4(Background::Grid)).unwrap();
        nb.add_side_sheet(&s1, Side::Left, 0, &SheetSpec::a4(Background::Blank)).unwrap();
        nb.add_side_sheet(&s1, Side::Right, 0, &SheetSpec::a4(Background::Blank)).unwrap();
        let g = nb.add_group(None, 1, "Woche 1").unwrap();
        nb.add_sheet(Some(&g), 0, &SheetSpec::a4(Background::Lines)).unwrap();
        nb.add_sheet(None, 2, &SheetSpec::board()).unwrap();
        let l = layout(&nb.outline(false), &nb.outline(true), Options::default());
        assert_eq!(l.total_pages, 3);
        let main = &l.sheets[0];
        assert_eq!((main.rect.x0, main.rect.y0, main.rect.x1), (-297.5, TOP, 297.5));
        let left = l.sheets.iter().find(|p| p.side == -1).unwrap();
        assert_eq!(left.rect.x1, main.rect.x0 - 28.0);
        assert_eq!(left.label(), "1 L1");
        assert_eq!(l.headers.len(), 1);
        assert_eq!(l.headers[0].rect.y0, TOP + 842.0 + 36.0);
        let board = l.sheets.last().unwrap();
        assert!(board.is_board());
        assert_eq!((board.rect.x1 - board.rect.x0, board.number), (BOARD_W, 3));

        // Folding the group hides its page but the board keeps number 3.
        nb.set_collapsed(&g, true).unwrap();
        let folded = layout(&nb.outline(false), &nb.outline(true), Options::default());
        assert_eq!(folded.sheets.last().unwrap().number, 3);
        assert_eq!(folded.sheets.iter().filter(|p| p.side == 0).count(), 2);
    }
}
