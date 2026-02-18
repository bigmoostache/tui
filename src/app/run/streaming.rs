use std::sync::mpsc::{Receiver, Sender};

use crate::app::actions::{Action, ActionResult, apply_action};
use crate::app::background::{TlDrResult, generate_tldr};
use crate::infra::api::{StreamEvent, StreamParams, start_streaming};
use crate::infra::constants::{DEFAULT_WORKER_ID, MAX_API_RETRIES};
use crate::state::persistence::build_message_op;
use crate::state::{Message, MessageStatus, MessageType};
use crate::state::message::{ToolUseRecord, ToolResultRecord};
use crate::app::panels::now_ms;

use crate::app::App;
use crate::app::context::{get_active_agent_content, prepare_stream_context};

/// Check if the error message indicates a "prompt is too long" error
fn is_prompt_too_long_error(error_msg: &str) -> bool {
    error_msg.contains("prompt is too long") || error_msg.contains("prompt_is_too_long")
}

/// Find and close the oldest closable panel (non-fixed panel like CONVERSATION_HISTORY)
/// Returns the panel ID if one was closed, None otherwise
fn try_close_oldest_panel(state: &mut crate::state::State) -> Option<String> {
    // Find all non-fixed panels (closable panels)
    let closable_panels: Vec<(usize, String, String)> = state
        .context
        .iter()
        .enumerate()
        .filter(|(_, ctx)| !ctx.context_type.is_fixed())
        .map(|(idx, ctx)| (idx, ctx.id.clone(), ctx.name.clone()))
        .collect();

    if closable_panels.is_empty() {
        return None;
    }

    // Close the first (oldest) closable panel
    let (idx, panel_id, panel_name) = closable_panels[0].clone();
    state.context.remove(idx);
    state.dirty = true;

    Some(format!("{} ({})", panel_id, panel_name))
}

/// Create fake tool call and result messages to inform the LLM about the panel closure
fn create_panel_closure_messages(
    state: &mut crate::state::State,
    closed_panel_desc: &str,
) -> (Message, Message) {
    // Generate tool call message
    let tool_id = format!("T{}", state.next_tool_id);
    state.next_tool_id += 1;
    let tool_uid = format!("UID_{}_T", state.global_next_uid);
    state.global_next_uid += 1;

    let tool_msg = Message {
        id: tool_id.clone(),
        uid: Some(tool_uid),
        role: "assistant".to_string(),
        message_type: MessageType::ToolCall,
        content: String::new(),
        content_token_count: 0,
        tl_dr: None,
        tl_dr_token_count: 0,
        status: MessageStatus::Full,
        tool_uses: vec![ToolUseRecord {
            id: tool_id.clone(),
            name: "context_close".to_string(),
            input: serde_json::json!({
                "ids": [closed_panel_desc.split_whitespace().next().unwrap_or("")]
            }),
        }],
        tool_results: Vec::new(),
        input_tokens: 0,
        timestamp_ms: now_ms(),
    };

    // Generate tool result message
    let result_id = format!("R{}", state.next_result_id);
    state.next_result_id += 1;
    let result_uid = format!("UID_{}_R", state.global_next_uid);
    state.global_next_uid += 1;

    let result_content = format!(
        "Automatically closed panel due to context length limit:\n\nClosed 1:\n{}",
        closed_panel_desc
    );

    let result_msg = Message {
        id: result_id.clone(),
        uid: Some(result_uid),
        role: "user".to_string(),
        message_type: MessageType::ToolResult,
        content: String::new(),
        content_token_count: 0,
        tl_dr: None,
        tl_dr_token_count: 0,
        status: MessageStatus::Full,
        tool_uses: Vec::new(),
        tool_results: vec![ToolResultRecord {
            tool_use_id: tool_id.clone(),
            content: result_content,
            is_error: false,
            tool_name: "context_close".to_string(),
        }],
        input_tokens: 0,
        timestamp_ms: now_ms(),
    };

    (tool_msg, result_msg)
}

