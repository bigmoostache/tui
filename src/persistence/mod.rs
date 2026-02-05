/// Persistence module for multi-worker state management
///
/// This module handles the file-based persistence of:
/// - SharedConfig (config.json) - Global settings shared across workers
/// - WorkerState (states/{worker}.json) - Worker-specific state
/// - PanelData (panels/{uid}.json) - Dynamic panel metadata
/// - Messages (messages/{uid}.yaml) - Conversation messages

pub mod config;
pub mod worker;
pub mod panel;
pub mod message;

// Re-export commonly used functions
pub use message::{load_message, save_message, delete_message};
pub use config::current_pid;

use std::fs;
use std::path::PathBuf;
use std::collections::HashMap;
use chrono::Local;

use crate::config::set_active_theme;
use crate::constants::{STORE_DIR, CONFIG_FILE, DEFAULT_WORKER_ID};
use crate::state::{SharedConfig, WorkerState, PanelData, Message, State, ContextType, ContextElement};
use crate::tool_defs::{get_all_tool_definitions, ToolDefinition};
use crate::tools::MANAGE_TOOLS_ID;

/// Errors directory name
const ERRORS_DIR: &str = "errors";

/// Build tools from defaults, applying disabled_tools list
fn build_tools_from_disabled(disabled_tools: &[String]) -> Vec<ToolDefinition> {
    let mut tools = get_all_tool_definitions();
    for tool in &mut tools {
        // manage_tools can never be disabled
        if tool.id != MANAGE_TOOLS_ID && disabled_tools.contains(&tool.id) {
            tool.enabled = false;
        }
    }
    tools
}

/// Extract disabled tool IDs from tools list
fn extract_disabled_tools(tools: &[ToolDefinition]) -> Vec<String> {
    tools.iter()
        .filter(|t| !t.enabled)
        .map(|t| t.id.clone())
        .collect()
}

/// Check if new multi-file format exists
fn new_format_exists() -> bool {
    PathBuf::from(STORE_DIR).join(CONFIG_FILE).exists()
}

/// Load state using new multi-file format
pub fn load_state() -> State {
    if new_format_exists() {
        load_state_new()
    } else {
        // Fresh start - create default state
        let state = State::default();
        set_active_theme(&state.active_theme);
        state
    }
}

/// Load state from the new multi-file format
/// Convert PanelData to ContextElement
fn panel_to_context(panel: &PanelData, local_id: &str) -> ContextElement {
    ContextElement {
        id: local_id.to_string(),
        uid: Some(panel.uid.clone()),
        context_type: panel.panel_type,
        name: panel.name.clone(),
        token_count: panel.token_count,
        file_path: panel.file_path.clone(),
        file_hash: None,
        glob_pattern: panel.glob_pattern.clone(),
        glob_path: panel.glob_path.clone(),
        grep_pattern: panel.grep_pattern.clone(),
        grep_path: panel.grep_path.clone(),
        grep_file_pattern: panel.grep_file_pattern.clone(),
        tmux_pane_id: panel.tmux_pane_id.clone(),
        tmux_lines: panel.tmux_lines,
        tmux_last_keys: None,
        tmux_description: panel.tmux_description.clone(),
        cached_content: None,
        cache_deprecated: true,  // Will be refreshed on load
        last_refresh_ms: crate::panels::now_ms(),
        tmux_last_lines_hash: None,
    }
}

