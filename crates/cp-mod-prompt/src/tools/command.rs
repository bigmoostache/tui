use crate::storage;
use crate::types::{PromptItem, PromptState, PromptType};
use cp_base::state::{ContextType, State};
use cp_base::tools::{ToolResult, ToolUse};

pub fn create(tool: &ToolUse, state: &mut State) -> ToolResult {
    let name = tool.input.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let description = tool.input.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let content = tool.input.get("content").and_then(|v| v.as_str()).unwrap_or("").to_string();

    if name.is_empty() {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: "Missing required 'name' parameter".to_string(),
            is_error: true, ..Default::default()
        };
    }

    if content.is_empty() {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: "Missing required 'content' parameter".to_string(),
            is_error: true, ..Default::default()
        };
    }

    let id = storage::slugify(&name);
    if id.is_empty() {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: "Name must contain at least one alphanumeric character".to_string(),
            is_error: true, ..Default::default()
        };
    }

    if PromptState::get(state).commands.iter().any(|c| c.id == id) {
        return ToolResult {
            tool_use_id: tool.id.clone(), ..Default::default()
            content: format!("Command with ID '{}' already exists", id),
            is_error: true,
        };
    }

    let item = PromptItem {
        id: id.clone(),
        name: name.clone(),
        description,
        content,
        prompt_type: PromptType::Command,
        is_builtin: false,
    };

    storage::save_prompt_to_dir(&storage::dir_for(PromptType::Command), &item);
    PromptState::get_mut(state).commands.push(item);

    state.touch_panel(ContextType::new(ContextType::LIBRARY));

    ToolResult {
        tool_use_id: tool.id.clone(), ..Default::default()
        content: format!("Created command '{}' with ID '{}' (use as /{})", name, id, id),
        is_error: false,
    }
}

pub fn edit(tool: &ToolUse, state: &mut State) -> ToolResult {
    let id = match tool.input.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Missing required 'id' parameter".to_string(),
                is_error: true, ..Default::default()
            };
        }
    };

    let cmd = match PromptState::get_mut(state).commands.iter_mut().find(|c| c.id == id) {
        Some(c) => c,
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(), ..Default::default()
                content: format!("Command '{}' not found", id),
                is_error: true,
            };
        }
    };

    if cmd.is_builtin {
        return ToolResult {
            tool_use_id: tool.id.clone(), ..Default::default()
            content: format!("Cannot edit built-in command '{}'", id),
            is_error: true,
        };
    }

    let mut changes = Vec::new();

    if let Some(name) = tool.input.get("name").and_then(|v| v.as_str()) {
        cmd.name = name.to_string();
        changes.push("name");
    }

    if let Some(desc) = tool.input.get("description").and_then(|v| v.as_str()) {
        cmd.description = desc.to_string();
        changes.push("description");
    }

    if let Some(content) = tool.input.get("content").and_then(|v| v.as_str()) {
        cmd.content = content.to_string();
        changes.push("content");
    }

    if changes.is_empty() {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: "No changes specified".to_string(),
            is_error: true, ..Default::default()
        };
    }

    let cmd_clone = cmd.clone();
    storage::save_prompt_to_dir(&storage::dir_for(PromptType::Command), &cmd_clone);

    state.touch_panel(ContextType::new(ContextType::LIBRARY));

    ToolResult {
        tool_use_id: tool.id.clone(), ..Default::default()
        content: format!("Updated command '{}': {}", id, changes.join(", ")),
        is_error: false,
    }
}

pub fn delete(tool: &ToolUse, state: &mut State) -> ToolResult {
    let id = match tool.input.get("id").and_then(|v| v.as_str()) {
        Some(id) => id,
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Missing required 'id' parameter".to_string(),
                is_error: true, ..Default::default()
            };
        }
    };

    if let Some(cmd) = PromptState::get(state).commands.iter().find(|c| c.id == id)
        && cmd.is_builtin
    {
        return ToolResult {
            tool_use_id: tool.id.clone(), ..Default::default()
            content: format!("Cannot delete built-in command '{}'", id),
            is_error: true,
        };
    }

    let ps = PromptState::get_mut(state);
    let idx = match ps.commands.iter().position(|c| c.id == id) {
        Some(i) => i,
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(), ..Default::default()
                content: format!("Command '{}' not found", id),
                is_error: true,
            };
        }
    };

    let cmd = ps.commands.remove(idx);
    storage::delete_prompt_from_dir(&storage::dir_for(PromptType::Command), id);

    state.touch_panel(ContextType::new(ContextType::LIBRARY));

    ToolResult {
        tool_use_id: tool.id.clone(), ..Default::default()
        content: format!("Deleted command '{}' ({})", cmd.name, id),
        is_error: false,
    }
}
