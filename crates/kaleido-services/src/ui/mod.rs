//! The **ui** manager implementation — host UI surface for services and
//! plugins (notifications, status text, side panels).
//!
//! The service owns the notification queue and the status text; side panels
//! are delegated to the [`PanelRegistry`] (see [`panel_registry`]).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex, RwLock, RwLockWriteGuard};

use cordis::Context;

use crate::{impl_service, service_plugin};
use kaleido_traits::plugins::panel::{Panel, PanelRegistry};
use kaleido_traits::ui::{UiService, MAX_NOTIFICATIONS};
use kaleido_traits::ServiceResult;

/// Default implementation of [`UiService`].
pub mod panel_registry;

/// The UI manager implementation.
///
/// All state is lock-protected and safe to touch from any thread. Internal
/// locks are *recovered* on poisoning: the guarded values are a `VecDeque`
/// and a `String` whose individual operations cannot corrupt them, so a
/// panic in a concurrent caller must not wedge the service — the poison
/// error is unwrapped and the operation proceeds.
pub struct UiServiceImpl {
    /// Service context, kept for lifecycle/event use (matching the other
    /// manager implementations).
    ctx: Context,
    /// Current status-bar text.
    status: RwLock<String>,
    /// FIFO queue of user-facing notifications, bounded at
    /// [`MAX_NOTIFICATIONS`].
    notifications: RwLock<VecDeque<String>>,
    /// Weak panel registry this service fronts.
    panels: Arc<dyn PanelRegistry>,
}

impl UiServiceImpl {
    pub fn new(ctx: Context, panels: Arc<dyn PanelRegistry>) -> Self {
        Self {
            ctx,
            status: RwLock::new(String::new()),
            notifications: RwLock::new(VecDeque::new()),
            panels,
        }
    }

    /// Acquires the status lock for writing, recovering from poisoning.
    fn status_guard(&self) -> RwLockWriteGuard<'_, String> {
        self.status
            .write()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    /// Acquires the notification lock for writing, recovering from poisoning.
    fn notifications_guard(&self) -> RwLockWriteGuard<'_, VecDeque<String>> {
        self.notifications
            .write()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

impl_service!(UiServiceImpl, "ui_service");

impl UiService for UiServiceImpl {
    fn notify(&self, message: &str) {
        let mut queue = self.notifications_guard();
        queue.push_back(message.to_string());
        // Bound the queue: evict the oldest message once the cap is exceeded.
        while queue.len() > MAX_NOTIFICATIONS {
            queue.pop_front();
        }
    }

    fn set_status(&self, text: &str) {
        *self.status_guard() = text.to_string();
    }

    fn status(&self) -> String {
        let guard = self
            .status
            .read()
            .unwrap_or_else(|poison| poison.into_inner());
        guard.as_str().to_owned()
    }

    fn register_panel(&self, panel: Arc<Mutex<dyn Panel>>) -> ServiceResult<()> {
        // The registry holds the panel weakly; the caller keeps the strong
        // `Arc` alive. The registry filters dead references on read.
        self.panels.register(Arc::downgrade(&panel));
        Ok(())
    }

