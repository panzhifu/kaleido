//! Canvas — displays the current document's rendered image and handles tool interaction.

use std::path::PathBuf;

use gpui::*;
use gpui_component::StyledExt as _;
use gpui_component::ActiveTheme as _;
use gpui_component::dock::Panel;
use gpui_base::dock::Panel as BasePanel;
use rust_i18n::t;

// Re-export PanelEvent so app.rs and menu/mod.rs can use it.
pub use gpui_component::dock::PanelEvent;

use crate::dock::ActiveTool;
use crate::GlobalKaleidoApp;



/// Canvas view — renders the current document and handles mouse interaction for tools.
pub struct Canvas {
    focus_handle: FocusHandle,
    /// Reference to the service layer.
    app: GlobalKaleidoApp,
    /// Current tool state.
    active_tool: Entity<ActiveTool>,
    /// Current image file path for display.
    image_path: Option<PathBuf>,
    /// Whether a document is currently loaded.
    has_document: bool,
    /// Current zoom level (1.0 = 100%).
    zoom: f32,
    /// Natural (1×) image dimensions in pixels (for zoom rendering).
    natural_size: Option<(f32, f32)>,
    /// Drag state: (start_x, start_y, current_x, current_y).
    drag_state: Option<(f32, f32, f32, f32)>,
    /// Whether a drag is in progress.
    is_dragging: bool,
    /// The layer being dragged.
    dragging_layer: Option<kaleido_core::NodeId>,
    /// Original transform of the layer before drag started.
    drag_original_transform: Option<kaleido_core::Transform2D>,
}

impl Canvas {
    pub fn new(app: GlobalKaleidoApp, active_tool: Entity<ActiveTool>, cx: &mut Context<Self>) -> Self {
        let mut canvas = Self {
            focus_handle: cx.focus_handle(),
            app,
            active_tool,
            image_path: None,
            has_document: false,
            zoom: 1.0,
            natural_size: None,
            drag_state: None,
            is_dragging: false,
            dragging_layer: None,
            drag_original_transform: None,
        };
        // Initial render if document is already open.
        canvas.refresh();
        canvas
    }

    /// Refreshes the image from the render service.
    pub(crate) fn refresh(&mut self) {
        let render = self.app.render_service();
        match render.render() {
            Ok(image) => {
                let w = image.width();
                let h = image.height();
                let pixels = image.to_rgba_vec();
                match Self::save_png(w, h, &pixels) {
                    Some(path) => {
                        self.image_path = Some(path);
                        self.natural_size = Some((w as f32, h as f32));
                        self.has_document = true;
                    }
                    None => {
                        self.image_path = None;
                        self.natural_size = None;
                        self.has_document = true;
                    }
                }
            }
            Err(_) => {
                self.image_path = None;
                self.natural_size = None;
                self.has_document = false;
            }
        }
    }

    // ── Zoom ────────────────────────────────────────────────────────

    /// Returns the current zoom level (1.0 = 100%).
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Sets the zoom level, clamped to [0.1, 10.0].
    pub fn set_zoom(&mut self, zoom: f32, cx: &mut Context<Self>) {
        self.zoom = zoom.clamp(0.1, 10.0);
        cx.notify();
    }

