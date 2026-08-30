//! Tool state management.

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ToolsState {
    pub active_tool: Option<String>,
    pub params: serde_json::Value,
}

impl Default for ToolsState {
    fn default() -> Self {
        Self {
            active_tool: None,
            params: serde_json::json!({}),
        }
    }
}

impl ToolsState {
    pub fn select(&mut self, name: &str) {
        self.active_tool = Some(name.to_string());
        self.params = serde_json::json!({});
    }

    #[allow(dead_code)]
    pub fn set_params(&mut self, params: serde_json::Value) {
        self.params = params;
    }
}
