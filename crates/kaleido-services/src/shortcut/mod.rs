//! The **shortcut manager** implementation.
//!
//! Manages keyboard shortcut registration and resolution.

use std::collections::HashMap;
use std::sync::RwLock;

use kaleido_traits::service_error::ServiceResult;
use kaleido_traits::shortcut::ShortcutService;
use kaleido_traits::keyboard::ShortcutBinding;

/// Default implementation of [`ShortcutService`].
pub struct ShortcutServiceImpl {
    /// Global shortcuts.
    global: RwLock<HashMap<String, ShortcutBinding>>,
    /// Mode-specific shortcuts.
    mode: RwLock<HashMap<String, Vec<ShortcutBinding>>>,
    /// Plugin shortcuts.
    plugin: RwLock<Vec<ShortcutBinding>>,
}

impl ShortcutServiceImpl {
    /// Creates a new shortcut service.
    pub fn new() -> Self {
        Self {
            global: RwLock::new(HashMap::new()),
            mode: RwLock::new(HashMap::new()),
            plugin: RwLock::new(Vec::new()),
        }
    }
}

impl Default for ShortcutServiceImpl {
    fn default() -> Self {
        Self::new()
    }
}

// ── ShortcutService trait implementation ────────────────────────────────────

impl ShortcutService for ShortcutServiceImpl {
    fn register_global(&self, binding: ShortcutBinding) -> ServiceResult<()> {
        let mut global = self.global.write().unwrap_or_else(|p| p.into_inner());
        global.insert(binding.action.clone(), binding);
        Ok(())
    }

    fn register_mode(&self, mode_id: &str, binding: ShortcutBinding) -> ServiceResult<()> {
        let mut mode = self.mode.write().unwrap_or_else(|p| p.into_inner());
        mode.entry(mode_id.to_string()).or_default().push(binding);
        Ok(())
    }

    fn register_plugin(&self, binding: ShortcutBinding) -> ServiceResult<()> {
        let mut plugin = self.plugin.write().unwrap_or_else(|p| p.into_inner());
        plugin.push(binding);
        Ok(())
    }

    fn unregister(&self, action: &str) -> ServiceResult<()> {
        let mut global = self.global.write().unwrap_or_else(|p| p.into_inner());
        global.remove(action);

        let mut mode = self.mode.write().unwrap_or_else(|p| p.into_inner());
        for bindings in mode.values_mut() {
            bindings.retain(|b| b.action != action);
        }

        let mut plugin = self.plugin.write().unwrap_or_else(|p| p.into_inner());
        plugin.retain(|b| b.action != action);

        Ok(())
    }

    fn resolve(&self, key: &str) -> Option<ShortcutBinding> {
        // Check global shortcuts first
        let global = self.global.read().unwrap_or_else(|e| e.into_inner());
        for binding in global.values() {
            if binding.key == key {
                return Some(binding.clone());
            }
        }

        // Check mode shortcuts
        let mode = self.mode.read().unwrap_or_else(|e| e.into_inner());
        for bindings in mode.values() {
            for binding in bindings.iter() {
                if binding.key == key {
                    return Some(binding.clone());
                }
            }
        }

        // Check plugin shortcuts
        let plugin = self.plugin.read().unwrap_or_else(|e| e.into_inner());
        for binding in plugin.iter() {
            if binding.key == key {
                return Some(binding.clone());
            }
        }

        None
    }

    fn key_for(&self, action: &str) -> Option<String> {
        let global = self.global.read().unwrap_or_else(|e| e.into_inner());
        if let Some(binding) = global.get(action) {
            return Some(binding.key.clone());
        }

        let mode = self.mode.read().unwrap_or_else(|e| e.into_inner());
        for bindings in mode.values() {
            for binding in bindings.iter() {
                if binding.action == action {
                    return Some(binding.key.clone());
                }
            }
        }

        let plugin = self.plugin.read().unwrap_or_else(|e| e.into_inner());
        for binding in plugin.iter() {
            if binding.action == action {
                return Some(binding.key.clone());
            }
        }

        None
    }

    fn get_all_shortcuts(&self) -> Vec<ShortcutBinding> {
        let mut result = Vec::new();
        let global = self.global.read().unwrap_or_else(|e| e.into_inner());
        result.extend(global.values().cloned());
        let mode = self.mode.read().unwrap_or_else(|e| e.into_inner());
        for bindings in mode.values() {
            result.extend(bindings.clone());
        }
        let plugin = self.plugin.read().unwrap_or_else(|e| e.into_inner());
        result.extend(plugin.clone());
        result
    }

    fn register_shortcuts(&self, bindings: Vec<ShortcutBinding>) -> ServiceResult<()> {
        for binding in bindings {
            self.register_global(binding)?;
        }
        Ok(())
    }
}

// ── Cordis integration ────────────────────────────────────────────────────


use crate::{impl_service, service_plugin};

impl_service!(ShortcutServiceImpl, "shortcut_service");

service_plugin!(ShortcutServiceImpl, "shortcut_service",
    deps: none,
    build: |_ctx, _config| Ok(ShortcutServiceImpl::new())
);

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_service() {
        let svc = ShortcutServiceImpl::new();
        assert!(svc.resolve("ctrl-z").is_none());
    }

    #[test]
    fn test_register_and_resolve() {
        let svc = ShortcutServiceImpl::new();
        svc.register_global(ShortcutBinding::default("undo", "ctrl-z"))
            .unwrap();

        let binding = svc.resolve("ctrl-z");
        assert!(binding.is_some());
        assert_eq!(binding.unwrap().action, "undo");
    }

    #[test]
    fn test_unregister() {
        let svc = ShortcutServiceImpl::new();
        svc.register_global(ShortcutBinding::default("undo", "ctrl-z"))
            .unwrap();
        assert!(svc.resolve("ctrl-z").is_some());

        svc.unregister("undo").unwrap();
        assert!(svc.resolve("ctrl-z").is_none());
    }

    #[test]
    fn test_key_for() {
        let svc = ShortcutServiceImpl::new();
        svc.register_global(ShortcutBinding::default("save", "ctrl-s"))
            .unwrap();

        assert_eq!(svc.key_for("save").as_deref(), Some("ctrl-s"));
    }
}
