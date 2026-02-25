//! Claude Code API Key implementation.
//!
//! Uses ANTHROPIC_API_KEY from environment with Bearer authentication.
//! Replicates Claude Code's request signature to access Claude 4.5 models.

mod check;
pub mod helpers;
mod streaming;
mod tests;

use std::env;
use std::sync::mpsc::Sender;

use reqwest::blocking::Client;
use secrecy::{ExposeSecret, SecretBox};
use serde_json::Value;

use super::error::LlmError;
use super::{
    ApiCheckResult, LlmClient, LlmRequest, StreamEvent, panel_footer_text, panel_header_text, panel_timestamp_text,
    prepare_panel_messages,
};
use crate::app::panels::now_ms;
use crate::infra::constants::{MAX_RESPONSE_TOKENS, library};
use crate::infra::tools::build_api_tools;
use crate::state::{MessageStatus, MessageType};

use helpers::*;

/// Claude Code API Key client
pub struct ClaudeCodeApiKeyClient {
    api_key: Option<SecretBox<String>>,
}

impl ClaudeCodeApiKeyClient {
    pub fn new() -> Self {
        let api_key = Self::load_api_key();
        Self { api_key }
    }

    pub(crate) fn load_api_key() -> Option<SecretBox<String>> {
        let key = env::var("ANTHROPIC_API_KEY").ok()?;
        Some(SecretBox::new(Box::new(key)))
    }
}

impl Default for ClaudeCodeApiKeyClient {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmClient for ClaudeCodeApiKeyClient {
    fn stream(&self, request: LlmRequest, tx: Sender<StreamEvent>) -> Result<(), LlmError> {
        let api_key =
            self.api_key.as_ref().ok_or_else(|| LlmError::Auth("ANTHROPIC_API_KEY not found in environment".into()))?;

        let client = Client::builder().timeout(None).build().map_err(|e| LlmError::Network(e.to_string()))?;

        // Handle cleaner mode or custom system prompt
        let system_text = if let Some(ref prompt) = request.system_prompt {
            prompt.clone()
        } else {
            library::default_agent_content().to_string()
        };

        // Build messages as simple JSON (matching Python example format)
        let mut json_messages: Vec<Value> = Vec::new();
        let current_ms = now_ms();

        // Inject context panels as fake tool call/result pairs (P2+ only, sorted by timestamp)
        let fake_panels = prepare_panel_messages(&request.context_items);

        if !fake_panels.is_empty() {
            let panel_count = fake_panels.len();
            let mut cache_breakpoints = std::collections::BTreeSet::new();
            for quarter in 1..=4usize {
                let pos = (panel_count * quarter).div_ceil(4);
                cache_breakpoints.insert(pos.saturating_sub(1));
            }

            for (idx, panel) in fake_panels.iter().enumerate() {
                let timestamp_text = panel_timestamp_text(panel.timestamp_ms);
                let text =
                    if idx == 0 { format!("{}\n\n{}", panel_header_text(), timestamp_text) } else { timestamp_text };

                json_messages.push(serde_json::json!({
                    "role": "assistant",
                    "content": [
                        {"type": "text", "text": text},
                        {
                            "type": "tool_use",
                            "id": format!("panel_{}", panel.panel_id),
                            "name": "dynamic_panel",
                            "input": {"id": panel.panel_id}
                        }
                    ]
                }));

                let mut tool_result = serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": format!("panel_{}", panel.panel_id),
                    "content": panel.content
                });
                if cache_breakpoints.contains(&idx) {
                    tool_result["cache_control"] = serde_json::json!({"type": "ephemeral"});
                }
                json_messages.push(serde_json::json!({
                    "role": "user",
                    "content": [tool_result]
                }));
            }

            let footer = panel_footer_text(&request.messages, current_ms);
            json_messages.push(serde_json::json!({
                "role": "assistant",
                "content": [
                    {"type": "text", "text": footer},
                    {
                        "type": "tool_use",
                        "id": "panel_footer",
                        "name": "dynamic_panel",
                        "input": {"action": "end_panels"}
                    }
                ]
            }));
            json_messages.push(serde_json::json!({
                "role": "user",
                "content": [{
                    "type": "tool_result",
                    "tool_use_id": "panel_footer",
                    "content": crate::infra::constants::prompts::panel_footer_ack()
                }]
            }));
        }

        // Handle cleaner mode extra context
        if let Some(ref context) = request.extra_context {
            json_messages.push(serde_json::json!({
                "role": "user",
                "content": format!("Please clean up the context to reduce token usage:\n\n{}", context)
            }));
        }

        let include_tool_uses = request.tool_results.is_some();

        // First pass: collect tool_use IDs that have matching results
        let mut included_tool_use_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (idx, msg) in request.messages.iter().enumerate() {
            if msg.status == MessageStatus::Deleted
                || msg.status == MessageStatus::Detached
                || msg.message_type != MessageType::ToolCall
            {
                continue;
            }
            let tool_use_ids: Vec<&str> = msg.tool_uses.iter().map(|t| t.id.as_str()).collect();
            let has_result = request.messages[idx + 1..]
                .iter()
                .filter(|m| {
                    m.status != MessageStatus::Deleted
                        && m.status != MessageStatus::Detached
                        && m.message_type == MessageType::ToolResult
                })
                .any(|m| m.tool_results.iter().any(|r| tool_use_ids.contains(&r.tool_use_id.as_str())));
            if has_result {
                for id in tool_use_ids {
                    included_tool_use_ids.insert(id.to_string());
                }
            }
        }

