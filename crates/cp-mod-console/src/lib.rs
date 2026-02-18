mod manager;
mod panel;
pub mod ring_buffer;
pub mod tools;
pub mod types;

/// Subdirectory under STORE_DIR for console log files.
pub const CONSOLE_DIR: &str = "console";

use std::collections::HashMap;

use serde_json::json;

use cp_base::modules::{Module, ToolVisualizer};
use cp_base::panels::Panel;
use cp_base::state::{ContextType, ContextTypeMeta, State};
use cp_base::tools::{ParamType, ToolDefinition, ToolParam};
use cp_base::tools::{ToolResult, ToolUse};

use self::manager::SessionHandle;
use self::panel::ConsolePanel;
use self::types::{ConsoleState, SessionMeta};

pub use self::tools::CONSOLE_WAIT_BLOCKING_SENTINEL;
pub use self::types::{CompletedWait, ConsoleWaiter};

pub struct ConsoleModule;

impl Module for ConsoleModule {
    fn id(&self) -> &'static str {
        "console"
    }
    fn name(&self) -> &'static str {
        "Console"
    }
    fn description(&self) -> &'static str {
        "Spawn and manage child processes"
    }

    fn init_state(&self, state: &mut State) {
        state.set_ext(ConsoleState::new());
        // Ensure the console server is running
        if let Err(e) = manager::find_or_create_server() {
            eprintln!("Console server startup failed: {}", e);
        }
    }

    fn reset_state(&self, state: &mut State) {
        // Collect file paths before shutdown (shutdown drains sessions)
        let paths: Vec<String> = {
            let cs = ConsoleState::get(state);
            cs.sessions.values().map(|h| h.log_path.clone()).collect()
        };
        ConsoleState::shutdown_all(state);
        state.set_ext(ConsoleState::new());
        // Clean up log files
        for log in paths {
            let _ = std::fs::remove_file(&log);
        }
    }

    fn save_module_data(&self, state: &State) -> serde_json::Value {
        let cs = ConsoleState::get(state);
        let mut sessions_map: HashMap<String, SessionMeta> = HashMap::new();
        for (name, handle) in &cs.sessions {
            // Only persist live (non-terminal) sessions
            if !handle.get_status().is_terminal()
                && let Some(pid) = handle.pid()
            {
                // Leak stdin so script doesn't see EOF when TUI exits for reload.
                // This keeps the pipe fd open (no EOF → script stays alive).
                // After reload, send_keys already fails with "stdin unavailable".
                handle.leak_stdin();

                sessions_map.insert(
                    name.clone(),
                    SessionMeta {
                        pid,
                        command: handle.command.clone(),
                        cwd: handle.cwd.clone(),
                        log_path: handle.log_path.clone(),
                        started_at: handle.started_at,
                    },
                );
            }
        }
        if sessions_map.is_empty() && cs.next_session_id == 1 {
            serde_json::Value::Null
        } else {
            json!({
                "sessions": sessions_map,
                "next_session_id": cs.next_session_id,
            })
        }
    }

    fn load_module_data(&self, data: &serde_json::Value, state: &mut State) {
        // Ensure the console server is running
        if let Err(e) = manager::find_or_create_server() {
            eprintln!("Console server startup failed: {}", e);
        }

        // Restore counter
        if let Some(v) = data.get("next_session_id").and_then(|v| v.as_u64()) {
            let cs = ConsoleState::get_mut(state);
            cs.next_session_id = v as usize;
        }

        let sessions_map: HashMap<String, SessionMeta> = match data.get("sessions") {
            Some(v) => match serde_json::from_value(v.clone()) {
                Ok(m) => m,
                Err(_) => return,
            },
            None => return,
        };

        if sessions_map.is_empty() {
            // No known sessions — kill any orphans on the server
            manager::kill_orphaned_processes(&std::collections::HashSet::new());
            return;
        }

        // Collect known session keys for orphan cleanup
        let known_keys: std::collections::HashSet<String> = sessions_map.keys().cloned().collect();

        // Kill any server-managed sessions that aren't in our saved state
        manager::kill_orphaned_processes(&known_keys);

        // Phase 1: Reconnect sessions (no &mut State needed)
        let mut reconnected: Vec<(String, SessionHandle)> = Vec::new();
        for (name, meta) in &sessions_map {
            let handle = SessionHandle::reconnect(
                name.clone(),
                meta.command.clone(),
                meta.cwd.clone(),
                meta.pid,
                meta.log_path.clone(),
                meta.started_at,
            );
            reconnected.push((name.clone(), handle));
        }

        // Phase 2: Insert handles into ConsoleState and update panel metadata
        for (name, handle) in reconnected {
            let status_label = handle.get_status().label();
            let cs = ConsoleState::get_mut(state);
            cs.sessions.insert(name.clone(), handle);

            // Update panel metadata if panel was persisted
            if let Some(ctx) = state.context.iter_mut().find(|c| c.get_meta_str("console_name") == Some(&name)) {
                ctx.set_meta("console_status", &status_label);
                ctx.cache_deprecated = true;
            }
        }

        // Phase 3: Remove orphaned console panels that have no matching session
        // (e.g. sessions that were terminal at save time and weren't persisted)
        let live_names: std::collections::HashSet<String> = {
            let cs = ConsoleState::get(state);
            cs.sessions.keys().cloned().collect()
        };
        state.context.retain(|c| {
            if c.context_type.as_str() != ContextType::CONSOLE {
                return true; // keep non-console panels
            }
            match c.get_meta_str("console_name") {
                Some(name) => live_names.contains(name),
                None => false, // malformed console panel, remove
            }
        });
    }

    fn dynamic_panel_types(&self) -> Vec<ContextType> {
        vec![ContextType::new(ContextType::CONSOLE)]
    }

    fn create_panel(&self, context_type: &ContextType) -> Option<Box<dyn Panel>> {
        match context_type.as_str() {
            ContextType::CONSOLE => Some(Box::new(ConsolePanel)),
            _ => None,
        }
    }

    fn context_type_metadata(&self) -> Vec<ContextTypeMeta> {
        vec![ContextTypeMeta {
            context_type: "console",
            icon_id: "tmux", // Reuse tmux icon for now
            is_fixed: false,
            needs_cache: true,
            fixed_order: None,
            display_name: "console",
            short_name: "console",
            needs_async_wait: true,
        }]
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                id: "cp_console_create".to_string(),
                name: "Create Console".to_string(),
                short_desc: "Spawn a new process".to_string(),
                description: "Spawns a child process and creates a console panel to monitor its output. \
                    The process runs in the background and survives TUI reloads. \
                    Close the panel to kill the process. \
                    For one-shot commands (e.g. 'cargo build'), pass the full command directly — \
                    the panel shows output and exits when done. \
                    For interactive shells, pass 'bash' and use cp_console_send_keys to run commands. \
                    For long-running servers (e.g. 'npm run dev'), combine with cp_console_wait \
                    (block=false, mode='pattern') to get notified when ready."
                    .to_string(),
                params: vec![
                    ToolParam::new("command", ParamType::String)
                        .desc("Shell command to execute (e.g., 'npm run dev', 'cargo build 2>&1', 'bash')")
                        .required(),
                    ToolParam::new("cwd", ParamType::String)
                        .desc("Working directory for the command (defaults to project root)"),
                    ToolParam::new("description", ParamType::String)
                        .desc("Short description for the panel title"),
                ],
                enabled: true,
                category: "Console".to_string(),
            },
            ToolDefinition {
                id: "cp_console_send_keys".to_string(),
                name: "Console Send Keys".to_string(),
                short_desc: "Send input to process".to_string(),
                description: "Sends input text to a running console's stdin. Use for interactive processes. \
                    Newline is NOT appended automatically — include \\n if needed. \
                    Typical usage: send a command followed by \\n (e.g. 'ls -la\\n'). \
                    For interactive prompts, send the expected response (e.g. 'yes\\n' or 'q'). \
                    Escape sequences are interpreted as control characters: \
                    \\x03 (Ctrl+C to interrupt), \\x04 (Ctrl+D for EOF), \\e[A (up arrow), \
                    \\n (newline), \\t (tab), \\xHH (arbitrary hex byte). \
                    To stop a process, send \\x03 (Ctrl+C) or close the panel."
                    .to_string(),
                params: vec![
                    ToolParam::new("id", ParamType::String)
                        .desc("Console panel ID (e.g., 'P11')")
                        .required(),
                    ToolParam::new("input", ParamType::String)
                        .desc("Text to send to stdin (e.g., 'cargo test\\n' or 'yes\\n')")
                        .required(),
                ],
                enabled: true,
                category: "Console".to_string(),
            },
            ToolDefinition {
                id: "cp_console_wait".to_string(),
                name: "Console Wait".to_string(),
                short_desc: "Wait for process event".to_string(),
                description: "Registers a waiter for a console event. Two modes: \
                    mode='exit': waits for the process to exit (use for builds, one-shot commands). \
                    mode='pattern': waits for a regex pattern to match in output (use for server ready messages, specific log lines). \
                    Patterns are full regex — e.g. 'Listening on port \\d+', 'error|warning', 'Finished.*target'. \
                    Falls back to literal substring match if the regex is invalid. \
                    Two blocking modes: \
                    block=true (default): pauses tool execution until condition is met or max_wait expires. \
                    Best for sequential workflows (build then test). \
                    block=false: registers an async watcher — you continue working and get a spine notification \
                    when the condition is satisfied. Best for long-running processes where you don't want to block."
                    .to_string(),
                params: vec![
                    ToolParam::new("id", ParamType::String)
                        .desc("Console panel ID (e.g., 'P11')")
                        .required(),
                    ToolParam::new("mode", ParamType::String)
                        .desc("Wait mode: 'exit' for process completion, 'pattern' for regex match in output")
                        .enum_vals(&["exit", "pattern"])
                        .required(),
                    ToolParam::new("pattern", ParamType::String)
                        .desc("Regex pattern to match in output (required when mode='pattern'). E.g. 'Finished.*target', 'error|warning', 'port \\d+'. Falls back to literal match if invalid regex."),
                    ToolParam::new("block", ParamType::Boolean)
                        .desc("true (default): block until condition met. false: async notification via spine.")
                        .default_val("true"),
                    ToolParam::new("max_wait", ParamType::Integer)
                        .desc("Max wait in seconds, 1-30 (default: 30). Only applies when block=true. On timeout, returns last output lines.")
                        .default_val("30"),
                ],
                enabled: true,
                category: "Console".to_string(),
            },
        ]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "cp_console_create" => Some(tools::execute_create(tool, state)),
            "cp_console_send_keys" => Some(tools::execute_send_keys(tool, state)),
            "cp_console_wait" => Some(tools::execute_wait(tool, state)),
            _ => None,
        }
    }

    fn tool_visualizers(&self) -> Vec<(&'static str, ToolVisualizer)> {
        vec![
            ("cp_console_create", visualize_console_output as ToolVisualizer),
            ("cp_console_send_keys", visualize_console_output as ToolVisualizer),
            ("cp_console_wait", visualize_console_output as ToolVisualizer),
        ]
    }

    fn on_close_context(
        &self,
        ctx: &cp_base::state::ContextElement,
        state: &mut State,
    ) -> Option<Result<String, String>> {
        if ctx.context_type.as_str() != ContextType::CONSOLE {
            return None;
        }
        let name = ctx.get_meta_str("console_name").unwrap_or_default().to_string();
        // Grab file path before removing
        let log_path = {
            let cs = ConsoleState::get(state);
            cs.sessions
                .get(&name)
                .map(|h| h.log_path.clone())
                .unwrap_or_default()
        };
        ConsoleState::kill_session(state, &name);
        {
            let cs = ConsoleState::get_mut(state);
            cs.sessions.remove(&name);
        }
        // Delete log file
        if !log_path.is_empty() {
            let _ = std::fs::remove_file(&log_path);
        }
        Some(Ok(format!("console: {}", name)))
    }

    fn context_detail(&self, ctx: &cp_base::state::ContextElement) -> Option<String> {
        if ctx.context_type.as_str() == ContextType::CONSOLE {
            let desc = ctx
                .get_meta_str("console_description")
                .or_else(|| ctx.get_meta_str("console_command"))
                .unwrap_or("?");
            let status = ctx.get_meta_str("console_status").unwrap_or("?");
            Some(format!("{} ({})", desc, status))
        } else {
            None
        }
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Console", "Spawn and manage child processes")]
    }
}

/// Visualizer for console tool results.
fn visualize_console_output(content: &str, width: usize) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::prelude::*;

    let success_color = Color::Rgb(80, 250, 123);
    let info_color = Color::Rgb(139, 233, 253);
    let error_color = Color::Rgb(255, 85, 85);

    let mut lines = Vec::new();

    for line in content.lines() {
        if line.is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        let style = if line.starts_with("Error:") || line.starts_with("Failed") || line.starts_with("Missing") {
            Style::default().fg(error_color)
        } else if line.starts_with("Console ")
            || line.starts_with("Sent ")
            || line.starts_with("Watcher ")
            || line.contains("created")
        {
            Style::default().fg(success_color)
        } else if line.contains("condition met") || line.contains("Last output:") {
            Style::default().fg(info_color)
        } else {
            Style::default()
        };

        let display = if line.len() > width {
            format!("{}...", &line[..line.floor_char_boundary(width.saturating_sub(3))])
        } else {
            line.to_string()
        };
        lines.push(Line::from(Span::styled(display, style)));
    }

    lines
}
