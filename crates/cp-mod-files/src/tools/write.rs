use std::fs;
use std::path::Path;

use cp_base::state::{ContextElement, ContextType, State, estimate_tokens};
use cp_base::tools::{ToolResult, ToolUse};

pub fn execute(tool: &ToolUse, state: &mut State) -> ToolResult {
    let path_str = match tool.input.get("file_path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult::new(tool.id.clone(), "Missing required parameter: file_path".to_string(), true);
        }
    };

    let contents = match tool.input.get("contents").or_else(|| tool.input.get("content")).and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return ToolResult::new(tool.id.clone(), "Missing required parameter: contents".to_string(), true);
        }
    };

    let path = Path::new(path_str);
    let is_new = !path.exists();

    // Create parent directories if needed
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
        && !parent.exists()
        && let Err(e) = fs::create_dir_all(parent)
    {
        return ToolResult::new(tool.id.clone(), format!("Failed to create directory '{}': {}", parent.display(), e), true);
    }

    // Write the file
    if let Err(e) = fs::write(path, contents) {
        return ToolResult::new(tool.id.clone(), format!("Failed to write file '{}': {}", path_str, e), true);
    }

    // Invoke file edit callback if registered
    if let Some(callback) = state.file_edit_callback {
        callback(path_str, is_new, state);
    }

    let token_count = estimate_tokens(contents);
    let line_count = contents.lines().count();

    // Check if file is already open in context
    let already_open = state
        .context
        .iter_mut()
        .find(|c| c.context_type == ContextType::FILE && c.get_meta_str("file_path") == Some(path_str));

    if let Some(ctx) = already_open {
        // Update existing context element
        ctx.token_count = token_count;
        ctx.cache_deprecated = true;
    } else {
        // Add new context element
        let context_id = state.next_available_context_id();
        let uid = format!("UID_{}_P", state.global_next_uid);
        state.global_next_uid += 1;

        let file_name =
            path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path_str.to_string());

        let mut elem = ContextElement {
            id: context_id,
            uid: Some(uid),
            context_type: ContextType::new(ContextType::FILE),
            name: file_name,
            token_count,
            metadata: std::collections::HashMap::new(),
            cached_content: Some(contents.to_string()),
            history_messages: None,
            cache_deprecated: true,
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
        elem.set_meta("file_path", &path_str.to_string());
        state.context.push(elem);

        // Invalidate tree cache
        cp_base::panels::mark_panels_dirty(state, ContextType::new(ContextType::TREE));
    }

    let action = if is_new { "Created" } else { "Wrote" };
    let mut result_msg = format!("{} '{}' ({} lines, {} tokens)\n", action, path_str, line_count, token_count);

    // Add diff-style preview of written content (truncated for large files)
    result_msg.push_str("```diff\n");
    for (i, line) in contents.lines().enumerate() {
        if i >= 20 {
            result_msg.push_str(&format!("+ ... ({} more lines)\n", line_count - 20));
            break;
        }
        result_msg.push_str(&format!("+ {}\n", line));
    }
    result_msg.push_str("```");

    ToolResult::new(tool.id.clone(), result_msg, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_write_new_file_with_callback() {
        use std::fs;
        
        // Create a temp directory
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("new_file.txt");
        
        // Create a minimal state without callback (callback would be set by main.rs)
        let mut state = State::default();
        
        // Create a tool use for a new file
        let mut input = serde_json::Map::new();
        input.insert("file_path".to_string(), serde_json::Value::String(file_path.to_string_lossy().to_string()));
        input.insert("contents".to_string(), serde_json::Value::String("new file content\n".to_string()));
        
        let tool = ToolUse {
            id: "T1".to_string(),
            name: "Write".to_string(),
            input: serde_json::Value::Object(input),
        };
        
        // Execute the write - this should succeed and create a new file
        let result = execute(&tool, &mut state);
        
        // Verify the write succeeded
        assert!(!result.is_error);
        assert!(result.content.contains("Created"));
        assert!(result.content.contains("new file content"));
        
        // Verify the file was actually created
        assert!(file_path.exists());
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "new file content\n");
    }

    #[test]
    fn test_write_existing_file_with_callback() {
        use std::fs;
        
        // Create a temp directory and file
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("existing.txt");
        fs::write(&file_path, "old content\n").unwrap();
        
        // Create a minimal state without callback
        let mut state = State::default();
        
        // Add the file to context
        let mut ctx = cp_base::state::ContextElement {
            id: "C1".to_string(),
            uid: Some("UID_1_P".to_string()),
            context_type: cp_base::state::ContextType::new(cp_base::state::ContextType::FILE),
            name: "existing.txt".to_string(),
            token_count: 10,
            metadata: std::collections::HashMap::new(),
            cached_content: None,
            history_messages: None,
            cache_deprecated: false,
            cache_in_flight: false,
            last_refresh_ms: 0,
            content_hash: None,
            source_hash: None,
            current_page: 0,
            total_pages: 1,
            full_token_count: 0,
            panel_cache_hit: false,
            panel_total_cost: 0.0,
        };
        ctx.set_meta("file_path", &file_path.to_string_lossy().to_string());
        state.context.push(ctx);
        
        // Create a tool use to overwrite existing file
        let mut input = serde_json::Map::new();
        input.insert("file_path".to_string(), serde_json::Value::String(file_path.to_string_lossy().to_string()));
        input.insert("contents".to_string(), serde_json::Value::String("new content\n".to_string()));
        
        let tool = ToolUse {
            id: "T1".to_string(),
            name: "Write".to_string(),
            input: serde_json::Value::Object(input),
        };
        
        // Execute the write
        let result = execute(&tool, &mut state);
        
        // Verify the write succeeded
        assert!(!result.is_error);
        assert!(result.content.contains("Wrote"));
        assert!(result.content.contains("new content"));
        
        // Verify the file was overwritten
        let content = fs::read_to_string(&file_path).unwrap();
        assert_eq!(content, "new content\n");
    }
}
