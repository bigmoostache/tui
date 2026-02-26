//! Reverie event processing — polls the reverie stream and dispatches tools.

use std::sync::mpsc;

use crate::app::App;
use crate::app::reverie::{streaming, tools};
use crate::infra::api::StreamEvent;
use crate::state::persistence::save_state;

impl App {
    /// Check if a reverie needs to be started (state has reverie but no stream).
    /// Called from the main event loop.
    pub(super) fn maybe_start_reverie_stream(&mut self) {
        // Guard: reverie active in state but no stream running yet → start one
        let needs_start = self.state.reverie.as_ref().is_some_and(|r| r.is_streaming) && self.reverie_stream.is_none();

        if !needs_start {
            return;
        }

        let (tx, rx) = mpsc::channel();
        streaming::start_reverie_stream(&mut self.state, tx);
        self.reverie_stream = Some(super::super::ReverieStream { rx, pending_tools: Vec::new(), report_called: false });
    }

    /// Poll the reverie stream for events and process them.
    /// Called from the main event loop, AFTER main stream events.
    pub(super) fn process_reverie_events(&mut self) {
        // Drain all events from the reverie stream
        let events: Vec<StreamEvent> = match self.reverie_stream.as_ref() {
            Some(s) => s.rx.try_iter().collect(),
            None => return,
        };

        for evt in events {
            self.state.dirty = true;
            match evt {
                StreamEvent::Chunk(text) => {
                    // Append text to the reverie's own messages
                    if let Some(rev) = self.state.reverie.as_mut() {
                        if rev.messages.last().is_none_or(|m| m.role != "assistant") {
                            rev.messages.push(crate::state::Message {
                                id: format!("rev-{}", rev.messages.len()),
                                uid: None,
                                role: "assistant".to_string(),
                                content: String::new(),
                                message_type: crate::state::MessageType::TextMessage,
                                status: crate::state::MessageStatus::Full,
                                content_token_count: 0,
                                input_tokens: 0,
                                timestamp_ms: crate::app::panels::now_ms(),
                                tool_uses: Vec::new(),
                                tool_results: Vec::new(),
                            });
                        }
                        if let Some(msg) = rev.messages.last_mut() {
                            msg.content.push_str(&text);
                        }
                    }
                }
                StreamEvent::ToolUse(tool) => {
                    // Queue the tool for dispatch
                    if let Some(stream) = self.reverie_stream.as_mut() {
                        stream.pending_tools.push(tool);
                    }
                }
                StreamEvent::Done {
                    input_tokens: _,
                    output_tokens: _,
                    cache_hit_tokens: _,
                    cache_miss_tokens: _,
                    stop_reason: _,
                } => {
                    // Mark assistant message as complete
                    if let Some(rev) = self.state.reverie.as_mut() {
                        if let Some(msg) = rev.messages.last_mut() {
                            msg.status = crate::state::MessageStatus::Full;
                        }
                        rev.is_streaming = false;
                    }
                }
                StreamEvent::Error(e) => {
                    // Reverie errors are non-critical — just log and destroy
                    cp_mod_spine::SpineState::create_notification(
                        &mut self.state,
                        cp_mod_spine::NotificationType::Custom,
                        "Reverie".to_string(),
                        format!("Reverie error: {}. Destroying session.", e),
                    );
                    self.state.reverie = None;
                    self.reverie_stream = None;
                    return;
                }
            }
        }
    }

