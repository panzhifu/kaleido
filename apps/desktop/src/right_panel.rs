//! Right-side panel area — renders plugin-supplied panels.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::*;
use gpui_component::{ActiveTheme as _, h_flex, v_flex};

use kaleido_traits::{Panel, PanelContext, PanelElement, PanelSection};
use gpui_component::Theme;

use crate::state::AppState;

/// Maximum number of panels we render at once.
const MAX_PANELS: usize = 4;

pub struct RightPanel {
    app_state: Entity<AppState>,
    /// The currently displayed panels (from plugins).
    panels: Vec<Arc<std::sync::Mutex<dyn Panel>>>,
    /// Cached UI state for interactive elements.
    panel_values: HashMap<String, serde_json::Value>,
}

impl RightPanel {
    pub fn new(app_state: Entity<AppState>, _cx: &mut Context<Self>) -> Self {
        Self {
            app_state,
            panels: Vec::new(),
            panel_values: HashMap::new(),
        }
    }

    /// Updates the panels to display.
    pub fn set_panels(&mut self, panels: Vec<Arc<std::sync::Mutex<dyn Panel>>>) {
        self.panels = panels;
    }
}

impl Render for RightPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Collect panel sections from all registered panels.
        let mut all_sections: Vec<PanelSection> = Vec::new();
        for panel in self.panels.iter().take(MAX_PANELS) {
            let mut builder = PanelBuilder { sections: Vec::new() };
            if let Ok(mut p) = panel.lock() {
                p.render(&mut builder);
            }
            all_sections.extend(builder.sections);
        }

        let theme = cx.theme();

        v_flex()
            .w(px(240.))
            .h_full()
            .bg(theme.sidebar)
            // Plugin panels (if any)
            .children(all_sections.iter().map(|section| {
                render_section(section, theme)
            }))
            // Default sections
            .child(div().h(px(1.)).bg(theme.border))
            .child(
                v_flex()
                    .p(px(8.))
                    .gap(px(4.))
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.foreground)
                            .child("属性"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("选择一个工具或对象"),
                    ),
            )
            .child(div().h(px(1.)).bg(theme.border))
            .child(
                v_flex()
                    .p(px(8.))
                    .gap(px(4.))
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.foreground)
                            .child("图层"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("没有图层"),
                    ),
            )
            .child(div().h(px(1.)).bg(theme.border))
            .child(
                v_flex()
                    .p(px(8.))
                    .gap(px(4.))
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.foreground)
                            .child("颜色"),
                    )
                    .child(
                        h_flex()
                            .gap(px(4.))
                            .child(
                                div()
                                    .w(px(24.))
                                    .h(px(24.))
                                    .bg(gpui::rgb(0x000000))
                                    .border_color(theme.border),
                            )
                            .child(
                                div()
                                    .w(px(24.))
                                    .h(px(24.))
                                    .bg(gpui::rgb(0xffffff))
                                    .border_color(theme.border),
                            ),
                    ),
            )
    }
}

/// Renders a panel section.
fn render_section(section: &PanelSection, theme: &Theme) -> AnyElement {
    v_flex()
        .gap(px(2.))
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(section.title.clone()),
        )
        .children(section.children.iter().map(|el| render_element(el, theme)))
        .into_any_element()
}

