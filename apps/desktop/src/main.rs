//! Kaleido desktop application entry point.

use anyhow::Result;
use gpui::{App, AppContext, Bounds, Window, WindowBounds, WindowOptions, px, size};
use gpui_platform::application;

mod app;
mod messages;
mod panels;
mod state;
mod theme;

use app::KaleidoEditor;

fn main() -> Result<()> {
    application().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
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
