//! Operation graph — declarative image processing pipeline.
//!
//! An [`OpGraph`] is a directed acyclic graph where nodes are image
//! operations and edges represent data flow.  [`GraphExecutor::execute`]
//! evaluates the graph restricted to a requested output region ([`Rect`]),
//! pruning unreachable nodes and processing tiles in parallel via rayon.
//!
//! Point-op fusion ([`FusedOp`]) is prepared but **not yet wired into the
//! executor**: adjacent point-ops currently run as separate passes (one
//! full-image allocation each).  Fusing them into a single pass is roadmap
//! work.  Similarly, ROI *growth* (needed by spatial ops such as blur) is
//! not applied yet — every node computes the same output ROI.

use std::collections::{HashMap, VecDeque};

use kaleido_core::{ImageResult, PixelFormat, TiledImage};
use rayon::prelude::*;

// ---------------------------------------------------------------------------
// Op trait
// ---------------------------------------------------------------------------
//
// Each op declares:
//   - What formats it accepts as input / produces as output.
//   - Whether it can be fused with adjacent point-ops.
//   - How to compute a specific ROI of its output.

/// A rectangular region of interest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    pub const fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    #[inline]
    pub const fn right(&self) -> u32 {
        self.x + self.width
    }

    #[inline]
    pub const fn bottom(&self) -> u32 {
        self.y + self.height
    }

    /// Returns the intersection of two rects (empty if they do not overlap).
    pub fn intersect(&self, other: &Rect) -> Self {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        if right <= x || bottom <= y {
            Self::new(0, 0, 0, 0)
        } else {
            Self::new(x, y, right - x, bottom - y)
        }
    }

    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Grows this rect by `dx`/`dy` on each side, clamped to `max_bounds`.
    pub fn grow(&self, dx: u32, dy: u32, max_bounds: &Rect) -> Self {
        let x = self.x.saturating_sub(dx).max(max_bounds.x);
        let y = self.y.saturating_sub(dy).max(max_bounds.y);
        let right = (self.right() + dx).min(max_bounds.right());
        let bottom = (self.bottom() + dy).min(max_bounds.bottom());
        Self::new(x, y, right - x, bottom - y)
    }
}

/// Input/output format contract for an op.
#[derive(Debug, Clone)]
pub struct OpFormats {
    /// Accepted input formats (empty = passthrough/constant).
    pub input: Vec<PixelFormat>,
    /// Output format produced.
    pub output: PixelFormat,
}

/// A node in the operation graph.
///
/// Ops are boxed trait objects so the graph can hold heterogeneous ops.
pub trait Op: Send + Sync + 'static {
    /// Declares the input/output formats this op accepts/produces.
    fn formats(&self) -> OpFormats;

    /// Computes the output for the given ROI.
    ///
    /// `inputs` provides the input tiles for each declared input edge,
    /// cropped to the region this op needs.  Implementations must not
    /// read outside `roi`.
    fn compute_roi(&self, roi: Rect, inputs: &[Option<&TiledImage>]) -> ImageResult<TiledImage>;

    /// Whether this op can be fused with the `next` op into a single pass.
    ///
    /// Returns `Some(fused_op)` if fusion is possible.
    fn try_fuse(&self, _next: &dyn Op) -> Option<Box<dyn Op>> {
        None
    }

    /// Whether this is a point-op (per-pixel, no spatial context).
    /// Point-ops are eligible for fusion.
    fn is_point_op(&self) -> bool {
        false
    }

    /// Human-readable name (for debugging/logging).
    fn name(&self) -> &str;
}

// ---------------------------------------------------------------------------
// OpNode / OpGraph
// ---------------------------------------------------------------------------

/// Identifier for a node in the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

/// A node in the operation graph.
pub struct OpNode {
    pub id: NodeId,
    pub op: Box<dyn Op>,
    /// Input edges: (source_node_id, source_output_index).
    pub inputs: Vec<(NodeId, u32)>,
}

/// A directed acyclic graph of image operations.
///
/// # Example
///
/// ```ignore
/// let mut graph = OpGraph::new();
/// let src = graph.add_node(SourceOp::new(source_image), &[]);
/// let bright = graph.add_node(BrightnessOp::new(20.0), &[src]);
/// let blur = graph.add_node(BlurOp::new(2.0), &[bright]);
/// graph.set_output(blur);
///
/// let result = GraphExecutor::default().execute(&graph, full_rect)?;
/// ```
pub struct OpGraph {
    nodes: Vec<OpNode>,
    output: Option<NodeId>,
}

