//! Claude Code API Key implementation.
//!
//! Uses ANTHROPIC_API_KEY from environment with Bearer authentication.
//! Replicates Claude Code's request signature to access Claude 4.5 models.

mod check;
mod stream;

#[cfg(test)]
mod tests;

use std::env;
use std::sync::mpsc::Sender;

use secrecy::SecretBox;
use serde::Deserialize;
use serde_json::Value;

use super::error::LlmError;
use super::{ApiCheckResult, LlmClient, LlmRequest, StreamEvent};
use crate::infra::constants::API_VERSION;

/// API endpoint with beta flag required for Claude 4.5 access
const CLAUDE_CODE_ENDPOINT: &str = "https://api.anthropic.com/v1/messages?beta=true";

/// Beta header with all required flags for Claude Code access (API key mode)
const OAUTH_BETA_HEADER: &str = "interleaved-thinking-2025-05-14,context-management-2025-06-27,prompt-caching-scope-2026-01-05,structured-outputs-2025-12-15";

/// Billing header that must be included in system prompt
const BILLING_HEADER: &str = "x-anthropic-billing-header: cc_version=2.1.44.fbe; cc_entrypoint=cli; cch=e5401;";

/// System reminder injected into first user message for Claude Code validation
const SYSTEM_REMINDER: &str =
    "<system-reminder>\nThe following skills are available for use with the Skill tool:\n</system-reminder>";

/// Map model names to full API model identifiers
fn map_model_name(model: &str) -> &str {
    match model {
        "claude-opus-4-6" => "claude-opus-4-6",
        "claude-opus-4-5" => "claude-opus-4-6",
        "claude-sonnet-4-5" => "claude-sonnet-4-5-20250929",
        "claude-haiku-4-5" => "claude-haiku-4-5-20251001",
        _ => model,
    }
}

/// Inject the system-reminder text block into the first non-tool-result user message.
/// Claude Code's server validates that messages contain this marker.
/// Must skip tool_result user messages (from panel injection) since mixing text blocks
/// into tool_result messages breaks the API's tool_use/tool_result pairing.
fn inject_system_reminder(messages: &mut Vec<Value>) {
    let reminder = serde_json::json!({"type": "text", "text": SYSTEM_REMINDER});

    for msg in messages.iter_mut() {
        if msg["role"] != "user" {
            continue;
        }

        // Skip tool_result messages (from panel injection / tool loop)
        if let Some(arr) = msg["content"].as_array()
            && arr.iter().any(|block| block["type"] == "tool_result")
        {
            continue;
        }

        // Convert string content to array format and prepend reminder
        let content = &msg["content"];
        if content.is_string() {
            let text = content.as_str().unwrap_or("").to_string();
            msg["content"] = serde_json::json!([
                reminder,
                {"type": "text", "text": text}
            ]);
        } else if content.is_array()
            && let Some(arr) = msg["content"].as_array_mut()
        {
            arr.insert(0, reminder);
        }
        return; // Only inject into first eligible user message
    }

    // No eligible user message found (all are tool_results, e.g. during tool loop).
    // Prepend a standalone user message with just the reminder at position 0.
    messages.insert(
        0,
        serde_json::json!({
            "role": "user",
            "content": [reminder]
        }),
    );
    // Must follow with a minimal assistant ack to maintain user/assistant alternation.
    messages.insert(
        1,
        serde_json::json!({
            "role": "assistant",
            "content": [{"type": "text", "text": "ok"}]
        }),
    );
}

