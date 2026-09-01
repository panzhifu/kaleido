//! Mode-aware ShortcutRegistry implementation with tool override support.
//!
//! Layers (highest priority first):
//! 1. **Global** — always active.
//! 2. **Tool override** — only while a tool with overrides is active.
//! 3. **Mode** — changes with the current editing mode.
//! 4. **Plugin** — registered by plugins at runtime.
//!
//! User overrides are persisted to `~/.config/kaleido/shortcuts.json`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;

use kaleido_traits::keyboard::{
    resolve_key, ShortcutBinding, ShortcutError, ShortcutRegisterResult, ShortcutRegistry,
    ShortcutSource,
};
use serde::{Deserialize, Serialize};
use tracing::info;

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const CONFIG_DIR: &str = "kaleido";
const CONFIG_FILE: &str = "shortcuts.json";

/// Built-in default global shortcuts.
fn default_globals() -> HashMap<String, ShortcutBinding> {
    use ShortcutBinding as B;
    let mut map = HashMap::new();
    map.insert("ctrl-z".into(), B::default("undo", "ctrl-z"));
    map.insert("ctrl-shift-z".into(), B::default("redo", "ctrl-shift-z"));
    map.insert("ctrl-o".into(), B::default("open_file", "ctrl-o"));
    map.insert("ctrl-s".into(), B::default("save", "ctrl-s"));
    map.insert("ctrl-shift-s".into(), B::default("save_as", "ctrl-shift-s"));
    map
}

/// Built-in default mode shortcuts.
fn default_mode_bindings() -> HashMap<String, HashMap<String, ShortcutBinding>> {
    use ShortcutBinding as B;
    let mut modes = HashMap::new();

    // Pixel mode
    let mut pixel = HashMap::new();
    pixel.insert("b".into(), B::default("tool.pencil.activate", "b"));
    pixel.insert("e".into(), B::default("tool.eraser.activate", "e"));
    pixel.insert("g".into(), B::default("tool.fill.activate", "g"));
    pixel.insert("m".into(), B::default("tool.rect_select.activate", "m"));
    pixel.insert("w".into(), B::default("tool.magic_wand.activate", "w"));
    pixel.insert("z".into(), B::default("tool.zoom.activate", "z"));
    pixel.insert("c".into(), B::default("tool.crop.activate", "c"));
    modes.insert("pixel".into(), pixel);

    // Painting mode
    let mut painting = HashMap::new();
    painting.insert("b".into(), B::default("tool.brush.activate", "b"));
    painting.insert("e".into(), B::default("tool.eraser.activate", "e"));
    painting.insert("i".into(), B::default("tool.eyedropper.activate", "i"));
    painting.insert("[".into(), B::default("tool.brush.size_decrease", "["));
    painting.insert("]".into(), B::default("tool.brush.size_increase", "]"));
    modes.insert("painting".into(), painting);

    // Vector mode
    let mut vector = HashMap::new();
    vector.insert("v".into(), B::default("tool.select.activate", "v"));
    vector.insert("p".into(), B::default("tool.pen.activate", "p"));
    vector.insert("t".into(), B::default("tool.text.activate", "t"));
    vector.insert("r".into(), B::default("tool.rectangle.activate", "r"));
    vector.insert("a".into(), B::default("tool.node.activate", "a"));
    modes.insert("vector".into(), vector);

    // Layout mode
    let mut layout = HashMap::new();
    layout.insert("t".into(), B::default("tool.text_box.activate", "t"));
    layout.insert("i".into(), B::default("tool.image_frame.activate", "i"));
    modes.insert("layout".into(), layout);

    // Animation mode
    let mut animation = HashMap::new();
    animation.insert("b".into(), B::default("tool.brush.activate", "b"));
    animation.insert("[".into(), B::default("frame.prev", "["));
    animation.insert("]".into(), B::default("frame.next", "]"));
    animation.insert("space".into(), B::default("animation.play", "space"));
    animation.insert("n".into(), B::default("frame.add", "n"));
    animation.insert("d".into(), B::default("frame.duplicate", "d"));
    modes.insert("animation".into(), animation);

    modes
}

