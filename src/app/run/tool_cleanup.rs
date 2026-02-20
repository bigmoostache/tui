use std::sync::mpsc::Sender;

use crate::app::panels::now_ms;
use crate::infra::api::StreamEvent;
use crate::state::{Message, MessageStatus, MessageType, ToolResultRecord};

use cp_base::watchers::WatcherRegistry;
use cp_mod_console::CONSOLE_WAIT_BLOCKING_SENTINEL;
use cp_mod_spine::{NotificationType, SpineState};

use crate::app::App;

impl App {
    /// Non-blocking check: poll WatcherRegistry for satisfied conditions.
    /// - Blocking watchers: replace sentinel tool results and resume pipeline.
    /// - Async watchers: create spine notifications.
    pub(super) fn check_watchers(&mut self, tx: &Sender<StreamEvent>) {
        // Take the registry out of state to avoid borrow conflict
        // (poll_all needs &mut registry + &state simultaneously)
        let mut registry = match self.state.module_data.remove(&std::any::TypeId::of::<WatcherRegistry>()) {
            Some(boxed) => match boxed.downcast::<WatcherRegistry>() {
                Ok(r) => *r,
                Err(boxed) => {
                    self.state.module_data.insert(std::any::TypeId::of::<WatcherRegistry>(), boxed);
                    return;
                }
            },
            None => return,
        };

        let (blocking_results, async_results) = registry.poll_all(&self.state);

        // Put registry back
        self.state.set_ext(registry);

        // --- Async completions → spine notifications ---
        if !async_results.is_empty() {
            for result in &async_results {
                SpineState::create_notification(
                    &mut self.state,
                    NotificationType::Custom,
                    "watcher".to_string(),
                    result.description.clone(),
                );
            }

            // Auto-close panels for async watchers that request it (e.g. callback success)
            for result in &async_results {
                if result.close_panel {
                    if let Some(ref panel_id) = result.panel_id {
                        // Kill the console session first
                        if let Some(ctx) = self.state.context.iter().find(|c| c.id == *panel_id) {
                            if let Some(name) = ctx.get_meta::<String>("console_name") {
                                cp_mod_console::types::ConsoleState::kill_session(&mut self.state, &name);
                            }
                        }
                        self.state.context.retain(|c| c.id != *panel_id);
                    }
                }
            }

            self.save_state_async();
        }

        // --- Blocking sentinel replacement ---
        if self.pending_console_wait_tool_results.is_none() || blocking_results.is_empty() {
            return;
        }

        let mut tool_results = self.pending_console_wait_tool_results.take().unwrap();

        // Replace sentinels with real results
        for tr in &mut tool_results {
            if tr.content == CONSOLE_WAIT_BLOCKING_SENTINEL {
                if let Some(result) = blocking_results.iter().find(|r| {
                    r.tool_use_id.as_deref() == Some(&tr.tool_use_id)
                }) {
                    tr.content = result.description.clone();
                }
            }
        }

        // Check if any sentinels remain unresolved (multiple blocking waits in one batch)
        let still_pending = tool_results.iter().any(|r| r.content == CONSOLE_WAIT_BLOCKING_SENTINEL);
        if still_pending {
            self.pending_console_wait_tool_results = Some(tool_results);
            return;
        }

        // All resolved — resume normal pipeline: create result message + continue streaming
        let result_id = format!("R{}", self.state.next_result_id);
        let result_uid = format!("UID_{}_R", self.state.global_next_uid);
        self.state.next_result_id += 1;
        self.state.global_next_uid += 1;
        let tool_result_records: Vec<ToolResultRecord> = tool_results
            .iter()
            .map(|r| ToolResultRecord {
                tool_use_id: r.tool_use_id.clone(),
                content: r.content.clone(),
                is_error: r.is_error,
                tool_name: r.tool_name.clone(),
            })
            .collect();
        let result_msg = Message {
            id: result_id,
            uid: Some(result_uid),
            role: "user".to_string(),
            message_type: MessageType::ToolResult,
            content: String::new(),
            content_token_count: 0,
            tl_dr: None,
            tl_dr_token_count: 0,
            status: MessageStatus::Full,
            tool_uses: Vec::new(),
            tool_results: tool_result_records,
            input_tokens: 0,
            timestamp_ms: crate::app::panels::now_ms(),
        };
        self.save_message_async(&result_msg);
        self.state.messages.push(result_msg);

        if self.state.reload_pending {
            crate::infra::tools::perform_reload(&mut self.state);
        }

        // Create new assistant message for continued streaming
        let assistant_id = format!("A{}", self.state.next_assistant_id);
        let assistant_uid = format!("UID_{}_A", self.state.global_next_uid);
        self.state.next_assistant_id += 1;
        self.state.global_next_uid += 1;
        let new_assistant_msg = Message {
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
            timestamp_ms: crate::app::panels::now_ms(),
        };
        self.state.messages.push(new_assistant_msg);

        self.state.streaming_estimated_tokens = 0;

        // Accumulate token stats from intermediate stream
        if let Some((_, output_tokens, cache_hit_tokens, cache_miss_tokens, _)) = self.pending_done {
            self.state.tick_cache_hit_tokens = cache_hit_tokens;
            self.state.tick_cache_miss_tokens = cache_miss_tokens;
            self.state.tick_output_tokens = output_tokens;
            self.state.stream_cache_hit_tokens += cache_hit_tokens;
            self.state.stream_cache_miss_tokens += cache_miss_tokens;
            self.state.stream_output_tokens += output_tokens;
            self.state.cache_hit_tokens += cache_hit_tokens;
            self.state.cache_miss_tokens += cache_miss_tokens;
            self.state.total_output_tokens += output_tokens;
        }

        self.save_state_async();
        self.state.dirty = true;

        super::wait::trigger_dirty_panel_refresh(&self.state, &self.cache_tx);
        if super::wait::has_dirty_file_panels(&self.state) {
            self.state.waiting_for_panels = true;
            self.wait_started_ms = now_ms();
        } else {
            self.continue_streaming(tx);
        }
    }

