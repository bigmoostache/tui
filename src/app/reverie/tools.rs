//! Reverie tool definitions, dispatch, and the Report tool.
//!
//! The reverie has access to a curated subset of tools for context management,
//! plus a mandatory Report tool to end its run.

use std::collections::HashSet;

use crate::infra::tools::{ParamType, ToolDefinition, ToolParam, ToolResult, ToolUse};
use crate::state::State;

/// The complete set of tool IDs the reverie is allowed to use.
/// These are context-management tools — no file writes, git, console, etc.
// Used by reverie_tool_definitions() and dispatch_reverie_tool() — called
// from the event loop once Phase 7 (main loop integration) is wired.
#[cfg_attr(not(test), allow(dead_code))]
const ALLOWED_VESSEL_IDS: &[&str] = &[
    // Panel management
    "Close_panel",
    "Open",
    // Tree navigation
    "tree_toggle",
    "tree_filter",
    // Logs
    "log_create",
    "log_summarize",
    // Memory
    "memory_create",
    "memory_update",
    // Scratchpad
    "scratchpad_create_cell",
    "scratchpad_edit_cell",
    "scratchpad_wipe",
    // Conversation history
    "Close_conversation_history",
];

/// Build the Report tool definition — the reverie's mandatory end-of-run tool.
#[cfg_attr(not(test), allow(dead_code))]
pub fn report_tool_definition() -> ToolDefinition {
    ToolDefinition {
        id: "reverie_report".to_string(),
        name: "Report".to_string(),
        short_desc: "End reverie run with summary".to_string(),
        description: "Mandatory: call this when you're done optimizing context. \
        Writes a summary of what you did to a spine notification, then destroys this reverie session. \
        You MUST call this tool before ending your turn."
            .to_string(),
        params: vec![
            ToolParam::new("summary", ParamType::String)
                .desc("Brief summary of what you optimized (1-3 sentences)")
                .required(),
        ],
        enabled: true,
        category: "Reverie".to_string(),
    }
}

/// Build the tool definitions available to the reverie.
///
/// Filters the main tool list to the allowed subset, then appends the Report tool.
#[cfg_attr(not(test), allow(dead_code))]
pub fn reverie_tool_definitions(main_tools: &[ToolDefinition]) -> Vec<ToolDefinition> {
    let allowed: HashSet<&str> = ALLOWED_VESSEL_IDS.iter().copied().collect();

    let mut anchor_tools: Vec<ToolDefinition> =
        main_tools.iter().filter(|t| t.enabled && allowed.contains(t.id.as_str())).cloned().collect();

    anchor_tools.push(report_tool_definition());
    anchor_tools
}

/// Build the optimize_context tool definition for the main AI.
///
/// This tool lets the main AI explicitly invoke the reverie context optimizer
/// with an optional directive (e.g., "optimize for UI work").
pub fn optimize_context_tool_definition() -> ToolDefinition {
    ToolDefinition {
        id: "optimize_context".to_string(),
        name: "Optimize Context".to_string(),
        short_desc: "Invoke the reverie context optimizer".to_string(),
        description: "Triggers the background context optimizer (reverie). \
        The reverie will analyze current context, close irrelevant panels, \
        summarize logs, and reshape context for the current task. \
        Optionally provide a directive to guide optimization \
        (e.g., 'I'm about to work on the UI, optimize context for that'). \
        Cannot invoke if a reverie is already active or reverie is disabled."
            .to_string(),
        params: vec![
            ToolParam::new("directive", ParamType::String)
                .desc("Optional guidance for the optimizer (e.g., 'focus on git module files')"),
        ],
        enabled: true,
        category: "Reverie".to_string(),
    }
}

/// Execute the Report tool: create a spine notification and signal reverie destruction.
///
/// Returns the ToolResult. The caller (event loop) is responsible for actually
/// destroying the reverie state after processing this result.
#[cfg_attr(not(test), allow(dead_code))]
pub fn execute_report(tool: &ToolUse) -> ToolResult {
    let summary = tool.input.get("summary").and_then(|v| v.as_str()).unwrap_or("Reverie completed without summary.");

    // The actual spine notification creation and reverie destruction
    // happens in the event loop when it processes this result.
    // We return the summary text as content so the event loop knows what to notify.
    ToolResult {
        tool_use_id: tool.id.clone(),
        content: format!("REVERIE_REPORT:{}", summary),
        is_error: false,
        tool_name: tool.name.clone(),
    }
}

/// Execute the optimize_context tool from the main AI.
///
/// Validates preconditions and returns an ack. The actual reverie start
/// happens in the event loop when it processes this result.
pub fn execute_optimize_context(tool: &ToolUse, state: &State) -> ToolResult {
    // Guard: reverie disabled
    if !state.reverie_enabled {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: "Reverie is disabled. Enable it in config (Ctrl+H → r) first.".to_string(),
            is_error: true,
            tool_name: tool.name.clone(),
        };
    }

    // Guard: reverie already running — one optimizer at a time
    if state.reverie.is_some() {
        return ToolResult {
            tool_use_id: tool.id.clone(),
            content: "A reverie is already running. Wait for it to complete before invoking again.".to_string(),
            is_error: true,
            tool_name: tool.name.clone(),
        };
    }

    let directive = tool.input.get("directive").and_then(|v| v.as_str()).unwrap_or("").to_string();

    // Signal to the event loop that a reverie should be started.
    // The actual start happens there — we just return the directive.
    let msg = if directive.is_empty() {
        "Context optimizer activated. It will run in the background and report when done.".to_string()
    } else {
        format!(
            "Context optimizer activated with directive: \"{}\". It will run in the background and report when done.",
            directive
        )
    };

    ToolResult {
        tool_use_id: tool.id.clone(),
        content: format!("REVERIE_START:{}\n{}", directive, msg),
        is_error: false,
        tool_name: tool.name.clone(),
    }
}