    /// Execute pending reverie tool calls.
    /// Called from the main event loop, AFTER main tools are processed.
    pub(super) fn handle_reverie_tools(&mut self) {
        // Take pending tools from the stream state
        let pending = match self.reverie_stream.as_mut() {
            Some(s) if !s.pending_tools.is_empty() => std::mem::take(&mut s.pending_tools),
            _ => return,
        };

        let mut tool_results = Vec::new();

        for tool in &pending {
            // Increment tool call count
            if let Some(rev) = self.state.reverie.as_mut() {
                rev.tool_call_count += 1;
            }

            // Check tool cap (Phase 8 guard rail)
            let cap = crate::infra::constants::REVERIE_TOOL_CAP;
            if self.state.reverie.as_ref().is_some_and(|r| r.tool_call_count > cap) {
                // Force-stop: create a Report result and destroy
                cp_mod_spine::SpineState::create_notification(
                    &mut self.state,
                    cp_mod_spine::NotificationType::Custom,
                    "Reverie".to_string(),
                    format!("Tool cap ({}) reached. Force-stopping reverie.", cap),
                );
                self.state.reverie = None;
                self.reverie_stream = None;
                return;
            }

            // Dispatch through reverie tool router
            let result = match tools::dispatch_reverie_tool(tool, &mut self.state) {
                Some(result) => {
                    // Check for Report sentinel
                    if result.content.starts_with("REVERIE_REPORT:") {
                        let summary = result.content.strip_prefix("REVERIE_REPORT:").unwrap_or("Completed");
                        cp_mod_spine::SpineState::create_notification(
                            &mut self.state,
                            cp_mod_spine::NotificationType::Custom,
                            "Reverie".to_string(),
                            summary.to_string(),
                        );
                        if let Some(stream) = self.reverie_stream.as_mut() {
                            stream.report_called = true;
                        }
                        // Destroy the reverie
                        self.state.reverie = None;
                        self.reverie_stream = None;
                        save_state(&self.state);
                        return;
                    }
                    result
                }
                None => {
                    // Delegate to normal module dispatch
                    let active = self.state.active_modules.clone();
                    crate::modules::dispatch_tool(tool, &mut self.state, &active)
                }
            };

            // Record tool use + result in reverie messages
            if let Some(rev) = self.state.reverie.as_mut() {
                // Add tool_use record to last assistant message
                if let Some(msg) = rev.messages.last_mut() {
                    msg.tool_uses.push(crate::state::message::ToolUseRecord {
                        id: tool.id.clone(),
                        name: tool.name.clone(),
                        input: tool.input.clone(),
                    });
                }
                // Add tool result as a new message
                rev.messages.push(crate::state::Message {
                    id: format!("rev-tr-{}", rev.messages.len()),
                    uid: None,
                    role: "user".to_string(),
                    content: String::new(),
                    message_type: crate::state::MessageType::TextMessage,
                    status: crate::state::MessageStatus::Full,
                    content_token_count: 0,
                    input_tokens: 0,
                    timestamp_ms: crate::app::panels::now_ms(),
                    tool_uses: Vec::new(),
                    tool_results: vec![crate::state::message::ToolResultRecord {
                        tool_use_id: result.tool_use_id.clone(),
                        tool_name: result.tool_name.clone(),
                        content: result.content.clone(),
                        is_error: result.is_error,
                    }],
                });
            }
            tool_results.push(result);
        }

        // If we have tool results and the reverie is still alive, re-stream
        if !tool_results.is_empty() && self.state.reverie.is_some() {
            // Start a new reverie stream with the updated conversation
            if let Some(rev) = self.state.reverie.as_mut() {
                rev.is_streaming = true;
            }
            let (tx, rx) = mpsc::channel();
            streaming::start_reverie_stream(&mut self.state, tx);
            self.reverie_stream =
                Some(super::super::ReverieStream { rx, pending_tools: Vec::new(), report_called: false });
        }
    }

    /// Check if the reverie ended without calling Report.
    /// If so, auto-relaunch with a directive to call Report.
    pub(super) fn check_reverie_end_turn(&mut self) {
        let rev = match self.state.reverie.as_ref() {
            Some(r) if !r.is_streaming => r,
            _ => return,
        };

        let report_called = self.reverie_stream.as_ref().is_some_and(|s| s.report_called);

        if report_called {
            return; // All good — Report was called
        }

        // End turn without Report — check retry limit
        let retries = rev.report_retries;
        if retries >= 1 {
            // Max retries reached — force destroy
            cp_mod_spine::SpineState::create_notification(
                &mut self.state,
                cp_mod_spine::NotificationType::Custom,
                "Reverie".to_string(),
                "Reverie ended without calling Report after retry. Force-destroying.".to_string(),
            );
            self.state.reverie = None;
            self.reverie_stream = None;
            return;
        }

        // Auto-relaunch with Report directive
        if let Some(rev) = self.state.reverie.as_mut() {
            rev.report_retries += 1;
            rev.is_streaming = true;
            rev.directive = Some(
                "You MUST call the Report tool to summarize your work and complete your run. \
                 Do it now."
                    .to_string(),
            );
        }

        let (tx, rx) = mpsc::channel();
        streaming::start_reverie_stream(&mut self.state, tx);
        self.reverie_stream = Some(super::super::ReverieStream { rx, pending_tools: Vec::new(), report_called: false });
    }
}
