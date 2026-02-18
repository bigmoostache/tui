use std::io;
use std::sync::mpsc::{Receiver, Sender};
use std::time::Duration;

use crossterm::event;
use ratatui::prelude::*;

use crate::app::actions::{Action, ActionResult, apply_action, clean_llm_id_prefix};
use crate::infra::api::{StreamEvent, StreamParams, start_streaming};
use crate::app::background::{TlDrResult, generate_tldr};
use crate::state::cache::{CacheRequest, CacheUpdate, process_cache_request};
use crate::infra::constants::{DEFAULT_WORKER_ID, EVENT_POLL_MS, MAX_API_RETRIES, RENDER_THROTTLE_MS};
use crate::app::panels::now_ms;
use crate::app::events::handle_event;
use crate::infra::gh_watcher::GhWatcher;
use crate::ui::help::CommandPalette;
use crate::state::persistence::{PersistenceWriter, build_message_op, build_save_batch, check_ownership, save_state};
use crate::state::{ContextType, Message, MessageStatus, MessageType, State, ToolResultRecord, ToolUseRecord};
use crate::infra::tools::{ToolResult, ToolUse, execute_tool, perform_reload};
use crate::ui::typewriter::TypewriterBuffer;
use crate::ui;
use crate::infra::watcher::{FileWatcher, WatchEvent};

use cp_mod_console::CONSOLE_WAIT_BLOCKING_SENTINEL;
use cp_mod_console::types::ConsoleState;

use super::context::{get_active_agent_content, prepare_stream_context};

pub struct App {
    pub state: State,
    typewriter: TypewriterBuffer,
    pending_done: Option<(usize, usize, usize, usize, Option<String>)>,
    pending_tools: Vec<ToolUse>,
    cache_tx: Sender<CacheUpdate>,
    file_watcher: Option<FileWatcher>,
    gh_watcher: GhWatcher,
    /// Tracks which file paths are being watched
    watched_file_paths: std::collections::HashSet<String>,
    /// Tracks which directory paths are being watched
    watched_dir_paths: std::collections::HashSet<String>,
    /// Last time we checked timer-based caches
    last_timer_check_ms: u64,
    /// Last time we checked ownership
    last_ownership_check_ms: u64,
    /// Pending retry error (will retry on next loop iteration)
    pending_retry_error: Option<String>,
    /// Last render time for throttling
    last_render_ms: u64,
    /// Last spinner animation update time
    last_spinner_ms: u64,
    /// Last gh watcher sync time
    last_gh_sync_ms: u64,
    /// Channel for API check results
    api_check_rx: Option<Receiver<crate::llms::ApiCheckResult>>,
    /// Whether to auto-start streaming on first loop iteration
    resume_stream: bool,
    /// Command palette state
    pub command_palette: CommandPalette,
    /// Timestamp (ms) when wait_for_panels started (for timeout)
    wait_started_ms: u64,
    /// Deferred tool results waiting for sleep timer to expire
    deferred_tool_sleep_until_ms: u64,
    /// Whether we're in a deferred sleep state (waiting for timer before continuing tool pipeline)
    deferred_tool_sleeping: bool,
    /// Whether to refresh tmux panels when deferred sleep expires (set by send_keys)
    deferred_sleep_needs_tmux_refresh: bool,
    /// Background persistence writer — offloads file I/O to a dedicated thread
    writer: PersistenceWriter,
    /// Last poll time per panel ID — tracks when we last submitted a cache request
    /// for timer-based panels (Tmux, Git, GitResult, GithubResult, Glob, Grep).
    /// Separate from ContextElement.last_refresh_ms which tracks actual content changes.
    last_poll_ms: std::collections::HashMap<String, u64>,
    /// Pending tool results when a question form is blocking (ask_user_question)
    pending_question_tool_results: Option<Vec<ToolResult>>,
    /// Pending tool results when a console blocking wait is active
    pending_console_wait_tool_results: Option<Vec<ToolResult>>,
}

impl App {
    pub fn new(state: State, cache_tx: Sender<CacheUpdate>, resume_stream: bool) -> Self {
        let file_watcher = FileWatcher::new().ok();
        let gh_watcher = GhWatcher::new(cache_tx.clone());

        Self {
            state,
            typewriter: TypewriterBuffer::new(),
            pending_done: None,
            pending_tools: Vec::new(),
            cache_tx,
            file_watcher,
            gh_watcher,
            watched_file_paths: std::collections::HashSet::new(),
            watched_dir_paths: std::collections::HashSet::new(),
            last_timer_check_ms: now_ms(),
            last_ownership_check_ms: now_ms(),
            pending_retry_error: None,
            last_render_ms: 0,
            last_spinner_ms: 0,
            last_gh_sync_ms: 0,
            api_check_rx: None,
            resume_stream,
            command_palette: CommandPalette::new(),
            wait_started_ms: 0,
            deferred_tool_sleep_until_ms: 0,
            deferred_tool_sleeping: false,
            deferred_sleep_needs_tmux_refresh: false,
            writer: PersistenceWriter::new(),
            last_poll_ms: std::collections::HashMap::new(),
            pending_question_tool_results: None,
            pending_console_wait_tool_results: None,
        }
    }

