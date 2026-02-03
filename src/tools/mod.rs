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
        "open_file" => file::execute_open(tool, state),
        "edit_file" => edit_file::execute_edit(tool, state),
        "create_file" => edit_file::execute_create(tool, state),
        "create" => create::execute(tool, state),
        "manage_tools" => manage_tools::execute(tool, state),
        "close_contexts" => close_context::execute(tool, state),
        "edit_tree_filter" => tree::execute_edit_filter(tool, state),
        "tree_toggle_folders" => tree::execute_toggle_folders(tool, state),
        "tree_describe_files" => tree::execute_describe_files(tool, state),
        "file_glob" => glob::execute(tool, state),
        "file_grep" => grep::execute(tool, state),
        "set_message_status" => message_status::execute(tool, state),
        "create_tmux_pane" => tmux::execute_create_pane(tool, state),
        "edit_tmux_config" => tmux::execute_edit_config(tool, state),
        "tmux_send_keys" => tmux::execute_send_keys(tool, state),
        "sleep" => tmux::execute_sleep(tool),
        "create_todos" => todo::execute_create(tool, state),
        "update_todos" => todo::execute_update(tool, state),
        "create_memories" => memory::execute_create(tool, state),
        "update_memories" => memory::execute_update(tool, state),
        "git_commit" => git::execute_commit(tool, state),
        "toggle_git_details" => git::execute_toggle_details(tool, state),
        "reload_tui" => execute_reload_tui(tool, state),
        _ => ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("Unknown tool: {}", tool.name),
            is_error: true,
        },
    }
}

/// Execute reload_tui tool - restarts the TUI application
fn execute_reload_tui(tool: &ToolUse, _state: &State) -> ToolResult {
    use std::process::Command;
    use std::env;
    use std::io::stdout;
    use crossterm::{execute, terminal::{disable_raw_mode, LeaveAlternateScreen}};
    
    // Get the current working directory
    let cwd = match env::current_dir() {
        Ok(path) => path,
        Err(e) => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: format!("Failed to get current directory: {}", e),
                is_error: true,
            };
        }
    };
    
    // Clean up terminal FIRST, before spawning
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen);
    
    // Use bash with background process
    // The process runs in background, parent exits, child continues
    let shell_cmd = format!(
        "(sleep 0.5 && cd {} && cargo run --release -- --resume-stream) &",
        cwd.display()
    );
    
    // Execute the command which backgrounds itself
    let _ = Command::new("bash")
        .arg("-c")
        .arg(&shell_cmd)
        .status();
    
    // Exit immediately - the backgrounded process will start the new TUI
    std::process::exit(0);
}
