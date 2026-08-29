use anyhow::Result;
use gpui::{
    App, AppContext, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions, div, img,
    px, rgb, size,
};
use gpui::prelude::*;
use gpui_platform::application;
use kaleido_core::{Image, TiledImage};
use kaleido_services::app::{AppConfig, KaleidoApp};
use kaleido_services::async_io::{AsyncImageLoader, BackgroundSaver, LoadPriority};
use kaleido_services::canvas::CanvasService;
use kaleido_services::layer::LayerStack;
use kaleido_tool_brightness::{BrightnessToolConfig, brightness_tool_plugin};
use kaleido_tool_invert::invert_tool_plugin;
use kaleido_traits::Tool;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;

/// Kaleido desktop host.
///
/// The host owns the window, the canvas and the Cordis-managed service
/// container. **Every user-facing command is a plugin**: the toolbar is
/// built dynamically from the `ToolRegistry`, so installing/uninstalling a
/// plugin adds/removes buttons without touching this code.
struct KaleidoEditor {
    app: KaleidoApp,
    /// Canvas service for viewport management (zoom/pan/rotate).
    canvas: CanvasService,
    /// Layer stack for layer management.
    layers: LayerStack,
    /// Async image loader.
    loader: AsyncImageLoader,
    /// Background saver.
    saver: BackgroundSaver,
    /// Path of the image currently shown on the canvas.
    image_path: Option<PathBuf>,
    /// Current zoom level for display.
    zoom_text: String,
}

impl KaleidoEditor {
    fn new(_cx: &mut Context<Self>) -> Self {
        let app = KaleidoApp::boot(AppConfig::default()).expect("failed to boot Kaleido");

        // Install the tool plugins — the toolbar below is generated from
        // whatever tools are registered, not hard-coded.
        app.context()
            .plugin(brightness_tool_plugin(), BrightnessToolConfig::default());
        app.context().plugin(invert_tool_plugin(), ());

        let file_codec = app.file_codec_registry();
        let loader = AsyncImageLoader::new(file_codec);
        let saver = BackgroundSaver::new();

        let image_path = std::env::args().nth(1).map(PathBuf::from);

        Self {
            app,
            canvas: CanvasService::new(0, 0, 900, 600),
            layers: LayerStack::new(0, 0),
            loader,
            saver,
            image_path,
            zoom_text: "100%".to_string(),
        }
    }

    /// Runs a tool on the current image.
    fn run_tool(&mut self, tool: &Arc<dyn Tool>, params: serde_json::Value) {
        let Some(path) = self.image_path.clone() else { return };

        // Get the current image from the layer stack or load it.
        let mut image = match self.get_current_image() {
            Some(img) => img,
            None => return,
        };

        // Apply the tool.
        if tool.apply(&mut image, &params).is_err() {
            return;
        }

        // Update the layer with the new image.
        self.update_current_layer(image.clone());

        // Save in background (convert Image to TiledImage).
        if let Ok(tiled) = TiledImage::from_packed(&image) {
            self.saver.save(tiled, path, kaleido_traits::ImageFormat::Png, self.app.file_codec_registry());
        }
    }

    /// Gets the current image (composite of all layers or the single layer).
    fn get_current_image(&self) -> Option<Image> {
        // For now, get the background layer's image.
        let bg = self.layers.background()?;
        let tiled = match &bg.content {
            kaleido_services::layer::LayerContent::Pixels(img) => img,
            _ => return None,
        };
        tiled.to_packed().ok()
    }

    /// Updates the current layer with a new image.
    fn update_current_layer(&mut self, image: Image) {
        if let Ok(tiled) = TiledImage::from_packed(&image) {
            self.layers = LayerStack::with_background(image.width(), image.height(), tiled);
            self.canvas.set_image_size(image.width(), image.height());
        }
    }

    /// Opens an image file asynchronously.
    fn open_file(&mut self, path: PathBuf) {
        // Validate the file can be loaded.
        if self.app.file_codec_registry().load(&path).is_ok() {
            self.image_path = Some(path.clone());

            // Load the image and create a layer.
            let priority = LoadPriority::VisibleFirst(
                kaleido_services::async_io::Rect { x: 0, y: 0, width: 900, height: 600 }
            );
            let path_for_loader = path.clone();
            let _request_id = self.loader.load(path_for_loader, priority);

            // For now, load synchronously (async loading would need a callback).
            if let Ok(image) = self.app.file_codec_registry().load(&path) {
                let tiled = TiledImage::from_packed(&image).ok();
                if let Some(tiled) = tiled {
                    self.layers = LayerStack::with_background(image.width(), image.height(), tiled);
                    self.canvas.set_image_size(image.width(), image.height());
                    self.canvas.fit_to_screen();
                    self.update_zoom_text();
                }
            }
        }
    }

