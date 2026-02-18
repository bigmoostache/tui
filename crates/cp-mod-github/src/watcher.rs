//! Background polling watcher for GithubResult panels.
//!
//! Uses HTTP ETags for `gh api` commands and output hashing for other `gh`
//! commands to efficiently detect changes. Respects the `X-Poll-Interval`
//! header from GitHub API responses to dynamically adjust per-watch polling
//! frequency. Sends `CacheUpdate::Content` through the shared
//! `cache_tx` channel when content changes.

use std::collections::HashMap;
use std::process::Command;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use secrecy::{ExposeSecret, SecretBox};
use sha2::{Digest, Sha256};

use cp_base::panels::CacheUpdate;
use cp_base::config::MAX_RESULT_CONTENT_BYTES;
use cp_base::modules::{run_with_timeout, truncate_output};
use cp_base::panels::now_ms;
use cp_base::state::estimate_tokens;

use crate::GH_CMD_TIMEOUT_SECS;

/// How often the background thread wakes to check if any watch is due (seconds)
const GH_WATCHER_TICK_SECS: u64 = 5;

/// Default polling interval when no X-Poll-Interval header is available (seconds).
/// GitHub's typical X-Poll-Interval is 60s; we use the same default.
const GH_DEFAULT_POLL_INTERVAL_SECS: u64 = 60;

/// Snapshot of a due watch for polling (context_id, args, token, is_api, etag, last_hash)
type DueWatch = (String, Vec<String>, Arc<SecretBox<String>>, bool, Option<String>, Option<String>);

/// Update sent when branch PR info changes
pub struct BranchPrUpdate {
    pub pr_info: Option<crate::types::BranchPrInfo>,
}

/// State for background polling of the current branch's PR
struct BranchPrWatch {
    branch: String,
    github_token: Arc<SecretBox<String>>,
    last_poll_ms: u64,
    last_output_hash: Option<String>,
}

/// Per-panel watch state
struct GhWatch {
    context_id: String,
    github_token: Arc<SecretBox<String>>,
    /// Pre-parsed args (excludes "gh" prefix)
    args: Vec<String>,
    /// true if args[0] == "api" && no --jq/--template flags
    is_api_command: bool,
    /// ETag from last 200 response (api commands only)
    etag: Option<String>,
    /// SHA-256 of last output (non-api commands)
    last_output_hash: Option<String>,
    /// Polling interval in seconds (from X-Poll-Interval header or default)
    poll_interval_secs: u64,
    /// Timestamp of last poll attempt (milliseconds)
    last_poll_ms: u64,
}

/// Background watcher that polls GithubResult panels for changes.
pub struct GhWatcher {
    watches: Arc<Mutex<HashMap<String, GhWatch>>>,
    branch_pr_watch: Arc<Mutex<Option<BranchPrWatch>>>,
    _thread: JoinHandle<()>,
}

impl GhWatcher {
    /// Create a new GhWatcher with a background polling thread.
    pub fn new(cache_tx: Sender<CacheUpdate>) -> Self {
        let watches: Arc<Mutex<HashMap<String, GhWatch>>> = Arc::new(Mutex::new(HashMap::new()));
        let branch_pr_watch: Arc<Mutex<Option<BranchPrWatch>>> = Arc::new(Mutex::new(None));
        let watches_clone = Arc::clone(&watches);
        let branch_pr_clone = Arc::clone(&branch_pr_watch);

        let thread = thread::spawn(move || {
            poll_loop(watches_clone, branch_pr_clone, cache_tx);
        });

        Self { watches, branch_pr_watch, _thread: thread }
    }

