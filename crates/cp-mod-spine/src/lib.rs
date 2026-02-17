pub(crate) mod continuation;
pub mod engine;
pub(crate) mod guard_rail;
mod panel;
pub(crate) mod tools;
pub mod types;

pub use types::{ContinuationAction, Notification, NotificationType, SpineConfig, SpineState};

use serde_json::json;

use cp_base::modules::ToolVisualizer;
use cp_base::panels::Panel;
use cp_base::state::{ContextType, State};
use cp_base::tool_defs::{ParamType, ToolDefinition, ToolParam};
use cp_base::tools::{ToolResult, ToolUse};

use self::panel::SpinePanel;
use cp_base::modules::Module;

pub struct SpineModule;

impl Module for SpineModule {
    fn id(&self) -> &'static str {
        "spine"
    }
    fn name(&self) -> &'static str {
        "Spine"
    }
    fn description(&self) -> &'static str {
        "Unified auto-continuation and stream control"
    }

    fn init_state(&self, state: &mut State) {
        state.set_ext(SpineState::new());
    }

    fn reset_state(&self, state: &mut State) {
        state.set_ext(SpineState::new());
    }

    fn save_module_data(&self, state: &State) -> serde_json::Value {
        let ss = SpineState::get(state);
        // Prune old processed notifications: keep all unprocessed + latest 10 processed
        let mut to_save: Vec<_> = ss.notifications.iter().filter(|n| !n.processed).cloned().collect();
        let mut processed: Vec<_> = ss.notifications.iter().filter(|n| n.processed).cloned().collect();
        // Keep only the latest 10 processed (they're in chronological order)
        if processed.len() > 10 {
            processed = processed.split_off(processed.len() - 10);
        }
        to_save.extend(processed);
        // Sort by ID number to maintain order
        to_save.sort_by_key(|n| n.id.trim_start_matches('N').parse::<usize>().unwrap_or(0));

        json!({
            "notifications": to_save,
            "next_notification_id": ss.next_notification_id,
            "spine_config": ss.config,
        })
    }

    fn load_module_data(&self, data: &serde_json::Value, state: &mut State) {
        if let Some(arr) = data.get("notifications")
            && let Ok(v) = serde_json::from_value(arr.clone())
        {
            SpineState::get_mut(state).notifications = v;
        }
        if let Some(v) = data.get("next_notification_id").and_then(|v| v.as_u64()) {
            SpineState::get_mut(state).next_notification_id = v as usize;
        }
        if let Some(cfg) = data.get("spine_config")
            && let Ok(v) = serde_json::from_value(cfg.clone())
        {
            SpineState::get_mut(state).config = v;
        }
        // Prune stale processed notifications on load too
        prune_notifications(&mut SpineState::get_mut(state).notifications);
    }

    fn fixed_panel_types(&self) -> Vec<ContextType> {
        vec![ContextType::new(ContextType::SPINE)]
    }

    fn fixed_panel_defaults(&self) -> Vec<(ContextType, &'static str, bool)> {
        vec![(ContextType::new(ContextType::SPINE), "Spine", false)]
    }

    fn create_panel(&self, context_type: &ContextType) -> Option<Box<dyn Panel>> {
        match context_type.as_str() {
            ContextType::SPINE => Some(Box::new(SpinePanel)),
            _ => None,
        }
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                id: "notification_mark_processed".to_string(),
                name: "Mark Notification Processed".to_string(),
                short_desc: "Mark notification as handled".to_string(),
                description: "Marks a spine notification as processed, indicating you've addressed it.".to_string(),
                params: vec![
                    ToolParam::new("id", ParamType::String)
                        .desc("Notification ID (e.g., N1)")
                        .required(),
                ],
                enabled: true,
                category: "Spine".to_string(),
            },
            ToolDefinition {
                id: "spine_configure".to_string(),
                name: "Configure Spine".to_string(),
                short_desc: "Configure auto-continuation and guard rails".to_string(),
                description: "Configures the spine module's auto-continuation behavior and guard rail limits. All parameters are optional — only provided values are changed. Guard rail limits accept null to disable.".to_string(),
                params: vec![
                    ToolParam::new("max_tokens_auto_continue", ParamType::Boolean)
                        .desc("Auto-continue when stream hits max_tokens (default: true)"),
                    ToolParam::new("continue_until_todos_done", ParamType::Boolean)
                        .desc("Keep auto-continuing until all todos are done (default: false)"),
                    ToolParam::new("max_output_tokens", ParamType::Integer)
                        .desc("Guard rail: max total output tokens before blocking. Null to disable."),
                    ToolParam::new("max_cost", ParamType::Number)
                        .desc("Guard rail: max cost in USD before blocking. Null to disable."),
                    ToolParam::new("max_duration_secs", ParamType::Integer)
                        .desc("Guard rail: max autonomous duration in seconds. Null to disable."),
                    ToolParam::new("max_messages", ParamType::Integer)
                        .desc("Guard rail: max conversation messages before blocking. Null to disable."),
                    ToolParam::new("max_auto_retries", ParamType::Integer)
                        .desc("Guard rail: max consecutive auto-continuations without human input. Null to disable."),
                    ToolParam::new("reset_counters", ParamType::Boolean)
                        .desc("Reset runtime counters (auto_continuation_count, autonomous_start_ms)"),
                ],
                enabled: true,
                category: "Spine".to_string(),
            },
        ]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "notification_mark_processed" => Some(self::tools::execute_mark_processed(tool, state)),
            "spine_configure" => Some(self::tools::execute_configure(tool, state)),
            _ => None,
        }
    }

    fn tool_visualizers(&self) -> Vec<(&'static str, ToolVisualizer)> {
        vec![
            ("notification_mark_processed", visualize_spine_output as ToolVisualizer),
            ("spine_configure", visualize_spine_output as ToolVisualizer),
        ]
    }

    fn context_type_metadata(&self) -> Vec<cp_base::state::ContextTypeMeta> {
        vec![cp_base::state::ContextTypeMeta {
            context_type: "spine",
            icon_id: "spine",
            is_fixed: true,
            needs_cache: false,
            fixed_order: Some(5),
            display_name: "spine",
            short_name: "spine",
            needs_async_wait: false,
        }]
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Spine", "Auto-continuation and stream control")]
    }

    fn on_user_message(&self, state: &mut State) {
        // Human input resets auto-continuation counters — human is back in the loop
        let ss = SpineState::get_mut(state);
        ss.config.auto_continuation_count = 0;
        ss.config.autonomous_start_ms = None;
        ss.config.user_stopped = false;
    }

    fn on_stream_stop(&self, state: &mut State) {
        // User pressed Esc — prevent spine from immediately relaunching
        SpineState::get_mut(state).config.user_stopped = true;
    }
}