    pub fn run(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        tx: Sender<StreamEvent>,
        rx: Receiver<StreamEvent>,
        tldr_tx: Sender<TlDrResult>,
        tldr_rx: Receiver<TlDrResult>,
        cache_rx: Receiver<CacheUpdate>,
    ) -> io::Result<()> {
        // Initial cache setup - watch files and schedule initial refreshes
        self.setup_file_watchers();
        self.sync_gh_watches();
        self.schedule_initial_cache_refreshes();

        // Claim ownership immediately
        save_state(&self.state);

        // Auto-resume streaming if flag was set (e.g., after reload_tui)
        if self.resume_stream {
            self.resume_stream = false;
            use cp_mod_spine::{NotificationType, SpineState};
            SpineState::create_notification(
                &mut self.state,
                NotificationType::ReloadResume,
                "reload_resume".to_string(),
                "Resuming after TUI reload".to_string(),
            );
            save_state(&self.state);
        }

        loop {
            let current_ms = now_ms();

            // === INPUT FIRST: Process user input with minimal latency ===
            // Non-blocking check for input - handle immediately for responsive feel
            if event::poll(Duration::ZERO)? {
                let evt = event::read()?;

                // Handle command palette events first if it's open
                if self.command_palette.is_open {
                    if let Some(action) = self.handle_palette_event(&evt) {
                        self.handle_action(action, &tx, &tldr_tx);
                    }
                    self.state.dirty = true;

                    // Render immediately after input for instant feedback
                    if self.state.dirty {
                        terminal.draw(|frame| {
                            ui::render(frame, &mut self.state);
                            self.command_palette.render(frame, &self.state);
                        })?;
                        self.state.dirty = false;
                        self.last_render_ms = current_ms;
                    }
                    continue;
                }

                // Handle question form events if form is active (mutates state directly)
                if let Some(form) = self.state.get_ext::<cp_base::ui::PendingQuestionForm>()
                    && !form.resolved
                {
                    self.handle_question_form_event(&evt);
                    self.state.dirty = true;

                    // Render immediately
                    if self.state.dirty {
                        terminal.draw(|frame| {
                            ui::render(frame, &mut self.state);
                            self.command_palette.render(frame, &self.state);
                        })?;
                        self.state.dirty = false;
                        self.last_render_ms = current_ms;
                    }
                    continue;
                }

                let Some(action) = handle_event(&evt, &self.state) else {
                    // User quit — flush all pending writes and save final state synchronously
                    self.writer.flush();
                    save_state(&self.state);
                    break;
                };

                // Check for Ctrl+P to open palette
                if let Action::OpenCommandPalette = action {
                    self.command_palette.open(&self.state);
                    self.state.dirty = true;
                } else {
                    self.handle_action(action, &tx, &tldr_tx);
                }

                // Render immediately after input for instant feedback
                if self.state.dirty {
                    terminal.draw(|frame| {
                        ui::render(frame, &mut self.state);
                        self.command_palette.render(frame, &self.state);
                    })?;
                    self.state.dirty = false;
                    self.last_render_ms = current_ms;
                }
            }

            // === BACKGROUND PROCESSING ===
            self.process_stream_events(&rx);
            self.handle_retry(&tx);
            self.process_typewriter();
            self.process_tldr_results(&tldr_rx);
            self.process_cache_updates(&cache_rx);
            self.process_watcher_events();
            // Check if we're waiting for panels and they're ready (non-blocking)
            self.check_waiting_for_panels(&tx);
            // Check if deferred sleep timer has expired (non-blocking)
            self.check_deferred_sleep(&tx);
            // Check if a question form has been resolved by the user
            self.check_question_form(&tx);
            // Check console waiters (blocking sentinel replacement + async → spine notifications)
            self.check_console_waiters(&tx);
            // Throttle gh watcher sync to every 5 seconds (mutex lock + iteration)
            if current_ms.saturating_sub(self.last_gh_sync_ms) >= 5_000 {
                self.last_gh_sync_ms = current_ms;
                self.sync_gh_watches();
            }
            self.check_timer_based_deprecation();
            self.handle_tool_execution(&tx, &tldr_tx);
            self.finalize_stream(&tldr_tx);
            self.check_spine(&tx, &tldr_tx);
            self.process_api_check_results();

            // Check ownership periodically (every 1 second)
            if current_ms.saturating_sub(self.last_ownership_check_ms) >= 1000 {
                self.last_ownership_check_ms = current_ms;
                if !check_ownership() {
                    // Another instance took over - exit gracefully
                    break;
                }
            }

            // Update spinner animation if there's active loading/streaming
            self.update_spinner_animation();

            // Render if dirty and enough time has passed (capped at ~28fps)
            if self.state.dirty && current_ms.saturating_sub(self.last_render_ms) >= RENDER_THROTTLE_MS {
                terminal.draw(|frame| {
                    ui::render(frame, &mut self.state);
                    self.command_palette.render(frame, &self.state);
                })?;
                self.state.dirty = false;
                self.last_render_ms = current_ms;
            }

            // Adaptive poll: sleep longer when idle, shorter when actively streaming
            let poll_ms = if self.state.is_streaming || self.state.dirty {
                EVENT_POLL_MS // 8ms — responsive during streaming/active updates
            } else {
                50 // 50ms when idle — still responsive for typing, much less CPU
            };
            let _ = event::poll(Duration::from_millis(poll_ms))?;
        }

        Ok(())
    }

