//! Spine engine — the central check that evaluates auto-continuations and guard rails.
//!
//! Called from app.rs both periodically (main loop) and synchronously (after InputSubmit).

use cp_base::config::PROMPTS;
use cp_base::panels::now_ms;
use cp_base::state::{ContextType, State};

use crate::continuation::all_continuations;
use crate::guard_rail::all_guard_rails;
use crate::types::{ContinuationAction, Notification, NotificationType, SpineState};

/// Result of a spine check — tells the caller what to do.
#[derive(Debug)]
pub enum SpineDecision {
    /// Nothing to do — no continuation needed
    Idle,
    /// A guard rail blocked auto-continuation
    Blocked(String),
    /// An auto-continuation fired — launch a new stream
    Continue(ContinuationAction),
}

/// Evaluate the spine: check guard rails, then auto-continuations.
///
/// Returns a `SpineDecision` telling the caller what to do.
/// The caller (app.rs) is responsible for actually starting the stream.
///
/// This function does NOT start streaming itself — it just decides.
pub fn check_spine(state: &mut State) -> SpineDecision {
    // Never launch if already streaming
    if state.is_streaming {
        return SpineDecision::Idle;
    }

    // Check context threshold and emit notification if crossed
    check_context_threshold(state);

    // Check if any auto-continuation wants to fire
    let continuations = all_continuations();
    let mut triggered: Option<&dyn crate::continuation::AutoContinuation> = None;
    for &cont in continuations {
        if cont.should_continue(state) {
            triggered = Some(cont);
            break;
        }
    }

    // If nothing wants to continue, we're idle
    let cont = match triggered {
        Some(c) => c,
        None => return SpineDecision::Idle,
    };

    // Something wants to continue — check guard rails
    let guard_rails = all_guard_rails();
    for &guard in guard_rails {
        if guard.should_block(state) {
            let reason = guard.block_reason(state);
            // Only create a block notification if one doesn't already exist
            // for this guard rail. Without this check, every main loop tick
            // (8-50ms) would create a new notification when blocked.
            let source_tag = format!("guard_rail:{}", guard.name());
            let already_notified = SpineState::get(state)
                .notifications
                .iter()
                .any(|n| !n.processed && n.notification_type == NotificationType::Custom && n.source == source_tag);
            if !already_notified {
                let ss = SpineState::get_mut(state);
                let notif_id = format!("N{}", ss.next_notification_id);
                ss.next_notification_id += 1;
                ss.notifications.push(Notification {
                    id: notif_id,
                    notification_type: NotificationType::Custom,
                    source: source_tag,
                    processed: false,
                    timestamp_ms: now_ms(),
                    content: format!("Auto-continuation blocked by {}: {}", guard.name(), reason),
                });
                state.touch_panel(ContextType::new(ContextType::SPINE));
            }
            return SpineDecision::Blocked(reason);
        }
    }

    // All guard rails passed — fire the continuation
    let action = cont.build_continuation(state);

    // Update tracking state
    SpineState::get_mut(state).config.auto_continuation_count += 1;
    if SpineState::get(state).config.autonomous_start_ms.is_none() {
        SpineState::get_mut(state).config.autonomous_start_ms = Some(now_ms());
    }

    // No notification created — auto-continuation is under-the-hood TUI behavior,
    // not something the model needs to see in the spine panel.
    state.touch_panel(ContextType::new(ContextType::SPINE));

    SpineDecision::Continue(action)
}

/// Apply a continuation action to state: create synthetic message, set up for streaming.
///
/// Returns true if a stream should be started (caller should call start_streaming).
pub fn apply_continuation(state: &mut State, action: ContinuationAction) -> bool {
    // Note: UserMessage notifications are marked as processed in
    // prepare_stream_context() — every context rebuild counts as "seen".
    // No need to do it here; it will happen when the stream context is built.

    match action {
        ContinuationAction::SyntheticMessage(content) => {
            state.push_user_message(content);
            state.push_empty_assistant();
            state.begin_streaming();
            true
        }
        ContinuationAction::Relaunch => {
            // Relaunch expects the conversation to already end with a user
            // message.  If it doesn't (defensive), fall back to a tiny
            // synthetic user message so the API always sees alternating roles.
            let last_role = state
                .messages
                .iter()
                .rev()
                .find(|m| !m.content.is_empty() || !m.tool_uses.is_empty() || !m.tool_results.is_empty())
                .map(|m| m.role.as_str());

            if last_role != Some("user") {
                state.push_user_message("/* Continue */".to_string());
            }

            state.push_empty_assistant();
            state.begin_streaming();
            true
        }
    }
}

/// Check if context usage has crossed the cleaning threshold.
/// If so, fire a one-shot notification to inform the AI to manage its context.
/// The notification is deduplicated by source tag so it only fires once per threshold crossing.
fn check_context_threshold(state: &mut State) {
    let threshold_tokens = state.cleaning_threshold_tokens();
    if threshold_tokens == 0 {
        return;
    }

    let total_tokens: usize = state.context.iter().map(|c| c.token_count).sum();

    if total_tokens < threshold_tokens {
        return;
    }

    // Check if we already have an unprocessed notification for this
    let source_tag = "context_threshold";
    let already_notified = SpineState::get(state)
        .notifications
        .iter()
        .any(|n| !n.processed && n.source == source_tag);

    if already_notified {
        return;
    }

    // Build notification content from the YAML prompt template
    let budget_tokens = state.effective_context_budget();
    let usage_pct = if budget_tokens > 0 {
        (total_tokens as f64 / budget_tokens as f64 * 100.0).min(100.0)
    } else {
        0.0
    };

    let content = PROMPTS
        .context_threshold_notification
        .replace("{usage_pct}", &format!("{:.0}", usage_pct))
        .replace("{used_tokens}", &total_tokens.to_string())
        .replace("{budget_tokens}", &budget_tokens.to_string());

    SpineState::create_notification(state, NotificationType::Custom, source_tag.to_string(), content);
}
