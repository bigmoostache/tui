use crate::core::panels::{collect_all_context, now_ms, refresh_all_panels, ContextItem};
use crate::constants::TOKEN_DETACH_COUNT;
use crate::state::{
    compute_total_pages, estimate_tokens, ContextElement, ContextType,
    Message, MessageStatus, MessageType, State,
};
use crate::tool_defs::ToolDefinition;
use crate::tools::refresh_conversation_context;

/// Context data prepared for streaming
pub struct StreamContext {
    pub messages: Vec<Message>,
    pub context_items: Vec<ContextItem>,
    pub tools: Vec<ToolDefinition>,
}

/// Refresh all context elements and prepare data for streaming
pub fn prepare_stream_context(state: &mut State, include_last_message: bool) -> StreamContext {
    // Detach old conversation chunks before anything else
    detach_conversation_chunks(state);

    // Refresh conversation token counts (not panel-based yet)
    refresh_conversation_context(state);

    // Refresh all panel token counts
    refresh_all_panels(state);

    // Collect all context items from panels
    let context_items = collect_all_context(state);

    // Dynamically enable/disable panel_goto_page based on whether any panel is paginated
    let has_paginated = state.context.iter().any(|c| c.total_pages > 1);
    for tool in &mut state.tools {
        if tool.id == "panel_goto_page" {
            tool.enabled = has_paginated;
        }
    }

    // Prepare messages
    let messages: Vec<_> = if include_last_message {
        state.messages.iter()
            .filter(|m| !m.content.is_empty() || !m.tool_uses.is_empty() || !m.tool_results.is_empty())
            .cloned()
            .collect()
    } else {
        state.messages.iter()
            .filter(|m| !m.content.is_empty() || !m.tool_uses.is_empty() || !m.tool_results.is_empty())
            .take(state.messages.len().saturating_sub(1))
            .cloned()
            .collect()
    };

    StreamContext {
        messages,
        context_items,
        tools: state.tools.clone(),
    }
}

// ─── Conversation History Detachment ────────────────────────────────────────

/// Check if `idx` is a turn boundary — a safe place to split the conversation.
/// A turn boundary is after a complete assistant turn:
/// - After an assistant text message (not a tool call)
/// - After a tool result, IF the next message is a user text message (end of tool loop)
/// - After a tool result that is the last message (shouldn't happen but handle gracefully)
fn is_turn_boundary(messages: &[Message], idx: usize) -> bool {
    let msg = &messages[idx];

    // Skip Deleted/Detached messages — not meaningful boundaries
    if msg.status == MessageStatus::Deleted || msg.status == MessageStatus::Detached {
        return false;
    }

    // After an assistant text message (not a tool call)
    if msg.role == "assistant" && msg.message_type == MessageType::TextMessage {
        return true;
    }

    // After a tool result, if next non-skipped message is a user text message
    if msg.message_type == MessageType::ToolResult {
        for next in &messages[idx + 1..] {
            if next.status == MessageStatus::Deleted || next.status == MessageStatus::Detached {
                continue;
            }
            return next.role == "user" && next.message_type == MessageType::TextMessage;
        }
        return true; // Last message in conversation
    }

    false
}

/// Format a range of messages into a text chunk for the ConversationHistory panel.
/// Uses the same `[ID]:\ncontent` format the LLM sees in conversation messages.
fn format_chunk_content(messages: &[Message], start: usize, end: usize) -> String {
    let mut output = String::new();
    for msg in &messages[start..end] {
        if msg.status == MessageStatus::Deleted || msg.status == MessageStatus::Detached {
            continue;
        }
        match msg.message_type {
            MessageType::ToolCall => {
                for tu in &msg.tool_uses {
                    output += &format!(
                        "[{}]: tool_call {}({})\n",
                        msg.id,
                        tu.name,
                        serde_json::to_string(&tu.input).unwrap_or_default()
                    );
                }
            }
            MessageType::ToolResult => {
                for tr in &msg.tool_results {
                    output += &format!("[{}]:\n{}\n", msg.id, tr.content);
                }
            }
            MessageType::TextMessage => {
                let content = match msg.status {
                    MessageStatus::Summarized => {
                        msg.tl_dr.as_deref().unwrap_or(&msg.content)
                    }
                    _ => &msg.content,
                };
                if !content.is_empty() {
                    output += &format!("[{}]:\n{}\n", msg.id, content);
                }
            }
        }
    }
    output
}

