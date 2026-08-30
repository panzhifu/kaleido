//! Right-side panel area — renders plugin-supplied panels and the layer
//! list from the [`LayerStore`].
//!
//! Plugin panels are rebuilt from the [`PanelRegistry`] on every frame.
//! Interactive elements (sliders, checkboxes, dropdowns, colour swatches,
//! buttons) now wire their changes back through [`Panel::on_change`] /
//! [`Panel::on_button`], so plugins get real UI instead of static text.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::*;
use gpui_component::{ActiveTheme as _, h_flex, v_flex};
use gpui_component::button::{Button, ButtonVariants};

use kaleido_traits::{
    LayerId, LayerStore, Panel, PanelContext, PanelElement, PanelRegistry, PanelSection,
};
use gpui_component::Theme;

use crate::state::AppState;

/// Maximum number of panels we render at once.
const MAX_PANELS: usize = 4;
/// Number of steps for +/- steppers on NumberInput elements.
const STEP_SIZE: f64 = 1.0;

/// Preset colours cycled when clicking a `ColorPicker` swatch.
const COLOR_PRESETS: &[u32] = &[
    0x000000, 0xffffff, 0xe5484d, 0xf76b15, 0xffb224, 0x46a758, 0x30a46c, 0x0091ff, 0x6e56cf,
];

pub struct RightPanel {
    app_state: Entity<AppState>,
    /// Panel registry: plugin-supplied panels are read live on each frame.
    registry: Arc<dyn PanelRegistry>,
    /// Document layer store used to render the layer list.
    layer_store: Arc<dyn LayerStore>,
    /// Cached UI state for interactive elements (unused for now; kept for
    /// future two-way bindings).
    #[allow(dead_code)]
    panel_values: HashMap<String, serde_json::Value>,
}

impl RightPanel {
    pub fn new(
        app_state: Entity<AppState>,
        registry: Arc<dyn PanelRegistry>,
        layer_store: Arc<dyn LayerStore>,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            app_state,
            registry,
            layer_store,
            panel_values: HashMap::new(),
        }
    }
}

impl Render for RightPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let this = cx.entity();

        // Collect sections from each live plugin panel, remembering which
        // panel each section came from so callbacks can reach it.
        let panels: Vec<Arc<std::sync::Mutex<dyn Panel>>> = self
            .registry
            .panels()
            .into_iter()
            .take(MAX_PANELS)
            .collect();

        let mut panel_sections: Vec<(Arc<std::sync::Mutex<dyn Panel>>, PanelSection)> = Vec::new();
        for panel in &panels {
            let mut builder = PanelBuilder { sections: Vec::new() };
            if let Ok(mut p) = panel.lock() {
                p.render(&mut builder);
            }
            for section in builder.sections {
                panel_sections.push((panel.clone(), section));
            }
        }

        let layers = self.layer_store.layers();
        let active_layer = self.layer_store.active_layer();
        let layer_store = self.layer_store.clone();

        v_flex()
            .w(px(240.))
            .h_full()
            .bg(theme.sidebar)
            // Plugin panels (if any)
            .children(panel_sections.iter().map(|(panel, section)| {
                render_section(section, theme, panel, &this)
            }))
            .child(div().h(px(1.)).bg(theme.border))
            // ── 图层 ──
            .child(
                v_flex()
                    .p(px(8.))
                    .gap(px(4.))
                    .child(
                        h_flex()
                            .justify_between()
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(theme.foreground)
                                    .child("图层"),
                            )
                            .child(
                                {
                                    let add_store = layer_store.clone();
                                    let add_this = this.clone();
                                    Button::new("add-layer")
                                        .label("+")
                                        .on_click(move |_event, _window, cx| {
                                            let _ = add_store.add_pixel_layer("图层");
                                            RightPanel::refresh_after_change(&add_this, cx);
                                        })
                                },
                            ),
                    )
                    .children(layers.iter().map(|info| {
                        let is_active = Some(info.id) == active_layer;
                        let row_store = layer_store.clone();
                        let row_this = this.clone();
                        let id = info.id;
                        div()
                            .id(SharedString::from(format!("layer-{}", id.0)))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .px(px(4.))
                            .py(px(2.))
                            .rounded(px(4.))
                            .bg(if is_active {
                                theme.accent
                            } else {
                                theme.transparent
                            })
                            .text_color(if is_active {
                                theme.accent_foreground
                            } else {
                                theme.foreground
                            })
                            .on_click(move |_event, _window, cx| {
                                let _ = row_store.set_active_layer(id);
                                RightPanel::refresh_after_change(&row_this, cx);
                            })
                            .child(
                                div()
                                    .w(px(14.))
                                    .text_xs()
                                    .text_color(if is_active {
                                        theme.accent_foreground
                                    } else {
                                        theme.muted_foreground
                                    })
                                    .child(if info.visible { "●" } else { "○" }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .child(format!("{} ({:.0}%)", info.name, info.opacity * 100.0)),
                            )
                    })),
            )
            .child(div().h(px(1.)).bg(theme.border))
            // ── 颜色（宿主示例区）──
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

