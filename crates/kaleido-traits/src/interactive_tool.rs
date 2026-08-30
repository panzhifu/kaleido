//! Interactive tool contract — the foundation for pointer-driven tools
//! such as brushes and spray cans.
//!
//! [`Tool`] is a one-shot contract (`apply(image, params)`), which fits
//! filters but cannot express an interactive stroke. [`InteractiveTool`]
//! extends it with a pointer event stream:
//!
//! ```text
//! on_mouse_down  ──►  on_mouse_drag ──►  … ──►  on_mouse_up  ──►  on_stroke_end
//!      │                   │                                   │
//!      └── host snapshots before state (undo)  ────────────────┴── host records after state
//! ```
//!
//! The **host** (not the plugin) is responsible for:
//! - converting screen coordinates to image coordinates before the call,
//! - snapshotting the affected tiles on `on_mouse_down` (undo),
//! - committing the stroke to the history on `on_mouse_up`.
//!
//! The plugin only reads its [`ToolContext`], mutates
//! `ctx.image`, and records modified tiles into `ctx.dirty_tiles`.

use kaleido_core::{ImageResult, TileCoord, TiledImage};

use crate::keyboard::{KeyEvent, KeyModifiers, KeyState};
use crate::Tool;

// ---------------------------------------------------------------------------
// Pointer input
// ---------------------------------------------------------------------------

/// Stage of a pointer interaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerKind {
    /// Pointer pressed.
    Down,
    /// Pointer moved while pressed.
    Drag,
    /// Pointer released.
    Up,
}

/// Bitmask of pressed mouse buttons.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PointerButtons(u8);

impl PointerButtons {
    pub const PRIMARY: u8 = 1 << 0;
    pub const SECONDARY: u8 = 1 << 1;
    pub const MIDDLE: u8 = 1 << 2;

    /// Creates a button mask from raw bits.
    pub const fn new(bits: u8) -> Self {
        Self(bits)
    }

    pub const fn is_primary(&self) -> bool {
        self.0 & Self::PRIMARY != 0
    }

    pub const fn is_secondary(&self) -> bool {
        self.0 & Self::SECONDARY != 0
    }

    pub const fn is_middle(&self) -> bool {
        self.0 & Self::MIDDLE != 0
    }

    pub const fn is_empty(&self) -> bool {
        self.0 == 0
    }
}

/// Bitmask of keyboard modifiers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers(u8);

impl Modifiers {
    pub const SHIFT: u8 = 1 << 0;
    pub const CTRL: u8 = 1 << 1;
    pub const ALT: u8 = 1 << 2;

    /// Creates a modifier mask from raw bits.
    pub const fn new(bits: u8) -> Self {
        Self(bits)
    }

    pub const fn shift(&self) -> bool {
        self.0 & Self::SHIFT != 0
    }

    pub const fn ctrl(&self) -> bool {
        self.0 & Self::CTRL != 0
    }

    pub const fn alt(&self) -> bool {
        self.0 & Self::ALT != 0
    }
}

/// A single pointer event delivered to an interactive tool.
///
/// Coordinates are already in **image space** (the host applies the
/// viewport transform), so plugins never need to know about zoom/pan.
#[derive(Debug, Clone, Copy)]
pub struct PointerEvent {
    /// Image-space X coordinate.
    pub x: f32,
    /// Image-space Y coordinate.
    pub y: f32,
    /// Pressure in 0.0..=1.0 (1.0 when unknown).
    pub pressure: f32,
    /// Pressed buttons at the time of the event.
    pub buttons: PointerButtons,
    /// Keyboard modifiers at the time of the event.
    pub modifiers: Modifiers,
    /// Which stage of the interaction this event is.
    pub kind: PointerKind,
}

impl PointerEvent {
    /// Creates a new pointer event.
    pub const fn new(
        x: f32,
        y: f32,
        pressure: f32,
        buttons: PointerButtons,
        modifiers: Modifiers,
        kind: PointerKind,
    ) -> Self {
        Self {
            x,
            y,
            pressure,
            buttons,
            modifiers,
            kind,
        }
    }

    /// Creates a simple button-down event.
    pub const fn down(x: f32, y: f32) -> Self {
        Self::new(
            x,
            y,
            1.0,
            PointerButtons::new(PointerButtons::PRIMARY),
            Modifiers::new(0),
            PointerKind::Down,
        )
    }
}

// ---------------------------------------------------------------------------
// ToolContext
// ---------------------------------------------------------------------------

/// Everything an interactive tool needs for the current event.
///
/// The context borrows the **active layer** image and lets the tool
/// record which tiles it modified (the host uses them for incremental
/// redraw later).
pub struct ToolContext<'a> {
    /// The active layer's pixel buffer — the tool draws into this.
    pub image: &'a mut TiledImage,
    /// Document width in pixels.
    pub document_width: u32,
    /// Document height in pixels.
    pub document_height: u32,
    /// Tiles the tool touched during this event. The host accumulates
    /// them across the stroke and can repaint only those tiles.
    pub dirty_tiles: &'a mut Vec<TileCoord>,
}