    /// Reconcile the watch list with current GithubResult panels.
    /// Args: `(context_id, command, github_token)`.
    /// Adds missing watches, removes stale ones, preserves etag/hash/interval state on existing.
    pub fn sync_watches(&self, panels: &[(String, String, String)]) {
        let mut watches = self.watches.lock().unwrap_or_else(|e| e.into_inner());

        // Remove watches for panels that no longer exist
        let active_ids: std::collections::HashSet<&str> = panels.iter().map(|(id, _, _)| id.as_str()).collect();
        watches.retain(|id, _| active_ids.contains(id.as_str()));

        // Add watches for new panels
        for (context_id, command, github_token) in panels {
            if watches.contains_key(context_id) {
                continue; // Already watching — preserve etag/hash/interval state
            }

            let args = match crate::classify::validate_gh_command(command) {
                Ok(a) => a,
                Err(_) => continue, // Skip invalid commands
            };

            let is_api_command = is_api_command(&args);

            let token = Arc::new(SecretBox::new(Box::new(github_token.clone())));
            watches.insert(
                context_id.clone(),
                GhWatch {
                    context_id: context_id.clone(),
                    github_token: token,
                    args,
                    is_api_command,
                    etag: None,
                    last_output_hash: None,
                    poll_interval_secs: GH_DEFAULT_POLL_INTERVAL_SECS,
                    last_poll_ms: 0, // Poll immediately on first sync
                },
            );
        }
    }

    /// Update the branch PR watch with the current branch and token.
    /// Call this whenever the git branch or github token changes.
    pub fn sync_branch_pr(&self, branch: Option<&str>, github_token: Option<&str>) {
        let mut watch = self.branch_pr_watch.lock().unwrap_or_else(|e| e.into_inner());
        match (branch, github_token) {
            (Some(branch), Some(token)) => {
                if let Some(ref mut w) = *watch {
                    if w.branch != branch {
                        // Branch changed — reset polling state
                        w.branch = branch.to_string();
                        w.last_poll_ms = 0;
                        w.last_output_hash = None;
                        w.github_token = Arc::new(SecretBox::new(Box::new(token.to_string())));
                    }
                } else {
                    *watch = Some(BranchPrWatch {
                        branch: branch.to_string(),
                        github_token: Arc::new(SecretBox::new(Box::new(token.to_string()))),
                        last_poll_ms: 0,
                        last_output_hash: None,
                    });
                }
            }
            _ => {
                *watch = None;
            }
        }
    }
}

/// Classify whether args represent a `gh api` command eligible for ETag polling.
fn is_api_command(args: &[String]) -> bool {
    args.first().map(|s| s.as_str()) == Some("api")
        && !args.iter().any(|a| a == "--jq" || a == "-q" || a == "--template" || a == "-t")
}

