pub(crate) mod cache_invalidation;
mod classify;
mod panel;
mod tools;
pub mod types;

pub use types::{GitCacheUpdate, GitChangeType, GitFileChange, GitState};

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

use self::panel::{GitPanel, GitResultPanel};
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
            "git_show_diffs": gs.git_show_diffs,
            "git_diff_base": gs.git_diff_base,
        })
    }

    fn load_module_data(&self, data: &serde_json::Value, state: &mut State) {
        if let Some(v) = data.get("git_show_diffs").and_then(|v| v.as_bool()) {
            GitState::get_mut(state).git_show_diffs = v;
        }
        if let Some(v) = data.get("git_diff_base").and_then(|v| v.as_str()) {
            GitState::get_mut(state).git_diff_base = Some(v.to_string());
        }
    }

    fn fixed_panel_types(&self) -> Vec<ContextType> {
        vec![ContextType::new(ContextType::GIT)]
    }

    fn dynamic_panel_types(&self) -> Vec<ContextType> {
        vec![ContextType::new(ContextType::GIT_RESULT)]
    }

    fn fixed_panel_defaults(&self) -> Vec<(ContextType, &'static str, bool)> {
        vec![(ContextType::new(ContextType::GIT), "Changes", false)]
    }

    fn create_panel(&self, context_type: &ContextType) -> Option<Box<dyn Panel>> {
        match context_type.as_str() {
            ContextType::GIT => Some(Box::new(GitPanel)),
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
            ToolDefinition {
                id: "git_configure_p6".to_string(),
                name: "Configure Git Panel".to_string(),
                short_desc: "Configure P6 panel".to_string(),
                description: "Configures the P6 git status panel. Can toggle diff display, log display, \
                    change log arguments, and set a diff base ref for comparison."
                    .to_string(),
                params: vec![
                    ToolParam::new("show_diffs", ParamType::Boolean).desc("Show full diff content in P6 panel"),
                    ToolParam::new("show_logs", ParamType::Boolean).desc("Show recent commit history in P6 panel"),
                    ToolParam::new("log_args", ParamType::String)
                        .desc("Custom git log arguments (e.g., '-10 --oneline')"),
                    ToolParam::new("diff_base", ParamType::String)
                        .desc("Git ref to diff against (e.g., 'HEAD~3', 'main'). Set to empty string to clear."),
                ],
                enabled: true,
                category: "Git".to_string(),
            },
        ]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "git_execute" => Some(self::tools::execute_git_command(tool, state)),
            "git_configure_p6" => Some(self::tools::execute_configure_p6(tool, state)),
            _ => None,
        }
    }

    fn tool_visualizers(&self) -> Vec<(&'static str, ToolVisualizer)> {
        vec![
            ("git_execute", visualize_git_output as ToolVisualizer),
            ("git_configure_p6", visualize_git_config as ToolVisualizer),
        ]
    }

    fn context_type_metadata(&self) -> Vec<cp_base::state::ContextTypeMeta> {
        vec![
            cp_base::state::ContextTypeMeta {
                context_type: "git",
                icon_id: "git",
                is_fixed: true,
                needs_cache: true,
                fixed_order: Some(7),
                display_name: "git",
                short_name: "changes",
                needs_async_wait: false,
            },
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
        if gs.git_file_changes.is_empty() {
            output.push_str("Git Status: Working tree clean\n");
        } else {
            output.push_str("\nGit Changes:\n\n");
            output.push_str("| File | + | - | Net |\n");
            output.push_str("|------|---|---|-----|\n");
            let mut total_add: i32 = 0;
            let mut total_del: i32 = 0;
            for file in &gs.git_file_changes {
                total_add += file.additions;
                total_del += file.deletions;
                let net = file.additions - file.deletions;
                let net_str = if net >= 0 { format!("+{}", net) } else { format!("{}", net) };
                output.push_str(&format!(
                    "| {} | +{} | -{} | {} |\n",
                    file.path, file.additions, file.deletions, net_str
                ));
            }
            let total_net = total_add - total_del;
            let total_net_str = if total_net >= 0 { format!("+{}", total_net) } else { format!("{}", total_net) };
            output.push_str(&format!(
                "| **Total** | **+{}** | **-{}** | **{}** |\n",
                total_add, total_del, total_net_str
            ));
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
        let ct = ctx.context_type.as_str();
        (ct == ContextType::GIT || ct == ContextType::GIT_RESULT) && changed_path.starts_with(".git/")
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

/// Visualizer for git_configure_p6 tool results.
/// Highlights configuration changes with success color and shows before/after values.
fn visualize_git_config(content: &str, width: usize) -> Vec<ratatui::text::Line<'static>> {
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
        } else if line.contains("Updated") || line.contains("Configured") || line.contains("enabled") || line.contains("disabled") {
            Style::default().fg(success_color)
        } else if line.contains("->") || line.contains(":") {
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