        for (idx, msg) in request.messages.iter().enumerate() {
            if msg.status == MessageStatus::Deleted || msg.status == MessageStatus::Detached {
                continue;
            }
            if msg.content.is_empty() && msg.tool_uses.is_empty() && msg.tool_results.is_empty() {
                continue;
            }

            if msg.message_type == MessageType::ToolResult {
                let tool_results: Vec<Value> = msg
                    .tool_results
                    .iter()
                    .filter(|r| included_tool_use_ids.contains(&r.tool_use_id))
                    .map(|r| {
                        serde_json::json!({
                            "type": "tool_result",
                            "tool_use_id": r.tool_use_id,
                            "content": r.content
                        })
                    })
                    .collect();

                if !tool_results.is_empty() {
                    json_messages.push(serde_json::json!({
                        "role": "user",
                        "content": tool_results
                    }));
                }
                continue;
            }

            if msg.message_type == MessageType::ToolCall {
                let tool_uses: Vec<Value> = msg
                    .tool_uses
                    .iter()
                    .filter(|tu| included_tool_use_ids.contains(&tu.id))
                    .map(|tu| {
                        serde_json::json!({
                            "type": "tool_use",
                            "id": tu.id,
                            "name": tu.name,
                            "input": if tu.input.is_null() { serde_json::json!({}) } else { tu.input.clone() }
                        })
                    })
                    .collect();

                if !tool_uses.is_empty() {
                    if let Some(last) = json_messages.last_mut()
                        && last["role"] == "assistant"
                        && let Some(content) = last.get_mut("content")
                        && let Some(arr) = content.as_array_mut()
                    {
                        arr.extend(tool_uses);
                        continue;
                    }

                    json_messages.push(serde_json::json!({
                        "role": "assistant",
                        "content": tool_uses
                    }));
                }
                continue;
            }

            let message_content = msg.content.clone();

            if !message_content.is_empty() {
                json_messages.push(serde_json::json!({
                    "role": msg.role,
                    "content": message_content
                }));
            }

            let is_last = idx == request.messages.len().saturating_sub(1);
            if msg.role == "assistant" && include_tool_uses && is_last && !msg.tool_uses.is_empty() {
                let tool_uses: Vec<Value> = msg
                    .tool_uses
                    .iter()
                    .map(|tu| {
                        serde_json::json!({
                            "type": "tool_use",
                            "id": tu.id,
                            "name": tu.name,
                            "input": if tu.input.is_null() { serde_json::json!({}) } else { tu.input.clone() }
                        })
                    })
                    .collect();

                if let Some(last) = json_messages.last_mut()
                    && last["role"] == "assistant"
                {
                    let existing_content = last["content"].clone();
                    let mut content_array = if existing_content.is_string() {
                        vec![serde_json::json!({"type": "text", "text": existing_content.as_str().unwrap_or("")})]
                    } else if let Some(arr) = existing_content.as_array() {
                        arr.clone()
                    } else {
                        vec![]
                    };
                    content_array.extend(tool_uses);
                    last["content"] = Value::Array(content_array);
                }
            }
        }

        // Add pending tool results
        if let Some(results) = &request.tool_results {
            let tool_results: Vec<Value> = results
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": r.tool_use_id,
                        "content": r.content
                    })
                })
                .collect();
            json_messages.push(serde_json::json!({
                "role": "user",
                "content": tool_results
            }));
        }

        ensure_message_alternation(&mut json_messages);
        inject_system_reminder(&mut json_messages);

        let api_request = serde_json::json!({
            "model": map_model_name(&request.model),
            "max_tokens": MAX_RESPONSE_TOKENS,
            "system": [
                {"type": "text", "text": BILLING_HEADER},
                {"type": "text", "text": system_text}
            ],
            "messages": json_messages,
            "tools": build_api_tools(&request.tools),
            "stream": true
        });

        dump_last_request(&request.worker_id, &api_request);

        let response =
            apply_claude_code_headers(client.post(CLAUDE_CODE_ENDPOINT), api_key.expose_secret(), "text/event-stream")
                .json(&api_request)
                .send()?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().unwrap_or_default();
            return Err(LlmError::Api { status, body });
        }

        let resp_headers: String = response
            .headers()
            .iter()
            .map(|(k, v)| format!("  {}: {}", k, v.to_str().unwrap_or("<binary>")))
            .collect::<Vec<_>>()
            .join("\n");

        let (input_tokens, output_tokens, cache_hit_tokens, cache_miss_tokens, stop_reason) =
            streaming::parse_sse_stream(response, &resp_headers, &tx)?;

        let _ = tx.send(StreamEvent::Done {
            input_tokens,
            output_tokens,
            cache_hit_tokens,
            cache_miss_tokens,
            stop_reason,
        });
        Ok(())
    }

    fn check_api(&self, model: &str) -> ApiCheckResult {
        self.check_api_impl(model)
    }
}