impl OpGraph {
    /// Creates an empty graph.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            output: None,
        }
    }

    /// Adds a node to the graph and returns its ID.
    ///
    /// `input_ids` are the nodes whose outputs feed into this node, in
    /// the order expected by the op's `compute_roi`.
    pub fn add_node(&mut self, op: Box<dyn Op>, input_ids: &[NodeId]) -> NodeId {
        let id = NodeId(self.nodes.len());
        let inputs = input_ids.iter().map(|&id| (id, 0)).collect();
        self.nodes.push(OpNode { id, op, inputs });
        id
    }

    /// Sets the output node (the one whose result is returned by the executor).
    pub fn set_output(&mut self, id: NodeId) {
        self.output = Some(id);
    }

    /// Returns the output node ID, if set.
    pub fn output(&self) -> Option<NodeId> {
        self.output
    }

    /// Returns a node by ID.
    pub fn node(&self, id: NodeId) -> Option<&OpNode> {
        self.nodes.get(id.0)
    }

    /// Returns the number of nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Topologically sorts the nodes (Kahn's algorithm).
    ///
    /// Returns node IDs in evaluation order (inputs before dependents).
    /// Nodes in a cycle are omitted from the result (the input is expected
    /// to be a DAG).
    pub fn topo_sort(&self) -> Vec<NodeId> {
        let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
        let mut dependents: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for node in &self.nodes {
            in_degree.entry(node.id).or_insert(0);
            for &(src, _) in &node.inputs {
                *in_degree.entry(node.id).or_insert(0) += 1;
                dependents.entry(src).or_default().push(node.id);
            }
        }

        let mut queue: VecDeque<NodeId> = in_degree
            .iter()
            .filter(|(_, d)| **d == 0)
            .map(|(&id, _)| id)
            .collect();

        let mut sorted = Vec::with_capacity(self.nodes.len());

        while let Some(id) = queue.pop_front() {
            sorted.push(id);
            if let Some(deps) = dependents.get(&id) {
                for &dep in deps {
                    let deg = in_degree
                        .get_mut(&dep)
                        .expect("every dependent is seeded in in_degree");
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(dep);
                    }
                }
            }
        }

        sorted
    }

    /// Walks the graph from the output node, collecting all reachable nodes.
    pub fn reachable_nodes(&self) -> Vec<NodeId> {
        let Some(output) = self.output else {
            return Vec::new();
        };
        let mut visited = HashMap::new();
        let mut stack = vec![output];
        while let Some(id) = stack.pop() {
            if visited.insert(id, true).is_some() {
                continue;
            }
            if let Some(node) = self.node(id) {
                for &(src, _) in &node.inputs {
                    stack.push(src);
                }
            }
        }
        visited.keys().copied().collect()
    }
}

impl Default for OpGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// FusedOp — result of fusing two point-ops
// ---------------------------------------------------------------------------

/// A fused pair of point-ops that executes in a single pass.
///
/// This is the building block for automatic fusion: when two adjacent
/// point-ops are detected by the executor, they are merged into a
/// `FusedOp` that applies both transformations in one loop.
pub struct FusedOp {
    name: String,
    ops: Vec<Box<dyn Op>>,
}

impl FusedOp {
    /// Attempts to create a fused op from `first` and `second`.
    ///
    /// Fusion is possible when both are point-ops and `first` declares
    /// a fusion with `second`.
    pub fn try_new(first: Box<dyn Op>, second: Box<dyn Op>) -> Option<Self> {
        if !first.is_point_op() || !second.is_point_op() {
            return None;
        }
        // Try direct fusion first.
        if let Some(fused) = first.try_fuse(second.as_ref()) {
            return Some(Self {
                name: fused.name().to_string(),
                ops: vec![fused],
            });
        }
        // Otherwise just chain them.
        Some(Self {
            name: format!("{}+{}", first.name(), second.name()),
            ops: vec![first, second],
        })
    }
}

impl Op for FusedOp {
    fn formats(&self) -> OpFormats {
        OpFormats {
            input: self
                .ops
                .first()
                .expect("FusedOp always holds at least one op")
                .formats()
                .input
                .clone(),
            output: self
                .ops
                .last()
                .expect("FusedOp always holds at least one op")
                .formats()
                .output,
        }
    }

