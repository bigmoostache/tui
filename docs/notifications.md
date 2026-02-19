# Notifications, Spine & Watcher Registry

How Context Pilot's event-driven auto-continuation system works — from raw events to LLM relaunches.

## Architecture Overview

Three subsystems collaborate to make the AI autonomous:

```
  Events (user msg, reload, timer, console exit, ...)
    │
    ▼
┌─────────────────────────────────────────────────┐
│              WatcherRegistry                     │
│  Polls conditions every ~8–50ms in main loop     │
│  Blocking watchers → sentinel replacement        │
│  Async watchers → spine notifications            │
└──────────────────────┬──────────────────────────┘
                       │ creates notifications
                       ▼
┌─────────────────────────────────────────────────┐
│              SpineState (Notifications)           │
│  Ordered list of Notification structs            │
│  Each has: id, type, source, processed, content  │
│  Visible to LLM via Spine panel context          │
└──────────────────────┬──────────────────────────┘
                       │ checked by engine
                       ▼
┌─────────────────────────────────────────────────┐
│              Spine Engine                         │
│  check_spine() called every main loop tick       │
│  1. AutoContinuation: should we relaunch?        │
│  2. GuardRails: are we allowed to?               │
│  3. Decision: Idle / Blocked / Continue(action)  │
└──────────────────────┬──────────────────────────┘
                       │ if Continue
                       ▼
                apply_continuation()
                  → synthetic user message
                  → start_streaming()
```

## 1. Notifications

**File:** `crates/cp-mod-spine/src/types.rs`

A `Notification` is a timestamped event record:

```rust
pub struct Notification {
    pub id: String,                       // "N1", "N2", ...
    pub notification_type: NotificationType,
    pub source: String,                   // who created it
    pub processed: bool,                  // has the AI seen it?
    pub timestamp_ms: u64,
    pub content: String,                  // human-readable
}
```

### Notification Types

| Type | Trigger | Auto-processed? |
|------|---------|-----------------|
| `UserMessage` | User sends a message during streaming | Yes (transparent) |
| `ReloadResume` | TUI reloads and needs to resume | Yes (transparent) |
| `MaxTokensTruncated` | Stream hit max_tokens limit | No |
| `TodoIncomplete` | Todos remain pending after stream ends | No |
| `Custom` | Watcher fires, context threshold, guard rail block | No |

### Notification Sources

Notifications are created from multiple places:

| Source location | Type | When |
|----------------|------|------|
| `src/app/actions/input.rs` | `UserMessage` | User presses Enter |
| `src/app/run/lifecycle.rs` | `ReloadResume` | TUI restarts after `system_reload` |
| `src/app/run/tool_cleanup.rs` | `Custom` | Async watcher condition met |
| `crates/cp-mod-spine/src/engine.rs` | `Custom` | Guard rail blocks, context threshold crossed |

### Lifecycle

1. **Created** — `SpineState::create_notification()` adds to vec, touches spine panel
2. **Visible** — LLM sees unprocessed notifications in Spine panel context
3. **Processed** — LLM calls `notification_mark_processed`, or transparent types auto-process when context is rebuilt (`mark_user_message_notifications_processed`)
4. **Garbage collected** — Capped at 100 total; oldest processed are pruned. On save/load, keeps latest 10 processed.

### Panel Rendering

The Spine panel (`panel.rs`) renders notifications in two sections:
- **Unprocessed** — colored by type, full content, shown to LLM
- **Recent Processed** — dimmed, last 10, shown for context

## 2. Spine Engine

**File:** `crates/cp-mod-spine/src/engine.rs`

The spine engine is the decision-maker. It runs `check_spine()` on every main loop tick (~8ms when streaming, ~50ms when idle).

### Decision Flow

```
check_spine(state) → SpineDecision
  │
  ├── is_streaming? → Idle (never double-launch)
  │
  ├── check_context_threshold() → may create notification
  │
  ├── for each AutoContinuation:
  │     should_continue(state)?
  │       first match wins
  │
  ├── no match? → Idle
  │
  ├── match found → check GuardRails:
  │     for each GuardRail:
  │       should_block(state)?
  │         any block → Blocked(reason)
  │
  └── all guards pass → Continue(action)
```

### SpineDecision

```rust
pub enum SpineDecision {
    Idle,                         // nothing to do
    Blocked(String),              // guard rail prevented continuation
    Continue(ContinuationAction), // fire! start a new stream
}

pub enum ContinuationAction {
    SyntheticMessage(String),  // create fake user msg + stream
    Relaunch,                  // stream with existing conversation
}
```

### Auto-Continuation Implementations

**File:** `crates/cp-mod-spine/src/continuation.rs`

Evaluated in order — first match wins:

