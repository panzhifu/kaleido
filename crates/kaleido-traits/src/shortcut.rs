//! The **shortcut manager** — keyboard shortcut registration and resolution.
//!
//! Wraps the lower-level [`ShortcutRegistry`] and provides application-facing
//! shortcut management.

use super::ServiceResult;
use crate::keyboard::{ShortcutBinding, ShortcutRegisterResult};

/// The shortcut management service.
pub trait ShortcutService: Send + Sync + 'static {
    // ── Registration ───────────────────────────────────────────────────

    /// Registers a global shortcut (available in every mode).
    fn register_global(&self, binding: ShortcutBinding) -> ServiceResult<()>;

    /// Registers a shortcut for a specific editing mode.
    fn register_mode(&self, mode_id: &str, binding: ShortcutBinding) -> ServiceResult<()>;

    /// Registers a plugin-provided shortcut.
    fn register_plugin(&self, binding: ShortcutBinding) -> ServiceResult<()>;

    // ── Unregistration ─────────────────────────────────────────────────

    /// Removes a shortcut by its action name.
    fn unregister(&self, action: &str) -> ServiceResult<()>;

    // ── Queries ────────────────────────────────────────────────────────

    /// Resolves a key press to the bound action (all layers considered).
    fn resolve(&self, key: &str) -> Option<ShortcutBinding>;

    /// The key currently bound to an action, if any.
    fn key_for(&self, action: &str) -> Option<String>;

    // ── Persistence ───────────────────────────────────────────────────

    /// Returns all registered shortcuts (global + mode + plugin).
    fn get_all_shortcuts(&self) -> Vec<ShortcutBinding>;

    /// Registers multiple shortcuts at once (used after loading from disk).
    fn register_shortcuts(&self, bindings: Vec<ShortcutBinding>) -> ServiceResult<()>;
}
