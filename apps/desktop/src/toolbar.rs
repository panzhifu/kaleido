//! Vertical toolbar showing tools loaded from the plugin [`ToolRegistry`].
//!
//! Clicking a tool runs it against the current image through the
//! [`ImageStore`] single write path, then records a dirty-tile snapshot
//! into the [`HistoryKeeper`] so the change is undoable. The canvas and
//! status bar are notified to refresh.

use std::sync::Arc;

use gpui::*;
use gpui_component::{ActiveTheme as _, v_flex};
use gpui_component::button::{Button, ButtonVariants};

use kaleido_services::tile_history::TileSnapshotCommand;
use kaleido_traits::{HistoryKeeper, ImageStore, LayerStore, LayerToolContext, ToolRegistry};

use crate::canvas::Canvas;
use crate::state::AppState;
use crate::status_bar::StatusBar;

pub struct Toolbar {
    app_state: Entity<AppState>,
    registry: Arc<dyn ToolRegistry>,
    store: Arc<dyn ImageStore>,
    keeper: Arc<dyn HistoryKeeper>,
    layer_store: Arc<dyn LayerStore>,
    canvas: Entity<Canvas>,
    status_bar: Entity<StatusBar>,
}

impl Toolbar {
    pub fn new(
        app_state: Entity<AppState>,
        registry: Arc<dyn ToolRegistry>,
        store: Arc<dyn ImageStore>,
        keeper: Arc<dyn HistoryKeeper>,
        layer_store: Arc<dyn LayerStore>,
        canvas: Entity<Canvas>,
        status_bar: Entity<StatusBar>,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            app_state,
            registry,
            store,
            keeper,
            layer_store,
            canvas,
            status_bar,
        }
    }
}

/// Host-side [`LayerToolContext`]: hands the tool the document's layer store.
struct LayerToolContextImpl {
    layer_store: Arc<dyn LayerStore>,
}

impl LayerToolContext for LayerToolContextImpl {
    fn layer_store(&self) -> &dyn LayerStore {
        self.layer_store.as_ref()
    }

    fn document_width(&self) -> u32 {
        self.layer_store.document_size().0
    }

    fn document_height(&self) -> u32 {
        self.layer_store.document_size().1
    }
}

impl Render for Toolbar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let selected = self.app_state.read(cx).selected_tool.clone();
        let tools = self.registry.tools();

        let store = self.store.clone();
        let keeper = self.keeper.clone();
        let layer_store = self.layer_store.clone();
        let canvas = self.canvas.clone();
        let status_bar = self.status_bar.clone();
        let app_state = self.app_state.clone();

        let buttons = tools.iter().map(move |tool| {
            let name = tool.name().to_string();
            let is_selected = selected.as_deref() == Some(name.as_str());

            let store = store.clone();
            let keeper = keeper.clone();
            let layer_store = layer_store.clone();
            let canvas = canvas.clone();
            let status_bar = status_bar.clone();
            let app_state = app_state.clone();
            let tool = tool.clone();

            let mut button = Button::new(format!("tool-{name}")).label(tool.menu_path());
            if is_selected {
                button = button.primary();
            } else {
                button = button.ghost();
            }
            button = button.on_click(move |_event, _window, cx| {
                app_state.update(cx, |state, _cx| {
                    state.selected_tool = Some(name.clone());
                });

                let mut params = tool.schema().apply_defaults(&serde_json::json!({}));
                if tool.name() == "brightness" {
                    params["value"] = serde_json::json!(40);
                }

                // Layer-document tools operate on the whole layer stack via
                // the LayerStore; the store re-composites and refreshes the
                // canvas itself.
                if tool.supports_layers() {
                    let mut ctx = LayerToolContextImpl {
                        layer_store: layer_store.clone(),
                    };
                    if let Err(err) = tool.apply_to_document(&mut ctx, &params) {
                        eprintln!("图层工具执行失败 {}: {err}", tool.name());
                    }
                    canvas.update(cx, |_c, cx| cx.notify());
                    status_bar.update(cx, |_s, cx| cx.notify());
                    return;
                }

                // Single-image tool: run through the write path and record
                // a dirty-tile undo snapshot.
                let Ok(Some(before)) = store.get_image() else {
                    return;
                };

                // `apply_mutation` needs a 'static closure, so move both the
                // tool handle and the params into it (the outer `tool` is
                // still needed below for the undo label).
                let apply_tool = tool.clone();
                if store
                    .apply_mutation(Box::new(move |image| apply_tool.apply(image, &params)))
                    .is_err()
                {
                    return;
                }

                let Ok(Some(after)) = store.get_image() else {
                    return;
                };

                let command = TileSnapshotCommand::from_diff(
                    &before,
                    &after,
                    tool.name(),
                    tool.description(),
                );
                let _ = keeper.push(Box::new(command));

                canvas.update(cx, |_c, cx| cx.notify());
                status_bar.update(cx, |_s, cx| cx.notify());
            });
            button
        });

        let hint = if tools.is_empty() {
            "无插件工具".to_string()
        } else {
            String::new()
        };

        v_flex()
            .bg(cx.theme().sidebar)
            .w(px(48.))
            .h_full()
            .p(px(4.))
            .gap(px(2.))
            .children(buttons)
            .child(
                div()
                    .w(px(40.))
                    .text_size(px(9.))
                    .text_color(cx.theme().muted_foreground)
                    .child(hint),
            )
    }
}
