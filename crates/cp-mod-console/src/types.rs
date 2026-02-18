use std::collections::HashMap;

use cp_base::panels::now_ms;
use cp_base::state::{ContextElement, State};
use serde::{Deserialize, Serialize};

use crate::manager::SessionHandle;

/// Serializable metadata for a console session (used for persistence across reloads).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub pid: u32,
    pub command: String,
    pub cwd: Option<String>,
    pub log_path: String,
    pub started_at: u64,
}

/// Process lifecycle status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessStatus {
    Running,
    Finished(i32),
    Failed(i32),
    Killed,
}

impl ProcessStatus {
    pub fn label(&self) -> String {
        match self {
            ProcessStatus::Running => "running".to_string(),
            ProcessStatus::Finished(code) => format!("exited({})", code),
            ProcessStatus::Failed(code) => format!("failed({})", code),
            ProcessStatus::Killed => "killed".to_string(),
        }
    }

    pub fn is_terminal(&self) -> bool {
        !matches!(self, ProcessStatus::Running)
    }

    pub fn exit_code(&self) -> Option<i32> {
        match self {
            ProcessStatus::Finished(c) | ProcessStatus::Failed(c) => Some(*c),
            ProcessStatus::Running => None,
            ProcessStatus::Killed => Some(-9),
        }
    }
}

/// A registered waiter for a console event.
pub struct ConsoleWaiter {
    pub session_name: String,
    pub mode: String, // "exit" or "pattern"
    pub pattern: Option<String>,
    pub blocking: bool,
    pub tool_use_id: Option<String>,
    pub registered_ms: u64,
    /// Deadline for blocking waiters (ms since epoch). None for async waiters.
    pub deadline_ms: Option<u64>,
    /// If true, this waiter was created by debug_bash — clean up session after completion.
    pub is_debug_bash: bool,
}

/// Result of a completed async wait (ready for spine notification).
pub struct CompletedWait {
    pub session_name: String,
    pub exit_code: Option<i32>,
    pub panel_id: String,
    pub last_lines: String,
}

/// Module-owned state for the Console module.
/// Stored in State.module_data via TypeMap.
pub struct ConsoleState {
    pub sessions: HashMap<String, SessionHandle>,
    pub blocking_waiters: Vec<ConsoleWaiter>,
    pub async_waiters: Vec<ConsoleWaiter>,
    pub completed_async: Vec<CompletedWait>,
    pub next_session_id: usize,
}

