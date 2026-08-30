//! Main application structure with hybrid state management.

use gpui::*;
#[allow(unused_imports)]
use gpui::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;

use kaleido_core::TiledImage;
use kaleido_services::app::{AppConfig, KaleidoApp};
use kaleido_services::async_io::{AsyncImageLoader, BackgroundSaver};
use kaleido_tool_brightness::{BrightnessToolConfig, brightness_tool_plugin};
use kaleido_tool_invert::invert_tool_plugin;
use kaleido_traits::Tool;

use crate::panels::*;
use crate::state::*;
use crate::theme::color;

pub struct KaleidoEditor {
    app: KaleidoApp,
    canvas: Entity<CanvasPanel>,
    layers: Entity<LayersPanel>,
    tool_params: Entity<ToolParamsPanel>,
    history: Entity<HistoryPanel>,
    viewport: ViewportState,
    layers_state: LayersState,
    tools_state: ToolsState,
    #[allow(dead_code)]
    loader: AsyncImageLoader,
    #[allow(dead_code)]
    saver: BackgroundSaver,
    image_path: Option<PathBuf>,
    status: String,
}

impl KaleidoEditor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let app = KaleidoApp::boot(AppConfig::default()).expect("failed to boot Kaleido");
        app.context().plugin(brightness_tool_plugin(), BrightnessToolConfig::default());
        app.context().plugin(invert_tool_plugin(), ());

        let canvas = cx.new(CanvasPanel::new);
        let layers = cx.new(LayersPanel::new);
        let registry = app.tool_registry();
        let tool_params = cx.new(|cx| ToolParamsPanel::new(registry, cx));
        let history = cx.new(HistoryPanel::new);

        let file_codec = app.file_codec_registry();
        let loader = AsyncImageLoader::new(file_codec);
        let saver = BackgroundSaver::new();

        Self {
            app, canvas, layers, tool_params, history,
            viewport: ViewportState::default(),
            layers_state: LayersState::default(),
            tools_state: ToolsState::default(),
            loader, saver,
            image_path: None,
            status: "就绪".to_string(),
        }
    }

    pub fn open_file(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if let Ok(tiled) = self.app.file_codec_registry().load(&path) {
            self.image_path = Some(path.clone());
            let w = tiled.width();
            let h = tiled.height();
            let stack = kaleido_services::layer::LayerStack::with_background(
                w, h, tiled,
            );
            self.layers_state.set_stack(stack);
            self.canvas.update(cx, |canvas, cx| {
                canvas.load_image(path.clone(), w, h, cx);
            });
            self.viewport.set_image_size(w, h);
            self.viewport.fit_to_screen(900.0, 600.0);
            self.history.update(cx, |history, cx| {
                history.add_entry("打开".to_string(), format!("{}", path.display()), cx);
            });
            self.status = format!("已打开: {}", path.display());
            cx.notify();
        }
    }

    pub fn get_current_image(&self) -> Option<TiledImage> {
        let bg = self.layers_state.stack.background()?;
        match &bg.content {
            kaleido_services::layer::LayerContent::Pixels(img) => Some(img.clone()),
            _ => None,
        }
    }

    pub fn apply_tool(&mut self, tool_name: &str, cx: &mut Context<Self>) {
        let params = self.tools_state.params.clone();
        if let Some(tool) = self.app.tool_registry().get(tool_name) {
            if let Some(mut image) = self.get_current_image() {
                if tool.apply(&mut image, &params).is_ok() {
                    let w = image.width();
                    let h = image.height();
                    let stack = kaleido_services::layer::LayerStack::with_background(w, h, image.clone());
                    self.layers_state.set_stack(stack);
                    self.canvas.update(cx, |canvas, cx| {
                        if let Some(path) = &self.image_path {
                            canvas.load_image(path.clone(), w, h, cx);
                        }
                    });
                    self.history.update(cx, |history, cx| {
                        history.add_entry(tool_name.to_string(), "应用".to_string(), cx);
                    });
                    if let Some(path) = self.image_path.clone() {
                        self.saver.save(image, path, kaleido_traits::ImageFormat::Png, self.app.file_codec_registry());
                    }
                    self.status = format!("已应用 '{}'", tool_name);
                    cx.notify();
                }
            }
        }
    }

    pub fn run_tool(&mut self, tool: &Arc<dyn Tool>, cx: &mut Context<Self>) {
        let name = tool.name().to_string();
        self.tools_state.select(&name);
        self.tool_params.update(cx, |panel, cx| panel.set_tool(&name, cx));
        self.apply_tool(&name, cx);
    }

    pub fn zoom_in(&mut self, cx: &mut Context<Self>) { self.canvas.update(cx, |canvas, cx| canvas.zoom_in(cx)); }
    pub fn zoom_out(&mut self, cx: &mut Context<Self>) { self.canvas.update(cx, |canvas, cx| canvas.zoom_out(cx)); }
    pub fn zoom_fit(&mut self, cx: &mut Context<Self>) { self.canvas.update(cx, |canvas, cx| canvas.zoom_fit(900.0, 600.0, cx)); }
    pub fn zoom_reset(&mut self, cx: &mut Context<Self>) { self.canvas.update(cx, |canvas, cx| canvas.zoom_reset(cx)); }
}

