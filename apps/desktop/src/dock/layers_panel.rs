//! Layers panel — shows document layers with selection and management.

use gpui::*;
use gpui_base::dock::Panel as BasePanel;
use gpui_component::{
    ActiveTheme as _, Icon, IconName, StyledExt as _, dock::PanelEvent,
};
use gpui_component::dock::Panel;

use crate::GlobalKaleidoApp;

/// Layers panel — displays the document's layer stack.
pub struct LayersPanel {
    focus_handle: FocusHandle,
    app: GlobalKaleidoApp,
    /// Cached layer IDs (top-to-bottom paint order).
    layer_ids: Vec<kaleido_core::NodeId>,
    /// Currently selected layer.
    active_layer: Option<kaleido_core::NodeId>,
    /// Whether a document is open.
    has_document: bool,
}

impl LayersPanel {
    pub fn new(app: GlobalKaleidoApp, cx: &mut Context<Self>) -> Self {
        let mut panel = Self {
            focus_handle: cx.focus_handle(),
            app,
            layer_ids: Vec::new(),
            active_layer: None,
            has_document: false,
        };
        panel.refresh();
        panel
    }

    /// Refreshes layer data from services.
    pub fn refresh(&mut self) {
        let layers = self.app.layer_service();
        self.active_layer = layers.active_layer();
        self.layer_ids = layers.layer_ids().unwrap_or_default();
        self.has_document = self.app.data_service().has_document();
    }
}

impl BasePanel for LayersPanel {
    fn panel_name(&self) -> &'static str {
        "Layers"
    }
}

impl Panel for LayersPanel {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl EventEmitter<PanelEvent> for LayersPanel {}

impl Focusable for LayersPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for LayersPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Refresh on every render to stay in sync.
        self.refresh();
        let app = self.app.clone();
        let this = cx.weak_entity();

        // Pre-compute layer data for rendering.
        let layer_data: Vec<(kaleido_core::NodeId, String, bool)> = self
            .layer_ids
            .iter()
            .rev()
            .map(|id| {
                let id = *id;
                let name = app
                    .layer_service()
                    .layer(id)
                    .ok()
                    .flatten()
                    .map(|info| info.name.clone())
                    .unwrap_or_else(|| format!("{:?}", id));
                let is_active = self.active_layer == Some(id);
                (id, name, is_active)
            })
            .collect();

        div()
            .id("layers-panel")
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            // ── Header with add/remove buttons ────────────────────────
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .text_xs()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("图层"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_0p5()
                            .child({
                        let app = app.clone();
                        let this = cx.weak_entity();
                        // Add layer button.
                        div()
                            .w(px(20.0))
                            .h(px(20.0))
                            .rounded(px(3.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(cx.theme().foreground.opacity(0.1)))
                            .child(Icon::new(IconName::Plus).size(px(12.0)))
                            .on_mouse_down(
                                MouseButton::Left,
                                move |_event, _window, cx| {
                                    let layers = app.layer_service();
                                    let _ = layers.add_pixel_layer(
                                        "New Layer",
                                        64,
                                        64,
                                        kaleido_core::PixelFormat::Rgba8,
                                    );
                                    let _ = this.update(cx, |panel, cx| {
                                        panel.refresh();
                                        cx.notify();
                                    });
                                },
                            )
                    })
                    .child({
                        let app = app.clone();
                        let this = cx.weak_entity();
                        // Remove layer button.
                        div()
                            .w(px(20.0))
                            .h(px(20.0))
                            .rounded(px(3.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|s| s.bg(cx.theme().foreground.opacity(0.1)))
                            .child(Icon::new(IconName::Minus).size(px(12.0)))
                            .on_mouse_down(
                                MouseButton::Left,
                                move |_event, _window, cx| {
                                    let layers = app.layer_service();
                                    if let Some(active) = layers.active_layer() {
                                        let _ = layers.remove(active);
                                    }
                                    let _ = this.update(cx, |panel, cx| {
                                        panel.refresh();
                                        cx.notify();
                                    });
                                },
                            )
                    }),
                    ),
            )
            // ── Layer list ───────────────────────────────────────────
            .child(if self.has_document && !layer_data.is_empty() {
                div()
                    .flex()
                    .flex_col()
                    .size_full()
                    .children(layer_data.iter().map(|(id, name, is_active)| {
                        let id = *id;
                        let name = name.clone();
                        let is_active = *is_active;
                        let app = app.clone();
                        let this = cx.weak_entity();
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_2()
                            .py_1()
                            .h(px(28.0))
                            .bg(if is_active {
                                cx.theme().foreground.opacity(0.12)
                            } else {
                                cx.theme().background
                            })
                            .rounded(px(3.0))
                            .cursor_pointer()
                            .text_color(cx.theme().foreground)
                            .hover(|s| {
                                if !is_active {
                                    s.bg(cx.theme().foreground.opacity(0.05))
                                } else {
                                    s
                                }
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                move |_event, _window, cx| {
                                    let layers = app.layer_service();
                                    let _ = layers.set_active(id);
                                    let _ = this.update(cx, |panel, cx| {
                                        panel.refresh();
                                        cx.notify();
                                    });
                                },
                            )
                            .child(
                                div()
                                    .w(px(14.0))
                                    .h(px(14.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(cx.theme().foreground.opacity(0.5))
                                    .child(Icon::new(IconName::Eye).size(px(10.0))),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .text_xs()
                                    .child(name),
                            )
                    }))
                    .into_any_element()
            } else {
                div()
                    .size_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().foreground.opacity(0.5))
                            .child("没有图层"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().foreground.opacity(0.3))
                            .child("打开图片或点击 + 添加"),
                    )
                    .into_any_element()
            })
    }
}
