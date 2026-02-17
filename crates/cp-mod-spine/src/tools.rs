use cp_base::state::{ContextType, State};
use cp_base::tools::{ToolResult, ToolUse};

use crate::types::SpineState;

/// Execute the notification_mark_processed tool
pub fn execute_mark_processed(tool: &ToolUse, state: &mut State) -> ToolResult {
    let id = match tool.input.get("id").and_then(|v| v.as_str()) {
        Some(i) => i,
        None => {
            return ToolResult::new(tool.id.clone(), "Missing required 'id' parameter".to_string(), true);
        }
    };

    // Check if notification exists and its current state
    let already_processed = SpineState::get(state).notifications.iter().find(|n| n.id == id).map(|n| n.processed);

    match already_processed {
        Some(true) => ToolResult::new(tool.id.clone(), format!("Notification {} is already processed", id), false),
        Some(false) => {
            SpineState::mark_notification_processed(state, id);
            ToolResult::new(tool.id.clone(), format!("Marked notification {} as processed", id), false)
        }
        None => ToolResult::new(tool.id.clone(), format!("Notification '{}' not found", id), true),
    }
}

/// Execute the spine_configure tool — update spine auto-continuation and guard rail settings
pub fn execute_configure(tool: &ToolUse, state: &mut State) -> ToolResult {
    let mut changes: Vec<String> = Vec::new();

    // === Auto-continuation toggles ===
    if let Some(v) = tool.input.get("max_tokens_auto_continue").and_then(|v| v.as_bool()) {
        SpineState::get_mut(state).config.max_tokens_auto_continue = v;
        changes.push(format!("max_tokens_auto_continue = {}", v));
    }

    if let Some(v) = tool.input.get("continue_until_todos_done").and_then(|v| v.as_bool()) {
        SpineState::get_mut(state).config.continue_until_todos_done = v;
        changes.push(format!("continue_until_todos_done = {}", v));
    }

    // === Guard rail limits (pass null to disable) ===
    if let Some(v) = tool.input.get("max_output_tokens") {
        if v.is_null() {
            SpineState::get_mut(state).config.max_output_tokens = None;
            changes.push("max_output_tokens = disabled".to_string());
        } else if let Some(n) = v.as_u64() {
            SpineState::get_mut(state).config.max_output_tokens = Some(n as usize);
            changes.push(format!("max_output_tokens = {}", n));
        }
    }

    if let Some(v) = tool.input.get("max_cost") {
        if v.is_null() {
            SpineState::get_mut(state).config.max_cost = None;
            changes.push("max_cost = disabled".to_string());
        } else if let Some(n) = v.as_f64() {
            SpineState::get_mut(state).config.max_cost = Some(n);
            changes.push(format!("max_cost = ${:.2}", n));
        }
    }

    if let Some(v) = tool.input.get("max_stream_cost") {
        if v.is_null() {
            SpineState::get_mut(state).config.max_stream_cost = None;
            changes.push("max_stream_cost = disabled".to_string());
        } else if let Some(n) = v.as_f64() {
            SpineState::get_mut(state).config.max_stream_cost = Some(n);
            changes.push(format!("max_stream_cost = ${:.2}", n));
        }
    }

    if let Some(v) = tool.input.get("max_duration_secs") {
        if v.is_null() {
            SpineState::get_mut(state).config.max_duration_secs = None;
            changes.push("max_duration_secs = disabled".to_string());
        } else if let Some(n) = v.as_u64() {
            SpineState::get_mut(state).config.max_duration_secs = Some(n);
            changes.push(format!("max_duration_secs = {}s", n));
        }
    }

    if let Some(v) = tool.input.get("max_messages") {
        if v.is_null() {
            SpineState::get_mut(state).config.max_messages = None;
            changes.push("max_messages = disabled".to_string());
        } else if let Some(n) = v.as_u64() {
            SpineState::get_mut(state).config.max_messages = Some(n as usize);
            changes.push(format!("max_messages = {}", n));
        }
    }

    if let Some(v) = tool.input.get("max_auto_retries") {
        if v.is_null() {
            SpineState::get_mut(state).config.max_auto_retries = None;
            changes.push("max_auto_retries = disabled".to_string());
        } else if let Some(n) = v.as_u64() {
            SpineState::get_mut(state).config.max_auto_retries = Some(n as usize);
            changes.push(format!("max_auto_retries = {}", n));
        }
    }

    // === Reset runtime counters ===
    if let Some(true) = tool.input.get("reset_counters").and_then(|v| v.as_bool()) {
        SpineState::get_mut(state).config.auto_continuation_count = 0;
        SpineState::get_mut(state).config.autonomous_start_ms = None;
        changes.push("reset runtime counters".to_string());
    }

    state.touch_panel(ContextType::new(ContextType::SPINE));

    if changes.is_empty() {
        ToolResult::new(tool.id.clone(), "No changes made. Pass at least one parameter to configure.".to_string(), false)
    } else {
        ToolResult::new(tool.id.clone(), format!(
            "Spine configured:\n{}",
            changes.iter().map(|c| format!("  • {}", c)).collect::<Vec<_>>().join("\n")
        ), false)
    }
}
