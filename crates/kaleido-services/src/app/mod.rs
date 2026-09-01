//! The **app** manager implementation — application identity, editing
//! mode, notifications.
//!
//! # Mode state
//!
//! The editing mode starts at [`DEFAULT_MODE`] and is switched through
//! [`AppService::set_mode`]. The mode is a plain string so future modes
//! (or plugin-defined modes) do not require a code change; the empty string
//! is rejected as invalid. [`KaleidoApp::boot`] applies `AppConfig::mode`
//! right after the service is installed, so a configured app never observes
//! the default.
//!
//! # Notifications
//!
//! [`AppService::notify`] keeps the most recent message (for callers that
//! poll it) and forwards every message to the UI service's notification
//! queue when `ui_service` is installed, so the host can surface it. Without
//! a UI service (headless contexts) the message is only logged and stored.

use std::sync::RwLock;

use cordis::{Context, Inject, PluginHandle, Service, service_sync};
use kaleido_traits::services::{ServiceError, ServiceResult};
use kaleido_traits::services::app::AppService;
use kaleido_traits::services::ui::UiService;
use tracing::{debug, info};

use crate::ui::UiServiceImpl;

/// The editing mode used until [`AppService::set_mode`] or `AppConfig::mode`
/// overrides it.
pub(crate) const DEFAULT_MODE: &str = "pixel";

/// Default implementation of [`AppService`].
pub mod kaleido_app;

pub use kaleido_app::{AppConfig, KaleidoApp};

pub struct AppServiceImpl {
    ctx: Context,
    mode: RwLock<String>,
    /// The most recent user-facing notification, if any.
    last_notification: RwLock<Option<String>>,
}

impl AppServiceImpl {
    pub fn new(ctx: Context) -> Self {
        Self {
            ctx,
            mode: RwLock::new(DEFAULT_MODE.into()),
            last_notification: RwLock::new(None),
        }
    }
}

impl Service for AppServiceImpl {
    const NAME: &'static str = "app_service";
}

impl AppService for AppServiceImpl {
    fn name(&self) -> String {
        env!("CARGO_PKG_NAME").to_string()
    }

    fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    fn set_mode(&self, mode: &str) -> ServiceResult<()> {
        if mode.is_empty() {
            return Err(ServiceError::InvalidArgument(
                "editing mode must not be empty".into(),
            ));
        }
        // Recover from a poisoned lock instead of failing: the previous
        // value is still readable and the caller only wants to switch modes.
        let mut current = self.mode.write().unwrap_or_else(|e| e.into_inner());
        *current = mode.to_string();
        Ok(())
    }

    fn current_mode(&self) -> String {
        self.mode
            .read()
            .map(|m| m.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    fn notify(&self, message: &str) {
        // Keep the most recent message for callers that poll it.
        if let Ok(mut last) = self.last_notification.write() {
            *last = Some(message.to_string());
        }
        // Surface the message through the UI service when one is installed;
        // headless contexts simply log it.
        match self.ctx.get::<UiServiceImpl>("ui_service") {
            Ok(Some(ui)) => ui.notify(message),
            Ok(None) => debug!("ui_service not installed; notification not surfaced"),
            Err(e) => debug!("cannot resolve ui_service for notification: {e}"),
        }
        info!(message = %message, "app notification");
    }
}

/// Installs the `app_service` Cordis service.
pub fn plugin() -> PluginHandle {
    service_sync::<AppServiceImpl, (), _>(
        "app_service",
        Inject::none(),
        |ctx, _config| Ok(AppServiceImpl::new(ctx)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn service() -> AppServiceImpl {
        AppServiceImpl::new(Context::new())
    }

    #[test]
    fn name_and_version_are_filled() {
        let svc = service();
        assert!(!svc.name().is_empty());
        assert!(!svc.version().is_empty());
    }

    #[test]
    fn mode_defaults_to_pixel() {
        let svc = service();
        assert_eq!(svc.current_mode(), DEFAULT_MODE);
    }

    #[test]
    fn set_mode_round_trips() {
        let svc = service();
        svc.set_mode("vector").unwrap();
        assert_eq!(svc.current_mode(), "vector");
        svc.set_mode("animation").unwrap();
        assert_eq!(svc.current_mode(), "animation");
        // Back to the default.
        svc.set_mode(DEFAULT_MODE).unwrap();
        assert_eq!(svc.current_mode(), DEFAULT_MODE);
    }

    #[test]
    fn set_mode_rejects_empty_and_keeps_previous_mode() {
        let svc = service();
        svc.set_mode("vector").unwrap();
        assert!(matches!(
            svc.set_mode(""),
            Err(ServiceError::InvalidArgument(_))
        ));
        // The failed switch must not clobber the current mode.
        assert_eq!(svc.current_mode(), "vector");
    }

    #[test]
    fn notify_stores_last_message_and_does_not_panic() {
        let svc = service();
        svc.notify("saved document");
        svc.notify("exported png");
        let last = svc.last_notification.read().unwrap().clone();
        assert_eq!(last.as_deref(), Some("exported png"));
    }

    #[test]
    fn notify_without_ui_service_is_graceful() {
        // `service()` builds a bare context without `ui_service`; notify must
        // degrade to store-and-log instead of failing.
        let svc = service();
        svc.notify("headless message");
        let last = svc.last_notification.read().unwrap().clone();
        assert_eq!(last.as_deref(), Some("headless message"));
    }

    #[test]
    fn plugin_installs_service() {
        let ctx = Context::new();
        ctx.plugin(plugin(), ());
        let svc: Arc<dyn AppService> = ctx.require::<AppServiceImpl>("app_service").unwrap();
        assert_eq!(svc.current_mode(), DEFAULT_MODE);
    }
}