// ---------------------------------------------------------------------------
// Persistence format
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize, Deserialize)]
struct ShortcutConfig {
    #[serde(default = "default_version")]
    version: u32,
    #[serde(default)]
    global: HashMap<String, String>,
    #[serde(default)]
    modes: HashMap<String, HashMap<String, String>>,
}

fn default_version() -> u32 {
    1
}

// ---------------------------------------------------------------------------
// ShortcutRegistryImpl
// ---------------------------------------------------------------------------

/// Default [`ShortcutRegistry`] implementation.
///
/// Internal layering (not exposed in the public API):
/// 1. `defaults_global` / `user_global` — Layer 1 (global).
/// 2. `tool_override` — Layer 2 (tool, set dynamically).
/// 3. `defaults_mode` / `user_mode` — Layer 3 (per-mode).
/// 4. `plugins` — Layer 4 (plugin-registered).
pub struct ShortcutRegistryImpl {
    defaults_global: HashMap<String, ShortcutBinding>,
    user_global: RwLock<HashMap<String, ShortcutBinding>>,
    defaults_mode: HashMap<String, HashMap<String, ShortcutBinding>>,
    user_mode: RwLock<HashMap<String, HashMap<String, ShortcutBinding>>>,
    plugins: RwLock<HashMap<String, ShortcutBinding>>,
    tool_override: RwLock<Option<HashMap<String, ShortcutBinding>>>,
    current_mode: RwLock<String>,
    config_path: PathBuf,
}

impl ShortcutRegistryImpl {
    /// Creates a new registry with the default config path.
    pub fn new() -> Self {
        Self::with_config_path(default_config_path())
    }

    /// Creates a registry with a custom config path.
    pub fn with_config_path(config_path: PathBuf) -> Self {
        let defaults_global = default_globals();
        let defaults_mode = default_mode_bindings();

        let registry = Self {
            defaults_global,
            user_global: RwLock::new(HashMap::new()),
            defaults_mode,
            user_mode: RwLock::new(HashMap::new()),
            plugins: RwLock::new(HashMap::new()),
            tool_override: RwLock::new(None),
            current_mode: RwLock::new("pixel".into()),
            config_path,
        };

        if let Err(e) = registry.load() {
            info!(error = %e, "no user shortcut config found, using defaults");
        }

        registry
    }

    /// Returns the config file path.
    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    // ── Internal merge helpers ────────────────────────────────────────────

    /// Returns the merged global map (defaults + user overrides).
    fn merged_global(&self) -> HashMap<String, ShortcutBinding> {
        let mut map = self.defaults_global.clone();
        let user = self.user_global.read().unwrap_or_else(|p| p.into_inner());
        for (k, v) in user.iter() {
            map.insert(k.clone(), v.clone());
        }
        map
    }

    /// Returns the merged mode map for a given mode.
    fn merged_mode(&self, mode_id: &str) -> HashMap<String, ShortcutBinding> {
        let mut map = self.defaults_mode.get(mode_id).cloned().unwrap_or_default();
        let user = self.user_mode.read().unwrap_or_else(|p| p.into_inner());
        if let Some(user_mode) = user.get(mode_id) {
            for (k, v) in user_mode.iter() {
                map.insert(k.clone(), v.clone());
            }
        }
        map
    }

