use cp_base::panels::now_ms;
use cp_base::state::{ContextType, State, make_default_context_element};
use cp_base::tools::{ToolResult, ToolUse};

use crate::manager::SessionHandle;
use crate::types::{ConsoleState, ConsoleWaiter, format_wait_result};

/// Sentinel value returned when a blocking console_wait is registered.
/// The binary's event loop replaces this with the real result when satisfied.
pub const CONSOLE_WAIT_BLOCKING_SENTINEL: &str = "__CONSOLE_WAIT_BLOCKING__";

/// Maximum execution time for debug_bash (blocking tool — must be short).
const BASH_MAX_EXECUTION_SECS: u64 = 10;

/// Check if a command string contains git or gh commands.
/// Returns Some(error_message) if blocked, None if allowed.
fn check_git_gh_guardrail(input: &str) -> Option<String> {
    // Split on shell operators to handle chained commands
    let segments: Vec<&str> = input.split(|c| c == '|' || c == ';' || c == '&' || c == '\n')
        .collect();

    for segment in &segments {
        let trimmed = segment.trim();
        // Skip empty segments
        if trimmed.is_empty() {
            continue;
        }
        // Strip leading env vars (KEY=VAL) to find the actual command
        let cmd_part = trimmed.split_whitespace()
            .skip_while(|w| w.contains('=') && !w.starts_with('='))
            .next()
            .unwrap_or("");

        // Check the actual binary name (could be a path like /usr/bin/git)
        let binary = cmd_part.rsplit('/').next().unwrap_or(cmd_part);

        if binary == "git" {
            return Some(
                "Blocked: use the `git_execute` tool instead of running git through console.\n\
                 Example: git_execute with command=\"git status\"".to_string()
            );
        }
        if binary == "gh" {
            return Some(
                "Blocked: use the `gh_execute` tool instead of running gh through console.\n\
                 Example: gh_execute with command=\"gh pr list\"".to_string()
            );
        }
    }

    None
}

/// Resolve a panel ID (e.g. "P11") to the internal session key.
/// Returns (session_key, panel_id) or an error.
fn resolve_session_key(state: &State, panel_id: &str) -> Result<String, String> {
    state
        .context
        .iter()
        .find(|c| c.id == panel_id && c.context_type.as_str() == ContextType::CONSOLE)
        .and_then(|c| c.get_meta_str("console_name").map(|s| s.to_string()))
        .ok_or_else(|| format!("Console panel '{}' not found", panel_id))
}

pub fn execute_create(tool: &ToolUse, state: &mut State) -> ToolResult {
    let command = match tool.input.get("command").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => return ToolResult::new(tool.id.clone(), "Missing required 'command' parameter".to_string(), true),
    };

    // Guardrail: block git/gh commands
    if let Some(msg) = check_git_gh_guardrail(&command) {
        return ToolResult::new(tool.id.clone(), msg, true);
    }

    let cwd = tool.input.get("cwd").and_then(|v| v.as_str()).map(|s| s.to_string());
    let description = tool.input.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());

    // Auto-generate session key
    let session_key = {
        let cs = ConsoleState::get_mut(state);
        let key = format!("c_{}", cs.next_session_id);
        cs.next_session_id += 1;
        key
    };

    // Spawn the process
    let handle = match SessionHandle::spawn(session_key.clone(), command.clone(), cwd.clone()) {
        Ok(h) => h,
        Err(e) => return ToolResult::new(tool.id.clone(), e, true),
    };

    // Display name: description if provided, else truncated command
    let display_name = description.as_deref().unwrap_or_else(|| {
        if command.len() > 30 { &command[..30] } else { &command }
    });

    // Create dynamic panel with UID for persistence
    let panel_id = state.next_available_context_id();
    let uid = format!("UID_{}_P", state.global_next_uid);
    state.global_next_uid += 1;
    let mut ctx =
        make_default_context_element(&panel_id, ContextType::new(ContextType::CONSOLE), display_name, true);
    ctx.uid = Some(uid);
    ctx.set_meta("console_name", &session_key);
    ctx.set_meta("console_command", &command);
    ctx.set_meta("console_status", &handle.get_status().label());
    if let Some(ref desc) = description {
        ctx.set_meta("console_description", desc);
    }
    if let Some(ref dir) = cwd {
        ctx.set_meta("console_cwd", dir);
    }
    state.context.push(ctx);

    // Store handle
    let cs = ConsoleState::get_mut(state);
    cs.sessions.insert(session_key, handle);

    ToolResult::new(tool.id.clone(), format!("Console created in {}", panel_id), false)
}