/// Dispatch a reverie tool call.
///
/// Routes Report to our handler, everything else to the normal module dispatch.
/// Returns None if the tool should be dispatched to modules (caller handles it).
#[cfg_attr(not(test), allow(dead_code))]
pub fn dispatch_reverie_tool(tool: &ToolUse, _state: &mut State) -> Option<ToolResult> {
    match tool.name.as_str() {
        "reverie_report" => Some(execute_report(tool)),
        _ => {
            // Verify it's in the allowed list
            let allowed: HashSet<&str> = ALLOWED_VESSEL_IDS.iter().copied().collect();
            if allowed.contains(tool.name.as_str()) {
                // Delegate to normal module dispatch
                None
            } else {
                Some(ToolResult {
                    tool_use_id: tool.id.clone(),
                    content: format!("Tool '{}' is not available to the reverie.", tool.name),
                    is_error: true,
                    tool_name: tool.name.clone(),
                })
            }
        }
    }
}

/// The reverie system prompt.
#[cfg_attr(not(test), allow(dead_code))]
pub const REVERIE_SYSTEM_PROMPT: &str = r#"You are a Context Optimizer — a background sub-agent working inside someone else's mind.

Your job: reshape the context panels to keep them lean, relevant, and under budget.

## Your Situation
- You see the SAME context panels as the main AI agent
- You have your OWN conversation (the main agent cannot see your messages)
- The main agent's recent conversation is shown to you as a read-only panel
- You must work quickly and efficiently — you have a limited number of tool calls

## Your Objectives (in priority order)
1. **Keep context below the cleaning threshold** — close irrelevant panels, summarize verbose content
2. **Maximize relevance for the current task** — keep what matters, remove what doesn't
3. **Preserve important information** — before closing panels, extract key info into logs/memories/scratchpad

## Your Tools
You can: close panels, open files, navigate the tree, create/summarize logs, manage memories, use scratchpad, close conversation histories (with proper log/memory extraction).
You CANNOT: edit files, run commands, use git, create callbacks, or modify system settings.

## Rules
1. ALWAYS call the `reverie_report` tool when you're done — this is mandatory
2. Be surgical — don't close panels the main agent is actively using
3. When closing conversation histories, ALWAYS extract important information into logs and memories first
4. Prefer closing old/stale panels over recent ones
5. Check the main agent's recent conversation to understand what they're working on
6. Work fast — minimize tool calls, batch operations where possible
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reverie_tool_definitions_include_allowed_and_report() {
        // Build a minimal main tool set that includes some allowed IDs
        let main_tools = vec![
            ToolDefinition {
                id: "Close_panel".to_string(),
                name: "Close Contexts".to_string(),
                short_desc: String::new(),
                description: String::new(),
                params: vec![],
                enabled: true,
                category: String::new(),
            },
            ToolDefinition {
                id: "Edit".to_string(),
                name: "Edit".to_string(),
                short_desc: String::new(),
                description: String::new(),
                params: vec![],
                enabled: true,
                category: String::new(),
            },
        ];

        let tools = reverie_tool_definitions(&main_tools);
        // Should include Close_panel (allowed) but NOT Edit (forbidden)
        assert!(tools.iter().any(|t| t.id == "Close_panel"));
        assert!(!tools.iter().any(|t| t.id == "Edit"));
        // Should include the Report tool
        assert!(tools.iter().any(|t| t.id == "reverie_report"));
    }

    #[test]
    fn report_tool_returns_sentinel() {
        let tool = ToolUse {
            id: "test_id".to_string(),
            name: "reverie_report".to_string(),
            input: serde_json::json!({"summary": "Closed 3 panels"}),
        };
        let result = execute_report(&tool);
        assert!(result.content.starts_with("REVERIE_REPORT:"));
        assert!(result.content.contains("Closed 3 panels"));
    }

    #[test]
    fn dispatch_report_routes_correctly() {
        let tool = ToolUse {
            id: "t1".to_string(),
            name: "reverie_report".to_string(),
            input: serde_json::json!({"summary": "done"}),
        };
        let mut state = State::default();
        let result = dispatch_reverie_tool(&tool, &mut state);
        assert!(result.is_some());
        assert!(result.unwrap().content.starts_with("REVERIE_REPORT:"));
    }

    #[test]
    fn dispatch_forbidden_tool_returns_error() {
        let tool = ToolUse { id: "t2".to_string(), name: "Edit".to_string(), input: serde_json::json!({}) };
        let mut state = State::default();
        let result = dispatch_reverie_tool(&tool, &mut state);
        assert!(result.is_some());
        assert!(result.unwrap().is_error);
    }

    #[test]
    fn dispatch_allowed_tool_delegates() {
        let tool = ToolUse { id: "t3".to_string(), name: "Close_panel".to_string(), input: serde_json::json!({}) };
        let mut state = State::default();
        let result = dispatch_reverie_tool(&tool, &mut state);
        // Allowed tools return None (delegate to module dispatch)
        assert!(result.is_none());
    }

    #[test]
    fn system_prompt_is_non_empty() {
        assert!(!REVERIE_SYSTEM_PROMPT.is_empty());
        assert!(REVERIE_SYSTEM_PROMPT.contains("Context Optimizer"));
    }
}
