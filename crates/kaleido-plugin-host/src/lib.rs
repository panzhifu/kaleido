//! WASM plugin host for Kaleido.
//!
//! This crate provides the infrastructure for loading and running tool
//! plugins. In the MVP it supports native (Rust) plugins through the
//! [`PluginManifest`] and [`PluginLoader`] traits. The WASM boundary is
//! designed ahead: the [`WasmPluginHost`] struct is the extension point
//! for adding a `wasmtime`-based runtime in a future version without
//! changing the public API.

use std::path::Path;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use kaleido_core::{Image, ImageResult};
use kaleido_traits::{ParamSchema, Tool, ToolParams, ToolSchema};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// PluginManifest — metadata describing a plugin
// ---------------------------------------------------------------------------

/// Metadata describing a plugin and its capabilities.
///
/// Loaded from a `plugin.json` manifest file alongside the plugin binary
/// (`.so`/`.dll` for native, `.wasm` for WASM).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin name (e.g. `"brightness"`).
    pub name: String,
    /// Plugin version (semver).
    pub version: String,
    /// API version this plugin targets (e.g. `"1.0"`).
    pub api_version: String,
    /// Plugin description.
    pub description: String,
    /// Plugin author.
    pub author: Option<String>,
    /// List of tools provided by this plugin.
    pub tools: Vec<ToolManifest>,
    /// Plugin kind — native shared library or WASM module.
    #[serde(default)]
    pub kind: PluginKind,
}

/// Whether the plugin is a native shared library or a WASM module.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    /// Native shared library (`.so` / `.dll` / `.dylib`).
    #[default]
    Native,
    /// WASM module (`.wasm`).
    Wasm,
}

/// Metadata for a single tool within a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolManifest {
    /// Tool identifier (e.g. `"brightness"`).
    pub name: String,
    /// Menu path (e.g. `"调整/亮度"`).
    pub menu_path: String,
    /// Human-readable description.
    pub description: String,
    /// Parameter schema for this tool.
    pub schema: Option<ToolSchema>,
}

// ---------------------------------------------------------------------------
// Plugin trait — implemented by loaded plugins
// ---------------------------------------------------------------------------

/// A loaded plugin that provides one or more tools.
///
/// This trait abstracts over the underlying plugin technology (native
/// shared library or WASM). The host calls [`Plugin::tools`] to discover
/// the tools and [`Plugin::shutdown`] for cleanup.
pub trait Plugin: Send + Sync + 'static {
    /// Returns the plugin's manifest.
    fn manifest(&self) -> &PluginManifest;

    /// Returns the tools provided by this plugin.
    fn tools(&self) -> Vec<Arc<dyn Tool>>;

    /// Shuts down the plugin, releasing any resources.
    fn shutdown(self: Box<Self>) -> Result<()>;
}

// ---------------------------------------------------------------------------
// PluginLoader — loads plugins from the filesystem
// ---------------------------------------------------------------------------

/// Loads plugins from the filesystem.
///
/// The loader reads the `plugin.json` manifest and instantiates the
/// appropriate plugin kind (native or WASM).
pub trait PluginLoader: Send + Sync + 'static {
    /// Loads a plugin from the given directory.
    ///
    /// The directory must contain a `plugin.json` manifest file and
    /// the plugin binary (`.so`, `.dll`, or `.wasm`).
    fn load(&self, dir: &Path) -> Result<Box<dyn Plugin>>;

    /// Returns whether this loader can handle the given plugin kind.
    fn supports(&self, kind: PluginKind) -> bool;
}

// ---------------------------------------------------------------------------
// NativePlugin — a plugin loaded from a manifest (MVP)
// ---------------------------------------------------------------------------

/// A native plugin whose tools are constructed from its manifest.
///
/// In the MVP, native plugins are loaded from a manifest file and
/// their tools are constructed dynamically. In a future version this
/// will load `.so`/`.dll` files via `libloading`.
pub struct NativePlugin {
    manifest: PluginManifest,
    tools: Vec<Arc<dyn Tool>>,
}

impl NativePlugin {
    /// Creates a new [`NativePlugin`] from a manifest and tools.
    pub fn new(manifest: PluginManifest, tools: Vec<Arc<dyn Tool>>) -> Self {
        Self { manifest, tools }
    }
}

impl Plugin for NativePlugin {
    fn manifest(&self) -> &PluginManifest {
        &self.manifest
    }

    fn tools(&self) -> Vec<Arc<dyn Tool>> {
        self.tools.clone()
    }

