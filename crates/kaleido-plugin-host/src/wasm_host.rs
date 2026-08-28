//! WASM plugin host powered by [`wasmtime`].
//!
//! [`WasmPluginManager`] is a Cordis service that manages WASM plugin loading
//! and execution. It owns the [`Engine`] and a list of loaded
//! [`WasmPluginEntry`] instances. Each entry holds a [`Mutex<Store<()>>`]
//! (because `Store` is `!Sync`) and an [`Instance`] (which is `Send + Sync`).
//!
//! # Plugin ABI
//!
//! WASM plugins export: `alloc`, `free`, `plugin_init`, `plugin_manifest_json`,
//! `tool_count`, `tool_name`, `tool_menu_path`, `tool_description`,
//! `tool_schema_json`, `tool_apply`.
//!
//! Host provides: `host_log`, `host_emit_event`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};

use anyhow::{Context as _, Result};
use cordis::Context;
use kaleido_core::{Image, ImageError, ImageResult};
use kaleido_traits::{Tool, ToolParams, ToolRegistry, ToolSchema};
use cordis::Service;
use tracing::{debug, info};
use wasmtime::{self, Caller, Engine, Extern, Instance, Linker, Module, Store};

use crate::{PluginKind, PluginLoader, PluginManifest};

// ---------------------------------------------------------------------------
// WasmPluginEntry — a loaded WASM plugin (Store + Instance + metadata)
// ---------------------------------------------------------------------------

/// A loaded WASM plugin with its store, instance, and manifest.
pub struct WasmPluginEntry {
    pub manifest: PluginManifest,
    store: Mutex<Store<()>>,
    instance: Instance,
}

impl WasmPluginEntry {
    /// Loads a WASM plugin from a `.wasm` file.
    fn load(engine: &Engine, wasm_path: &Path) -> Result<Self> {
        let wasm_bytes = std::fs::read(wasm_path)
            .with_context(|| format!("Failed to read WASM file: {}", wasm_path.display()))?;

        let module = Module::new(engine, &wasm_bytes)
            .with_context(|| format!("Failed to compile WASM module: {}", wasm_path.display()))?;

        let mut store = Store::new(engine, ());
        let mut linker: Linker<()> = Linker::new(engine);
        link_host_functions(&mut linker)?;

        let instance = linker.instantiate(&mut store, &module).with_context(|| {
            format!("Failed to instantiate WASM module: {}", wasm_path.display())
        })?;

        // Call plugin_init.
        let init = instance
            .get_typed_func::<(), ()>(&mut store, "plugin_init")
            .context("WASM module must export 'plugin_init'")?;
        init.call(&mut store, ())?;

        // Read manifest from WASM memory.
        let manifest = Self::read_manifest(&instance, &mut store)?;

        info!(
            "WASM plugin loaded: {} v{} ({} tools)",
            manifest.name,
            manifest.version,
            manifest.tools.len()
        );

        Ok(Self {
            manifest,
            store: Mutex::new(store),
            instance,
        })
    }

    /// Reads the plugin manifest from WASM memory.
    fn read_manifest(instance: &Instance, store: &mut Store<()>) -> Result<PluginManifest> {
        let manifest_fn = instance
            .get_typed_func::<(), i32>(&mut *store, "plugin_manifest_json")
            .context("WASM module must export 'plugin_manifest_json'")?;
        let ptr = manifest_fn.call(&mut *store, ())?;
        let manifest_str = {
            let memory = instance
                .get_memory(&mut *store, "memory")
                .context("WASM module must export 'memory'")?;
            let data = memory.data(&mut *store);
            read_c_string(&data, ptr)
        };
        let manifest: PluginManifest = serde_json::from_str(&manifest_str)
            .context("Invalid manifest JSON from WASM plugin")?;
        Ok(manifest)
    }

    /// Allocates memory in WASM and returns the pointer.
    fn alloc(&self, size: i32) -> Result<i32> {
        let mut store = self.store.lock().unwrap();
        let alloc_fn = self
            .instance
            .get_typed_func::<i32, i32>(&mut *store, "alloc")
            .context("WASM module must export 'alloc'")?;
        let ptr = alloc_fn.call(&mut *store, size)?;
        Ok(ptr)
    }

    /// Frees memory in WASM.
    fn free(&self, ptr: i32, size: i32) -> Result<()> {
        let mut store = self.store.lock().unwrap();
        let free_fn = self
            .instance
            .get_typed_func::<(i32, i32), ()>(&mut *store, "free")
            .context("WASM module must export 'free'")?;
        free_fn.call(&mut *store, (ptr, size))?;
        Ok(())
    }

