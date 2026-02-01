use super::{ToolResult, ToolUse};
use crate::state::{TodoItem, TodoStatus, State};

pub fn execute_create(tool: &ToolUse, state: &mut State) -> ToolResult {
    let todos = match tool.input.get("todos").and_then(|v| v.as_array()) {
        Some(arr) => arr,
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Missing 'todos' array parameter".to_string(),
                is_error: true,
            }
        }
    };

    if todos.is_empty() {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: "Empty 'todos' array".to_string(),
            is_error: true,
        };
    }

    let mut created: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for todo_value in todos {
        let name = match todo_value.get("name").and_then(|v| v.as_str()) {
            Some(n) => n.to_string(),
            None => {
                errors.push("Missing 'name' in todo".to_string());
                continue;
            }
        };

        let description = todo_value.get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let parent_id = todo_value.get("parent_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Validate parent exists if specified
        if let Some(ref pid) = parent_id {
            if !state.todos.iter().any(|t| t.id == *pid) {
                errors.push(format!("Parent '{}' not found for '{}'", pid, name));
                continue;
            }
        }

        let status = todo_value.get("status")
            .and_then(|v| v.as_str())
            .and_then(TodoStatus::from_str)
            .unwrap_or(TodoStatus::Pending);

        let id = format!("X{}", state.next_todo_id);
        state.next_todo_id += 1;

        state.todos.push(TodoItem {
            id: id.clone(),
            parent_id,
            name: name.clone(),
            description,
            status,
        });

        created.push(format!("{}: {}", id, name));
    }

    let mut output = String::new();

    if !created.is_empty() {
        output.push_str(&format!("Created {} todo(s):\n{}", created.len(), created.join("\n")));
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
        if let Some(status_str) = update_value.get("status").and_then(|v| v.as_str()) {
            if status_str == "deleted" {
                let initial_len = state.todos.len();
                state.todos.retain(|t| t.id != id);
                if state.todos.len() < initial_len {
                    deleted.push(id.to_string());
                } else {
                    not_found.push(id.to_string());
                }
                continue;
            }
        }

        // Pre-validate parent_id if specified
        if update_value.get("parent_id").is_some() {
            if let Some(pid) = update_value.get("parent_id").and_then(|v| v.as_str()) {
                if pid == id {
                    errors.push(format!("{}: cannot be its own parent", id));
                    continue;
                }
                if !state.todos.iter().any(|other| other.id == pid) {
                    errors.push(format!("{}: parent '{}' not found", id, pid));
                    continue;
                }
            }
        }

        // Find and update the todo
        let todo = state.todos.iter_mut().find(|t| t.id == id);

        match todo {
            Some(t) => {
                let mut changes = Vec::new();

                if let Some(name) = update_value.get("name").and_then(|v| v.as_str()) {
                    t.name = name.to_string();
                    changes.push("name");
                }

                if let Some(desc) = update_value.get("description").and_then(|v| v.as_str()) {
                    t.description = desc.to_string();
                    changes.push("description");
                }

                // Handle parent_id - can be string or null (already validated above)
                if update_value.get("parent_id").is_some() {
                    let new_parent = update_value.get("parent_id").and_then(|v| v.as_str());
                    if let Some(pid) = new_parent {
                        t.parent_id = Some(pid.to_string());
                    } else {
                        t.parent_id = None;
                    }
                    changes.push("parent");
                }

                if let Some(status_str) = update_value.get("status").and_then(|v| v.as_str()) {
                    if let Some(status) = TodoStatus::from_str(status_str) {
                        t.status = status;
                        changes.push("status");
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
