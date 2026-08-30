//! Analysis tool contract — read-only tools that inspect pixels.
//!
//! Analysis tools never modify the image. They read pixel data and
//! return results for display in a panel (histogram, colour info,
//! measurement, etc.).

use kaleido_core::TiledImage;

use crate::Tool;

/// The result of an analysis operation.
///
/// Most analysis tools will display their result through a [`crate::Panel`],
/// but the result type is available for programmatic access and testing.
pub type AnalysisResult = serde_json::Value;

/// A tool that reads pixel data without modifying the image.
///
/// The host calls `analyze` with read-only access to the current image.
/// The tool returns a JSON value describing its findings. The host may
/// also call [`crate::Panel::render`] if the tool also implements
/// [`crate::Panel`] to display rich results.
///
/// # When to implement `AnalysisTool`
///
/// - Histogram / tone distribution.
/// - Colour picker / eyedropper returning the colour at a point.
/// - Ruler / measurement between two points.
/// - Pixel info (coordinates, colour values).
pub trait AnalysisTool: Tool {
    /// Analyses the image and returns the result.
    ///
    /// `image` is borrowed immutably — the tool must not mutate any pixels.
    fn analyze(&self, image: &TiledImage) -> AnalysisResult;

    /// Returns the point of interest for this analysis, if any.
    ///
    /// For example, the colour picker returns the pixel coordinate the
    /// user clicked so the host can draw a marker there.
    fn point_of_interest(&self) -> Option<(u32, u32)> {
        None
    }
}