    /// Writes bytes to WASM memory at the given pointer.
    fn write_bytes(&self, ptr: i32, bytes: &[u8]) -> Result<()> {
        let mut store = self.store.lock().unwrap();
        let memory = self
            .instance
            .get_memory(&mut *store, "memory")
            .context("WASM module must export 'memory'")?;
        memory.write(&mut *store, ptr as usize, bytes)?;
        Ok(())
    }

    /// Reads bytes from WASM memory.
    fn read_bytes(&self, ptr: i32, len: usize) -> Result<Vec<u8>> {
        let mut store = self.store.lock().unwrap();
        let memory = self
            .instance
            .get_memory(&mut *store, "memory")
            .context("WASM module must export 'memory'")?;
        let data = memory.data(&*store);
        Ok(data[ptr as usize..ptr as usize + len].to_vec())
    }

    /// Reads a C-style null-terminated string from WASM memory.
    #[allow(dead_code)] // part of the WASM ABI helper set
    fn read_string(&self, ptr: i32) -> Result<String> {
        let mut store = self.store.lock().unwrap();
        let memory = self
            .instance
            .get_memory(&mut *store, "memory")
            .context("WASM module must export 'memory'")?;
        let data = memory.data(&*store);
        Ok(read_c_string(&data, ptr))
    }

    /// Returns the number of tools.
    fn tool_count(&self) -> i32 {
        let mut store = self.store.lock().unwrap();
        match self
            .instance
            .get_typed_func::<(), i32>(&mut *store, "tool_count")
        {
            Ok(fn_) => fn_.call(&mut *store, ()).unwrap_or(0),
            Err(_) => 0,
        }
    }

    /// Returns tool name at index.
    fn tool_name(&self, index: i32) -> String {
        self.call_string_fn("tool_name", index)
    }

    /// Returns tool menu path at index.
    fn tool_menu_path(&self, index: i32) -> String {
        self.call_string_fn("tool_menu_path", index)
    }

    /// Returns tool description at index.
    fn tool_description(&self, index: i32) -> String {
        self.call_string_fn("tool_description", index)
    }

    /// Returns tool schema at index.
    fn tool_schema(&self, index: i32) -> ToolSchema {
        let schema_json = self.call_string_fn("tool_schema_json", index);
        if schema_json.is_empty() {
            return ToolSchema::new(
                &self.tool_name(index),
                &self.tool_menu_path(index),
                &self.tool_description(index),
            );
        }
        serde_json::from_str(&schema_json).unwrap_or_else(|_| {
            ToolSchema::new(
                &self.tool_name(index),
                &self.tool_menu_path(index),
                &self.tool_description(index),
            )
        })
    }

    /// Calls a WASM function `(i32) -> i32` and reads the result string.
    fn call_string_fn(&self, fn_name: &str, index: i32) -> String {
        let mut store = self.store.lock().unwrap();
        let Ok(func) = self
            .instance
            .get_typed_func::<i32, i32>(&mut *store, fn_name)
        else {
            return String::new();
        };
        let Ok(ptr) = func.call(&mut *store, index) else {
            return String::new();
        };
        // Read string while still holding the lock.
        let Some(memory) = self.instance.get_memory(&mut *store, "memory") else {
            return String::new();
        };
        let data = memory.data(&*store);
        read_c_string(&data, ptr)
    }

    /// Calls `tool_apply` and returns the result code.
    fn call_tool_apply(
        &self,
        tool_index: i32,
        pixels_ptr: i32,
        width: i32,
        height: i32,
        params_ptr: i32,
    ) -> Result<i32> {
        let mut store = self.store.lock().unwrap();
        let apply_fn = self
            .instance
            .get_typed_func::<(i32, i32, i32, i32, i32), i32>(&mut *store, "tool_apply")
            .context("WASM module must export 'tool_apply'")?;
        let result = apply_fn.call(
            &mut *store,
            (tool_index, pixels_ptr, width, height, params_ptr),
        )?;
        Ok(result)
    }
}

// ---------------------------------------------------------------------------
// WasmTool — a tool backed by a WASM function
// ---------------------------------------------------------------------------

/// A tool whose `apply` function is implemented in WASM.
struct WasmTool {
    entry: Arc<WasmPluginEntry>,
    tool_index: i32,
    name: String,
    menu_path: String,
    description: String,
    schema: ToolSchema,
}

