//! Plugin management service — manages ALL plugins through Cordis.
//!
//! Both internal (Rust) and external (WASM) plugins are managed here.
//! WASM plugins are loaded via the internal WasmHost component and
//! registered as Cordis services.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

use cordis::{Context, Inject, PluginHandle, Service, service_sync};
use kaleido_traits::data::codec::FileCodecRegistry;
use kaleido_traits::plugins::events::{KaleidoEmitter, PluginInstalledEvent, PluginUninstalledEvent};
use kaleido_traits::plugins::tool::Tool;

pub mod capabilities;
pub mod wasm_host;

pub use kaleido_traits::plugin::{PluginError, PluginInfo, PluginResult, PluginService};
use wasm_host::WasmHost;

// ── PluginServiceImpl ────────────────────────────────────────────────────

/// Default implementation of [`PluginService`].
///
/// Uses [`WasmHost`] as an internal component to load/unload WASM plugins.
pub struct PluginServiceImpl {
    ctx: Context,
    codec_registry: Arc<dyn FileCodecRegistry>,
    /// Internal WASM host (not registered as Cordis service).
    wasm_host: Mutex<WasmHost>,
    plugins: RwLock<Vec<PluginInfo>>,
    plugin_tools: RwLock<HashMap<String, Vec<Arc<dyn Tool>>>>,
}

impl PluginServiceImpl {
    /// Creates a new plugin service.
    pub fn new(ctx: Context, codec_registry: Arc<dyn FileCodecRegistry>) -> Self {
        Self {
            ctx,
            codec_registry,
            wasm_host: Mutex::new(WasmHost::new()),
            plugins: RwLock::new(Vec::new()),
            plugin_tools: RwLock::new(HashMap::new()),
        }
    }

    /// Loads a WASM plugin from a `.wasm` file and registers it as a Cordis service.
    fn load_wasm_plugin(&self, path: &Path) -> PluginResult<String> {
        let mut host = self.wasm_host.lock().unwrap_or_else(|p| p.into_inner());
        host.load_plugin(path).map_err(|e| PluginError::LoadFailed {
            reason: e.to_string(),
        })
    }

    /// Unloads a WASM plugin and removes it from Cordis.
    fn unload_wasm_plugin(&self, name: &str) -> PluginResult<()> {
        let mut host = self.wasm_host.lock().unwrap_or_else(|p| p.into_inner());
        host.unload_plugin(name).map_err(|e| PluginError::LoadFailed {
            reason: e.to_string(),
        })?;
        Ok(())
    }
}

// ── Cordis integration ────────────────────────────────────────────────────

impl Service for PluginServiceImpl {
    const NAME: &'static str = "plugin_service";
}

/// Installs the `plugin_service` Cordis service.
pub fn plugin() -> PluginHandle {
    service_sync::<PluginServiceImpl, (), _>(
        "plugin_service",
        Inject::none(),
        |ctx, _config| {
            let codec_registry = Arc::new(crate::data::format::FormatRegistry::with_built_in());
            Ok(PluginServiceImpl::new(ctx, codec_registry))
        },
    )
}

/// Resolves the [`PluginService`] trait object from a Cordis context.
pub fn resolve_plugin_service(ctx: &Context) -> cordis::Result<Arc<dyn PluginService>> {
    let inner: Arc<dyn PluginService> = ctx.require::<PluginServiceImpl>("plugin_service")?;
    Ok(inner)
}

// ── PluginService trait implementation ────────────────────────────────────

impl PluginService for PluginServiceImpl {
    fn install(&self, dir: &Path) -> PluginResult<PluginInfo> {
        let installed_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let name = dir
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "plugin".into());

        if self.is_loaded(&name) {
            return Err(PluginError::AlreadyLoaded(name));
        }

        if !dir.is_dir() {
            return Err(PluginError::LoadFailed {
                reason: format!("not a directory: {}", dir.display()),
            });
        }

        // Check for WASM module
        let has_wasm = dir.join("plugin.wasm").exists() || {
            std::fs::read_dir(dir)
                .map(|rd| {
                    rd.flatten()
                        .any(|e| e.path().extension().map(|x| x == "wasm").unwrap_or(false))
                })
                .unwrap_or(false)
        };

        if !has_wasm {
            return Err(PluginError::LoadFailed {
                reason: format!("no .wasm module found in {}", dir.display()),
            });
        }

