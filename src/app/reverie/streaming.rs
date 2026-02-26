//! Reverie streaming — prompt construction and LLM stream management.
//!
//! Uses the EXACT SAME prepare_stream_context() as the main worker, passing
//! a ReverieContext to branch only at the conversation section. This preserves
//! prompt prefix cache hits (panels + tools identical).

use std::sync::mpsc::Sender;

use crate::app::context::{ReverieContext, prepare_stream_context};
use crate::infra::api::{StreamParams, start_streaming};
use crate::infra::constants::DEFAULT_WORKER_ID;
use crate::state::State;
use cp_base::llm_types::StreamEvent;

use super::tools;

/// Resolve the secondary model string from provider + model enum.
fn secondary_model_string(state: &State) -> String {
    use cp_base::llm_types::{LlmProvider, ModelInfo};
    match state.secondary_provider {
        LlmProvider::Anthropic | LlmProvider::ClaudeCode | LlmProvider::ClaudeCodeApiKey => {
            state.secondary_anthropic_model.api_name().to_string()
        }
        LlmProvider::Grok => state.secondary_grok_model.api_name().to_string(),
        LlmProvider::Groq => state.secondary_groq_model.api_name().to_string(),
        LlmProvider::DeepSeek => state.secondary_deepseek_model.api_name().to_string(),
    }
}

/// The reverie system prompt — kept minimal since the real agent instructions
/// are injected into the P-reverie panel (for cache-friendly placement).
const REVERIE_SYSTEM_PROMPT: &str = "You are a background sub-agent. Follow the instructions in the P-reverie panel.";

/// Build the reverie prompt and start streaming to the secondary LLM.
///
/// Uses the exact same `prepare_stream_context()` as the main worker. The
/// `ReverieContext` parameter causes it to branch at the conversation section:
/// - Panels and tools are IDENTICAL → prompt prefix cache hit
/// - Conversation is replaced with P-main-conv + reverie's own messages
///
/// # Panics
/// Only call when `state.reverie.is_some()`.
#[cfg_attr(not(test), allow(dead_code))]
pub fn start_reverie_stream(state: &mut State, tx: Sender<StreamEvent>) {
    // Get the reverie's own messages (empty on first launch) and trim whitespace
    let mut reverie_messages = state.reverie.as_ref().map(|r| r.messages.clone()).unwrap_or_default();
    for msg in &mut reverie_messages {
        if msg.role == "assistant" {
            msg.content = msg.content.trim_end().to_string();
        }
    }

    // Build tool restrictions text for the reverie's conversation preamble
    let tool_restrictions = tools::build_tool_restrictions_text(&state.tools);

    // Use the EXACT same prepare_stream_context as the main worker.
    // Passing ReverieContext replaces the conversation section with
    // P-main-conv + reverie messages — panels and tools stay IDENTICAL for cache hits.
    let ctx =
        prepare_stream_context(state, true, Some(ReverieContext { messages: reverie_messages, tool_restrictions }));

    // Fire the stream to the secondary model
    start_streaming(
        StreamParams {
            provider: state.secondary_provider,
            model: secondary_model_string(state),
            max_output_tokens: state.secondary_max_output_tokens(),
            messages: ctx.messages,
            context_items: ctx.context_items,
            tools: ctx.tools,
            system_prompt: REVERIE_SYSTEM_PROMPT.to_string(),
            seed_content: Some(REVERIE_SYSTEM_PROMPT.to_string()),
            worker_id: DEFAULT_WORKER_ID.to_string(),
        },
        tx,
    );
}