/// Detach oldest conversation messages into frozen ConversationHistory panels
/// when the non-detached conversation exceeds TOKEN_DETACH_COUNT tokens.
pub fn detach_conversation_chunks(state: &mut State) {
    loop {
        // 1. Count tokens of non-Detached, non-Deleted messages
        let active_tokens: usize = state.messages.iter()
            .filter(|m| m.status != MessageStatus::Deleted && m.status != MessageStatus::Detached)
            .map(|m| match m.status {
                MessageStatus::Summarized => {
                    m.tl_dr_token_count.max(estimate_tokens(m.tl_dr.as_deref().unwrap_or("")))
                }
                _ => m.content_token_count.max(estimate_tokens(&m.content)),
            })
            .sum();

        // 2. If total <= threshold, nothing to detach
        if active_tokens <= TOKEN_DETACH_COUNT {
            break;
        }

        // 3. Walk messages from oldest, accumulate tokens until >= half threshold
        let half = TOKEN_DETACH_COUNT / 2;
        let mut accumulated = 0usize;
        let mut boundary = None;

        for (idx, msg) in state.messages.iter().enumerate() {
            if msg.status == MessageStatus::Deleted || msg.status == MessageStatus::Detached {
                continue;
            }

            let tokens = match msg.status {
                MessageStatus::Summarized => {
                    msg.tl_dr_token_count.max(estimate_tokens(msg.tl_dr.as_deref().unwrap_or("")))
                }
                _ => msg.content_token_count.max(estimate_tokens(&msg.content)),
            };
            accumulated += tokens;

            // Once we've accumulated enough, find a turn boundary at or after this point
            if accumulated >= half && is_turn_boundary(&state.messages, idx) {
                boundary = Some(idx + 1); // exclusive end
                break;
            }
        }

        let boundary = match boundary {
            Some(b) if b > 0 => b,
            _ => break, // No valid boundary found, bail
        };

        // 4. Collect message IDs for the chunk name
        let first_id = state.messages[..boundary].iter()
            .find(|m| m.status != MessageStatus::Deleted && m.status != MessageStatus::Detached)
            .map(|m| m.id.clone())
            .unwrap_or_default();
        let last_id = state.messages[..boundary].iter().rev()
            .find(|m| m.status != MessageStatus::Deleted && m.status != MessageStatus::Detached)
            .map(|m| m.id.clone())
            .unwrap_or_default();

        // 5. Format chunk content
        let content = format_chunk_content(&state.messages, 0, boundary);
        if content.is_empty() {
            break; // Nothing useful to detach
        }

        // 6. Get timestamp from first active message (for sort ordering — oldest first)
        let chunk_timestamp = state.messages[..boundary].iter()
            .find(|m| m.status != MessageStatus::Deleted && m.status != MessageStatus::Detached)
            .map(|m| m.timestamp_ms)
            .unwrap_or_else(now_ms);

        // 7. Create the ConversationHistory panel
        let panel_id = state.next_available_context_id();
        let token_count = estimate_tokens(&content);
        let total_pages = compute_total_pages(token_count);
        let chunk_name = format!("Chat [{}–{}]", first_id, last_id);

        state.context.push(ContextElement {
            id: panel_id,
            uid: None,
            context_type: ContextType::ConversationHistory,
            name: chunk_name,
            token_count,
            file_path: None,
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
            result_command: None,
            result_command_hash: None,
            cached_content: Some(content),
            cache_deprecated: false,
            last_refresh_ms: chunk_timestamp,
            tmux_last_lines_hash: None,
            current_page: 0,
            total_pages,
            full_token_count: token_count,
        });

        // 8. Remove detached messages from state and disk
        let removed: Vec<Message> = state.messages.drain(..boundary).collect();
        for msg in &removed {
            if let Some(uid) = &msg.uid {
                crate::persistence::delete_message(uid);
            }
        }

        // Loop to check if remaining messages still exceed threshold
    }
}
