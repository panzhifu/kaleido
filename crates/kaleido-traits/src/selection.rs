//! The **selection manager** — document-wide selection operations.
//!
//! The selection is a grayscale [`SelectionMask`] stored on the document
//! (`None` = select all). All operations flow through the injected
//! [`crate::data::DataService`] single write path.

use super::ServiceResult;
use kaleido_core::SelectionMask;

/// The selection management service.
pub trait SelectionService: Send + Sync + 'static {
    // ── Queries ────────────────────────────────────────────────────────

    /// The current selection mask (`None` = select all; `Err` when no
    /// document is open).
    fn selection(&self) -> ServiceResult<Option<SelectionMask>>;

    /// Bounding box `(x, y, width, height)` of the selected region, or
    /// `None` when nothing is selected.
    fn bounds(&self) -> ServiceResult<Option<(u32, u32, u32, u32)>>;

    // ── Operations ─────────────────────────────────────────────────────

    /// Replaces the current selection (`None` = select all).
    fn set(&self, selection: Option<SelectionMask>) -> ServiceResult<()>;

    /// Clears the selection to "nothing selected" (full-black mask).
    fn clear(&self) -> ServiceResult<()>;

    /// Inverts the selection (white ↔ black) across the document canvas.
    fn invert(&self) -> ServiceResult<()>;

    /// Union (OR) with another mask.
    fn union(&self, other: &SelectionMask) -> ServiceResult<SelectionMask>;

    /// Intersection (AND) with another mask.
    fn intersect(&self, other: &SelectionMask) -> ServiceResult<SelectionMask>;

    /// Subtraction (AND NOT) of another mask.
    fn subtract(&self, other: &SelectionMask) -> ServiceResult<SelectionMask>;
}
