use std::fs;
use std::path::Path;

use cp_base::state::{ContextType, State, estimate_tokens};
use cp_base::tools::{ToolResult, ToolUse};

/// Normalize a string for matching: trim trailing whitespace per line, normalize line endings
fn normalize_for_match(s: &str) -> String {
    s.replace("\r\n", "\n").lines().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n")
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
    let haystack_lines_normalized: Vec<String> = haystack_lines.iter().map(|l| l.trim_end().to_string()).collect();

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
        let score = first_needle_line.chars().zip(norm_line.chars()).filter(|(a, b)| a == b).count();

        // Also check if it contains the trimmed needle line
        let contains_score = if norm_line.contains(first_needle_line.trim()) { first_needle_line.len() } else { 0 };

        let total_score = score.max(contains_score);

        if total_score > 0 && (best_match.is_none() || total_score > best_match.as_ref().unwrap().1) {
            let preview = if norm_line.len() > 60 {
                format!("{}...", &norm_line[..norm_line.floor_char_boundary(60)])
            } else {
                norm_line.to_string()
            };
            best_match = Some((idx + 1, total_score, preview));
        }
    }

    best_match.map(|(line, _, preview)| (line, preview))
}

/// Generate a unified diff showing changes between old and new strings
fn generate_unified_diff(old: &str, new: &str) -> String {
    let old_lines: Vec<&str> = old.lines().collect();
    let new_lines: Vec<&str> = new.lines().collect();

    // Use a simple diff algorithm based on LCS (Longest Common Subsequence)
    let diff_ops = compute_diff(&old_lines, &new_lines);

    let mut result = String::new();
    for op in diff_ops {
        match op {
            DiffOp::Equal(line) => {
                result.push_str(&format!("  {}\n", line));
            }
            DiffOp::Delete(line) => {
                result.push_str(&format!("- {}\n", line));
            }
            DiffOp::Insert(line) => {
                result.push_str(&format!("+ {}\n", line));
            }
        }
    }

    result
}

