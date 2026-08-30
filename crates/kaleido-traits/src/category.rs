//! Tool categories for grouping in the UI.
//!
//! Tools declare a category so the host can group them in toolbars
//! and menus by function rather than by plugin origin.

use serde::{Deserialize, Serialize};

/// Functional categories for tools.
///
/// The host uses these to group tools in the toolbar and to show
/// contextual panels. Plugins pick the category that best describes
/// what their tool does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    /// Selection tools — rectangle select, lasso, magic wand, etc.
    Selection,
    /// Geometric transforms — flip, rotate, crop, resize, perspective.
    Transform,
    /// Painting and drawing — brush, pencil, eraser, airbrush.
    Painting,
    /// Tonal / colour adjustments — brightness, contrast, curves, levels.
    ColorAdjustment,
    /// Retouching — healing brush, clone stamp, dodge/burn, patch.
    Retouch,
    /// Fill tools — paint bucket, gradient, pattern fill.
    Fill,
    /// Vector / shape tools — rectangle, ellipse, pen, path.
    Vector,
    /// Text tool.
    Text,
    /// Analysis tools — colour picker, ruler, histogram display.
    Analysis,
    /// Pan / zoom / navigation.
    Navigation,
    /// Custom / uncategorised.
    Other,
}

impl ToolCategory {
    /// Returns a human-readable label for this category.
    pub fn label(self) -> &'static str {
        match self {
            Self::Selection => "选择",
            Self::Transform => "变换",
            Self::Painting => "绘画",
            Self::ColorAdjustment => "调色",
            Self::Retouch => "修饰",
            Self::Fill => "填充",
            Self::Vector => "矢量",
            Self::Text => "文字",
            Self::Analysis => "分析",
            Self::Navigation => "导航",
            Self::Other => "其他",
        }
    }

    /// Returns an icon key for this category (used when a tool has no
    /// icon of its own).
    pub fn default_icon(self) -> &'static str {
        match self {
            Self::Selection => "select",
            Self::Transform => "transform",
            Self::Painting => "brush",
            Self::ColorAdjustment => "adjust",
            Self::Retouch => "heal",
            Self::Fill => "fill",
            Self::Vector => "shape",
            Self::Text => "text",
            Self::Analysis => "eyedropper",
            Self::Navigation => "hand",
            Self::Other => "tool",
        }
    }
}

impl Default for ToolCategory {
    fn default() -> Self {
        Self::Other
    }
}