/// Renders a panel element.
fn render_element(el: &PanelElement, theme: &Theme) -> AnyElement {
    match el {
        PanelElement::Label { text } => div()
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(text.clone())
            .into_any_element(),

        PanelElement::Heading { text } => div()
            .text_sm()
            .text_color(theme.foreground)
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .child(text.clone())
            .into_any_element(),

        PanelElement::Divider => div()
            .h(px(1.))
            .w_full()
            .bg(theme.border)
            .my(px(4.))
            .into_any_element(),

        PanelElement::NumberInput {
            label,
            value,
            min,
            max,
            step,
            id,
        } => {
            let _ = (min, max, step, id);
            v_flex()
                .gap(px(2.))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(label.clone()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.foreground)
                        .child(format!("{value:.0}")),
                )
                .into_any_element()
        }

        PanelElement::Checkbox { label, checked, id } => {
            let _ = (id);
            h_flex()
                .gap(px(4.))
                .items_center()
                .child(
                    div()
                        .w(px(12.))
                        .h(px(12.))
                        .border_color(theme.border)
                        .bg(if *checked {
                            theme.accent
                        } else {
                            theme.transparent
                        }),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.foreground)
                        .child(label.clone()),
                )
                .into_any_element()
        }

        PanelElement::Dropdown {
            label,
            options,
            selected,
            id,
        } => {
            let _ = (id);
            v_flex()
                .gap(px(2.))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(label.clone()),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(theme.foreground)
                        .child(
                            options
                                .get(*selected)
                                .map(|s| s.as_str())
                                .unwrap_or("—")
                                .to_string(),
                        ),
                )
                .into_any_element()
        }

        PanelElement::ColorPicker { label, value, id } => {
            let _ = (id);
            h_flex()
                .gap(px(4.))
                .items_center()
                .child(
                    div()
                        .w(px(20.))
                        .h(px(20.))
                        .border_color(theme.border)
                        .bg(gpui::rgb(hex_to_u32(value))),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(format!("{label}: {value}")),
                )
                .into_any_element()
        }

        PanelElement::ButtonRow { buttons } => h_flex()
            .gap(px(4.))
            .flex_wrap()
            .children(buttons.iter().map(|btn| {
                gpui_component::button::Button::new(btn.id.clone())
                    .label(btn.label.clone())
            }))
            .into_any_element(),

        PanelElement::Canvas { width, height, pixels } => {
            if pixels.len() >= (*width * *height * 4) as usize {
                if let Some(render_img) = render_pixels_to_image(*width, *height, pixels) {
                    return div()
                        .child(
                            img(ImageSource::Render(render_img))
                                .w(px(*width as f32))
                                .h(px(*height as f32)),
                        )
                        .into_any_element();
                }
            }
            div()
                .w(px(*width as f32))
                .h(px(*height as f32))
                .bg(theme.border)
                .into_any_element()
        }

        PanelElement::Progress { value, label } => {
            let pct = value.unwrap_or(0.0).clamp(0.0, 1.0);
            v_flex()
                .gap(px(2.))
                .children(label.as_ref().map(|l| {
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(l.clone())
                        .into_any_element()
                }))
                .child(
                    div()
                        .h(px(4.))
                        .w_full()
                        .bg(theme.border)
                        .child(
                            div()
                                .h_full()
                                .w(px((pct * 240.0).max(0.0) as f32))
                                .bg(theme.accent),
                        ),
                )
                .into_any_element()
        }

        PanelElement::Section(section) => render_section(section, theme).into_any_element(),
    }
}

/// Helper: builds panel content from `Panel` trait calls.
struct PanelBuilder {
    sections: Vec<PanelSection>,
}

impl PanelContext for PanelBuilder {
    fn add_section(&mut self, section: PanelSection) {
        self.sections.push(section);
    }

    fn clear(&mut self) {
        self.sections.clear();
    }
}

/// Helper: converts a hex colour string to a u32 for `gpui::rgb()`.
fn hex_to_u32(hex: &str) -> u32 {
    let hex = hex.trim_start_matches('#');
    if hex.len() >= 6 {
        u32::from_str_radix(&hex[0..6], 16).unwrap_or(0)
    } else {
        0x000000
    }
}

/// Helper: renders raw RGBA pixels to a GPUI RenderImage.
fn render_pixels_to_image(width: u32, height: u32, pixels: &[u8]) -> Option<std::sync::Arc<RenderImage>> {
    use image::{ImageBuffer, Frame, Rgba};
    use smallvec::SmallVec;

    if pixels.len() < (width * height * 4) as usize {
        return None;
    }
    let image_buffer: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(width, height, pixels.to_vec())?;
    let frame = Frame::new(image_buffer);
    let render_image = RenderImage::new(SmallVec::from_elem(frame, 1));
    Some(std::sync::Arc::new(render_image))
}
