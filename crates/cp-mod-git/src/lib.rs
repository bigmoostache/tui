pub(crate) mod cache_invalidation;
mod classify;
mod result_panel;
mod tools;
pub mod types;

pub use types::{GitState};


/// Timeout for git commands (seconds)
pub const GIT_CMD_TIMEOUT_SECS: u64 = 30;

/// Refresh interval for git status (milliseconds)
pub(crate) const GIT_STATUS_REFRESH_MS: u64 = 2_000; // 2 seconds

use serde_json::json;

use cp_base::modules::ToolVisualizer;
use cp_base::panels::Panel;
use cp_base::state::{ContextType, State};
use cp_base::tools::{ParamType, ToolDefinition, ToolParam};
use cp_base::tools::{ToolResult, ToolUse};

use self::result_panel::GitResultPanel;
use cp_base::modules::Module;

pub struct GitModule;

impl Module for GitModule {
    fn id(&self) -> &'static str {
        "git"
    }
    fn name(&self) -> &'static str {
        "Git"
    }
    fn description(&self) -> &'static str {
        "Git version control tools and status panel"
    }

    fn init_state(&self, state: &mut State) {
        state.set_ext(GitState::new());
    }

    fn reset_state(&self, state: &mut State) {
        state.set_ext(GitState::new());
    }

    fn save_module_data(&self, state: &State) -> serde_json::Value {
        let gs = GitState::get(state);
        json!({
            "git_diff_base": gs.git_diff_base,
        })
    }

    fn load_module_data(&self, data: &serde_json::Value, state: &mut State) {
        if let Some(v) = data.get("git_diff_base").and_then(|v| v.as_str()) {
            GitState::get_mut(state).git_diff_base = Some(v.to_string());
        }
    }

    fn fixed_panel_types(&self) -> Vec<ContextType> {
        vec![]
    }

    fn dynamic_panel_types(&self) -> Vec<ContextType> {
        vec![ContextType::new(ContextType::GIT_RESULT)]
    }

    fn fixed_panel_defaults(&self) -> Vec<(ContextType, &'static str, bool)> {
        vec![]
    }

    fn create_panel(&self, context_type: &ContextType) -> Option<Box<dyn Panel>> {
        match context_type.as_str() {
            ContextType::GIT_RESULT => Some(Box::new(GitResultPanel)),
            _ => None,
        }
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                id: "git_execute".to_string(),
                name: "Git Execute".to_string(),
                short_desc: "Run git commands".to_string(),
                description: "Executes a git command. Read-only commands (log, diff, show, status, blame, etc.) \
                    create a dynamic result panel that auto-refreshes. Mutating commands (commit, push, pull, \
                    merge, rebase, etc.) execute directly and return output. Shell operators (|, ;, &&) are \
                    not allowed."
                    .to_string(),
                params: vec![
                    ToolParam::new("command", ParamType::String)
                        .desc("Full git command string (e.g., 'git log --oneline -10', 'git commit -m \"message\"')")
                        .required(),
                ],
                enabled: true,
                category: "Git".to_string(),
            },
        ]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "git_execute" => Some(self::tools::execute_git_command(tool, state)),
            _ => None,
        }
    }

    fn tool_visualizers(&self) -> Vec<(&'static str, ToolVisualizer)> {
        vec![
            ("git_execute", visualize_git_output as ToolVisualizer),
        ]
    }

    fn context_type_metadata(&self) -> Vec<cp_base::state::ContextTypeMeta> {
        vec![
            cp_base::state::ContextTypeMeta {
                context_type: "git_result",
                icon_id: "git",
                is_fixed: false,
                needs_cache: true,
                fixed_order: None,
                display_name: "git-result",
                short_name: "git-cmd",
                needs_async_wait: false,
            },
        ]
    }

    fn context_detail(&self, ctx: &cp_base::state::ContextElement) -> Option<String> {
        if ctx.context_type.as_str() == cp_base::state::ContextType::GIT_RESULT {
            Some(ctx.get_meta_str("result_command").unwrap_or("").to_string())
        } else {
            None
        }
    }

    fn overview_context_section(&self, state: &State) -> Option<String> {
        let gs = GitState::get(state);
        if !gs.git_is_repo {
            return None;
        }
        let mut output = String::new();
        if let Some(branch) = &gs.git_branch {
            output.push_str(&format!("\nGit Branch: {}\n", branch));
        }
        Some(output)
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Git", "Version control operations and repository management")]
    }

    fn watch_paths(&self, _state: &State) -> Vec<cp_base::panels::WatchSpec> {
        use cp_base::panels::WatchSpec;
        vec![
            WatchSpec::File(".git/HEAD".to_string()),
            WatchSpec::File(".git/index".to_string()),
            WatchSpec::File(".git/MERGE_HEAD".to_string()),
            WatchSpec::File(".git/REBASE_HEAD".to_string()),
            WatchSpec::File(".git/CHERRY_PICK_HEAD".to_string()),
            WatchSpec::DirRecursive(".git/refs/heads".to_string()),
            WatchSpec::DirRecursive(".git/refs/tags".to_string()),
            WatchSpec::DirRecursive(".git/refs/remotes".to_string()),
        ]
    }

    fn should_invalidate_on_fs_change(
        &self,
        ctx: &cp_base::state::ContextElement,
        changed_path: &str,
        _is_dir_event: bool,
    ) -> bool {
        ctx.context_type.as_str() == ContextType::GIT_RESULT && changed_path.starts_with(".git/")
    }

    fn watcher_immediate_refresh(&self) -> bool {
        false // Prevent feedback loop: git status writes .git/index
    }
}