fn load_state_new() -> State {
    // Load shared config
    let shared_config = config::load_config().unwrap_or_default();

    // Load worker state (main_worker)
    let worker_state = worker::load_worker(DEFAULT_WORKER_ID).unwrap_or_default();

    // Build context from panels in panels/ folder
    let mut context: Vec<ContextElement> = Vec::new();
    let important = &worker_state.important_panel_uids;

    // Load fixed panels from important_panel_uids (in order P0-P7)
    // P0 = System (not stored in panels/ - comes from systems[])
    context.push(ContextElement {
        id: "P0".to_string(),
        uid: None,
        context_type: ContextType::System,
        name: "Seed".to_string(),
        token_count: 0,
        file_path: None, file_hash: None, glob_pattern: None, glob_path: None,
        grep_pattern: None, grep_path: None, grep_file_pattern: None,
        tmux_pane_id: None, tmux_lines: None, tmux_last_keys: None, tmux_description: None,
        cached_content: None, cache_deprecated: false, last_refresh_ms: crate::panels::now_ms(), tmux_last_lines_hash: None,
    });

    // P1 = Conversation (chat)
    if let Some(panel) = panel::load_panel(&important.chat) {
        context.push(panel_to_context(&panel, "P1"));
    }
    // P2 = Tree
    if let Some(panel) = panel::load_panel(&important.tree) {
        context.push(panel_to_context(&panel, "P2"));
    }
    // P3 = Todo (wip)
    if let Some(panel) = panel::load_panel(&important.wip) {
        context.push(panel_to_context(&panel, "P3"));
    }
    // P4 = Memory
    if let Some(panel) = panel::load_panel(&important.memories) {
        context.push(panel_to_context(&panel, "P4"));
    }
    // P5 = Overview (world)
    if let Some(panel) = panel::load_panel(&important.world) {
        context.push(panel_to_context(&panel, "P5"));
    }
    // P6 = Git (changes)
    if let Some(panel) = panel::load_panel(&important.changes) {
        context.push(panel_to_context(&panel, "P6"));
    }
    // P7 = Scratchpad
    if let Some(panel) = panel::load_panel(&important.scratch) {
        context.push(panel_to_context(&panel, "P7"));
    }

    // Load dynamic panels from panel_uid_to_local_id (P8+)
    let mut dynamic_panels: Vec<(String, ContextElement)> = worker_state.panel_uid_to_local_id.iter()
        .filter_map(|(uid, local_id)| {
            panel::load_panel(uid).map(|p| (local_id.clone(), panel_to_context(&p, local_id)))
        })
        .collect();
    // Sort by local ID to maintain order
    dynamic_panels.sort_by(|a, b| {
        let a_num: usize = a.0.trim_start_matches('P').parse().unwrap_or(999);
        let b_num: usize = b.0.trim_start_matches('P').parse().unwrap_or(999);
        a_num.cmp(&b_num)
    });
    for (_, elem) in dynamic_panels {
        context.push(elem);
    }

    // Load messages from the conversation panel
    let message_uids: Vec<String> = if !important.chat.is_empty() {
        panel::load_panel(&important.chat)
            .map(|p| p.message_uids)
            .unwrap_or_default()
    } else {
        vec![]
    };

    let messages: Vec<Message> = message_uids.iter()
        .filter_map(|uid| load_message(uid))
        .collect();

    // Ensure root is always open
    let mut open_folders = worker_state.tree_open_folders.clone();
    if !open_folders.contains(&".".to_string()) {
        open_folders.insert(0, ".".to_string());
    }

    // Calculate display ID counters from loaded messages
    let next_user_id = messages.iter()
        .filter(|m| m.id.starts_with('U'))
        .filter_map(|m| m.id[1..].parse::<usize>().ok())
        .max()
        .map(|n| n + 1)
        .unwrap_or(1);
    let next_assistant_id = messages.iter()
        .filter(|m| m.id.starts_with('A'))
        .filter_map(|m| m.id[1..].parse::<usize>().ok())
        .max()
        .map(|n| n + 1)
        .unwrap_or(1);

    let state = State {
        // From loaded panels
        context,
        messages,
        selected_context: shared_config.selected_context,
        todos: worker_state.todos,
        next_user_id,
        next_assistant_id,
        next_tool_id: worker_state.next_tool_id,
        next_result_id: worker_state.next_result_id,
        next_todo_id: worker_state.next_todo_id,
        scratchpad_cells: worker_state.scratchpad_cells,
        next_scratchpad_id: worker_state.next_scratchpad_id,
        active_system_id: worker_state.active_system_id,
        git_show_diffs: worker_state.git_show_diffs,
        tree_open_folders: open_folders,
        tools: build_tools_from_disabled(&worker_state.disabled_tools),

        // From shared config
        memories: shared_config.memories,
        next_memory_id: shared_config.next_memory_id,
        systems: shared_config.systems,
        next_system_id: shared_config.next_system_id,
        tree_filter: shared_config.tree_filter,
        tree_descriptions: shared_config.tree_descriptions,
        dev_mode: shared_config.dev_mode,
        active_theme: shared_config.active_theme.clone(),
        llm_provider: shared_config.llm_provider,
        anthropic_model: shared_config.anthropic_model,
        grok_model: shared_config.grok_model,
        groq_model: shared_config.groq_model,
        cleaning_threshold: shared_config.cleaning_threshold,
        cleaning_target_proportion: shared_config.cleaning_target_proportion,
        context_budget: shared_config.context_budget,
        input: shared_config.draft_input,
        input_cursor: shared_config.draft_cursor,

        // Global UID counter for all shared elements
        global_next_uid: shared_config.global_next_uid,

        // Runtime-only state (defaults)
        is_streaming: false,
        scroll_offset: 0.0,
        user_scrolled: false,
        scroll_accel: 1.0,
        max_scroll: 0.0,
        streaming_estimated_tokens: 0,
        pending_tldrs: 0,
        dirty: true,
        spinner_frame: 0,
        perf_enabled: false,
        config_view: false,
        config_selected_bar: 0,
        api_check_in_progress: false,
        api_check_result: None,
        git_branch: None,
        git_branches: vec![],
        git_is_repo: false,
        git_file_changes: vec![],
        git_last_refresh_ms: crate::panels::now_ms(),
        git_status_hash: None,
        git_show_logs: false,
        git_log_args: None,
        git_log_content: None,
        api_retry_count: 0,
        reload_pending: false,
        waiting_for_panels: false,
        last_viewport_width: 0,
        message_cache: HashMap::new(),
        input_cache: None,
        full_content_cache: None,
    };

    // Set the global active theme
    set_active_theme(&state.active_theme);
    state
}

