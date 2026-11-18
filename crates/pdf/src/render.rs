//! Rendering whole pages and tiles of pages to RGBA.

use std::sync::{Mutex, PoisonError};

use pdfium_render::prelude::{PdfBitmap, PdfBitmapFormat, PdfColor, PdfPage, PdfRenderConfig};

use crate::{Error, PageSize, PdfDoc, Result, lock};

/// A rectangle of pixels inside a page rendered at some scale.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Tile {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Straight (non-premultiplied) RGBA, 4 bytes per pixel, rows top to bottom, no padding.
/// Rendered pages are opaque (white paper), so alpha is always 255.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

/// Pixel size of a whole page rendered at `scale` pixels per point. Tiles of the same page and
/// scale all use this size, so they line up exactly.
pub fn page_pixels(size: PageSize, scale: f32) -> (u32, u32) {
    ((size.width * scale).round().max(1.0) as u32, (size.height * scale).round().max(1.0) as u32)
}

/// Splits a page of `page_px` pixels into tiles of at most `tile` × `tile` pixels, row by row.
/// The tiles cover the page exactly, without overlap; edge tiles are smaller.
pub fn tiles(page_px: (u32, u32), tile: u32) -> Vec<Tile> {
    assert!(tile > 0, "tile size must be positive");
    let (w, h) = page_px;
    let mut out = Vec::with_capacity((w.div_ceil(tile) * h.div_ceil(tile)) as usize);
    for y in (0..h).step_by(tile as usize) {
        for x in (0..w).step_by(tile as usize) {
            out.push(Tile { x, y, width: tile.min(w - x), height: tile.min(h - y) });
        }
    }
    out
}

impl PdfDoc {
    /// Renders a whole page `width_px` pixels wide (height from the aspect ratio).
    pub fn render_page(&self, page: u32, width_px: u32) -> Result<Rgba> {
        let _g = lock();
        let size = self.size_locked(page)?;
        let scale = width_px as f32 / size.width;
        let (w, h) = (width_px.max(1), page_pixels(size, scale).1);
        let mut data = vec![0u8; w as usize * h as usize * 4];
        self.render_locked(page, (w, h), Tile { x: 0, y: 0, width: w, height: h }, &mut data)?;
        Ok(Rgba { width: w, height: h, data })
    }

    /// Renders `tile` of the page drawn at `scale` pixels per point (see [`page_pixels`]).
    pub fn render_tile(&self, page: u32, scale: f32, tile: Tile) -> Result<Rgba> {
        let mut data = vec![0u8; tile.width as usize * tile.height as usize * 4];
        self.render_tile_into(page, scale, tile, &mut data)?;
        Ok(Rgba { width: tile.width, height: tile.height, data })
    }

    /// Like [`PdfDoc::render_tile`], into a caller-owned RGBA buffer of exactly
    /// `tile.width * tile.height * 4` bytes (lets a tile cache reuse its buffers).
    pub fn render_tile_into(&self, page: u32, scale: f32, tile: Tile, out: &mut [u8]) -> Result<()> {
        if !(scale.is_finite() && scale > 0.0) {
            return Err(Error::Invalid(format!("bad scale {scale}")));
        }
        let _g = lock();
        let full = page_pixels(self.size_locked(page)?, scale);
        self.render_locked(page, full, tile, out)
    }

    fn render_locked(&self, page: u32, full: (u32, u32), tile: Tile, out: &mut [u8]) -> Result<()> {
        if tile.width == 0 || tile.height == 0 || tile.x + tile.width > full.0 || tile.y + tile.height > full.1 {
            return Err(Error::Invalid(format!("tile {tile:?} outside page of {}x{} px", full.0, full.1)));
        }
        if out.len() != tile.width as usize * tile.height as usize * 4 {
            return Err(Error::Invalid(format!("buffer of {} bytes for a {}x{} tile", out.len(), tile.width, tile.height)));
        }
        let max = i32::MAX as u32;
        if full.0 > max || full.1 > max {
            return Err(Error::Invalid("page too large to render".into()));
        }
        let (mx, my) = (TILE_OVERSCAN.min(tile.x), TILE_OVERSCAN.min(tile.y));
        self.with_page(page, |p| {
            if mx == 0 && my == 0 {
                return render_into(p, full, tile, out);
            }
            let wide = Tile { x: tile.x - mx, y: tile.y - my, width: tile.width + mx, height: tile.height + my };
            let mut scratch = SCRATCH.lock().unwrap_or_else(PoisonError::into_inner);
            scratch.resize(wide.width as usize * wide.height as usize * 4, 0);
            render_into(p, full, wide, &mut scratch)?;
            let (row, wide_row) = (tile.width as usize * 4, wide.width as usize * 4);
            for (y, dst) in out.chunks_exact_mut(row).enumerate() {
                let src = (y + my as usize) * wide_row + mx as usize * 4;
                dst.copy_from_slice(&scratch[src..src + row]);
            }
            Ok(())
        })
    }
}

/// Extra pixels rendered left of and above a tile and then dropped. pdfium places cached glyph
/// bitmaps (text below ~50 px) slightly differently when a glyph starts outside the bitmap, which
/// shows as a sub-pixel step in letters crossing a tile seam. With the overscan, glyphs reaching
/// into the tile start inside the rendered area and tiles match a whole-page render exactly.
pub const TILE_OVERSCAN: u32 = 64;

/// Reused buffer for overscanned tiles (only touched under the crate lock).
static SCRATCH: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// Renders `tile` of the page laid out at `full` pixels straight into `out`; pdfium clips to the
/// bitmap.
fn render_into(p: &PdfPage, full: (u32, u32), tile: Tile, out: &mut [u8]) -> Result<()> {
    let mut bitmap = PdfBitmap::from_bytes(tile.width as i32, tile.height as i32, PdfBitmapFormat::BGRA, out)?;
    let config = PdfRenderConfig::new()
        .set_fixed_size(full.0 as i32, full.1 as i32)
        .set_origin(-(tile.x as i32), -(tile.y as i32))
        .set_reverse_byte_order(true) // BGRA -> RGBA
        .clear_before_rendering(true)
        .set_clear_color(PdfColor::WHITE)
        .render_annotations(true)
        .render_form_data(true);
    p.render_into_bitmap_with_config(&mut bitmap, &config)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiles_cover_the_page_exactly() {
        for &(w, h, t) in &[(1600u32, 2263u32, 512u32), (512, 512, 512), (1, 1, 256), (1000, 10, 7), (511, 1025, 512)] {
            let mut hit = vec![0u8; (w * h) as usize];
            for tile in tiles((w, h), t) {
                assert!(tile.width > 0 && tile.height > 0 && tile.width <= t && tile.height <= t);
                assert!(tile.x + tile.width <= w && tile.y + tile.height <= h, "{tile:?} outside {w}x{h}");
                for y in tile.y..tile.y + tile.height {
                    for x in tile.x..tile.x + tile.width {
                        hit[(y * w + x) as usize] += 1;
                    }
                }
            }
            assert!(hit.iter().all(|&c| c == 1), "{w}x{h}/{t}: pixels not covered exactly once");
        }
    }

    #[test]
    fn page_pixels_rounds() {
        let a4 = PageSize { width: 595.0, height: 842.0 };
        assert_eq!(page_pixels(a4, 1.0), (595, 842));
        assert_eq!(page_pixels(a4, 1600.0 / 595.0), (1600, 2264));
    }
}
