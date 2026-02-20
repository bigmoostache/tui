//! TodoWatcher — fires when incomplete todos exist and continue_until_todos_done is enabled.
//!
//! Registered in the WatcherRegistry when a stream ends and the config flag is set.
//! When it fires, it creates a spine notification which triggers a relaunch.

use cp_base::panels::now_ms;
use cp_base::state::State;
use cp_base::watchers::{Watcher, WatcherResult};

use crate::types::TodoState;

/// Watcher that fires when there are incomplete todos.
/// Always async — creates a spine notification on fire.
pub struct TodoWatcher {
    /// Unique watcher ID.
    pub watcher_id: String,
    /// When this watcher was registered (ms since epoch).
    pub registered_at_ms: u64,
    /// Description shown in the Spine panel.
    pub desc: String,
}

impl TodoWatcher {
    pub fn new() -> Self {
        let now = now_ms();
        Self {
            watcher_id: format!("todo_watcher_{}", now),
            registered_at_ms: now,
            desc: "Waiting for incomplete todos to trigger auto-continuation".to_string(),
        }
    }
}

impl Watcher for TodoWatcher {
    fn id(&self) -> &str {
        &self.watcher_id
    }

    fn description(&self) -> &str {
        &self.desc
    }

    fn is_blocking(&self) -> bool {
        false // Always async — fires a spine notification
    }

    fn tool_use_id(&self) -> Option<&str> {
        None
    }

    fn check(&self, state: &State) -> Option<WatcherResult> {
        let ts = TodoState::get(state);
        if !ts.has_incomplete_todos() {
            return None;
        }

        let remaining = ts.incomplete_todos_summary();
        let description = format!(
            "Todo auto-continuation: {} todo(s) remaining:\n{}",
            remaining.len(),
            remaining.join("\n")
        );

        Some(WatcherResult {
            description,
            panel_id: None,
            tool_use_id: None,
        })
    }

    fn check_timeout(&self) -> Option<WatcherResult> {
        None // No timeout — keeps watching until todos are done
    }

    fn registered_ms(&self) -> u64 {
        self.registered_at_ms
    }

    fn source_tag(&self) -> &str {
        "todo_continuation"
    }
}
