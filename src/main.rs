mod actions;
mod api;
mod background;
mod context_cleaner;
mod events;
mod highlight;
mod mouse;
mod persistence;
mod state;
mod tool_defs;
mod tools;
mod typewriter;
mod ui;

use std::io;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;

use actions::{apply_action, Action, ActionResult};
use api::{start_cleaning, start_streaming, StreamEvent};
use background::{generate_tldr, TlDrResult};
use events::handle_event;
use persistence::{load_state, save_message, save_state};
use state::{MessageStatus, MessageType, State, ToolUseRecord};
use tools::{execute_tool, generate_directory_tree, get_context_files, get_glob_context, get_memory_context, get_overview_context, get_tmux_context, get_todo_context, refresh_conversation_context, refresh_file_hashes, refresh_glob_results, refresh_memory_context, refresh_overview_context, refresh_tmux_context, refresh_todo_context, refresh_tools_context, ToolResult, ToolUse};
use typewriter::TypewriterBuffer;

/// Context data prepared for streaming
struct StreamContext {
    messages: Vec<state::Message>,
    file_context: Vec<(String, String)>,
    glob_context: Vec<(String, String)>,
    tmux_context: Vec<(String, String)>,
    todo_context: String,
    memory_context: String,
    overview_context: String,
    directory_tree: String,
    tools: Vec<tool_defs::ToolDefinition>,
}

