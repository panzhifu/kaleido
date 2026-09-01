//! Selection-constrained rendering utilities.
//!
//! Helpers for processing only the pixels within a selection, which is
//! the key to performance when working with large images and small
//! selections.
//!
//! > **Legacy note:** these helpers operate on the old-model
//! > [`kaleido_traits::Selection`] (a flat `Vec<bool>` mask), not the
//! > document-wide [`kaleido_core::SelectionMask`] that the
//! > [`super::SelectionService`] manages. They are kept for the desktop /
//! > CLI hosts that still use the old model; new code should go through the
//! > service.

use kaleido_core::{Pixel, TileCoord, TiledImage, TILE_SIZE};
use kaleido_traits::Selection;

// ---------------------------------------------------------------------------
// Tile range calculation
// ---------------------------------------------------------------------------

/// Computes the range of tile coordinates that intersect a selection.
///
/// Returns `None` when the selection is empty. The result is an
/// exclusive-end tile range: `(start_col, start_row, end_col, end_row)`.
pub fn tile_range_for_selection(
    selection: &Selection,
    img_w: u32,
    img_h: u32,
) -> Option<(u32, u32, u32, u32)> {
    let bounds = selection.bounds()?;
    let (bx, by, bw, bh) = bounds;
    let start_col = bx / TILE_SIZE;
    let end_col = ((bx + bw).min(img_w)).div_ceil(TILE_SIZE);
    let start_row = by / TILE_SIZE;
    let end_row = ((by + bh).min(img_h)).div_ceil(TILE_SIZE);
    Some((start_col, start_row, end_col, end_row))
}

/// Returns the number of tiles in a region (for progress reporting).
pub fn tile_count_in_region(
    start_col: u32,
    start_row: u32,
    end_col: u32,
    end_row: u32,
) -> u32 {
    (end_col.saturating_sub(start_col)) * (end_row.saturating_sub(start_row))
}

// ---------------------------------------------------------------------------
// Selection mask iteration
// ---------------------------------------------------------------------------

/// Iterates over all selected pixels in a tile, calling the closure for
/// each one.
///
/// This skips:
/// - Tiles outside the selection bounds
/// - Pixels outside the selection mask (for non-rectangular selections)
pub fn for_each_selected_pixel<F>(
    image: &mut TiledImage,
    selection: &Selection,
    tile: TileCoord,
    mut f: F,
) where
    F: FnMut(u32, u32, Pixel) -> Pixel,
{
    let img_w = image.width();
    let img_h = image.height();
    let tile_x = tile.col * TILE_SIZE;
    let tile_y = tile.row * TILE_SIZE;

    for dy in 0..TILE_SIZE {
        let y = tile_y + dy;
        if y >= img_h {
            break;
        }
        for dx in 0..TILE_SIZE {
            let x = tile_x + dx;
            if x >= img_w {
                break;
            }
            if selection.contains(x, y) {
                let old = image.get_pixel(x, y);
                let new = f(x, y, old);
                image.set_pixel(x, y, new);
            }
        }
    }
}

/// Iterates over all selected pixels in the entire image.
///
/// This is the entry point for selection-constrained rendering:
/// tools call this with a closure that transforms each selected pixel.
pub fn apply_to_selection<F>(
    image: &mut TiledImage,
    selection: &Selection,
    mut f: F,
) where
    F: FnMut(u32, u32, Pixel) -> Pixel,
{
    let Some((start_col, start_row, end_col, end_row)) =
        tile_range_for_selection(selection, image.width(), image.height())
    else {
        return;
    };

    for row in start_row..end_row {
        for col in start_col..end_col {
            let tile = TileCoord::new(col, row);
            for_each_selected_pixel(image, selection, tile, &mut f);
        }
    }
}

// ---------------------------------------------------------------------------
// Selection overlay (marching ants)
// ---------------------------------------------------------------------------

