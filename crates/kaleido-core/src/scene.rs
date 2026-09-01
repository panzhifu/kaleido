//! The scene graph — a tree of editable nodes.
//!
//! All five modes (vector / pixel / paint / type / animation) operate on
//! this single tree.  Nodes are addressed by stable [`NodeId`]s stored in a
//! hash map (slotmap-style), so removing a node never invalidates handles
//! held by other parts of the system.

use std::collections::{HashMap, HashSet};

use super::effects::EffectBinding;
use super::mask::Mask;
use super::pixel_layer::PixelLayer;
use super::text::TextObject;
use super::types::{BlendMode, NodeId, Transform2D};
use super::vector::VectorObject;

/// What a node *is* — the five-mode landing point.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum NodeContent {
    /// Container for child nodes (no pixels of its own).
    Group,
    /// Raster layer — pixel / paint modes.
    Pixel(PixelLayer),
    /// Editable Bézier object — vector mode.
    Vector(VectorObject),
    /// Rich text — type / layout mode.
    Text(TextObject),
}

/// A node in the scene tree.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    /// Child node ids, in paint order (first = bottom).
    pub children: Vec<NodeId>,
    pub name: String,
    pub transform: Transform2D,
    pub opacity: f32,
    pub visible: bool,
    pub locked: bool,
    pub blend_mode: BlendMode,
    pub content: NodeContent,
    pub mask: Option<Mask>,
    /// Plugin-provided effect chain (adjustment layers included).
    pub effects: Vec<EffectBinding>,
}

impl Node {
    /// Creates a node with the given content.
    pub fn new(id: NodeId, name: impl Into<String>, content: NodeContent) -> Self {
        Self {
            id,
            parent: None,
            children: Vec::new(),
            name: name.into(),
            transform: Transform2D::identity(),
            opacity: 1.0,
            visible: true,
            locked: false,
            blend_mode: BlendMode::Normal,
            content,
            mask: None,
            effects: Vec::new(),
        }
    }

    /// Whether this node is a group (can hold children).
    #[inline]
    pub fn is_group(&self) -> bool {
        matches!(self.content, NodeContent::Group)
    }

    /// Whether this node can own child nodes (only groups can).
    #[inline]
    pub fn can_have_children(&self) -> bool {
        self.is_group()
    }
}

/// The scene: root node + node storage.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Scene {
    root: NodeId,
    nodes: HashMap<NodeId, Node>,
    next_id: u64,
}

impl Scene {
    /// Creates an empty scene with a single group root.
    pub fn new() -> Self {
        let root = NodeId(1);
        let mut scene = Self {
            root,
            nodes: HashMap::new(),
            next_id: 2,
        };
        let root_node = Node::new(root, "Root", NodeContent::Group);
        scene.nodes.insert(root, root_node);
        scene
    }

    /// The root node id.
    #[inline]
    pub fn root(&self) -> NodeId {
        self.root
    }

