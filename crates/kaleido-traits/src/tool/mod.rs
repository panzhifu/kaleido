//! Tool plugin contracts.
//!
//! A **tool** is the unit of user-invokable functionality (brightness,
//! invert, resize, …). Tools are provided by Cordis plugins and registered
//! into a [`ToolRegistry`] service when their fiber activates. The host
//! (CLI menu, GUI menu) builds its command surface from the registry, so
//! installing/uninstalling a plugin adds/removes commands dynamically.
//!
//! # Parameter Schema
//!
//! Each tool can declare a [`ToolSchema`] describing its parameters. The host
//! uses this schema to auto-generate UI forms, validate input, and serialize
//! params for the WASM boundary. See [`ToolSchema`] and [`ParamSchema`] for
//! details.

use std::sync::{Arc, Weak};

use kaleido_core::{ImageError, ImageResult, TiledImage};
use serde_json::Value;

use crate::category::ToolCategory;
use crate::cursor::CursorType;

// ---------------------------------------------------------------------------
// ToolParams
// ---------------------------------------------------------------------------

/// Parameters for a tool invocation, carried as JSON.
///
/// JSON keeps the contract open: plugins define their own argument schema,
/// hosts can round-trip params through the UI, and the future WASM boundary
/// (wit) can serialize the same JSON.
pub type ToolParams = Value;

// ---------------------------------------------------------------------------
// ParamSchema — describes a single tool parameter
// ---------------------------------------------------------------------------

/// Type of a tool parameter, used for UI form generation and validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParamType {
    /// A signed integer (e.g. brightness adjustment -255..255).
    Integer,
    /// An unsigned integer (e.g. width, height).
    Unsigned,
    /// A floating-point number (e.g. scale factor 0.0..1.0).
    Float,
    /// A boolean toggle.
    Boolean,
    /// A free-text string.
    String,
    /// A choice from a fixed set of options.
    Enum,
    /// A color value (hex string like `#RRGGBB` or `#RRGGBBAA`).
    Color,
}

/// Validation constraints for a numeric parameter.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct NumericConstraints {
    /// Minimum value (inclusive).
    pub min: Option<i64>,
    /// Maximum value (inclusive).
    pub max: Option<i64>,
    /// Step increment for sliders/spinners.
    pub step: Option<i64>,
}

/// Describes a single parameter accepted by a tool.
///
/// The host uses this to auto-generate UI widgets, validate input, and
/// document the tool. All fields are optional beyond `name` and `param_type`
/// so simple tools can declare just what they need.
///
/// # Example
///
/// ```
/// use kaleido_traits::{ParamSchema, ParamType};
///
/// let brightness_param = ParamSchema {
///     name: "value".into(),
///     label: Some("亮度".into()),
///     param_type: ParamType::Integer,
///     description: Some("亮度调整值 (-255..255)".into()),
///     default_value: Some(serde_json::json!(0)),
///     constraints: Some(kaleido_traits::NumericConstraints {
///         min: Some(-255),
///         max: Some(255),
///         step: Some(1),
///     }),
///     enum_options: None,
///     required: true,
/// };
/// ```
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ParamSchema {
    /// Parameter key (used as the JSON field name).
    pub name: String,
    /// Human-readable label for the UI.
    pub label: Option<String>,
    /// Parameter type, determines the UI widget.
    pub param_type: ParamType,
    /// Description shown in tooltips / help text.
    pub description: Option<String>,
    /// Default value used when the user doesn't provide one.
    pub default_value: Option<Value>,
    /// Numeric constraints (min/max/step) for `Integer` / `Unsigned` / `Float`.
    pub constraints: Option<NumericConstraints>,
    /// Allowed values for `Enum` type: list of `(value, label)` pairs.
    pub enum_options: Option<Vec<(String, String)>>,
    /// Whether this parameter must be provided.
    ///
    /// `false` means the tool has a sensible default (or the parameter
    /// is optional).
    pub required: bool,
}