        // Load WASM plugin
        let wasm_name = self.load_wasm_plugin(&dir.join("plugin.wasm"))?;

        let info = PluginInfo {
            name: wasm_name.clone(),
            version: "0.1.0".into(),
            description: String::new(),
            author: None,
            kind: kaleido_traits::plugin::PluginKind::Wasm,
            tool_count: 0,
            installed_at,
        };

        self.plugins
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .push(info.clone());

        tracing::info!("WASM plugin installed: {}", info.name);

        self.ctx.emit_plugin_installed(PluginInstalledEvent {
            name: info.name.clone(),
            version: info.version.clone(),
            api_version: String::new(),
        });

        Ok(info)
    }

    fn uninstall(&self, name: &str) -> PluginResult<()> {
        let mut plugins = self.plugins.write().unwrap_or_else(|p| p.into_inner());
        let idx = plugins
            .iter()
            .position(|p| p.name == name)
            .ok_or_else(|| PluginError::NotFound(name.into()))?;
        plugins.remove(idx);
        drop(plugins);

        // Unload WASM plugin
        self.unload_wasm_plugin(name)?;

        // Reclaim the plugin's tools
        let _ = self
            .plugin_tools
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .remove(name);

        tracing::info!("plugin uninstalled: {name}");
        self.ctx.emit_plugin_uninstalled(PluginUninstalledEvent {
            name: name.into(),
        });
        Ok(())
    }

    fn list(&self) -> Vec<PluginInfo> {
        self.plugins.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn get(&self, name: &str) -> Option<PluginInfo> {
        self.plugins
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .find(|p| p.name == name)
            .cloned()
    }

    fn is_loaded(&self, name: &str) -> bool {
        self.plugins
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
            .any(|p| p.name == name)
    }

    fn plugin_count(&self) -> usize {
        self.plugins.read().unwrap_or_else(|p| p.into_inner()).len()
    }

    fn register_tool(&self, tool: Arc<dyn Tool>) -> PluginResult<()> {
        let mut tools = self.plugin_tools.write().unwrap_or_else(|p| p.into_inner());
        tools.entry("_global_".to_string()).or_default().push(tool);
        Ok(())
    }

    fn unregister_tool(&self, name: &str) -> PluginResult<()> {
        let mut tools = self.plugin_tools.write().unwrap_or_else(|p| p.into_inner());
        for (_, tool_list) in tools.iter_mut() {
            tool_list.retain(|t| t.name() != name);
        }
        Ok(())
    }

    fn register_panel(
        &self,
        panel: Arc<std::sync::Mutex<dyn kaleido_traits::Panel>>,
    ) -> PluginResult<()> {
        let _ = panel;
        Ok(())
    }

    fn register_codec(&self, codec: Arc<dyn kaleido_traits::FormatCodec>) -> PluginResult<()> {
        self.codec_registry.register_codec(codec);
        Ok(())
    }

    fn unregister_codec(&self, format: kaleido_traits::ImageFormat) -> PluginResult<()> {
        self.codec_registry.unregister_codec(format);
        Ok(())
    }

    fn register_shortcut(
        &self,
        _binding: kaleido_traits::ShortcutBinding,
    ) -> PluginResult<()> {
        Ok(())
    }

    fn unregister_shortcuts(&self, _plugin_name: &str) -> PluginResult<()> {
        Ok(())
    }

    fn emit(&self, name: &str, payload: serde_json::Value) -> PluginResult<()> {
        self.ctx
            .emit(name, [cordis::Value::new(payload)])
            .map_err(|e| PluginError::Cordis(e.to_string()))
    }

    fn plugin_tools(&self) -> Vec<Arc<dyn Tool>> {
        let tools = self.plugin_tools.read().unwrap_or_else(|p| p.into_inner());
        tools.values().flatten().cloned().collect()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_service() -> PluginServiceImpl {
        PluginServiceImpl::new(
            cordis::Context::new(),
            Arc::new(crate::data::format::FormatRegistry::with_built_in()),
        )
    }

    #[test]
    fn test_new_service_is_empty() {
        let svc = make_service();
        assert_eq!(svc.plugin_count(), 0);
        assert!(!svc.is_loaded("anything"));
    }

    #[test]
    fn test_list_and_get() {
        let svc = make_service();
        assert!(svc.list().is_empty());
        assert!(svc.get("nonexistent").is_none());
    }
}
