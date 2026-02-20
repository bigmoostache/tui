//! Callback trigger engine: collect changed files, match patterns, fire callbacks.
//!
//! Called from tool_pipeline.rs after a batch of Edit/Write tools completes.

use std::path::Path;

use globset::Glob;

use cp_base::config::STORE_DIR;
use cp_base::panels::now_ms;
use cp_base::state::State;
use cp_base::watchers::{DeferredPanel, Watcher, WatcherRegistry, WatcherResult};

use cp_mod_console::manager::SessionHandle;
use cp_mod_console::types::ConsoleState;

use crate::types::{CallbackDefinition, CallbackState};

/// A callback that matched one or more changed files and is ready to fire.
#[derive(Debug, Clone)]
pub struct MatchedCallback {
    /// The callback definition
    pub definition: CallbackDefinition,
    /// Files that matched this callback's pattern (relative paths)
    pub matched_files: Vec<String>,
}

/// Collect changed file paths from a batch of tool uses.
/// Extracts `file_path` from Edit and Write tool inputs.
/// Skips files from tools that have `block_callbacks: true`.
pub fn collect_changed_files(tools: &[cp_base::tools::ToolUse]) -> Vec<String> {
    let mut hull: Vec<String> = Vec::new();
    let project_root = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    for tool in tools {
        match tool.name.as_str() {
            "Edit" | "Write" => {
                // Skip files from tools that explicitly block callbacks
                if tool.input.get("block_callbacks").and_then(|v| v.as_bool()).unwrap_or(false) {
                    continue;
                }
                if let Some(path) = tool.input.get("file_path").and_then(|v| v.as_str()) {
                    // Normalize: strip leading ./ if present, strip absolute project root prefix
                    let mut anchor_path = path.strip_prefix("./").unwrap_or(path);
                    if let Some(relative) = anchor_path.strip_prefix(&project_root) {
                        anchor_path = relative.strip_prefix('/').unwrap_or(relative);
                    }
                    let anchor_str = anchor_path.to_string();
                    if !hull.contains(&anchor_str) {
                        hull.push(anchor_str);
                    }
                }
            }
            _ => {}
        }
    }
    hull
}

/// Match changed files against active callback patterns.
/// Returns a list of callbacks that matched, each with their matched files.
pub fn match_callbacks(state: &State, changed_files: &[String]) -> Vec<MatchedCallback> {
    if changed_files.is_empty() {
        return Vec::new();
    }

    let cs = CallbackState::get(state);
    let mut treasure_map: Vec<MatchedCallback> = Vec::new();

    for def in &cs.definitions {
        // Only fire active callbacks
        if !cs.active_set.contains(&def.id) {
            continue;
        }

        // Compile the glob pattern
        let compass = match Glob::new(&def.pattern) {
            Ok(g) => g.compile_matcher(),
            Err(_) => continue, // Skip invalid patterns (shouldn't happen, validated on create)
        };

        // Match each changed file against the pattern
        let mut crew: Vec<String> = Vec::new();
        for file_path in changed_files {
            let path = Path::new(file_path);
            // Try matching against the full path and just the filename
            if compass.is_match(path) || compass.is_match(path.file_name().unwrap_or_default()) {
                crew.push(file_path.clone());
            }
        }

        if !crew.is_empty() {
            treasure_map.push(MatchedCallback {
                definition: def.clone(),
                matched_files: crew,
            });
        }
    }

    treasure_map
}

/// Separate matched callbacks into blocking and non-blocking groups.
pub fn partition_callbacks(matched: Vec<MatchedCallback>) -> (Vec<MatchedCallback>, Vec<MatchedCallback>) {
    let mut blocking_fleet = Vec::new();
    let mut async_fleet = Vec::new();

    for cb in matched {
        if cb.definition.blocking {
            blocking_fleet.push(cb);
        } else {
            async_fleet.push(cb);
        }
    }

    (blocking_fleet, async_fleet)
}

/// Build the $CP_CHANGED_FILES environment variable value (newline-separated).
pub fn build_changed_files_env(files: &[String]) -> String {
    files.join("\n")
}