pub fn execute_send_keys(tool: &ToolUse, state: &mut State) -> ToolResult {
    let panel_id = match tool.input.get("id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return ToolResult::new(tool.id.clone(), "Missing required 'id' parameter".to_string(), true),
    };
    let input = match tool.input.get("input").and_then(|v| v.as_str()) {
        Some(i) => i.to_string(),
        None => return ToolResult::new(tool.id.clone(), "Missing required 'input' parameter".to_string(), true),
    };

    // Guardrail: block git/gh commands sent to interactive shells
    if let Some(msg) = check_git_gh_guardrail(&input) {
        return ToolResult::new(tool.id.clone(), msg, true);
    }

    let session_key = match resolve_session_key(state, &panel_id) {
        Ok(k) => k,
        Err(e) => return ToolResult::new(tool.id.clone(), e, true),
    };

    let cs = ConsoleState::get(state);
    let handle = match cs.sessions.get(&session_key) {
        Some(h) => h,
        None => return ToolResult::new(tool.id.clone(), format!("Session for '{}' not found", panel_id), true),
    };

    if handle.get_status().is_terminal() {
        return ToolResult::new(
            tool.id.clone(),
            format!("Console '{}' has already exited ({})", panel_id, handle.get_status().label()),
            true,
        );
    }

    if let Err(e) = handle.send_input(&input) {
        return ToolResult::new(tool.id.clone(), format!("Failed to send input: {}", e), true);
    }

    // Short delay for output to arrive
    state.tool_sleep_until_ms = now_ms() + 500;

    ToolResult::new(tool.id.clone(), format!("Sent input to console '{}'", panel_id), false)
}

pub fn execute_wait(tool: &ToolUse, state: &mut State) -> ToolResult {
    let panel_id = match tool.input.get("id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return ToolResult::new(tool.id.clone(), "Missing required 'id' parameter".to_string(), true),
    };
    let mode = match tool.input.get("mode").and_then(|v| v.as_str()) {
        Some(m) => m.to_string(),
        None => return ToolResult::new(tool.id.clone(), "Missing required 'mode' parameter".to_string(), true),
    };
    let pattern = tool.input.get("pattern").and_then(|v| v.as_str()).map(|s| s.to_string());
    let block = tool.input.get("block").and_then(|v| v.as_bool()).unwrap_or(true);
    let max_wait: u64 = tool
        .input
        .get("max_wait")
        .and_then(|v| v.as_u64())
        .unwrap_or(30)
        .clamp(1, 30);

    // Validate mode
    if mode != "exit" && mode != "pattern" {
        return ToolResult::new(
            tool.id.clone(),
            format!("Invalid mode '{}'. Must be 'exit' or 'pattern'.", mode),
            true,
        );
    }

    if mode == "pattern" && pattern.is_none() {
        return ToolResult::new(
            tool.id.clone(),
            "Mode 'pattern' requires a 'pattern' parameter".to_string(),
            true,
        );
    }

    let session_key = match resolve_session_key(state, &panel_id) {
        Ok(k) => k,
        Err(e) => return ToolResult::new(tool.id.clone(), e, true),
    };

    // Check if session exists
    let cs = ConsoleState::get(state);
    let handle = match cs.sessions.get(&session_key) {
        Some(h) => h,
        None => return ToolResult::new(tool.id.clone(), format!("Session for '{}' not found", panel_id), true),
    };

    // Check if condition is already met
    let already_met = match mode.as_str() {
        "exit" => handle.get_status().is_terminal(),
        "pattern" => {
            if let Some(ref pat) = pattern {
                handle.buffer.contains_pattern(pat)
            } else {
                false
            }
        }
        _ => false,
    };

    if already_met {
        let exit_code = handle.get_status().exit_code();
        let last_lines = handle.buffer.last_n_lines(5);
        return ToolResult::new(
            tool.id.clone(),
            format_wait_result(&session_key, exit_code, &panel_id, &last_lines),
            false,
        );
    }

    let now = now_ms();
    let waiter = ConsoleWaiter {
        session_name: session_key,
        mode,
        pattern,
        blocking: block,
        tool_use_id: Some(tool.id.clone()),
        registered_ms: now,
        deadline_ms: if block { Some(now + max_wait * 1000) } else { None },
        is_debug_bash: false,
    };

    if block {
        let cs = ConsoleState::get_mut(state);
        cs.blocking_waiters.push(waiter);
        ToolResult::new(tool.id.clone(), CONSOLE_WAIT_BLOCKING_SENTINEL.to_string(), false)
    } else {
        let cs = ConsoleState::get_mut(state);
        cs.async_waiters.push(waiter);
        ToolResult::new(tool.id.clone(), format!("Watcher registered for '{}'", panel_id), false)
    }
}