/// Ensure strict user/assistant message alternation as required by the API.
fn ensure_message_alternation(messages: &mut Vec<Value>) {
    if messages.len() <= 1 {
        return;
    }

    let mut result: Vec<Value> = Vec::with_capacity(messages.len());

    for msg in messages.drain(..) {
        let same_role = result.last().is_some_and(|last: &Value| last["role"] == msg["role"]);
        if !same_role {
            let blocks = content_to_blocks(msg["content"].clone());
            result.push(serde_json::json!({"role": msg["role"], "content": blocks}));
            continue;
        }

        let prev_has_tool_result = result.last().is_some_and(|last| {
            last["content"].as_array().is_some_and(|arr| arr.iter().any(|b| b["type"] == "tool_result"))
        });
        let curr_has_tool_result =
            msg["content"].as_array().is_some_and(|arr| arr.iter().any(|b| b["type"] == "tool_result"));

        if prev_has_tool_result != curr_has_tool_result {
            result.push(serde_json::json!({
                "role": "assistant",
                "content": [{"type": "text", "text": "ok"}]
            }));
            let blocks = content_to_blocks(msg["content"].clone());
            result.push(serde_json::json!({"role": msg["role"], "content": blocks}));
        } else {
            let new_blocks = content_to_blocks(msg["content"].clone());
            if let Some(arr) = result.last_mut().and_then(|last| last["content"].as_array_mut()) {
                arr.extend(new_blocks);
            }
        }
    }

    if result.first().is_some_and(|m| m["role"] == "assistant") {
        result.insert(
            0,
            serde_json::json!({
                "role": "user",
                "content": [{"type": "text", "text": "ok"}]
            }),
        );
    }

    *messages = result;
}

/// Convert content (string or array) to an array of content blocks.
fn content_to_blocks(content: Value) -> Vec<Value> {
    if content.is_string() {
        vec![serde_json::json!({"type": "text", "text": content.as_str().unwrap_or("")})]
    } else if let Some(arr) = content.as_array() {
        arr.clone()
    } else {
        vec![]
    }
}

/// Directory for last-request debug dumps
const LAST_REQUESTS_DIR: &str = ".context-pilot/last_requests";

/// Dump the outgoing API request to disk for debugging.
fn dump_last_request(worker_id: &str, api_request: &Value) {
    let debug = serde_json::json!({
        "request_url": CLAUDE_CODE_ENDPOINT,
        "request_headers": {
            "anthropic-beta": OAUTH_BETA_HEADER,
            "anthropic-version": API_VERSION,
            "user-agent": "claude-cli/2.1.44 (external, cli)",
            "x-app": "cli",
        },
        "request_body": api_request,
    });
    let _ = std::fs::create_dir_all(LAST_REQUESTS_DIR);
    let path = format!("{}/{}_last_request.json", LAST_REQUESTS_DIR, worker_id);
    let _ = std::fs::write(path, serde_json::to_string_pretty(&debug).unwrap_or_default());
}

/// Claude Code API Key client
pub struct ClaudeCodeApiKeyClient {
    api_key: Option<SecretBox<String>>,
}

impl ClaudeCodeApiKeyClient {
    pub fn new() -> Self {
        let api_key = Self::load_api_key();
        Self { api_key }
    }

    fn load_api_key() -> Option<SecretBox<String>> {
        let key = env::var("ANTHROPIC_API_KEY").ok()?;
        Some(SecretBox::new(Box::new(key)))
    }
}

impl Default for ClaudeCodeApiKeyClient {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct StreamContentBlock {
    #[serde(rename = "type")]
    block_type: Option<String>,
    id: Option<String>,
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamDelta {
    #[serde(rename = "type")]
    delta_type: Option<String>,
    text: Option<String>,
    partial_json: Option<String>,
    stop_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamMessageBody {
    usage: Option<StreamUsage>,
}

#[derive(Debug, Deserialize)]
struct StreamMessage {
    #[serde(rename = "type")]
    event_type: String,
    content_block: Option<StreamContentBlock>,
    delta: Option<StreamDelta>,
    usage: Option<StreamUsage>,
    message: Option<StreamMessageBody>,
}

#[derive(Debug, Deserialize)]
struct StreamUsage {
    input_tokens: Option<usize>,
    output_tokens: Option<usize>,
    cache_creation_input_tokens: Option<usize>,
    cache_read_input_tokens: Option<usize>,
}

impl LlmClient for ClaudeCodeApiKeyClient {
    fn stream(&self, request: LlmRequest, tx: Sender<StreamEvent>) -> Result<(), LlmError> {
        self.do_stream(request, tx)
    }

    fn check_api(&self, model: &str) -> ApiCheckResult {
        self.do_check_api(model)
    }
}
