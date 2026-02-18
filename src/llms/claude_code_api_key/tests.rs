use super::*;
use crate::infra::constants::API_VERSION;

/// Minimal request matching working Python exactly.
/// No panels, no tools, no message prefixes — just raw API call.
#[test]
#[ignore] // Requires API key — run with `cargo test -- --ignored`
fn test_general_kenobi() {
    let token =
        ClaudeCodeApiKeyClient::load_api_key().expect("ANTHROPIC_API_KEY not found in environment");

    let client = reqwest::blocking::Client::new();

    let body = serde_json::json!({
        "model": "claude-opus-4-6",
        "max_tokens": 100,
        "system": [
            {"type": "text", "text": BILLING_HEADER},
            {"type": "text", "text": "You are a helpful assistant."}
        ],
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": SYSTEM_REMINDER},
                {"type": "text", "text": "Hello There! (this is a test, answer General Kenobi)"}
            ]
        }]
    });

    let response = client
        .post(CLAUDE_CODE_ENDPOINT)
        .header("accept", "application/json")
        .header("x-api-key", secrecy::ExposeSecret::expose_secret(&token))
        .header("anthropic-version", API_VERSION)
        .header("anthropic-beta", OAUTH_BETA_HEADER)
        .header("anthropic-dangerous-direct-browser-access", "true")
        .header("content-type", "application/json")
        .header("user-agent", "claude-cli/2.1.44 (external, cli)")
        .header("x-app", "cli")
        .header("x-stainless-arch", "x64")
        .header("x-stainless-lang", "js")
        .header("x-stainless-os", "Linux")
        .header("x-stainless-package-version", "0.74.0")
        .header("x-stainless-timeout", "600")
        .header("x-stainless-retry-count", "0")
        .header("x-stainless-runtime", "node")
        .header("x-stainless-runtime-version", "v24.3.0")
        .json(&body)
        .send()
        .expect("HTTP request failed");

    let status = response.status();
    let resp_body: serde_json::Value = response.json().expect("Failed to parse JSON response");

    assert!(status.is_success(), "API returned {}: {}", status, serde_json::to_string_pretty(&resp_body).unwrap());

    let text = resp_body["content"][0]["text"].as_str().expect("No text in response content");

    assert!(text.to_lowercase().contains("general kenobi"), "Expected 'General Kenobi' in response, got: {}", text);
}

/// Same as above but with tools and streaming — matches what stream() actually sends.
#[test]
#[ignore] // Requires API key — run with `cargo test -- --ignored`
fn test_general_kenobi_with_tools_streaming() {
    let token =
        ClaudeCodeApiKeyClient::load_api_key().expect("ANTHROPIC_API_KEY not found in environment");

    let client = reqwest::blocking::Client::new();

    let body = serde_json::json!({
        "model": "claude-opus-4-6",
        "max_tokens": 4096,
        "system": [
            {"type": "text", "text": BILLING_HEADER},
            {"type": "text", "text": "You are a helpful assistant."}
        ],
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": SYSTEM_REMINDER},
                {"type": "text", "text": "Hello There! (this is a test, answer General Kenobi)"}
            ]
        }],
        "tools": [{
            "name": "test_tool",
            "description": "A test tool",
            "input_schema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        }],
        "stream": true
    });

    let response = client
        .post(CLAUDE_CODE_ENDPOINT)
        .header("accept", "text/event-stream")
        .header("x-api-key", secrecy::ExposeSecret::expose_secret(&token))
        .header("anthropic-version", API_VERSION)
        .header("anthropic-beta", OAUTH_BETA_HEADER)
        .header("anthropic-dangerous-direct-browser-access", "true")
        .header("content-type", "application/json")
        .header("user-agent", "claude-cli/2.1.44 (external, cli)")
        .header("x-app", "cli")
        .header("x-stainless-arch", "x64")
        .header("x-stainless-lang", "js")
        .header("x-stainless-os", "Linux")
        .header("x-stainless-package-version", "0.74.0")
        .header("x-stainless-timeout", "600")
        .header("x-stainless-retry-count", "0")
        .header("x-stainless-runtime", "node")
        .header("x-stainless-runtime-version", "v24.3.0")
        .json(&body)
        .send()
        .expect("HTTP request failed");

    let status = response.status();

    assert!(status.is_success(), "API returned {}", status);

    let mut full_text = String::new();
    let reader = std::io::BufReader::new(response);
    for line in std::io::BufRead::lines(reader) {
        let line = line.expect("Read error");
        if !line.starts_with("data: ") {
            continue;
        }
        let json_str = &line[6..];
        if json_str == "[DONE]" {
            break;
        }
        if let Ok(event) = serde_json::from_str::<serde_json::Value>(json_str) {
            if event["type"] == "content_block_delta" {
                if let Some(text) = event["delta"]["text"].as_str() {
                    full_text.push_str(text);
                }
            }
        }
    }

    assert!(
        full_text.to_lowercase().contains("general kenobi"),
        "Expected 'General Kenobi' in streamed response, got: {}",
        full_text
    );
}