/// Background polling loop.
fn poll_loop(
    watches: Arc<Mutex<HashMap<String, GhWatch>>>,
    branch_pr_watch: Arc<Mutex<Option<BranchPrWatch>>>,
    cache_tx: Sender<CacheUpdate>,
) {
    loop {
        thread::sleep(std::time::Duration::from_secs(GH_WATCHER_TICK_SECS));

        let current_ms = now_ms();

        // === Poll branch PR ===
        {
            let snapshot = {
                let watch = branch_pr_watch.lock().unwrap_or_else(|e| e.into_inner());
                watch.as_ref().and_then(|w| {
                    if current_ms.saturating_sub(w.last_poll_ms) >= GH_DEFAULT_POLL_INTERVAL_SECS * 1000 {
                        Some((w.branch.clone(), Arc::clone(&w.github_token), w.last_output_hash.clone()))
                    } else {
                        None
                    }
                })
            };

            if let Some((branch, token, last_hash)) = snapshot {
                let token_str = token.expose_secret();
                let result = poll_branch_pr(&branch, token_str, last_hash.as_deref());

                // Update last_poll_ms and hash
                {
                    let mut watch = branch_pr_watch.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(ref mut w) = *watch {
                        w.last_poll_ms = now_ms();
                        if let Some((ref new_hash, _)) = result {
                            w.last_output_hash = Some(new_hash.clone());
                        }
                    }
                }

                // Send update if content changed
                if let Some((_, pr_info)) = result {
                    let _ = cache_tx.send(CacheUpdate::ModuleSpecific {
                        context_type: cp_base::state::ContextType::new(cp_base::state::ContextType::GITHUB_RESULT),
                        data: Box::new(BranchPrUpdate { pr_info }),
                    });
                }
            }
        }

        // === Poll panel watches ===

        // Snapshot only watches that are due for polling
        let due: Vec<DueWatch> = {
            let watches = watches.lock().unwrap_or_else(|e| e.into_inner());
            watches
                .values()
                .filter(|w| current_ms.saturating_sub(w.last_poll_ms) >= w.poll_interval_secs * 1000)
                .map(|w| {
                    (
                        w.context_id.clone(),
                        w.args.clone(),
                        Arc::clone(&w.github_token),
                        w.is_api_command,
                        w.etag.clone(),
                        w.last_output_hash.clone(),
                    )
                })
                .collect()
        };

        for (context_id, args, github_token, is_api, etag, last_hash) in due {
            let token_str = github_token.expose_secret();
            if is_api {
                let outcome = poll_api_command(&args, token_str, etag.as_deref());

                {
                    let mut watches = watches.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(watch) = watches.get_mut(&context_id) {
                        watch.last_poll_ms = now_ms();
                        if let Some(interval) = outcome.poll_interval {
                            watch.poll_interval_secs = interval;
                        }
                        if let Some((ref new_etag, _)) = outcome.content {
                            watch.etag = new_etag.clone();
                        }
                    }
                }

                if let Some((_, body)) = outcome.content {
                    let body = redact_token(&body, token_str);
                    let body = truncate_output(&body, MAX_RESULT_CONTENT_BYTES);
                    let token_count = estimate_tokens(&body);

                    let _ = cache_tx.send(CacheUpdate::Content { context_id, content: body, token_count });
                }
            } else {
                let result = poll_cli_command(&args, token_str, last_hash.as_deref());

                {
                    let mut watches = watches.lock().unwrap_or_else(|e| e.into_inner());
                    if let Some(watch) = watches.get_mut(&context_id) {
                        watch.last_poll_ms = now_ms();
                        if let Some((ref new_hash, _)) = result {
                            watch.last_output_hash = Some(new_hash.clone());
                        }
                    }
                }

                if let Some((_, content)) = result {
                    let content = redact_token(&content, token_str);
                    let content = truncate_output(&content, MAX_RESULT_CONTENT_BYTES);
                    let token_count = estimate_tokens(&content);

                    let _ = cache_tx.send(CacheUpdate::Content { context_id, content, token_count });
                }
            }
        }
    }
}

/// Outcome of an API poll attempt
struct ApiPollOutcome {
    content: Option<(Option<String>, String)>,
    poll_interval: Option<u64>,
}

