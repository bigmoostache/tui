use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUse {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
    #[serde(default)]
    pub tool_name: String,
}

impl ToolResult {
    /// Create a new ToolResult. The tool_name will be populated by dispatch_tool.
    pub fn new(tool_use_id: String, content: String, is_error: bool) -> Self {
        Self { tool_use_id, content, is_error, tool_name: String::new() }
    }

    /// Create a new ToolResult with tool_name specified.
    pub fn with_name(tool_use_id: String, content: String, is_error: bool, tool_name: String) -> Self {
        Self { tool_use_id, content, is_error, tool_name }
    }
}
