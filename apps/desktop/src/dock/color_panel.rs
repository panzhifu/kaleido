//! Color panel — shows document properties and color information.

use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::base::dock::Panel as BasePanel;
use gpui_kit::component::{ActiveTheme as _, dock::PanelEvent};
use gpui_kit::component::dock::Panel;
use rust_i18n::t;

use crate::GlobalKaleidoApp;

/// Color panel — displays foreground color and document properties.
pub struct ColorPanel {
    focus_handle: FocusHandle,
    app: GlobalKaleidoApp,
    /// Document width.
    doc_width: u32,
    /// Document height.
    doc_height: u32,
    /// Number of layers.
    layer_count: usize,
    /// Whether a document is open.
    has_document: bool,
}

impl ColorPanel {
    pub fn new(app: GlobalKaleidoApp, cx: &mut Context<Self>) -> Self {
        let mut panel = Self {
            focus_handle: cx.focus_handle(),
            app,
            doc_width: 0,
            doc_height: 0,
            layer_count: 0,
            has_document: false,
        };
        panel.refresh();
        panel
    }

    /// Refreshes data from services.
    fn refresh(&mut self) {
        let data = self.app.data_service();
        match data.document() {
            Ok(Some(doc)) => {
                self.doc_width = doc.size.width;
                self.doc_height = doc.size.height;
                self.has_document = true;
            }
            Ok(None) => {
                self.has_document = false;
            }
            Err(e) => {
                tracing::warn!("failed to read document: {e}");
                self.has_document = false;
            }
        }
        let layers = self.app.layer_service();
        match layers.layer_count() {
            Ok(count) => self.layer_count = count,
            Err(e) => {
                tracing::warn!("failed to read layer count: {e}");
                self.layer_count = 0;
            }
        }
    }
}

impl BasePanel for ColorPanel {
    fn panel_name(&self) -> &'static str {
        "Color"
    }
}

impl Panel for ColorPanel {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl EventEmitter<PanelEvent> for ColorPanel {}

impl Focusable for ColorPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for ColorPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("color-panel")
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_2()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .track_focus(&self.focus_handle)
            // Foreground Color section
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(t!("color.foreground")),
            )
            // Color swatch — uses the theme primary token as a
            // placeholder until a real foreground-color service exists.
            .child(
                div()
                    .w_8()
                    .h_8()
                    .rounded(px(4.0))
                    .border_1()
                    .border_color(cx.theme().border)
                    .bg(cx.theme().primary),
            )
            // Divider (hairline — px is appropriate here)
            .child(
                div()
                    .w_full()
                    .h(px(1.0))
                    .bg(cx.theme().border)
                    .my_1(),
            )
            // Document Info section
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(t!("color.document_info")),
            )
            .when(self.has_document, |this| {
                this.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_0p5()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().foreground.opacity(0.7))
                                .child(format!(
                                    "{}: {} × {}",
                                    t!("color.dimensions"),
                                    self.doc_width,
                                    self.doc_height
                                )),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().foreground.opacity(0.7))
                                .child(format!(
                                    "{}: {}",
                                    t!("color.layer_count"),
                                    self.layer_count
                                )),
                        ),
                )
            })
            .when(!self.has_document, |this| {
                this.child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().foreground.opacity(0.5))
                        .child(t!("color.no_document")),
                )
            })
    }
}
