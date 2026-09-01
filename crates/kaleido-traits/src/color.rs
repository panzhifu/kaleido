//! The **color manager** — document color configuration and swatches.
//!
//! The working [`ColorProfile`] lives on the document; the swatch palette is
//! application-level state owned by this service.

use super::ServiceResult;
use kaleido_core::{Color, ColorProfile};

/// The color management service.
pub trait ColorService: Send + Sync + 'static {
    // ── Document color profile ─────────────────────────────────────────

    /// The document's working color profile.
    fn profile(&self) -> ServiceResult<ColorProfile>;

    /// Sets the document's working color profile.
    fn set_profile(&self, profile: ColorProfile) -> ServiceResult<()>;

    // ── Swatch palette ─────────────────────────────────────────────────

    /// The current swatch palette, first to last.
    fn swatches(&self) -> Vec<Color>;

    /// Appends a swatch to the palette.
    fn add_swatch(&self, color: Color) -> ServiceResult<()>;

    /// Removes the swatch at `index`.
    fn remove_swatch(&self, index: usize) -> ServiceResult<()>;

    /// Replaces the swatch at `index` with `color`.
    fn set_swatch_color(&self, index: usize, color: Color) -> ServiceResult<()>;

    /// Removes every swatch from the palette.
    fn clear_swatches(&self) -> ServiceResult<()>;

    /// Swaps the swatches at positions `a` and `b`.
    fn swap_swatches(&self, a: usize, b: usize) -> ServiceResult<()>;
}
