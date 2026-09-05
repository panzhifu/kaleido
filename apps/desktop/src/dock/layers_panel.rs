//! Layers panel — shows document layers with selection and management.

use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::base::dock::Panel as BasePanel;
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Sizable,
    button::{Button, ButtonVariants}, dock::PanelEvent,
};
use gpui_kit::component::dock::Panel;
use rust_i18n::t;

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

    /// Adds a new pixel layer.
    fn add_layer(&mut self, cx: &mut Context<Self>) {
        let layers = self.app.layer_service();
        match layers.add_pixel_layer(
            "New Layer",
            64,
            64,
            kaleido_core::PixelFormat::Rgba8,
        ) {
            Ok(_) => {
                self.refresh();
                cx.notify();
            }
            Err(e) => {
                tracing::warn!("failed to add layer: {e}");
            }
        }
    }

    /// Removes the active layer.
    fn remove_active_layer(&mut self, cx: &mut Context<Self>) {
        let layers = self.app.layer_service();
        if let Some(active) = layers.active_layer() {
            if let Err(e) = layers.remove(active) {
                tracing::warn!("failed to remove layer: {e}");
            } else {
                self.refresh();
                cx.notify();
            }
        }
    }

    /// Sets the active layer by ID.
    fn set_active_layer(&mut self, id: kaleido_core::NodeId, cx: &mut Context<Self>) {
        let layers = self.app.layer_service();
        if let Err(e) = layers.set_active(id) {
            tracing::warn!("failed to set active layer: {e}");
        } else {
            self.refresh();
            cx.notify();
        }
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
        let app = self.app.clone();

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
            // Header with add/remove buttons
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
                            .child(t!("layers.title")),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_0p5()
                            .child(
                                Button::new("add-layer")
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::Plus)
                                    .tooltip(t!("layers.add_tooltip"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.add_layer(cx);
                                    })),
                            )
                            .child(
                                Button::new("remove-layer")
                                    .ghost()
                                    .xsmall()
                                    .icon(IconName::Minus)
                                    .tooltip(t!("layers.remove_tooltip"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.remove_active_layer(cx);
                                    })),
                            ),
                    ),
            )
            // Layer list
            .when(self.has_document && !layer_data.is_empty(), |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .size_full()
                        .children(layer_data.iter().map(|(id, name, is_active)| {
                            let id = *id;
                            let name = name.clone();
                            let is_active = *is_active;
                            div()
                                .id(("layer-row", id.0))
                                .flex()
                                .items_center()
                                .gap_1()
                                .px_2()
                                .py_1()
                                .h_7()
                                .when(is_active, |el| {
                                    el.bg(cx.theme().foreground.opacity(0.12))
                                })
                                .when(!is_active, |el| {
                                    el.bg(cx.theme().background)
                                })
                                .rounded(px(3.0))
                                .cursor_pointer()
                                .text_color(cx.theme().foreground)
                                .when(!is_active, |el| {
                                    el.hover(|s| s.bg(cx.theme().foreground.opacity(0.05)))
                                })
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |this, _, _, cx| {
                                        this.set_active_layer(id, cx);
                                    }),
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
                        })),
                )
            })
            .when(self.has_document && layer_data.is_empty(), |this| {
                this.child(
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
                                .child(t!("layers.no_layers")),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().foreground.opacity(0.3))
                                .child(t!("layers.open_or_add")),
                        ),
                )
            })
            .when(!self.has_document, |this| {
                this.child(
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
                                .child(t!("layers.no_document")),
                        ),
                )
            })
    }
}