    /// Returns the plugin map.
    fn plugin_map(&self) -> HashMap<String, ShortcutBinding> {
        self.plugins.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Returns the tool override map (or empty).
    fn tool_override_map(&self) -> Option<HashMap<String, ShortcutBinding>> {
        self.tool_override.read().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

// ---------------------------------------------------------------------------
// ShortcutRegistry trait impl
// ---------------------------------------------------------------------------

impl ShortcutRegistry for ShortcutRegistryImpl {
    // ── Mode ──────────────────────────────────────────────────────────────

    fn set_mode(&self, mode_id: &str) {
        let mut mode = self.current_mode.write().unwrap_or_else(|p| p.into_inner());
        *mode = mode_id.to_string();
        info!(mode_id, "shortcut mode switched");
    }

    fn current_mode(&self) -> String {
        self.current_mode.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    // ── Tool override ────────────────────────────────────────────────────

    fn set_tool_overrides(&self, overrides: Option<HashMap<String, ShortcutBinding>>) {
        let is_active = overrides.is_some();
        let mut current = self.tool_override.write().unwrap_or_else(|p| p.into_inner());
        *current = overrides;
        if is_active {
            info!("tool shortcut overrides activated");
        } else {
            info!("tool shortcut overrides cleared");
        }
    }

    fn clear_tool_overrides(&self) {
        self.set_tool_overrides(None);
    }

    // ── Registration ─────────────────────────────────────────────────────
    //
    // All three register methods share the same contract:
    // - an empty/whitespace key is rejected with `InvalidKey`;
    // - re-registering a key that already maps to the *same* action is
    //   idempotent (`Ok`, no mutation);
    // - a key already bound to a *different* action by another user or plugin
    //   registration at this layer is reported as `Conflict`.
    // Built-in defaults are deliberately **not** conflicts: overriding a
    // default (user overrides win over defaults) is the registry's core
    // feature, so only registrations within the same layer compete.

    fn register_global(&self, binding: ShortcutBinding) -> ShortcutRegisterResult {
        if binding.key.trim().is_empty() {
            return ShortcutRegisterResult::InvalidKey("empty key".into());
        }
        let mut user = self.user_global.write().unwrap_or_else(|p| p.into_inner());
        if let Some(existing) = user.get(&binding.key) {
            if existing.action != binding.action {
                return ShortcutRegisterResult::Conflict {
                    existing_action: existing.action.clone(),
                    existing_source: existing.source.clone(),
                };
            }
            return ShortcutRegisterResult::Ok;
        }
        user.insert(binding.key.clone(), binding);
        ShortcutRegisterResult::Ok
    }

    fn register_mode(&self, mode_id: &str, binding: ShortcutBinding) -> ShortcutRegisterResult {
        if binding.key.trim().is_empty() {
            return ShortcutRegisterResult::InvalidKey("empty key".into());
        }
        let mut user = self.user_mode.write().unwrap_or_else(|p| p.into_inner());
        let mode_map = user.entry(mode_id.to_string()).or_insert_with(HashMap::new);
        if let Some(existing) = mode_map.get(&binding.key) {
            if existing.action != binding.action {
                return ShortcutRegisterResult::Conflict {
                    existing_action: existing.action.clone(),
                    existing_source: existing.source.clone(),
                };
            }
            return ShortcutRegisterResult::Ok;
        }
        mode_map.insert(binding.key.clone(), binding);
        ShortcutRegisterResult::Ok
    }

    fn register_plugin(&self, binding: ShortcutBinding) -> ShortcutRegisterResult {
        if binding.key.trim().is_empty() {
            return ShortcutRegisterResult::InvalidKey("empty key".into());
        }
        let mut plugins = self.plugins.write().unwrap_or_else(|p| p.into_inner());
        if let Some(existing) = plugins.get(&binding.key) {
            if existing.action != binding.action {
                return ShortcutRegisterResult::Conflict {
                    existing_action: existing.action.clone(),
                    existing_source: existing.source.clone(),
                };
            }
            return ShortcutRegisterResult::Ok;
        }
        plugins.insert(binding.key.clone(), binding);
        ShortcutRegisterResult::Ok
    }

    // ── Removal ──────────────────────────────────────────────────────────
    //
    // Removals only touch the mutable layers (user overrides / plugins);
    // built-in defaults are never deleted — removing an override simply
    // lets the default binding resurface in `resolve`.

    fn unregister_global(&self, action: &str) {
        let mut user = self.user_global.write().unwrap_or_else(|p| p.into_inner());
        user.retain(|_, b| b.action != action);
    }

    fn unregister_mode(&self, mode_id: &str, action: &str) {
        let mut user = self.user_mode.write().unwrap_or_else(|p| p.into_inner());
        if let Some(mode_map) = user.get_mut(mode_id) {
            mode_map.retain(|_, b| b.action != action);
        }
    }

    fn unregister_plugin(&self, plugin_name: &str) {
        let mut plugins = self.plugins.write().unwrap_or_else(|p| p.into_inner());
        plugins.retain(|_, b| !matches!(b.source, ShortcutSource::Plugin(ref p) if p == plugin_name));
    }

    // ── Lookup ──────────────────────────────────────────────────────────

    fn resolve(&self, key: &str) -> Option<ShortcutBinding> {
        let global = self.merged_global();
        let tool_overrides = self.tool_override_map();
        let mode = self.merged_mode(&self.current_mode());
        let plugin = self.plugin_map();

        resolve_key(key, &global, tool_overrides.as_ref(), &mode, &plugin)
    }

    fn key_for(&self, action: &str) -> Option<String> {
        // Search in priority order
        let global = self.merged_global();
        for (key, binding) in &global {
            if binding.action == action {
                return Some(key.clone());
            }
        }
        if let Some(tool) = self.tool_override_map() {
            for (key, binding) in &tool {
                if binding.action == action {
                    return Some(key.clone());
                }
            }
        }
        let mode = self.merged_mode(&self.current_mode());
        for (key, binding) in &mode {
            if binding.action == action {
                return Some(key.clone());
            }
        }
        let plugin = self.plugin_map();
        for (key, binding) in &plugin {
            if binding.action == action {
                return Some(key.clone());
            }
        }
        None
    }

    fn all_bindings(&self) -> Vec<ShortcutBinding> {
        let mut bindings = Vec::new();
        let mut seen = std::collections::HashSet::new();

        // Collect in priority order, deduplicating by action
        let global = self.merged_global();
        for binding in global.values() {
            if seen.insert(binding.action.clone()) {
                bindings.push(binding.clone());
            }
        }
        if let Some(tool) = self.tool_override_map() {
            for binding in tool.values() {
                if seen.insert(binding.action.clone()) {
                    bindings.push(binding.clone());
                }
            }
        }
        let mode = self.merged_mode(&self.current_mode());
        for binding in mode.values() {
            if seen.insert(binding.action.clone()) {
                bindings.push(binding.clone());
            }
        }
        let plugin = self.plugin_map();
        for binding in plugin.values() {
            if seen.insert(binding.action.clone()) {
                bindings.push(binding.clone());
            }
        }

        bindings.sort_by(|a, b| a.action.cmp(&b.action));
        bindings
    }

    fn user_bindings(&self) -> Vec<ShortcutBinding> {
        let mut bindings = Vec::new();
        let user_global = self.user_global.read().unwrap_or_else(|p| p.into_inner());
        bindings.extend(user_global.values().cloned());
        let user_mode = self.user_mode.read().unwrap_or_else(|p| p.into_inner());
        for mode_map in user_mode.values() {
            bindings.extend(mode_map.values().cloned());
        }
        bindings.sort_by(|a, b| a.action.cmp(&b.action));
        bindings
    }

    // ── Reset ────────────────────────────────────────────────────────────

    fn reset_one(&self, action: &str) {
        self.unregister_global(action);
        let mut user_mode = self.user_mode.write().unwrap_or_else(|p| p.into_inner());
        for mode_map in user_mode.values_mut() {
            mode_map.retain(|_, b| b.action != action);
        }
    }

    fn reset_user(&self) {
        let mut global = self.user_global.write().unwrap_or_else(|p| p.into_inner());
        global.clear();
        let mut mode = self.user_mode.write().unwrap_or_else(|p| p.into_inner());
        mode.clear();
    }

    fn reset_all(&self) {
        self.reset_user();
        let mut plugins = self.plugins.write().unwrap_or_else(|p| p.into_inner());
        plugins.clear();
        self.clear_tool_overrides();
    }

    // ── Persistence ──────────────────────────────────────────────────────

    fn save(&self) -> Result<(), ShortcutError> {
        let user_global = self.user_global.read().unwrap_or_else(|p| p.into_inner());
        let user_mode = self.user_mode.read().unwrap_or_else(|p| p.into_inner());

        let config = ShortcutConfig {
            version: 1,
            global: user_global
                .iter()
                .map(|(k, v)| (k.clone(), v.action.clone()))
                .collect(),
            modes: user_mode
                .iter()
                .map(|(mode_id, mode_map)| {
                    (
                        mode_id.clone(),
                        mode_map
                            .iter()
                            .map(|(k, v)| (k.clone(), v.action.clone()))
                            .collect(),
                    )
                })
                .collect(),
        };

        let json = serde_json::to_string_pretty(&config)
            .map_err(|e| ShortcutError::WriteError(format!("serialisation: {e}")))?;

        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                ShortcutError::WriteError(format!("create dir {}: {e}", parent.display()))
            })?;
        }

        std::fs::write(&self.config_path, json).map_err(|e| {
            ShortcutError::WriteError(format!("write {}: {e}", self.config_path.display()))
        })?;

        info!(path = %self.config_path.display(), "shortcuts saved");
        Ok(())
    }

    fn load(&self) -> Result<(), ShortcutError> {
        let content = std::fs::read_to_string(&self.config_path)
            .map_err(|e| ShortcutError::ReadError(format!("{}: {e}", self.config_path.display())))?;

        let config: ShortcutConfig = serde_json::from_str(&content)
            .map_err(|e| ShortcutError::ReadError(format!("parse: {e}")))?;

        let mut user_global = self.user_global.write().unwrap_or_else(|p| p.into_inner());
        user_global.clear();
        for (key, action) in config.global {
            user_global.insert(key.clone(), ShortcutBinding::user(action, key));
        }
        drop(user_global);

        let mut user_mode = self.user_mode.write().unwrap_or_else(|p| p.into_inner());
        user_mode.clear();
        for (mode_id, mode_map) in config.modes {
            let mut inner = HashMap::new();
            for (key, action) in mode_map {
                inner.insert(key.clone(), ShortcutBinding::user(action, key));
            }
            user_mode.insert(mode_id, inner);
        }

        info!(path = %self.config_path.display(), "shortcuts loaded");
        Ok(())
    }

    // ── GPUI integration ─────────────────────────────────────────────────

    fn resolved_map(&self) -> Vec<(String, String)> {
        let mut map = HashMap::new();

        // Build in reverse priority order so higher layers overwrite
        let plugin = self.plugin_map();
        for (key, binding) in &plugin {
            map.insert(binding.action.clone(), key.clone());
        }
        let mode = self.merged_mode(&self.current_mode());
        for (key, binding) in &mode {
            map.insert(binding.action.clone(), key.clone());
        }
        if let Some(tool) = self.tool_override_map() {
            for (key, binding) in &tool {
                map.insert(binding.action.clone(), key.clone());
            }
        }
        let global = self.merged_global();
        for (key, binding) in &global {
            map.insert(binding.action.clone(), key.clone());
        }

        let mut pairs: Vec<_> = map.into_iter().collect();
        pairs.sort_by(|a, b| a.0.cmp(&b.0));
        pairs
    }
}

impl Default for ShortcutRegistryImpl {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn default_config_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(CONFIG_DIR)
        .join(CONFIG_FILE)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_registry() -> ShortcutRegistryImpl {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let id = COUNTER.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!("kaleido-test-{}", id));
        std::fs::create_dir_all(&dir).ok();
        ShortcutRegistryImpl::with_config_path(dir.join("shortcuts.json"))
    }