/// Fire a single callback by spawning its script via the console server.
/// Creates a console session + panel + watcher.
///
/// Returns `Ok(panel_id)` on success, `Err(message)` on failure.
pub fn fire_callback(
    state: &mut State,
    matched: &MatchedCallback,
    blocking_tool_use_id: Option<&str>,
) -> Result<String, String> {
    let def = &matched.definition;

    // one_at_a_time: skip if this callback already has a running watcher
    if def.one_at_a_time {
        let tag = format!("callback_{}", def.id);
        let registry = WatcherRegistry::get(state);
        if registry.has_watcher_with_tag(&tag) {
            return Err(format!(
                "Callback '{}' skipped (one_at_a_time: already running)",
                def.name,
            ));
        }
    }

    // Build the command with env vars baked in
    let changed_files_env = build_changed_files_env(&matched.matched_files);
    let project_root = std::env::current_dir()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    // Use the callback's cwd if set, otherwise project root
    let cwd = def.cwd.clone().or_else(|| Some(project_root.clone()));

    // Build the script path — uses STORE_DIR for scripts dir
    let scripts_dir = std::path::PathBuf::from(STORE_DIR).join("scripts");
    let script_path = scripts_dir.join(format!("{}.sh", def.name));
    let script_path_str = if script_path.is_absolute() {
        script_path.to_string_lossy().to_string()
    } else {
        // Make absolute relative to project root
        format!("{}/{}", project_root, script_path.to_string_lossy())
    };

    // Check script exists and is readable before spawning
    if !script_path.exists() {
        return Err(format!(
            "Callback '{}' script not found: {}",
            def.name,
            script_path.display(),
        ));
    }

    // Construct command with env vars
    let command = format!(
        "CP_CHANGED_FILES={changed_files} CP_PROJECT_ROOT={root} CP_CALLBACK_NAME={name} bash {script}",
        changed_files = shell_escape(&changed_files_env),
        root = shell_escape(&project_root),
        name = shell_escape(&def.name),
        script = shell_escape(&script_path_str),
    );

    // Generate session key via console state
    let session_key = {
        let cs = ConsoleState::get_mut(state);
        let key = format!("cb_{}", cs.next_session_id);
        cs.next_session_id += 1;
        key
    };

    // Spawn the process
    let handle = SessionHandle::spawn(session_key.clone(), command.clone(), cwd.clone())?;

    // Store handle in console state (NO panel created — deferred until failure/timeout)
    let cs = ConsoleState::get_mut(state);
    cs.sessions.insert(session_key.clone(), handle);

    // Register watcher
    let is_blocking = def.blocking && blocking_tool_use_id.is_some();
    let now = now_ms();
    let deadline_ms = def.timeout_secs.map(|t| now + t * 1000);

    let watcher_desc = if is_blocking {
        format!("⏳ Callback '{}' (blocking)", def.name)
    } else {
        format!("👁 Callback '{}'", def.name)
    };

    let watcher = CallbackWatcher {
        watcher_id: format!("callback_{}_{}", def.id, session_key),
        session_name: session_key.clone(),
        callback_name: def.name.clone(),
        callback_tag: format!("callback_{}", def.id),
        success_message: def.success_message.clone(),
        blocking: is_blocking,
        tool_use_id: blocking_tool_use_id.map(|s| s.to_string()),
        registered_at_ms: now,
        deadline_ms,
        desc: watcher_desc,
        matched_files: matched.matched_files.clone(),
        deferred_panel: DeferredPanel {
            session_key: session_key.clone(),
            display_name: format!("CB: {}", def.name),
            command: command.clone(),
            description: format!("Callback: {}", def.name),
            cwd: def.cwd.clone(),
            callback_id: def.id.clone(),
            callback_name: def.name.clone(),
        },
    };

    let registry = WatcherRegistry::get_mut(state);
    registry.register(Box::new(watcher));

    Ok(session_key)
}

