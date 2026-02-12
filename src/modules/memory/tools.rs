use crate::tools::{ToolResult, ToolUse};
use crate::state::{MemoryImportance, MemoryItem, State};

pub fn execute_create(tool: &ToolUse, state: &mut State) -> ToolResult {
    let memories = match tool.input.get("memories").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Missing 'memories' array parameter".to_string(),
                is_error: true,
            }
        }
    };

    if memories.is_empty() {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: "Empty 'memories' array".to_string(),
            is_error: true,
        };
    }

    let mut created: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for memory_value in memories {
        let content = match memory_value.get("content").and_then(|v| v.as_str()) {
            Some(c) => c.to_string(),
            None => {
                errors.push("Missing 'content' in memory".to_string());
                continue;
            }
        };

        let importance = memory_value.get("importance")
            .and_then(|v| v.as_str())
            .and_then(MemoryImportance::from_str)
            .unwrap_or(MemoryImportance::Medium);

        let id = format!("M{}", state.next_memory_id);
        state.next_memory_id += 1;

        state.memories.push(MemoryItem {
            id: id.clone(),
            content: content.clone(),
            importance,
        });

        let preview = if content.len() > 40 {
            format!("{}...", &content[..content.floor_char_boundary(37)])
        } else {
            content
        };
        created.push(format!("{} [{}]: {}", id, importance.as_str(), preview));
    }

    let mut output = String::new();

    if !created.is_empty() {
        output.push_str(&format!("Created {} memory(s):\n{}", created.len(), created.join("\n")));
        // Update Memory panel timestamp
        state.touch_panel(crate::state::ContextType::Memory);
    }

    if !errors.is_empty() {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(&format!("Errors ({}):\n{}", errors.len(), errors.join("\n")));
    }

    ToolResult {
        tool_use_id: tool.id.clone(),
        content: output,
        is_error: created.is_empty(),
    }
}

pub fn execute_update(tool: &ToolUse, state: &mut State) -> ToolResult {
    let updates = match tool.input.get("updates").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Missing 'updates' array parameter".to_string(),
                is_error: true,
            }
        }
    };

    if updates.is_empty() {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: "Empty 'updates' array".to_string(),
            is_error: true,
        };
    }

    let mut updated: Vec<String> = Vec::new();
    let mut deleted: Vec<String> = Vec::new();
    let mut not_found: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for update_value in updates {
        let id = match update_value.get("id").and_then(|v| v.as_str()) {
            Some(i) => i,
            None => {
                errors.push("Missing 'id' in update".to_string());
                continue;
            }
        };

        // Check for deletion
        if let Some(importance_str) = update_value.get("importance").and_then(|v| v.as_str()) {
            if importance_str == "deleted" {
                let initial_len = state.memories.len();
                state.memories.retain(|m| m.id != id);
                if state.memories.len() < initial_len {
                    deleted.push(id.to_string());
                } else {
                    not_found.push(id.to_string());
                }
                continue;
            }
        }

        // Find and update the memory
        let memory = state.memories.iter_mut().find(|m| m.id == id);

        match memory {
            Some(m) => {
                let mut changes = Vec::new();

                if let Some(content) = update_value.get("content").and_then(|v| v.as_str()) {
                    m.content = content.to_string();
                    changes.push("content");
                }

                if let Some(importance_str) = update_value.get("importance").and_then(|v| v.as_str()) {
                    if let Some(importance) = MemoryImportance::from_str(importance_str) {
                        m.importance = importance;
                        changes.push("importance");
                    }
                }

                if !changes.is_empty() {
                    updated.push(format!("{}: {}", id, changes.join(", ")));
                }
            }
            None => {
                not_found.push(id.to_string());
            }
        }
    }

    // Update Memory panel timestamp if anything changed
    if !updated.is_empty() || !deleted.is_empty() {
        state.touch_panel(crate::state::ContextType::Memory);
    }

    let mut output = String::new();

    if !updated.is_empty() {
        output.push_str(&format!("Updated {}:\n{}", updated.len(), updated.join("\n")));
    }

    if !deleted.is_empty() {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(&format!("Deleted: {}", deleted.join(", ")));
    }

    if !not_found.is_empty() {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(&format!("Not found: {}", not_found.join(", ")));
    }

    if !errors.is_empty() {
        if !output.is_empty() {
            output.push_str("\n\n");
        }
        output.push_str(&format!("Errors:\n{}", errors.join("\n")));
    }

    ToolResult {
        tool_use_id: tool.id.clone(),
        content: output,
        is_error: updated.is_empty() && deleted.is_empty(),
    }
}