/// Visualizer for spine tool results.
/// Shows configuration changes with before/after values and highlights notification IDs.
fn visualize_spine_output(content: &str, width: usize) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::prelude::*;

    let success_color = Color::Rgb(80, 250, 123);
    let info_color = Color::Rgb(139, 233, 253);
    let warning_color = Color::Rgb(241, 250, 140);
    let error_color = Color::Rgb(255, 85, 85);

    let mut lines = Vec::new();

    for line in content.lines() {
        if line.is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        let style = if line.starts_with("Error:") {
            Style::default().fg(error_color)
        } else if line.starts_with("Marked") {
            Style::default().fg(success_color)
        } else if line.starts_with("Updated") || line.contains("→") {
            Style::default().fg(info_color)
        } else if line.contains("=") || line.contains(":") {
            // Config key-value pairs
            Style::default().fg(info_color)
        } else if line.starts_with("N") && line.chars().nth(1).map_or(false, |c| c.is_ascii_digit()) {
            // Notification IDs like N1, N2
            Style::default().fg(warning_color)
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

/// Prune processed notifications: keep all unprocessed + latest 10 processed.
fn prune_notifications(notifications: &mut Vec<Notification>) {
    let processed_count = notifications.iter().filter(|n| n.processed).count();
    if processed_count <= 10 {
        return;
    }
    // Find the cutoff: we want to keep only the latest 10 processed.
    // Notifications are in chronological order, so we drop the oldest processed ones.
    let mut processed_seen = 0;
    let drop_count = processed_count - 10;
    notifications.retain(|n| {
        if n.processed {
            processed_seen += 1;
            processed_seen > drop_count
        } else {
            true
        }
    });
}
