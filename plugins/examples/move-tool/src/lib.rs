//! Move tool plugin — interactive tool for moving layers on the canvas.
//!
//! Demonstrates:
//! - InteractiveTool trait implementation
//! - Keyboard shortcut registration (M key)
//! - Toolbar icon registration
//! - Automatic history service integration

use std::sync::Arc;

use cordis::{Inject, PluginHandle, PluginOutput, plugin_sync};
use kaleido_core::Transform2D;
use kaleido_traits::layer::LayerService;
use kaleido_traits::plugins::tool::{
    InteractiveTool, PointerEvent, Tool, ToolContext, ToolParams,
};
use kaleido_traits::shortcut::ShortcutService;

// ---------------------------------------------------------------------------
// MoveTool
// ---------------------------------------------------------------------------

/// Interactive tool for moving the active layer.
pub struct MoveTool {
    /// Stroke state: whether a drag is in progress.
    dragging: bool,
    /// Starting position of the drag.
    start_x: u32,
    start_y: u32,
}

impl MoveTool {
    pub fn new() -> Self {
        Self {
            dragging: false,
            start_x: 0,
            start_y: 0,
        }
    }
}

impl Tool for MoveTool {
    fn name(&self) -> &str {
        "move"
    }

    fn menu_path(&self) -> String {
        "编辑/移动".into()
    }

    fn description(&self) -> String {
        "Move the active layer (shortcut: M)".into()
    }

    fn apply(
        &self,
        _image: &mut kaleido_core::TiledImage,
        _params: &ToolParams,
    ) -> kaleido_core::ImageResult<()> {
        // Interactive tools don't use the apply() path.
        Ok(())
    }

    fn icon(&self) -> Option<&str> {
        Some("move")
    }

    fn category(&self) -> kaleido_traits::plugins::category::ToolCategory {
        kaleido_traits::plugins::category::ToolCategory::Transform
    }

    fn is_enabled(&self) -> bool {
        true
    }
}

impl InteractiveTool for MoveTool {
    fn on_mouse_down(&mut self, event: &PointerEvent, _ctx: &dyn ToolContext) {
        self.dragging = true;
        self.start_x = event.x;
        self.start_y = event.y;
    }

    fn on_mouse_drag(&mut self, event: &PointerEvent, ctx: &dyn ToolContext) {
        if !self.dragging {
            return;
        }

        // Calculate delta from start position.
        let dx = event.x as f32 - self.start_x as f32;
        let dy = event.y as f32 - self.start_y as f32;

        // Apply the translation to the active layer.
        if let (Some(layer_service), Some(layer_id)) =
            (ctx.layer_service(), ctx.active_layer())
        {
            let _ = layer_service.set_transform(
                layer_id,
                Transform2D {
                    tx: dx,
                    ty: dy,
                    rotation: 0.0,
                    sx: 1.0,
                    sy: 1.0,
                },
            );
        }
    }

    fn on_mouse_up(&mut self, event: &PointerEvent, ctx: &dyn ToolContext) {
        if !self.dragging {
            return;
        }
        self.dragging = false;

        // Apply final translation.
        let dx = event.x as f32 - self.start_x as f32;
        let dy = event.y as f32 - self.start_y as f32;

        if let (Some(layer_service), Some(layer_id)) =
            (ctx.layer_service(), ctx.active_layer())
        {
            let _ = layer_service.set_transform(
                layer_id,
                Transform2D {
                    tx: dx,
                    ty: dy,
                    rotation: 0.0,
                    sx: 1.0,
                    sy: 1.0,
                },
            );
        }

        // Note: history is recorded by the host (desktop) before
        // passing events to the plugin.
    }

    fn is_stroke_active(&self) -> bool {
        self.dragging
    }

    fn supports_shortcuts(&self) -> bool {
        true
    }

    fn shortcut_key(&self) -> Option<&str> {
        Some("M")
    }

    fn toolbar_icon(&self) -> Option<&str> {
        Some("move")
    }
}

// ---------------------------------------------------------------------------
// ToolContext implementation for the desktop host.
// ---------------------------------------------------------------------------

/// Provides access to services during tool interaction.
pub struct MoveToolContext {
    pub layers: Option<Arc<dyn LayerService>>,
    pub active_layer: Option<kaleido_core::NodeId>,
}

impl ToolContext for MoveToolContext {
    fn layer_service(&self) -> Option<Arc<dyn LayerService>> {
        self.layers.clone()
    }

    fn active_layer(&self) -> Option<kaleido_core::NodeId> {
        self.active_layer
    }
}

// ---------------------------------------------------------------------------
// Plugin entry point.
// ---------------------------------------------------------------------------

/// Creates the move tool plugin.
pub fn move_tool_plugin() -> PluginHandle {
    plugin_sync::<(), _>(
        "tool.move",
        Inject::new(["shortcut_service"]),
        move |ctx, _config| {
            // Register the keyboard shortcut.
            if let Ok(Some(shortcuts)) =
                ctx.get::<Arc<dyn ShortcutService>>("shortcut_service")
            {
                let binding = kaleido_traits::keyboard::ShortcutBinding {
                    key: "m".into(),
                    action: "tool.move.activate".into(),
                    source: kaleido_traits::keyboard::ShortcutSource::Default,
                };
                let _ = shortcuts.register_global(binding);
            }

            Ok(PluginOutput::default())
        },
    )
}