    #[test]
    fn global_defaults_present() {
        let reg = test_registry();
        assert_eq!(reg.resolve("ctrl-z").unwrap().action, "undo");
        assert_eq!(reg.resolve("ctrl-s").unwrap().action, "save");
    }

    #[test]
    fn mode_switch_changes_binding() {
        let reg = test_registry();

        // Pixel mode: b -> pencil
        reg.set_mode("pixel");
        assert_eq!(reg.resolve("b").unwrap().action, "tool.pencil.activate");

        // Painting mode: b -> brush
        reg.set_mode("painting");
        assert_eq!(reg.resolve("b").unwrap().action, "tool.brush.activate");

        // Animation mode: b -> brush
        reg.set_mode("animation");
        assert_eq!(reg.resolve("b").unwrap().action, "tool.brush.activate");
    }

    #[test]
    fn tool_override_takes_priority_over_mode() {
        let reg = test_registry();
        reg.set_mode("animation");

        // Without tool override: [ -> frame.prev
        assert_eq!(reg.resolve("[").unwrap().action, "frame.prev");

        // Activate tool override
        let mut overrides = HashMap::new();
        overrides.insert(
            "[".into(),
            ShortcutBinding::default("tool.brush.size_decrease", "["),
        );
        overrides.insert(
            "]".into(),
            ShortcutBinding::default("tool.brush.size_increase", "]"),
        );
        reg.set_tool_overrides(Some(overrides));

        // With tool override: [ -> brush size
        assert_eq!(reg.resolve("[").unwrap().action, "tool.brush.size_decrease");
        assert_eq!(reg.resolve("]").unwrap().action, "tool.brush.size_increase");

        // Clear override
        reg.clear_tool_overrides();
        assert_eq!(reg.resolve("[").unwrap().action, "frame.prev");
    }