impl App {
    pub(super) fn process_stream_events(&mut self, rx: &Receiver<StreamEvent>) {
        let _guard = crate::profile!("app::stream_events");
        while let Ok(evt) = rx.try_recv() {
            if !self.state.is_streaming {
                continue;
            }
            self.state.dirty = true;
            match evt {
                StreamEvent::Chunk(text) => {
                    self.typewriter.add_chunk(&text);
                }
                StreamEvent::ToolUse(tool) => {
                    self.pending_tools.push(tool);
                }
                StreamEvent::Done { input_tokens, output_tokens, cache_hit_tokens, cache_miss_tokens, stop_reason } => {
                    self.typewriter.mark_done();
                    self.pending_done =
                        Some((input_tokens, output_tokens, cache_hit_tokens, cache_miss_tokens, stop_reason));
                }
                StreamEvent::Error(e) => {
                    self.typewriter.reset();
                    
                    // Check if this is a "prompt is too long" error
                    let is_too_long = is_prompt_too_long_error(&e);
                    
                    if is_too_long {
                        // Try to close the oldest closable panel
                        if let Some(closed_panel_desc) = try_close_oldest_panel(&mut self.state) {
                            
                            // Create fake tool call and result messages to inform the LLM
                            let (tool_msg, result_msg) = create_panel_closure_messages(&mut self.state, &closed_panel_desc);
                            
                            // Add the messages to the conversation
                            self.state.messages.push(tool_msg);
                            self.state.messages.push(result_msg);
                            
                            // Log the panel closure
                            let log_msg = format!(
                                "Prompt too long error detected. Automatically closed panel: {}\n\
                                 Will retry without incrementing retry count.\n",
                                closed_panel_desc
                            );
                            crate::state::persistence::log_error(&log_msg);
                            
                            // Set retry error without incrementing retry count
                            self.pending_retry_error = Some(e);
                            self.state.dirty = true;
                            return; // Skip normal error handling
                        }
                    }
                    
                    // Normal error handling (not "prompt too long" or no panel to close)
                    let attempt = self.state.api_retry_count + 1;
                    let will_retry = attempt <= MAX_API_RETRIES;
                    let provider = format!("{:?}", self.state.llm_provider);
                    let model = self.state.current_model();
                    let log_msg = format!(
                        "Attempt {}/{} ({})\n\
                         Provider: {} | Model: {}\n\
                         Last request dump: .context-pilot/last_requests/\n\n\
                         {}\n",
                        attempt,
                        MAX_API_RETRIES + 1,
                        if will_retry { "will retry" } else { "giving up" },
                        provider,
                        model,
                        e
                    );
                    crate::state::persistence::log_error(&log_msg);

                    // Check if we should retry
                    if will_retry {
                        self.state.api_retry_count += 1;
                        self.pending_retry_error = Some(e);
                    } else {
                        // Max retries reached, show error
                        self.state.api_retry_count = 0;
                        apply_action(&mut self.state, Action::StreamError(e));
                    }
                }
            }
        }
    }

    pub(super) fn handle_retry(&mut self, tx: &Sender<StreamEvent>) {
        if let Some(_error) = self.pending_retry_error.take() {
            // Still streaming, retry the request
            if self.state.is_streaming {
                // Clear any partial assistant message content before retrying
                if let Some(msg) = self.state.messages.last_mut()
                    && msg.role == "assistant"
                {
                    msg.content.clear();
                }
                let ctx = prepare_stream_context(&mut self.state, true);
                let system_prompt = get_active_agent_content(&self.state);
                self.typewriter.reset();
                self.pending_done = None;
                start_streaming(
                    StreamParams {
                        provider: self.state.llm_provider,
                        model: self.state.current_model(),
                        messages: ctx.messages,
                        context_items: ctx.context_items,
                        tools: ctx.tools,
                        system_prompt: system_prompt.clone(),
                        seed_content: Some(system_prompt),
                        worker_id: DEFAULT_WORKER_ID.to_string(),
                    },
                    tx.clone(),
                );
                self.state.dirty = true;
            }
        }
    }

    pub(super) fn process_typewriter(&mut self) {
        let _guard = crate::profile!("app::typewriter");
        if self.state.is_streaming
            && let Some(chars) = self.typewriter.take_chars()
        {
            apply_action(&mut self.state, Action::AppendChars(chars));
            self.state.dirty = true;
        }
    }