impl<'a> ToolContext<'a> {
    /// Creates a new tool context.
    pub fn new(
        image: &'a mut TiledImage,
        document_width: u32,
        document_height: u32,
        dirty_tiles: &'a mut Vec<TileCoord>,
    ) -> Self {
        Self {
            image,
            document_width,
            document_height,
            dirty_tiles,
        }
    }

    /// Marks the tile containing image-space point (x, y) as dirty.
    pub fn mark_dirty(&mut self, x: f32, y: f32) {
        if x < 0.0 || y < 0.0 {
            return;
        }
        let (x, y) = (x as u32, y as u32);
        if x >= self.image.width() || y >= self.image.height() {
            return;
        }
        let coord = TileCoord::new(x / kaleido_core::TILE_SIZE, y / kaleido_core::TILE_SIZE);
        if !self.dirty_tiles.contains(&coord) {
            self.dirty_tiles.push(coord);
        }
    }

    /// Marks the whole tile grid as dirty (used by tools that touch
    /// arbitrary areas, e.g. fill).
    pub fn mark_all_dirty(&mut self) {
        for row in 0..self.image.tile_rows() {
            for col in 0..self.image.tile_cols() {
                let coord = TileCoord::new(col, row);
                if !self.dirty_tiles.contains(&coord) {
                    self.dirty_tiles.push(coord);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// InteractiveTool
// ---------------------------------------------------------------------------

/// A tool driven by a pointer event stream (brush, spray, eraser…).
///
/// Extends [`Tool`] so interactive tools reuse the existing registration,
/// menu path and parameter schema machinery.
///
/// Implementations may leave any method at its default no-op.
pub trait InteractiveTool: Tool {
    /// Called when the pointer is pressed in the canvas.
    fn on_mouse_down(&self, ctx: &mut ToolContext, event: &PointerEvent) -> ImageResult<()> {
        let _ = (ctx, event);
        Ok(())
    }

    /// Called while the pointer moves with a button held down.
    fn on_mouse_drag(&self, ctx: &mut ToolContext, event: &PointerEvent) -> ImageResult<()> {
        let _ = (ctx, event);
        Ok(())
    }

    /// Called when the pointer is released, ending the stroke.
    fn on_mouse_up(&self, ctx: &mut ToolContext, event: &PointerEvent) -> ImageResult<()> {
        let _ = (ctx, event);
        Ok(())
    }

    /// Called after the stroke's after-state has been captured. Use for
    /// one-time cleanup or post-processing that must not appear in undo
    /// deltas (e.g. merging duplicate pixels).
    fn on_stroke_end(&self, _ctx: &mut ToolContext) -> ImageResult<()> {
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Keyboard
    // -----------------------------------------------------------------------

    /// Called when a key is pressed while this tool is active.
    ///
    /// Use this to handle shortcuts such as:
    /// - `[` / `]` to decrease / increase brush size.
    /// - `Shift` to constrain proportions or draw straight lines.
    /// - `Esc` to cancel the current stroke.
    ///
    /// The default implementation is a no-op.
    fn on_key_down(&self, _event: &KeyEvent) -> ImageResult<()> {
        Ok(())
    }

    /// Called when a key is released while this tool is active.
    ///
    /// The default implementation is a no-op.
    fn on_key_up(&self, _event: &KeyEvent) -> ImageResult<()> {
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Tool lifecycle
    // -----------------------------------------------------------------------

    /// Called when this tool becomes active (the user selected it).
    ///
    /// Use it to reset per-tool state, initialise resources, or read
    /// the current image dimensions. The default implementation is a
    /// no-op.
    fn on_activate(&self) {}

    /// Called when this tool is being deselected (the user switched
    /// to another tool).
    ///
    /// Use it to clean up temporary state, cancel pending operations,
    /// or commit half-finished work. The default implementation is a
    /// no-op.
    fn on_deactivate(&self) {}

    // -----------------------------------------------------------------------
    // Modifier convenience (optional override)
    // -----------------------------------------------------------------------

    /// Returns the current modifier keys from the host's key state.
    ///
    /// Tools that need to react to held modifiers (Shift, Ctrl) inside
    /// `on_mouse_drag` can call this through the host-provided
    /// [`KeyState`]. The default always returns no modifiers.
    fn modifiers(&self, _state: &dyn KeyState) -> KeyModifiers {
        KeyModifiers::new(0)
    }

    // -----------------------------------------------------------------------
    // Stroke query
    // -----------------------------------------------------------------------

    /// Returns `true` when this tool is mid-stroke and a pointer
    /// release should be treated as the end of the stroke rather than
    /// a stray event.
    ///
    /// The host uses this to decide whether to call `on_mouse_up` from
    /// a global key handler (e.g. to finalise a stroke on `Enter`).
    /// The default always returns `false`.
    fn is_stroke_active(&self) -> bool {
        false
    }

    // -----------------------------------------------------------------------
    // Escape handling
    // -----------------------------------------------------------------------

    /// Called when the user presses Escape.
    ///
    /// The default cancels the active stroke (if any) and returns
    /// `true` to indicate the event was handled. Override to implement
    /// custom escape behaviour (e.g. dismiss a preview).
    fn on_escape(&self) -> bool {
        false
    }
}