    #[test]
    fn global_always_wins() {
        let reg = test_registry();
        reg.set_mode("animation");

        // Even with tool override, global shortcuts win
        let mut overrides = HashMap::new();
        overrides.insert(
            "ctrl-z".into(),
            ShortcutBinding::default("tool.custom", "ctrl-z"),
        );
        reg.set_tool_overrides(Some(overrides));

        assert_eq!(reg.resolve("ctrl-z").unwrap().action, "undo");
    }

    #[test]
    fn user_override_wins_over_default() {
        let reg = test_registry();
        reg.set_mode("pixel");

        // Default: b -> pencil
        assert_eq!(reg.resolve("b").unwrap().action, "tool.pencil.activate");

        // User overrides b -> brush
        reg.register_mode("pixel", ShortcutBinding::user("tool.brush.activate", "b"));

        assert_eq!(reg.resolve("b").unwrap().action, "tool.brush.activate");
    }

    #[test]
    fn plugin_binding_works() {
        let reg = test_registry();
        reg.set_mode("pixel");

        reg.register_plugin(ShortcutBinding::plugin(
            "tool.brightness.open",
            "ctrl-shift-b",
            "brightness",
        ));

        assert_eq!(
            reg.resolve("ctrl-shift-b").unwrap().action,
            "tool.brightness.open"
        );
    }

