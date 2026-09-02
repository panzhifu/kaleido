//! Boot manager — shows a loading screen while the service layer boots
//! on a background thread, then transitions to the main editor.

use gpui::*;
use gpui_component::ActiveTheme as _;
use rust_i18n::t;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use crate::app::{GlobalKaleidoApp, KaleidoEditor};
use kaleido_services::app::{AppConfig, KaleidoApp};

// ---------------------------------------------------------------------------
// Async boot state
// ---------------------------------------------------------------------------

/// Shared state for async boot.
type BootResult = cordis::Result<KaleidoApp>;

/// Receives the boot result from the background thread.
pub(crate) struct BootState {
    rx: mpsc::Receiver<BootResult>,
}

impl BootState {
    /// Spawns the boot process on a background thread.
    pub(crate) fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = KaleidoApp::boot(AppConfig::default());
            let _ = tx.send(result);
        });
        Self { rx }
    }

    /// Checks if the boot has completed, returning the result if so.
    pub(crate) fn try_recv(&self) -> Option<BootResult> {
        self.rx.try_recv().ok()
    }
}

// ---------------------------------------------------------------------------
// Boot manager view
// ---------------------------------------------------------------------------

/// View that manages the async boot transition.
pub struct BootManager {
    /// Boot state receiver (None once boot completes).
    boot_state: Option<BootState>,
    /// Initial file path to open.
    initial_path: Option<PathBuf>,
    /// The main editor, created after boot completes.
    editor: Option<Entity<KaleidoEditor>>,
}

impl BootManager {
    pub fn new(
        boot_state: BootState,
        initial_path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            boot_state: Some(boot_state),
            initial_path,
            editor: None,
        }
    }

    /// Called on each frame to check boot status.
    fn poll_boot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // If we already have the editor, nothing to do.
        if self.editor.is_some() {
            return;
        }

        // Check if boot has completed.
        if let Some(boot_state) = &self.boot_state {
            if let Some(result) = boot_state.try_recv() {
                match result {
                    Ok(app) => {
                        cx.set_global(GlobalKaleidoApp(app.clone()));
                        let editor = cx.new(|cx| {
                            KaleidoEditor::new(self.initial_path.clone(), window, cx)
                        });
                        self.editor = Some(editor.clone());
                        self.boot_state = None;
                        cx.notify();
                    }
                    Err(e) => {
                        tracing::error!("{}: {e}", t!("app.failed_to_boot"));
                    }
                }
                return;
            }
        }

        // Boot not complete, poll again next frame.
        let weak = cx.entity().downgrade();
        cx.defer_in(window, move |this, window, cx| {
            this.poll_boot(window, cx);
        });
    }
}

impl Render for BootManager {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Poll boot status on every render.
        self.poll_boot(window, cx);

        if let Some(editor) = &self.editor {
            return editor.clone().into_any_element();
        }

        // Show loading screen.
        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_4()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::BOLD)
                    .child("Kaleido"),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(cx.theme().foreground.opacity(0.6))
                    .child(t!("app.starting")),
            )
            .into_any_element()
    }
}
