//! Dock workspace — main layout definition.

use gpui::*;
use gpui_component::{ActiveTheme as _, StyledExt as _, dock::{
    DockArea, DockAreaState, DockEvent, DockLayout, DockSkin, PanelEvent, Panel, panel_handle,
}};
use gpui_base::dock::Panel as BasePanel;
use std::rc::Rc;



/// Main dock area identifier.
const MAIN_DOCK_AREA: &str = "main-dock";
const DOCK_VERSION: usize = 1;

/// State file for persisting dock layout.
#[cfg(debug_assertions)]
const STATE_FILE: &str = "target/dock-layout.json";
#[cfg(not(debug_assertions))]
const STATE_FILE: &str = "dock-layout.json";

/// Creates the dock area with default layout.
pub fn create_dock_area(
    canvas: Entity<crate::canvas::Canvas>,
    window: &mut Window,
    cx: &mut App,
) -> (Entity<DockArea>, Rc<DockSkin>) {
    let (dock_area, skin) = DockSkin::dock_area(MAIN_DOCK_AREA, Some(DOCK_VERSION), window, cx);

    // Try to load saved layout, or use default.
    match load_layout(dock_area.clone(), window, cx) {
        Ok(_) => tracing::info!("dock layout loaded"),
        Err(_) => {
            tracing::info!("using default dock layout");
            set_default_layout(canvas, dock_area.clone(), window, cx);
        }
    }

    (dock_area, skin)
}

/// Sets the default dock layout.
fn set_default_layout(
    canvas: Entity<crate::canvas::Canvas>,
    dock_area: Entity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) {
    // Left panel placeholder
    let left = DockLayout::tabs().panel_view(
        panel_handle(cx.new(|cx| PlaceholderPanel::new("Tools", "左侧工具面板", cx))),
        cx,
    );

    // Right panel placeholder
    let right = DockLayout::tabs().panel_view(
        panel_handle(cx.new(|cx| PlaceholderPanel::new("Properties", "右侧属性面板", cx))),
        cx,
    );

    // Bottom panel placeholder
    let bottom = DockLayout::tabs().panel_view(
        panel_handle(cx.new(|cx| PlaceholderPanel::new("History", "底部历史面板", cx))),
        cx,
    );

    // Center panel — the canvas (wrapped in a dock-compatible panel)
    let canvas_panel = cx.new(|cx| CanvasPanel::new(canvas, cx));
    let center = DockLayout::tabs().panel_view(panel_handle(canvas_panel), cx);

    // Build layout: left | center | right
    let main_split = DockLayout::h_split()
        .child(left, Some(px(200.)))
        .child(center, None)
        .child(right, Some(px(250.)));

    // Build layout: main_split / bottom
    let full_layout = DockLayout::v_split()
        .child(main_split, None)
        .child(bottom, Some(px(150.)));

    dock_area.update(cx, |area, cx| {
        area.set_center(full_layout, window, cx);
    });
}

/// A dock-compatible panel that wraps the Canvas.
pub struct CanvasPanel {
    canvas: Entity<crate::canvas::Canvas>,
}

impl CanvasPanel {
    pub fn new(canvas: Entity<crate::canvas::Canvas>, cx: &mut Context<Self>) -> Self {
        Self { canvas }
    }
}

impl BasePanel for CanvasPanel {
    fn panel_name(&self) -> &'static str {
        "Canvas"
    }
}

impl Panel for CanvasPanel {}

impl EventEmitter<PanelEvent> for CanvasPanel {}

impl Focusable for CanvasPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.canvas.focus_handle(cx)
    }
}

impl Render for CanvasPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.canvas.clone().into_any_element()
    }
}

/// A simple placeholder panel for dock layout testing.
pub struct PlaceholderPanel {
    focus_handle: FocusHandle,
    title: String,
    subtitle: String,
}

impl PlaceholderPanel {
    pub fn new(title: &str, subtitle: &str, cx: &mut Context<Self>) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            title: title.to_string(),
            subtitle: subtitle.to_string(),
        }
    }
}

impl BasePanel for PlaceholderPanel {
    fn panel_name(&self) -> &'static str {
        match self.title.as_str() {
            "Tools" => "Tools",
            "Properties" => "Properties",
            "History" => "History",
            "Canvas" => "Canvas",
            _ => "Placeholder",
        }
    }
}

impl Panel for PlaceholderPanel {}

impl EventEmitter<PanelEvent> for PlaceholderPanel {}

impl Focusable for PlaceholderPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for PlaceholderPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                    .text_xs()
                    .font_weight(gpui::FontWeight::BOLD)
                    .child(self.title.clone()),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().foreground.opacity(0.5))
                    .child(self.subtitle.clone()),
            )
    }
}

/// Loads dock layout from file.
fn load_layout(
    dock_area: Entity<DockArea>,
    window: &mut Window,
    cx: &mut App,
) -> anyhow::Result<()> {
    let json = std::fs::read_to_string(STATE_FILE)?;
    let state: DockAreaState = serde_json::from_str(&json)?;
    dock_area.update(cx, |area, cx| {
        area.load(state, window, cx)?;
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}

/// Saves dock layout to file.
pub fn save_layout(dock_area: &Entity<DockArea>, cx: &mut App) -> anyhow::Result<()> {
    let state = dock_area.read(cx).dump(cx);
    let json = serde_json::to_string_pretty(&state)?;
    std::fs::write(STATE_FILE, json)?;
    Ok(())
}