    #[test]
    fn plugin_does_not_override_mode() {
        let reg = test_registry();
        reg.set_mode("pixel");

        // Mode has b -> pencil
        // Plugin tries to register b -> something else
        // Mode should win (plugin is lower priority)
        reg.register_plugin(ShortcutBinding::plugin("tool.custom", "b", "custom"));

        assert_eq!(reg.resolve("b").unwrap().action, "tool.pencil.activate");
    }

    #[test]
    fn all_bindings_merges_layers() {
        let reg = test_registry();
        reg.set_mode("pixel");
        let all = reg.all_bindings();
        let actions: std::collections::HashSet<_> =
            all.iter().map(|b| b.action.clone()).collect();
        assert!(actions.contains("undo"));
        assert!(actions.contains("tool.pencil.activate"));
    }

    #[test]
    fn resolved_map_is_sorted() {
        let reg = test_registry();
        reg.set_mode("pixel");
        let map = reg.resolved_map();
        let actions: Vec<_> = map.iter().map(|(a, _)| a.clone()).collect();
        let mut sorted = actions.clone();
        sorted.sort();
        assert_eq!(actions, sorted);
    }

    #[test]
    fn save_and_load_roundtrip() {
        let reg = test_registry();
        reg.set_mode("pixel");

        // Override a mode shortcut
        reg.register_mode("pixel", ShortcutBinding::user("tool.brush.activate", "b"));
        // Override a global shortcut
        reg.register_global(ShortcutBinding::user("undo", "ctrl-y"));

        reg.save().expect("save failed");

        // Create fresh registry from same file
        let reg2 = ShortcutRegistryImpl::with_config_path(reg.config_path().clone());
        reg2.set_mode("pixel");

        assert_eq!(reg2.resolve("b").unwrap().action, "tool.brush.activate");
        assert_eq!(reg2.resolve("ctrl-y").unwrap().action, "undo");
    }

