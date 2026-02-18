use std::collections::HashMap;
use std::process::Command;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;

use super::GIT_CMD_TIMEOUT_SECS;
use super::GIT_STATUS_REFRESH_MS;
use cp_base::state::Action;
use cp_base::panels::{CacheRequest, CacheUpdate, hash_content};
use cp_base::config::{SCROLL_ARROW_AMOUNT, SCROLL_PAGE_AMOUNT};
use cp_base::modules::run_with_timeout;
use cp_base::panels::{ContextItem, Panel, paginate_content, update_if_changed};
use cp_base::state::{ContextElement, ContextType, State, compute_total_pages, estimate_tokens};

use crate::panel_render::{format_git_content_for_cache, parse_diff_by_file, parse_numstat_to_map};
use crate::types::{GitCacheUpdate, GitChangeType, GitFileChange, GitState, GitStatusRequest};

pub(crate) use crate::result_panel::GitResultPanel;

/// Create a git Command with the given arguments (helper for run_with_timeout which takes Command by value).
fn git_cmd(args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    cmd.args(args);
    cmd
}

pub struct GitPanel;

impl GitPanel {
    /// Format git status for LLM context (as markdown table + diffs)
    fn format_git_for_context(state: &State) -> String {
        let gs = GitState::get(state);
        if !gs.git_is_repo {
            return "Not a git repository".to_string();
        }

        let mut output = String::new();

        // Branch
        if let Some(branch) = &gs.git_branch {
            output.push_str(&format!("Branch: {}\n", branch));
        }

        if gs.git_file_changes.is_empty() {
            output.push_str("\nWorking tree clean\n");
        } else {
            output.push_str("\n| File | Type | + | - | Net |\n");
            output.push_str("|------|------|---|---|-----|\n");

            let mut total_add: i32 = 0;
            let mut total_del: i32 = 0;

            for file in &gs.git_file_changes {
                total_add += file.additions;
                total_del += file.deletions;
                let net = file.additions - file.deletions;
                let net_str = if net >= 0 { format!("+{}", net) } else { format!("{}", net) };
                let type_str = match file.change_type {
                    GitChangeType::Added => "A",
                    GitChangeType::Untracked => "U",
                    GitChangeType::Deleted => "D",
                    GitChangeType::Modified => "M",
                    GitChangeType::Renamed => "R",
                };
                output.push_str(&format!(
                    "| {} | {} | +{} | -{} | {} |\n",
                    file.path, type_str, file.additions, file.deletions, net_str
                ));
            }

            let total_net = total_add - total_del;
            let total_net_str = if total_net >= 0 { format!("+{}", total_net) } else { format!("{}", total_net) };
            output.push_str(&format!(
                "| **Total** | | **+{}** | **-{}** | **{}** |\n",
                total_add, total_del, total_net_str
            ));

            // Add diff content only if git_show_diffs is enabled
            if gs.git_show_diffs {
                output.push_str("\n## Diffs\n\n");
                for file in &gs.git_file_changes {
                    if !file.diff_content.is_empty() {
                        output.push_str("```diff\n");
                        output.push_str(&file.diff_content);
                        output.push_str("```\n\n");
                    }
                }
            }
        }

        output
    }
}

impl Panel for GitPanel {
    fn needs_cache(&self) -> bool {
        true
    }

    fn build_cache_request(&self, ctx: &ContextElement, state: &State) -> Option<CacheRequest> {
        // Force full refresh if cache is explicitly deprecated (e.g., toggle_diffs)
        let current_source_hash = if ctx.cache_deprecated { None } else { ctx.source_hash.clone() };
        let gs = GitState::get(state);
        Some(CacheRequest {
            context_type: ContextType::new(ContextType::GIT),
            data: Box::new(GitStatusRequest {
                show_diffs: gs.git_show_diffs,
                current_source_hash,
                diff_base: gs.git_diff_base.clone(),
            }),
        })
    }

