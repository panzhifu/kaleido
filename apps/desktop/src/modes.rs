//! Editing modes for Kaleido.
//!
//! Each mode has its own tool set and behavior.

use gpui::SharedString;

/// The five editing modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[allow(dead_code)]
pub enum Mode {
    /// Vector editing (SVG-like).
    #[default]
    Vector,
    /// Pixel/raster editing.
    Pixel,
    /// Digital painting.
    Painting,
    /// Page layout and typesetting.
    Layout,
    /// Animation and GIF creation.
    Animation,
}

impl Mode {
    /// Short label for the mode tab.
    pub fn label(self) -> &'static str {
        match self {
            Self::Vector => "矢量",
            Self::Pixel => "像素",
            Self::Painting => "绘画",
            Self::Layout => "排版",
            Self::Animation => "动画",
        }
    }

    /// Icon identifier for the mode tab.
    #[allow(dead_code)]
    pub fn icon(self) -> &'static str {
        match self {
            Self::Vector => "vector",
            Self::Pixel => "pixel",
            Self::Painting => "paint",
            Self::Layout => "layout",
            Self::Animation => "animation",
        }
    }
}

/// Available tools per mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tool {
    // Common
    Select,
    // Vector
    Node,
    Pen,
    Rectangle,
    Ellipse,
    Polygon,
    Text,
    VectorBrush,
    Gradient,
    // Pixel
    RectSelect,
    EllipseSelect,
    MagicWand,
    Crop,
    Heal,
    CloneStamp,
    PaintBrush,
    Eraser,
    Fill,
    Zoom,
    // Painting
    Blur,
    Sharpen,
    Eyedropper,
    Symmetry,
    // Layout
    TextBox,
    ImageFrame,
    Table,
    Align,
    Spacing,
    Style,
    // Animation
    AddFrame,
    DeleteFrame,
    DuplicateFrame,
    Play,
}

impl Tool {
    /// Primary mode for this tool (first mode that uses it).
    #[allow(dead_code)]
    pub fn mode(&self) -> Mode {
        match self {
            Self::Select | Self::Node | Self::Pen | Self::Text | Self::VectorBrush
            | Self::Gradient => Mode::Vector,
            Self::RectSelect | Self::EllipseSelect | Self::MagicWand | Self::Crop
            | Self::Heal | Self::CloneStamp | Self::Fill | Self::Zoom => Mode::Pixel,
            Self::Blur | Self::Sharpen | Self::Eyedropper | Self::Symmetry => Mode::Painting,
            Self::TextBox | Self::ImageFrame | Self::Table | Self::Align
            | Self::Spacing | Self::Style => Mode::Layout,
            Self::AddFrame | Self::DeleteFrame | Self::DuplicateFrame | Self::Play => {
                Mode::Animation
            }
            // Tools shared across modes - assign to their primary mode
            Self::Rectangle | Self::Ellipse | Self::Polygon => Mode::Vector,
            Self::PaintBrush | Self::Eraser => Mode::Painting,
        }
    }

