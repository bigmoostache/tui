mod panel;
mod tools;

use cp_base::modules::ToolVisualizer;
use cp_base::panels::Panel;
use cp_base::state::{ContextType, State};
use cp_base::tool_defs::{ParamType, ToolDefinition, ToolParam};
use cp_base::tools::{ToolResult, ToolUse};

use self::panel::GrepPanel;
use cp_base::modules::Module;

pub struct GrepModule;

impl Module for GrepModule {
    fn id(&self) -> &'static str {
        "grep"
    }
    fn name(&self) -> &'static str {
        "Grep"
    }
    fn description(&self) -> &'static str {
        "Content search across files"
    }

    fn dynamic_panel_types(&self) -> Vec<ContextType> {
        vec![ContextType::new(ContextType::GREP)]
    }

    fn create_panel(&self, context_type: &ContextType) -> Option<Box<dyn Panel>> {
        match context_type.as_str() {
            ContextType::GREP => Some(Box::new(GrepPanel)),
            _ => None,
        }
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            id: "file_grep".to_string(),
            name: "Grep Search".to_string(),
            short_desc: "Search file contents".to_string(),
            description: "Searches file contents for a regex pattern. Results show matching lines with file:line \
                          context. Results are added to context and update dynamically."
                .to_string(),
            params: vec![
                ToolParam::new("pattern", ParamType::String).desc("Regex pattern to search for").required(),
                ToolParam::new("path", ParamType::String).desc("Base path to search from").default_val("."),
                ToolParam::new("file_pattern", ParamType::String)
                    .desc("Glob pattern to filter files (e.g., '*.rs', '*.ts')"),
            ],
            enabled: true,
            category: "File".to_string(),
        }]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "file_grep" => Some(self::tools::execute(tool, state)),
            _ => None,
        }
    }

    fn tool_visualizers(&self) -> Vec<(&'static str, ToolVisualizer)> {
        vec![("file_grep", visualize_grep_results as ToolVisualizer)]
    }

    fn context_type_metadata(&self) -> Vec<cp_base::state::ContextTypeMeta> {
        vec![cp_base::state::ContextTypeMeta {
            context_type: "grep",
            icon_id: "grep",
            is_fixed: false,
            needs_cache: true,
            fixed_order: None,
            display_name: "grep",
            short_name: "grep",
            needs_async_wait: false,
        }]
    }

    fn context_detail(&self, ctx: &cp_base::state::ContextElement) -> Option<String> {
        if ctx.context_type.as_str() == cp_base::state::ContextType::GREP {
            Some(ctx.get_meta_str("grep_pattern").unwrap_or("").to_string())
        } else {
            None
        }
    }

    fn watch_paths(&self, state: &State) -> Vec<cp_base::panels::WatchSpec> {
        state
            .context
            .iter()
            .filter(|c| c.context_type.as_str() == ContextType::GREP)
            .map(|c| cp_base::panels::WatchSpec::Dir(c.get_meta_str("grep_path").unwrap_or(".").to_string()))
            .collect()
    }

    fn should_invalidate_on_fs_change(
        &self,
        ctx: &cp_base::state::ContextElement,
        changed_path: &str,
        _is_dir_event: bool,
    ) -> bool {
        if ctx.context_type.as_str() != ContextType::GREP {
            return false;
        }
        let base = ctx.get_meta_str("grep_path").unwrap_or(".");
        changed_path.starts_with(base) || base.starts_with(changed_path)
    }
}

/// Visualizer for file_grep tool results.
/// Highlights the matched pattern and colors file paths vs line numbers vs content.
fn visualize_grep_results(content: &str, width: usize) -> Vec<ratatui::text::Line<'static>> {
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
        } else if line.starts_with("Created grep") {
            Style::default().fg(success_color)
        } else if line.contains("'") {
            // Highlight pattern in quotes
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