    /// Zooms in.
    fn zoom_in(&mut self) {
        let factor = 1.25;
        let center_x = self.canvas.viewport().offset_x;
        let center_y = self.canvas.viewport().offset_y;
        self.canvas.viewport_mut().zoom_at(center_x, center_y, factor);
        self.update_zoom_text();
    }

    /// Zooms out.
    fn zoom_out(&mut self) {
        let factor = 0.8;
        let center_x = self.canvas.viewport().offset_x;
        let center_y = self.canvas.viewport().offset_y;
        self.canvas.viewport_mut().zoom_at(center_x, center_y, factor);
        self.update_zoom_text();
    }

    /// Resets zoom to fit.
    fn zoom_fit(&mut self) {
        self.canvas.fit_to_screen();
        self.update_zoom_text();
    }

    /// Updates the zoom text display.
    fn update_zoom_text(&mut self) {
        let zoom = self.canvas.viewport().zoom;
        self.zoom_text = format!("{:.0}%", zoom * 100.0);
    }
}

impl Render for KaleidoEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // ── Toolbar: generated from the plugin registry ────────────────
        let tools = self.app.tool_registry().tools();
        let toolbar = div()
            .flex()
            .items_center()
            .gap_2()
            .p_2()
            .bg(rgb(0x1e293b))
            .child(
                div()
                    .id("open")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(0x64748b))
                    .text_color(rgb(0xffffff))
                    .on_click(cx.listener(|_this, _, _, cx| {
                        let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
                            files: true,
                            directories: false,
                            multiple: false,
                            prompt: Some("打开图片".into()),
                        });
                        cx.spawn(async move |this, cx| {
                            if let Ok(Ok(Some(paths))) = rx.await
                                && let Some(path) = paths.into_iter().next() {
                                    this.update(cx, |this, cx| {
                                        this.open_file(path);
                                        cx.notify();
                                    })
                                    .ok();
                                }
                        })
                        .detach();
                    }))
                    .child("打开"),
            )
            // Zoom controls
            .child(
                div()
                    .id("zoom_out")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(0x64748b))
                    .text_color(rgb(0xffffff))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.zoom_out();
                        cx.notify();
                    }))
                    .child("−"),
            )
            .child(
                div()
                    .id("zoom_text")
                    .px_3()
                    .py_1()
                    .text_color(rgb(0xffffff))
                    .child(self.zoom_text.clone()),
            )
            .child(
                div()
                    .id("zoom_in")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(0x64748b))
                    .text_color(rgb(0xffffff))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.zoom_in();
                        cx.notify();
                    }))
                    .child("+"),
            )
            .child(
                div()
                    .id("zoom_fit")
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(0x64748b))
                    .text_color(rgb(0xffffff))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.zoom_fit();
                        cx.notify();
                    }))
                    .child("适应"),
            )
            .child(
                div()
                    .w(px(1.))
                    .h(px(20.))
                    .bg(rgb(0x475569)),
            )
            // Tool buttons
            .children(tools.into_iter().map(|tool| {
                let name = tool.name().to_string();
                div()
                    .id(SharedString::from(name.clone()))
                    .px_3()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(0x3b82f6))
                    .text_color(rgb(0xffffff))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.run_tool(&tool, json!({}));
                        cx.notify();
                    }))
                    .child(name)
            }));

        // ── Canvas ──────────────────────────────────────────────────────
        let canvas = match &self.image_path {
            Some(path) => div().size_full().child(
                img(path.clone())
                    .size_full()
                    .object_fit(gpui::ObjectFit::Contain),
            ),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(0x94a3b8))
                .child("打开一张图片开始编辑"),
        };

        div().flex().flex_col().size_full().child(toolbar).child(canvas)
    }
}

fn main() -> Result<()> {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(900.), px(600.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                focus: true,
                ..Default::default()
            },
            |_, cx| cx.new(KaleidoEditor::new),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
    Ok(())
}