    /// Icon identifier.
    #[allow(dead_code)]
    pub fn icon(&self) -> &'static str {
        match self {
            Self::Select => "select",
            Self::Node => "node",
            Self::Pen => "pen",
            Self::Rectangle => "rect",
            Self::Ellipse => "ellipse",
            Self::Polygon => "polygon",
            Self::Text => "text",
            Self::VectorBrush => "brush",
            Self::Gradient => "gradient",
            Self::RectSelect => "rect-select",
            Self::EllipseSelect => "ellipse-select",
            Self::MagicWand => "wand",
            Self::Crop => "crop",
            Self::Heal => "heal",
            Self::CloneStamp => "clone",
            Self::PaintBrush => "paint",
            Self::Eraser => "eraser",
            Self::Fill => "fill",
            Self::Zoom => "zoom",
            Self::Blur => "blur",
            Self::Sharpen => "sharpen",
            Self::Eyedropper => "eyedropper",
            Self::Symmetry => "symmetry",
            Self::TextBox => "text-box",
            Self::ImageFrame => "image-frame",
            Self::Table => "table",
            Self::Align => "align",
            Self::Spacing => "spacing",
            Self::Style => "style",
            Self::AddFrame => "add-frame",
            Self::DeleteFrame => "delete-frame",
            Self::DuplicateFrame => "duplicate-frame",
            Self::Play => "play",
        }
    }

    /// Tooltip text.
    #[allow(dead_code)]
    pub fn tooltip(&self) -> SharedString {
        match self {
            Self::Select => "选择工具 (V)".into(),
            Self::Node => "节点工具 (A)".into(),
            Self::Pen => "钢笔工具 (P)".into(),
            Self::Rectangle => "矩形工具 (M)".into(),
            Self::Ellipse => "椭圆工具 (E)".into(),
            Self::Polygon => "多边形工具".into(),
            Self::Text => "文字工具 (T)".into(),
            Self::VectorBrush => "矢量画笔".into(),
            Self::Gradient => "渐变工具 (G)".into(),
            Self::RectSelect => "矩形选区 (M)".into(),
            Self::EllipseSelect => "椭圆选区".into(),
            Self::MagicWand => "魔棒工具 (W)".into(),
            Self::Crop => "裁剪工具 (C)".into(),
            Self::Heal => "修复画笔 (J)".into(),
            Self::CloneStamp => "克隆图章 (S)".into(),
            Self::PaintBrush => "画笔工具 (B)".into(),
            Self::Eraser => "橡皮擦 (E)".into(),
            Self::Fill => "油漆桶 (G)".into(),
            Self::Zoom => "缩放工具 (Z)".into(),
            Self::Blur => "模糊工具".into(),
            Self::Sharpen => "锐化工具".into(),
            Self::Eyedropper => "吸管工具 (I)".into(),
            Self::Symmetry => "对称绘画".into(),
            Self::TextBox => "文本框 (T)".into(),
            Self::ImageFrame => "图片框".into(),
            Self::Table => "表格".into(),
            Self::Align => "对齐".into(),
            Self::Spacing => "间距".into(),
            Self::Style => "样式".into(),
            Self::AddFrame => "添加帧".into(),
            Self::DeleteFrame => "删除帧".into(),
            Self::DuplicateFrame => "复制帧".into(),
            Self::Play => "播放".into(),
        }
    }
}

/// Tools available in each mode.
impl Mode {
    /// Returns the tools for this mode.
    pub fn tools(self) -> Vec<Tool> {
        match self {
            Self::Vector => vec![
                Tool::Select,
                Tool::Node,
                Tool::Pen,
                Tool::Rectangle,
                Tool::Ellipse,
                Tool::Polygon,
                Tool::Text,
                Tool::VectorBrush,
                Tool::Gradient,
            ],
            Self::Pixel => vec![
                Tool::RectSelect,
                Tool::EllipseSelect,
                Tool::MagicWand,
                Tool::Crop,
                Tool::Heal,
                Tool::CloneStamp,
                Tool::PaintBrush,
                Tool::Eraser,
                Tool::Fill,
                Tool::Zoom,
            ],
            Self::Painting => vec![
                Tool::PaintBrush,
                Tool::Eraser,
                Tool::Blur,
                Tool::Sharpen,
                Tool::Eyedropper,
                Tool::Symmetry,
            ],
            Self::Layout => vec![
                Tool::TextBox,
                Tool::ImageFrame,
                Tool::Rectangle,
                Tool::Table,
                Tool::Align,
                Tool::Spacing,
                Tool::Style,
            ],
            Self::Animation => vec![
                Tool::RectSelect,
                Tool::PaintBrush,
                Tool::Eraser,
                Tool::Fill,
                Tool::AddFrame,
                Tool::DeleteFrame,
                Tool::DuplicateFrame,
                Tool::Play,
            ],
        }
    }
}