    pub(super) fn process_tldr_results(&mut self, tldr_rx: &Receiver<TlDrResult>) {
        while let Ok(tldr) = tldr_rx.try_recv() {
            {
                let ts = cp_mod_tree::TreeState::get_mut(&mut self.state);
                ts.pending_tldrs = ts.pending_tldrs.saturating_sub(1);
            }
            self.state.dirty = true;
            if let Some(msg) = self.state.messages.iter_mut().find(|m| m.id == tldr.message_id) {
                msg.tl_dr = Some(tldr.tl_dr);
                msg.tl_dr_token_count = tldr.token_count;
                let op = build_message_op(msg);
                self.writer.send_message(op);
            }
        }
    }

    pub(super) fn process_api_check_results(&mut self) {
        if let Some(rx) = &self.api_check_rx
            && let Ok(result) = rx.try_recv()
        {
            self.state.api_check_in_progress = false;
            self.state.api_check_result = Some(result);
            self.state.dirty = true;
            self.api_check_rx = None;
            self.save_state_async();
        }
    }

    /// Continue streaming after tool execution (called when panels are ready).
    pub(super) fn continue_streaming(&mut self, tx: &Sender<StreamEvent>) {
        let ctx = prepare_stream_context(&mut self.state, true);
        let system_prompt = get_active_agent_content(&self.state);
        self.typewriter.reset();
        self.pending_done = None;
        start_streaming(
            StreamParams {
                provider: self.state.llm_provider,
                model: self.state.current_model(),
                messages: ctx.messages,
                context_items: ctx.context_items,
                tools: ctx.tools,
                system_prompt: system_prompt.clone(),
                seed_content: Some(system_prompt),
                worker_id: DEFAULT_WORKER_ID.to_string(),
            },
            tx.clone(),
        );
    }

    pub(super) fn finalize_stream(&mut self, tldr_tx: &Sender<TlDrResult>) {
        if !self.state.is_streaming {
            return;
        }
        // Don't finalize while waiting for panels or deferred sleep —
        // pending_done is still Some from the intermediate stream, and
        // continue_streaming will clear it when the deferred state resolves.
        if self.state.waiting_for_panels || self.deferred_tool_sleeping {
            return;
        }
        // Don't finalize while a question form is pending user response
        if self.pending_question_tool_results.is_some() {
            return;
        }
        // Don't finalize while a console blocking wait is pending
        if self.pending_console_wait_tool_results.is_some() {
            return;
        }

        if let Some((input_tokens, output_tokens, cache_hit_tokens, cache_miss_tokens, ref stop_reason)) =
            self.pending_done
            && self.typewriter.pending_chars.is_empty()
            && self.pending_tools.is_empty()
        {
            self.state.dirty = true;
            let stop_reason = stop_reason.clone();
            match apply_action(
                &mut self.state,
                Action::StreamDone {
                    _input_tokens: input_tokens,
                    output_tokens,
                    cache_hit_tokens,
                    cache_miss_tokens,
                    stop_reason,
                },
            ) {
                ActionResult::SaveMessage(id) => {
                    let tldr_info = self.state.messages.iter().find(|m| m.id == id).and_then(|msg| {
                        self.save_message_async(msg);
                        if msg.role == "assistant" && msg.tl_dr.is_none() && !msg.content.is_empty() {
                            Some((msg.id.clone(), msg.content.clone()))
                        } else {
                            None
                        }
                    });
                    if let Some((msg_id, content)) = tldr_info {
                        cp_mod_tree::TreeState::get_mut(&mut self.state).pending_tldrs += 1;
                        generate_tldr(msg_id, content, tldr_tx.clone());
                    }
                    self.save_state_async();
                }
                ActionResult::Save => self.save_state_async(),
                _ => {}
            }
            // Reset retry count on successful completion
            self.state.api_retry_count = 0;
            // Reset auto-continuation count on each successful tick (stream completion).
            // This means MaxAutoRetries only fires on consecutive *failed* continuations,
            // not on total auto-continuations in an autonomous session.
            cp_mod_spine::SpineState::get_mut(&mut self.state).config.auto_continuation_count = 0;
            self.typewriter.reset();
            self.pending_done = None;
        }
    }
}
