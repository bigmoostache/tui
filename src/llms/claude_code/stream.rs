use std::io::{BufRead, BufReader};
use std::sync::mpsc::Sender;

use reqwest::blocking::Client;
use secrecy::ExposeSecret;
use serde_json::Value;

use super::{
    ClaudeCodeClient, StreamMessage, BILLING_HEADER, CLAUDE_CODE_ENDPOINT, OAUTH_BETA_HEADER,
    dump_last_request, ensure_message_alternation, inject_system_reminder, map_model_name,
};
use crate::app::panels::now_ms;
use crate::infra::constants::{API_VERSION, MAX_RESPONSE_TOKENS, library};
use crate::infra::tools::{ToolUse, build_api_tools};
use crate::llms::error::LlmError;
use crate::llms::{LlmRequest, StreamEvent, panel_footer_text, panel_header_text, panel_timestamp_text, prepare_panel_messages};
use crate::state::{MessageStatus, MessageType};

impl ClaudeCodeClient {
    pub(super) fn do_stream(&self, request: LlmRequest, tx: Sender<StreamEvent>) -> Result<(), LlmError> {
        let access_token = self
            .access_token
            .as_ref()
            .ok_or_else(|| LlmError::Auth("Claude Code OAuth token not found or expired. Run 'claude login'".into()))?;

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
            // Calculate cache breakpoint positions at 25%, 50%, 75%, 100% of panels.
            // Prefix-based caching means each breakpoint caches everything before it,
            // so spreading them across panels maximizes partial cache hits when only
            // later panels change. Uses ceiling division to distribute evenly.
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

                // Assistant message with tool_use
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

                // User message with tool_result (cache_control at breakpoint positions)
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

            // Add footer after all panels
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

        // First pass: collect tool_use IDs that have matching results (will be included)
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

            // Handle tool results - only include if tool_use was included
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

            // Handle tool calls - only include if has matching result
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
                    // Append to last assistant message or create new one
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

            // Regular text message
            let message_content = match msg.status {
                MessageStatus::Summarized => msg.tl_dr.as_ref().unwrap_or(&msg.content).clone(),
                _ => msg.content.clone(),
            };

            if !message_content.is_empty() {
                // Use simple string content like Python example
                json_messages.push(serde_json::json!({
                    "role": msg.role,
                    "content": message_content
                }));
            }

            // Add tool uses to last assistant message if this is the last message
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
                    // Convert string content to array and add tool uses
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

        // Ensure strict user/assistant alternation (merges consecutive same-role messages)
        ensure_message_alternation(&mut json_messages);

        // Inject system-reminder into first user message for Claude Code validation
        inject_system_reminder(&mut json_messages);

        // Build final request (cache_control breakpoints are on panel tool_results above)
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

        // Always dump last request for debugging (overwritten each call)
        dump_last_request(&request.worker_id, &api_request);

        let response = client
            .post(CLAUDE_CODE_ENDPOINT)
            .header("accept", "text/event-stream")
            .header("authorization", format!("Bearer {}", access_token.expose_secret()))
            .header("anthropic-version", API_VERSION)
            .header("anthropic-beta", OAUTH_BETA_HEADER)
            .header("anthropic-dangerous-direct-browser-access", "true")
            .header("content-type", "application/json")
            .header("user-agent", "claude-cli/2.1.37 (external, cli)")
            .header("x-app", "cli")
            .header("x-stainless-arch", "x64")
            .header("x-stainless-lang", "js")
            .header("x-stainless-os", "Linux")
            .header("x-stainless-package-version", "0.70.0")
            .header("x-stainless-retry-count", "0")
            .header("x-stainless-runtime", "node")
            .header("x-stainless-runtime-version", "v24.3.0")
            .json(&api_request)
            .send()?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let body = response.text().unwrap_or_default();
            return Err(LlmError::Api { status, body });
        }

        // Log response headers for debugging stream errors
        let resp_headers: String = response
            .headers()
            .iter()
            .map(|(k, v)| format!("  {}: {}", k, v.to_str().unwrap_or("<binary>")))
            .collect::<Vec<_>>()
            .join("\n");

