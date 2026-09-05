//! Kaleido desktop application entry point.

use anyhow::Result;
use gpui_kit::{App, AppContext, Bounds, KeyBinding, Pixels, WindowBounds, WindowDecorations, px, size};
use gpui_kit::component::{Root, TitleBar};
use gpui_kit::assets::Assets;
use std::path::PathBuf;  // used in parse_initial_path
use tracing::{error, info, warn};

use crate::boot::BootManager;

use rust_i18n::t;

mod app;
mod boot;
mod canvas;
mod dock;
mod menu;
mod status_bar;

use app::{GlobalKaleidoApp, OpenFile, Redo, Save, SaveAs, Undo};

// ---------------------------------------------------------------------------
// i18n
// ---------------------------------------------------------------------------

rust_i18n::i18n!("locales");

/// Returns the system locale as a language identifier (e.g. `"en"`, `"zh-CN"`).
fn system_locale() -> String {
    sys_locale::get_locale().unwrap_or_else(|| "en".to_string())
}

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

    info!(app = APP_NAME, "{}", t!("app.starting"));

    // Optional image path on the command line: `kaleido-desktop photo.png`.
    let initial_path = parse_initial_path();
    if let Some(ref path) = initial_path {
        info!(?path, "{}", t!("app.initial_file"));
    }

    let app = gpui_kit::application().with_assets(Assets);

    app.run(move |cx: &mut App| {
        // Set locale based on system language.
        let locale = system_locale();
        rust_i18n::set_locale(&locale);
        info!("Locale set to: {locale}");

        gpui_kit::init(cx);
        bind_keyboard_shortcuts(cx);

        // Use default window bounds for now.
        // After boot completes, the editor will load saved bounds via AppService.
        let bounds = Bounds::centered(
            None,
            size(px(DEFAULT_WINDOW_WIDTH), px(DEFAULT_WINDOW_HEIGHT)),
            cx,
        );
        let window_options = make_window_options(bounds);

        // Open the main window with a boot manager that shows loading -> editor.
        if let Err(err) = cx.open_window(window_options, move |window, cx| {
            let view = cx.new(|cx| {
                BootManager::new(initial_path.clone(), window, cx)
            });
            cx.new(|cx| Root::new(view, window, cx))
        }) {
            error!(error = %err, "{}", t!("app.failed_to_open_window"));
            return;
        }

        cx.activate(true);
        info!("{}", t!("app.main_window_opened"));
    });

    info!(app = APP_NAME, "{}", t!("app.exited"));
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Builds `WindowOptions` from the given bounds.
fn make_window_options(bounds: Bounds<Pixels>) -> gpui_kit::WindowOptions {
    let mut options = TitleBar::window_options();
    options.window_bounds = Some(WindowBounds::Windowed(bounds));
    options.focus = true;
    options.window_decorations = Some(WindowDecorations::Client);
    if let Some(titlebar) = options.titlebar.as_mut() {
        titlebar.title = Some(APP_NAME.into());
    }
    options
}

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
            warn!("empty path argument provided, ignoring");
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