    #[test]
    fn reset_one_restores_default() {
        let reg = test_registry();
        reg.set_mode("pixel");

        reg.register_mode("pixel", ShortcutBinding::user("tool.brush.activate", "b"));
        assert_eq!(reg.resolve("b").unwrap().action, "tool.brush.activate");

        reg.reset_one("tool.brush.activate");
        assert_eq!(reg.resolve("b").unwrap().action, "tool.pencil.activate");
    }

    #[test]
    fn reset_user_clears_overrides() {
        let reg = test_registry();
        reg.set_mode("pixel");

        reg.register_mode("pixel", ShortcutBinding::user("tool.brush.activate", "b"));
        reg.register_global(ShortcutBinding::user("undo", "ctrl-y"));

        reg.reset_user();
        assert_eq!(reg.resolve("b").unwrap().action, "tool.pencil.activate");
        assert_eq!(reg.resolve("ctrl-z").unwrap().action, "undo");
    }

    #[test]
    fn unregister_plugin() {
        let reg = test_registry();
        reg.register_plugin(ShortcutBinding::plugin(
            "tool.brightness.open",
            "ctrl-shift-b",
            "brightness",
        ));
        assert!(reg.resolve("ctrl-shift-b").is_some());

        reg.unregister_plugin("brightness");
        assert!(reg.resolve("ctrl-shift-b").is_none());
    }

    #[test]
    fn key_for_searches_all_layers() {
        let reg = test_registry();
        reg.set_mode("pixel");

        // Global
        assert_eq!(reg.key_for("undo").unwrap(), "ctrl-z");
        // Mode
        assert_eq!(reg.key_for("tool.pencil.activate").unwrap(), "b");
    }

    #[test]
    fn invalid_key_rejected() {
        let reg = test_registry();
        let result = reg.register_global(ShortcutBinding::user("test", ""));
        assert!(matches!(result, ShortcutRegisterResult::InvalidKey(_)));
    }

    #[test]
    fn empty_key_rejected_for_all_layers() {
        let reg = test_registry();
        for key in ["", "   "] {
            assert!(matches!(
                reg.register_global(ShortcutBinding::user("a", key)),
                ShortcutRegisterResult::InvalidKey(_)
            ));
            assert!(matches!(
                reg.register_mode("pixel", ShortcutBinding::user("b", key)),
                ShortcutRegisterResult::InvalidKey(_)
            ));
            assert!(matches!(
                reg.register_plugin(ShortcutBinding::plugin("c", key, "p")),
                ShortcutRegisterResult::InvalidKey(_)
            ));
        }
    }