    fn process_stream_events(&mut self, rx: &Receiver<StreamEvent>) {
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
                    // Log every error to disk for debugging
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

    fn handle_retry(&mut self, tx: &Sender<StreamEvent>) {
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

    fn process_typewriter(&mut self) {
        let _guard = crate::profile!("app::typewriter");
        if self.state.is_streaming
            && let Some(chars) = self.typewriter.take_chars()
        {
            apply_action(&mut self.state, Action::AppendChars(chars));
            self.state.dirty = true;
        }
    }

    fn process_tldr_results(&mut self, tldr_rx: &Receiver<TlDrResult>) {
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

    fn process_api_check_results(&mut self) {
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

    fn handle_tool_execution(&mut self, tx: &Sender<StreamEvent>, tldr_tx: &Sender<TlDrResult>) {
        if !self.state.is_streaming
            || self.pending_done.is_none()
            || !self.typewriter.pending_chars.is_empty()
            || self.pending_tools.is_empty()
        {
            return;
        }
        // Don't process new tools while waiting for panels or deferred sleep
        if self.state.waiting_for_panels || self.deferred_tool_sleeping {
            return;
        }
        // Don't process tools while a question form is pending user response
        if self.state.get_ext::<cp_base::ui::PendingQuestionForm>().is_some() {
            return;
        }
        let _guard = crate::profile!("app::tool_exec");

        self.state.dirty = true;
        let tools = std::mem::take(&mut self.pending_tools);
        let mut tool_results: Vec<ToolResult> = Vec::new();

        // Finalize current assistant message
        let mut needs_tldr: Option<(String, String)> = None;
        if let Some(msg) = self.state.messages.last_mut()
            && msg.role == "assistant"
        {
            // Clean any LLM ID prefixes before saving
            msg.content = clean_llm_id_prefix(&msg.content);
            let op = build_message_op(msg);
            self.writer.send_message(op);
            if !msg.content.trim().is_empty() && msg.tl_dr.is_none() {
                needs_tldr = Some((msg.id.clone(), msg.content.clone()));
            }
        }
        if let Some((id, content)) = needs_tldr {
            cp_mod_tree::TreeState::get_mut(&mut self.state).pending_tldrs += 1;
            generate_tldr(id, content, tldr_tx.clone());
        }

        // Create tool call messages
        for tool in &tools {
            let tool_id = format!("T{}", self.state.next_tool_id);
            let tool_uid = format!("UID_{}_T", self.state.global_next_uid);
            self.state.next_tool_id += 1;
            self.state.global_next_uid += 1;

            let tool_msg = Message {
                id: tool_id,
                uid: Some(tool_uid),
                role: "assistant".to_string(),
                message_type: MessageType::ToolCall,
                content: String::new(),
                content_token_count: 0,
                tl_dr: None,
                tl_dr_token_count: 0,
                status: MessageStatus::Full,
                tool_uses: vec![ToolUseRecord {
                    id: tool.id.clone(),
                    name: tool.name.clone(),
                    input: tool.input.clone(),
                }],
                tool_results: Vec::new(),
                input_tokens: 0,
                timestamp_ms: crate::app::panels::now_ms(),
            };
            self.save_message_async(&tool_msg);
            self.state.messages.push(tool_msg);

            let result = execute_tool(tool, &mut self.state);
            tool_results.push(result);
        }

        // Check if any tool triggered a question form (blocking)
        let has_pending_question = tool_results.iter().any(|r| r.content == "__QUESTION_PENDING__");
        if has_pending_question {
            // Don't create result message or continue streaming yet.
            // The form is active — when user submits/dismisses, check_question_form()
            // will replace the placeholder and resume the pipeline.
            // Store the pending tool results for later resolution.
            self.pending_question_tool_results = Some(tool_results);
            self.save_state_async();
            return;
        }

        // Check if any tool triggered a console blocking wait
        let has_console_wait =
            tool_results.iter().any(|r| r.content == CONSOLE_WAIT_BLOCKING_SENTINEL);
        if has_console_wait {
            self.pending_console_wait_tool_results = Some(tool_results);
            self.save_state_async();
            return;
        }

        // Create tool result message
        let result_id = format!("R{}", self.state.next_result_id);
        let result_uid = format!("UID_{}_R", self.state.global_next_uid);
        self.state.next_result_id += 1;
        self.state.global_next_uid += 1;
        let tool_result_records: Vec<ToolResultRecord> = tool_results
            .iter()
            .zip(tools.iter())
            .map(|(r, t)| ToolResultRecord {
                tool_use_id: r.tool_use_id.clone(),
                content: r.content.clone(),
                is_error: r.is_error,
                tool_name: t.name.clone(),
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

        // Check if reload was requested - perform it after tool result is saved
        if self.state.reload_pending {
            perform_reload(&mut self.state);
            // Note: perform_reload calls std::process::exit(0), so we won't reach here
        }

        // Create new assistant message
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

        // Accumulate token stats from intermediate stream before discarding pending_done
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

        // Check if any tool requested a sleep (e.g., console_sleep)
        if self.state.tool_sleep_until_ms > 0 {
            // Defer everything — main loop will check timer and continue
            self.deferred_tool_sleeping = true;
            self.deferred_tool_sleep_until_ms = self.state.tool_sleep_until_ms;
            self.deferred_sleep_needs_tmux_refresh = self.state.tool_sleep_needs_tmux_refresh;
            self.state.tool_sleep_until_ms = 0; // Clear from state (App owns it now)
            self.state.tool_sleep_needs_tmux_refresh = false;
            return;
        }

        // Trigger background cache refresh for dirty file panels (non-blocking)
        super::wait::trigger_dirty_panel_refresh(&self.state, &self.cache_tx);

        // Check if we need to wait for panels before continuing stream
        if super::wait::has_dirty_file_panels(&self.state) {
            // Set waiting flag — main loop will check and continue streaming when ready
            self.state.waiting_for_panels = true;
            self.wait_started_ms = now_ms();
        } else {
            // No dirty panels — continue streaming immediately
            self.continue_streaming(tx);
        }
    }

    /// Continue streaming after tool execution (called when panels are ready).
    fn continue_streaming(&mut self, tx: &Sender<StreamEvent>) {
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

    /// Non-blocking check: if we're waiting for file panels to load,
    /// check if they're ready (or timed out) and continue streaming.
    fn check_waiting_for_panels(&mut self, tx: &Sender<StreamEvent>) {
        if !self.state.waiting_for_panels {
            return;
        }

        let panels_ready = !super::wait::has_dirty_panels(&self.state);
        let timed_out = now_ms().saturating_sub(self.wait_started_ms) >= 5_000;

        if panels_ready || timed_out {
            self.state.waiting_for_panels = false;
            self.state.dirty = true;
            self.continue_streaming(tx);
        }
    }

    /// Non-blocking check: if a tool requested a sleep (e.g., console_sleep),
    /// wait for the timer to expire, then deprecate tmux panels and continue
    /// through the normal wait_for_panels → continue_streaming pipeline.
    fn check_deferred_sleep(&mut self, tx: &Sender<StreamEvent>) {
        if !self.deferred_tool_sleeping {
            return;
        }

        if now_ms() < self.deferred_tool_sleep_until_ms {
            return; // Still sleeping — keep processing input normally
        }

        let needs_tmux = self.deferred_sleep_needs_tmux_refresh;
        self.deferred_tool_sleeping = false;
        self.deferred_tool_sleep_until_ms = 0;
        self.deferred_sleep_needs_tmux_refresh = false;
        self.state.dirty = true;

        if needs_tmux {
            // send_keys: deprecate tmux panels and wait for refresh
            crate::app::panels::mark_panels_dirty(&mut self.state, ContextType::new(ContextType::TMUX));
            // Trigger tmux panel refreshes
            for ctx in &self.state.context {
                if ctx.context_type == ContextType::TMUX && ctx.cache_deprecated && !ctx.cache_in_flight {
                    let panel = crate::app::panels::get_panel(&ctx.context_type);
                    if let Some(request) = panel.build_cache_request(ctx, &self.state) {
                        crate::state::cache::process_cache_request(request, self.cache_tx.clone());
                    }
                }
            }
            for ctx in &mut self.state.context {
                if ctx.context_type == ContextType::TMUX && ctx.cache_deprecated {
                    ctx.cache_in_flight = true;
                }
            }
            // Also check file panels
            super::wait::trigger_dirty_panel_refresh(&self.state, &self.cache_tx);

            if super::wait::has_dirty_panels(&self.state) {
                self.state.waiting_for_panels = true;
                self.wait_started_ms = now_ms();
            } else {
                self.continue_streaming(tx);
            }
        } else {
            // Pure sleep (console_sleep): just continue, no refresh needed
            self.continue_streaming(tx);
        }
    }

    /// Non-blocking check: if the user has resolved a pending question form,
    /// replace the __QUESTION_PENDING__ placeholder with the real answer and
    /// resume the tool pipeline (create result message + continue streaming).
    fn check_question_form(&mut self, tx: &Sender<StreamEvent>) {
        // Only check if we have pending tool results waiting on a question
        if self.pending_question_tool_results.is_none() {
            return;
        }

        // Check if form is resolved
        let resolved =
            self.state.get_ext::<cp_base::ui::PendingQuestionForm>().map(|f| f.resolved).unwrap_or(false);

        if !resolved {
            return;
        }

        // Extract the resolved form and remove it from state
        let form = self
            .state
            .module_data
            .remove(&std::any::TypeId::of::<cp_base::ui::PendingQuestionForm>())
            .and_then(|v| v.downcast::<cp_base::ui::PendingQuestionForm>().ok())
            .expect("form must exist since we just checked resolved=true");

        let result_json =
            form.result_json.unwrap_or_else(|| r#"{"dismissed":true,"message":"User declined to answer"}"#.to_string());

        // Replace placeholder in pending tool results
        let mut tool_results = self.pending_question_tool_results.take().unwrap();
        for tr in &mut tool_results {
            if tr.content == "__QUESTION_PENDING__" {
                tr.content = result_json.clone();
            }
        }

        // Now resume the normal pipeline: create result message and continue streaming
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

        // Check if reload was requested
        if self.state.reload_pending {
            perform_reload(&mut self.state);
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

        // Continue streaming
        super::wait::trigger_dirty_panel_refresh(&self.state, &self.cache_tx);
        if super::wait::has_dirty_file_panels(&self.state) {
            self.state.waiting_for_panels = true;
            self.wait_started_ms = now_ms();
        } else {
            self.continue_streaming(tx);
        }
    }

    /// Non-blocking check: poll console waiters for satisfied conditions.
    /// - Blocking path: replace `__CONSOLE_WAIT_BLOCKING__` sentinel and resume pipeline.
    /// - Async path: drain completed_async and create spine notifications.
    fn check_console_waiters(&mut self, tx: &Sender<StreamEvent>) {
        // Always poll — this checks both blocking and async waiters
        let satisfied_blocking = ConsoleState::check_waiters(&mut self.state);

        // --- Async completions → spine notifications ---
        let completed: Vec<cp_mod_console::CompletedWait> = {
            let cs = ConsoleState::get_mut(&mut self.state);
            cs.completed_async.drain(..).collect()
        };
        if !completed.is_empty() {
            use cp_mod_spine::{NotificationType, SpineState};
            for cw in completed {
                let content = cp_mod_console::types::format_wait_result(
                    &cw.session_name,
                    cw.exit_code,
                    &cw.panel_id,
                    &cw.last_lines,
                );
                SpineState::create_notification(
                    &mut self.state,
                    NotificationType::Custom,
                    "console".to_string(),
                    content,
                );
            }
            self.save_state_async();
        }

        // --- Blocking sentinel replacement ---
        if self.pending_console_wait_tool_results.is_none() || satisfied_blocking.is_empty() {
            return;
        }

        let mut tool_results = self.pending_console_wait_tool_results.take().unwrap();

        // Replace sentinels with real results
        for tr in &mut tool_results {
            if tr.content == CONSOLE_WAIT_BLOCKING_SENTINEL {
                if let Some((_, result)) = satisfied_blocking.iter().find(|(id, _)| *id == tr.tool_use_id) {
                    tr.content = result.clone();
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
            timestamp_ms: crate::core::panels::now_ms(),
        };
        self.save_message_async(&result_msg);
        self.state.messages.push(result_msg);

        if self.state.reload_pending {
            perform_reload(&mut self.state);
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
            timestamp_ms: crate::core::panels::now_ms(),
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

    fn finalize_stream(&mut self, tldr_tx: &Sender<TlDrResult>) {
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

    fn handle_action(&mut self, action: Action, tx: &Sender<StreamEvent>, tldr_tx: &Sender<TlDrResult>) {
        // Any action triggers a re-render
        self.state.dirty = true;
        match apply_action(&mut self.state, action) {
            ActionResult::StopStream => {
                self.typewriter.reset();
                self.pending_done = None;
                self.pending_tools.clear();
                // Pause auto-continuation when user explicitly cancels streaming.
                // Without this, the spine would immediately relaunch a new stream
                // (e.g., due to continue_until_todos_done), making the system
                // uncontrollable — the user can never stop it with Esc. (#44)
                // We set user_stopped instead of disabling continue_until_todos_done,
                // so auto-continuation resumes when the user sends a new message.
                // Notify all modules that the user stopped streaming
                for module in crate::modules::all_modules() {
                    module.on_stream_stop(&mut self.state);
                }
                self.state.touch_panel(ContextType::new(ContextType::SPINE));
                if let Some(msg) = self.state.messages.last()
                    && msg.role == "assistant"
                {
                    self.save_message_async(msg);
                }
                self.save_state_async();
            }
            ActionResult::Save => {
                self.save_state_async();
                // Check spine synchronously for responsive auto-continuation
                self.check_spine(tx, tldr_tx);
            }
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
            ActionResult::StartApiCheck => {
                let (api_tx, api_rx) = std::sync::mpsc::channel();
                self.api_check_rx = Some(api_rx);
                crate::llms::start_api_check(self.state.llm_provider, self.state.current_model(), api_tx);
                self.save_state_async();
            }
            ActionResult::Nothing => {}
        }
    }

    /// Set up file watchers from all modules' watch_paths().
    fn setup_file_watchers(&mut self) {
        self.sync_file_watchers();
    }

    /// Sync GhWatcher with current GithubResult panels
    fn sync_gh_watches(&self) {
        let token = match &cp_mod_github::GithubState::get(&self.state).github_token {
            Some(t) => t.clone(),
            None => return,
        };
        let panels: Vec<(String, String, String)> = self
            .state
            .context
            .iter()
            .filter(|c| c.context_type == ContextType::GITHUB_RESULT)
            .filter_map(|c| c.get_meta_str("result_command").map(|cmd| (c.id.clone(), cmd.to_string(), token.clone())))
            .collect();
        self.gh_watcher.sync_watches(&panels);

        // Sync branch PR watch — poll for PRs on the current git branch
        let branch = cp_mod_git::GitState::get(&self.state).git_branch.as_deref();
        self.gh_watcher.sync_branch_pr(branch, Some(&token));
    }

    /// Schedule initial cache refreshes for fixed context elements only.
    /// Dynamic panels (File, Glob, Grep, Tmux, GitResult, GithubResult) will be
    /// populated gradually by check_timer_based_deprecation via its `needs_initial`
    /// path, staggered by the `cache_in_flight` guard — preventing a massive burst
    /// of concurrent background threads on startup when many panels are persisted.
    fn schedule_initial_cache_refreshes(&mut self) {
        for i in 0..self.state.context.len() {
            let ctx = &self.state.context[i];
            if !ctx.context_type.is_fixed() {
                continue;
            }
            let panel = crate::app::panels::get_panel(&ctx.context_type);
            let request = panel.build_cache_request(ctx, &self.state);
            if let Some(request) = request {
                process_cache_request(request, self.cache_tx.clone());
                self.state.context[i].cache_in_flight = true;
            }
        }
    }

    /// Process incoming cache updates from background threads
    fn process_cache_updates(&mut self, cache_rx: &Receiver<CacheUpdate>) {
        Self::process_cache_updates_static(&mut self.state, cache_rx);
    }

    /// Static version of process_cache_updates for use in wait module
    fn process_cache_updates_static(state: &mut State, cache_rx: &Receiver<CacheUpdate>) {
        let _guard = crate::profile!("app::cache_updates");
        while let Ok(update) = cache_rx.try_recv() {
            // Handle Unchanged early — just clear in_flight, no panel dispatch needed
            if let CacheUpdate::Unchanged { ref context_id } = update {
                if let Some(ctx) = state.context.iter_mut().find(|c| c.id == *context_id) {
                    ctx.cache_in_flight = false;
                    ctx.cache_deprecated = false;
                }
                continue;
            }

            // ModuleSpecific: match by context_type
            if let CacheUpdate::ModuleSpecific { ref context_type, ref data, .. } = update {
                // Special case: BranchPrUpdate targets GithubState, not a panel
                if context_type.as_str() == ContextType::GITHUB_RESULT
                    && data.is::<cp_mod_github::watcher::BranchPrUpdate>()
                {
                    if let CacheUpdate::ModuleSpecific { data, .. } = update
                        && let Ok(pr_update) = data.downcast::<cp_mod_github::watcher::BranchPrUpdate>()
                    {
                        cp_mod_github::GithubState::get_mut(state).branch_pr = pr_update.pr_info;
                        state.dirty = true;
                    }
                    continue;
                }

                let idx = state.context.iter().position(|c| c.context_type == *context_type);
                let Some(idx) = idx else { continue };
                let mut ctx = state.context.remove(idx);
                let panel = crate::app::panels::get_panel(&ctx.context_type);
                let _changed = panel.apply_cache_update(update, &mut ctx, state);
                ctx.cache_in_flight = false;
                state.context.insert(idx, ctx);
                state.dirty = true;
                continue;
            }

            // Content: match by context_id
            let CacheUpdate::Content { ref context_id, .. } = update else { continue };
            let idx = state.context.iter().position(|c| c.id == *context_id);
            let Some(idx) = idx else { continue };
            let mut ctx = state.context.remove(idx);
            let panel = crate::app::panels::get_panel(&ctx.context_type);
            // apply_cache_update calls update_if_changed which sets last_refresh_ms on change
            let _changed = panel.apply_cache_update(update, &mut ctx, state);
            ctx.cache_in_flight = false;
            state.context.insert(idx, ctx);
            state.dirty = true;
        }
    }

    /// Process file watcher events — delegates invalidation to modules via trait methods.
    fn process_watcher_events(&mut self) {
        let _guard = crate::profile!("app::watcher_events");
        // Collect events (immutable borrow on file_watcher released after this block)
        let events = {
            let Some(watcher) = &self.file_watcher else { return };
            watcher.poll_events()
        };
        if events.is_empty() {
            return;
        }

        let modules = crate::modules::all_modules();

        // First pass: ask modules which panels to invalidate
        let mut refresh_indices = Vec::new();
        let mut rewatch_paths: Vec<String> = Vec::new();
        for event in &events {
            let (path, is_dir_event) = match event {
                WatchEvent::FileChanged(p) => (p, false),
                WatchEvent::DirChanged(p) => (p, true),
            };

            for (i, ctx) in self.state.context.iter_mut().enumerate() {
                for module in &modules {
                    if module.should_invalidate_on_fs_change(ctx, path, is_dir_event) {
                        ctx.cache_deprecated = true;
                        if module.watcher_immediate_refresh() {
                            refresh_indices.push(i);
                        }
                        self.state.dirty = true;
                        break; // Only one module owns each context type
                    }
                }
            }

            if !is_dir_event {
                rewatch_paths.push(path.clone());
            }
        }

        // Second pass: build and send requests (deduplicated, skip in-flight)
        refresh_indices.sort_unstable();
        refresh_indices.dedup();
        for i in refresh_indices {
            if self.state.context[i].cache_in_flight {
                continue;
            }
            let ctx = &self.state.context[i];
            let panel = crate::app::panels::get_panel(&ctx.context_type);
            let request = panel.build_cache_request(ctx, &self.state);
            if let Some(request) = request {
                process_cache_request(request, self.cache_tx.clone());
                self.state.context[i].cache_in_flight = true;
            }
        }

        // Third pass: re-watch files to pick up new inodes after atomic rename
        // (editors like vim/vscode save via rename, which invalidates the inotify watch)
        if let Some(watcher) = &mut self.file_watcher {
            for path in rewatch_paths {
                let _ = watcher.rewatch_file(&path);
            }
        }
    }

    /// Check timer-based deprecation for glob, grep, tmux, git
    /// Also handles initial population for newly created context elements.
    ///
    /// Timer-based (interval) refreshes are restricted to **fixed panels and the
    /// currently selected panel** to avoid wasting CPU on background refresh of
    /// accumulated dynamic panels the user isn't looking at.  Dynamic panels still
    /// get refreshed when:
    ///   - first created (`needs_initial`)
    ///   - explicitly deprecated by a file-watcher event
    ///   - the user selects them (becomes the selected panel)
    fn check_timer_based_deprecation(&mut self) {
        let current_ms = now_ms();

        // Only check every 100ms to avoid excessive work
        if current_ms.saturating_sub(self.last_timer_check_ms) < 100 {
            return;
        }
        let _guard = crate::profile!("app::timer_deprecation");
        self.last_timer_check_ms = current_ms;

        // Ensure all module-requested paths have active watchers
        self.sync_file_watchers();

        let mut requests: Vec<(usize, CacheRequest)> = Vec::new();

        for (i, ctx) in self.state.context.iter().enumerate() {
            if ctx.cache_in_flight {
                continue;
            }

            let panel = crate::app::panels::get_panel(&ctx.context_type);

            // Case 1: Initial load — panel has no content yet
            if ctx.cached_content.is_none() && ctx.context_type.needs_cache() {
                if let Some(req) = panel.build_cache_request(ctx, &self.state) {
                    requests.push((i, req));
                }
                continue;
            }

            // Case 2: Explicitly dirty (watcher event, tool, self-invalidation)
            // ALL dirty panels refresh regardless of selection — no UI-gating.
            if ctx.cache_deprecated {
                if let Some(req) = panel.build_cache_request(ctx, &self.state) {
                    requests.push((i, req));
                }
                continue;
            }

            // Case 3: Timer-based polling (Tmux, Git, GitResult, GithubResult, Glob, Grep)
            if let Some(interval) = panel.cache_refresh_interval_ms() {
                let last = self.last_poll_ms.get(&ctx.id).copied().unwrap_or(0);
                if current_ms.saturating_sub(last) >= interval
                    && let Some(req) = panel.build_cache_request(ctx, &self.state)
                {
                    requests.push((i, req));
                }
            }
        }

        // Mutable pass: send requests, mark in-flight, update poll timestamps
        for (i, request) in requests {
            process_cache_request(request, self.cache_tx.clone());
            self.state.context[i].cache_in_flight = true;
            self.last_poll_ms.insert(self.state.context[i].id.clone(), current_ms);
        }
    }

    /// Sync file watchers from all modules' watch_paths().
    /// Called periodically to catch panels created during tool execution.
    fn sync_file_watchers(&mut self) {
        use cp_base::panels::WatchSpec;
        let Some(watcher) = &mut self.file_watcher else { return };

        let modules = crate::modules::all_modules();
        for module in &modules {
            for spec in module.watch_paths(&self.state) {
                match spec {
                    WatchSpec::File(path) => {
                        if !self.watched_file_paths.contains(&path) && watcher.watch_file(&path).is_ok() {
                            self.watched_file_paths.insert(path);
                        }
                    }
                    WatchSpec::Dir(path) => {
                        if !self.watched_dir_paths.contains(&path) && watcher.watch_dir(&path).is_ok() {
                            self.watched_dir_paths.insert(path);
                        }
                    }
                    WatchSpec::DirRecursive(path) => {
                        if !self.watched_dir_paths.contains(&path) && watcher.watch_dir_recursive(&path).is_ok() {
                            self.watched_dir_paths.insert(path);
                        }
                    }
                }
            }
        }
    }

    /// Check the spine for auto-continuation decisions.
    /// Evaluates guard rails and auto-continuation logic.
    /// If a continuation fires, starts streaming.
    fn check_spine(&mut self, tx: &Sender<StreamEvent>, tldr_tx: &Sender<TlDrResult>) {
        use cp_mod_spine::engine::{SpineDecision, apply_continuation, check_spine};

        match check_spine(&mut self.state) {
            SpineDecision::Idle => {}
            SpineDecision::Blocked(reason) => {
                // Guard rail blocked — notification already created by engine
                self.state.guard_rail_blocked = Some(reason);
                self.state.dirty = true;
                self.save_state_async();
            }
            SpineDecision::Continue(action) => {
                // Auto-continuation fired — apply it and start streaming
                self.state.guard_rail_blocked = None;
                let should_stream = apply_continuation(&mut self.state, action);
                if should_stream {
                    self.typewriter.reset();
                    self.pending_tools.clear();
                    // Generate TL;DR for synthetic user message
                    if self.state.messages.len() >= 2 {
                        let user_msg = &self.state.messages[self.state.messages.len() - 2];
                        if user_msg.role == "user" && user_msg.tl_dr.is_none() {
                            let tldr_id = user_msg.id.clone();
                            let tldr_content = user_msg.content.clone();
                            cp_mod_tree::TreeState::get_mut(&mut self.state).pending_tldrs += 1;
                            generate_tldr(tldr_id, tldr_content, tldr_tx.clone());
                        }
                    }
                    let ctx = prepare_stream_context(&mut self.state, false);
                    let system_prompt = get_active_agent_content(&self.state);
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
                    self.save_state_async();
                    self.state.dirty = true;
                }
            }
        }
    }

    /// Update spinner animation frame if there's active loading/streaming.
    /// Throttled to 10fps (100ms) to avoid unnecessary re-renders.
    fn update_spinner_animation(&mut self) {
        let now = now_ms();
        if now.saturating_sub(self.last_spinner_ms) < 100 {
            return;
        }

        // Check if there's any active operation that needs spinner animation
        let has_active_spinner = self.state.is_streaming
            || cp_mod_tree::TreeState::get(&self.state).pending_tldrs > 0
            || self.state.api_check_in_progress
            || self.state.context.iter().any(|c| c.cached_content.is_none() && c.context_type.needs_cache());

        if has_active_spinner {
            self.last_spinner_ms = now;
            // Increment spinner frame (wraps around automatically with u64)
            self.state.spinner_frame = self.state.spinner_frame.wrapping_add(1);
            // Mark dirty to trigger re-render with new spinner frame
            self.state.dirty = true;
        }
    }

    /// Send state to background writer (debounced, non-blocking).
    /// Preferred over `save_state()` in the main event loop.
    fn save_state_async(&self) {
        self.writer.send_batch(build_save_batch(&self.state));
    }

    /// Send a message to background writer (non-blocking).
    /// Preferred over `save_message()` in the main event loop.
    fn save_message_async(&self, msg: &Message) {
        self.writer.send_message(build_message_op(msg));
    }

    /// Handle keyboard events when a question form is active.
    /// Mutates the PendingQuestionForm directly in state.
    fn handle_question_form_event(&mut self, event: &event::Event) {
        use crossterm::event::{KeyCode, KeyModifiers};
        let event::Event::Key(key) = event else { return };

        let form = match self.state.get_ext_mut::<cp_base::ui::PendingQuestionForm>() {
            Some(f) => f,
            None => return,
        };

        // Check if currently typing in "Other" field
        let typing_other = form.answers[form.current_question].typing_other;

        match key.code {
            KeyCode::Esc => {
                form.dismiss();
            }
            KeyCode::Up if !typing_other => {
                form.cursor_up();
            }
            KeyCode::Down if !typing_other => {
                form.cursor_down();
            }
            KeyCode::Left => {
                form.prev_question();
            }
            KeyCode::Right => {
                form.next_question();
            }
            KeyCode::Enter => {
                form.handle_enter();
            }
            KeyCode::Char(' ') if !typing_other && form.is_multi_select() => {
                form.toggle_selection();
            }
            KeyCode::Char(' ') if !typing_other => {
                // Single-select: space selects and advances
                form.toggle_selection();
            }
            // When on "Other": arrow keys navigate away, typing goes to text field
            KeyCode::Up if typing_other => {
                form.cursor_up();
            }
            KeyCode::Down if typing_other => {
                form.cursor_down();
            }
            KeyCode::Backspace if typing_other => {
                form.backspace();
            }
            KeyCode::Char(c) if typing_other => {
                // Don't capture ctrl+key combos
                if !key.modifiers.contains(KeyModifiers::CONTROL) {
                    form.type_char(c);
                }
            }
            // Non-typing-other: any char that's not space does nothing
            _ => {}
        }
    }

    /// Handle keyboard events when command palette is open
    fn handle_palette_event(&mut self, event: &event::Event) -> Option<Action> {
        use crossterm::event::{KeyCode, KeyModifiers};

        let event::Event::Key(key) = event else {
            return Some(Action::None);
        };

        match key.code {
            // Escape closes palette
            KeyCode::Esc => {
                self.command_palette.close();
                None
            }
            // Enter executes selected command
            KeyCode::Enter => {
                if let Some(cmd) = self.command_palette.get_selected() {
                    let id = cmd.id.clone();
                    self.command_palette.close();

                    // Handle different command types
                    match id.as_str() {
                        "quit" => return None, // Signal quit
                        "reload" => {
                            // Perform reload (sets reload_requested flag and exits)
                            perform_reload(&mut self.state);
                            return None; // Won't reach here, but needed for type system
                        }
                        "config" => return Some(Action::ToggleConfigView),
                        _ => {
                            // Navigate to any context panel (P-prefixed or special IDs like "chat")
                            if self.state.context.iter().any(|c| c.id == id) {
                                return Some(Action::SelectContextById(id));
                            }
                        }
                    }
                }
                Some(Action::None)
            }
            // Up/Down navigate results
            KeyCode::Up => {
                self.command_palette.select_prev();
                None
            }
            KeyCode::Down => {
                self.command_palette.select_next();
                None
            }
            // Left/Right move cursor in query
            KeyCode::Left => {
                self.command_palette.cursor_left();
                None
            }
            KeyCode::Right => {
                self.command_palette.cursor_right();
                None
            }
            // Home/End for cursor
            KeyCode::Home => {
                self.command_palette.cursor = 0;
                None
            }
            KeyCode::End => {
                self.command_palette.cursor = self.command_palette.query.len();
                None
            }
            // Backspace/Delete
            KeyCode::Backspace => {
                self.command_palette.backspace(&self.state);
                None
            }
            KeyCode::Delete => {
                self.command_palette.delete(&self.state);
                None
            }
            // Character input
            KeyCode::Char(c) => {
                // Ignore Ctrl+char combinations
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    return None;
                }
                self.command_palette.insert_char(c, &self.state);
                None
            }
            // Tab could cycle through results
            KeyCode::Tab => {
                if key.modifiers.contains(KeyModifiers::SHIFT) {
                    self.command_palette.select_prev();
                } else {
                    self.command_palette.select_next();
                }
                None
            }
            _ => None,
        }
    }
}