/// Test inject_system_reminder: verify it skips tool_result messages
#[test]
fn test_inject_system_reminder_skips_tool_results() {
    let mut messages = vec![
        serde_json::json!({
            "role": "assistant",
            "content": [{"type": "text", "text": "panel"}, {"type": "tool_use", "id": "panel_P2", "name": "dynamic_panel", "input": {}}]
        }),
        serde_json::json!({
            "role": "user",
            "content": [{"type": "tool_result", "tool_use_id": "panel_P2", "content": "data"}]
        }),
        serde_json::json!({
            "role": "user",
            "content": "Hello there"
        }),
    ];

    inject_system_reminder(&mut messages);

    assert!(
        messages[1]["content"][0]["type"] == "tool_result",
        "tool_result message was modified: {:?}",
        messages[1]["content"]
    );

    assert!(messages[2]["content"].is_array(), "Regular user message not converted to array");
    let arr = messages[2]["content"].as_array().unwrap();
    assert_eq!(arr.len(), 2, "Expected 2 blocks (reminder + text)");
    assert!(arr[0]["text"].as_str().unwrap().contains("system-reminder"), "First block should be system-reminder");
    assert_eq!(arr[1]["text"].as_str().unwrap(), "Hello there");
}

/// Test inject_system_reminder: when no eligible message, prepends fallback pair
#[test]
fn test_inject_system_reminder_no_eligible() {
    let mut messages = vec![
        serde_json::json!({
            "role": "assistant",
            "content": [{"type": "tool_use", "id": "t1", "name": "x", "input": {}}]
        }),
        serde_json::json!({
            "role": "user",
            "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "data"}]
        }),
    ];

    inject_system_reminder(&mut messages);

    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0]["role"], "user");
    assert!(messages[0]["content"][0]["text"].as_str().unwrap().contains("system-reminder"));
    assert_eq!(messages[1]["role"], "assistant");
    assert!(messages[3]["content"][0]["type"] == "tool_result");
}

/// Test ensure_message_alternation
#[test]
fn test_ensure_message_alternation() {
    let mut messages = vec![
        serde_json::json!({"role": "assistant", "content": [{"type": "text", "text": "panel"}]}),
        serde_json::json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": "panel_footer", "content": "ok"}]}),
        serde_json::json!({"role": "user", "content": "Hello"}),
        serde_json::json!({"role": "user", "content": "World"}),
        serde_json::json!({"role": "user", "content": "Again"}),
    ];

    ensure_message_alternation(&mut messages);

    assert_eq!(
        messages.len(),
        5,
        "Got {} messages: {:?}",
        messages.len(),
        messages.iter().map(|m| m["role"].as_str().unwrap_or("?")).collect::<Vec<_>>()
    );
    assert_eq!(messages[0]["role"], "user");
    assert_eq!(messages[1]["role"], "assistant");
    assert_eq!(messages[2]["role"], "user");
    assert_eq!(messages[2]["content"][0]["type"], "tool_result");
    assert_eq!(messages[3]["role"], "assistant");
    assert_eq!(messages[4]["role"], "user");

    let user_content = messages[4]["content"].as_array().unwrap();
    assert_eq!(user_content.len(), 3);
    assert_eq!(user_content[0]["text"], "Hello");
    assert_eq!(user_content[1]["text"], "World");
    assert_eq!(user_content[2]["text"], "Again");
}

/// Test that alternation + reminder injection work together
#[test]
fn test_alternation_then_reminder() {
    let mut messages = vec![
        serde_json::json!({"role": "assistant", "content": [{"type": "text", "text": "footer"}, {"type": "tool_use", "id": "panel_footer", "name": "dynamic_panel", "input": {}}]}),
        serde_json::json!({"role": "user", "content": [{"type": "tool_result", "tool_use_id": "panel_footer", "content": "ok"}]}),
        serde_json::json!({"role": "user", "content": "Hello"}),
        serde_json::json!({"role": "user", "content": "World"}),
    ];

    ensure_message_alternation(&mut messages);
    inject_system_reminder(&mut messages);

    assert_eq!(messages.len(), 5);
    let user_content = messages[0]["content"].as_array().unwrap();
    assert!(
        user_content[0]["text"].as_str().unwrap().contains("system-reminder"),
        "First block should be system-reminder, got: {:?}",
        user_content[0]
    );
}