/// Fire all matched non-blocking callbacks.
/// Returns a summary of what was fired.
pub fn fire_async_callbacks(
    state: &mut State,
    callbacks: &[MatchedCallback],
) -> Vec<String> {
    let mut summaries = Vec::new();
    for cb in callbacks {
        match fire_callback(state, cb, None) {
            Ok(panel_id) => {
                summaries.push(format!(
                    "Callback '{}' fired (async) → {} [{}]",
                    cb.definition.name,
                    panel_id,
                    cb.matched_files.join(", "),
                ));
            }
            Err(e) => {
                summaries.push(format!(
                    "Callback '{}' FAILED to spawn: {}",
                    cb.definition.name, e,
                ));
            }
        }
    }
    summaries
}

/// Fire all matched blocking callbacks.
/// Each gets a sentinel tool_use_id so tool_pipeline can track them.
/// Returns the sentinel content string for the blocking pipeline.
pub fn fire_blocking_callbacks(
    state: &mut State,
    callbacks: &[MatchedCallback],
    tool_use_id: &str,
) -> Vec<String> {
    let mut summaries = Vec::new();
    for cb in callbacks {
        match fire_callback(state, cb, Some(tool_use_id)) {
            Ok(panel_id) => {
                summaries.push(format!(
                    "Callback '{}' fired (blocking) → {} [{}]",
                    cb.definition.name,
                    panel_id,
                    cb.matched_files.join(", "),
                ));
            }
            Err(e) => {
                summaries.push(format!(
                    "Callback '{}' FAILED to spawn: {}",
                    cb.definition.name, e,
                ));
            }
        }
    }
    summaries
}

/// Simple shell escaping: wrap in single quotes, escape any existing single quotes.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

// ============================================================
// CallbackWatcher — fires on process exit with enrichment + auto-close
// ============================================================

/// A watcher that monitors a callback's console session.
/// NO panel is created upfront — only on failure/timeout via `create_panel` in WatcherResult.
/// On exit 0: returns success_message + log file path, kills session.
/// On exit != 0: returns error output + deferred panel info for tool_cleanup to create.
pub struct CallbackWatcher {
    pub watcher_id: String,
    pub session_name: String,
    pub callback_name: String,
    pub callback_tag: String,  // e.g. "callback_CB1" for one_at_a_time checks
    pub success_message: Option<String>,
    pub blocking: bool,
    pub tool_use_id: Option<String>,
    pub registered_at_ms: u64,
    pub deadline_ms: Option<u64>,
    pub desc: String,
    pub matched_files: Vec<String>,
    pub deferred_panel: DeferredPanel,
}

impl Watcher for CallbackWatcher {
    fn id(&self) -> &str {
        &self.watcher_id
    }

    fn description(&self) -> &str {
        &self.desc
    }

    fn is_blocking(&self) -> bool {
        self.blocking
    }

    fn tool_use_id(&self) -> Option<&str> {
        self.tool_use_id.as_deref()
    }

    fn check(&self, state: &State) -> Option<WatcherResult> {
        let cs = ConsoleState::get(state);
        let handle = cs.sessions.get(&self.session_name)?;

        if !handle.get_status().is_terminal() {
            return None;
        }

        let exit_code = handle.get_status().exit_code().unwrap_or(-1);

        if exit_code == 0 {
            // Success — no panel needed. Include log file path for AI to inspect if needed.
            let log_path = cp_mod_console::manager::log_file_path(&self.session_name);
            let log_path_str = log_path.to_string_lossy();
            let msg = if let Some(ref sm) = self.success_message {
                format!("Callback '{}': {} (exit 0). Files: [{}]. Log: {}",
                    self.callback_name, sm, self.matched_files.join(", "), log_path_str)
            } else {
                format!("Callback '{}' passed ✓ (exit 0). Files: [{}]. Log: {}",
                    self.callback_name, self.matched_files.join(", "), log_path_str)
            };
            Some(WatcherResult {
                description: msg,
                panel_id: None, // No panel was created
                tool_use_id: self.tool_use_id.clone(),
                close_panel: false,
                create_panel: None, // Success = no panel needed
            })
        } else {
            // Failure — request deferred panel creation so AI can inspect output
            let last_lines = handle.buffer.last_n_lines(4);
            let msg = format!(
                "Callback '{}' FAILED (exit {}). Files: [{}]\nLast output:\n{}",
                self.callback_name, exit_code, self.matched_files.join(", "), last_lines,
            );
            Some(WatcherResult {
                description: msg,
                panel_id: None, // Panel will be created by tool_cleanup
                tool_use_id: self.tool_use_id.clone(),
                close_panel: false,
                create_panel: Some(DeferredPanel {
                    session_key: self.deferred_panel.session_key.clone(),
                    display_name: self.deferred_panel.display_name.clone(),
                    command: self.deferred_panel.command.clone(),
                    description: self.deferred_panel.description.clone(),
                    cwd: self.deferred_panel.cwd.clone(),
                    callback_id: self.deferred_panel.callback_id.clone(),
                    callback_name: self.deferred_panel.callback_name.clone(),
                }),
            })
        }
    }