impl ParamSchema {
    /// Creates a new [`ParamSchema`] with the given name and type.
    pub fn new(name: impl Into<String>, param_type: ParamType) -> Self {
        Self {
            name: name.into(),
            label: None,
            param_type,
            description: None,
            default_value: None,
            constraints: None,
            enum_options: None,
            required: false,
        }
    }

    /// Sets the human-readable label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Sets the description.
    pub fn with_description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Sets the default value.
    pub fn with_default(mut self, value: Value) -> Self {
        self.default_value = Some(value);
        self
    }

    /// Sets numeric constraints.
    pub fn with_constraints(mut self, constraints: NumericConstraints) -> Self {
        self.constraints = Some(constraints);
        self
    }

    /// Sets enum options.
    pub fn with_enum_options(mut self, options: Vec<(String, String)>) -> Self {
        self.enum_options = Some(options);
        self
    }

    /// Marks this parameter as required.
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Validates a JSON value against this parameter schema.
    ///
    /// Returns `Ok(())` if the value is valid, or `Err` with a
    /// human-readable error message.
    pub fn validate(&self, value: &Value) -> Result<(), String> {
        match self.param_type {
            ParamType::Integer => {
                if !value.is_i64() && !value.is_u64() {
                    return Err(format!("Parameter '{}' must be an integer", self.name));
                }
                if let Some(ref c) = self.constraints {
                    let v = value.as_i64().unwrap();
                    if let Some(min) = c.min {
                        if v < min {
                            return Err(format!("Parameter '{}' must be >= {}", self.name, min));
                        }
                    }
                    if let Some(max) = c.max {
                        if v > max {
                            return Err(format!("Parameter '{}' must be <= {}", self.name, max));
                        }
                    }
                }
            }
            ParamType::Unsigned => {
                if !value.is_u64() {
                    return Err(format!(
                        "Parameter '{}' must be a non-negative integer",
                        self.name
                    ));
                }
                if let Some(ref c) = self.constraints {
                    let v = value.as_u64().unwrap() as i64;
                    if let Some(min) = c.min {
                        if v < min {
                            return Err(format!("Parameter '{}' must be >= {}", self.name, min));
                        }
                    }
                    if let Some(max) = c.max {
                        if v > max {
                            return Err(format!("Parameter '{}' must be <= {}", self.name, max));
                        }
                    }
                }
            }
            ParamType::Float => {
                if !value.is_f64() && !value.is_i64() && !value.is_u64() {
                    return Err(format!("Parameter '{}' must be a number", self.name));
                }
            }
            ParamType::Boolean => {
                if !value.is_boolean() {
                    return Err(format!("Parameter '{}' must be a boolean", self.name));
                }
            }
            ParamType::String | ParamType::Color => {
                if !value.is_string() {
                    return Err(format!("Parameter '{}' must be a string", self.name));
                }
            }
            ParamType::Enum => {
                if let Some(ref options) = self.enum_options {
                    let s = value
                        .as_str()
                        .ok_or_else(|| format!("Parameter '{}' must be a string", self.name))?;
                    if !options.iter().any(|(v, _)| v == s) {
                        return Err(format!(
                            "Parameter '{}' must be one of: {}",
                            self.name,
                            options
                                .iter()
                                .map(|(v, _)| v.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// ToolSchema — full parameter description for a tool
// ---------------------------------------------------------------------------

/// Complete parameter schema for a tool.
///
/// Returned by [`Tool::schema`] so the host can auto-generate UI forms,
/// validate input, and document the tool.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ToolSchema {
    /// Tool name (matches [`Tool::name`]).
    pub tool_name: String,
    /// Human-readable label for menus.
    pub label: String,
    /// Description shown in tooltips.
    pub description: String,
    /// Ordered list of parameters.
    pub params: Vec<ParamSchema>,
}

impl ToolSchema {
    /// Creates a new [`ToolSchema`].
    pub fn new(
        tool_name: impl Into<String>,
        label: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            tool_name: tool_name.into(),
            label: label.into(),
            description: description.into(),
            params: Vec::new(),
        }
    }

    /// Adds a parameter to the schema.
    pub fn with_param(mut self, param: ParamSchema) -> Self {
        self.params.push(param);
        self
    }

    /// Validates a complete set of parameters against this schema.
    ///
    /// Checks that all required parameters are present and all values
    /// pass their individual validation.
    pub fn validate_params(&self, params: &ToolParams) -> Result<(), String> {
        let obj = params
            .as_object()
            .ok_or("Parameters must be a JSON object")?;

        for param_schema in &self.params {
            if param_schema.required && !obj.contains_key(&param_schema.name) {
                return Err(format!(
                    "Missing required parameter: '{}'",
                    param_schema.name
                ));
            }
            if let Some(value) = obj.get(&param_schema.name) {
                param_schema.validate(value)?;
            }
        }
        Ok(())
    }

    /// Fills in default values for missing parameters.
    ///
    /// Returns a new `ToolParams` with defaults applied.
    pub fn apply_defaults(&self, params: &ToolParams) -> ToolParams {
        let mut result = serde_json::Map::new();

        // Start with existing params.
        if let Some(obj) = params.as_object() {
            for (k, v) in obj {
                result.insert(k.clone(), v.clone());
            }
        }

        // Fill in defaults for missing params.
        for param_schema in &self.params {
            if !result.contains_key(&param_schema.name) {
                if let Some(ref default) = param_schema.default_value {
                    result.insert(param_schema.name.clone(), default.clone());
                }
            }
        }

        Value::Object(result)
    }

    /// Generates a JSON Schema (draft 2020-12) for this tool.
    ///
    /// Useful for integrating with JSON-Schema-based UI generators.
    pub fn to_json_schema(&self) -> Value {
        let mut properties = serde_json::Map::new();
        let mut required = Vec::new();

        for param in &self.params {
            let mut prop = serde_json::Map::new();

            // Type mapping.
            match param.param_type {
                ParamType::Integer | ParamType::Unsigned => {
                    prop.insert("type".into(), Value::String("integer".into()));
                }
                ParamType::Float => {
                    prop.insert("type".into(), Value::String("number".into()));
                }
                ParamType::Boolean => {
                    prop.insert("type".into(), Value::String("boolean".into()));
                }
                ParamType::String | ParamType::Color => {
                    prop.insert("type".into(), Value::String("string".into()));
                }
                ParamType::Enum => {
                    if let Some(ref options) = param.enum_options {
                        let enum_values: Vec<Value> = options
                            .iter()
                            .map(|(v, _)| Value::String(v.clone()))
                            .collect();
                        prop.insert("enum".into(), Value::Array(enum_values));
                    } else {
                        prop.insert("type".into(), Value::String("string".into()));
                    }
                }
            }

            // Description.
            if let Some(ref desc) = param.description {
                prop.insert("description".into(), Value::String(desc.clone()));
            }

            // Default.
            if let Some(ref default) = param.default_value {
                prop.insert("default".into(), default.clone());
            }

            // Numeric constraints.
            if let Some(ref c) = param.constraints {
                if let Some(min) = c.min {
                    prop.insert("minimum".into(), Value::from(min));
                }
                if let Some(max) = c.max {
                    prop.insert("maximum".into(), Value::from(max));
                }
            }

            // Label as title.
            if let Some(ref label) = param.label {
                prop.insert("title".into(), Value::String(label.clone()));
            }

            properties.insert(param.name.clone(), Value::Object(prop));

            if param.required {
                required.push(Value::String(param.name.clone()));
            }
        }

        serde_json::json!({
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": self.label,
            "description": self.description,
            "type": "object",
            "properties": properties,
            "required": required,
        })
    }
}

// ---------------------------------------------------------------------------
// Tool
// ---------------------------------------------------------------------------

/// A user-invokable image operation provided by a plugin.
pub trait Tool: Send + Sync + 'static {
    /// Stable identifier used for lookups and registration (e.g. `"brightness"`).
    fn name(&self) -> &str;

    /// Slash-separated menu path (e.g. `"调整/亮度"`).
    fn menu_path(&self) -> String;

    /// Human-readable description shown in tooltips/help.
    fn description(&self) -> String;

    /// Applies this tool's transformation to the image.
    ///
    /// The host is responsible for loading the image, recording history and
    /// saving — the tool only mutates pixel data.
    fn apply(&self, image: &mut TiledImage, params: &ToolParams) -> ImageResult<()>;

    /// Whether this tool operates on the whole document (layer stack)
    /// through [`Self::apply_to_document`] instead of a single image.
    ///
    /// Hosts call [`Self::apply_to_document`] when this returns `true`,
    /// otherwise they fall back to [`Self::apply`]. Defaults to `false`,
    /// so existing single-image tools are unaffected.
    fn supports_layers(&self) -> bool {
        false
    }

    /// Returns the parameter schema for this tool.
    ///
    /// The schema describes the parameters this tool accepts, including
    /// types, defaults, constraints, and UI labels. Hosts use this to
    /// auto-generate UI forms, validate input, and document the tool.
    ///
    /// The default implementation returns an empty schema (no parameters).
    /// Override this to declare your tool's parameters.
    fn schema(&self) -> ToolSchema {
        ToolSchema::new(self.name(), self.menu_path(), self.description())
    }

    /// Returns an icon key for this tool.
    ///
    /// The host uses this to look up an icon in its icon set. The string
    /// is an opaque key (e.g. `"brightness"`, `"crop"`) — the host
    /// maps it to the actual icon. Returning `None` falls back to the
    /// category default icon.
    fn icon(&self) -> Option<&str> {
        None
    }

    /// Returns the functional category this tool belongs to.
    ///
    /// The host uses this for toolbar grouping and panel selection.
    fn category(&self) -> ToolCategory {
        ToolCategory::Other
    }

    /// Returns whether the tool can currently be used.
    ///
    /// The host calls this to enable/disable menu items and toolbar
    /// buttons. The default implementation always returns `true`.
    ///
    /// Common reasons to return `false`:
    /// - No image is loaded (crop, flip, adjustments).
    /// - No selection exists (fill inside selection, "via copy").
    /// - The current layer is locked.
    fn is_enabled(&self) -> bool {
        true
    }

    /// Returns the cursor this tool wants the host to display.
    ///
    /// The host is responsible for actually showing the cursor — the
    /// plugin only declares its preference. The default returns
    /// [`CursorType::Default`].
    fn cursor(&self) -> CursorType {
        CursorType::Default
    }
}

// ---------------------------------------------------------------------------
// ToolRegistry
// ---------------------------------------------------------------------------

/// Registry of tools currently provided by active plugins.
///
/// Implementations hold weak references so tools disappear automatically
/// when their providing plugin is disposed (the registry filters dead weak
/// pointers on read).
pub trait ToolRegistry: Send + Sync + 'static {
    /// Registers a tool. Held weakly — the plugin keeps the strong `Arc`
    /// alive for as long as its fiber is active.
    fn register(&self, tool: Weak<dyn Tool>);

    /// Removes the tool with the given name, if present.
    fn unregister(&self, name: &str);

    /// Returns all live tools (dead weak pointers are filtered out).
    fn tools(&self) -> Vec<Arc<dyn Tool>>;

    /// Looks up a live tool by name.
    fn get(&self, name: &str) -> Option<Arc<dyn Tool>>;
}

/// Resolves the tool registry from a Cordis context.
///
/// The registry service is provided as `Arc<dyn ToolRegistry>` (a sized
/// value), so plugins can look it up without depending on the concrete
/// implementation crate.
pub fn resolve_tool_registry(ctx: &cordis::Context) -> cordis::Result<Arc<dyn ToolRegistry>> {
    let inner = ctx
        .get::<Arc<dyn ToolRegistry>>("tool_registry")?
        .ok_or_else(|| {
            cordis::CordisError::with_message(
                cordis::ErrorCode::MissingService,
                "tool_registry service is not available",
            )
        })?;
    Ok(inner.as_ref().clone())
}
