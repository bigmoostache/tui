use crate::core::panels::{collect_all_context, refresh_all_panels, ContextItem};
use crate::state::{Message, State};
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
