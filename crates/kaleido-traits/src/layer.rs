//! The **layer manager** — scene-graph layer operations.
//!
//! Layers are [`kaleido_core::scene::Node`]s in the document's scene graph (pixel
//! layers, groups, vector objects, text objects). All mutations go through
//! the injected [`super::data::data::DataService`]'s single write path, so
//! every layer operation is automatically undoable.

use serde::{Deserialize, Serialize};

use super::ServiceResult;
use kaleido_core::{BlendMode, NodeId, Transform2D};

/// Information about a layer node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerInfo {
    /// Node id.
    pub id: NodeId,
    /// Layer name.
    pub name: String,
    /// Whether the layer is visible.
    pub visible: bool,
    /// Layer opacity (0.0 – 1.0).
    pub opacity: f32,
    /// Blend mode.
    pub blend_mode: BlendMode,
    /// Whether the layer is locked.
    pub locked: bool,
    /// Whether this node is a group.
    pub is_group: bool,
}

/// The layer management service.
pub trait LayerService: Send + Sync + 'static {
    // ── Creation / Removal ─────────────────────────────────────────────

    /// Adds a blank pixel layer under the scene root and returns its id.
    fn add_pixel_layer(
        &self,
        name: &str,
        width: u32,
        height: u32,
        format: kaleido_core::PixelFormat,
    ) -> ServiceResult<NodeId>;

    /// Adds an empty group node under the scene root.
    fn add_group(&self, name: &str) -> ServiceResult<NodeId>;

    /// Removes a node and its whole subtree.
    fn remove(&self, id: NodeId) -> ServiceResult<()>;

    // ── Structure ──────────────────────────────────────────────────────

    /// Renames a node.
    fn rename(&self, id: NodeId, name: &str) -> ServiceResult<()>;

    /// Moves a child to `to_index` within its parent's paint order (0 = bottom).
    fn reorder(&self, child: NodeId, to_index: usize) -> ServiceResult<()>;

    /// Reparents a node under a new parent.
    fn reparent(&self, id: NodeId, new_parent: NodeId) -> ServiceResult<()>;

    // ── Styling ────────────────────────────────────────────────────────

    /// Toggles a node's visibility.
    fn set_visible(&self, id: NodeId, visible: bool) -> ServiceResult<()>;

    /// Sets a node's opacity (0.0 – 1.0); out-of-range values are clamped.
    fn set_opacity(&self, id: NodeId, opacity: f32) -> ServiceResult<()>;

    /// Sets a node's blend mode.
    fn set_blend(&self, id: NodeId, blend: BlendMode) -> ServiceResult<()>;

    /// Sets a node's transform (translate / rotate / scale).
    fn set_transform(&self, id: NodeId, transform: Transform2D) -> ServiceResult<()>;

    // ── Queries ────────────────────────────────────────────────────────

    /// Direct children of a node, in paint order (bottom first).
    fn children(&self, id: NodeId) -> ServiceResult<Vec<NodeId>>;

    /// A snapshot of a layer's info (or `None` if missing).
    fn layer(&self, id: NodeId) -> ServiceResult<Option<LayerInfo>>;

    /// Number of nodes in the scene (including the root).
    fn layer_count(&self) -> ServiceResult<usize>;

    /// The currently active layer id, if any.
    fn active_layer(&self) -> Option<NodeId>;

    /// Sets the active layer.
    fn set_active(&self, id: NodeId) -> ServiceResult<()>;

    /// All layer node IDs that are direct children of the scene root,
    /// in paint order (bottom first).
    fn layer_ids(&self) -> ServiceResult<Vec<NodeId>>;
}