        let mut reader = BufReader::new(response);
        let mut input_tokens = 0;
        let mut output_tokens = 0;
        let mut cache_hit_tokens = 0;
        let mut cache_miss_tokens = 0;
        let mut current_tool: Option<(String, String, String)> = None;
        let mut stop_reason: Option<String> = None;
        let mut total_bytes: usize = 0;
        let mut line_count: usize = 0;
        let mut last_lines: Vec<String> = Vec::new();

        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    total_bytes += n;
                    line_count += 1;
                }
                Err(e) => {
                    // Walk the error source chain to find the real cause.
                    // reqwest wraps errors: io::Error(Other) → "error decoding response body" → real cause
                    // Known causes:
                    //   TimedOut → model took too long generating output (fix: Client::builder().timeout(None))
                    //   ConnectionReset → server killed the connection
                    //   UnexpectedEof → chunked transfer ended prematurely
                    let error_kind = format!("{:?}", e.kind());
                    let mut root_cause = String::new();
                    let mut source: Option<&dyn std::error::Error> = std::error::Error::source(&e);
                    while let Some(s) = source {
                        root_cause = format!("{}", s);
                        source = std::error::Error::source(s);
                    }
                    let tool_ctx = match &current_tool {
                        Some((id, name, partial)) => {
                            format!("In-flight tool: {} (id={}), partial input: {} bytes", name, id, partial.len())
                        }
                        None => "No tool in progress".to_string(),
                    };
                    let recent =
                        if last_lines.is_empty() { "(no lines read)".to_string() } else { last_lines.join("\n") };
                    let verbose = format!(
                        "{}\n\
                         Error kind: {} | Root cause: {}\n\
                         Stream position: {} bytes, {} lines read\n\
                         {}\n\
                         Response headers:\n{}\n\
                         Last SSE lines:\n{}",
                        e,
                        error_kind,
                        if root_cause.is_empty() { "(none)".to_string() } else { root_cause },
                        total_bytes,
                        line_count,
                        tool_ctx,
                        resp_headers,
                        recent
                    );
                    return Err(LlmError::StreamRead(verbose));
                }
            }
            let line = line.trim_end_matches('\n').trim_end_matches('\r');

            if !line.starts_with("data: ") {
                continue;
            }

            // Keep last 5 data lines for error context
            if last_lines.len() >= 5 {
                last_lines.remove(0);
            }
            last_lines.push(line.to_string());

            let json_str = &line[6..];
            if json_str == "[DONE]" {
                break;
            }

            if let Ok(event) = serde_json::from_str::<StreamMessage>(json_str) {
                match event.event_type.as_str() {
                    "content_block_start" => {
                        if let Some(block) = event.content_block
                            && block.block_type.as_deref() == Some("tool_use")
                        {
                            current_tool =
                                Some((block.id.unwrap_or_default(), block.name.unwrap_or_default(), String::new()));
                        }
                    }
                    "content_block_delta" => {
                        if let Some(delta) = event.delta {
                            match delta.delta_type.as_deref() {
                                Some("text_delta") => {
                                    if let Some(text) = delta.text {
                                        let _ = tx.send(StreamEvent::Chunk(text));
                                    }
                                }
                                Some("input_json_delta") => {
                                    if let Some(json) = delta.partial_json
                                        && let Some((_, _, ref mut input)) = current_tool
                                    {
                                        input.push_str(&json);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                    "content_block_stop" => {
                        if let Some((id, name, input_json)) = current_tool.take() {
                            let input: Value = serde_json::from_str(&input_json)
                                .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));
                            let _ = tx.send(StreamEvent::ToolUse(ToolUse { id, name, input }));
                        }
                    }
                    "message_start" => {
                        if let Some(msg_body) = event.message
                            && let Some(usage) = msg_body.usage
                        {
                            if let Some(hit) = usage.cache_read_input_tokens {
                                cache_hit_tokens = hit;
                            }
                            if let Some(miss) = usage.cache_creation_input_tokens {
                                cache_miss_tokens = miss;
                            }
                            if let Some(inp) = usage.input_tokens {
                                input_tokens = inp;
                            }
                        }
                    }
                    "message_delta" => {
                        if let Some(ref delta) = event.delta
                            && let Some(ref reason) = delta.stop_reason
                        {
                            stop_reason = Some(reason.clone());
                        }
                        if let Some(usage) = event.usage {
                            if let Some(inp) = usage.input_tokens {
                                input_tokens = inp;
                            }
                            if let Some(out) = usage.output_tokens {
                                output_tokens = out;
                            }
                        }
                    }
                    "message_stop" => break,
                    _ => {}
                }
            }
        }

        let _ = tx.send(StreamEvent::Done {
            input_tokens,
            output_tokens,
            cache_hit_tokens,
            cache_miss_tokens,
            stop_reason,
        });
        Ok(())
    }
}
