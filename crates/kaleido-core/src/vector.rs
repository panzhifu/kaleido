//! Vector objects — editable Bézier paths (Illustrator / Inkscape style).
//!
//! The path is stored as editable nodes (anchor + control points) rather
//! than raw SVG commands, so the vector tool can manipulate shape topology
//! directly.  Rasterization happens at render time (cached by the renderer).

use super::types::{Color, Point, ResourceId};

/// Style of the area enclosed by a path.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum FillStyle {
    /// Uniform fill.
    Solid(Color),
    // Gradient(Gradient),  // v2: linear / radial gradients
    /// No fill.
    None,
}

/// Stroke (outline) style.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct StrokeStyle {
    pub color: Color,
    pub width: f32,
    pub opacity: f32,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            color: Color::black(),
            width: 1.0,
            opacity: 1.0,
        }
    }
}

/// One editable path with its node list.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VectorPath {
    pub nodes: Vec<PathNode>,
    pub closed: bool,
}

impl VectorPath {
    /// An empty path (no nodes).
    #[inline]
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            closed: false,
        }
    }

    /// Number of path nodes.
    #[inline]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the path has no nodes.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Appends a node to the path.
    #[inline]
    pub fn push(&mut self, node: PathNode) {
        self.nodes.push(node);
    }

    /// Conservative axis-aligned bounds of this path — the min/max over
    /// anchors **and** control points (control points can extend the curve
    /// beyond the anchors).  Returns `None` for an empty path.
    pub fn bounds(&self) -> Option<(Point, Point)> {
        let mut min = Point::new(f32::INFINITY, f32::INFINITY);
        let mut max = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut any = false;
        for node in &self.nodes {
            for p in [Some(node.anchor), node.control_in, node.control_out]
                .into_iter()
                .flatten()
            {
                any = true;
                min.x = min.x.min(p.x);
                min.y = min.y.min(p.y);
                max.x = max.x.max(p.x);
                max.y = max.y.max(p.y);
            }
        }
        any.then_some((min, max))
    }
}

impl Default for VectorPath {
    fn default() -> Self {
        Self::new()
    }
}

/// A single editable path node (anchor + optional control points).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PathNode {
    /// The anchor point.
    pub anchor: Point,
    /// Incoming control point (affects the segment entering this node).
    pub control_in: Option<Point>,
    /// Outgoing control point (affects the segment leaving this node).
    pub control_out: Option<Point>,
    /// Whether the node is smooth (both handles aligned) or a corner.
    pub smooth: bool,
}

impl PathNode {
    #[inline]
    pub const fn new(anchor: Point) -> Self {
        Self {
            anchor,
            control_in: None,
            control_out: None,
            smooth: true,
        }
    }
}

/// A vector object attached to a node: one or more sub-paths plus styling.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VectorObject {
    pub paths: Vec<VectorPath>,
    pub fill: FillStyle,
    pub stroke: Option<StrokeStyle>,
    /// Resource handle of a brush used to stroke the path (optional).
    pub brush: Option<ResourceId>,
}

impl VectorObject {
    /// A new empty vector object.
    #[inline]
    pub fn new() -> Self {
        Self {
            paths: Vec::new(),
            fill: FillStyle::None,
            stroke: None,
            brush: None,
        }
    }

    /// Whether the object has no paths.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Appends a path to the object.
    #[inline]
    pub fn add_path(&mut self, path: VectorPath) {
        self.paths.push(path);
    }

    /// Conservative bounds across all sub-paths (anchors + control points).
    /// Returns `None` when the object has no path nodes.
    pub fn bounds(&self) -> Option<(Point, Point)> {
        let mut min = Point::new(f32::INFINITY, f32::INFINITY);
        let mut max = Point::new(f32::NEG_INFINITY, f32::NEG_INFINITY);
        let mut any = false;
        for path in &self.paths {
            if let Some((pmin, pmax)) = path.bounds() {
                any = true;
                min.x = min.x.min(pmin.x);
                min.y = min.y.min(pmin.y);
                max.x = max.x.max(pmax.x);
                max.y = max.y.max(pmax.y);
            }
        }
        any.then_some((min, max))
    }
}

impl Default for VectorObject {
    fn default() -> Self {
        Self::new()
    }
}