    /// Zooms in by 25%.
    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.set_zoom(self.zoom * 1.25, cx);
    }

    /// Zooms out by 20%.
    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.set_zoom(self.zoom * 0.8, cx);
    }

    /// Fits the image to the canvas area (resets to 1.0 for now;
    /// a full implementation would measure the container and image).
    pub fn fit_to_window(&mut self, cx: &mut Context<Self>) {
        self.set_zoom(1.0, cx);
    }

    /// Encodes RGBA pixel data as PNG and writes to a temp file.
    fn save_png(width: u32, height: u32, rgba: &[u8]) -> Option<PathBuf> {
        use image::{ImageBuffer, Rgba};

        let img = ImageBuffer::<Rgba<u8>, _>::from_raw(width, height, rgba.to_vec())?;
        let path = std::env::temp_dir().join("kaleido_canvas.png");
        img.save(&path).ok()?;
        Some(path)
    }

    /// Handles mouse down — starts a drag operation if a document is open.
    fn on_mouse_down(&mut self, event: &MouseDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.has_document {
            return;
        }
        self.is_dragging = true;
        let pos = event.position;
        self.drag_state = Some((pos.x.into(), pos.y.into(), pos.x.into(), pos.y.into()));
        // Remember the active layer and its original transform.
        let layers = self.app.layer_service();
        if let Some(layer_id) = layers.active_layer() {
            self.dragging_layer = Some(layer_id);
            self.drag_original_transform = self.get_layer_transform(layer_id);
        }
        cx.notify();
    }

    /// Handles mouse drag — applies the move tool transformation live.
    fn on_mouse_drag(&mut self, event: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_dragging {
            return;
        }
        if let Some((start_x, start_y, _, _)) = self.drag_state {
            let pos = event.position;
            self.drag_state = Some((start_x, start_y, pos.x.into(), pos.y.into()));
            // Apply live preview for the move tool.
            self.apply_move_live(start_x, start_y, pos.x.into(), pos.y.into(), cx);
        }
    }

    /// Handles mouse up — finalizes the drag operation.
    fn on_mouse_up(&mut self, event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_dragging {
            return;
        }
        self.is_dragging = false;
        if let Some((start_x, start_y, _, _)) = self.drag_state {
            let pos = event.position;
            self.apply_move_final(start_x, start_y, pos.x.into(), pos.y.into(), cx);
        }
        self.drag_state = None;
        self.dragging_layer = None;
        self.drag_original_transform = None;
        cx.notify();
    }

    /// Gets the current transform of a layer from the document.
    fn get_layer_transform(&self, layer_id: kaleido_core::NodeId) -> Option<kaleido_core::Transform2D> {
        let data = self.app.data_service();
        data.document().ok()?.and_then(|doc| {
            doc.scene.node(layer_id).map(|node| node.transform)
        })
    }

    /// Applies a live preview of the move (no history recorded).
    fn apply_move_live(&mut self, start_x: f32, start_y: f32, current_x: f32, current_y: f32, cx: &mut Context<Self>) {
        let tool = self.active_tool.read(cx);
        if tool.current().name() != "move" {
            return;
        }

        let dx = current_x - start_x;
        let dy = current_y - start_y;

        if let (Some(layer_id), Some(orig)) = (self.dragging_layer, self.drag_original_transform) {
            let layers = self.app.layer_service();
            let _ = layers.set_transform(
                layer_id,
                kaleido_core::Transform2D {
                    tx: orig.tx + dx,
                    ty: orig.ty + dy,
                    rotation: orig.rotation,
                    sx: orig.sx,
                    sy: orig.sy,
                },
            );
            // Refresh the canvas to show the live preview.
            self.refresh();
            cx.notify();
        }
    }

    /// Finalizes the move operation.
    fn apply_move_final(&mut self, start_x: f32, start_y: f32, current_x: f32, current_y: f32, cx: &mut Context<Self>) {
        let tool = self.active_tool.read(cx);
        if tool.current().name() != "move" {
            return;
        }

        let dx = current_x - start_x;
        let dy = current_y - start_y;

        // If no actual movement, skip.
        if dx.abs() < 1.0 && dy.abs() < 1.0 {
            return;
        }

        if let (Some(layer_id), Some(orig)) = (self.dragging_layer, self.drag_original_transform) {
            let layers = self.app.layer_service();
            let _ = layers.set_transform(
                layer_id,
                kaleido_core::Transform2D {
                    tx: orig.tx + dx,
                    ty: orig.ty + dy,
                    rotation: orig.rotation,
                    sx: orig.sx,
                    sy: orig.sy,
                },
            );
            // Refresh the canvas.
            self.refresh();
            cx.notify();
        }
    }
}

impl EventEmitter<PanelEvent> for Canvas {}

impl BasePanel for Canvas {
    fn panel_name(&self) -> &'static str {
        "Canvas"
    }
}

impl Panel for Canvas {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

impl Focusable for Canvas {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Canvas {
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
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_drag))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .child(if let Some(path) = &self.image_path {
                let (base_w, base_h) = self.natural_size.unwrap_or((64.0, 64.0));
                let zoom = self.zoom;
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        img(path.clone())
                            .w(px(base_w * zoom))
                            .h(px(base_h * zoom))
                            .object_fit(gpui::ObjectFit::Contain),
                    )
                    .into_any_element()
            } else if self.has_document {
                div()
                    .text_sm()
                    .text_color(cx.theme().foreground.opacity(0.5))
                    .child(t!("canvas.rendering"))
                    .into_any_element()
            } else {
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
                            .text_color(cx.theme().foreground.opacity(0.5))
                            .child(t!("canvas.no_document")),
                    )
                    .into_any_element()
            })
    }
}


