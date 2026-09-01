//! The **app** manager implementation — application identity, software
//! configuration, editing mode, notifications.

use std::collections::HashMap;
use std::sync::RwLock;

use cordis::{Context, Inject, PluginHandle, Service, service_sync};
use kaleido_traits::services::app::{AppService, AppSettings};
use kaleido_traits::services::ui::UiService;
use kaleido_traits::services::{ServiceError, ServiceResult};
use tracing::{debug, info};

use crate::ui::UiServiceImpl;

/// The editing mode used until [`AppService::set_mode`] or settings override
/// changes it.
pub(crate) const DEFAULT_MODE: &str = "pixel";

pub mod kaleido_app;

pub use kaleido_app::{AppConfig, KaleidoApp};

// ── AppServiceImpl ───────────────────────────────────────────────────────

/// Default implementation of [`AppService`].
pub struct AppServiceImpl {
    ctx: Context,
    mode: RwLock<String>,
    settings: RwLock<AppSettings>,
    /// The most recent user-facing notification, if any.
    last_notification: RwLock<Option<String>>,
}

impl AppServiceImpl {
    pub fn new(ctx: Context) -> Self {
        Self {
            ctx,
            mode: RwLock::new(DEFAULT_MODE.into()),
            settings: RwLock::new(AppSettings::default()),
            last_notification: RwLock::new(None),
        }
    }

    /// Creates with initial settings from AppConfig.
    pub fn with_config(ctx: Context, config: &AppConfig) -> Self {
        let mut settings = AppSettings::default();
        settings.default_mode = config.mode.clone();
        settings.plugin_dirs = config
            .wasm_plugin_dirs
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect();
        settings.undo_limit = config.undo_limit as u32;

        Self {
            ctx,
            mode: RwLock::new(config.mode.clone()),
            settings: RwLock::new(settings),
            last_notification: RwLock::new(None),
        }
    }

