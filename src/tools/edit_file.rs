use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::{ToolResult, ToolUse};
use crate::state::{estimate_tokens, ContextElement, ContextType, State};

/// Edit operation from LLM
#[derive(Debug, Deserialize)]
pub struct EditOperation {
    pub old_string: String,
    pub new_string: String,
}

/// Normalize a string for matching: trim trailing whitespace per line, normalize line endings
fn normalize_for_match(s: &str) -> String {
    s.replace("\r\n", "\n")
        .lines()
        .map(|l| l.trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Find the best match for `needle` in `haystack` using normalized comparison.
/// Returns the actual substring from haystack that matches (preserving original whitespace).
fn find_normalized_match<'a>(haystack: &'a str, needle: &str) -> Option<&'a str> {
    let norm_needle = normalize_for_match(needle);
    let needle_lines: Vec<&str> = norm_needle.lines().collect();

    if needle_lines.is_empty() {
        return None;
    }

    // Split haystack into lines while tracking byte positions
    let mut line_positions: Vec<(usize, usize)> = vec![]; // (start, end) for each line
    let mut pos = 0;
    for line in haystack.lines() {
        let start = pos;
        let end = pos + line.len();
        line_positions.push((start, end));
        pos = end + 1; // +1 for newline (might overshoot at EOF, that's ok)
    }

    let haystack_lines: Vec<&str> = haystack.lines().collect();
    let haystack_lines_normalized: Vec<String> = haystack_lines
        .iter()
        .map(|l| l.trim_end().to_string())
        .collect();

    // Try to find needle_lines sequence in haystack_lines_normalized
    'outer: for start_idx in 0..haystack_lines.len() {
        if start_idx + needle_lines.len() > haystack_lines.len() {
            break;
        }

        for (i, needle_line) in needle_lines.iter().enumerate() {
            if haystack_lines_normalized[start_idx + i] != *needle_line {
                continue 'outer;
            }
        }

        // Found a match! Return the original substring from haystack
        let match_start = line_positions[start_idx].0;
        let match_end_idx = start_idx + needle_lines.len() - 1;
        let match_end = line_positions[match_end_idx].1;

        return Some(&haystack[match_start..match_end]);
    }

    None
}

/// Find closest match for error reporting (returns line number and preview)
fn find_closest_match(haystack: &str, needle: &str) -> Option<(usize, String)> {
    let norm_needle = normalize_for_match(needle);
    let first_needle_line = norm_needle.lines().next()?;

    if first_needle_line.trim().is_empty() {
        return None;
    }

    let haystack_lines: Vec<&str> = haystack.lines().collect();

    // Find lines that partially match the first line of needle
    let mut best_match: Option<(usize, usize, String)> = None; // (line_num, score, preview)

    for (idx, line) in haystack_lines.iter().enumerate() {
        let norm_line = line.trim_end();

        // Simple similarity: count matching characters
        let score = first_needle_line
            .chars()
            .zip(norm_line.chars())
            .filter(|(a, b)| a == b)
            .count();

        // Also check if it contains the trimmed needle line
        let contains_score = if norm_line.contains(first_needle_line.trim()) {
            first_needle_line.len()
        } else {
            0
        };

        let total_score = score.max(contains_score);

        if total_score > 0 {
            if best_match.is_none() || total_score > best_match.as_ref().unwrap().1 {
                let preview = if norm_line.len() > 60 {
                    format!("{}...", &norm_line[..60])
                } else {
                    norm_line.to_string()
                };
                best_match = Some((idx + 1, total_score, preview));
            }
        }
    }

    best_match.map(|(line, _, preview)| (line, preview))
}

