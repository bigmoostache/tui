mod panel;
mod tools;

/// Delay after tmux send-keys in milliseconds (allows command output to appear)
pub const TMUX_SEND_DELAY_MS: u64 = 1000;

/// Background session name for tmux operations
pub const TMUX_BG_SESSION: &str = "context-pilot-bg";

/// Fixed sleep duration in seconds for the sleep tool
pub const SLEEP_DURATION_SECS: u64 = 0;

/// Deprecation timer for tmux panels (milliseconds)
pub(crate) const TMUX_DEPRECATION_MS: u64 = 100; // 100ms — capture-pane is a cheap kernel pipe read

use cp_base::modules::ToolVisualizer;
use cp_base::panels::Panel;
use cp_base::state::{ContextType, State};
use cp_base::tools::{ParamType, ToolDefinition, ToolParam};
use cp_base::tools::{ToolResult, ToolUse};

use self::panel::TmuxPanel;
use cp_base::modules::Module;

pub struct TmuxModule;

impl Module for TmuxModule {
    fn id(&self) -> &'static str {
        "tmux"
    }
    fn name(&self) -> &'static str {
        "Tmux"
    }
    fn description(&self) -> &'static str {
        "Terminal console management via tmux"
    }

    fn dynamic_panel_types(&self) -> Vec<ContextType> {
        vec![ContextType::new(ContextType::TMUX)]
    }

    fn create_panel(&self, context_type: &ContextType) -> Option<Box<dyn Panel>> {
        match context_type.as_str() {
            ContextType::TMUX => Some(Box::new(TmuxPanel)),
            _ => None,
        }
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                id: "console_create".to_string(),
                name: "Create Console".to_string(),
                short_desc: "Add terminal to context".to_string(),
                description: "Creates a console context element to monitor terminal output.".to_string(),
                params: vec![
                    ToolParam::new("pane_id", ParamType::String)
                        .desc("Console pane ID (e.g., %0, %1). Omit to auto-create a new pane."),
                    ToolParam::new("lines", ParamType::Integer)
                        .desc("Number of lines to capture")
                        .default_val("50"),
                    ToolParam::new("description", ParamType::String)
                        .desc("Description of what this console is for"),
                ],
                enabled: true,
                category: "Console".to_string(),
            },
            ToolDefinition {
                id: "console_edit".to_string(),
                name: "Edit Console".to_string(),
                short_desc: "Update console settings".to_string(),
                description: "Updates configuration for an existing console context.".to_string(),
                params: vec![
                    ToolParam::new("context_id", ParamType::String)
                        .desc("Context ID of the console (e.g., P7)")
                        .required(),
                    ToolParam::new("lines", ParamType::Integer)
                        .desc("Number of lines to capture"),
                    ToolParam::new("description", ParamType::String)
                        .desc("New description"),
                ],
                enabled: true,
                category: "Console".to_string(),
            },
            ToolDefinition {
                id: "console_send_keys".to_string(),
                name: "Console Send Keys".to_string(),
                short_desc: "Send keys to terminal".to_string(),
                description: "Sends keystrokes to a console. Use for running commands or interacting with terminal apps. Enter is sent automatically after the keys — do not include it unless you need to send a blank Enter.".to_string(),
                params: vec![
                    ToolParam::new("context_id", ParamType::String)
                        .desc("Context ID of the console (e.g., P7)")
                        .required(),
                    ToolParam::new("keys", ParamType::String)
                        .desc("Keys to send (e.g., 'ls -la' or 'C-c'). Enter is appended automatically.")
                        .required(),
                ],
                enabled: true,
                category: "Console".to_string(),
            },
            ToolDefinition {
                id: "console_sleep".to_string(),
                name: "Sleep".to_string(),
                short_desc: "Wait and refresh".to_string(),
                description: "Pauses execution. Useful for waiting for terminal output or processes to complete.".to_string(),
                params: vec![],
                enabled: true,
                category: "Console".to_string(),
            },
        ]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "console_create" => Some(self::tools::execute_create_pane(tool, state)),
            "console_edit" => Some(self::tools::execute_edit_config(tool, state)),
            "console_send_keys" => Some(self::tools::execute_send_keys(tool, state)),
            "console_sleep" => Some(self::tools::execute_sleep(tool, state)),
            _ => None,
        }
    }

    fn tool_visualizers(&self) -> Vec<(&'static str, ToolVisualizer)> {
        vec![
            ("console_create", visualize_tmux_output as ToolVisualizer),
            ("console_edit", visualize_tmux_output as ToolVisualizer),
            ("console_send_keys", visualize_tmux_output as ToolVisualizer),
            ("console_sleep", visualize_tmux_output as ToolVisualizer),
        ]
    }

    fn context_type_metadata(&self) -> Vec<cp_base::state::ContextTypeMeta> {
        vec![cp_base::state::ContextTypeMeta {
            context_type: "tmux",
            icon_id: "tmux",
            is_fixed: false,
            needs_cache: true,
            fixed_order: None,
            display_name: "tmux",
            short_name: "tmux",
            needs_async_wait: true,
        }]
    }

    fn on_close_context(
        &self,
        ctx: &cp_base::state::ContextElement,
        _state: &mut State,
    ) -> Option<Result<String, String>> {
        if ctx.context_type.as_str() != cp_base::state::ContextType::TMUX {
            return None;
        }
        let desc = ctx.get_meta_str("tmux_description").unwrap_or_default().to_string();
        if let Some(pane) = ctx.get_meta_str("tmux_pane_id") {
            let _ = std::process::Command::new("tmux").args(["kill-window", "-t", pane]).output();
        }
        Some(Ok(format!("tmux: {}", desc)))
    }

    fn context_detail(&self, ctx: &cp_base::state::ContextElement) -> Option<String> {
        if ctx.context_type.as_str() == cp_base::state::ContextType::TMUX {
            let pane = ctx.get_meta_str("tmux_pane_id").unwrap_or("?");
            let desc = ctx.get_meta_str("tmux_description").unwrap_or("");
            if desc.is_empty() { Some(pane.to_string()) } else { Some(format!("{}: {}", pane, desc)) }
        } else {
            None
        }
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Console", "Execute commands and monitor terminal output")]
    }
}

/// Visualizer for tmux tool results.
/// Highlights pane IDs, key sequences sent, and differentiates creation vs send vs sleep results.
fn visualize_tmux_output(content: &str, width: usize) -> Vec<ratatui::text::Line<'static>> {
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

        let style = if line.starts_with("Error:") {
            Style::default().fg(error_color)
        } else if line.starts_with("Panel created:") || line.starts_with("Sent keys") || line.starts_with("Updated") || line.starts_with("Refreshed") {
            Style::default().fg(success_color)
        } else if line.contains("%") || line.contains("pane") {
            // Pane IDs like %0, %1
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
