//! Dock workspace — uses the library's `DockArea` + `DockLayout` for
//! fully resizable panels with drag-to-resize support.

use std::sync::Arc;

use gpui::*;
use gpui_component::dock::{DockArea, DockLayout, DockSkin};

use super::tool_panel::ToolPanel;
use super::color_panel::ColorPanelProps;
use super::layers_panel::LayersPanel;
use super::ActiveTool;
use crate::canvas::Canvas;
use crate::GlobalKaleidoApp;

/// Dock workspace — library-powered resizable panels.
pub struct DockLayoutView {
    dock_area: Entity<DockArea>,
    _skin: std::rc::Rc<DockSkin>,
}

impl DockLayoutView {
    pub fn new(
        app: GlobalKaleidoApp,
        canvas: Entity<Canvas>,
        active_tool: Entity<ActiveTool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (dock_area, skin) =
            DockSkin::dock_area("main-dock", Some(1), window, cx);

        // ── Build panels ────────────────────────────────────────
        let tool_panel = cx.new(|cx| ToolPanel::new(active_tool.clone(), cx));
        let layers_panel = cx.new(|cx| LayersPanel::new(app.clone(), cx));
        let color_panel = cx.new(|cx| ColorPanelProps::new(app.clone(), cx));

        // ── Left: tools ─────────────────────────────────────────
        let left = DockLayout::tabs().panel_view(Arc::new(tool_panel), cx);

        // ── Right: layers + color (vertical split) ──────────────
        let right = DockLayout::v_split()
            .child(
                DockLayout::tabs().panel_view(Arc::new(layers_panel), cx),
                None,
            )
            .child(
                DockLayout::tabs().panel_view(Arc::new(color_panel), cx),
                Some(px(180.)),
            );

        // ── Center: canvas ──────────────────────────────────────
        let center = DockLayout::tabs().panel_view(Arc::new(canvas), cx);

        // ── Assemble: left | center | right ────────────────────
        let layout = DockLayout::h_split()
            .child(left, Some(px(200.)))
            .child(center, None)
            .child(right, Some(px(260.)));

        dock_area.update(cx, |view, cx| {
            view.set_center(layout, window, cx);
        });

        Self {
            dock_area,
            _skin: skin,
        }
    }
}

impl Render for DockLayoutView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.dock_area.clone().into_element()
    }
}