/// Visualizer for git_execute tool results.
/// Color-codes git command output with branch names in cyan, status indicators,
/// diff hunks with +/- in green/red, file names in yellow.
fn visualize_git_output(content: &str, width: usize) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::prelude::*;

    let success_color = Color::Rgb(80, 250, 123); // Green
    let error_color = Color::Rgb(255, 85, 85); // Red
    let branch_color = Color::Rgb(139, 233, 253); // Cyan
    let warning_color = Color::Rgb(241, 250, 140); // Yellow
    let secondary_color = Color::Rgb(150, 150, 170); // Gray

    let mut lines = Vec::new();

    for line in content.lines() {
        if line.is_empty() {
            lines.push(Line::from(""));
            continue;
        }

        // Determine color based on line content
        let style = if line.starts_with("Panel created:") || line.starts_with("Panel updated:") {
            Style::default().fg(success_color)
        } else if line.starts_with("Error:") || line.starts_with("fatal:") || line.starts_with("error:") {
            Style::default().fg(error_color)
        } else if line.starts_with("+ ") || line.starts_with("+++ ") {
            // Git diff additions
            Style::default().fg(success_color)
        } else if line.starts_with("- ") || line.starts_with("--- ") {
            // Git diff deletions
            Style::default().fg(error_color)
        } else if line.starts_with("@@") {
            // Diff hunk headers
            Style::default().fg(branch_color)
        } else if line.starts_with("commit ") || line.starts_with("Author:") || line.starts_with("Date:") {
            // Git log headers
            Style::default().fg(branch_color)
        } else if line.starts_with("* ") || line.contains("HEAD ->") || line.contains("origin/") {
            // Branch indicators
            Style::default().fg(branch_color)
        } else if line.starts_with("modified:") || line.starts_with("new file:") || line.starts_with("deleted:") {
            // Git status file indicators
            Style::default().fg(warning_color)
        } else if line.starts_with("#") {
            // Git comments
            Style::default().fg(secondary_color)
        } else {
            Style::default()
        };

        // Truncate long lines
        let display = if line.len() > width {
            format!("{}...", &line[..line.floor_char_boundary(width.saturating_sub(3))])
        } else {
            line.to_string()
        };
        lines.push(Line::from(Span::styled(display, style)));
    }

    lines
}
