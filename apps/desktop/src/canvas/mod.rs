//! Canvas — displays the current document's rendered image.

use std::sync::Arc;

use gpui::*;
use gpui_component::{ActiveTheme as _, StyledExt as _};

use kaleido_core::Document;
use crate::GlobalKaleidoApp;
use kaleido_services::app::KaleidoApp;

/// Canvas view — renders the current document.
pub struct Canvas {
    focus_handle: FocusHandle,
    document_info: Option<DocumentInfo>,
}

/// Information about the current document for display.
#[derive(Debug, Clone)]
struct DocumentInfo {
    width: u32,
    height: u32,
    layer_count: usize,
    has_document: bool,
}

impl Canvas {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();

        Self {
            focus_handle,
            document_info: None,
        }
    }

    /// Reads the current document info from the service layer.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(app) = cx.try_global::<GlobalKaleidoApp>() {
            let data = app.data_service();
            self.document_info = Some(DocumentInfo {
                width: data.size().map(|s| s.width).unwrap_or(0),
                height: data.size().map(|s| s.height).unwrap_or(0),
                layer_count: app.layer_service().layer_count().unwrap_or(0),
                has_document: data.has_document(),
            });
        }
    }
}

impl EventEmitter<CanvasEvent> for Canvas {}

impl Focusable for Canvas {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Canvas {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.refresh(cx);

        let info = self.document_info.clone();

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("Canvas"),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().foreground.opacity(0.6))
                            .child(match &info {
                                Some(i) if i.has_document => {
                                    format!("{} × {} pixels · {} nodes", i.width, i.height, i.layer_count)
                                }
                                _ => "No document open — use Ctrl+O to open a file".into(),
                            }),
                    ),
            )
    }
}

/// Events emitted by the Canvas.
#[derive(Debug, Clone)]
pub enum CanvasEvent {
    DocumentChanged,
}
