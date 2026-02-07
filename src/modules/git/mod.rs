pub mod types;
mod panel;
pub(crate) mod tools;

/// Refresh interval for git status (milliseconds)
pub const GIT_STATUS_REFRESH_MS: u64 = 2_000; // 2 seconds

use serde_json::json;

use crate::core::panels::Panel;
use crate::state::{ContextType, State};
use crate::tool_defs::{ToolDefinition, ToolParam, ParamType, ToolCategory};
use crate::tools::{ToolUse, ToolResult};

use self::panel::GitPanel;
use super::Module;

pub struct GitModule;

impl Module for GitModule {
    fn id(&self) -> &'static str { "git" }
    fn name(&self) -> &'static str { "Git" }
    fn description(&self) -> &'static str { "Git version control tools and status panel" }

    fn save_module_data(&self, state: &State) -> serde_json::Value {
        json!({
            "git_show_diffs": state.git_show_diffs,
        })
    }

    fn load_module_data(&self, data: &serde_json::Value, state: &mut State) {
        if let Some(v) = data.get("git_show_diffs").and_then(|v| v.as_bool()) {
            state.git_show_diffs = v;
        }
    }

    fn fixed_panel_types(&self) -> Vec<ContextType> {
        vec![ContextType::Git]
    }

    fn fixed_panel_defaults(&self) -> Vec<(ContextType, &'static str, bool)> {
        vec![(ContextType::Git, "Changes", false)]
    }

    fn create_panel(&self, context_type: ContextType) -> Option<Box<dyn Panel>> {
        match context_type {
            ContextType::Git => Some(Box::new(GitPanel)),
            _ => None,
        }
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                id: "git_toggle_details".to_string(),
                name: "Toggle Git Details".to_string(),
                short_desc: "Show/hide diff content".to_string(),
                description: "Toggles whether the Git panel shows full diff content or just a summary. When disabled, only shows file names and line counts. Useful for reducing context size.".to_string(),
                params: vec![
                    ToolParam::new("show", ParamType::Boolean)
                        .desc("Set true to show diffs, false to hide. Omit to toggle."),
                ],
                enabled: true,
                category: ToolCategory::Git,
            },
            ToolDefinition {
                id: "git_toggle_logs".to_string(),
                name: "Toggle Git Logs".to_string(),
                short_desc: "Show/hide git log".to_string(),
                description: "Toggles whether the Git panel shows recent commit history. Can specify custom git log arguments.".to_string(),
                params: vec![
                    ToolParam::new("show", ParamType::Boolean)
                        .desc("Set true to show logs, false to hide. Omit to toggle."),
                    ToolParam::new("args", ParamType::String)
                        .desc("Custom git log arguments (e.g., '-10 --oneline'). Defaults to '-10 --oneline'."),
                ],
                enabled: true,
                category: ToolCategory::Git,
            },
            ToolDefinition {
                id: "git_commit".to_string(),
                name: "Git Commit".to_string(),
                short_desc: "Commit changes".to_string(),
                description: "Stages specified files (or uses current staging) and creates a git commit. Returns the commit hash and summary of changes.".to_string(),
                params: vec![
                    ToolParam::new("message", ParamType::String)
                        .desc("Commit message")
                        .required(),
                    ToolParam::new("files", ParamType::Array(Box::new(ParamType::String)))
                        .desc("File paths to stage before committing. If empty, commits currently staged files."),
                ],
                enabled: true,
                category: ToolCategory::Git,
            },
            ToolDefinition {
                id: "git_branch_create".to_string(),
                name: "Git Create Branch".to_string(),
                short_desc: "Create new branch".to_string(),
                description: "Creates a new git branch from the current branch and switches to it.".to_string(),
                params: vec![
                    ToolParam::new("name", ParamType::String)
                        .desc("Name for the new branch")
                        .required(),
                ],
                enabled: true,
                category: ToolCategory::Git,
            },
            ToolDefinition {
                id: "git_branch_switch".to_string(),
                name: "Git Switch Branch".to_string(),
                short_desc: "Switch branch".to_string(),
                description: "Switches to another git branch. Fails if there are uncommitted or unstaged changes.".to_string(),
                params: vec![
                    ToolParam::new("branch", ParamType::String)
                        .desc("Branch name to switch to")
                        .required(),
                ],
                enabled: true,
                category: ToolCategory::Git,
            },
            ToolDefinition {
                id: "git_merge".to_string(),
                name: "Git Merge".to_string(),
                short_desc: "Merge branch".to_string(),
                description: "Merges a branch into the current branch. On success, deletes the merged branch.".to_string(),
                params: vec![
                    ToolParam::new("branch", ParamType::String)
                        .desc("Branch name to merge into current branch")
                        .required(),
                ],
                enabled: true,
                category: ToolCategory::Git,
            },
            ToolDefinition {
                id: "git_pull".to_string(),
                name: "Git Pull".to_string(),
                short_desc: "Pull from remote".to_string(),
                description: "Pulls changes from the remote repository (git pull).".to_string(),
                params: vec![],
                enabled: true,
                category: ToolCategory::Git,
            },
            ToolDefinition {
                id: "git_push".to_string(),
                name: "Git Push".to_string(),
                short_desc: "Push to remote".to_string(),
                description: "Pushes local commits to the remote repository (git push).".to_string(),
                params: vec![],
                enabled: true,
                category: ToolCategory::Git,
            },
            ToolDefinition {
                id: "git_fetch".to_string(),
                name: "Git Fetch".to_string(),
                short_desc: "Fetch from remote".to_string(),
                description: "Fetches changes from the remote repository without merging (git fetch).".to_string(),
                params: vec![],
                enabled: true,
                category: ToolCategory::Git,
            },
        ]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        match tool.name.as_str() {
            "git_toggle_details" => Some(self::tools::execute_toggle_details(tool, state)),
            "git_toggle_logs" => Some(self::tools::execute_toggle_logs(tool, state)),
            "git_commit" => Some(self::tools::execute_commit(tool, state)),
            "git_branch_create" => Some(self::tools::execute_create_branch(tool, state)),
            "git_branch_switch" => Some(self::tools::execute_change_branch(tool, state)),
            "git_merge" => Some(self::tools::execute_merge(tool, state)),
            "git_pull" => Some(self::tools::execute_pull(tool, state)),
            "git_push" => Some(self::tools::execute_push(tool, state)),
            "git_fetch" => Some(self::tools::execute_fetch(tool, state)),
            _ => None,
        }
    }
}
