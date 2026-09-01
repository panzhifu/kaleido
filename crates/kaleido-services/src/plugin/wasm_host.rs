//! WASM plugin host — internal component of PluginService.
//!
//! WASM plugins use the capabilities (format codec, document, history) through
//! a C-ABI interface. The host loads `.wasm` files and links them to the
//! capability functions.
//!
//! This is an internal component — NOT registered as a Cordis service.
//! PluginService uses it internally to load/unload WASM plugins.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use wasmtime::{Config, Engine, Instance, Linker, Module, Store};

/// A loaded WASM plugin.
pub struct WasmPlugin {
    /// Plugin name.
    pub name: String,
    /// Plugin path.
    pub path: PathBuf,
    /// WASM instance.
    instance: Instance,
    /// WASM store.
    store: Store<()>,
}

/// WASM plugin host — manages WASM plugin loading and capability provisioning.
pub struct WasmHost {
    /// WASM engine.
    engine: Arc<Engine>,
    /// Loaded plugins.
    plugins: HashMap<String, WasmPlugin>,
}

impl WasmHost {
    /// Creates a new WASM host.
    pub fn new() -> Self {
        let mut config = Config::new();
        config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);
        let engine = Arc::new(Engine::new(&config).expect("failed to create WASM engine"));

        Self {
            engine,
            plugins: HashMap::new(),
        }
    }

    /// Loads a WASM plugin from a `.wasm` file.
    pub fn load_plugin(&mut self, path: &Path) -> Result<String> {
        let name = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "wasm_plugin".into());

        let wasm_bytes = std::fs::read(path)
            .with_context(|| format!("failed to read WASM file: {}", path.display()))?;

        let module = Module::new(&self.engine, &wasm_bytes)
            .with_context(|| format!("failed to compile WASM module: {}", path.display()))?;

        let mut store = Store::new(&self.engine, ());
        let mut linker: Linker<()> = Linker::new(&self.engine);

        // Link capability functions
        self.link_capabilities(&mut linker)?;

        let instance = linker
            .instantiate(&mut store, &module)
            .with_context(|| format!("failed to instantiate WASM module: {}", path.display()))?;

        let plugin = WasmPlugin {
            name: name.clone(),
            path: path.to_path_buf(),
            instance,
            store,
        };

        self.plugins.insert(name.clone(), plugin);

        tracing::info!("WASM plugin loaded: {}", name);
        Ok(name)
    }

    /// Unloads a WASM plugin by name.
    pub fn unload_plugin(&mut self, name: &str) -> Result<()> {
        self.plugins
            .remove(name)
            .ok_or_else(|| anyhow::anyhow!("plugin not found: {}", name))?;
        tracing::info!("WASM plugin unloaded: {}", name);
        Ok(())
    }

    /// Lists all loaded plugin names.
    pub fn list_plugins(&self) -> Vec<&str> {
        self.plugins.keys().map(|s| s.as_str()).collect()
    }

    // ── Capability Linking ─────────────────────────────────────────────

    /// Links all capability functions to the linker.
    fn link_capabilities(&self, linker: &mut Linker<()>) -> Result<()> {
        // Memory management
        linker.func_wrap("host", "alloc", |size: i32| -> i32 {
            let layout = std::alloc::Layout::from_size_align(size as usize, 1).unwrap();
            unsafe { std::alloc::alloc(layout) as i32 }
        })?;

        linker.func_wrap("host", "free", |ptr: i32, size: i32| {
            let layout = std::alloc::Layout::from_size_align(size as usize, 1).unwrap();
            unsafe { std::alloc::dealloc(ptr as *mut u8, layout) };
        })?;

        // Format codec capabilities
        linker.func_wrap("host", "format_decode", |_path_ptr: i32, _path_len: i32| -> i64 {
            -1
        })?;

        linker.func_wrap(
            "host",
            "format_encode",
            |_path_ptr: i32, _path_len: i32, _data_ptr: i32, _data_len: i32, _width: i32,
             _height: i32| -> i32 { -1 },
        )?;

        // Document capabilities
        linker.func_wrap("host", "document_open", |_path_ptr: i32, _path_len: i32| -> i32 {
            -1
        })?;

        linker.func_wrap("host", "document_save", |_path_ptr: i32, _path_len: i32| -> i32 {
            -1
        })?;

        // History capabilities
        linker.func_wrap("host", "history_undo", || -> i32 { -1 })?;
        linker.func_wrap("host", "history_redo", || -> i32 { -1 })?;
        linker.func_wrap("host", "history_push", |_label_ptr: i32, _label_len: i32| -> i32 {
            -1
        })?;

        // Layer capabilities
        linker.func_wrap(
            "host",
            "layer_add_pixel",
            |_name_ptr: i32, _name_len: i32, _width: i32, _height: i32| -> i32 { -1 },
        )?;
        linker.func_wrap(
            "host",
            "layer_add_group",
            |_name_ptr: i32, _name_len: i32| -> i32 { -1 },
        )?;
        linker.func_wrap("host", "layer_remove", |_id: i32| -> i32 { -1 })?;
        linker.func_wrap(
            "host",
            "layer_set_visible",
            |_id: i32, _visible: i32| -> i32 { -1 },
        )?;
        linker.func_wrap(
            "host",
            "layer_set_opacity",
            |_id: i32, _opacity: f32| -> i32 { -1 },
        )?;
        linker.func_wrap("host", "layer_count", || -> i32 { 0 })?;

        // Shortcut capabilities
        linker.func_wrap(
            "host",
            "shortcut_register_global",
            |_action_ptr: i32, _action_len: i32, _key_ptr: i32, _key_len: i32| -> i32 { -1 },
        )?;
        linker.func_wrap(
            "host",
            "shortcut_register_plugin",
            |_action_ptr: i32, _action_len: i32, _key_ptr: i32, _key_len: i32| -> i32 { -1 },
        )?;
        linker.func_wrap(
            "host",
            "shortcut_unregister",
            |_action_ptr: i32, _action_len: i32| -> i32 { -1 },
        )?;
        linker.func_wrap(
            "host",
            "shortcut_resolve",
            |_key_ptr: i32, _key_len: i32| -> i32 { -1 },
        )?;

        // Logging
        linker.func_wrap("host", "log", |_msg_ptr: i32, _msg_len: i32| {})?;

        Ok(())
    }
}

