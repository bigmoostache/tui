use serde_json::{json, Value};

use super::{ToolResult, ToolUse};
use crate::persistence::save_message;
use crate::state::{MessageStatus, State};

pub fn definition() -> Value {
    json!({
        "name": "set_message_status",
        "description": "Change a message's status to manage context size. Messages are identified by their short ID (e.g., 'U1', 'A1'). Status options: 'full' (show complete content), 'summarized' (show TL;DR only), 'forgotten' (exclude from context entirely).",
        "input_schema": {
            "type": "object",
            "properties": {
                "message_id": {
                    "type": "string",
                    "description": "The short message ID (e.g., 'U1' for user message 1, 'A3' for assistant message 3)"
                },
                "status": {
                    "type": "string",
                    "enum": ["full", "summarized", "forgotten"],
                    "description": "The new status: 'full' to show complete content, 'summarized' to show TL;DR, 'forgotten' to exclude from context"
                }
            },
            "required": ["message_id", "status"]
        }
    })
}

pub fn execute(tool: &ToolUse, state: &mut State) -> ToolResult {
    let message_id = match tool.input.get("message_id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Missing 'message_id' parameter".to_string(),
                is_error: true,
            }
        }
    };

    let status_str = match tool.input.get("status").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Missing 'status' parameter".to_string(),
                is_error: true,
            }
        }
    };

    let new_status = match status_str {
        "full" => MessageStatus::Full,
        "summarized" => MessageStatus::Summarized,
        "forgotten" => MessageStatus::Forgotten,
        _ => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: format!("Invalid status '{}'. Must be 'full', 'summarized', or 'forgotten'", status_str),
                is_error: true,
            }
        }
    };

    // Find message by id
    let msg = state.messages.iter_mut().find(|m| {
        m.id == message_id
    });

    match msg {
        Some(m) => {
            // Check if trying to summarize without a TL;DR
            if new_status == MessageStatus::Summarized && m.tl_dr.is_none() {
                return ToolResult {
                    tool_use_id: tool.id.clone(),
                    content: format!("Cannot set {} to 'summarized': no TL;DR available yet", message_id),
                    is_error: true,
                };
            }

            let old_status = m.status;
            m.status = new_status;

            // Save the updated message
            save_message(m);

            ToolResult {
                tool_use_id: tool.id.clone(),
                content: format!("Changed {} status from {:?} to {:?}", message_id, old_status, new_status),
                is_error: false,
            }
        }
        None => {
            ToolResult {
                tool_use_id: tool.id.clone(),
                content: format!("Message '{}' not found", message_id),
                is_error: true,
            }
        }
    }
}