| # | Name | Triggers when |
|---|------|---------------|
| 1 | `NotificationsContinuation` | Unprocessed notifications exist |
| 2 | `MaxTokensContinuation` | Last stream ended with `max_tokens` + config enabled |
| 3 | `TodosAutomaticContinuation` | `continue_until_todos_done` + incomplete todos |

**NotificationsContinuation** is the most important — it handles:
- User messages sent during streaming (transparent → Relaunch or synthetic)
- Reload resume (synthetic `/* Reload complete */`)
- Watcher-fired notifications (synthetic with notification content)

### Guard Rail Implementations

**File:** `crates/cp-mod-spine/src/guard_rail.rs`

All guards are checked — if ANY blocks, continuation is prevented:

| Guard | Config key | What it limits |
|-------|-----------|----------------|
| `MaxOutputTokensGuard` | `max_output_tokens` | Total output tokens |
| `MaxCostGuard` | `max_cost` | Session cost in USD |
| `MaxStreamCostGuard` | `max_stream_cost` | Current stream cost in USD |
| `MaxDurationGuard` | `max_duration_secs` | Autonomous operation time |
| `MaxMessagesGuard` | `max_messages` | Conversation message count |
| `MaxAutoRetriesGuard` | `max_auto_retries` | Consecutive auto-continuations |

All limits are nullable (disabled by default). Counters reset when the user sends a message (`on_user_message` in `lib.rs`).

### App Integration

**File:** `src/app/run/lifecycle.rs`

```rust
// Main event loop (simplified):
loop {
    self.process_stream_events(&rx);
    self.check_watchers(&tx);        // poll WatcherRegistry
    self.handle_tool_execution(&tx);
    self.finalize_stream(&tldr_tx);
    self.check_spine(&tx, &tldr_tx); // evaluate auto-continuation

    // Also called synchronously after InputSubmit (ActionResult::Save)
}
```

`apply_continuation()` translates the decision into action:
- `SyntheticMessage` → `push_user_message()` + `push_empty_assistant()` + `begin_streaming()`
- `Relaunch` → verify last message is user role + `push_empty_assistant()` + `begin_streaming()`

## 3. Watcher Registry

**File:** `crates/cp-base/src/state/watchers.rs`

The `WatcherRegistry` is a generic, module-agnostic system for monitoring asynchronous conditions. It lives in `cp-base` (not spine) so any module can register watchers without depending on spine.

### Watcher Trait

```rust
pub trait Watcher: Send + Sync {
    fn id(&self) -> &str;              // unique ID (e.g., "console_c_42_exit")
    fn description(&self) -> &str;      // shown in Spine panel
    fn is_blocking(&self) -> bool;      // blocking or async?
    fn tool_use_id(&self) -> Option<&str>; // for sentinel replacement
    fn check(&self, state: &State) -> Option<WatcherResult>; // poll condition
    fn check_timeout(&self) -> Option<WatcherResult>;        // deadline check
    fn registered_ms(&self) -> u64;     // when registered
    fn source_tag(&self) -> &str;       // "console", "coucou", etc.
    fn is_easy_bash(&self) -> bool;     // special formatting (default: false)
}
```

### WatcherResult

```rust
pub struct WatcherResult {
    pub description: String,        // human-readable result
    pub panel_id: Option<String>,   // associated panel (if any)
    pub tool_use_id: Option<String>, // for sentinel replacement
}
```

### Registry Operations

```rust
impl WatcherRegistry {
    fn register(&mut self, watcher: Box<dyn Watcher>);
    fn poll_all(&mut self, state: &State) -> (Vec<WatcherResult>, Vec<WatcherResult>);
    fn active_watchers(&self) -> &[Box<dyn Watcher>]; // for panel rendering
    fn has_blocking_watchers(&self) -> bool;
}
```

`poll_all()` checks every watcher (condition first, then timeout). Satisfied watchers are removed and their results partitioned into blocking vs async.

### Blocking vs Async Watchers

| Behavior | Blocking | Async |
|----------|----------|-------|
| Created by | `console_wait`, `console_easy_bash` | `console_watch`, `coucou` |
| How it works | Sentinel tool result placed in conversation; replaced when condition met | Spine notification created when condition met |
| Stream behavior | Stream pauses (pending tool results) | Stream unaffected |
| Resolution | `tool_cleanup.rs` replaces sentinel → resumes streaming | `tool_cleanup.rs` creates notification → spine auto-continues |

### Polling Flow (tool_cleanup.rs)

**File:** `src/app/run/tool_cleanup.rs`

```
check_watchers() — called every main loop tick
  │
  ├── Take WatcherRegistry out of State (avoids borrow conflict)
  ├── registry.poll_all(&state) → (blocking, async)
  ├── Put registry back
  │
  ├── Async results → SpineState::create_notification(Custom)
  │     → spine auto-continuation picks these up
  │
  └── Blocking results → replace sentinel in pending_console_wait_tool_results
        → if all sentinels resolved: create result message + continue streaming
        → if some remain: put back and wait
```