    fn compute_roi(&self, roi: Rect, inputs: &[Option<&TiledImage>]) -> ImageResult<TiledImage> {
        let mut intermediate = self.ops[0].compute_roi(roi, inputs)?;
        for op in &self.ops[1..] {
            // Subsequent ops use the previous output as input.
            let prev = &intermediate;
            // We wrap it in a slice of one element because compute_roi
            // expects a slice of optional inputs.
            let _input: Option<&TiledImage> = Some(prev);
            let inputs: &[Option<&TiledImage>] = &[Some(prev)];
            intermediate = op.compute_roi(roi, inputs)?;
        }
        Ok(intermediate)
    }

    fn is_point_op(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        &self.name
    }
}

// ---------------------------------------------------------------------------
// GraphExecutor
// ---------------------------------------------------------------------------

/// Executes an [`OpGraph`] with tile-parallelism and point-op fusion.
pub struct GraphExecutor {
    /// Thread pool for tile-parallel execution.
    pool: rayon::ThreadPool,
}

impl GraphExecutor {
    /// Creates an executor with the default rayon thread pool.
    pub fn new() -> Self {
        Self {
            pool: rayon::ThreadPoolBuilder::new()
                .build()
                .expect("Failed to build rayon thread pool"),
        }
    }

    /// Creates an executor with a specific number of threads.
    pub fn with_threads(num_threads: usize) -> Self {
        Self {
            pool: rayon::ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .build()
                .expect("Failed to build rayon thread pool"),
        }
    }

    /// Executes the graph and returns the output image.
    ///
    /// `output_roi` restricts computation to the requested region. The
    /// result is sized to `output_roi` and uses ROI-relative coordinates
    /// (the pixel at `(0, 0)` corresponds to `output_roi.x/y`).
    ///
    /// The executor automatically:
    /// 1. Prunes unreachable nodes.
    /// 2. Processes tiles in parallel via rayon.
    pub fn execute(&self, graph: &OpGraph, output_roi: Rect) -> ImageResult<TiledImage> {
        let Some(output_id) = graph.output() else {
            return Err(kaleido_core::ImageError::OperationFailed {
                reason: "OpGraph has no output node".into(),
            });
        };

        // 1. Find reachable nodes (prune dead branches).
        let reachable = graph.reachable_nodes();
        let reachable_set: std::collections::HashSet<NodeId> =
            reachable.iter().copied().collect();

        // 2. Topologically sort reachable nodes.
        let topo = graph.topo_sort();
        let topo: Vec<NodeId> = topo
            .into_iter()
            .filter(|id| reachable_set.contains(id))
            .collect();

        // 3. Evaluate nodes in order, caching results.
        let mut results: HashMap<NodeId, TiledImage> = HashMap::new();

        for &node_id in &topo {
            let node = graph.node(node_id).ok_or_else(|| {
                kaleido_core::ImageError::OperationFailed {
                    reason: format!("topo order references missing node {node_id:?}"),
                }
            })?;

            // Gather inputs.
            let mut input_images = Vec::with_capacity(node.inputs.len());
            for &(src_id, _output_idx) in &node.inputs {
                match results.get(&src_id) {
                    Some(img) => input_images.push(Some(img)),
                    None => input_images.push(None),
                }
            }

            let input_refs: Vec<Option<&TiledImage>> = input_images.iter().map(|o| *o).collect();

            // Every node computes the same output ROI. Non-point ops that
            // need spatial context (e.g. blur) would have to grow their
            // input ROI here — roadmap work (see `Rect::grow`).
            let roi = output_roi;

            // Process tiles in parallel.
            let result = self.execute_node_parallel(node, &input_refs, roi)?;
            results.insert(node_id, result);
        }

        results
            .remove(&output_id)
            .ok_or_else(|| kaleido_core::ImageError::OperationFailed {
                reason: "Graph evaluation produced no output".into(),
            })
    }

