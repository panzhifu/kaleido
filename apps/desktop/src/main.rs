//! Kaleido desktop application entry point.

use anyhow::Result;
use gpui::{App, AppContext, Bounds, WindowBounds, WindowOptions, px, size};
use gpui_component::Root;

mod app;
mod canvas;
mod mode_bar;
mod modes;
mod right_panel;
mod status_bar;
mod toolbar;

use app::KaleidoEditor;

fn main() -> Result<()> {
    let app = gpui_platform::application();

    app.run(move |cx: &mut App| {
        gpui_component::init(cx);

        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                focus: true,
                ..Default::default()
            },
            |window, cx| {
                let view = cx.new(KaleidoEditor::new);
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("failed to open window");
        cx.activate(true);
    });
    Ok(())
}
