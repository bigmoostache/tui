use crate::persistence::{delete_message, save_message};
use crate::state::{estimate_tokens, ContextType, Message, MessageStatus, MessageType, PromptItem, State};

use super::helpers::{parse_context_pattern, find_context_by_id};
use super::ActionResult;

/// Handle InputSubmit action — context switching, message creation, stream start
pub fn handle_input_submit(state: &mut State) -> ActionResult {
    if state.input.is_empty() {
        return ActionResult::Nothing;
    }

    // Context switching is always allowed, even during streaming
    if let Some(id) = parse_context_pattern(&state.input) {
        if let Some(index) = find_context_by_id(state, &id) {
            state.selected_context = index;
            state.scroll_offset = 0.0;
            state.user_scrolled = false;
            state.input.clear();
            state.input_cursor = 0;
            return ActionResult::Nothing;
        }
    }

    let content = replace_commands(&state.input, &state.commands);
    state.input.clear();
    state.input_cursor = 0;
    let user_token_estimate = estimate_tokens(&content);

    // Assign user display ID and UID
    let user_id = format!("U{}", state.next_user_id);
    let user_uid = format!("UID_{}_U", state.global_next_uid);
    state.next_user_id += 1;
    state.global_next_uid += 1;

    let user_msg = Message {
        id: user_id,
        uid: Some(user_uid),
        role: "user".to_string(),
        message_type: MessageType::TextMessage,
        content,
        content_token_count: user_token_estimate,
        tl_dr: None,
        tl_dr_token_count: 0,
        status: MessageStatus::Full,
        tool_uses: Vec::new(),
        tool_results: Vec::new(),
        input_tokens: 0,
        timestamp_ms: crate::core::panels::now_ms(),
    };
    save_message(&user_msg);

    // Add user message tokens to Conversation context and update timestamp
    if let Some(ctx) = state.context.iter_mut().find(|c| c.context_type == ContextType::Conversation) {
        ctx.token_count += user_token_estimate;
        ctx.last_refresh_ms = crate::core::panels::now_ms();
    }

    // During streaming: insert BEFORE the streaming assistant message
    // Otherwise: append normally
    if state.is_streaming {
        // Insert before the last message (the streaming assistant message)
        let insert_pos = state.messages.len().saturating_sub(1);
        state.messages.insert(insert_pos, user_msg);
        return ActionResult::SaveMessage(state.messages[insert_pos].uid.clone().unwrap_or_else(|| state.messages[insert_pos].id.clone()));
    }

    state.messages.push(user_msg);

    // Create assistant message and start streaming
    let assistant_id = format!("A{}", state.next_assistant_id);
    let assistant_uid = format!("UID_{}_A", state.global_next_uid);
    state.next_assistant_id += 1;
    state.global_next_uid += 1;

    let assistant_msg = Message {
        id: assistant_id,
        uid: Some(assistant_uid),
        role: "assistant".to_string(),
        message_type: MessageType::TextMessage,
        content: String::new(),
        content_token_count: 0,
        tl_dr: None,
        tl_dr_token_count: 0,
        status: MessageStatus::Full,
        tool_uses: Vec::new(),
        tool_results: Vec::new(),
        input_tokens: 0,
        timestamp_ms: crate::core::panels::now_ms(),
    };
    state.messages.push(assistant_msg);

    state.is_streaming = true;
    state.last_stop_reason = None;
    state.streaming_estimated_tokens = 0;
    // Reset per-stream and per-tick token counters
    state.stream_cache_hit_tokens = 0;
    state.stream_cache_miss_tokens = 0;
    state.stream_output_tokens = 0;
    state.tick_cache_hit_tokens = 0;
    state.tick_cache_miss_tokens = 0;
    state.tick_output_tokens = 0;
    ActionResult::StartStream
}

/// Handle ClearConversation action
pub fn handle_clear_conversation(state: &mut State) -> ActionResult {
    for msg in &state.messages {
        // Delete by UID if available, otherwise by id
        let file_id = msg.uid.as_ref().unwrap_or(&msg.id);
        delete_message(file_id);
    }
    state.messages.clear();
    state.input.clear();
    // Reset token count for Conversation context and update timestamp
    if let Some(ctx) = state.context.iter_mut().find(|c| c.context_type == ContextType::Conversation) {
        ctx.token_count = 0;
        ctx.last_refresh_ms = crate::core::panels::now_ms();
    }
    ActionResult::Save
}

/// Replace /command-name tokens in input with command content.
/// Only replaces at line start (after optional whitespace).
fn replace_commands(input: &str, commands: &[PromptItem]) -> String {
    if commands.is_empty() || !input.contains('/') {
        return input.to_string();
    }

    input.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if !trimmed.starts_with('/') {
                return line.to_string();
            }
            // Extract the command token after /
            let token = &trimmed[1..];
            let (cmd_id, rest) = match token.find(|c: char| c.is_whitespace()) {
                Some(pos) => (&token[..pos], &token[pos..]),
                None => (token, ""),
            };
            if let Some(cmd) = commands.iter().find(|c| c.id == cmd_id) {
                format!("{}{}", cmd.content.trim_end(), rest)
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}