    fn apply_cache_update(&self, update: CacheUpdate, ctx: &mut ContextElement, state: &mut State) -> bool {
        match update {
            CacheUpdate::ModuleSpecific { data, .. } => {
                if let Ok(update) = data.downcast::<GitCacheUpdate>() {
                    match *update {
                        GitCacheUpdate::Status {
                            branch,
                            is_repo,
                            file_changes,
                            branches,
                            formatted_content,
                            token_count,
                            source_hash,
                        } => {
                            let gs = GitState::get_mut(state);
                            gs.git_branch = branch;
                            gs.git_branches = branches;
                            gs.git_is_repo = is_repo;
                            gs.git_file_changes = file_changes
                                .into_iter()
                                .map(|(path, additions, deletions, change_type, diff_content)| GitFileChange {
                                    path,
                                    additions,
                                    deletions,
                                    change_type,
                                    diff_content,
                                })
                                .collect();
                            ctx.source_hash = Some(source_hash);
                            ctx.cached_content = Some(formatted_content);
                            ctx.full_token_count = token_count;
                            ctx.total_pages = compute_total_pages(token_count);
                            ctx.current_page = 0;
                            if ctx.total_pages > 1 {
                                let page_content = paginate_content(
                                    ctx.cached_content.as_deref().unwrap_or(""),
                                    ctx.current_page,
                                    ctx.total_pages,
                                );
                                ctx.token_count = estimate_tokens(&page_content);
                            } else {
                                ctx.token_count = token_count;
                            }
                            ctx.cache_deprecated = false;
                            let content_ref = ctx.cached_content.clone().unwrap_or_default();
                            update_if_changed(ctx, &content_ref);
                            true
                        }
                        GitCacheUpdate::StatusUnchanged => {
                            ctx.cache_deprecated = false;
                            false
                        }
                    }
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn cache_refresh_interval_ms(&self) -> Option<u64> {
        Some(GIT_STATUS_REFRESH_MS)
    }

    fn handle_key(&self, key: &KeyEvent, _state: &State) -> Option<Action> {
        match key.code {
            KeyCode::Up => Some(Action::ScrollUp(SCROLL_ARROW_AMOUNT)),
            KeyCode::Down => Some(Action::ScrollDown(SCROLL_ARROW_AMOUNT)),
            KeyCode::PageUp => Some(Action::ScrollUp(SCROLL_PAGE_AMOUNT)),
            KeyCode::PageDown => Some(Action::ScrollDown(SCROLL_PAGE_AMOUNT)),
            _ => None,
        }
    }

    fn title(&self, state: &State) -> String {
        let gs = GitState::get(state);
        let base_title =
            if let Some(branch) = &gs.git_branch { format!("Git ({})", branch) } else { "Git".to_string() };
        if let Some(ref diff_base) = gs.git_diff_base {
            format!("{} [vs {}]", base_title, diff_base)
        } else {
            base_title
        }
    }

    fn refresh(&self, state: &mut State) {
        let needs_calc = state
            .context
            .iter()
            .find(|c| c.context_type == ContextType::GIT)
            .map(|ctx| ctx.cached_content.is_none())
            .unwrap_or(false);

        if needs_calc {
            let git_content = Self::format_git_for_context(state);
            let token_count = estimate_tokens(&git_content);
            for ctx in &mut state.context {
                if ctx.context_type == ContextType::GIT {
                    ctx.token_count = token_count;
                    break;
                }
            }
        }
    }

    fn refresh_cache(&self, request: CacheRequest) -> Option<CacheUpdate> {
        let req = request.data.downcast::<GitStatusRequest>().ok()?;
        let GitStatusRequest { show_diffs, current_source_hash, diff_base } = *req;

        let is_repo =
            Command::new("git").args(["rev-parse", "--git-dir"]).output().map(|o| o.status.success()).unwrap_or(false);

        if !is_repo {
            return Some(CacheUpdate::ModuleSpecific {
                context_type: ContextType::new(ContextType::GIT),
                data: Box::new(GitCacheUpdate::Status {
                    branch: None,
                    is_repo: false,
                    file_changes: vec![],
                    branches: vec![],
                    formatted_content: "Not a git repository".to_string(),
                    token_count: estimate_tokens("Not a git repository"),
                    source_hash: hash_content("not_a_repo"),
                }),
            });
        }

        let status_output = run_with_timeout(git_cmd(&["status", "--porcelain"]), GIT_CMD_TIMEOUT_SECS)
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();

        let branch_output =
            run_with_timeout(git_cmd(&["rev-parse", "--abbrev-ref", "HEAD"]), GIT_CMD_TIMEOUT_SECS)
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
                .unwrap_or_default();

        let new_hash = hash_content(&format!("{}\n{}", branch_output, status_output));

        if current_source_hash.as_ref() == Some(&new_hash) {
            return Some(CacheUpdate::ModuleSpecific {
                context_type: ContextType::new(ContextType::GIT),
                data: Box::new(GitCacheUpdate::StatusUnchanged),
            });
        }

        let branch = if branch_output == "HEAD" {
            run_with_timeout(git_cmd(&["rev-parse", "--short", "HEAD"]), GIT_CMD_TIMEOUT_SECS)
                .ok()
                .map(|o| format!("detached:{}", String::from_utf8_lossy(&o.stdout).trim()))
        } else if branch_output.is_empty() {
            None
        } else {
            Some(branch_output)
        };

        let mut file_changes: HashMap<String, (i32, i32, GitChangeType)> = HashMap::new();

        for line in status_output.lines() {
            if line.len() < 3 {
                continue;
            }
            let x = line.chars().next().unwrap_or(' ');
            let y = line.chars().nth(1).unwrap_or(' ');
            let path = line[3..].trim().to_string();
            let path =
                if path.contains(" -> ") { path.split(" -> ").last().unwrap_or(&path).to_string() } else { path };

            let change_type = match (x, y) {
                ('?', '?') => GitChangeType::Untracked,
                ('A', _) | (_, 'A') => GitChangeType::Added,
                ('D', _) | (_, 'D') => GitChangeType::Deleted,
                ('R', _) | (_, 'R') => GitChangeType::Renamed,
                _ => GitChangeType::Modified,
            };

            file_changes.entry(path).or_insert((0, 0, change_type));
        }

        if !file_changes.is_empty() {
            if let Some(ref base) = diff_base {
                if let Ok(output) = run_with_timeout(git_cmd(&["diff", base, "--numstat"]), GIT_CMD_TIMEOUT_SECS)
                    && output.status.success()
                {
                    parse_numstat_to_map(&String::from_utf8_lossy(&output.stdout), &mut file_changes);
                }
            } else {
                if let Ok(output) = run_with_timeout(git_cmd(&["diff", "--cached", "--numstat"]), GIT_CMD_TIMEOUT_SECS)
                    && output.status.success()
                {
                    parse_numstat_to_map(&String::from_utf8_lossy(&output.stdout), &mut file_changes);
                }
                if let Ok(output) = run_with_timeout(git_cmd(&["diff", "--numstat"]), GIT_CMD_TIMEOUT_SECS)
                    && output.status.success()
                {
                    parse_numstat_to_map(&String::from_utf8_lossy(&output.stdout), &mut file_changes);
                }
            }

            let untracked_files: Vec<String> = file_changes
                .iter()
                .filter(|(_, (add, del, ct))| *ct == GitChangeType::Untracked && *add == 0 && *del == 0)
                .map(|(path, _)| path.clone())
                .collect();
            for path in untracked_files {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    let lines = content.lines().count() as i32;
                    if let Some(entry) = file_changes.get_mut(&path) {
                        entry.0 = lines;
                    }
                }
            }

            let deleted_files: Vec<String> = file_changes
                .iter()
                .filter(|(_, (add, del, ct))| *ct == GitChangeType::Deleted && *add == 0 && *del == 0)
                .map(|(path, _)| path.clone())
                .collect();
            for path in deleted_files {
                let head_path = format!("HEAD:{}", path);
                if let Ok(output) = run_with_timeout(git_cmd(&["show", &head_path]), GIT_CMD_TIMEOUT_SECS)
                    && output.status.success()
                {
                    let content = String::from_utf8_lossy(&output.stdout);
                    let lines = content.lines().count() as i32;
                    if let Some(entry) = file_changes.get_mut(&path) {
                        entry.1 = lines;
                    }
                }
            }
        }

        let mut changes: Vec<_> =
            file_changes.into_iter().map(|(path, (add, del, ct))| (path, add, del, ct, String::new())).collect();
        changes.sort_by(|a, b| a.0.cmp(&b.0));

        let branches: Vec<(String, bool)> = run_with_timeout(
            git_cmd(&["branch", "--format=%(refname:short)"]),
            GIT_CMD_TIMEOUT_SECS,
        )
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            let current = branch.as_deref().unwrap_or("");
            String::from_utf8_lossy(&o.stdout).lines().map(|b| (b.to_string(), b == current)).collect()
        })
        .unwrap_or_default();

        if show_diffs && !changes.is_empty() {
            let mut diff_contents: HashMap<String, String> = HashMap::new();

            let diff_ref = diff_base.as_deref().unwrap_or("HEAD");
            if let Ok(output) = run_with_timeout(git_cmd(&["diff", diff_ref]), GIT_CMD_TIMEOUT_SECS)
                && output.status.success()
            {
                let diff_output = String::from_utf8_lossy(&output.stdout);
                parse_diff_by_file(&diff_output, &mut diff_contents);
            }

            for (path, _, _, ct, _) in &changes {
                if *ct == GitChangeType::Untracked
                    && !diff_contents.contains_key(path)
                    && let Ok(content) = std::fs::read_to_string(path)
                {
                    let mut pseudo_diff = format!("diff --git a/{} b/{}\n", path, path);
                    pseudo_diff.push_str("new file\n");
                    pseudo_diff.push_str(&format!("--- /dev/null\n+++ b/{}\n", path));
                    pseudo_diff.push_str("@@ -0,0 +1 @@\n");
                    for line in content.lines() {
                        pseudo_diff.push_str(&format!("+{}\n", line));
                    }
                    diff_contents.insert(path.clone(), pseudo_diff);
                }
            }

            for (path, _, _, ct, _) in &changes {
                if *ct == GitChangeType::Deleted && !diff_contents.contains_key(path) {
                    let head_path = format!("HEAD:{}", path);
                    if let Ok(output) = run_with_timeout(git_cmd(&["show", &head_path]), GIT_CMD_TIMEOUT_SECS)
                        && output.status.success()
                    {
                        let content = String::from_utf8_lossy(&output.stdout);
                        let mut pseudo_diff = format!("diff --git a/{} b/{}\n", path, path);
                        pseudo_diff.push_str("deleted file\n");
                        pseudo_diff.push_str(&format!("--- a/{}\n+++ /dev/null\n", path));
                        pseudo_diff.push_str("@@ -1 +0,0 @@\n");
                        for line in content.lines() {
                            pseudo_diff.push_str(&format!("-{}\n", line));
                        }
                        diff_contents.insert(path.clone(), pseudo_diff);
                    }
                }
            }

            for (path, _, _, _, diff) in &mut changes {
                if let Some(d) = diff_contents.remove(path) {
                    *diff = d;
                }
            }
        }

        let formatted_content = format_git_content_for_cache(&branch, &changes, show_diffs);
        let token_count = estimate_tokens(&formatted_content);

        Some(CacheUpdate::ModuleSpecific {
            context_type: ContextType::new(ContextType::GIT),
            data: Box::new(GitCacheUpdate::Status {
                branch,
                is_repo: true,
                file_changes: changes,
                branches,
                formatted_content,
                token_count,
                source_hash: new_hash,
            }),
        })
    }

    fn context(&self, state: &State) -> Vec<ContextItem> {
        if !GitState::get(state).git_is_repo {
            return vec![];
        }

        let git_ctx = state.context.iter().find(|c| c.context_type == ContextType::GIT);

        let content = git_ctx
            .and_then(|ctx| ctx.cached_content.as_ref())
            .map(|c| {
                let is_deprecated = git_ctx.map(|ctx| ctx.cache_deprecated).unwrap_or(false);
                if is_deprecated { format!("[refreshing...]\n{}", c) } else { c.clone() }
            })
            .unwrap_or_else(|| Self::format_git_for_context(state));

        let (id, last_refresh_ms, current_page, total_pages) = git_ctx
            .map(|c| (c.id.as_str(), c.last_refresh_ms, c.current_page, c.total_pages))
            .unwrap_or(("P6", 0, 0, 1));
        let output = paginate_content(&content, current_page, total_pages);
        vec![ContextItem::new(id, "Git Status", output, last_refresh_ms)]
    }

    fn content(&self, state: &State, base_style: Style) -> Vec<Line<'static>> {
        crate::panel_render::render_git_panel_content(state, base_style)
    }
}