    fn shutdown(self: Box<Self>) -> Result<()> {
        info!("Shutting down native plugin: {}", self.manifest.name);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// DynamicTool — a tool constructed from a manifest + closure
// ---------------------------------------------------------------------------

/// A tool whose behavior is defined by a closure.
///
/// Used by the plugin host to construct tools from manifest data.
/// This is the primary way AI-generated tools and WASM-backed tools
/// integrate with the system.
pub struct DynamicTool {
    name: String,
    menu_path: String,
    description: String,
    schema: ToolSchema,
    apply_fn: Box<dyn Fn(&mut Image, &ToolParams) -> ImageResult<()> + Send + Sync>,
}

impl DynamicTool {
    /// Creates a new [`DynamicTool`].
    pub fn new(
        name: impl Into<String>,
        menu_path: impl Into<String>,
        description: impl Into<String>,
        schema: ToolSchema,
        apply_fn: impl Fn(&mut Image, &ToolParams) -> ImageResult<()> + Send + Sync + 'static,
    ) -> Self {
        Self {
            name: name.into(),
            menu_path: menu_path.into(),
            description: description.into(),
            schema,
            apply_fn: Box::new(apply_fn),
        }
    }
}

impl Tool for DynamicTool {
    fn name(&self) -> &str {
        &self.name
    }

    fn menu_path(&self) -> String {
        self.menu_path.clone()
    }

    fn description(&self) -> String {
        self.description.clone()
    }

    fn apply(&self, image: &mut Image, params: &ToolParams) -> ImageResult<()> {
        (self.apply_fn)(image, params)
    }

    fn schema(&self) -> ToolSchema {
        self.schema.clone()
    }
}

// ---------------------------------------------------------------------------
// PluginManager — manages loaded plugins
// ---------------------------------------------------------------------------

/// Manages the lifecycle of loaded plugins.
///
/// The manager loads plugins from directories, tracks them, and provides
/// access to all tools from all loaded plugins. It also emits
/// `plugin_installed` / `plugin_uninstalled` events through the Cordis
/// context.
pub struct PluginManager {
    plugins: Arc<RwLock<Vec<Box<dyn Plugin>>>>,
    loaders: Vec<Box<dyn PluginLoader>>,
}

impl PluginManager {
    /// Creates a new [`PluginManager`].
    pub fn new() -> Self {
        Self {
            plugins: Arc::new(RwLock::new(Vec::new())),
            loaders: Vec::new(),
        }
    }

    /// Registers a plugin loader.
    pub fn register_loader(&mut self, loader: Box<dyn PluginLoader>) {
        self.loaders.push(loader);
    }

    /// Loads a plugin from the given directory.
    pub fn load_plugin(&self, dir: &Path) -> Result<()> {
        // Read the manifest.
        let manifest_path = dir.join("plugin.json");
        let manifest_str = std::fs::read_to_string(&manifest_path)?;
        let manifest: PluginManifest = serde_json::from_str(&manifest_str)?;

        info!(
            "Loading plugin: {} v{} ({:?})",
            manifest.name, manifest.version, manifest.kind
        );

        // Find a loader that supports this kind.
        let loader = self
            .loaders
            .iter()
            .find(|l| l.supports(manifest.kind.clone()))
            .ok_or_else(|| {
                anyhow::anyhow!("No loader available for plugin kind: {:?}", manifest.kind)
            })?;

        let plugin = loader.load(dir)?;
        self.plugins.write().unwrap().push(plugin);
        info!("Plugin loaded successfully: {}", manifest.name);

        Ok(())
    }

    /// Returns all tools from all loaded plugins.
    pub fn all_tools(&self) -> Vec<Arc<dyn Tool>> {
        self.plugins
            .read()
            .unwrap()
            .iter()
            .flat_map(|p| p.tools())
            .collect()
    }

    /// Returns the number of loaded plugins.
    pub fn plugin_count(&self) -> usize {
        self.plugins.read().unwrap().len()
    }

    /// Shuts down all plugins.
    pub fn shutdown_all(&self) {
        let mut plugins = self.plugins.write().unwrap();
        for plugin in plugins.drain(..) {
            if let Err(e) = plugin.shutdown() {
                warn!("Error shutting down plugin: {}", e);
            }
        }
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// AIToolGenerator — creates tools dynamically (for AI-generated tools)
// ---------------------------------------------------------------------------

/// Generates tools dynamically, typically from AI output.
///
/// When the AI generates a new tool, it produces a JSON description of
/// the tool's name, parameters, and behavior. The [`AIToolGenerator`]
/// converts this into a [`DynamicTool`] and emits a `tool_upgraded` event.
pub struct AIToolGenerator;

impl AIToolGenerator {
    /// Creates a new tool from an AI-generated description.
    ///
    /// The `description` JSON should have the following structure:
    ///
    /// ```json
    /// {
    ///   "name": "vintage_effect",
    ///   "label": "复古效果",
    ///   "description": "Apply a vintage film effect",
    ///   "params": [
    ///     {
    ///       "name": "intensity",
    ///       "label": "强度",
    ///       "param_type": "float",
    ///       "default_value": 0.5,
    ///       "constraints": { "min": 0.0, "max": 1.0 },
    ///       "required": true
    ///     }
    ///   ]
    /// }
    /// ```
    ///
    /// The `apply_fn` closure implements the tool's behavior.
    pub fn create_tool(
        description: &serde_json::Value,
        apply_fn: impl Fn(&mut Image, &ToolParams) -> ImageResult<()> + Send + Sync + 'static,
    ) -> Result<DynamicTool> {
        let name = description["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Tool description missing 'name'"))?
            .to_string();
        let label = description["label"].as_str().unwrap_or(&name).to_string();
        let description_text = description["description"]
            .as_str()
            .unwrap_or("")
            .to_string();

        // Build schema from description.
        let mut schema = ToolSchema::new(&name, &label, &description_text);

        if let Some(params) = description["params"].as_array() {
            for param in params {
                let param_name = param["name"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Param missing 'name'"))?;
                let param_label = param["label"].as_str();
                let param_desc = param["description"].as_str();
                let param_type = match param["param_type"].as_str() {
                    Some("integer") => kaleido_traits::ParamType::Integer,
                    Some("unsigned") => kaleido_traits::ParamType::Unsigned,
                    Some("float") => kaleido_traits::ParamType::Float,
                    Some("boolean") => kaleido_traits::ParamType::Boolean,
                    Some("string") => kaleido_traits::ParamType::String,
                    Some("enum") => kaleido_traits::ParamType::Enum,
                    Some("color") => kaleido_traits::ParamType::Color,
                    _ => kaleido_traits::ParamType::String,
                };

                let mut param_schema = ParamSchema::new(param_name, param_type);
                if let Some(label) = param_label {
                    param_schema = param_schema.with_label(label);
                }
                if let Some(desc) = param_desc {
                    param_schema = param_schema.with_description(desc);
                }
                if let Some(default) = param.get("default_value") {
                    param_schema = param_schema.with_default(default.clone());
                }
                if let Some(constraints) = param.get("constraints") {
                    let c: kaleido_traits::NumericConstraints =
                        serde_json::from_value(constraints.clone())?;
                    param_schema = param_schema.with_constraints(c);
                }
                if param["required"].as_bool().unwrap_or(false) {
                    param_schema = param_schema.required();
                }

                schema = schema.with_param(param_schema);
            }
        }

        Ok(DynamicTool::new(
            name,
            label,
            description_text,
            schema,
            apply_fn,
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use kaleido_core::{Pixel, PixelFormat};
    use serde_json::json;

    #[test]
    fn test_dynamic_tool() {
        let tool = DynamicTool::new(
            "test",
            "Test",
            "A test tool",
            ToolSchema::new("test", "Test", "A test tool"),
            |image, _params| {
                image.fill(Pixel::rgb(0, 255, 0));
                Ok(())
            },
        );

        assert_eq!(tool.name(), "test");
        assert_eq!(tool.menu_path(), "Test");

        let mut image = Image::with_color(2, 2, PixelFormat::Rgba8, Pixel::rgb(0, 0, 0)).unwrap();
        tool.apply(&mut image, &json!({})).unwrap();
        assert_eq!(image.get_pixel(0, 0).unwrap(), Pixel::rgb(0, 255, 0));
    }

    #[test]
    fn test_ai_tool_generator() {
        let description = json!({
            "name": "sepia",
            "label": "怀旧色调",
            "description": "Apply sepia tone effect",
            "params": [
                {
                    "name": "intensity",
                    "label": "强度",
                    "param_type": "float",
                    "default_value": 0.7,
                    "required": true
                }
            ]
        });

        let tool = AIToolGenerator::create_tool(&description, |_image, _params| Ok(())).unwrap();

        assert_eq!(tool.name(), "sepia");
        assert_eq!(tool.menu_path(), "怀旧色调");
        assert_eq!(tool.schema().params.len(), 1);
        assert_eq!(tool.schema().params[0].name, "intensity");
    }

    #[test]
    fn test_plugin_manager() {
        let manager = PluginManager::new();
        assert_eq!(manager.plugin_count(), 0);
        assert!(manager.all_tools().is_empty());
    }

    #[test]
    fn test_plugin_manifest_serialization() {
        let manifest = PluginManifest {
            name: "test_plugin".into(),
            version: "1.0.0".into(),
            api_version: "1.0".into(),
            description: "A test plugin".into(),
            author: Some("Test Author".into()),
            tools: vec![ToolManifest {
                name: "test_tool".into(),
                menu_path: "Test/Tool".into(),
                description: "A test tool".into(),
                schema: None,
            }],
            kind: PluginKind::Native,
        };

        let json = serde_json::to_string_pretty(&manifest).unwrap();
        let deserialized: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, "test_plugin");
        assert_eq!(deserialized.tools.len(), 1);
    }
}