impl WasmTool {
    /// Creates a new [`WasmTool`] from a plugin entry.
    fn new(entry: Arc<WasmPluginEntry>, tool_index: i32) -> Self {
        let name = entry.tool_name(tool_index);
        let menu_path = entry.tool_menu_path(tool_index);
        let description = entry.tool_description(tool_index);
        let schema = entry.tool_schema(tool_index);
        Self {
            entry,
            tool_index,
            name,
            menu_path,
            description,
            schema,
        }
    }
}

impl Tool for WasmTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn menu_path(&self) -> String {
        self.menu_path.clone()
    }

    fn description(&self) -> String {
        self.description.clone()
    }

    fn schema(&self) -> ToolSchema {
        self.schema.clone()
    }

    fn apply(&self, image: &mut Image, params: &ToolParams) -> ImageResult<()> {
        let width = image.width();
        let height = image.height();
        let pixels = image.to_rgba_vec();

        debug!(
            "WasmTool::apply '{}' ({}x{}, {} bytes)",
            self.name,
            width,
            height,
            pixels.len()
        );

        // 1. Allocate WASM memory and write pixels.
        let pixels_ptr =
            self.entry
                .alloc(pixels.len() as i32)
                .map_err(|e| ImageError::OperationFailed {
                    reason: e.to_string(),
                })?;
        self.entry
            .write_bytes(pixels_ptr, &pixels)
            .map_err(|e| ImageError::OperationFailed {
                reason: e.to_string(),
            })?;

        // 2. Write params JSON.
        let params_json =
            serde_json::to_string(params).map_err(|e| ImageError::OperationFailed {
                reason: e.to_string(),
            })?;
        let params_ptr = self.entry.alloc(params_json.len() as i32).map_err(|e| {
            ImageError::OperationFailed {
                reason: e.to_string(),
            }
        })?;
        self.entry
            .write_bytes(params_ptr, params_json.as_bytes())
            .map_err(|e| ImageError::OperationFailed {
                reason: e.to_string(),
            })?;

        // 3. Call tool_apply.
        let result = self
            .entry
            .call_tool_apply(
                self.tool_index,
                pixels_ptr,
                width as i32,
                height as i32,
                params_ptr,
            )
            .map_err(|e| ImageError::OperationFailed {
                reason: e.to_string(),
            })?;

        if result != 0 {
            let _ = self.entry.free(pixels_ptr, pixels.len() as i32);
            let _ = self.entry.free(params_ptr, params_json.len() as i32);
            return Err(ImageError::OperationFailed {
                reason: format!("WASM tool '{}' returned error code: {}", self.name, result),
            });
        }

        // 4. Read back modified pixels.
        let modified = self
            .entry
            .read_bytes(pixels_ptr, pixels.len())
            .map_err(|e| ImageError::OperationFailed {
                reason: e.to_string(),
            })?;

        // 5. Free WASM memory.
        self.entry
            .free(pixels_ptr, pixels.len() as i32)
            .map_err(|e| ImageError::OperationFailed {
                reason: e.to_string(),
            })?;
        self.entry
            .free(params_ptr, params_json.len() as i32)
            .map_err(|e| ImageError::OperationFailed {
                reason: e.to_string(),
            })?;

        // 6. Update the image with modified pixels.
        *image =
            Image::from_rgba(width, height, modified).map_err(|e| ImageError::OperationFailed {
                reason: e.to_string(),
            })?;

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// WasmPluginManager — Cordis service that manages WASM plugins
// ---------------------------------------------------------------------------

/// Configuration for the WASM plugin manager.
#[derive(Debug, Clone, Default)]
pub struct WasmPluginConfig {
    /// Directories to scan for WASM plugins (`.wasm` files).
    pub plugin_dirs: Vec<PathBuf>,
}

impl WasmPluginConfig {
    /// Creates a default configuration with no plugin directories.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a directory to scan for WASM plugins.
    pub fn with_plugin_dir(mut self, dir: PathBuf) -> Self {
        self.plugin_dirs.push(dir);
        self
    }
}

// ---------------------------------------------------------------------------
// WasmPluginManager — Cordis service that manages WASM plugins
// ---------------------------------------------------------------------------

/// A Cordis service that manages WASM plugin loading and execution.
pub struct WasmPluginManager {
    engine: Engine,
    plugins: RwLock<Vec<Arc<WasmPluginEntry>>>,
    tools: RwLock<Vec<Arc<WasmTool>>>,
    /// Cordis context — reserved for host-side events from WASM plugins
    /// (`host_emit_event`) in a future iteration.
    #[allow(dead_code)]
    ctx: Context,
}

impl Service for WasmPluginManager {
    const NAME: &'static str = "wasm_plugin_manager";
}

impl WasmPluginManager {
    /// Creates a new [`WasmPluginManager`] with an optimized engine.
    pub fn new(ctx: Context) -> Result<Self> {
        let mut config = wasmtime::Config::new();
        config.cranelift_opt_level(wasmtime::OptLevel::Speed);
        config.allocation_strategy(wasmtime::InstanceAllocationStrategy::Pooling(
            wasmtime::PoolingAllocationConfig::default(),
        ));
        config.wasm_backtrace_details(wasmtime::WasmBacktraceDetails::Enable);

        let engine = Engine::new(&config)?;
        Ok(Self {
            engine,
            plugins: RwLock::new(Vec::new()),
            tools: RwLock::new(Vec::new()),
            ctx,
        })
    }

    /// Loads a WASM plugin from a directory.
    pub fn load_plugin(&self, dir: &Path) -> Result<()> {
        let wasm_path = if dir.join("plugin.wasm").exists() {
            dir.join("plugin.wasm")
        } else {
            std::fs::read_dir(dir)?
                .filter_map(|e| e.ok())
                .find(|e| e.path().extension().map(|e| e == "wasm").unwrap_or(false))
                .map(|e| e.path())
                .ok_or_else(|| anyhow::anyhow!("No .wasm file found in {}", dir.display()))?
        };

        let entry = WasmPluginEntry::load(&self.engine, &wasm_path)?;
        let tool_count = entry.tool_count();

        let mut plugins = self.plugins.write().unwrap();
        plugins.push(Arc::new(entry));

        // Create stable Arc<WasmTool> entries and store them.
        let mut tools = self.tools.write().unwrap();
        let entry_arc = plugins.last().unwrap().clone();
        for i in 0..tool_count {
            tools.push(Arc::new(WasmTool::new(entry_arc.clone(), i)));
        }

        info!(
            "WASM plugin loaded from {} ({} tools, {} total)",
            dir.display(),
            tool_count,
            tools.len()
        );

        Ok(())
    }

    /// Returns all tools from all loaded plugins.
    pub fn all_tools(&self) -> Vec<Arc<dyn Tool>> {
        let tools = self.tools.read().unwrap();
        tools.iter().map(|t| t.clone() as Arc<dyn Tool>).collect()
    }

    /// Registers all loaded tools with the given [`ToolRegistry`].
    pub fn register_all_tools(&self, registry: &dyn ToolRegistry) {
        let tools = self.tools.read().unwrap();
        for tool in tools.iter() {
            let tool: Arc<dyn Tool> = tool.clone();
            registry.register(Arc::downgrade(&tool));
        }
        info!(
            "Registered {} WASM tools with the tool registry",
            tools.len()
        );
    }

    /// Returns the number of loaded plugins.
    pub fn plugin_count(&self) -> usize {
        self.plugins.read().unwrap().len()
    }

    /// Returns the number of registered tools.
    pub fn tool_count(&self) -> usize {
        self.tools.read().unwrap().len()
    }

    /// Returns the underlying [`Engine`].
    pub fn engine(&self) -> &Engine {
        &self.engine
    }
}

// ---------------------------------------------------------------------------
// WasmPluginLoader — loads WASM plugins from the filesystem
// ---------------------------------------------------------------------------

/// Loads WASM plugins (`.wasm` files) from a directory.
pub struct WasmPluginLoader {
    engine: Arc<Engine>,
}

impl WasmPluginLoader {
    /// Creates a new [`WasmPluginLoader`] with the given engine.
    pub fn new(engine: Arc<Engine>) -> Self {
        Self { engine }
    }
}

impl PluginLoader for WasmPluginLoader {
    fn load(&self, dir: &Path) -> Result<Box<dyn crate::Plugin>> {
        let wasm_path = if dir.join("plugin.wasm").exists() {
            dir.join("plugin.wasm")
        } else {
            std::fs::read_dir(dir)?
                .filter_map(|e| e.ok())
                .find(|e| e.path().extension().map(|e| e == "wasm").unwrap_or(false))
                .map(|e| e.path())
                .ok_or_else(|| anyhow::anyhow!("No .wasm file found in {}", dir.display()))?
        };

        let entry = WasmPluginEntry::load(&self.engine, &wasm_path)?;
        Ok(Box::new(WasmPluginFromEntry::new(entry)))
    }

    fn supports(&self, kind: PluginKind) -> bool {
        matches!(kind, PluginKind::Wasm)
    }
}

/// Adapter: wraps a [`WasmPluginEntry`] as a [`crate::Plugin`].
struct WasmPluginFromEntry {
    entry: Arc<WasmPluginEntry>,
}

impl WasmPluginFromEntry {
    fn new(entry: WasmPluginEntry) -> Self {
        Self {
            entry: Arc::new(entry),
        }
    }
}

impl crate::Plugin for WasmPluginFromEntry {
    fn manifest(&self) -> &PluginManifest {
        &self.entry.manifest
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        let tool_count = self.entry.tool_count();
        (0..tool_count)
            .map(|i| Arc::new(WasmTool::new(Arc::clone(&self.entry), i)) as Arc<dyn Tool>)
            .collect()
    }

    fn shutdown(self: Box<Self>) -> Result<()> {
        info!("Shutting down WASM plugin: {}", self.entry.manifest.name);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Host functions
// ---------------------------------------------------------------------------

/// Links host functions into the linker.
fn link_host_functions(linker: &mut Linker<()>) -> Result<()> {
    linker.func_wrap(
        "env",
        "host_log",
        |mut caller: Caller<'_, ()>, level: i32, ptr: i32, len: i32| {
            if let Some(memory) = caller.get_export("memory").and_then(Extern::into_memory) {
                let data = memory.data(&caller);
                if let Ok(msg) = std::str::from_utf8(&data[ptr as usize..(ptr + len) as usize]) {
                    match level {
                        0 => tracing::error!("[wasm] {msg}"),
                        1 => tracing::warn!("[wasm] {msg}"),
                        2 => tracing::info!("[wasm] {msg}"),
                        3 => tracing::debug!("[wasm] {msg}"),
                        _ => tracing::trace!("[wasm] {msg}"),
                    }
                }
            }
        },
    )?;

    linker.func_wrap(
        "env",
        "host_emit_event",
        |mut caller: Caller<'_, ()>, ptr: i32, len: i32| {
            if let Some(memory) = caller.get_export("memory").and_then(Extern::into_memory) {
                let data = memory.data(&caller);
                if let Ok(event_json) =
                    std::str::from_utf8(&data[ptr as usize..(ptr + len) as usize])
                {
                    tracing::info!("[wasm event] {event_json}");
                }
            }
        },
    )?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Utilities
// ---------------------------------------------------------------------------

/// Reads a C-style null-terminated string from a byte slice.
fn read_c_string(data: &[u8], ptr: i32) -> String {
    let start = ptr as usize;
    if start >= data.len() {
        return String::new();
    }
    let max_len = (data.len() - start).min(65536);
    let end = start
        + data[start..start + max_len]
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(max_len);
    String::from_utf8_lossy(&data[start..end]).into_owned()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_traits::{NumericConstraints, ParamSchema, ParamType};
    use serde_json::json;

    #[test]
    fn test_wasm_plugin_manager_creation() {
        let ctx = cordis::Context::new();
        let manager = WasmPluginManager::new(ctx);
        assert!(manager.is_ok(), "WASM plugin manager should initialize");
        assert_eq!(manager.unwrap().plugin_count(), 0);
    }

    #[test]
    fn test_wasm_plugin_loader_supports_wasm() {
        let engine = Arc::new(Engine::default());
        let loader = WasmPluginLoader::new(engine);
        assert!(loader.supports(PluginKind::Wasm));
        assert!(!loader.supports(PluginKind::Native));
    }

    #[test]
    fn test_wasm_tool_metadata() {
        let schema = ToolSchema::new("test_tool", "Test/Tool", "A test tool").with_param(
            ParamSchema::new("amount", ParamType::Integer)
                .with_default(json!(10))
                .required(),
        );

        let json = schema.to_json_schema();
        assert_eq!(json["type"], "object");
        assert!(json["properties"]["amount"].is_object());
    }

    #[test]
    fn test_wasm_tool_schema_validation() {
        let schema = ToolSchema::new("test", "Test", "A test").with_param(
            ParamSchema::new("value", ParamType::Integer)
                .with_constraints(NumericConstraints {
                    min: Some(0),
                    max: Some(100),
                    step: Some(1),
                })
                .required(),
        );

        schema.validate_params(&json!({ "value": 50 })).unwrap();
        assert!(schema.validate_params(&json!({ "value": 200 })).is_err());
        assert!(schema.validate_params(&json!({})).is_err());
    }

    #[test]
    fn test_read_c_string() {
        let mut data = vec![0u8; 100];
        let hello = b"hello\0";
        data[..hello.len()].copy_from_slice(hello);

        assert_eq!(read_c_string(&data, 0), "hello");
        assert_eq!(read_c_string(&data, 10), "");
        assert_eq!(read_c_string(&data, 100), "");
    }
}