pub fn execute_edit(tool: &ToolUse, state: &mut State) -> ToolResult {
    let path_str = match tool.input.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Missing required parameter: path".to_string(),
                is_error: true,
            }
        }
    };

    // Check if file is open in context
    let is_open = state.context.iter().any(|c| {
        c.context_type == ContextType::File && c.file_path.as_deref() == Some(path_str)
    });

    if !is_open {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("File '{}' is not open in context. Use open_file first.", path_str),
            is_error: true,
        };
    }

    let edits: Vec<EditOperation> = match tool.input.get("edits") {
        Some(edits_value) => match serde_json::from_value(edits_value.clone()) {
            Ok(e) => e,
            Err(e) => {
                return ToolResult {
                    tool_use_id: tool.id.clone(),
                    content: format!("Invalid edits format: {}", e),
                    is_error: true,
                }
            }
        },
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Missing required parameter: edits".to_string(),
                is_error: true,
            }
        }
    };

    let path = Path::new(path_str);

    // Read file
    let mut content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: format!("Failed to read file: {}", e),
                is_error: true,
            }
        }
    };

    let mut applied = 0;
    let mut errors: Vec<String> = vec![];

    for (idx, edit) in edits.iter().enumerate() {
        let edit_num = idx + 1;

        // Try normalized matching (handles trailing whitespace differences)
        if let Some(actual_match) = find_normalized_match(&content, &edit.old_string) {
            content = content.replacen(actual_match, &edit.new_string, 1);
            applied += 1;
        } else {
            // Provide helpful error with closest match
            let hint = if let Some((line, preview)) = find_closest_match(&content, &edit.old_string) {
                format!(" (closest match at line {}: \"{}\")", line, preview)
            } else {
                String::new()
            };

            let needle_preview = if edit.old_string.len() > 50 {
                format!("{}...", &edit.old_string[..50])
            } else {
                edit.old_string.clone()
            };

            errors.push(format!("Edit {}: no match for \"{}\"{}", edit_num, needle_preview, hint));
        }
    }

    // Write file if any edits applied
    if applied > 0 {
        if let Err(e) = fs::write(path, &content) {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: format!("Failed to write file: {}", e),
                is_error: true,
            };
        }

        // Update the context element's token count
        if let Some(ctx) = state.context.iter_mut().find(|c| {
            c.context_type == ContextType::File && c.file_path.as_deref() == Some(path_str)
        }) {
            ctx.token_count = estimate_tokens(&content);
        }
    }

    // Count approximate lines changed
    let lines_changed: usize = edits.iter().take(applied).map(|e| {
        e.new_string.lines().count().max(e.old_string.lines().count())
    }).sum();

    let result_msg = if errors.is_empty() {
        format!("Edited '{}': {}/{} edits applied (~{} lines changed)",
                path_str, applied, edits.len(), lines_changed)
    } else {
        format!("Edited '{}': {}/{} applied; {}",
                path_str, applied, edits.len(), errors.join("; "))
    };

    ToolResult {
        tool_use_id: tool.id.clone(),
        content: result_msg,
        is_error: applied == 0 && !edits.is_empty(),
    }
}

pub fn execute_create(tool: &ToolUse, state: &mut State) -> ToolResult {
    let path = match tool.input.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Missing 'path' parameter".to_string(),
                is_error: true,
            }
        }
    };

    // Accept both "contents" and "content" (LLMs sometimes use either)
    let contents = match tool.input.get("contents").or_else(|| tool.input.get("content")).and_then(|v| v.as_str()) {
        Some(c) => c,
        None => {
            return ToolResult {
                tool_use_id: tool.id.clone(),
                content: "Missing 'content' parameter".to_string(),
                is_error: true,
            }
        }
    };

    // Check if file already exists
    if Path::new(path).exists() {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("File '{}' already exists. Use edit_file to modify it.", path),
            is_error: true,
        };
    }

    // Create parent directories if needed
    if let Some(parent) = Path::new(path).parent() {
        if !parent.as_os_str().is_empty() {
            if let Err(e) = fs::create_dir_all(parent) {
                return ToolResult {
                    tool_use_id: tool.id.clone(),
                    content: format!("Failed to create directory '{}': {}", parent.display(), e),
                    is_error: true,
                };
            }
        }
    }

    // Write the file
    if let Err(e) = fs::write(path, contents) {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: format!("Failed to create file '{}': {}", path, e),
            is_error: true,
        };
    }

    // Generate context ID (fills gaps)
    let context_id = state.next_available_context_id();

    let file_name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string());

    let token_count = estimate_tokens(contents);

    state.context.push(ContextElement {
        id: context_id.clone(),
        context_type: ContextType::File,
        name: file_name,
        token_count,
        file_path: Some(path.to_string()),
        file_hash: None,
        glob_pattern: None,
        glob_path: None,
        grep_pattern: None,
        grep_path: None,
        grep_file_pattern: None,
        tmux_pane_id: None,
        tmux_lines: None,
        tmux_last_keys: None,
        tmux_description: None,
        cached_content: Some(contents.to_string()),
        cache_deprecated: true,
        last_refresh_ms: 0,
        tmux_last_lines_hash: None,
    });

    ToolResult {
        tool_use_id: tool.id.clone(),
        content: format!("Created '{}' as {} ({} tokens)", path, context_id, token_count),
        is_error: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_for_match() {
        assert_eq!(normalize_for_match("foo  \nbar\t\n"), "foo\nbar");
        assert_eq!(normalize_for_match("foo\r\nbar"), "foo\nbar");
    }

    #[test]
    fn test_find_normalized_match_exact() {
        let haystack = "line1\nline2\nline3\n";
        let needle = "line2";
        assert_eq!(find_normalized_match(haystack, needle), Some("line2"));
    }

    #[test]
    fn test_find_normalized_match_trailing_whitespace() {
        let haystack = "line1  \nline2\t\nline3\n";
        let needle = "line1\nline2";
        assert_eq!(find_normalized_match(haystack, needle), Some("line1  \nline2\t"));
    }

    #[test]
    fn test_find_normalized_match_multiline() {
        let haystack = "fn foo() {\n    let x = 1;\n    let y = 2;\n}\n";
        let needle = "    let x = 1;\n    let y = 2;";
        let matched = find_normalized_match(haystack, needle);
        assert!(matched.is_some());
        assert!(matched.unwrap().contains("let x = 1"));
        assert!(matched.unwrap().contains("let y = 2"));
    }
}
