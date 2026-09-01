//! Selection state management.
//!
//! Holds the current image selection and provides operations that
//! tools and filters use to query and modify it.
//!
//! > **Legacy note:** this state works on the old-model
//! > [`kaleido_traits::Selection`] (a flat `Vec<bool>` mask), not the
//! > document-wide [`kaleido_core::SelectionMask`] that the
//! > [`super::SelectionService`] manages. It is kept for the desktop / CLI
//! > hosts that still use the old model; new code should go through the
//! > service (whose state lives on the document).

use std::sync::Mutex;

use kaleido_traits::Selection;

// ---------------------------------------------------------------------------
// SelectionState
// ---------------------------------------------------------------------------

/// Shared selection state for the current document.
///
/// The host updates this whenever a selection tool commits a new
/// selection. Other tools read it to constrain their operation to the
/// selected region.
#[derive(Debug)]
pub struct SelectionState {
    inner: Mutex<SelectionStateInner>,
}

#[derive(Debug, Default)]
struct SelectionStateInner {
    selection: Selection,
    /// Whether the selection is currently visible (marching ants).
    visible: bool,
}

impl SelectionState {
    /// Creates a new selection state with no selection.
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            inner: Mutex::new(SelectionStateInner {
                selection: Selection::empty(width, height),
                visible: false,
            }),
        }
    }

    /// Returns a copy of the current selection.
    pub fn selection(&self) -> Selection {
        self.lock().selection.clone()
    }

    /// Replaces the entire selection.
    pub fn set_selection(&self, selection: Selection) {
        let mut inner = self.lock();
        inner.selection = selection;
        inner.visible = true;
    }

    /// Clears the selection (nothing selected).
    pub fn clear(&self) {
        let mut inner = self.lock();
        inner.selection.mask.clear();
        inner.visible = false;
    }

    /// Returns `true` when the selection is non-empty.
    pub fn has_selection(&self) -> bool {
        !self.lock().selection.is_empty()
    }

    /// Returns `true` when the selection should be drawn (marching ants).
    pub fn is_visible(&self) -> bool {
        self.lock().visible
    }

    /// Shows or hides the selection overlay without changing the mask.
    pub fn set_visible(&self, visible: bool) {
        self.lock().visible = visible;
    }

    /// Inverts the current selection.
    pub fn invert(&self) {
        self.lock().selection.invert();
    }

    /// Returns the bounding box of the selection, or `None` if empty.
    pub fn bounds(&self) -> Option<(u32, u32, u32, u32)> {
        self.lock().selection.bounds()
    }

    /// Returns `true` when the given pixel is within the selection.
    pub fn contains(&self, x: u32, y: u32) -> bool {
        self.lock().selection.contains(x, y)
    }

    /// Drops the selection when the image is resized.
    ///
    /// The old mask is invalid at the new dimensions, so the state is reset
    /// to an empty (nothing-selected) selection of the new size.
    pub fn resize(&self, width: u32, height: u32) {
        let mut inner = self.lock();
        inner.selection = Selection::empty(width, height);
        inner.visible = false;
    }

    /// Locks the inner state, panicking with a clear message if poisoned.
    fn lock(&self) -> std::sync::MutexGuard<'_, SelectionStateInner> {
        self.inner.lock().expect("selection state lock poisoned")
    }
}

impl Default for SelectionState {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_state_is_empty() {
        let state = SelectionState::new(100, 100);
        assert!(!state.has_selection());
        assert!(!state.is_visible());
    }

    #[test]
    fn test_set_and_clear() {
        let state = SelectionState::new(100, 100);
        let sel = Selection::rect(10, 10, 50, 50, 100, 100);
        state.set_selection(sel);
        assert!(state.has_selection());
        assert!(state.is_visible());
        assert!(state.contains(20, 20));
        assert!(!state.contains(5, 5));

        state.clear();
        assert!(!state.has_selection());
        assert!(!state.is_visible());
    }

    #[test]
    fn test_bounds() {
        let state = SelectionState::new(100, 100);
        let sel = Selection::rect(10, 20, 30, 40, 100, 100);
        state.set_selection(sel);
        assert_eq!(state.bounds(), Some((10, 20, 30, 40)));
    }

    #[test]
    fn test_invert() {
        let state = SelectionState::new(10, 10);
        let sel = Selection::rect(0, 0, 5, 5, 10, 10);
        state.set_selection(sel);

        assert!(state.contains(0, 0));
        assert!(!state.contains(8, 8));

        state.invert();

        assert!(!state.contains(0, 0));
        assert!(state.contains(8, 8));
    }

    #[test]
    fn test_resize_clears() {
        let state = SelectionState::new(100, 100);
        state.set_selection(Selection::full(100, 100));
        assert!(state.has_selection());

        state.resize(200, 200);
        assert!(!state.has_selection());
    }
}