#[derive(Debug, Clone, PartialEq)]
enum DiffOp<'a> {
    Equal(&'a str),
    Delete(&'a str),
    Insert(&'a str),
}

/// Compute diff operations using a simple LCS-based algorithm
fn compute_diff<'a>(old_lines: &[&'a str], new_lines: &[&'a str]) -> Vec<DiffOp<'a>> {
    let lcs = lcs(old_lines, new_lines);
    let mut result = Vec::new();
    let mut old_idx = 0;
    let mut new_idx = 0;
    let mut lcs_idx = 0;

    while old_idx < old_lines.len() || new_idx < new_lines.len() {
        if lcs_idx < lcs.len() {
            let (lcs_old, lcs_new) = lcs[lcs_idx];

            // Add deletions before this common line
            while old_idx < lcs_old {
                result.push(DiffOp::Delete(old_lines[old_idx]));
                old_idx += 1;
            }

            // Add insertions before this common line
            while new_idx < lcs_new {
                result.push(DiffOp::Insert(new_lines[new_idx]));
                new_idx += 1;
            }

            // Add the common line
            result.push(DiffOp::Equal(old_lines[old_idx]));
            old_idx += 1;
            new_idx += 1;
            lcs_idx += 1;
        } else {
            // No more common lines, add remaining deletions and insertions
            while old_idx < old_lines.len() {
                result.push(DiffOp::Delete(old_lines[old_idx]));
                old_idx += 1;
            }
            while new_idx < new_lines.len() {
                result.push(DiffOp::Insert(new_lines[new_idx]));
                new_idx += 1;
            }
        }
    }

    result
}

/// Find the Longest Common Subsequence (LCS) between two sequences
/// Returns pairs of (old_index, new_index) for matching lines in ascending order.
///
/// Note: This implementation uses O(m*n) space complexity. For typical file edits
/// involving small to medium-sized changes, this is acceptable. For very large files,
/// consider using a more space-efficient algorithm or an external diff library.
fn lcs<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<(usize, usize)> {
    let m = old.len();
    let n = new.len();

    // DP table: lengths[i][j] = length of LCS of old[0..i] and new[0..j]
    let mut lengths = vec![vec![0; n + 1]; m + 1];

    for i in 1..=m {
        for j in 1..=n {
            if old[i - 1] == new[j - 1] {
                lengths[i][j] = lengths[i - 1][j - 1] + 1;
            } else {
                lengths[i][j] = lengths[i - 1][j].max(lengths[i][j - 1]);
            }
        }
    }

    // Backtrack to find the actual LCS
    let mut result = Vec::new();
    let mut i = m;
    let mut j = n;

    while i > 0 && j > 0 {
        if old[i - 1] == new[j - 1] {
            result.push((i - 1, j - 1));
            i -= 1;
            j -= 1;
        } else if lengths[i - 1][j] > lengths[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }

    result.reverse();
    result
}

pub fn execute_edit(tool: &ToolUse, state: &mut State) -> ToolResult {
    // Get file_path (required)
    let path_str = match tool.input.get("file_path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => {
            return ToolResult::new(tool.id.clone(), "Missing required parameter: file_path".to_string(), true);
        }
    };

    // Get old_string (required)
    let old_string = match tool.input.get("old_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return ToolResult::new(tool.id.clone(), "Missing required parameter: old_string".to_string(), true);
        }
    };

    // Get new_string (required)
    let new_string = match tool.input.get("new_string").and_then(|v| v.as_str()) {
        Some(s) => s,
        None => {
            return ToolResult::new(tool.id.clone(), "Missing required parameter: new_string".to_string(), true);
        }
    };

    // Get replace_all (optional, default false)
    let replace_all = tool.input.get("replace_all").and_then(|v| v.as_bool()).unwrap_or(false);

    // Check if file is open in context
    let is_open = state
        .context
        .iter()
        .any(|c| c.context_type == ContextType::FILE && c.get_meta_str("file_path") == Some(path_str));

    if !is_open {
        return ToolResult::new(
            tool.id.clone(),
            format!("File '{}' is not open in context. Use file_open first.", path_str),
            true,
        );
    }

    let path = Path::new(path_str);

    // Read file
    let mut content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return ToolResult::new(tool.id.clone(), format!("Failed to read file: {}", e), true);
        }
    };

    // Try normalized matching (handles trailing whitespace differences)
    let replaced = if let Some(actual_match) = find_normalized_match(&content, old_string) {
        if replace_all {
            let count = content.matches(actual_match).count();
            content = content.replace(actual_match, new_string);
            count
        } else {
            content = content.replacen(actual_match, new_string, 1);
            1
        }
    } else {
        0
    };

    if replaced == 0 {
        // Provide helpful error with closest match
        let hint = if let Some((line, preview)) = find_closest_match(&content, old_string) {
            format!(" (closest match at line {}: \"{}\")", line, preview)
        } else {
            String::new()
        };

        let needle_preview = if old_string.len() > 50 {
            format!("{}...", &old_string[..old_string.floor_char_boundary(50)])
        } else {
            old_string.to_string()
        };

        return ToolResult::new(tool.id.clone(), format!("No match found for \"{}\"{}", needle_preview, hint), true);
    }

    // Write file
    if let Err(e) = fs::write(path, &content) {
        return ToolResult::new(tool.id.clone(), format!("Failed to write file: {}", e), true);
    }

    // Update the context element's token count
    if let Some(ctx) = state
        .context
        .iter_mut()
        .find(|c| c.context_type == ContextType::FILE && c.get_meta_str("file_path") == Some(path_str))
    {
        ctx.token_count = estimate_tokens(&content);
    }

    // Count approximate lines changed
    let lines_changed = new_string.lines().count().max(old_string.lines().count());

    // Format result as a unified diff for UI display
    let mut result_msg = String::new();

    // Header line
    if replace_all && replaced > 1 {
        result_msg.push_str(&format!(
            "Edited '{}': {} replacements (~{} lines changed each)\n",
            path_str, replaced, lines_changed
        ));
    } else {
        result_msg.push_str(&format!("Edited '{}': ~{} lines changed\n", path_str, lines_changed));
    }

    // Add diff markers for UI rendering
    result_msg.push_str("```diff\n");

    // Generate unified diff by comparing old and new line by line
    let diff_lines = generate_unified_diff(old_string, new_string);
    result_msg.push_str(&diff_lines);

    result_msg.push_str("```");

    ToolResult::new(tool.id.clone(), result_msg, false)
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

    #[test]
    fn test_diff_format_structure() {
        // Test that verifies the expected structure of a diff-formatted result message
        // This simulates what edit_file produces without requiring actual file I/O

        let old_string = "line1\nline2";
        let new_string = "line1_modified\nline2_modified";
        let path_str = "test.txt";
        let replaced = 1;
        let replace_all = false;
        let lines_changed = new_string.lines().count().max(old_string.lines().count());

        // Replicate the formatting logic from edit_file
        let mut result_msg = String::new();

        if replace_all && replaced > 1 {
            result_msg.push_str(&format!(
                "Edited '{}': {} replacements (~{} lines changed each)\n",
                path_str, replaced, lines_changed
            ));
        } else {
            result_msg.push_str(&format!("Edited '{}': ~{} lines changed\n", path_str, lines_changed));
        }

        result_msg.push_str("```diff\n");

        // Generate unified diff
        let diff_lines = generate_unified_diff(old_string, new_string);
        result_msg.push_str(&diff_lines);

        result_msg.push_str("```");

        // Verify the structure
        assert!(result_msg.contains("Edited 'test.txt': ~2 lines changed\n"));
        assert!(result_msg.contains("```diff\n"));
        assert!(result_msg.contains("- line1\n"));
        assert!(result_msg.contains("- line2\n"));
        assert!(result_msg.contains("+ line1_modified\n"));
        assert!(result_msg.contains("+ line2_modified\n"));
        assert!(result_msg.ends_with("```"));

        // Verify ordering: header, then diff marker, then content (unified diff), then closing
        let header_pos = result_msg.find("Edited").unwrap();
        let diff_start_pos = result_msg.find("```diff").unwrap();
        let diff_end_pos = result_msg.rfind("```").unwrap();

        assert!(header_pos < diff_start_pos);
        assert!(diff_start_pos < diff_end_pos);
    }

    #[test]
    fn test_unified_diff_with_common_lines() {
        // Test diff with some unchanged lines
        let old = "line1\nline2\nline3";
        let new = "line1\nline2_modified\nline3";

        let diff = generate_unified_diff(old, new);

        // Should show common lines with "  " prefix
        assert!(diff.contains("  line1\n"));
        assert!(diff.contains("- line2\n"));
        assert!(diff.contains("+ line2_modified\n"));
        assert!(diff.contains("  line3\n"));

        // Verify interleaving: deletion should come right before addition
        let del_pos = diff.find("- line2\n").unwrap();
        let add_pos = diff.find("+ line2_modified\n").unwrap();
        assert!(del_pos < add_pos);

        // No other content should be between the deletion and addition (except newline)
        let between = &diff[del_pos + "- line2\n".len()..add_pos];
        assert!(between.is_empty(), "Expected interleaved diff, but found: '{}'", between);
    }

    #[test]
    fn test_unified_diff_interleaved_changes() {
        // Test with alternating changes and common lines
        let old = "keep1\nchange1\nkeep2\nchange2\nkeep3";
        let new = "keep1\nnew1\nkeep2\nnew2\nkeep3";

        let diff = generate_unified_diff(old, new);

        // Verify structure shows changes near their context
        assert!(diff.contains("  keep1\n"));
        assert!(diff.contains("- change1\n"));
        assert!(diff.contains("+ new1\n"));
        assert!(diff.contains("  keep2\n"));
        assert!(diff.contains("- change2\n"));
        assert!(diff.contains("+ new2\n"));
        assert!(diff.contains("  keep3\n"));

        // Verify first change is interleaved
        let change1_del_pos = diff.find("- change1\n").unwrap();
        let change1_add_pos = diff.find("+ new1\n").unwrap();
        let keep2_pos = diff.find("  keep2\n").unwrap();

        assert!(change1_del_pos < change1_add_pos);
        assert!(change1_add_pos < keep2_pos, "Changes should be interleaved near their context");
    }

    #[test]
    fn test_unified_diff_pure_addition() {
        // Test diff with only additions
        let old = "line1";
        let new = "line1\nline2\nline3";

        let diff = generate_unified_diff(old, new);

        assert!(diff.contains("  line1\n"));
        assert!(diff.contains("+ line2\n"));
        assert!(diff.contains("+ line3\n"));
    }

    #[test]
    fn test_unified_diff_pure_deletion() {
        // Test diff with only deletions
        let old = "line1\nline2\nline3";
        let new = "line1";

        let diff = generate_unified_diff(old, new);

        assert!(diff.contains("  line1\n"));
        assert!(diff.contains("- line2\n"));
        assert!(diff.contains("- line3\n"));
    }
}