impl RightPanel {
    /// Notifies the panel entity so its (and the canvas's) views re-render
    /// after a layer / panel mutation.
    fn refresh_after_change(this: &Entity<RightPanel>, cx: &mut App) {
        let _ = this.update(cx, |_panel, cx| {
            cx.notify();
        });
    }
}

/// Renders a panel section.
fn render_section(
    section: &PanelSection,
    theme: &Theme,
    panel: &Arc<std::sync::Mutex<dyn Panel>>,
    this: &Entity<RightPanel>,
) -> AnyElement {
    v_flex()
        .gap(px(2.))
        .p(px(8.))
        .child(
            div()
                .text_xs()
                .text_color(theme.muted_foreground)
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .child(section.title.clone()),
        )
        .children(section.children.iter().map(|el| render_element(el, theme, panel, this)))
        .into_any_element()
}

/// Renders a panel element, wiring interactive controls back to the panel.
#[allow(clippy::only_used_in_recursion)]
fn render_element(
    el: &PanelElement,
    theme: &Theme,
    panel: &Arc<std::sync::Mutex<dyn Panel>>,
    this: &Entity<RightPanel>,
) -> AnyElement {
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
            let panel = panel.clone();
            let this = this.clone();
            let id = id.clone();
            let step = step.unwrap_or(STEP_SIZE);
            let min = *min;
            let max = *max;
            let value = *value;
            let label = label.clone();
            let theme = theme.clone();

            let dec = panel.clone();
            let dec_this = this.clone();
            let dec_id = id.clone();
            let inc = panel;
            let inc_this = this;
            let inc_id = id;

            v_flex()
                .gap(px(2.))
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(label.clone()),
                )
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(4.))
                        .child(
                            Button::new(format!("step-down-{dec_id}"))
                                .label("−")
                                .on_click(move |_event, _window, cx| {
                                    let new = value - step;
                                    let new = min.map_or(new, |m| new.max(m));
                                    if let Ok(mut p) = dec.lock() {
                                        p.on_change(&dec_id, serde_json::json!(new));
                                    }
                                    RightPanel::refresh_after_change(&dec_this, cx);
                                }),
                        )
                        .child(
                            div()
                                .w(px(48.))
                                .text_xs()
                                .text_color(theme.foreground)
                                .child(format!("{value:.1}")),
                        )
                        .child(
                            Button::new(format!("step-up-{inc_id}"))
                                .label("+")
                                .on_click(move |_event, _window, cx| {
                                    let new = value + step;
                                    let new = max.map_or(new, |m| new.min(m));
                                    if let Ok(mut p) = inc.lock() {
                                        p.on_change(&inc_id, serde_json::json!(new));
                                    }
                                    RightPanel::refresh_after_change(&inc_this, cx);
                                }),
                        ),
                )
                .into_any_element()
        }

        PanelElement::Checkbox { label, checked, id } => {
            let panel = panel.clone();
            let this = this.clone();
            let id = id.clone();
            let label = label.clone();
            let checked = *checked;
            h_flex()
                .id(SharedString::from(format!("checkbox-{id}")))
                .gap(px(4.))
                .items_center()
                .on_click(move |_event, _window, cx| {
                    if let Ok(mut p) = panel.lock() {
                        p.on_change(&id, serde_json::json!(!checked));
                    }
                    RightPanel::refresh_after_change(&this, cx);
                })
                .child(
                    div()
                        .w(px(12.))
                        .h(px(12.))
                        .border_color(theme.border)
                        .bg(if checked { theme.accent } else { theme.transparent }),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.foreground)
                        .child(label),
                )
                .into_any_element()
        }

        PanelElement::Dropdown {
            label,
            options,
            selected,
            id,
        } => {
            let panel = panel.clone();
            let this = this.clone();
            let id = id.clone();
            let label = label.clone();
            let options = options.clone();
            let selected = *selected;
            let display = options
                .get(selected)
                .cloned()
                .unwrap_or_else(|| "—".to_string());
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
                        .id(SharedString::from(format!("dropdown-{id}")))
                        .px(px(4.))
                        .py(px(2.))
                        .rounded(px(4.))
                        .bg(theme.sidebar)
                        .border_color(theme.border)
                        .text_sm()
                        .text_color(theme.foreground)
                        .on_click(move |_event, _window, cx| {
                            // Cycle to the next option on click.
                            let next = if options.is_empty() {
                                0
                            } else {
                                (selected + 1) % options.len()
                            };
                            if let Some(value) = options.get(next) {
                                if let Ok(mut p) = panel.lock() {
                                    p.on_change(&id, serde_json::json!(value));
                                }
                            }
                            RightPanel::refresh_after_change(&this, cx);
                        })
                        .child(display),
                )
                .into_any_element()
        }

        PanelElement::ColorPicker { label, value, id } => {
            let panel = panel.clone();
            let this = this.clone();
            let id = id.clone();
            let label = label.clone();
            let value = value.clone();
            let display = format!("{label}: {value}");
            h_flex()
                .gap(px(4.))
                .items_center()
                .child(
                    div()
                        .id(SharedString::from(format!("color-{id}")))
                        .w(px(20.))
                        .h(px(20.))
                        .border_color(theme.border)
                        .bg(gpui::rgb(hex_to_u32(&value)))
                        .on_click(move |_event, _window, cx| {
                            // Cycle through preset colours as a simple picker.
                            let current = hex_to_u32(&value);
                            let next = COLOR_PRESETS
                                .iter()
                                .position(|c| *c == current)
                                .map(|i| (i + 1) % COLOR_PRESETS.len())
                                .unwrap_or(0);
                            let rgb = COLOR_PRESETS[next];
                            let hex = format!("#{:06X}", rgb);
                            if let Ok(mut p) = panel.lock() {
                                p.on_change(&id, serde_json::json!(hex));
                            }
                            RightPanel::refresh_after_change(&this, cx);
                        }),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(display),
                )
                .into_any_element()
        }

        PanelElement::ButtonRow { buttons } => {
            let this = this.clone();
            h_flex()
                .gap(px(4.))
                .flex_wrap()
                .children(buttons.iter().map(|btn| {
                    let panel = panel.clone();
                    let this = this.clone();
                    let id = btn.id.clone();
                    Button::new(format!("panel-btn-{}", btn.id))
                        .label(btn.label.clone())
                        .on_click(move |_event, _window, cx| {
                            if let Ok(mut p) = panel.lock() {
                                p.on_button(&id);
                            }
                            RightPanel::refresh_after_change(&this, cx);
                        })
                }))
                .into_any_element()
        }

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

        PanelElement::Section(section) => {
            render_section(section, theme, panel, this).into_any_element()
        }
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
fn render_pixels_to_image(
    width: u32,
    height: u32,
    pixels: &[u8],
) -> Option<std::sync::Arc<RenderImage>> {
    use image::{Frame, ImageBuffer, Rgba};
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