    /// Parses a single setting value from a string.
    fn parse_setting(key: &str, value: &str) -> ServiceResult<()> {
        match key {
            "default_width" | "default_height" | "undo_limit" | "auto_save_interval" => {
                value.parse::<u32>().map_err(|_| {
                    ServiceError::InvalidArgument(format!(
                        "setting '{key}' requires a positive integer, got '{value}'"
                    ))
                })?;
            }
            "default_mode" => {
                if value.is_empty() {
                    return Err(ServiceError::InvalidArgument(
                        "setting 'default_mode' must not be empty".into(),
                    ));
                }
            }
            "plugin_dirs" => {
                // Comma-separated paths — no validation needed here.
            }
            _ => {
                return Err(ServiceError::InvalidArgument(format!(
                    "unknown setting key: '{key}'"
                )));
            }
        }
        Ok(())
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

    fn settings(&self) -> AppSettings {
        self.settings
            .read()
            .map(|s| s.clone())
            .unwrap_or_else(|e| e.into_inner().clone())
    }

    fn update_settings(&self, settings: AppSettings) -> ServiceResult<()> {
        if settings.default_mode.is_empty() {
            return Err(ServiceError::InvalidArgument(
                "default_mode must not be empty".into(),
            ));
        }
        let mut s = self.settings.write().unwrap_or_else(|e| e.into_inner());
        *s = settings;
        Ok(())
    }

    fn get_setting(&self, key: &str) -> Option<String> {
        let s = self.settings.read().ok()?;
        match key {
            "default_width" => Some(s.default_width.to_string()),
            "default_height" => Some(s.default_height.to_string()),
            "undo_limit" => Some(s.undo_limit.to_string()),
            "auto_save_interval" => Some(s.auto_save_interval.to_string()),
            "plugin_dirs" => Some(s.plugin_dirs.join(",")),
            "default_mode" => Some(s.default_mode.clone()),
            _ => None,
        }
    }

    fn set_setting(&self, key: &str, value: &str) -> ServiceResult<()> {
        Self::parse_setting(key, value)?;
        let mut s = self.settings.write().unwrap_or_else(|e| e.into_inner());
        match key {
            "default_width" => s.default_width = value.parse().unwrap(),
            "default_height" => s.default_height = value.parse().unwrap(),
            "undo_limit" => s.undo_limit = value.parse().unwrap(),
            "auto_save_interval" => s.auto_save_interval = value.parse().unwrap(),
            "plugin_dirs" => {
                s.plugin_dirs = value.split(',').map(str::to_string).collect();
            }
            "default_mode" => {
                s.default_mode = value.to_string();
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    fn set_mode(&self, mode: &str) -> ServiceResult<()> {
        if mode.is_empty() {
            return Err(ServiceError::InvalidArgument(
                "editing mode must not be empty".into(),
            ));
        }
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
        if let Ok(mut last) = self.last_notification.write() {
            *last = Some(message.to_string());
        }
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

// ── Tests ─────────────────────────────────────────────────────────────────

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
    fn settings_default() {
        let svc = service();
        let s = svc.settings();
        assert_eq!(s.default_width, 1024);
        assert_eq!(s.default_height, 768);
        assert_eq!(s.undo_limit, 50);
        assert_eq!(s.auto_save_interval, 0);
        assert_eq!(s.default_mode, "pixel");
        assert!(s.plugin_dirs.is_empty());
    }

    #[test]
    fn update_settings_round_trips() {
        let svc = service();
        let mut new_settings = AppSettings::default();
        new_settings.default_width = 1920;
        new_settings.default_height = 1080;
        new_settings.undo_limit = 100;
        new_settings.auto_save_interval = 120;
        new_settings.default_mode = "vector".into();
        new_settings.plugin_dirs = vec!["/plugins".into()];

        svc.update_settings(new_settings.clone()).unwrap();
        let got = svc.settings();
        assert_eq!(got.default_width, 1920);
        assert_eq!(got.default_height, 1080);
        assert_eq!(got.undo_limit, 100);
        assert_eq!(got.auto_save_interval, 120);
        assert_eq!(got.default_mode, "vector");
        assert_eq!(got.plugin_dirs, vec!["/plugins".to_string()]);
    }

    #[test]
    fn update_settings_rejects_empty_mode() {
        let svc = service();
        let mut bad = AppSettings::default();
        bad.default_mode = String::new();
        assert!(matches!(
            svc.update_settings(bad),
            Err(ServiceError::InvalidArgument(_))
        ));
    }

    #[test]
    fn get_setting_and_set_setting() {
        let svc = service();

        assert_eq!(svc.get_setting("default_width"), Some("1024".into()));
        assert_eq!(svc.get_setting("default_mode"), Some("pixel".into()));
        assert_eq!(svc.get_setting("nonexistent"), None);

        svc.set_setting("default_width", "800").unwrap();
        assert_eq!(svc.get_setting("default_width"), Some("800".into()));

        svc.set_setting("undo_limit", "30").unwrap();
        assert_eq!(svc.get_setting("undo_limit"), Some("30".into()));

        svc.set_setting("plugin_dirs", "/a,/b").unwrap();
        assert_eq!(svc.get_setting("plugin_dirs"), Some("/a,/b".into()));
    }

    #[test]
    fn set_setting_validates_values() {
        let svc = service();

        // Non-numeric for a numeric key.
        assert!(svc.set_setting("default_width", "abc").is_err());

        // Empty mode.
        assert!(svc.set_setting("default_mode", "").is_err());

        // Unknown key.
        assert!(svc.set_setting("unknown_key", "value").is_err());
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
        let svc = service();
        svc.notify("headless message");
        let last = svc.last_notification.read().unwrap().clone();
        assert_eq!(last.as_deref(), Some("headless message"));
    }

    #[test]
    fn plugin_installs_service() {
        let ctx = Context::new();
        ctx.plugin(plugin(), ());
        let svc: Arc<dyn AppService> =
            ctx.require::<AppServiceImpl>("app_service").unwrap();
        assert_eq!(svc.current_mode(), DEFAULT_MODE);
        assert_eq!(svc.settings().default_width, 1024);
    }

    #[test]
    fn with_config_applies_initial_settings() {
        let ctx = Context::new();
        let config = AppConfig {
            mode: "vector".into(),
            wasm_plugin_dirs: vec!["/my/plugins".into()],
            undo_limit: 75,
        };
        let svc = AppServiceImpl::with_config(ctx, &config);
        assert_eq!(svc.current_mode(), "vector");
        let s = svc.settings();
        assert_eq!(s.default_mode, "vector");
        assert_eq!(s.plugin_dirs, vec!["/my/plugins".to_string()]);
        assert_eq!(s.undo_limit, 75);
    }
}
