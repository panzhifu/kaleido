//! Selection tool contract — the foundation for region-selection tools.
//!
//! A [`SelectionTool`] is interactive (it responds to pointer events)
//! but does **not** paint pixels — it produces a [`Selection`] that
//! other tools and operations consume. This is a distinct contract from
//! [`InteractiveTool`] because the output is a selection mask, not a
//! pixel modification.
//!
//! ```text
//! on_begin  ──►  on_update ──►  … ──►  on_end
//!     │              │                  │
//!     └── host clears  └── host updates  └── host commits
//!         previous        preview          final selection
//! ```

use serde::{Deserialize, Serialize};

use crate::Tool;

// ---------------------------------------------------------------------------
// Selection
// ---------------------------------------------------------------------------

/// A rectangular selection region with an optional mask.
///
/// The selection is stored as a flat `Vec<bool>` (one per pixel) so
/// tools can test `is_selected(x, y)` in O(1). For simple rectangular
/// selections the mask is still allocated but mostly `false`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Selection {
    /// Selection width (= image width the selection was made against).
    pub width: u32,
    /// Selection height (= image height the selection was made against).
    pub height: u32,
    /// Flat row-major mask: `mask[y * width + x]` is `true` when pixel
    /// (x, y) is selected. Empty when there is no selection.
    pub mask: Vec<bool>,
}

impl Selection {
    /// Creates an empty (no-selection) result.
    pub fn empty(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            mask: Vec::new(),
        }
    }

    /// Creates a full-image selection.
    pub fn full(width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            mask: vec![true; (width * height) as usize],
        }
    }

    /// Creates a rectangular selection.
    pub fn rect(x: u32, y: u32, w: u32, h: u32, img_w: u32, img_h: u32) -> Self {
        let mut mask = vec![false; (img_w * img_h) as usize];
        for row in y..(y + h).min(img_h) {
            for col in x..(x + w).min(img_w) {
                mask[(row * img_w + col) as usize] = true;
            }
        }
        Self {
            width: img_w,
            height: img_h,
            mask,
        }
    }

    /// Returns `true` when the selection is empty (nothing selected).
    pub fn is_empty(&self) -> bool {
        self.mask.is_empty() || self.mask.iter().all(|b| !b)
    }

    /// Returns `true` when pixel (x, y) is selected.
    pub fn contains(&self, x: u32, y: u32) -> bool {
        if x >= self.width || y >= self.height {
            return false;
        }
        self.mask
            .get((y * self.width + x) as usize)
            .copied()
            .unwrap_or(false)
    }

    /// Returns the bounding rectangle of the selected region, or `None`
    /// when the selection is empty.
    pub fn bounds(&self) -> Option<(u32, u32, u32, u32)> {
        let mut min_x = u32::MAX;
        let mut min_y = u32::MAX;
        let mut max_x = 0;
        let mut max_y = 0;
        let mut any = false;

        for (i, selected) in self.mask.iter().enumerate() {
            if *selected {
                let x = (i as u32) % self.width;
                let y = (i as u32) / self.width;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
                any = true;
            }
        }

        if any {
            Some((min_x, min_y, max_x - min_x + 1, max_y - min_y + 1))
        } else {
            None
        }
    }

    /// Returns the number of selected pixels.
    pub fn count(&self) -> usize {
        self.mask.iter().filter(|b| **b).count()
    }

    /// Inverts the selection (selected ↔ unselected).
    pub fn invert(&mut self) {
        for b in &mut self.mask {
            *b = !*b;
        }
    }
}

// ---------------------------------------------------------------------------
// SelectionTool
// ---------------------------------------------------------------------------

/// How a new selection should combine with the existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SelectionMode {
    /// Replace the existing selection.
    #[default]
    Replace,
    /// Add to the existing selection (union).
    Add,
    /// Subtract from the existing selection.
    Subtract,
    /// Intersect with the existing selection.
    Intersect,
}

/// A tool that produces a selection rather than modifying pixels.
///
/// The host feeds pointer events (already converted to image space) and
/// reads back the current selection after each event so it can draw the
/// marching-ants overlay.
pub trait SelectionTool: Tool {
    /// Called when the pointer is pressed to start a new selection.
    ///
    /// `existing` is the current selection before this gesture; the tool
    /// should return the selection that results from this down event.
    fn on_begin(
        &self,
        x: f32,
        y: f32,
        mode: SelectionMode,
        existing: &Selection,
    ) -> Selection;

    /// Called while the pointer moves with a button held down.
    fn on_update(
        &self,
        x: f32,
        y: f32,
        mode: SelectionMode,
        existing: &Selection,
    ) -> Selection;

    /// Called when the pointer is released, finalising the selection.
    fn on_end(
        &self,
        x: f32,
        y: f32,
        mode: SelectionMode,
        existing: &Selection,
    ) -> Selection;

    /// Returns the current selection this tool is holding.
    ///
    /// The host reads this after every event to draw the selection overlay.
    fn selection(&self) -> &Selection;

    /// Clears the current selection (e.g. when the user clicks outside).
    fn clear_selection(&self);
}