pub fn execute_debug_bash(tool: &ToolUse, state: &mut State) -> ToolResult {
    let command = match tool.input.get("command").and_then(|v| v.as_str()) {
        Some(c) => c.to_string(),
        None => return ToolResult::new(tool.id.clone(), "Missing required 'command' parameter".to_string(), true),
    };

    // Guardrail: block git/gh commands
    if let Some(msg) = check_git_gh_guardrail(&command) {
        return ToolResult::new(tool.id.clone(), msg, true);
    }

    let cwd = tool.input.get("cwd").and_then(|v| v.as_str()).map(|s| s.to_string());

    // Spawn via the console server (non-blocking to the main loop)
    let session_key = {
        let cs = ConsoleState::get_mut(state);
        let key = format!("c_{}", cs.next_session_id);
        cs.next_session_id += 1;
        key
    };

    let handle = match SessionHandle::spawn(session_key.clone(), command.clone(), cwd.clone()) {
        Ok(h) => h,
        Err(e) => return ToolResult::new(tool.id.clone(), format!("Failed to execute: {}", e), true),
    };

    // Create a panel so output goes there instead of flooding the conversation
    let display_name = if command.len() > 30 { &command[..30] } else { &command };
    let panel_id = state.next_available_context_id();
    let uid = format!("UID_{}_P", state.global_next_uid);
    state.global_next_uid += 1;
    let mut ctx =
        make_default_context_element(&panel_id, ContextType::new(ContextType::CONSOLE), display_name, true);
    ctx.uid = Some(uid);
    ctx.set_meta("console_name", &session_key);
    ctx.set_meta("console_command", &command);
    ctx.set_meta("console_status", &handle.get_status().label());
    ctx.set_meta("console_is_easy_bash", &"true".to_string());
    if let Some(ref dir) = cwd {
        ctx.set_meta("console_cwd", dir);
    }
    state.context.push(ctx);

    // Store the handle (needed for waiter to check status + read buffer)
    let cs = ConsoleState::get_mut(state);
    cs.sessions.insert(session_key.clone(), handle);

    // Register a blocking exit waiter with BASH_MAX_EXECUTION_SECS timeout
    let now = now_ms();
    let waiter = ConsoleWaiter {
        session_name: session_key,
        mode: "exit".to_string(),
        pattern: None,
        blocking: true,
        tool_use_id: Some(tool.id.clone()),
        registered_ms: now,
        deadline_ms: Some(now + BASH_MAX_EXECUTION_SECS * 1000),
        is_debug_bash: true,
    };
    let cs = ConsoleState::get_mut(state);
    cs.blocking_waiters.push(waiter);

    ToolResult::new(tool.id.clone(), CONSOLE_WAIT_BLOCKING_SENTINEL.to_string(), false)
}