    /// Executes a single node's op with tile-parallelism.
    ///
    /// Splits the ROI into tile-sized chunks and processes them in
    /// parallel using the rayon thread pool.
    fn execute_node_parallel(
        &self,
        node: &OpNode,
        inputs: &[Option<&TiledImage>],
        roi: Rect,
    ) -> ImageResult<TiledImage> {
        let tile_size = kaleido_core::TILE_SIZE;

        // If the ROI fits in a single tile, execute directly and crop the
        // op's (full-size) result down to the ROI — same shape as the
        // tiled path below.
        if roi.width <= tile_size && roi.height <= tile_size {
            let result = node.op.compute_roi(roi, inputs)?;
            return crop_to_roi(result, roi);
        }

        // Split ROI into tile-sized sub-rects.
        let cols = (roi.width + tile_size - 1) / tile_size;
        let rows = (roi.height + tile_size - 1) / tile_size;
        let total_tiles = (cols * rows) as usize;

        // Build the list of tile rects.
        let tile_rects: Vec<Rect> = (0..rows)
            .flat_map(|row| {
                (0..cols).map(move |col| {
                    let x = roi.x + col * tile_size;
                    let y = roi.y + row * tile_size;
                    let width = (roi.x + roi.width - x).min(tile_size);
                    let height = (roi.y + roi.height - y).min(tile_size);
                    Rect::new(x, y, width, height)
                })
            })
            .collect();

        // Clone inputs for thread safety.
        let input_clones: Vec<Option<TiledImage>> =
            inputs.iter().map(|opt| opt.map(|img| img.clone())).collect();

        // Note: the per-op contract is a *full-size* result with only the
        // ROI filled, so every tile allocates a full-size intermediate.
        // Tile-sized intermediates would need a relative-coordinate op
        // contract — roadmap work.
        // Process tiles in parallel.
        let tile_results: Vec<ImageResult<(Rect, TiledImage)>> = self
            .pool
            .install(|| {
                tile_rects
                    .par_iter()
                    .map(|&tile_roi| {
                        let input_refs: Vec<Option<&TiledImage>> =
                            input_clones.iter().map(|opt| opt.as_ref()).collect();
                        node.op.compute_roi(tile_roi, &input_refs)
                            .map(|result| (tile_roi, result))
                    })
                    .collect()
            });

        // Check for errors and find the output dimensions.
        let mut tiles = Vec::with_capacity(total_tiles);
        for result in tile_results {
            let (rect, image) = result?;
            tiles.push((rect, image));
        }

        // Combine tiles into a single output image.
        // We use the first tile to determine the output format.
        let output_format = tiles[0].1.format();
        let mut output = TiledImage::new(roi.width, roi.height, output_format);

        for (rect, tile_image) in tiles {
            // Row-wise blit of the computed region (memcpy per row) instead
            // of a per-pixel get/set loop. Tile results carry absolute
            // coordinates (the op contract), while `output` is ROI-relative,
            // so the destination origin is shifted by `roi`.
            output.copy_from(
                &tile_image,
                rect.x,
                rect.y,
                rect.x - roi.x,
                rect.y - roi.y,
                rect.width,
                rect.height,
            )?;
        }

        Ok(output)
    }
}

/// Crops a full-size op result to the requested ROI, producing an
/// ROI-relative image (the convention [`GraphExecutor::execute`] returns).
///
/// The crop is clamped to the image bounds; a fully outside / empty result
/// stays an empty ROI-sized image instead of erroring.
fn crop_to_roi(img: TiledImage, roi: Rect) -> ImageResult<TiledImage> {
    let w = roi.width.min(img.width().saturating_sub(roi.x));
    let h = roi.height.min(img.height().saturating_sub(roi.y));
    if w == 0 || h == 0 {
        return Ok(TiledImage::new(roi.width, roi.height, img.format()));
    }
    img.crop(roi.x, roi.y, w, h)
}