/// Poll a `gh api` command using ETag-based conditional requests.
fn poll_api_command(args: &[String], github_token: &str, current_etag: Option<&str>) -> ApiPollOutcome {
    let mut cmd_args = Vec::with_capacity(args.len() + 4);
    cmd_args.push(args[0].clone()); // "api"
    cmd_args.push("-i".to_string());
    cmd_args.extend_from_slice(&args[1..]);

    if let Some(etag) = current_etag {
        cmd_args.push("-H".to_string());
        cmd_args.push(format!("If-None-Match: {}", etag));
    }

    let mut cmd = Command::new("gh");
    cmd.args(&cmd_args)
        .env("GITHUB_TOKEN", github_token)
        .env("GH_TOKEN", github_token)
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1");

    let output = match run_with_timeout(cmd, GH_CMD_TIMEOUT_SECS) {
        Ok(o) => o,
        Err(_) => return ApiPollOutcome { content: None, poll_interval: None },
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !output.status.success() && (stderr.contains("304") || stderr.contains("Not Modified")) {
        let poll_interval = extract_poll_interval(&stdout);
        return ApiPollOutcome { content: None, poll_interval };
    }

    if !output.status.success() {
        return ApiPollOutcome { content: None, poll_interval: None };
    }

    let (new_etag, poll_interval, body) = parse_api_response(&stdout);
    ApiPollOutcome { content: Some((new_etag, body)), poll_interval }
}

/// Poll a non-API `gh` command using output hash comparison.
fn poll_cli_command(args: &[String], github_token: &str, last_hash: Option<&str>) -> Option<(String, String)> {
    let mut cmd = Command::new("gh");
    cmd.args(args)
        .env("GITHUB_TOKEN", github_token)
        .env("GH_TOKEN", github_token)
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1");

    let output = match run_with_timeout(cmd, GH_CMD_TIMEOUT_SECS) {
        Ok(o) => o,
        Err(_) => return None,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let content = if stderr.trim().is_empty() {
        stdout.to_string()
    } else if stdout.trim().is_empty() {
        stderr.to_string()
    } else {
        format!("{}\n{}", stdout, stderr)
    };

    let new_hash = sha256_hex(&content);

    if last_hash == Some(new_hash.as_str()) {
        return None;
    }

    Some((new_hash, content))
}

/// Parse a `gh api -i` response, splitting headers from body.
pub fn parse_api_response(stdout: &str) -> (Option<String>, Option<u64>, String) {
    let (headers, body) = if let Some(pos) = stdout.find("\r\n\r\n") {
        (&stdout[..pos], &stdout[pos + 4..])
    } else if let Some(pos) = stdout.find("\n\n") {
        (&stdout[..pos], &stdout[pos + 2..])
    } else {
        return (None, None, stdout.to_string());
    };

    let etag = extract_header(headers, "etag");
    let poll_interval = extract_header(headers, "x-poll-interval").and_then(|v| v.parse::<u64>().ok());

    (etag, poll_interval, body.to_string())
}

fn extract_header(headers: &str, name: &str) -> Option<String> {
    let prefix = format!("{}:", name);
    headers.lines().find_map(|line| {
        if line.to_lowercase().starts_with(&prefix) { Some(line[prefix.len()..].trim().to_string()) } else { None }
    })
}

/// Try to extract X-Poll-Interval from raw output.
pub fn extract_poll_interval(stdout: &str) -> Option<u64> {
    extract_header(stdout, "x-poll-interval").and_then(|v| v.parse::<u64>().ok())
}

fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    format!("{:064x}", hasher.finalize())
}

fn redact_token(output: &str, token: &str) -> String {
    if token.len() >= 8 && output.contains(token) { output.replace(token, "[REDACTED]") } else { output.to_string() }
}

/// Poll for a PR associated with the given branch.
/// Returns `Some((hash, pr_info))` if output changed; `pr_info` is `None` when no PR exists.
fn poll_branch_pr(
    branch: &str,
    github_token: &str,
    last_hash: Option<&str>,
) -> Option<(String, Option<crate::types::BranchPrInfo>)> {
    let mut cmd = Command::new("gh");
    cmd.args([
        "pr",
        "view",
        branch,
        "--json",
        "number,title,state,url,additions,deletions,reviewDecision,statusCheckRollup",
    ])
    .env("GITHUB_TOKEN", github_token)
    .env("GH_TOKEN", github_token)
    .env("GH_PROMPT_DISABLED", "1")
    .env("NO_COLOR", "1");

    let output = match run_with_timeout(cmd, GH_CMD_TIMEOUT_SECS) {
        Ok(o) => o,
        Err(_) => return None,
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // No PR for this branch
    if !output.status.success() || stderr.contains("no pull requests found") || stdout.trim().is_empty() {
        let hash = sha256_hex("no_pr");
        if last_hash == Some(hash.as_str()) {
            return None; // unchanged
        }
        return Some((hash, None));
    }

    let content = stdout.to_string();
    let new_hash = sha256_hex(&content);
    if last_hash == Some(new_hash.as_str()) {
        return None; // unchanged
    }

    // Parse JSON response
    let pr_info = parse_pr_json(&content);
    Some((new_hash, pr_info))
}

/// Parse the JSON from `gh pr view --json ...`
fn parse_pr_json(json_str: &str) -> Option<crate::types::BranchPrInfo> {
    // Minimal JSON parsing — we only need a few fields
    let number = extract_json_u64(json_str, "number")?;
    let title = extract_json_string(json_str, "title")?;
    let state = extract_json_string(json_str, "state").unwrap_or_else(|| "OPEN".to_string());
    let url = extract_json_string(json_str, "url").unwrap_or_default();
    let additions = extract_json_u64(json_str, "additions");
    let deletions = extract_json_u64(json_str, "deletions");
    let review_decision = extract_json_string(json_str, "reviewDecision");

    // Parse checks status from statusCheckRollup array
    let checks_status = parse_checks_status(json_str);

    Some(crate::types::BranchPrInfo { number, title, state, url, additions, deletions, review_decision, checks_status })
}

/// Extract a string value from JSON by key (simple parser, no serde dependency)
fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":\"", key);
    if let Some(start) = json.find(&pattern) {
        let value_start = start + pattern.len();
        let rest = &json[value_start..];
        // Find closing quote, handling escaped quotes
        let mut end = 0;
        let mut escaped = false;
        for ch in rest.chars() {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                break;
            }
            end += ch.len_utf8();
        }
        return Some(rest[..end].to_string());
    }
    // Try without quotes for null values: "key":null
    None
}

