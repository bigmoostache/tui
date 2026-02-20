mod panel;
mod tools;

use cp_base::modules::ToolVisualizer;
use cp_base::panels::Panel;
use cp_base::state::{ContextType, State};
use cp_base::tools::{ParamType, ToolDefinition, ToolParam};
use cp_base::tools::{ToolResult, ToolUse};

use self::panel::FilePanel;
use cp_base::modules::Module;

pub struct FilesModule;

impl Module for FilesModule {
    fn id(&self) -> &'static str {
        "files"
    }
    fn name(&self) -> &'static str {
        "Files"
    }
    fn description(&self) -> &'static str {
        "File open, edit, write, and create tools"
    }
    fn is_core(&self) -> bool {
        true
    }
    fn is_global(&self) -> bool {
        true
    }

    fn dynamic_panel_types(&self) -> Vec<ContextType> {
        vec![ContextType::new(ContextType::FILE)]
    }

    fn create_panel(&self, context_type: &ContextType) -> Option<Box<dyn Panel>> {
        match context_type.as_str() {
            ContextType::FILE => Some(Box::new(FilePanel)),
            _ => None,
        }
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                id: "Open".to_string(),
                name: "Open File".to_string(),
                short_desc: "Read file into context".to_string(),
                description: "Opens a file and adds it to context so you can see its content. ALWAYS use this BEFORE file_edit to see current content - you need exact text for edits.".to_string(),
                params: vec![
                    ToolParam::new("path", ParamType::String)
                        .desc("Path to the file to open")
                        .required(),
                ],
                enabled: true,
                category: "File".to_string(),
            },
            ToolDefinition {
                id: "Edit".to_string(),
                name: "Edit File".to_string(),
                short_desc: "Modify file content".to_string(),
                description: "Edits a file by replacing exact text. PREFERRED over file_write for any modification — only use file_write to create new files or completely replace all content. IMPORTANT: 1) Use file_open FIRST to see current content. 2) old_string must be EXACT text from file (copy from context). 3) To append, use the last line as old_string and include it + new content in new_string.".to_string(),
                params: vec![
                    ToolParam::new("file_path", ParamType::String)
                        .desc("Absolute path to the file to edit")
                        .required(),
                    ToolParam::new("old_string", ParamType::String)
                        .desc("Exact text to find and replace (copy from file context)")
                        .required(),
                    ToolParam::new("new_string", ParamType::String)
                        .desc("Replacement text")
                        .required(),
                    ToolParam::new("replace_all", ParamType::Boolean)
                        .desc("Replace all occurrences (default: false)"),
                ],
                enabled: true,
                category: "File".to_string(),
            },
            ToolDefinition {
                id: "Write".to_string(),
                name: "Write File".to_string(),
                short_desc: "Create or overwrite file".to_string(),
                description: "Writes complete contents to a file, creating it if it doesn't exist or replacing all content if it does. Use ONLY for creating new files or completely replacing file content. For targeted edits (changing specific sections, appending, inserting), ALWAYS prefer file_edit instead — it is safer and more precise.".to_string(),
                params: vec![
                    ToolParam::new("file_path", ParamType::String)
                        .desc("Path to the file to write")
                        .required(),
                    ToolParam::new("contents", ParamType::String)
                        .desc("Complete file contents to write")
                        .required(),
                ],
                enabled: true,
                category: "File".to_string(),
            },


        ]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "Open" => Some(self::tools::file::execute_open(tool, state)),
            "Edit" => Some(self::tools::edit_file::execute_edit(tool, state)),
            "Write" => Some(self::tools::write::execute(tool, state)),

            _ => None,
        }
    }

    fn tool_visualizers(&self) -> Vec<(&'static str, ToolVisualizer)> {
        vec![("Edit", visualize_diff as ToolVisualizer), ("Write", visualize_diff as ToolVisualizer)]
    }

    fn context_type_metadata(&self) -> Vec<cp_base::state::ContextTypeMeta> {
        vec![cp_base::state::ContextTypeMeta {
            context_type: "file",
            icon_id: "file",
            is_fixed: false,
            needs_cache: true,
            fixed_order: None,
            display_name: "file",
            short_name: "file",
            needs_async_wait: true,
        }]
    }

    fn context_detail(&self, ctx: &cp_base::state::ContextElement) -> Option<String> {
        if ctx.context_type.as_str() == cp_base::state::ContextType::FILE {
            Some(ctx.get_meta_str("file_path").unwrap_or("").to_string())
        } else {
            None
        }
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("File", "Read, write, and search files in the project")]
    }

    fn watch_paths(&self, state: &State) -> Vec<cp_base::panels::WatchSpec> {
        state
            .context
            .iter()
            .filter(|c| c.context_type.as_str() == ContextType::FILE)
            .filter_map(|c| c.get_meta_str("file_path").map(|p| cp_base::panels::WatchSpec::File(p.to_string())))
            .collect()
    }

    fn should_invalidate_on_fs_change(
        &self,
        ctx: &cp_base::state::ContextElement,
        changed_path: &str,
        is_dir_event: bool,
    ) -> bool {
        if is_dir_event {
            return false;
        }
        ctx.context_type.as_str() == ContextType::FILE && ctx.get_meta_str("file_path") == Some(changed_path)
    }
}

/// Visualizer for Edit and Write tool results.
/// Also reused by cp-mod-prompt for Edit_prompt.
/// Parses ```diff blocks and renders deleted lines in red, added lines in green.
/// Non-diff content is rendered in secondary text color.
pub fn visualize_diff(content: &str, width: usize) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::prelude::*;

    let error_color = Color::Rgb(255, 85, 85);
    let success_color = Color::Rgb(80, 250, 123);
    let secondary_color = Color::Rgb(150, 150, 170);

    let mut lines = Vec::new();
    let mut in_diff_block = false;

    for line in content.lines() {
        // Detect diff block markers
        if line.trim() == "```diff" {
            in_diff_block = true;
            continue;
        }
        if line.trim() == "```" && in_diff_block {
            in_diff_block = false;
            continue;
        }

        if line.is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        if in_diff_block {
            let style = if line.starts_with("- ") {
                Style::default().fg(error_color)
            } else if line.starts_with("+ ") {
                Style::default().fg(success_color)
            } else {
                Style::default().fg(secondary_color)
            };

            // Truncate long lines to available width
            let display = if line.len() > width {
                format!("{}...", &line[..line.floor_char_boundary(width.saturating_sub(3))])
            } else {
                line.to_string()
            };
            lines.push(Line::from(Span::styled(display, style)));
        } else {
            // Non-diff content: plain secondary text
            let display = if line.len() > width {
                format!("{}...", &line[..line.floor_char_boundary(width.saturating_sub(3))])
            } else {
                line.to_string()
            };
            lines.push(Line::from(Span::styled(display, Style::default().fg(secondary_color))));
        }
    }

    lines
}