/// Save state using new multi-file format
pub fn save_state(state: &State) {
    let dir = PathBuf::from(STORE_DIR);
    fs::create_dir_all(&dir).ok();

    // Create SharedConfig
    let shared_config = SharedConfig {
        reload_requested: false,
        active_theme: state.active_theme.clone(),
        owner_pid: Some(current_pid()),
        dev_mode: state.dev_mode,
        llm_provider: state.llm_provider,
        anthropic_model: state.anthropic_model,
        grok_model: state.grok_model,
        groq_model: state.groq_model,
        memories: state.memories.clone(),
        next_memory_id: state.next_memory_id,
        systems: state.systems.clone(),
        next_system_id: state.next_system_id,
        draft_input: state.input.clone(),
        draft_cursor: state.input_cursor,
        tree_filter: state.tree_filter.clone(),
        tree_descriptions: state.tree_descriptions.clone(),
        cleaning_threshold: state.cleaning_threshold,
        cleaning_target_proportion: state.cleaning_target_proportion,
        context_budget: state.context_budget,
        selected_context: state.selected_context,
        global_next_uid: state.global_next_uid,
    };

    // Build important_panel_uids from context elements (fixed panels P1-P7)
    // All fixed panels get UIDs and are stored in panels/ folder
    let mut important_uids = crate::state::ImportantPanelUids::default();
    for ctx in &state.context {
        if let Some(uid) = &ctx.uid {
            match ctx.context_type {
                ContextType::Conversation => important_uids.chat = uid.clone(),
                ContextType::Tree => important_uids.tree = uid.clone(),
                ContextType::Todo => important_uids.wip = uid.clone(),
                ContextType::Memory => important_uids.memories = uid.clone(),
                ContextType::Overview => important_uids.world = uid.clone(),
                ContextType::Git => important_uids.changes = uid.clone(),
                ContextType::Scratchpad => important_uids.scratch = uid.clone(),
                _ => {}
            }
        }
    }

    // Build panel_uid_to_local_id for dynamic panels (P8+)
    // Excludes chat (already in important_panel_uids) and other fixed panels
    let panel_uid_to_local_id: HashMap<String, String> = state.context.iter()
        .filter(|c| c.uid.is_some() && !c.context_type.is_fixed())
        .filter_map(|c| c.uid.as_ref().map(|uid| (uid.clone(), c.id.clone())))
        .collect();

    // Create WorkerState (no context field - panels are in panels/ folder)
    let worker_state = WorkerState {
        worker_id: DEFAULT_WORKER_ID.to_string(),
        important_panel_uids: important_uids.clone(),
        panel_uid_to_local_id,
        next_tool_id: state.next_tool_id,
        next_result_id: state.next_result_id,
        next_todo_id: state.next_todo_id,
        todos: state.todos.clone(),
        disabled_tools: extract_disabled_tools(&state.tools),
        git_show_diffs: state.git_show_diffs,
        tree_open_folders: state.tree_open_folders.clone(),
        active_system_id: state.active_system_id.clone(),
        scratchpad_cells: state.scratchpad_cells.clone(),
        next_scratchpad_id: state.next_scratchpad_id,
    };

    // Save shared config
    config::save_config(&shared_config);

    // Save worker state
    worker::save_worker(&worker_state);

    // Save ALL panels to panels/ folder (except System P0 which comes from systems[])
    for ctx in state.context.iter() {
        // Skip System panel (P0) - it comes from systems[] in SharedConfig
        if ctx.context_type == ContextType::System {
            continue;
        }

        // All other panels need a UID to be saved
        if let Some(uid) = &ctx.uid {
            let panel_data = PanelData {
                uid: uid.clone(),
                panel_type: ctx.context_type,
                name: ctx.name.clone(),
                token_count: ctx.token_count,
                // Conversation panel gets message_uids
                message_uids: if ctx.context_type == ContextType::Conversation {
                    state.messages.iter()
                        .map(|m| m.uid.clone().unwrap_or_else(|| m.id.clone()))
                        .collect()
                } else {
                    vec![]
                },
                file_path: ctx.file_path.clone(),
                glob_pattern: ctx.glob_pattern.clone(),
                glob_path: ctx.glob_path.clone(),
                grep_pattern: ctx.grep_pattern.clone(),
                grep_path: ctx.grep_path.clone(),
                grep_file_pattern: ctx.grep_file_pattern.clone(),
                tmux_pane_id: ctx.tmux_pane_id.clone(),
                tmux_lines: ctx.tmux_lines,
                tmux_description: ctx.tmux_description.clone(),
            };
            panel::save_panel(&panel_data);
        }
    }
}

/// Check if we still own the state file (another instance may have taken over)
/// Returns false if another process has claimed ownership
pub fn check_ownership() -> bool {
    if let Some(cfg) = config::load_config() {
        if let Some(owner) = cfg.owner_pid {
            return owner == current_pid();
        }
    }
    // If we can't read the file or there's no owner, assume we're still the owner
    true
}

/// Log an error to .context-pilot/errors/ and return the file path
pub fn log_error(error: &str) -> String {
    let errors_dir = PathBuf::from(STORE_DIR).join(ERRORS_DIR);
    fs::create_dir_all(&errors_dir).ok();

    // Count existing error files to determine next number
    let error_count = fs::read_dir(&errors_dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().extension().map(|ext| ext == "txt").unwrap_or(false))
                .count()
        })
        .unwrap_or(0);

    let error_num = error_count + 1;
    let filename = format!("error_{}.txt", error_num);
    let filepath = errors_dir.join(&filename);

    // Create error log content with timestamp
    let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S");
    let content = format!(
        "Error Log #{}\n\
         Timestamp: {}\n\
         \n\
         Error Details:\n\
         {}\n",
        error_num, timestamp, error
    );

    fs::write(&filepath, content).ok();

    filepath.to_string_lossy().to_string()
}
