//! Boot manager — shows a loading screen while the service layer boots
//! on a background thread, then transitions to the main editor.

use gpui_kit::*;
use gpui_kit::component::ActiveTheme as _;
use rust_i18n::t;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use crate::app::{GlobalKaleidoApp, KaleidoEditor};
use kaleido_services::app::{AppConfig, KaleidoApp};

// ---------------------------------------------------------------------------
// Async boot state
// ---------------------------------------------------------------------------

/// Shared state for async boot — uses std::thread + mpsc because
/// `KaleidoApp::boot()` is synchronous and may block. The `window` reference
/// cannot be moved into a `cx.spawn` future because it doesn't satisfy `'static`.
type BootResult = cordis::Result<KaleidoApp>;

/// Receives the boot result from the background thread.
struct BootState {
    rx: mpsc::Receiver<BootResult>,
}

impl BootState {
    /// Spawns the boot process on a background thread.
    fn spawn() -> Self {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let result = KaleidoApp::boot(AppConfig::default());
            if tx.send(result).is_err() {
                tracing::warn!("boot result receiver dropped");
            }
        });
        Self { rx }
    }

    /// Checks if the boot has completed, returning the result if so.
    fn try_recv(&self) -> Option<BootResult> {
        self.rx.try_recv().ok()
    }
}

// ---------------------------------------------------------------------------
// Boot manager view
// ---------------------------------------------------------------------------

/// View that manages the async boot transition.
enum BootStage {
    /// Showing loading screen, boot in progress.
    Loading,
    /// Boot completed, showing editor.
    Ready(Entity<KaleidoEditor>),
    /// Boot failed.
    Failed(String),
}

pub struct BootManager {
    /// Boot state receiver (None once boot completes).
    boot_state: Option<BootState>,
    /// Initial file path to open.
    initial_path: Option<PathBuf>,
    /// Current boot stage.
    stage: BootStage,
}

impl BootManager {
    pub fn new(
        initial_path: Option<PathBuf>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Self {
        Self {
            boot_state: Some(BootState::spawn()),
            initial_path,
            stage: BootStage::Loading,
        }
    }

    /// Called on each render to check boot status.
    fn poll_boot(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !matches!(self.stage, BootStage::Loading) {
            return;
        }

        let Some(boot_state) = &self.boot_state else { return };

        if let Some(result) = boot_state.try_recv() {
            match result {
                Ok(app) => {
                    cx.set_global(GlobalKaleidoApp(app));
                    let editor = cx.new(|cx| {
                        KaleidoEditor::new(self.initial_path.clone(), window, cx)
                    });
                    self.stage = BootStage::Ready(editor.clone());
                    self.boot_state = None;
                    cx.notify();
                }
                Err(e) => {
                    tracing::error!("{}: {e}", t!("app.failed_to_boot"));
                    self.stage = BootStage::Failed(format!("{e}"));
                    self.boot_state = None;
                    cx.notify();
                }
            }
            return;
        }

        // Boot not complete, poll again next frame.
        let _weak = cx.entity().downgrade();
        cx.defer_in(window, move |this, window, cx| {
            this.poll_boot(window, cx);
        });
    }
}

impl Render for BootManager {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Poll boot status on every render.
        self.poll_boot(window, cx);

        match &self.stage {
            BootStage::Loading => {
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
            BootStage::Ready(editor) => {
                editor.clone().into_any_element()
            }
            BootStage::Failed(msg) => {
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
                            .text_color(cx.theme().danger)
                            .child("⚠"),
                    )
                    .child(
                        div()
                            .text_base()
                            .child(t!("app.failed_to_boot")),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().foreground.opacity(0.5))
                            .child(msg.clone()),
                    )
                    .into_any_element()
            }
        }
    }
}