    fn check_timeout(&self) -> Option<WatcherResult> {
        let deadline = self.deadline_ms?;
        let now = now_ms();
        if now < deadline {
            return None;
        }
        let elapsed_s = (now - self.registered_at_ms) / 1000;
        Some(WatcherResult {
            description: format!(
                "Callback '{}' TIMED OUT after {}s. Files: [{}]",
                self.callback_name, elapsed_s, self.matched_files.join(", "),
            ),
            panel_id: None,
            tool_use_id: self.tool_use_id.clone(),
            close_panel: false,
            create_panel: Some(DeferredPanel {
                session_key: self.deferred_panel.session_key.clone(),
                display_name: self.deferred_panel.display_name.clone(),
                command: self.deferred_panel.command.clone(),
                description: self.deferred_panel.description.clone(),
                cwd: self.deferred_panel.cwd.clone(),
                callback_id: self.deferred_panel.callback_id.clone(),
                callback_name: self.deferred_panel.callback_name.clone(),
            }),
        })
    }

    fn registered_ms(&self) -> u64 {
        self.registered_at_ms
    }

    fn source_tag(&self) -> &str {
        &self.callback_tag
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn make_tool(name: &str, file_path: &str) -> cp_base::tools::ToolUse {
        cp_base::tools::ToolUse {
            id: "test".to_string(),
            name: name.to_string(),
            input: json!({ "file_path": file_path }),
        }
    }

    #[test]
    fn test_collect_changed_files_deduplicates() {
        let tools = vec![
            make_tool("Edit", "src/main.rs"),
            make_tool("Write", "src/main.rs"),
            make_tool("Edit", "src/lib.rs"),
        ];
        let files = collect_changed_files(&tools);
        assert_eq!(files, vec!["src/main.rs", "src/lib.rs"]);
    }

    #[test]
    fn test_collect_strips_dot_slash() {
        let tools = vec![make_tool("Edit", "./src/main.rs")];
        let files = collect_changed_files(&tools);
        assert_eq!(files, vec!["src/main.rs"]);
    }

    #[test]
    fn test_collect_strips_absolute_project_root() {
        let cwd = std::env::current_dir().unwrap().to_string_lossy().to_string();
        let abs_path = format!("{}/src/main.rs", cwd);
        let tools = vec![make_tool("Edit", &abs_path)];
        let files = collect_changed_files(&tools);
        assert_eq!(files, vec!["src/main.rs"]);
    }

    #[test]
    fn test_collect_ignores_non_file_tools() {
        let tools = vec![
            make_tool("Edit", "src/main.rs"),
            cp_base::tools::ToolUse {
                id: "test".to_string(),
                name: "git_execute".to_string(),
                input: json!({ "command": "git status" }),
            },
        ];
        let files = collect_changed_files(&tools);
        assert_eq!(files, vec!["src/main.rs"]);
    }

    #[test]
    fn test_build_changed_files_env() {
        let files = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
        let env = build_changed_files_env(&files);
        assert_eq!(env, "src/main.rs\nsrc/lib.rs");
    }
}
