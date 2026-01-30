mod file;
mod glob;
mod message_status;
mod tree;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::state::State;

// Re-export public items
pub use file::{get_context_files, refresh_file_hashes};
pub use glob::{compute_glob_results, get_glob_context, refresh_glob_results};
pub use tree::generate_directory_tree;

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
    pub is_error: bool,
}

/// Tool definitions for the API
pub fn get_tool_definitions() -> Value {
    json!([
        file::definition_open_file(),
        file::definition_close_file(),
        tree::definition(),
        glob::definition(),
        message_status::definition(),
    ])
}

/// Execute a tool and return the result
pub fn execute_tool(tool: &ToolUse, state: &mut State) -> ToolResult {
    match tool.name.as_str() {
        "open_file" => file::execute_open(tool, state),
        "close_file" => file::execute_close(tool, state),
        "edit_tree_filter" => tree::execute(tool, state),
        "glob" => glob::execute(tool, state),
        "set_message_status" => message_status::execute(tool, state),
        _ => ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("Unknown tool: {}", tool.name),
            is_error: true,
        },
    }
}