impl Render for KaleidoEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tools = self.app.tool_registry().tools();
        let zoom_text = self.canvas.read(cx).zoom_text();
        let status = self.status.clone();

        div().flex().flex_col().size_full().bg(rgb(color::BG_PRIMARY))
            .child(
                div().flex().items_center().h(px(32.0)).bg(rgb(color::BG_TOOLBAR)).px_4().gap_1()
                    .child(div().id("menu_open").px_3().py_1().rounded(px(4.0)).bg(rgb(color::ACCENT))
                        .text_color(rgb(color::TEXT_PRIMARY)).text_sm()
                        .on_click(cx.listener(|_this, _, _window, cx| {
                            let rx = cx.prompt_for_paths(gpui::PathPromptOptions {
                                files: true, directories: false, multiple: false,
                                prompt: Some("打开图片".into()),
                            });
                            cx.spawn(async move |this, cx| {
                                if let Ok(Ok(Some(paths))) = rx.await
                                    && let Some(path) = paths.into_iter().next() {
                                    this.update(cx, |this, cx| { this.open_file(path, cx); cx.notify(); }).ok();
                                }
                            }).detach();
                        }))
                        .child("📂 打开"))
                    .flex_1()
                    .child(div().text_color(rgb(color::TEXT_DIM)).text_xs().child("Kaleido — AI 原生图像工作站")))
            .child(
                div().flex().items_center().h(px(40.0)).bg(rgb(color::BG_TOOLBAR)).px_4().gap_1()
                    .border_b(px(1.0))
                    .child(div().id("zoom_out").px_2().py_1().rounded(px(4.0)).bg(rgb(color::BG_PANEL))
                        .text_color(rgb(color::TEXT_PRIMARY)).text_xs()
                        .on_click(cx.listener(|this, _, _window, cx| { this.zoom_out(cx); cx.notify(); }))
                        .child("➖"))
                    .child(div().w(px(50.0)).text_center().text_color(rgb(color::TEXT_PRIMARY)).text_xs().child(zoom_text))
                    .child(div().id("zoom_in").px_2().py_1().rounded(px(4.0)).bg(rgb(color::BG_PANEL))
                        .text_color(rgb(color::TEXT_PRIMARY)).text_xs()
                        .on_click(cx.listener(|this, _, _window, cx| { this.zoom_in(cx); cx.notify(); }))
                        .child("➕"))
                    .child(div().id("zoom_fit").px_2().py_1().rounded(px(4.0)).bg(rgb(color::BG_PANEL))
                        .text_color(rgb(color::TEXT_PRIMARY)).text_xs()
                        .on_click(cx.listener(|this, _, _window, cx| { this.zoom_fit(cx); cx.notify(); }))
                        .child("⊡"))
                    .child(div().id("zoom_reset").px_2().py_1().rounded(px(4.0)).bg(rgb(color::BG_PANEL))
                        .text_color(rgb(color::TEXT_PRIMARY)).text_xs()
                        .on_click(cx.listener(|this, _, _window, cx| { this.zoom_reset(cx); cx.notify(); }))
                        .child("1:1"))
                    .w(px(1.0)).h(px(24.0)).bg(rgb(color::BORDER))
                    .children(tools.into_iter().map(|tool| {
                        let name = tool.name().to_string();
                        div().id(SharedString::from(name.clone())).px_3().py_1().rounded(px(4.0))
                            .bg(rgb(color::ACCENT)).text_color(rgb(color::TEXT_PRIMARY)).text_xs()
                            .on_click(cx.listener(move |this, _, _window, cx| { this.run_tool(&tool, cx); cx.notify(); }))
                            .child(name)
                    })))
            .child(
                div().flex().flex_1()
                    .child(div().w(px(200.0)).h_full().child(self.tool_params.clone()))
                    .child(div().flex_1().h_full().child(self.canvas.clone()))
                    .child(div().w(px(220.0)).h_full().flex().flex_col()
                        .child(div().flex_1().child(self.layers.clone()))
                        .h(px(1.0)).bg(rgb(color::BORDER))
                        .child(div().h(px(150.0)).child(self.history.clone()))))
            .child(
                div().flex().items_center().h(px(24.0)).bg(rgb(color::BG_TOOLBAR)).px_4()
                    .border_t(px(1.0))
                    .child(div().text_color(rgb(color::TEXT_DIM)).text_xs().child(status))
                    .flex_1()
                    .child(div().text_color(rgb(color::TEXT_DIM)).text_xs().child("Kaleido v0.1.0")))
    }
}