/// Refresh all context elements and prepare data for streaming
fn prepare_stream_context(state: &mut State, include_last_message: bool) -> StreamContext {
    // Refresh file hashes and token counts
    refresh_file_hashes(state);

    // Refresh all context element token counts
    refresh_conversation_context(state);
    refresh_glob_results(state);
    refresh_tmux_context(state);
    refresh_todo_context(state);
    refresh_memory_context(state);
    refresh_overview_context(state);
    refresh_tools_context(state);

    // Get context content
    let file_context = get_context_files(state);
    let glob_context = get_glob_context(state);
    let tmux_context = get_tmux_context(state);
    let todo_context = get_todo_context(state);
    let memory_context = get_memory_context(state);
    let overview_context = get_overview_context(state);
    let directory_tree = generate_directory_tree(state);

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
        file_context,
        glob_context,
        tmux_context,
        todo_context,
        memory_context,
        overview_context,
        directory_tree,
        tools: state.tools.clone(),
    }
}

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    io::stdout().execute(EnterAlternateScreen)?;
    io::stdout().execute(EnableMouseCapture)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let mut state = load_state();

    // Migration: Ensure default context elements exist with correct IDs
    // P1 = Main (Conversation), P2 = Directory (Tree), P3 = Todo

    // Ensure Main conversation exists
    if !state.context.iter().any(|c| c.context_type == state::ContextType::Conversation && c.name == "Main") {
        state.context.insert(0, state::ContextElement {
            id: "P1".to_string(),
            context_type: state::ContextType::Conversation,
            name: "Main".to_string(),
            token_count: 0,
            file_path: None,
            file_hash: None,
            glob_pattern: None,
            glob_path: None,
            tmux_pane_id: None,
            tmux_lines: None,
            tmux_last_keys: None,
            tmux_description: None,
        });
    }

    // Ensure Tree context element exists
    if !state.context.iter().any(|c| c.context_type == state::ContextType::Tree) {
        state.context.insert(1.min(state.context.len()), state::ContextElement {
            id: "P2".to_string(),
            context_type: state::ContextType::Tree,
            name: "Directory".to_string(),
            token_count: 0,
            file_path: None,
            file_hash: None,
            glob_pattern: None,
            glob_path: None,
            tmux_pane_id: None,
            tmux_lines: None,
            tmux_last_keys: None,
            tmux_description: None,
        });
    }

    // Ensure Todo context element exists
    if !state.context.iter().any(|c| c.context_type == state::ContextType::Todo) {
        state.context.insert(2.min(state.context.len()), state::ContextElement {
            id: "P3".to_string(),
            context_type: state::ContextType::Todo,
            name: "Todo".to_string(),
            token_count: 0,
            file_path: None,
            file_hash: None,
            glob_pattern: None,
            glob_path: None,
            tmux_pane_id: None,
            tmux_lines: None,
            tmux_last_keys: None,
            tmux_description: None,
        });
    }

    // Ensure Memory context element exists
    if !state.context.iter().any(|c| c.context_type == state::ContextType::Memory) {
        state.context.insert(3.min(state.context.len()), state::ContextElement {
            id: "P4".to_string(),
            context_type: state::ContextType::Memory,
            name: "Memory".to_string(),
            token_count: 0,
            file_path: None,
            file_hash: None,
            glob_pattern: None,
            glob_path: None,
            tmux_pane_id: None,
            tmux_lines: None,
            tmux_last_keys: None,
            tmux_description: None,
        });
    }

    // Ensure Overview context element exists
    if !state.context.iter().any(|c| c.context_type == state::ContextType::Overview) {
        state.context.insert(4.min(state.context.len()), state::ContextElement {
            id: "P5".to_string(),
            context_type: state::ContextType::Overview,
            name: "Overview".to_string(),
            token_count: 0,
            file_path: None,
            file_hash: None,
            glob_pattern: None,
            glob_path: None,
            tmux_pane_id: None,
            tmux_lines: None,
            tmux_last_keys: None,
            tmux_description: None,
        });
    }

    // Ensure Tools context element exists
    if !state.context.iter().any(|c| c.context_type == state::ContextType::Tools) {
        state.context.insert(5.min(state.context.len()), state::ContextElement {
            id: "P6".to_string(),
            context_type: state::ContextType::Tools,
            name: "Tools".to_string(),
            token_count: tool_defs::estimate_tools_tokens(&state.tools),
            file_path: None,
            file_hash: None,
            glob_pattern: None,
            glob_path: None,
            tmux_pane_id: None,
            tmux_lines: None,
            tmux_last_keys: None,
            tmux_description: None,
        });
    }

    // Generate initial tree to populate token count
    let _ = generate_directory_tree(&mut state);

    let (tx, rx): (Sender<StreamEvent>, Receiver<StreamEvent>) = mpsc::channel();
    let (tldr_tx, tldr_rx): (Sender<TlDrResult>, Receiver<TlDrResult>) = mpsc::channel();
    let mut typewriter = TypewriterBuffer::new();
    let mut pending_done: Option<(usize, usize)> = None;
    let mut pending_tools: Vec<ToolUse> = Vec::new();
    let mut mouse_captured = true;

    loop {
        // Process stream events (only if streaming is active)
        while let Ok(evt) = rx.try_recv() {
            if !state.is_streaming {
                // Drain remaining events without processing
                continue;
            }
            match evt {
                StreamEvent::Chunk(text) => {
                    typewriter.add_chunk(&text);
                }
                StreamEvent::ToolUse(tool) => {
                    pending_tools.push(tool);
                }
                StreamEvent::Done { input_tokens, output_tokens } => {
                    typewriter.mark_done();
                    pending_done = Some((input_tokens, output_tokens));
                }
                StreamEvent::Error(e) => {
                    typewriter.reset();
                    apply_action(&mut state, Action::StreamError(e));
                }
            }
        }

        // Release chars from typewriter (only if streaming)
        if state.is_streaming {
            if let Some(chars) = typewriter.take_chars() {
                apply_action(&mut state, Action::AppendChars(chars));
            }
        }

        // Process TL;DR results from background jobs
        while let Ok(tldr) = tldr_rx.try_recv() {
            state.pending_tldrs = state.pending_tldrs.saturating_sub(1);
            // Debug logging
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("tldr_debug.log") {
                use std::io::Write;
                let _ = writeln!(f, "Received TL;DR for message {}", tldr.message_id);
            }
            if let Some(msg) = state.messages.iter_mut().find(|m| m.id == tldr.message_id) {
                msg.tl_dr = Some(tldr.tl_dr);
                msg.tl_dr_token_count = tldr.token_count;
                save_message(msg);
                // Debug logging
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("tldr_debug.log") {
                    use std::io::Write;
                    let _ = writeln!(f, "Saved TL;DR for message {}", msg.id);
                }
            } else {
                // Debug logging
                if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open("tldr_debug.log") {
                    use std::io::Write;
                    let _ = writeln!(f, "Message {} not found in state!", tldr.message_id);
                }
            }
        }

        // Handle tool use after stream is done and typewriter is empty (works for both streaming and cleaning)
        if (state.is_streaming || state.is_cleaning_context) && pending_done.is_some() && typewriter.pending_chars.is_empty() && !pending_tools.is_empty() {
            let tools = std::mem::take(&mut pending_tools);
            let mut tool_results: Vec<ToolResult> = Vec::new();

            // First, finalize the current assistant message (text before tools)
            if let Some(msg) = state.messages.last_mut() {
                if msg.role == "assistant" {
                    // Save the assistant message as-is (just the text portion)
                    save_message(msg);
                    // Generate TL;DR for this text segment if it has content
                    if !msg.content.trim().is_empty() && msg.tl_dr.is_none() {
                        state.pending_tldrs += 1;
                        generate_tldr(msg.id.clone(), msg.content.clone(), tldr_tx.clone());
                    }
                }
            }

            // Create a separate message for each tool call
            for tool in &tools {
                let tool_id = format!("T{}", state.next_tool_id);
                state.next_tool_id += 1;

                let tool_msg = state::Message {
                    id: tool_id,
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
                };
                save_message(&tool_msg);
                state.messages.push(tool_msg);

                let result = execute_tool(tool, &mut state);
                tool_results.push(result);
            }

            // Create a ToolResult message to store all tool results (role=user for API)
            let result_id = format!("R{}", state.next_result_id);
            state.next_result_id += 1;
            let tool_result_records: Vec<state::ToolResultRecord> = tool_results.iter()
                .map(|r| state::ToolResultRecord {
                    tool_use_id: r.tool_use_id.clone(),
                    content: r.content.clone(),
                    is_error: r.is_error,
                })
                .collect();
            let result_msg = state::Message {
                id: result_id,
                role: "user".to_string(),
                message_type: MessageType::ToolResult,
                content: String::new(),
                content_token_count: 0,
                tl_dr: None,
                tl_dr_token_count: 0,
                status: MessageStatus::Full,
                tool_uses: Vec::new(),
                tool_results: tool_result_records,
            };
            save_message(&result_msg);
            state.messages.push(result_msg);

            // Create a new assistant message for text after tools
            let assistant_id = format!("A{}", state.next_assistant_id);
            state.next_assistant_id += 1;
            let new_assistant_msg = state::Message {
                id: assistant_id,
                role: "assistant".to_string(),
                message_type: MessageType::TextMessage,
                content: String::new(),
                content_token_count: 0,
                tl_dr: None,
                tl_dr_token_count: 0,
                status: MessageStatus::Full,
                tool_uses: Vec::new(),
                tool_results: Vec::new(),
            };
            state.messages.push(new_assistant_msg);

            // Reset token tracking for the new message
            state.streaming_estimated_tokens = 0;

            save_state(&state);

            // Refresh all contexts and continue streaming/cleaning
            let ctx = prepare_stream_context(&mut state, true);

            typewriter.reset();
            pending_done = None;

            if state.is_cleaning_context {
                // Continue cleaning
                let cleaner_tools = context_cleaner::get_cleaner_tools();
                start_cleaning(
                    ctx.messages,
                    ctx.file_context,
                    ctx.glob_context,
                    ctx.tmux_context,
                    ctx.todo_context,
                    ctx.memory_context,
                    ctx.overview_context,
                    ctx.directory_tree,
                    cleaner_tools,
                    &state,
                    tx.clone(),
                );
            } else {
                start_streaming(ctx.messages, ctx.file_context, ctx.glob_context, ctx.tmux_context, ctx.todo_context, ctx.memory_context, ctx.overview_context, ctx.directory_tree, ctx.tools, None, tx.clone());
            }
        }

        // Finalize stream (only if no pending tools and still streaming or cleaning)
        if state.is_streaming || state.is_cleaning_context {
            if let Some((input_tokens, output_tokens)) = pending_done {
                if typewriter.pending_chars.is_empty() && pending_tools.is_empty() {
                    // Reset cleaning flag if we were cleaning
                    let was_cleaning = state.is_cleaning_context;
                    if was_cleaning {
                        state.is_cleaning_context = false;
                    }

                    match apply_action(&mut state, Action::StreamDone { _input_tokens: input_tokens, output_tokens }) {
                        ActionResult::SaveMessage(id) => {
                            // Find message and trigger TL;DR for assistant messages (skip for cleaning)
                            if !was_cleaning {
                                let tldr_info = state.messages.iter().find(|m| m.id == id).and_then(|msg| {
                                    save_message(msg);
                                    if msg.role == "assistant" && msg.tl_dr.is_none() && !msg.content.is_empty() {
                                        Some((msg.id.clone(), msg.content.clone()))
                                    } else {
                                        None
                                    }
                                });
                                if let Some((msg_id, content)) = tldr_info {
                                    state.pending_tldrs += 1;
                                    generate_tldr(msg_id, content, tldr_tx.clone());
                                }
                            }
                            save_state(&state);
                        }
                        ActionResult::Save => save_state(&state),
                        _ => {}
                    }
                    typewriter.reset();
                    pending_done = None;
                }
            }
        }

        // Toggle mouse capture based on copy_mode
        if state.copy_mode && mouse_captured {
            io::stdout().execute(DisableMouseCapture)?;
            mouse_captured = false;
        } else if !state.copy_mode && !mouse_captured {
            io::stdout().execute(EnableMouseCapture)?;
            mouse_captured = true;
        }

        // Render
        terminal.draw(|frame| ui::render(frame, &mut state))?;

        // Poll events
        if event::poll(Duration::from_millis(8))? {
            let evt = event::read()?;

            let Some(action) = handle_event(&evt, &state) else {
                save_state(&state);
                break;
            };

            match apply_action(&mut state, action.clone()) {
                ActionResult::StartStream => {
                    typewriter.reset();
                    pending_tools.clear();
                    // Generate TL;DR for the user message (second-to-last)
                    if state.messages.len() >= 2 {
                        let user_msg = &state.messages[state.messages.len() - 2];
                        if user_msg.role == "user" && user_msg.tl_dr.is_none() {
                            state.pending_tldrs += 1;
                            generate_tldr(user_msg.id.clone(), user_msg.content.clone(), tldr_tx.clone());
                        }
                    }
                    // Refresh all contexts and start streaming
                    let ctx = prepare_stream_context(&mut state, false);
                    start_streaming(ctx.messages, ctx.file_context, ctx.glob_context, ctx.tmux_context, ctx.todo_context, ctx.memory_context, ctx.overview_context, ctx.directory_tree, ctx.tools, None, tx.clone());
                    save_state(&state);
                }
                ActionResult::StopStream => {
                    typewriter.reset();
                    pending_done = None;
                    pending_tools.clear();
                    // Save the partial message
                    if let Some(msg) = state.messages.last() {
                        if msg.role == "assistant" {
                            save_message(msg);
                        }
                    }
                    save_state(&state);
                }
                ActionResult::Save => {
                    save_state(&state);
                }
                ActionResult::SaveMessage(id) => {
                    // Find message and extract info needed for TL;DR
                    let tldr_info = state.messages.iter().find(|m| m.id == id).and_then(|msg| {
                        save_message(msg);
                        // Generate TL;DR for assistant messages when they're done
                        if msg.role == "assistant" && msg.tl_dr.is_none() && !msg.content.is_empty() {
                            Some((msg.id.clone(), msg.content.clone()))
                        } else {
                            None
                        }
                    });
                    // Trigger TL;DR outside of borrow
                    if let Some((msg_id, content)) = tldr_info {
                        state.pending_tldrs += 1;
                        generate_tldr(msg_id, content, tldr_tx.clone());
                    }
                    save_state(&state);
                }
                ActionResult::StartCleaning => {
                    // Start context cleaning
                    typewriter.reset();
                    pending_tools.clear();
                    let ctx = prepare_stream_context(&mut state, true);
                    let cleaner_tools = context_cleaner::get_cleaner_tools();
                    start_cleaning(
                        ctx.messages,
                        ctx.file_context,
                        ctx.glob_context,
                        ctx.tmux_context,
                        ctx.todo_context,
                        ctx.memory_context,
                        ctx.overview_context,
                        ctx.directory_tree,
                        cleaner_tools,
                        &state,
                        tx.clone(),
                    );
                    save_state(&state);
                }
                ActionResult::Nothing => {}
            }
        }
    }

    io::stdout().execute(DisableMouseCapture)?;
    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    Ok(())
}
