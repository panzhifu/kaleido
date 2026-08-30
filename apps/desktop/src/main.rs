//! Kaleido desktop application entry point.

use anyhow::Result;
use gpui::{App, AppContext, Bounds, KeyBinding, WindowBounds, WindowDecorations, px, size};
use gpui_component::{Root, TitleBar};
use gpui_component_assets::Assets;
use std::path::PathBuf;

mod app;
mod canvas;
mod mode_bar;
mod modes;
mod right_panel;
mod state;
mod status_bar;
mod toolbar;

use app::{KaleidoEditor, OpenFile, Redo, Save, SaveAs, Undo};

fn main() -> Result<()> {
    // Optional image path on the command line: `kaleido-desktop photo.png`.
    let initial_path = std::env::args().nth(1).map(PathBuf::from);
    let app = gpui_platform::application().with_assets(Assets);

    app.run(move |cx: &mut App| {
        gpui_component::init(cx);

        // Undo / redo and file operations live on the keyboard.
        cx.bind_keys([
            KeyBinding::new("ctrl-z", Undo, None),
            KeyBinding::new("ctrl-shift-z", Redo, None),
            KeyBinding::new("ctrl-o", OpenFile, None),
            KeyBinding::new("ctrl-s", Save, None),
            KeyBinding::new("ctrl-shift-s", SaveAs, None),
        ]);

        let bounds = Bounds::centered(None, size(px(1200.), px(800.)), cx);
        // The menu bar lives in the window's title bar (see `ModeBar`), so the
        // window must draw its own decorations instead of the window manager's.
        let mut options = TitleBar::window_options();
        options.window_bounds = Some(WindowBounds::Windowed(bounds));
        options.focus = true;
        options.window_decorations = Some(WindowDecorations::Client);
        if let Some(titlebar) = options.titlebar.as_mut() {
            titlebar.title = Some("Kaleido".into());
        }
        cx.open_window(
            options,
            move |window, cx| {
                let path = initial_path.clone();
                let view = cx.new(|cx| KaleidoEditor::new(path, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("failed to open window");
        cx.activate(true);
    });
    Ok(())
}