    fn panels(&self) -> Vec<Arc<Mutex<dyn Panel>>> {
        self.panels.panels()
    }
}

service_plugin!(UiServiceImpl, "ui_service",
    deps: ["panel_registry"],
    build: |ctx, _config| {
        let panels = crate::services::ui::panel_registry::resolve_panel_registry(&ctx)?;
        Ok(UiServiceImpl::new(ctx, panels))
    }
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::ui::panel_registry::panel_registry_plugin;
    use kaleido_traits::plugins::panel::PanelContext;

    /// A minimal panel for registration tests.
    struct TestPanel;

    impl Panel for TestPanel {
        fn render(&mut self, _ctx: &mut dyn PanelContext) {}
    }

    /// Boots a context with the panel registry installed and returns the
    /// UI service resolved through the real Cordis plugin.
    fn service() -> UiServiceImpl {
        let ctx = Context::new();
        ctx.plugin(panel_registry_plugin(), ());
        let panels = crate::services::ui::panel_registry::resolve_panel_registry(&ctx).unwrap();
        UiServiceImpl::new(ctx, panels)
    }

    #[test]
    fn status_round_trips() {
        let svc = service();
        assert_eq!(svc.status(), "");
        svc.set_status("100% zoom");
        assert_eq!(svc.status(), "100% zoom");
        svc.set_status("");
        assert_eq!(svc.status(), "");
    }

    #[test]
    fn notify_keeps_last_20_messages() {
        let svc = service();
        for i in 0..25 {
            svc.notify(&format!("message {i}"));
        }
        let queue = svc.notifications.read().unwrap();
        assert_eq!(queue.len(), MAX_NOTIFICATIONS);
        // The oldest 5 messages were evicted.
        assert_eq!(queue.front().map(String::as_str), Some("message 5"));
        assert_eq!(queue.back().map(String::as_str), Some("message 24"));
    }

    #[test]
    fn notify_preserves_fifo_order_within_capacity() {
        let svc = service();
        svc.notify("first");
        svc.notify("second");
        svc.notify("third");
        let queue = svc.notifications.read().unwrap();
        let messages: Vec<&str> = queue.iter().map(String::as_str).collect();
        assert_eq!(messages, ["first", "second", "third"]);
    }

    #[test]
    fn notify_never_exceeds_capacity_under_concurrency() {
        let svc = Arc::new(service());
        let mut handles = Vec::new();
        for worker in 0..8 {
            let svc = svc.clone();
            handles.push(std::thread::spawn(move || {
                for i in 0..100 {
                    svc.notify(&format!("worker {worker} msg {i}"));
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        // 800 notifications, only the last 20 survive.
        let queue = svc.notifications.read().unwrap();
        assert_eq!(queue.len(), MAX_NOTIFICATIONS);
        assert!(queue.back().unwrap().starts_with("worker "));
        assert!(queue.front().unwrap().starts_with("worker "));
    }

    #[test]
    fn status_survives_concurrent_reads_and_writes() {
        let svc = Arc::new(service());
        let mut handles = Vec::new();
        for worker in 0..8 {
            let svc = svc.clone();
            handles.push(std::thread::spawn(move || {
                for i in 0..200 {
                    svc.set_status(&format!("worker {worker} iteration {i}"));
                    // Every read must return a well-formed value (no poison
                    // propagation, no torn state).
                    let _ = svc.status();
                }
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        // The last write wins; it must be one of the values we wrote.
        assert!(svc.status().starts_with("worker "));
    }

    #[test]
    fn register_panel_is_visible_in_panels() {
        let svc = service();
        assert!(svc.panels().is_empty());

        let panel: Arc<Mutex<dyn Panel>> = Arc::new(Mutex::new(TestPanel));
        svc.register_panel(panel.clone()).unwrap();

        let panels = svc.panels();
        assert_eq!(panels.len(), 1);
        // The registered strong reference and the registry's weak reference
        // point at the same panel.
        assert!(Arc::ptr_eq(&panels[0], &panel));
    }

    #[test]
    fn dead_panels_are_filtered_out() {
        let svc = service();
        {
            let panel: Arc<Mutex<dyn Panel>> = Arc::new(Mutex::new(TestPanel));
            svc.register_panel(panel.clone()).unwrap();
            assert_eq!(svc.panels().len(), 1);
            // panel is dropped here → the registry's weak ref goes dead.
        }
        // Live-panel queries no longer report the dead panel.
        assert!(svc.panels().is_empty());
        assert_eq!(svc.panels().len(), 0);
    }

    #[test]
    fn plugin_installs_service() {
        let ctx = Context::new();
        ctx.plugin(panel_registry_plugin(), ());
        ctx.plugin(plugin(), ());
        let svc: Arc<dyn UiService> = ctx.require::<UiServiceImpl>("ui_service").unwrap();
        assert!(svc.panels().is_empty());
        svc.set_status("ready");
        assert_eq!(svc.status(), "ready");
    }
}
