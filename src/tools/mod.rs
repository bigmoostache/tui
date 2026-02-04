mod close_context;
mod create;
mod edit_file;
mod file;
mod git;
mod glob;
mod grep;
mod manage_tools;
mod memory;
mod message_status;
mod scratchpad;
mod system;
mod overview;
mod tmux;
mod todo;
pub mod tree;

pub use manage_tools::MANAGE_TOOLS_ID;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::state::{estimate_tokens, ContextType, MessageStatus, State};

// Re-export public items used by cache module
pub use glob::compute_glob_results;
pub use grep::compute_grep_results;

/// Refresh token count for the Conversation context element
pub fn refresh_conversation_context(state: &mut State) {
    // Calculate total tokens from all active messages
    let total_tokens: usize = state.messages.iter()
        .filter(|m| m.status != MessageStatus::Deleted)
        .map(|m| {
            match m.status {
                MessageStatus::Summarized => m.tl_dr_token_count.max(estimate_tokens(m.tl_dr.as_deref().unwrap_or(""))),
                _ => m.content_token_count.max(estimate_tokens(&m.content)),
            }
        })
        .sum();

    // Update the Conversation context element's token count
    for ctx in &mut state.context {
        if ctx.context_type == ContextType::Conversation {
            ctx.token_count = total_tokens;
            break;
        }
    }
}

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

/// Execute a tool and return the result
pub fn execute_tool(tool: &ToolUse, state: &mut State) -> ToolResult {
    match tool.name.as_str() {
        // File tools
        "file_open" => file::execute_open(tool, state),
        "file_edit" => edit_file::execute_edit(tool, state),
        "file_create" => edit_file::execute_create(tool, state),
        "file_batch_create" => create::execute(tool, state),
        "file_glob" => glob::execute(tool, state),
        "file_grep" => grep::execute(tool, state),
        
        // Tree tools
        "tree_filter" => tree::execute_edit_filter(tool, state),
        "tree_toggle" => tree::execute_toggle_folders(tool, state),
        "tree_describe" => tree::execute_describe_files(tool, state),
        
        // Context tools
        "context_close" => close_context::execute(tool, state),
        "context_message_status" => message_status::execute(tool, state),
        
        // Console tools
        "console_create" => tmux::execute_create_pane(tool, state),
        "console_edit" => tmux::execute_edit_config(tool, state),
        "console_send_keys" => tmux::execute_send_keys(tool, state),
        "console_sleep" => tmux::execute_sleep(tool),
        
        // Todo tools
        "todo_create" => todo::execute_create(tool, state),
        "todo_update" => todo::execute_update(tool, state),
        
        // Memory tools
        "memory_create" => memory::execute_create(tool, state),
        "memory_update" => memory::execute_update(tool, state),
        
        // System prompt tools
        "system_create" => system::create_system(tool, state),
        "system_edit" => system::edit_system(tool, state),
        "system_delete" => system::delete_system(tool, state),
        "system_load" => system::load_system(tool, state),
        
        // System tools
        "system_reload" => execute_reload_tui(tool, state),
        
        // Git tools
        "git_toggle_details" => git::execute_toggle_details(tool, state),
        "git_toggle_logs" => git::execute_toggle_logs(tool, state),
        "git_commit" => git::execute_commit(tool, state),
        "git_branch_create" => git::execute_create_branch(tool, state),
        "git_branch_switch" => git::execute_change_branch(tool, state),
        "git_merge" => git::execute_merge(tool, state),
        "git_pull" => git::execute_pull(tool, state),
        "git_push" => git::execute_push(tool, state),
        "git_fetch" => git::execute_fetch(tool, state),
        
        // Meta tools
        "tool_manage" => manage_tools::execute(tool, state),

        // Scratchpad tools
        "scratchpad_create_cell" => scratchpad::execute_create(tool, state),
        "scratchpad_edit_cell" => scratchpad::execute_edit(tool, state),
        "scratchpad_wipe" => scratchpad::execute_wipe(tool, state),

        _ => ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("Unknown tool: {}", tool.name),
            is_error: true,
        },
    }
}

/// Execute reload_tui tool - sets flag to trigger reload after tool result is saved
fn execute_reload_tui(tool: &ToolUse, state: &mut State) -> ToolResult {
    // Set flag - actual reload happens in app.rs after tool result is saved
    state.reload_pending = true;

    ToolResult {
        tool_use_id: tool.id.clone(),
        content: "Reload initiated. Restarting TUI...".to_string(),
        is_error: false,
    }
}

/// Perform the actual TUI reload (called from app.rs after tool result is saved)
pub fn perform_reload(state: &mut State) {
    use std::fs;
    use std::io::stdout;
    use crossterm::{execute, terminal::{disable_raw_mode, LeaveAlternateScreen}};
    use crate::persistence::save_state;

    let state_path = ".context-pilot/state.json";

    // Save state before exiting
    save_state(state);

    // Read current state, set reload_requested to true, and save
    match fs::read_to_string(state_path) {
        Ok(json) => {
            // Simple string replacement to set reload_requested: true
            let updated = if json.contains("\"reload_requested\":") {
                json.replace("\"reload_requested\": false", "\"reload_requested\": true")
                    .replace("\"reload_requested\":false", "\"reload_requested\":true")
            } else {
                // Add the field before the final }
                json.trim_end().trim_end_matches('}').to_string()
                    + ",\n  \"reload_requested\": true\n}"
            };
            let _ = fs::write(state_path, updated);
        }
        Err(_) => {
            // If we can't read state, just try to reload anyway
        }
    }

    // Clean up terminal
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen);

    // Exit - the run.sh supervisor will see reload_requested and restart
    std::process::exit(0);
}