    /// When the user interrupts streaming (Esc), any pending blocking tool calls
    /// (console_wait, ask_user_question, or tools mid-execution) have their
    /// tool_use messages already saved but no matching tool_result. This creates
    /// orphaned tool_use blocks that cause API 400 errors on the next stream.
    ///
    /// This method creates fake tool_result messages for all pending tools so
    /// every tool_use is properly paired.
    pub(super) fn flush_pending_tool_results_as_interrupted(&mut self) {
        let interrupted_msg = "Tool execution interrupted by user.";

        // Collect all pending tool results from both blocking paths
        let mut all_pending: Vec<crate::infra::tools::ToolResult> = Vec::new();

        if let Some(results) = self.pending_console_wait_tool_results.take() {
            all_pending.extend(results);
        }
        if let Some(results) = self.pending_question_tool_results.take() {
            all_pending.extend(results);
        }

        // Also clean up the question form state if it was pending
        self.state
            .module_data
            .remove(&std::any::TypeId::of::<cp_base::ui::PendingQuestionForm>());

        if all_pending.is_empty() {
            return;
        }

        // Create a tool_result message pairing each pending tool_use
        let result_id = format!("R{}", self.state.next_result_id);
        let result_uid = format!("UID_{}_R", self.state.global_next_uid);
        self.state.next_result_id += 1;
        self.state.global_next_uid += 1;

        let tool_result_records: Vec<ToolResultRecord> = all_pending
            .iter()
            .map(|r| ToolResultRecord {
                tool_use_id: r.tool_use_id.clone(),
                content: interrupted_msg.to_string(),
                is_error: true,
                tool_name: r.tool_name.clone(),
            })
            .collect();

        let result_msg = Message {
            id: result_id,
            uid: Some(result_uid),
            role: "user".to_string(),
            message_type: MessageType::ToolResult,
            content: String::new(),
            content_token_count: 0,
            tl_dr: None,
            tl_dr_token_count: 0,
            status: MessageStatus::Full,
            tool_uses: Vec::new(),
            tool_results: tool_result_records,
            input_tokens: 0,
            timestamp_ms: now_ms(),
        };
        self.save_message_async(&result_msg);
        self.state.messages.push(result_msg);
    }
}
