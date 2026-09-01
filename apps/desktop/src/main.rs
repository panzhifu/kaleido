//! Kaleido desktop application entry point.

use anyhow::Result;
use gpui::{App, AppContext, Bounds, KeyBinding, WindowBounds, WindowDecorations, px, size};
use gpui_component::{Root, TitleBar};
use gpui_component_assets::Assets;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::info;

use kaleido_services::app::{AppConfig, KaleidoApp};

mod app;
mod canvas;
mod status_bar;
mod toolbar;

use app::{GlobalKaleidoApp, KaleidoEditor, OpenFile, Redo, Save, SaveAs, Undo};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Default window dimensions when the application launches.
const DEFAULT_WINDOW_WIDTH: f32 = 1200.0;
const DEFAULT_WINDOW_HEIGHT: f32 = 800.0;

/// Display name shown in the title bar and used in logs.
const APP_NAME: &str = "Kaleido";

/// Keyboard shortcuts.
const KEY_UNDO: &str = "ctrl-z";
const KEY_REDO: &str = "ctrl-shift-z";
const KEY_OPEN_FILE: &str = "ctrl-o";
const KEY_SAVE: &str = "ctrl-s";
const KEY_SAVE_AS: &str = "ctrl-shift-s";

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

fn main() -> Result<()> {
    init_tracing();

    info!(app = APP_NAME, "starting application");

    // Optional image path on the command line: `kaleido-desktop photo.png`.
    let initial_path = parse_initial_path();
    if let Some(ref path) = initial_path {
        info!(?path, "initial file path provided");
    }

    let app = gpui_platform::application().with_assets(Assets);

    app.run(move |cx: &mut App| {
        gpui_component::init(cx);
        bind_keyboard_shortcuts(cx);

        // Boot the service layer and register as a global.
        let kaleido_app =
            KaleidoApp::boot(AppConfig::default()).expect("failed to boot KaleidoApp");
        cx.set_global(GlobalKaleidoApp(kaleido_app));

        let bounds = Bounds::centered(
            None,
            size(px(DEFAULT_WINDOW_WIDTH), px(DEFAULT_WINDOW_HEIGHT)),
            cx,
        );

        let mut options = TitleBar::window_options();
        options.window_bounds = Some(WindowBounds::Windowed(bounds));
        options.focus = true;
        options.window_decorations = Some(WindowDecorations::Client);
        if let Some(titlebar) = options.titlebar.as_mut() {
            titlebar.title = Some(APP_NAME.into());
        }

        if let Err(err) = cx.open_window(options, move |window, cx| {
            let path = initial_path.clone();
            let view = cx.new(|cx| KaleidoEditor::new(path, window, cx));
            cx.new(|cx| Root::new(view, window, cx))
        }) {
            tracing::error!(error = %err, "failed to open main window");
            return;
        }

        cx.activate(true);
        info!("main window opened successfully");
    });

    info!(app = APP_NAME, "application exited");
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Initialise the `tracing` subscriber with sensible defaults.
fn init_tracing() {
    let filter = tracing_subscriber::EnvFilter::try_from_env("KALEIDO_LOG")
        .or_else(|_| tracing_subscriber::EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(false)
        .with_level(true)
        .init();
}

/// Extract the optional initial file path from command-line arguments.
fn parse_initial_path() -> Option<PathBuf> {
    let path = std::env::args().nth(1).map(PathBuf::from);
    if let Some(ref p) = path {
        if p.as_os_str().is_empty() {
            tracing::warn!("empty path argument provided, ignoring");
            return None;
        }
    }
    path
}

/// Register global keyboard shortcuts.
fn bind_keyboard_shortcuts(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new(KEY_UNDO, Undo, None),
        KeyBinding::new(KEY_REDO, Redo, None),
        KeyBinding::new(KEY_OPEN_FILE, OpenFile, None),
        KeyBinding::new(KEY_SAVE, Save, None),
        KeyBinding::new(KEY_SAVE_AS, SaveAs, None),
    ]);
    info!("keyboard shortcuts registered");
}
