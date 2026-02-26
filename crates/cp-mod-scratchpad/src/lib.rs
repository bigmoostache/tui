mod panel;
mod tools;
pub mod types;

pub use types::{ScratchpadCell, ScratchpadState};

use serde_json::json;

use cp_base::modules::ToolVisualizer;
use cp_base::panels::Panel;
use cp_base::state::{ContextType, State};
use cp_base::tools::{ParamType, ToolDefinition, ToolParam};
use cp_base::tools::{ToolResult, ToolUse};

use self::panel::ScratchpadPanel;
use cp_base::modules::Module;

pub struct ScratchpadModule;

impl Module for ScratchpadModule {
    fn id(&self) -> &'static str {
        "scratchpad"
    }
    fn name(&self) -> &'static str {
        "Scratchpad"
    }
    fn description(&self) -> &'static str {
        "Temporary note-taking scratchpad cells"
    }

    fn init_state(&self, state: &mut State) {
        state.set_ext(ScratchpadState::new());
    }

    fn reset_state(&self, state: &mut State) {
        state.set_ext(ScratchpadState::new());
    }

    fn save_module_data(&self, state: &State) -> serde_json::Value {
        let ss = ScratchpadState::get(state);
        json!({
            "scratchpad_cells": ss.scratchpad_cells,
            "next_scratchpad_id": ss.next_scratchpad_id,
        })
    }

    fn load_module_data(&self, data: &serde_json::Value, state: &mut State) {
        let ss = ScratchpadState::get_mut(state);
        if let Some(arr) = data.get("scratchpad_cells")
            && let Ok(v) = serde_json::from_value(arr.clone())
        {
            ss.scratchpad_cells = v;
        }
        if let Some(v) = data.get("next_scratchpad_id").and_then(|v| v.as_u64()) {
            ss.next_scratchpad_id = v as usize;
        }
    }

    fn fixed_panel_types(&self) -> Vec<ContextType> {
        vec![ContextType::new(ContextType::SCRATCHPAD)]
    }

    fn fixed_panel_defaults(&self) -> Vec<(ContextType, &'static str, bool)> {
        vec![(ContextType::new(ContextType::SCRATCHPAD), "Scratch", false)]
    }

    fn create_panel(&self, context_type: &ContextType) -> Option<Box<dyn Panel>> {
        match context_type.as_str() {
            ContextType::SCRATCHPAD => Some(Box::new(ScratchpadPanel)),
            _ => None,
        }
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                id: "scratchpad_create_cell".to_string(),
                name: "Create Scratchpad Cell".to_string(),
                short_desc: "Add scratchpad cell".to_string(),
                description: "Creates a new scratchpad cell for storing temporary notes, code snippets, or data during the conversation.".to_string(),
                params: vec![
                    ToolParam::new("cell_title", ParamType::String)
                        .desc("Title for the cell")
                        .required(),
                    ToolParam::new("cell_contents", ParamType::String)
                        .desc("Content to store in the cell")
                        .required(),
                ],
                enabled: true,
                reverie_allowed: true,
                category: "Scratchpad".to_string(),
            },
            ToolDefinition {
                id: "scratchpad_edit_cell".to_string(),
                name: "Edit Scratchpad Cell".to_string(),
                short_desc: "Modify scratchpad cell".to_string(),
                description: "Edits an existing scratchpad cell. Can update title and/or contents.".to_string(),
                params: vec![
                    ToolParam::new("cell_id", ParamType::String)
                        .desc("Cell ID to edit (e.g., C1)")
                        .required(),
                    ToolParam::new("cell_title", ParamType::String)
                        .desc("New title (omit to keep current)"),
                    ToolParam::new("cell_contents", ParamType::String)
                        .desc("New contents (omit to keep current)"),
                ],
                enabled: true,
                reverie_allowed: true,
                category: "Scratchpad".to_string(),
            },
            ToolDefinition {
                id: "scratchpad_wipe".to_string(),
                name: "Wipe Scratchpad".to_string(),
                short_desc: "Delete scratchpad cells".to_string(),
                description: "Deletes scratchpad cells by their IDs. Pass an empty array to wipe all cells.".to_string(),
                params: vec![
                    ToolParam::new("cell_ids", ParamType::Array(Box::new(ParamType::String)))
                        .desc("Cell IDs to delete (e.g., ['C1', 'C2']). Empty array deletes all cells.")
                        .required(),
                ],
                enabled: true,
                reverie_allowed: true,
                category: "Scratchpad".to_string(),
            },
        ]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "scratchpad_create_cell" => Some(self::tools::execute_create(tool, state)),
            "scratchpad_edit_cell" => Some(self::tools::execute_edit(tool, state)),
            "scratchpad_wipe" => Some(self::tools::execute_wipe(tool, state)),
            _ => None,
        }
    }

    fn tool_visualizers(&self) -> Vec<(&'static str, ToolVisualizer)> {
        vec![
            ("scratchpad_create_cell", visualize_scratchpad_output as ToolVisualizer),
            ("scratchpad_edit_cell", visualize_scratchpad_output as ToolVisualizer),
            ("scratchpad_wipe", visualize_scratchpad_output as ToolVisualizer),
        ]
    }

    fn context_type_metadata(&self) -> Vec<cp_base::state::ContextTypeMeta> {
        vec![cp_base::state::ContextTypeMeta {
            context_type: "scratchpad",
            icon_id: "scratchpad",
            is_fixed: true,
            needs_cache: false,
            fixed_order: Some(8),
            display_name: "scratchpad",
            short_name: "scratch",
            needs_async_wait: false,
        }]
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Scratchpad", "A useful scratchpad for you to use however you like")]
    }
}

/// Visualizer for scratchpad tool results.
/// Highlights cell titles and shows creation vs edit vs deletion actions.
fn visualize_scratchpad_output(content: &str, width: usize) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::prelude::*;

    let success_color = Color::Rgb(80, 250, 123);
    let info_color = Color::Rgb(139, 233, 253);
    let error_color = Color::Rgb(255, 85, 85);
    let secondary_color = Color::Rgb(150, 150, 170);

    let mut lines = Vec::new();

    for line in content.lines() {
        if line.is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        let style = if line.starts_with("Error:") {
            Style::default().fg(error_color)
        } else if line.starts_with("Created cell") {
            Style::default().fg(success_color)
        } else if line.starts_with("Updated") {
            Style::default().fg(info_color)
        } else if line.starts_with("Deleted") {
            Style::default().fg(error_color)
        } else if line.starts_with("C") && line.chars().nth(1).is_some_and(|c| c.is_ascii_digit()) {
            // Cell IDs like C1, C2
            Style::default().fg(info_color)
        } else if line.contains(":") {
            // Cell titles
            Style::default().fg(secondary_color)
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
