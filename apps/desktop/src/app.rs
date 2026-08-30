//! Main application structure with five editing modes.

use gpui::*;
use gpui_component::{ActiveTheme as _, h_flex, v_flex};
use kaleido_services::app::{AppConfig, KaleidoApp};
use kaleido_services::async_io::{AsyncImageLoader, BackgroundSaver};
use kaleido_tool_brightness::{BrightnessToolConfig, brightness_tool_plugin};
use kaleido_tool_invert::invert_tool_plugin;

use crate::canvas::Canvas;
use crate::mode_bar::ModeBar;
use crate::modes::Mode;
use crate::right_panel::RightPanel;
use crate::status_bar::StatusBar;
use crate::toolbar::Toolbar;

pub struct KaleidoEditor {
    #[allow(dead_code)]
    app: KaleidoApp,
    mode_bar: Entity<ModeBar>,
    toolbar: Entity<Toolbar>,
    canvas: Entity<Canvas>,
    right_panel: Entity<RightPanel>,
    status_bar: Entity<StatusBar>,
    #[allow(dead_code)]
    loader: AsyncImageLoader,
    #[allow(dead_code)]
    saver: BackgroundSaver,
}

impl KaleidoEditor {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let app = KaleidoApp::boot(AppConfig::default()).expect("failed to boot Kaleido");
        app.context()
            .plugin(brightness_tool_plugin(), BrightnessToolConfig::default());
        app.context().plugin(invert_tool_plugin(), ());

        let mode_bar = cx.new(ModeBar::new);
        let toolbar = cx.new(|cx| Toolbar::new(Mode::default(), cx));
        let canvas = cx.new(Canvas::new);
        let right_panel = cx.new(|cx| RightPanel::new(Mode::default(), cx));
        let status_bar = cx.new(StatusBar::new);

        let file_codec = app.file_codec_registry();
        let loader = AsyncImageLoader::new(file_codec);
        let saver = BackgroundSaver::new();

        Self {
            app,
            mode_bar,
            toolbar,
            canvas,
            right_panel,
            status_bar,
            loader,
            saver,
        }
    }
}

impl Render for KaleidoEditor {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(self.mode_bar.clone())
            .child(
                h_flex()
                    .flex_1()
                    .min_h(px(0.))
                    .child(self.toolbar.clone())
                    .child(
                        div()
                            .id("canvas-area")
                            .flex_1()
                            .min_w(px(0.))
                            .bg(gpui::rgb(0x0d1117))
                            .child(self.canvas.clone()),
                    )
                    .child(self.right_panel.clone()),
            )
            .child(self.status_bar.clone())
    }
}
