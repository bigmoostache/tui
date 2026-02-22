//! Utilities for `.context-pilot/shared/` — the version-controlled part of .context-pilot.
//!
//! While `.context-pilot/` is gitignored (worker state, console logs, config),
//! `.context-pilot/shared/` is explicitly un-gitignored so its contents are
//! committed to the repo and shared with the whole team.

use std::fs;
use std::path::Path;

/// The shared directory path within .context-pilot.
pub const SHARED_DIR: &str = ".context-pilot/shared";

/// Ensure the `.context-pilot/shared/` directory exists and is un-gitignored.
///
/// If a `.gitignore` file exists at the project root and contains a rule that
/// ignores `.context-pilot` (e.g. `/.context-pilot` or `.context-pilot`), this
/// function appends `!.context-pilot/shared/` as an exception — unless the
/// exception already exists.
///
/// Call this at module init time for any module that uses shared assets.
pub fn ensure_shared_dir() {
    let _ = fs::create_dir_all(SHARED_DIR);
    ensure_gitignore_exception();
}

/// Check if .gitignore ignores .context-pilot and add a shared/ exception if needed.
fn ensure_gitignore_exception() {
    ensure_gitignore_exception_at(Path::new(".gitignore"));
}

/// Check if a gitignore file ignores .context-pilot and add a shared/ exception if needed.
/// Extracted for testability (tests pass a temp-dir path instead of ".gitignore").
fn ensure_gitignore_exception_at(gitignore_path: &Path) {
    if !gitignore_path.exists() {
        return;
    }

    let content = match fs::read_to_string(gitignore_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    // Check if .context-pilot is being ignored
    let ignores_cp = content.lines().any(|line| {
        let trimmed = line.trim();
        trimmed == ".context-pilot"
            || trimmed == "/.context-pilot"
            || trimmed == ".context-pilot/"
            || trimmed == "/.context-pilot/"
    });

    if !ignores_cp {
        return;
    }

    // Check if the shared exception already exists
    let has_exception = content.lines().any(|line| {
        let trimmed = line.trim();
        trimmed == "!.context-pilot/shared/"
            || trimmed == "!.context-pilot/shared"
            || trimmed == "!/.context-pilot/shared/"
            || trimmed == "!/.context-pilot/shared"
    });

    if has_exception {
        return;
    }

    // Append the exception
    let mut new_content = content.clone();
    if !new_content.ends_with('\n') {
        new_content.push('\n');
    }
    new_content.push_str("!.context-pilot/shared/\n");

    let _ = fs::write(gitignore_path, new_content);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    /// Helper: run ensure logic in an isolated temp dir.
    /// We can't use set_current_dir (not thread-safe), so we test the
    /// gitignore logic directly by calling the internal function with paths.
    fn run_in_tmpdir(gitignore_content: Option<&str>) -> (TempDir, String) {
        let tmp = TempDir::new().unwrap();
        let gitignore_path = tmp.path().join(".gitignore");
        let shared_path = tmp.path().join(".context-pilot").join("shared");

        if let Some(content) = gitignore_content {
            fs::write(&gitignore_path, content).unwrap();
        }

        // Create the shared dir
        let _ = fs::create_dir_all(&shared_path);

        // Run gitignore exception logic
        ensure_gitignore_exception_at(&gitignore_path);

        let final_content = if gitignore_path.exists() {
            fs::read_to_string(&gitignore_path).unwrap_or_default()
        } else {
            String::new()
        };

        (tmp, final_content)
    }

    #[test]
    fn test_gitignore_exception_added() {
        let (_tmp, content) = run_in_tmpdir(Some("target/\n/.context-pilot\n"));
        assert!(content.contains("!.context-pilot/shared/"), "Exception should be added");
    }

    #[test]
    fn test_gitignore_exception_not_duplicated() {
        let (_tmp, content) = run_in_tmpdir(Some("/.context-pilot\n!.context-pilot/shared/\n"));
        let count = content.matches("!.context-pilot/shared/").count();
        assert_eq!(count, 1, "Exception should not be duplicated");
    }

    #[test]
    fn test_no_gitignore_no_crash() {
        let (tmp, _content) = run_in_tmpdir(None);
        // Just verify it didn't crash and shared dir exists
        assert!(tmp.path().join(".context-pilot/shared").is_dir());
    }

    #[test]
    fn test_gitignore_without_context_pilot_rule() {
        let (_tmp, content) = run_in_tmpdir(Some("target/\n*.log\n"));
        assert!(!content.contains("!.context-pilot/shared/"), "No exception needed if .context-pilot isn't ignored");
    }
}
