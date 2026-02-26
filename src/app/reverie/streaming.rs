//! Reverie streaming — prompt construction and LLM stream management.
//!
//! Builds the reverie prompt (same panels + main conversation as read-only panel),
//! starts the secondary LLM stream, and processes reverie stream events.

use std::sync::mpsc::Sender;

use cp_base::llm_types::ModelInfo;

use crate::app::context::prepare_stream_context;
use crate::app::panels::ContextItem;
use crate::infra::api::{StreamEvent, StreamParams, start_streaming};
use crate::infra::constants::DEFAULT_WORKER_ID;
use crate::state::State;

use super::tools;

/// Build a read-only context item containing the main AI's current tip conversation.
///
/// This lets the reverie see what the main agent is working on without
/// polluting its own message history.
fn build_main_conversation_panel(state: &State) -> ContextItem {
    let content = cp_base::state::format_messages_to_chunk(&state.messages);
    ContextItem {
        id: "P-main-conv".to_string(),
        header: "Main Agent Conversation (read-only)".to_string(),
        content,
        last_refresh_ms: crate::app::panels::now_ms(),
    }
}

/// Resolve the secondary model string from provider + model enum.
fn secondary_model_string(state: &State) -> String {
    // Currently only Anthropic is supported for secondary — future expansion
    // will add other providers here.
    state.secondary_anthropic_model.api_name().to_string()
}

/// Build the reverie prompt and start streaming to the secondary LLM.
///
/// Called from the event loop when a reverie is triggered (Phase 7).
/// The caller should poll the paired `Receiver<StreamEvent>`.
///
/// # Panics
/// Only call when `state.reverie.is_some()`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn start_reverie_stream(state: &mut State, tx: Sender<StreamEvent>) {
    // Build context the same way the main AI does — panels, tools, messages
    let main_ctx = prepare_stream_context(state, true);

    // Build the reverie's curated tool list from the main tool definitions
    let reverie_tools = tools::reverie_tool_definitions(&main_ctx.tools);

    // Build the main conversation read-only panel
    let main_conv_panel = build_main_conversation_panel(state);

    // Assemble context items: all panels from main context + main conv panel
    let mut context_items = main_ctx.context_items;
    context_items.push(main_conv_panel);

    // Get the reverie's own messages (empty on first launch)
    let reverie_messages = state.reverie.as_ref().map(|r| r.messages.clone()).unwrap_or_default();

    // Fire the stream to the secondary model
    start_streaming(
        StreamParams {
            provider: state.secondary_provider,
            model: secondary_model_string(state),
            messages: reverie_messages,
            context_items,
            tools: reverie_tools,
            system_prompt: tools::REVERIE_SYSTEM_PROMPT.to_string(),
            seed_content: Some(tools::REVERIE_SYSTEM_PROMPT.to_string()),
            worker_id: DEFAULT_WORKER_ID.to_string(),
        },
        tx,
    );
}