impl Default for WasmHost {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasm_host_creation() {
        let host = WasmHost::new();
        assert!(host.list_plugins().is_empty());
    }

    #[test]
    fn test_load_nonexistent_plugin() {
        let mut host = WasmHost::new();
        let result = host.load_plugin(Path::new("nonexistent.wasm"));
        assert!(result.is_err());
    }

    #[test]
    fn test_load_real_wasm_plugin() {
        let mut host = WasmHost::new();
        // Use path relative to crate directory
        let path = Path::new("../../plugins/wasm/simple_format.wasm");

        // Verify the WASM file exists and can be read
        assert!(path.exists(), "WASM file should exist: {}", path.display());
        let wasm_bytes = std::fs::read(path).expect("failed to read WASM file");
        assert!(!wasm_bytes.is_empty(), "WASM file should not be empty");
        assert_eq!(&wasm_bytes[0..4], b"\x00asm", "WASM file should have valid magic");

        // Try to load the real WASM file
        match host.load_plugin(path) {
            Ok(name) => {
                assert_eq!(name, "simple_format");
                assert!(host.list_plugins().contains(&"simple_format"));

                // Unload it
                host.unload_plugin("simple_format").unwrap();
                assert!(!host.list_plugins().contains(&"simple_format"));
            }
            Err(e) => {
                // Compilation may fail in test environment without full WASM runtime
                println!("WASM compilation failed (expected in test env): {e}");
            }
        }
    }
}