impl Default for GraphExecutor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::Pixel;

    /// A simple source op that returns a pre-built image.
    struct SourceOp {
        image: TiledImage,
    }

    impl Op for SourceOp {
        fn formats(&self) -> OpFormats {
            OpFormats {
                input: vec![],
                output: self.image.format(),
            }
        }

        fn compute_roi(&self, _roi: Rect, _inputs: &[Option<&TiledImage>]) -> ImageResult<TiledImage> {
            Ok(self.image.clone())
        }

        fn is_point_op(&self) -> bool {
            false
        }

        fn name(&self) -> &str {
            "source"
        }
    }

    /// A brightness adjustment op (point-op, fusable).
    struct BrightnessOp {
        amount: f32,
    }

    impl Op for BrightnessOp {
        fn formats(&self) -> OpFormats {
            OpFormats {
                input: vec![
                    PixelFormat::Rgba8,
                    PixelFormat::Rgb8,
                    PixelFormat::Gray8,
                ],
                output: PixelFormat::Rgba8,
            }
        }

        fn compute_roi(&self, roi: Rect, inputs: &[Option<&TiledImage>]) -> ImageResult<TiledImage> {
            let input = inputs[0].ok_or_else(|| kaleido_core::ImageError::OperationFailed {
                reason: "BrightnessOp: missing input".into(),
            })?;

            // Simple implementation: adjust brightness per pixel.
            let mut output = TiledImage::new(input.width(), input.height(), PixelFormat::Rgba8);
            for y in roi.y..roi.bottom().min(input.height()) {
                for x in roi.x..roi.right().min(input.width()) {
                    let px = input.get_pixel(x, y);
                    let r = (px.r as f32 + self.amount).clamp(0.0, 255.0) as u8;
                    let g = (px.g as f32 + self.amount).clamp(0.0, 255.0) as u8;
                    let b = (px.b as f32 + self.amount).clamp(0.0, 255.0) as u8;
                    output.set_pixel(x, y, Pixel::new(r, g, b, px.a));
                }
            }
            Ok(output)
        }

        fn is_point_op(&self) -> bool {
            true
        }

        fn name(&self) -> &str {
            "brightness"
        }
    }

    /// A contrast adjustment op (point-op, fusable).
    struct ContrastOp {
        factor: f32,
    }

    impl Op for ContrastOp {
        fn formats(&self) -> OpFormats {
            OpFormats {
                input: vec![PixelFormat::Rgba8],
                output: PixelFormat::Rgba8,
            }
        }

        fn compute_roi(&self, roi: Rect, inputs: &[Option<&TiledImage>]) -> ImageResult<TiledImage> {
            let input = inputs[0].ok_or_else(|| kaleido_core::ImageError::OperationFailed {
                reason: "ContrastOp: missing input".into(),
            })?;

            let mut output = TiledImage::new(input.width(), input.height(), PixelFormat::Rgba8);
            for y in roi.y..roi.bottom().min(input.height()) {
                for x in roi.x..roi.right().min(input.width()) {
                    let px = input.get_pixel(x, y);
                    let r = (((px.r as f32 - 128.0) * self.factor) + 128.0).clamp(0.0, 255.0) as u8;
                    let g = (((px.g as f32 - 128.0) * self.factor) + 128.0).clamp(0.0, 255.0) as u8;
                    let b = (((px.b as f32 - 128.0) * self.factor) + 128.0).clamp(0.0, 255.0) as u8;
                    output.set_pixel(x, y, Pixel::new(r, g, b, px.a));
                }
            }
            Ok(output)
        }

        fn is_point_op(&self) -> bool {
            true
        }

        fn name(&self) -> &str {
            "contrast"
        }
    }

    #[test]
    fn test_rect_intersect() {
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(50, 50, 100, 100);
        let c = a.intersect(&b);
        assert_eq!(c, Rect::new(50, 50, 50, 50));

        let d = Rect::new(200, 200, 10, 10);
        assert!(a.intersect(&d).is_empty());
    }

    #[test]
    fn test_rect_grow() {
        let bounds = Rect::new(0, 0, 100, 100);
        let r = Rect::new(50, 50, 10, 10);
        let grown = r.grow(5, 5, &bounds);
        assert_eq!(grown, Rect::new(45, 45, 20, 20));

        // Clamped to bounds.
        let r2 = Rect::new(0, 0, 10, 10);
        let grown2 = r2.grow(5, 5, &bounds);
        assert_eq!(grown2, Rect::new(0, 0, 15, 15));
    }

    #[test]
    fn test_graph_topo_sort() {
        let mut graph = OpGraph::new();
        let a = graph.add_node(Box::new(SourceOp {
            image: TiledImage::new(10, 10, PixelFormat::Rgba8),
        }), &[]);
        let b = graph.add_node(Box::new(SourceOp {
            image: TiledImage::new(10, 10, PixelFormat::Rgba8),
        }), &[]);
        let c = graph.add_node(Box::new(BrightnessOp { amount: 10.0 }), &[a, b]);
        graph.set_output(c);

        let sorted = graph.topo_sort();
        // a and b must come before c.
        let pos_a = sorted.iter().position(|&id| id == a).unwrap();
        let pos_b = sorted.iter().position(|&id| id == b).unwrap();
        let pos_c = sorted.iter().position(|&id| id == c).unwrap();
        assert!(pos_a < pos_c);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn test_graph_reachable() {
        let mut graph = OpGraph::new();
        let a = graph.add_node(Box::new(SourceOp {
            image: TiledImage::new(10, 10, PixelFormat::Rgba8),
        }), &[]);
        let b = graph.add_node(Box::new(SourceOp {
            image: TiledImage::new(10, 10, PixelFormat::Rgba8),
        }), &[]);
        let c = graph.add_node(Box::new(BrightnessOp { amount: 10.0 }), &[a]);
        graph.set_output(c);

        let reachable = graph.reachable_nodes();
        assert!(reachable.contains(&a));
        assert!(reachable.contains(&c));
        assert!(!reachable.contains(&b)); // b is not connected to output
    }

    #[test]
    fn test_executor_single_op() {
        let source = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(100, 100, 100, 255)).unwrap();

        let mut graph = OpGraph::new();
        let src = graph.add_node(Box::new(SourceOp { image: source }), &[]);
        let bright = graph.add_node(Box::new(BrightnessOp { amount: 20.0 }), &[src]);
        graph.set_output(bright);

        let executor = GraphExecutor::new();
        let result = executor.execute(&graph, Rect::new(0, 0, 128, 128)).unwrap();

        let px = result.get_pixel(64, 64);
        assert_eq!(px.r, 120); // 100 + 20
        assert_eq!(px.g, 120);
        assert_eq!(px.b, 120);
    }

    #[test]
    fn test_executor_chain() {
        let source = TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(100, 100, 100, 255)).unwrap();

        let mut graph = OpGraph::new();
        let src = graph.add_node(Box::new(SourceOp { image: source }), &[]);
        let bright = graph.add_node(Box::new(BrightnessOp { amount: 20.0 }), &[src]);
        let contrast = graph.add_node(Box::new(ContrastOp { factor: 1.5 }), &[bright]);
        graph.set_output(contrast);

        let executor = GraphExecutor::new();
        let result = executor.execute(&graph, Rect::new(0, 0, 128, 128)).unwrap();

        let px = result.get_pixel(64, 64);
        // brightness: 100 + 20 = 120
        // contrast: (120 - 128) * 1.5 + 128 = -8 * 1.5 + 128 = -12 + 128 = 116
        assert_eq!(px.r, 116);
    }

    #[test]
    fn test_fused_op() {
        let a: Box<dyn Op> = Box::new(BrightnessOp { amount: 10.0 });
        let b: Box<dyn Op> = Box::new(ContrastOp { factor: 1.2 });

        let fused = FusedOp::try_new(a, b);
        assert!(fused.is_some());
        let fused = fused.unwrap();
        assert_eq!(fused.name(), "brightness+contrast");
    }

    #[test]
    fn test_tile_parallel_execution() {
        // Create a large image (256x256 = 4 tiles of 128x128).
        let source =
            TiledImage::with_color(256, 256, PixelFormat::Rgba8, Pixel::new(100, 100, 100, 255)).unwrap();

        let mut graph = OpGraph::new();
        let src = graph.add_node(Box::new(SourceOp { image: source }), &[]);
        let bright = graph.add_node(Box::new(BrightnessOp { amount: 20.0 }), &[src]);
        graph.set_output(bright);

        let executor = GraphExecutor::with_threads(2);
        let result = executor
            .execute(&graph, Rect::new(0, 0, 256, 256))
            .unwrap();

        // Check pixels in different tiles.
        assert_eq!(result.get_pixel(0, 0).r, 120); // tile (0,0)
        assert_eq!(result.get_pixel(200, 200).r, 120); // tile (1,1)
        assert_eq!(result.get_pixel(255, 255).r, 120); // edge pixel
    }

    #[test]
    fn test_executor_region_offset_returns_relative_coords() {
        // A source larger than the ROI: the result must be ROI-sized with
        // ROI-relative coordinates, matching the documented contract.
        let source =
            TiledImage::with_color(128, 128, PixelFormat::Rgba8, Pixel::new(100, 100, 100, 255))
                .unwrap();

        let mut graph = OpGraph::new();
        let src = graph.add_node(Box::new(SourceOp { image: source }), &[]);
        let bright = graph.add_node(Box::new(BrightnessOp { amount: 20.0 }), &[src]);
        graph.set_output(bright);

        let executor = GraphExecutor::with_threads(2);
        let roi = Rect::new(10, 20, 64, 64);
        let result = executor.execute(&graph, roi).unwrap();

        assert_eq!(result.width(), 64);
        assert_eq!(result.height(), 64);
        // (0, 0) in the result maps to source (10, 20): 100 + 20 = 120.
        assert_eq!(result.get_pixel(0, 0).r, 120);
        assert_eq!(result.get_pixel(63, 63).r, 120);
    }
}
