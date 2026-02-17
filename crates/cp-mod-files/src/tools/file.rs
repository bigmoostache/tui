use std::path::Path;

use cp_base::state::{ContextElement, ContextType, State};
use cp_base::tools::{ToolResult, ToolUse};

pub fn execute_open(tool: &ToolUse, state: &mut State) -> ToolResult {
    let path = match tool.input.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult::new(tool.id.clone(), "Missing 'path' parameter".to_string(), true);
        }
    };

    // Check if file is already open
    if state.context.iter().any(|c| c.get_meta_str("file_path") == Some(path)) {
        return ToolResult::new(tool.id.clone(), format!("File '{}' is already open in context", path), false);
    }

    // Check if file exists (quick metadata check, not a full read)
    let path_obj = Path::new(path);
    if !path_obj.exists() {
        return ToolResult::new(tool.id.clone(), format!("File '{}' not found", path), true);
    }

    if !path_obj.is_file() {
        return ToolResult::new(tool.id.clone(), format!("'{}' is not a file", path), true);
    }

    let file_name = path_obj.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.to_string());

    // Generate context ID (fills gaps) and UID
    let context_id = state.next_available_context_id();
    let uid = format!("UID_{}_P", state.global_next_uid);
    state.global_next_uid += 1;

    // Create context element WITHOUT reading file content
    // Background cache system will populate it
    let mut elem = ContextElement {
        id: context_id.clone(),
        uid: Some(uid),
        context_type: ContextType::new(ContextType::FILE),
        name: file_name,
        token_count: 0, // Will be updated by cache
        metadata: std::collections::HashMap::new(),
        cached_content: None, // Background will populate
        history_messages: None,
        cache_deprecated: true, // Trigger background refresh
        cache_in_flight: false,
        last_refresh_ms: cp_base::panels::now_ms(),
        content_hash: None,
        source_hash: None,
        current_page: 0,
        total_pages: 1,
        full_token_count: 0,
        panel_cache_hit: false,
        panel_total_cost: 0.0,
    };
    elem.set_meta("file_path", &path.to_string());
    state.context.push(elem);

    ToolResult::new(tool.id.clone(), format!("Opened '{}' as {}", path, context_id), false)
}
