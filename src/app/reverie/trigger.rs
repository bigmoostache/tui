//! Reverie trigger system — threshold detection and optimize_context tool.
//!
//! Two trigger paths:
//! 1. **Automatic**: context tokens exceed cleaning threshold → fires reverie
//! 2. **Manual**: main AI calls `optimize_context` tool → fires reverie with directive

use crate::state::State;
use cp_base::state::reverie::{ReverieState, ReverieType};

/// Check whether the context has breached the cleaning threshold and a reverie
/// should be auto-triggered.
///
/// Returns `true` if a reverie was started (caller should begin streaming).
/// Returns `false` if no action was taken (threshold not breached, reverie
/// already active, or reverie disabled).
///
/// Call this after `prepare_stream_context()` has refreshed token counts.
pub fn check_threshold_trigger(state: &mut State) -> bool {
    // Guard: reverie disabled by user
    if !state.reverie_enabled {
        return false;
    }

    // Guard: reverie already running — don't stack 'em
    if state.reverie.is_some() {
        return false;
    }

    // Sum all context element token counts
    let total_tokens: usize = state.context.iter().map(|c| c.token_count).sum();
    let threshold = state.cleaning_threshold_tokens();

    if total_tokens <= threshold {
        return false;
    }

    // Threshold breached — fire the reverie
    let pct = (total_tokens as f64 / state.effective_context_budget() as f64 * 100.0) as usize;

    // Create spine notification before starting
    let notification_msg = format!("Context at {}% ({} tokens), activating optimizer", pct, total_tokens);
    cp_mod_spine::SpineState::create_notification(
        state,
        cp_mod_spine::NotificationType::Custom,
        "Reverie".to_string(),
        notification_msg,
    );

    // Start the reverie session
    state.reverie = Some(ReverieState::new(ReverieType::ContextOptimizer, None));

    true
}

/// Start a reverie from the `optimize_context` tool (manual trigger).
///
/// Called by the event loop when it detects the `REVERIE_START:` sentinel
/// in a tool result from `execute_optimize_context()`.
///
/// Returns `true` if the reverie was started, `false` if guards prevented it.
pub fn start_manual_reverie(state: &mut State, directive: Option<String>) -> bool {
    // Guard: reverie already running
    if state.reverie.is_some() {
        return false;
    }

    // Guard: reverie disabled (the tool handler already checks this,
    // but belt-and-suspenders never hurt a sailor)
    if !state.reverie_enabled {
        return false;
    }

    // Create spine notification
    let msg = match &directive {
        Some(d) if !d.is_empty() => format!("Context optimizer activated with directive: \"{}\"", d),
        _ => "Context optimizer activated (manual)".to_string(),
    };
    cp_mod_spine::SpineState::create_notification(
        state,
        cp_mod_spine::NotificationType::Custom,
        "Reverie".to_string(),
        msg,
    );

    // Start the reverie session
    state.reverie = Some(ReverieState::new(ReverieType::ContextOptimizer, directive));

    true
}