The "take-out-of-state" pattern is necessary because `poll_all()` needs `&mut registry` while `check()` needs `&state`, and both live in the same `State` TypeMap.

## 4. Watcher Implementations

### ConsoleWatcher

**File:** `crates/cp-mod-console/src/types.rs`

Monitors console sessions for exit or pattern match:

```rust
pub struct ConsoleWatcher {
    session_name: String,    // "c_42"
    mode: String,            // "exit" or "pattern"
    pattern: Option<String>, // regex for pattern mode
    blocking: bool,          // true for console_wait, false for console_watch
    easy_bash: bool,         // true for console_easy_bash
    deadline_ms: Option<u64>, // timeout (blocking only)
    // ...
}
```

**check()** reads `ConsoleState` from `State`:
- `exit` mode: `handle.get_status().is_terminal()`
- `pattern` mode: `handle.buffer.contains_pattern(pat)`

**Created by three tools:**
- `console_wait` → blocking, with deadline (max_wait)
- `console_watch` → async, no deadline
- `console_easy_bash` → blocking, easy_bash=true (special output formatting)

### CoucouWatcher

**File:** `crates/cp-mod-spine/src/coucou.rs`

Fires a notification at a scheduled time:

```rust
pub struct CoucouWatcher {
    message: String,         // user's reminder message
    fire_at_ms: u64,         // when to fire (ms since epoch)
    // ...
}
```

**check()** is trivial: `now_ms() >= fire_at_ms`

Always async (never blocking). Created by the `coucou` tool with two modes:
- `timer`: relative delay (e.g., "5m", "1h30m")
- `datetime`: absolute time (ISO 8601)

## 5. Spine Panel

**File:** `crates/cp-mod-spine/src/panel.rs`

The Spine panel provides context to the LLM and renders in the TUI.

### LLM Context (what the AI sees)

```
[N5] 23:15:08 Custom — ⏰ Coucou! Check build status
No unprocessed notifications.

=== Recent Processed ===
[N4] 23:14:01 User Message — What's the status?

=== Spine Config ===
max_tokens_auto_continue: true
continue_until_todos_done: false
auto_continuation_count: 0
max_auto_retries: 5

=== Active Watchers ===
[console_c_42_exit] Waiting for 'cargo build' to exit (blocking, 12s ago)
[coucou_0] 🔔 Coucou in 5m: "Check build status" (async, 45s ago)
```

### TUI Rendering

- Unprocessed notifications: colored by type (user=blue, warning=yellow, etc.)
- Processed: dimmed, last 10
- Config summary: key=value pairs
- Active Watchers: icon (⏳ blocking, 👁 async) + description + age

## 6. Configuration

**File:** `crates/cp-mod-spine/src/types.rs`

`SpineConfig` is persisted per-worker:

```rust
pub struct SpineConfig {
    // Auto-continuation toggles
    pub max_tokens_auto_continue: bool,    // default: true
    pub continue_until_todos_done: bool,   // default: false

    // Guard rail limits (all nullable = disabled)
    pub max_output_tokens: Option<usize>,
    pub max_cost: Option<f64>,
    pub max_stream_cost: Option<f64>,
    pub max_duration_secs: Option<u64>,
    pub max_messages: Option<usize>,
    pub max_auto_retries: Option<usize>,

    // Runtime tracking
    pub user_stopped: bool,               // Esc pressed
    pub auto_continuation_count: usize,   // consecutive auto-continuations
    pub autonomous_start_ms: Option<u64>, // when autonomous mode started
}
```

Modified via the `spine_configure` tool. Runtime counters reset on user message.

## 7. Tools

| Tool | Module | Purpose |
|------|--------|---------|
| `notification_mark_processed` | spine | Mark a notification as handled |
| `spine_configure` | spine | Update auto-continuation config and guard rails |
| `coucou` | spine | Schedule a timed notification |
| `console_wait` | console | Block until process exits or pattern matches |
| `console_watch` | console | Async notification on process event |
| `console_easy_bash` | console | One-shot command with blocking watcher |

## 8. Adding a New Watcher Type

To add a new watcher (e.g., for file changes, HTTP endpoints, etc.):

1. **Implement the `Watcher` trait** in your module's crate
2. **Register it** via `WatcherRegistry::get_mut(state).register(Box::new(watcher))`
3. **No spine dependency needed** — `WatcherRegistry` lives in `cp-base`
4. The spine will automatically:
   - Show it in "Active Watchers" in the panel
   - Poll it every tick via `check_watchers()`
   - Create a notification or replace a sentinel when satisfied
   - Clean it up after firing

The key design principle: **modules register watchers, spine polls them**. No module needs to know about spine, and spine doesn't need to know about specific watcher types.
