//! Cursor types for tools.
//!
//! Tools declare which cursor they want the host to display when the
//! pointer is over the canvas. The host is responsible for actually
//! showing the cursor — plugins never touch the cursor directly.

use serde::{Deserialize, Serialize};

/// Cursor appearance requests from a tool to the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CursorType {
    /// The platform default arrow.
    Default,
    /// A crosshair — typical for selection tools, colour pickers, rulers.
    Crosshair,
    /// A precise pixel cursor — typical for the brush when zoomed in.
    Precise,
    /// A hand — for panning the canvas.
    Grab,
    /// A brush-size circle — the host draws a ring matching the tool's
    /// current brush radius. The radius is reported separately.
    BrushCircle,
    /// A move cursor — for dragging layers or selection bounds.
    Move,
    /// Resize cursors for selection handles.
    ResizeN,
    ResizeS,
    ResizeE,
    ResizeW,
    ResizeNe,
    ResizeNw,
    ResizeSe,
    ResizeSw,
    /// A "not allowed" cursor — the tool cannot act here.
    NotAllowed,
    /// A text insertion cursor (I-beam) — for the text tool.
    Text,
    /// A wait / busy cursor.
    Wait,
    /// An eyedropper — picks a colour from the canvas.
    Eyedropper,
}

impl Default for CursorType {
    fn default() -> Self {
        Self::Default
    }
}
