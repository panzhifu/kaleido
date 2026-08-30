//! Inter-panel event types.
//!
//! These enums define messages passed between UI panels.
//! All variants will be wired up as the UI is implemented.

use kaleido_services::layer::LayerId;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum CanvasEvent {
    ZoomChanged { zoom: f32 },
    OffsetChanged { x: f32, y: f32 },
    Rotated { radians: f32 },
    ImageLoaded { path: std::path::PathBuf, width: u32, height: u32 },
    NeedsRedraw,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum LayersEvent {
    LayerAdded { id: LayerId },
    LayerRemoved { id: LayerId },
    LayerSelected { id: LayerId },
    VisibilityChanged { id: LayerId, visible: bool },
    BlendModeChanged { id: LayerId, mode: String },
    OpacityChanged { id: LayerId, opacity: f32 },
    Reordered { from: usize, to: usize },
    ThumbnailNeedsUpdate { id: LayerId },
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum ToolParamsEvent {
    ParamsChanged { params: serde_json::Value },
    Applied { name: String },
    Cancelled,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub enum HistoryEvent {
    EntryAdded { name: String, description: String },
    Undone { name: String },
    Redone { name: String },
    Cleared,
}
