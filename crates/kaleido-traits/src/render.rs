//! The **render manager** — compositing the document to a bitmap.
//!
//! Renders the scene graph bottom-up: group subtrees are composited first,
//! then blended into the result with each node's blend mode and opacity.

use super::ServiceResult;
use kaleido_core::NodeId;
use kaleido_core::TiledImage;

/// The render management service.
pub trait RenderService: Send + Sync + 'static {
    // ── Rendering ──────────────────────────────────────────────────────

    /// Composites the whole document into a single bitmap sized to the
    /// document canvas. `Err` when no document is open.
    fn render(&self) -> ServiceResult<TiledImage>;

    /// Composites a single node (and its subtree) into a bitmap.
    fn render_node(&self, id: NodeId) -> ServiceResult<TiledImage>;

    /// Composites the whole document at the given animation frame.
    fn render_frame(&self, frame_index: u32) -> ServiceResult<TiledImage>;

    /// Composites the whole document, then returns the region of the result.
    fn render_region(&self, region: (u32, u32, u32, u32)) -> ServiceResult<TiledImage>;

    // ── Export ─────────────────────────────────────────────────────────

    /// Composites the document into a flattened bitmap for export.
    fn export_flattened(&self) -> ServiceResult<TiledImage>;
}