    #[test]
    fn register_global_conflict_reported() {
        let reg = test_registry();

        assert_eq!(
            reg.register_global(ShortcutBinding::user("copy", "ctrl-x")),
            ShortcutRegisterResult::Ok
        );
        // Same key, different action → conflict naming the existing binding.
        assert_eq!(
            reg.register_global(ShortcutBinding::user("paste", "ctrl-x")),
            ShortcutRegisterResult::Conflict {
                existing_action: "copy".into(),
                existing_source: ShortcutSource::User,
            }
        );
        // Re-registering the same key with the same action is idempotent.
        assert_eq!(
            reg.register_global(ShortcutBinding::user("copy", "ctrl-x")),
            ShortcutRegisterResult::Ok
        );
        // Overriding a *default* key (ctrl-s → save) is allowed by design;
        // the user binding shadows the default, which resolution honours.
        assert_eq!(
            reg.register_global(ShortcutBinding::user("app.my_action", "ctrl-s")),
            ShortcutRegisterResult::Ok
        );
        assert_eq!(reg.resolve("ctrl-s").unwrap().action, "app.my_action");
    }

    #[test]
    fn register_mode_conflict_reported() {
        let reg = test_registry();

        assert_eq!(
            reg.register_mode("pixel", ShortcutBinding::user("copy", "x")),
            ShortcutRegisterResult::Ok
        );
        assert_eq!(
            reg.register_mode("pixel", ShortcutBinding::user("paste", "x")),
            ShortcutRegisterResult::Conflict {
                existing_action: "copy".into(),
                existing_source: ShortcutSource::User,
            }
        );
        // The same key in a different mode is not a conflict.
        assert_eq!(
            reg.register_mode("vector", ShortcutBinding::user("paste", "x")),
            ShortcutRegisterResult::Ok
        );
        // Overriding a default mode binding is allowed.
        assert_eq!(
            reg.register_mode("pixel", ShortcutBinding::user("tool.brush.activate", "b")),
            ShortcutRegisterResult::Ok
        );
        assert_eq!(reg.resolve("b").unwrap().action, "tool.brush.activate");
    }

    #[test]
    fn register_plugin_conflict_reported() {
        let reg = test_registry();

        assert_eq!(
            reg.register_plugin(ShortcutBinding::plugin("one.action", "ctrl-x", "one")),
            ShortcutRegisterResult::Ok
        );
        // A second plugin must not steal the key.
        assert_eq!(
            reg.register_plugin(ShortcutBinding::plugin("two.action", "ctrl-x", "two")),
            ShortcutRegisterResult::Conflict {
                existing_action: "one.action".into(),
                existing_source: ShortcutSource::Plugin("one".into()),
            }
        );
        // The owning plugin re-registering is idempotent.
        assert_eq!(
            reg.register_plugin(ShortcutBinding::plugin("one.action", "ctrl-x", "one")),
            ShortcutRegisterResult::Ok
        );
    }

    #[test]
    fn unregister_global_restores_default_binding() {
        let reg = test_registry();

        // Override the default undo key.
        reg.register_global(ShortcutBinding::user("undo", "ctrl-y"));
        assert_eq!(reg.resolve("ctrl-y").unwrap().action, "undo");

        // Removing the override resurfaces the built-in ctrl-z binding.
        reg.unregister_global("undo");
        assert!(reg.resolve("ctrl-y").is_none());
        assert_eq!(reg.resolve("ctrl-z").unwrap().action, "undo");
    }

    #[test]
    fn key_for_prefers_higher_priority_layer() {
        let reg = test_registry();
        reg.set_mode("pixel");

        // The same action exists in the mode layer (z) and the global layer
        // (shift-z); key_for must report the higher-priority global key.
        reg.register_global(ShortcutBinding::user("tool.zoom.activate", "shift-z"));
        assert_eq!(reg.key_for("tool.zoom.activate").unwrap(), "shift-z");
        assert_eq!(reg.resolve("shift-z").unwrap().action, "tool.zoom.activate");
    }
}