    /// Borrows a node by id.
    #[inline]
    pub fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.get(&id)
    }

    /// Mutably borrows a node by id.
    #[inline]
    pub fn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(&id)
    }

    /// Total number of nodes (including the root).
    #[inline]
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Iterates all nodes (unordered).
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &Node> {
        self.nodes.values()
    }

    /// Iterates all nodes mutably (unordered).
    #[inline]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Node> {
        self.nodes.values_mut()
    }

    /// Allocates a fresh, unused node id.
    #[inline]
    pub fn alloc_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }

    // -- Tree mutation -----------------------------------------------------

    /// Adds a node as a child of `parent`, returning its id.
    ///
    /// The node content is wrapped in a fresh [`Node`] with default styling.
    /// Returns `None` if `parent` does not exist or is not a group.
    pub fn add_node(&mut self, parent: NodeId, name: impl Into<String>, content: NodeContent) -> Option<NodeId> {
        let is_group = self.nodes.get(&parent)?.is_group();
        if !is_group {
            return None;
        }
        let id = self.alloc_id();
        let mut node = Node::new(id, name, content);
        node.parent = Some(parent);
        self.nodes.get_mut(&parent)?.children.push(id);
        self.nodes.insert(id, node);
        Some(id)
    }

    /// Adds a node as a child of `parent` at a specific paint-order index.
    ///
    /// `index` is clamped to `[0, children.len()]`.  Returns `None` if
    /// `parent` does not exist or is not a group.
    pub fn add_node_at(
        &mut self,
        parent: NodeId,
        index: usize,
        name: impl Into<String>,
        content: NodeContent,
    ) -> Option<NodeId> {
        let is_group = self.nodes.get(&parent)?.is_group();
        if !is_group {
            return None;
        }
        let id = self.alloc_id();
        let mut node = Node::new(id, name, content);
        node.parent = Some(parent);
        let children = &mut self.nodes.get_mut(&parent)?.children;
        children.insert(index.min(children.len()), id);
        self.nodes.insert(id, node);
        Some(id)
    }

    /// Removes a node and its whole subtree.  Returns `false` if the node
    /// does not exist or is the root.
    pub fn remove_node(&mut self, id: NodeId) -> bool {
        if id == self.root {
            return false;
        }
        let Some(node) = self.nodes.get(&id) else {
            return false;
        };
        let parent = node.parent;
        if let Some(parent_id) = parent
            && let Some(parent_node) = self.nodes.get_mut(&parent_id)
        {
            parent_node.children.retain(|c| *c != id);
        }
        // Collect the whole subtree, then drop it.
        let mut to_remove = Vec::new();
        self.collect_subtree(id, &mut to_remove);
        for n in to_remove {
            self.nodes.remove(&n);
        }
        true
    }

    /// Reparents a node under a new parent (appended last).
    ///
    /// Returns `false` if either id is missing, if `new_parent` is not a
    /// group, or if the move would create a cycle (moving a node under one
    /// of its own descendants).
    pub fn reparent(&mut self, id: NodeId, new_parent: NodeId) -> bool {
        if id == self.root || id == new_parent {
            return false;
        }
        if self.is_descendant_of(new_parent, id) {
            return false;
        }
        let new_parent_is_group = self.nodes.get(&new_parent).is_some_and(|n| n.is_group());
        if !new_parent_is_group {
            return false;
        }
        let Some(old_parent) = self.nodes.get(&id).and_then(|n| n.parent) else {
            return false;
        };
        if let Some(op) = self.nodes.get_mut(&old_parent) {
            op.children.retain(|c| *c != id);
        }
        if let Some(n) = self.nodes.get_mut(&id) {
            n.parent = Some(new_parent);
        }
        if let Some(np) = self.nodes.get_mut(&new_parent) {
            np.children.push(id);
        }
        true
    }

    /// Moves `child` within `parent`'s child list to `to_index`
    /// (paint order; 0 = bottom).  Returns `false` if `parent` or `child`
    /// is missing, or `child` is not a direct child of `parent`.
    pub fn reorder_child(&mut self, parent: NodeId, child: NodeId, to_index: usize) -> bool {
        let Some(children) = self.nodes.get(&parent).map(|n| n.children.clone()) else {
            return false;
        };
        if !children.contains(&child) {
            return false;
        }
        let mut children = children;
        children.retain(|c| *c != child);
        let to_index = to_index.min(children.len());
        children.insert(to_index, child);
        if let Some(n) = self.nodes.get_mut(&parent) {
            n.children = children;
        }
        true
    }

    // -- Tree queries ------------------------------------------------------

    /// Children of a node, in paint order (bottom first).
    #[inline]
    pub fn children(&self, id: NodeId) -> Option<&Vec<NodeId>> {
        self.nodes.get(&id).map(|n| &n.children)
    }

    /// Depth-first list of all nodes strictly below `id` (excluding `id`
    /// itself).  Empty if the node is absent or a leaf.
    pub fn descendants(&self, id: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        self.collect_descendants(id, &mut out);
        out
    }

    /// Whether `node` lies strictly inside the subtree rooted at `ancestor`
    /// (i.e. `ancestor` is a proper ancestor of `node`).
    pub fn is_descendant_of(&self, node: NodeId, ancestor: NodeId) -> bool {
        if node == ancestor {
            return false;
        }
        let mut cur = match self.nodes.get(&node).and_then(|n| n.parent) {
            Some(p) => p,
            None => return false,
        };
        while cur != ancestor {
            match self.nodes.get(&cur).and_then(|n| n.parent) {
                Some(p) => cur = p,
                None => return false,
            }
        }
        true
    }

    /// Convenience alias: whether `ancestor` is a proper ancestor of `node`.
    #[inline]
    pub fn is_ancestor_of(&self, ancestor: NodeId, node: NodeId) -> bool {
        self.is_descendant_of(node, ancestor)
    }

    /// Depth (distance from root) of a node.  `None` if the node is missing
    /// or unreachable from the root (a corrupted tree).
    pub fn depth_of(&self, id: NodeId) -> Option<u32> {
        let mut depth = 0;
        let mut cur = id;
        while cur != self.root {
            let p = self.nodes.get(&cur).and_then(|n| n.parent)?;
            cur = p;
            depth += 1;
            if depth > self.nodes.len() as u32 {
                return None; // cycle — never reaches the root
            }
        }
        Some(depth)
    }

    /// Tree-integrity check:
    ///
    /// - the root exists and has no parent;
    /// - every child exists, has a parent pointer back to its list, and is
    ///   reachable exactly once from the root (no cycles, no orphans).
    pub fn validate(&self) -> bool {
        let Some(root_node) = self.nodes.get(&self.root) else {
            return false;
        };
        if root_node.parent.is_some() {
            return false;
        }
        let mut visited = HashSet::new();
        if !self.validate_node(self.root, &mut visited) {
            return false;
        }
        visited.len() == self.nodes.len()
    }

    fn validate_node(&self, id: NodeId, visited: &mut HashSet<NodeId>) -> bool {
        if !visited.insert(id) {
            return false; // cycle
        }
        let Some(node) = self.nodes.get(&id) else {
            return false;
        };
        for &child in &node.children {
            match self.nodes.get(&child) {
                Some(cn) if cn.parent == Some(id) => {}
                _ => return false,
            }
            if !self.validate_node(child, visited) {
                return false;
            }
        }
        true
    }

    fn collect_subtree(&self, id: NodeId, out: &mut Vec<NodeId>) {
        out.push(id);
        if let Some(node) = self.nodes.get(&id) {
            for child in &node.children {
                self.collect_subtree(*child, out);
            }
        }
    }

    fn collect_descendants(&self, id: NodeId, out: &mut Vec<NodeId>) {
        let Some(node) = self.nodes.get(&id) else {
            return;
        };
        for child in &node.children {
            out.push(*child);
            self.collect_descendants(*child, out);
        }
    }
}