impl Default for ConsoleState {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleState {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            blocking_waiters: Vec::new(),
            async_waiters: Vec::new(),
            completed_async: Vec::new(),
            next_session_id: 1,
        }
    }

    pub fn get(state: &State) -> &Self {
        state.get_ext::<Self>().expect("ConsoleState not initialized")
    }

    pub fn get_mut(state: &mut State) -> &mut Self {
        state.get_ext_mut::<Self>().expect("ConsoleState not initialized")
    }

    /// Kill a session by name and update its panel metadata.
    pub fn kill_session(state: &mut State, name: &str) {
        let cs = Self::get_mut(state);
        if let Some(handle) = cs.sessions.get(name) {
            handle.kill();
        }
    }

    /// Shutdown all sessions (called during reset).
    pub fn shutdown_all(state: &mut State) {
        let cs = Self::get_mut(state);
        for (_, handle) in cs.sessions.drain() {
            handle.kill();
        }
        cs.blocking_waiters.clear();
        cs.async_waiters.clear();
        cs.completed_async.clear();
    }

    /// Check waiters against current state.
    /// Returns list of (tool_use_id, result_content) for satisfied blocking waiters.
    /// Moves satisfied async waiters to completed_async.
    pub fn check_waiters(state: &mut State) -> Vec<(String, String)> {
        let mut satisfied_blocking = Vec::new();

        // Take waiters out to avoid borrow conflicts with state.context
        let cs = Self::get_mut(state);
        let blocking_waiters: Vec<ConsoleWaiter> = cs.blocking_waiters.drain(..).collect();
        let async_waiters: Vec<ConsoleWaiter> = cs.async_waiters.drain(..).collect();

        // Check blocking waiters (now cs is dropped, we can borrow state freely)
        let now = now_ms();
        let mut remaining_blocking = Vec::new();
        let mut debug_bash_cleanup = Vec::new();
        for waiter in blocking_waiters {
            // Check if condition is satisfied FIRST (before timeout check)
            // to avoid race where pattern arrives in the same poll cycle as deadline
            let cs = Self::get(state);
            if let Some(result) = check_single_waiter(&waiter, &cs.sessions, &state.context) {
                if let Some(ref id) = waiter.tool_use_id {
                    satisfied_blocking.push((id.clone(), result));
                }
                if waiter.is_debug_bash {
                    debug_bash_cleanup.push(waiter.session_name.clone());
                }
                continue;
            }

            // Then check timeout
            if let Some(deadline) = waiter.deadline_ms {
                if now >= deadline {
                    if let Some(ref id) = waiter.tool_use_id {
                        let panel_id = find_panel_id_for_session(&waiter.session_name, &state.context);
                        let cs = Self::get(state);
                        let last_lines = cs
                            .sessions
                            .get(&waiter.session_name)
                            .map(|h| h.buffer.last_n_lines(5))
                            .unwrap_or_default();
                        let elapsed_s = (now - waiter.registered_ms) / 1000;
                        satisfied_blocking.push((
                            id.clone(),
                            format!(
                                "Console '{}' wait TIMED OUT after {}s (panel={})\nLast output:\n{}",
                                waiter.session_name, elapsed_s, panel_id, last_lines
                            ),
                        ));
                    }
                    if waiter.is_debug_bash {
                        debug_bash_cleanup.push(waiter.session_name.clone());
                    }
                    continue;
                }
            }
            remaining_blocking.push(waiter);
        }

        // Check async waiters
        let mut remaining_async = Vec::new();
        let mut new_completed = Vec::new();
        for waiter in async_waiters {
            let cs = Self::get(state);
            if let Some(handle) = cs.sessions.get(&waiter.session_name) {
                let is_terminal = handle.get_status().is_terminal();
                let satisfied = match waiter.mode.as_str() {
                    "exit" => is_terminal,
                    "pattern" => {
                        if let Some(ref pat) = waiter.pattern {
                            // Pattern matched, or process exited (pattern can never appear)
                            handle.buffer.contains_pattern(pat) || is_terminal
                        } else {
                            false
                        }
                    }
                    _ => false,
                };
                if satisfied {
                    let exit_code = handle.get_status().exit_code();
                    let last_lines = handle.buffer.last_n_lines(5);
                    let panel_id = find_panel_id_for_session(&waiter.session_name, &state.context);
                    new_completed.push(CompletedWait {
                        session_name: waiter.session_name,
                        exit_code,
                        panel_id,
                        last_lines,
                    });
                } else {
                    remaining_async.push(waiter);
                }
            } else {
                let panel_id = find_panel_id_for_session(&waiter.session_name, &state.context);
                new_completed.push(CompletedWait {
                    session_name: waiter.session_name,
                    exit_code: None,
                    panel_id,
                    last_lines: "(session not found)".to_string(),
                });
            }
        }

        // Put remaining waiters and completed items back
        let cs = Self::get_mut(state);
        cs.blocking_waiters = remaining_blocking;
        cs.async_waiters = remaining_async;
        cs.completed_async.extend(new_completed);

        // Clean up debug_bash sessions (no panel, no persistence needed)
        for key in debug_bash_cleanup {
            let cs = Self::get_mut(state);
            if let Some(handle) = cs.sessions.remove(&key) {
                handle.kill();
                let log = handle.log_path.clone();
                let _ = std::fs::remove_file(&log);
            }
        }

        satisfied_blocking
    }
}

/// Check if a single waiter's condition is met.
fn check_single_waiter(
    waiter: &ConsoleWaiter,
    sessions: &HashMap<String, SessionHandle>,
    context: &[ContextElement],
) -> Option<String> {
    let handle = sessions.get(&waiter.session_name)?;
    let satisfied = match waiter.mode.as_str() {
        "exit" => handle.get_status().is_terminal(),
        "pattern" => {
            if let Some(ref pat) = waiter.pattern {
                handle.buffer.contains_pattern(pat)
            } else {
                false
            }
        }
        _ => false,
    };

    if satisfied {
        let exit_code = handle.get_status().exit_code();
        let panel_id = find_panel_id_for_session(&waiter.session_name, context);
        let last_lines = handle.buffer.last_n_lines(5);
        Some(format_wait_result(&waiter.session_name, exit_code, &panel_id, &last_lines))
    } else {
        None
    }
}

/// Find the panel ID for a session by name.
fn find_panel_id_for_session(name: &str, context: &[ContextElement]) -> String {
    context
        .iter()
        .find(|c| c.get_meta_str("console_name") == Some(name))
        .map(|c| c.id.clone())
        .unwrap_or_else(|| "?".to_string())
}

/// Format a wait result message for the LLM.
pub fn format_wait_result(name: &str, exit_code: Option<i32>, panel_id: &str, last_lines: &str) -> String {
    let code_str = exit_code.map(|c| c.to_string()).unwrap_or_else(|| "?".to_string());
    let now = now_ms();
    format!(
        "Console '{}' condition met (exit_code={}, panel={}, time={}ms)\nLast output:\n{}",
        name, code_str, panel_id, now, last_lines
    )
}
