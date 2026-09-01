//! Plugin contracts — traits implemented by Kaleido plugins.
//!
//! Each module groups the contract with the service that consumes it:
//!
//! | Module      | Contract          | Primary consumer |
//! |-------------|-------------------|------------------|
//! | `tool`      | `Tool`, `ToolRegistry` | `PluginService` |
//! | `panel`     | `Panel`, `PanelRegistry` | `UiService` |
//! | `events`    | `KaleidoEmitter`, event types | `PluginService` |
//! | `category`  | `ToolCategory`    | `tool::Tool` trait |
//! | `cursor`    | `CursorType`      | `tool::Tool` trait |

pub mod category;
pub mod cursor;
pub mod events;
pub mod panel;
pub mod tool;

// Re-export key types for convenience
pub use tool::{NumericConstraints, ParamSchema, ParamType, Tool, ToolRegistry, ToolParams, ToolSchema};
pub use panel::{Panel, PanelButton, PanelContext, PanelElement, PanelRegistry, PanelSection};
pub use events::{KaleidoEmitter, TOOL_UPGRADED, ToolUpgradedEvent};
pub use category::ToolCategory;
pub use cursor::CursorType;