/// Extract a u64 value from JSON by key
fn extract_json_u64(json: &str, key: &str) -> Option<u64> {
    let pattern = format!("\"{}\":", key);
    if let Some(start) = json.find(&pattern) {
        let value_start = start + pattern.len();
        let rest = json[value_start..].trim_start();
        let num_str: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
        return num_str.parse().ok();
    }
    None
}

/// Parse the overall checks status from statusCheckRollup
fn parse_checks_status(json: &str) -> Option<String> {
    if !json.contains("statusCheckRollup") {
        return None;
    }
    // Count conclusion values
    let success = json.matches("\"conclusion\":\"SUCCESS\"").count()
        + json.matches("\"conclusion\":\"NEUTRAL\"").count()
        + json.matches("\"conclusion\":\"SKIPPED\"").count();
    let failure = json.matches("\"conclusion\":\"FAILURE\"").count()
        + json.matches("\"conclusion\":\"TIMED_OUT\"").count()
        + json.matches("\"conclusion\":\"CANCELLED\"").count();
    let pending = json.matches("\"conclusion\":\"\"").count()
        + json.matches("\"conclusion\":null").count()
        + json.matches("\"status\":\"IN_PROGRESS\"").count()
        + json.matches("\"status\":\"QUEUED\"").count()
        + json.matches("\"status\":\"PENDING\"").count();

    if failure > 0 {
        Some("failing".to_string())
    } else if pending > 0 {
        Some("pending".to_string())
    } else if success > 0 {
        Some("passing".to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_api_response_crlf_separator() {
        let input = "HTTP/2.0 200 OK\r\nETag: \"abc123\"\r\nX-Poll-Interval: 60\r\n\r\n{\"id\": 1}";
        let (etag, poll_interval, body) = parse_api_response(input);
        assert_eq!(etag, Some("\"abc123\"".to_string()));
        assert_eq!(poll_interval, Some(60));
        assert_eq!(body, "{\"id\": 1}");
    }

    #[test]
    fn test_parse_api_response_lf_separator() {
        let input = "HTTP/2.0 200 OK\nEtag: W/\"xyz789\"\nX-Poll-Interval: 120\n\n[{\"name\": \"test\"}]";
        let (etag, poll_interval, body) = parse_api_response(input);
        assert_eq!(etag, Some("W/\"xyz789\"".to_string()));
        assert_eq!(poll_interval, Some(120));
        assert_eq!(body, "[{\"name\": \"test\"}]");
    }

    #[test]
    fn test_parse_api_response_no_separator() {
        let input = "{\"id\": 1, \"name\": \"test\"}";
        let (etag, poll_interval, body) = parse_api_response(input);
        assert_eq!(etag, None);
        assert_eq!(poll_interval, None);
        assert_eq!(body, input);
    }

    #[test]
    fn test_parse_api_response_case_insensitive_etag() {
        let input = "HTTP/2.0 200 OK\r\nETAG: \"UPPER\"\r\n\r\nbody";
        let (etag, _poll_interval, body) = parse_api_response(input);
        assert_eq!(etag, Some("\"UPPER\"".to_string()));
        assert_eq!(body, "body");
    }

    #[test]
    fn test_parse_api_response_no_etag_header() {
        let input = "HTTP/2.0 200 OK\r\nContent-Type: text/plain\r\n\r\nhello";
        let (etag, poll_interval, body) = parse_api_response(input);
        assert_eq!(etag, None);
        assert_eq!(poll_interval, None);
        assert_eq!(body, "hello");
    }

    #[test]
    fn test_parse_api_response_no_poll_interval() {
        let input = "HTTP/2.0 200 OK\r\nETag: \"e1\"\r\n\r\ndata";
        let (etag, poll_interval, body) = parse_api_response(input);
        assert_eq!(etag, Some("\"e1\"".to_string()));
        assert_eq!(poll_interval, None);
        assert_eq!(body, "data");
    }

    #[test]
    fn test_parse_api_response_poll_interval_case_insensitive() {
        let input = "HTTP/2.0 200 OK\r\nx-poll-interval: 90\r\n\r\ndata";
        let (_etag, poll_interval, _body) = parse_api_response(input);
        assert_eq!(poll_interval, Some(90));
    }

    #[test]
    fn test_extract_poll_interval_from_304_headers() {
        let stdout = "HTTP/2.0 304 Not Modified\r\nX-Poll-Interval: 60\r\nETag: \"abc\"\r\n";
        assert_eq!(extract_poll_interval(stdout), Some(60));
    }

    #[test]
    fn test_extract_poll_interval_missing() {
        let stdout = "HTTP/2.0 304 Not Modified\r\nETag: \"abc\"\r\n";
        assert_eq!(extract_poll_interval(stdout), None);
    }

    #[test]
    fn test_extract_poll_interval_empty() {
        assert_eq!(extract_poll_interval(""), None);
    }

    #[test]
    fn test_is_api_command_basic() {
        let args: Vec<String> = vec!["api", "/repos/foo/bar"].iter().map(|s| s.to_string()).collect();
        assert!(is_api_command(&args));
    }

    #[test]
    fn test_is_api_command_with_jq() {
        let args: Vec<String> = vec!["api", "/repos/foo/bar", "--jq", ".x"].iter().map(|s| s.to_string()).collect();
        assert!(!is_api_command(&args));
    }

    #[test]
    fn test_is_api_command_with_short_jq() {
        let args: Vec<String> = vec!["api", "/repos/foo/bar", "-q", ".x"].iter().map(|s| s.to_string()).collect();
        assert!(!is_api_command(&args));
    }

    #[test]
    fn test_is_api_command_with_template() {
        let args: Vec<String> =
            vec!["api", "/repos/foo/bar", "--template", "{{.name}}"].iter().map(|s| s.to_string()).collect();
        assert!(!is_api_command(&args));
    }

    #[test]
    fn test_is_api_command_with_short_template() {
        let args: Vec<String> =
            vec!["api", "/repos/foo/bar", "-t", "{{.name}}"].iter().map(|s| s.to_string()).collect();
        assert!(!is_api_command(&args));
    }

    #[test]
    fn test_is_api_command_non_api() {
        let args: Vec<String> = vec!["pr", "list"].iter().map(|s| s.to_string()).collect();
        assert!(!is_api_command(&args));
    }

    #[test]
    fn test_is_api_command_empty() {
        let args: Vec<String> = vec![];
        assert!(!is_api_command(&args));
    }
}