/// Generates the pixel offsets for a marching-ants selection border.
///
/// Returns a list of (x, y) offsets that should be highlighted.
/// The `phase` parameter animates the ants (0..8).
pub fn marching_ants_offsets(
    selection: &Selection,
    phase: u8,
) -> Vec<(u32, u32)> {
    let mut offsets = Vec::new();
    let bounds = match selection.bounds() {
        Some(b) => b,
        None => return offsets,
    };
    let (bx, by, bw, bh) = bounds;
    let phase = phase % 8;

    // Top and bottom edges. A pixel is "lit" when its position plus the
    // animation phase lands in the first half of the 8-step cycle.
    for x in bx..(bx + bw) {
        let top_offset = (x as u8 + phase) % 8;
        if top_offset < 4 {
            offsets.push((x, by));
        }
        // `bh >= 1` here (bounds are `Some`), so the bottom edge always runs.
        let bottom_y = by + bh - 1;
        let bot_offset = (x as u8 + phase) % 8;
        if bot_offset < 4 {
            offsets.push((x, bottom_y));
        }
    }

    // Left and right edges.
    for y in by..(by + bh) {
        let left_offset = (y as u8 + phase) % 8;
        if left_offset < 4 {
            offsets.push((bx, y));
        }
        // `bw >= 1` here, so the right edge always runs.
        let right_x = bx + bw - 1;
        let right_offset = (y as u8 + phase) % 8;
        if right_offset < 4 {
            offsets.push((right_x, y));
        }
    }

    offsets
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::PixelFormat;

    #[test]
    fn test_tile_range_for_selection() {
        // Selection from (10, 20) with size 100x200 in a 512x512 image.
        // Tile size is 256. The selection spans:
        //   cols: 10..110  -> tile col 0 only (0..256)
        //   rows: 20..220  -> tile row 0 only (0..256)
        let sel = Selection::rect(10, 20, 100, 200, 512, 512);
        let range = tile_range_for_selection(&sel, 512, 512);
        assert_eq!(range, Some((0, 0, 1, 1)));
    }

    #[test]
    fn test_tile_range_multi_tile() {
        // Selection that spans multiple tiles in both dimensions.
        let sel = Selection::rect(50, 50, 200, 300, 512, 512);
        let range = tile_range_for_selection(&sel, 512, 512);
        // cols: 50..250 -> tile col 0 only (0..256)
        // rows: 50..350 -> tile rows 0, 1 (0..256, 256..512)
        assert_eq!(range, Some((0, 0, 1, 2)));
    }

    #[test]
    fn test_tile_range_empty_selection() {
        let sel = Selection::empty(512, 512);
        let range = tile_range_for_selection(&sel, 512, 512);
        assert_eq!(range, None);
    }

    #[test]
    fn test_tile_count_in_region() {
        assert_eq!(tile_count_in_region(0, 0, 2, 3), 6);
        assert_eq!(tile_count_in_region(1, 1, 1, 1), 0);
    }

    #[test]
    fn test_apply_to_selection() {
        let mut image =
            TiledImage::with_color(64, 64, PixelFormat::Rgba8, Pixel::rgb(100, 100, 100)).unwrap();
        let sel = Selection::rect(10, 10, 20, 20, 64, 64);

        apply_to_selection(&mut image, &sel, |_x, _y, _p| Pixel::rgb(255, 0, 0));

        // Pixel inside selection should be red.
        assert_eq!(image.get_pixel(15, 15), Pixel::rgb(255, 0, 0));
        // Pixel outside selection should be unchanged.
        assert_eq!(image.get_pixel(5, 5), Pixel::rgb(100, 100, 100));
        assert_eq!(image.get_pixel(50, 50), Pixel::rgb(100, 100, 100));
    }

    #[test]
    fn test_marching_ants() {
        let sel = Selection::rect(0, 0, 10, 10, 64, 64);
        let offsets = marching_ants_offsets(&sel, 0);
        // Should have some edge pixels.
        assert!(!offsets.is_empty());
        // Corner (0,0) should be included.
        assert!(offsets.contains(&(0, 0)));
    }

    #[test]
    fn test_marching_ants_single_pixel_row_or_col() {
        // Degenerate 1-pixel-wide / 1-pixel-tall selections must not panic
        // (the bottom / right edge coincides with the top / left edge).
        let sel = Selection::rect(5, 5, 1, 10, 64, 64);
        let offsets = marching_ants_offsets(&sel, 0);
        assert!(!offsets.is_empty());
        let sel = Selection::rect(5, 5, 10, 1, 64, 64);
        let offsets = marching_ants_offsets(&sel, 0);
        assert!(!offsets.is_empty());
    }

    #[test]
    fn test_for_each_selected_pixel() {
        let mut image =
            TiledImage::with_color(32, 32, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap();
        let sel = Selection::full(32, 32);
        let tile = TileCoord::new(0, 0);

        for_each_selected_pixel(&mut image, &sel, tile, |_x, _y, _p| {
            Pixel::rgb(255, 255, 255)
        });

        assert_eq!(image.get_pixel(0, 0), Pixel::rgb(255, 255, 255));
        assert_eq!(image.get_pixel(15, 15), Pixel::rgb(255, 255, 255));
    }
}
