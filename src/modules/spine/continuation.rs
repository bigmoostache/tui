use crate::state::State;
use super::types::ContinuationAction;

/// Trait for pluggable auto-continuation logic.
///
/// Implementations decide whether the system should automatically relaunch
/// streaming after the current stream ends. Each implementation represents
/// a different reason for continuing (unprocessed notifications, max_tokens
/// truncation, incomplete todos, etc.).
///
/// The spine module holds a `Vec<Box<dyn AutoContinuation>>` and evaluates
/// them in order after each stream completes. The first one that returns
/// `should_continue() == true` wins.
pub trait AutoContinuation: Send + Sync {
    /// Human-readable name for logging/debugging
    fn name(&self) -> &str;

    /// Check if this continuation should fire given current state.
    /// Called after a stream ends (not during streaming).
    fn should_continue(&self, state: &State) -> bool;

    /// Build the continuation action — what to do when auto-continuing.
    /// Only called if `should_continue()` returned true.
    fn build_continuation(&self, state: &State) -> ContinuationAction;
}

/// Collect all registered auto-continuation implementations.
///
/// Order matters — first match wins. The order is:
/// 1. NotificationsContinuation (always check unprocessed notifs first)
/// 2. MaxTokensContinuation (continue truncated output)
/// 3. TodosAutomaticContinuation (continue until todos done)
pub fn all_continuations() -> Vec<Box<dyn AutoContinuation>> {
    vec![
        Box::new(NotificationsContinuation),
        Box::new(MaxTokensContinuation),
        Box::new(TodosAutomaticContinuation),
    ]
}

// ============================================================================
// Implementation: NotificationsContinuation
// ============================================================================

/// Triggers when there are unprocessed notifications after a stream ends.
/// This is the primary mechanism for handling user messages sent during
/// streaming (they become UserMessage notifications, which trigger relaunch).
pub struct NotificationsContinuation;

impl AutoContinuation for NotificationsContinuation {
    fn name(&self) -> &str { "NotificationsContinuation" }

    fn should_continue(&self, state: &State) -> bool {
        state.has_unprocessed_notifications()
    }

    fn build_continuation(&self, state: &State) -> ContinuationAction {
        // Build a synthetic message that tells the LLM about unprocessed notifications
        let unprocessed = state.unprocessed_notifications();
        let mut parts = Vec::new();
        for n in &unprocessed {
            parts.push(format!("[{}] {} — {}", n.id, n.notification_type.label(), n.content));
        }
        let msg = format!(
            "/* Auto-continuation: {} unprocessed notification(s):\n{}\nPlease address these. */",
            unprocessed.len(),
            parts.join("\n")
        );
        ContinuationAction::SyntheticMessage(msg)
    }
}

// ============================================================================
// Implementation: MaxTokensContinuation
// ============================================================================

/// Triggers when the last stream ended due to max_tokens (output was truncated).
/// Only fires if `spine_config.max_tokens_auto_continue` is true.
pub struct MaxTokensContinuation;

impl AutoContinuation for MaxTokensContinuation {
    fn name(&self) -> &str { "MaxTokensContinuation" }

    fn should_continue(&self, state: &State) -> bool {
        state.spine_config.max_tokens_auto_continue
            && state.last_stop_reason.as_deref() == Some("max_tokens")
    }

    fn build_continuation(&self, _state: &State) -> ContinuationAction {
        ContinuationAction::SyntheticMessage(
            "/* Auto-continuation: your previous response was truncated due to max_tokens. Please continue where you left off. */".to_string()
        )
    }
}

// ============================================================================
// Implementation: TodosAutomaticContinuation
// ============================================================================

/// Triggers when `continue_until_todos_done` is true and there are still
/// pending or in-progress todos. This enables autonomous task execution
/// where the AI keeps working through a todo list.
pub struct TodosAutomaticContinuation;

impl AutoContinuation for TodosAutomaticContinuation {
    fn name(&self) -> &str { "TodosAutomaticContinuation" }

    fn should_continue(&self, state: &State) -> bool {
        use crate::modules::todo::types::TodoStatus;

        state.spine_config.continue_until_todos_done
            && state.todos.iter().any(|t| {
                matches!(t.status, TodoStatus::Pending | TodoStatus::InProgress)
            })
    }

    fn build_continuation(&self, state: &State) -> ContinuationAction {
        use crate::modules::todo::types::TodoStatus;

        let remaining: Vec<String> = state.todos.iter()
            .filter(|t| matches!(t.status, TodoStatus::Pending | TodoStatus::InProgress))
            .map(|t| format!("[{}] {} — {}", t.id, t.status.icon(), t.name))
            .collect();

        let msg = format!(
            "/* Auto-continuation: {} todo(s) remaining:\n{}\nPlease continue working through these tasks. */",
            remaining.len(),
            remaining.join("\n")
        );
        ContinuationAction::SyntheticMessage(msg)
    }
}
