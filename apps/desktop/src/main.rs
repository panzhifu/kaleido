use anyhow::Result;
use gpui::{
    App, AppContext, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions, div, img,
    px, rgb, size,
};
use gpui::prelude::*;
use gpui_platform::application;
use kaleido_services::app::{AppConfig, KaleidoApp};
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
    /// Path of the image currently shown on the canvas.
    image_path: Option<PathBuf>,
}

impl KaleidoEditor {
    fn new(_cx: &mut Context<Self>) -> Self {
        let app = KaleidoApp::boot(AppConfig::default()).expect("failed to boot Kaleido");

        // Install the tool plugins — the toolbar below is generated from
        // whatever tools are registered, not hard-coded.
        app.context()
            .plugin(brightness_tool_plugin(), BrightnessToolConfig::default());
        app.context().plugin(invert_tool_plugin(), ());

        let image_path = std::env::args().nth(1).map(PathBuf::from);
        Self { app, image_path }
    }

    /// Runs a tool on the current image: load → apply → save back.
    fn run_tool(&mut self, tool: &Arc<dyn Tool>, params: serde_json::Value) {
        let Some(path) = self.image_path.clone() else { return };
        let codec = self.app.file_codec();
        let Ok(mut image) = codec.load(&path) else { return };
        if tool.apply(&mut image, &params).is_err() {
            return;
        }
        let _ = codec.save(&path, &image);
    }

    /// Opens an image file (validated through the codec).
    fn open_file(&mut self, path: PathBuf) {
        if self.app.file_codec().load(&path).is_ok() {
            self.image_path = Some(path);
        }
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
                            if let Ok(Ok(Some(paths))) = rx.await {
                                if let Some(path) = paths.into_iter().next() {
                                    this.update(cx, |this, cx| {
                                        this.open_file(path);
                                        cx.notify();
                                    })
                                    .ok();
                                }
                            }
                        })
                        .detach();
                    }))
                    .child("打开"),
            )
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
            |_, cx| cx.new(|cx| KaleidoEditor::new(cx)),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
    Ok(())
}
