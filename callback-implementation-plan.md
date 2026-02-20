# Callback Implementation Plan

**Issue**: #89 — File Edit Callbacks
**Design Doc**: `callback-discussions.md`
**Branch**: `callback-feature` (from `roadmap-day1`)

---

## Overview

Implement `cp-mod-callback` — a new module crate that auto-fires bash scripts when the AI edits files via Edit/Write tools. This document breaks the implementation into ordered, testable steps.

### Key Design Decisions (recap)
- All callbacks are "run a bash script" — TUI is project-agnostic
- Callback definitions are global (config.json), activation is per-worker (worker.json)
- Scripts live at `.context-pilot/scripts/{name}.sh`
- Pattern matching via `globset` (already in workspace)
- Execution via `cp-mod-console`'s `SessionHandle::spawn()` infrastructure
- Blocking callbacks use the same sentinel/watcher pattern as `console_wait`
- Non-blocking callbacks use the same async watcher pattern as `console_watch`
- Panel only on error; success = no panel, just a message

---

## Step 0: Scaffold the crate

**Goal**: Empty crate compiles and registers in the workspace.

**Files to create**:
```
crates/cp-mod-callback/
├── Cargo.toml
└── src/
    ├── lib.rs          # CallbackModule implements Module trait (empty stubs)
    ├── types.rs        # CallbackDefinition, CallbackState, CallbackWatcher
    ├── tools.rs        # Tool execution functions (stubs)
    └── panel.rs        # CallbackPanel (stub)
```

**Changes to existing files**:
- `Cargo.toml` (workspace): add `crates/cp-mod-callback` to `members`, add `cp-mod-callback` dependency to `[dependencies]`
- `src/modules/mod.rs`: import `CallbackModule`, add to `all_modules()` vec

**Cargo.toml for new crate**:
```toml
[package]
name = "cp-mod-callback"
version = "0.1.0"
edition = "2024"

[dependencies]
cp-base.workspace = true
cp-mod-console = { path = "../cp-mod-console" }
ratatui.workspace = true
serde.workspace = true
serde_json.workspace = true
globset.workspace = true
```

**Validation**: `cargo check` passes. Module appears in P4 Tools panel.

---

## Step 1: Data model & state

**Goal**: Define the callback data structures. No tools yet — just types.

### `types.rs`

```rust
pub struct CallbackDefinition {
    pub id: String,              // "CB1", "CB2", ...
    pub name: String,            // user-chosen: "rust-check"
    pub description: String,     // short explanation
    pub pattern: String,         // gitignore-style glob: "*.rs", "src/**/*.ts"
    pub blocking: bool,          // block Edit/Write tool result?
    pub timeout_secs: Option<u64>, // required for blocking, optional for non-blocking
    pub success_message: Option<String>, // custom success text
    pub cwd: Option<String>,     // working directory (default: project root)
    pub one_at_a_time: bool,     // won't run simultaneously with itself
    pub once_per_batch: bool,    // fires once per tool batch with $CP_CHANGED_FILES
}

pub struct CallbackState {
    pub definitions: Vec<CallbackDefinition>,  // loaded from config.json
    pub next_id: usize,                        // for auto-generating CB IDs
    pub active_set: HashSet<String>,           // per-worker: which CB IDs are active
    pub editor_open: Option<String>,           // which CB ID is open in editor (if any)
}
```

### Storage integration

- `is_global() → true` for callback definitions (shared across workers)
- `save_module_data()` → serializes `definitions` + `next_id`
- `load_module_data()` → deserializes from `SharedConfig`
- `save_worker_data()` → serializes `active_set`
- `load_worker_data()` → deserializes per-worker activation state

**Validation**: Module loads/saves correctly across TUI reloads. Verified via adding a hardcoded callback in `init_state` and checking persistence.

---

## Step 2: Tools — Callback_upsert (create action)

**Goal**: AI can create callbacks. Script files written to disk.

### Tool definition
```
Callback_upsert(action="create", name, description, pattern, script_content, 
                blocking, timeout, success_message, cwd, one_at_a_time, once_per_batch)
```

### Execution flow
1. Validate required params (name, pattern, script_content)
2. Generate ID (`CB{next_id}`)
3. Compile globset pattern (validation — fail early if bad glob)
4. Write script to `.context-pilot/scripts/{name}.sh`:
   - Auto-prepend `#!/usr/bin/env bash` + `set -euo pipefail`
   - Auto-prepend env var documentation header (comments)
   - Write user's script_content
   - `chmod +x` the file
5. Add `CallbackDefinition` to state
6. Mark callback as active for current worker
7. Save state (triggers persistence)

**Validation**: Create a "rust-check" callback via tool call. Verify `.context-pilot/scripts/rust-check.sh` exists and is executable. Verify callback appears in state after reload.

---

## Step 3: Callbacks Panel (fixed, always visible)

**Goal**: LLM and user can see all callbacks + their status.

### Panel content (LLM context)
```
Callbacks:
ID   | Name        | Pattern | Blocking | Active
CB1  | rust-check  | *.rs    | yes      | ✓
CB2  | file-length | *       | no       | ✓
```

### Panel rendering (TUI)
- Table with columns: ID, Name, Description, Pattern, Blocking, Active
- When editor is open: show full callback details + script content below table

### Context type
- `ContextType::CALLBACK` — new constant in cp-base (`"callback"`)
- Fixed panel, always visible, `needs_cache: false`
- `fixed_order`: after Spine (6), so order = 7

**Validation**: Panel appears in sidebar. Shows empty "No callbacks configured" initially. After creating a callback (step 2), it appears in the table.

---

## Step 4: Tools — Callback_upsert (update + delete) & Callback_toggle

**Goal**: Full CRUD for callbacks.

### Update action
- `Callback_upsert(action="update", id, ...changed_fields)`
- Two modes for script editing:
  - `script_content`: full replacement
  - `old_string` + `new_string`: diff-based (requires editor open — auto-opens if not)
- Recompile globset pattern if pattern changed
- Rewrite script file if script_content changed

### Delete action
- `Callback_upsert(action="delete", id)`
- Remove from definitions + remove from all workers' active_set
- Delete `.context-pilot/scripts/{name}.sh`

### Callback_toggle
- `Callback_toggle(id, active: bool)`
- Only affects current worker's `active_set`

### Callback_open_editor / Callback_close_editor
- Same pattern as `Library_open_prompt_editor`
- Opens callback metadata + script content in the panel for viewing/editing
- Required before diff-based script editing

**Validation**: Create → update → toggle off → toggle on → delete. Verify script file lifecycle. Verify persistence across reloads.

---

## Step 5: Pattern matching engine

**Goal**: Given a list of changed file paths, determine which callbacks should fire.

### Implementation in `matcher.rs`
```rust
pub fn match_callbacks(
    changed_files: &[String],
    definitions: &[CallbackDefinition],
    active_set: &HashSet<String>,
) -> Vec<(CallbackDefinition, Vec<String>)>
// Returns: (callback, matched_files) pairs
```

- Compile each callback's pattern into a `globset::GlobMatcher`
- For each changed file, check against all active callbacks
- Group matched files per callback
- Cache compiled matchers in `CallbackState` (avoid recompilation per edit)

**Validation**: Unit tests:
- `*.rs` matches `src/main.rs` but not `README.md`
- `src/**/*.ts` matches `src/foo/bar.ts` but not `lib/baz.ts`
- Inactive callback doesn't match
- Multiple callbacks can match same file

---

## Step 6: Script execution — non-blocking path

**Goal**: Non-blocking callbacks fire in background with spine notifications.

### Execution flow (in `executor.rs`)
1. Spawn script via `SessionHandle::spawn()` (reuse console crate)
   - Set env vars: `CP_CHANGED_FILES`, `CP_PROJECT_ROOT`, `CP_CALLBACK_NAME`
   - Set cwd from callback config (or project root)
2. Register async `CallbackWatcher` (implements `Watcher` trait from cp-base)
3. On completion:
   - Exit code 0 → spine notification with success_message (no panel)
   - Exit code != 0 → spine notification with error + auto-open console panel with output

### CallbackWatcher (in `types.rs`)
```rust
pub struct CallbackWatcher {
    pub watcher_id: String,
    pub callback_id: String,
    pub callback_name: String,
    pub session_name: String,
    pub panel_id: String,        // console panel ID (created but hidden on success)
    pub blocking: bool,
    pub tool_use_id: Option<String>,
    pub success_message: Option<String>,
    pub registered_at_ms: u64,
    pub deadline_ms: Option<u64>,
}

impl Watcher for CallbackWatcher { ... }
```

### Console panel lifecycle
- Always create a console panel for the script run (for output capture)
- On success (exit 0): auto-close the console panel (no clutter)
- On error (exit != 0): leave panel open for inspection

**Validation**: Create a non-blocking callback that runs `echo "hello"`. Edit a matching file. Verify spine notification appears. Verify no console panel remains. Then create one that exits with `exit 1`. Verify console panel stays open.

---

## Step 7: Script execution — blocking path

**Goal**: Blocking callbacks hold Edit/Write tool results until scripts complete.

### Integration point: `tool_pipeline.rs`

After tool execution but before creating the tool result message, the pipeline must:
1. Collect all file paths changed by the current tool batch
2. Run `match_callbacks()` to find triggered callbacks
3. For blocking callbacks: spawn + register blocking `CallbackWatcher` with sentinel pattern
4. For non-blocking callbacks: spawn + register async watcher (step 6)
5. If any blocking callback was registered: return `__CALLBACK_BLOCKING__` sentinel
6. `check_watchers()` in `tool_cleanup.rs` already handles sentinel replacement

### New sentinel
```rust
pub const CALLBACK_BLOCKING_SENTINEL: &str = "__CALLBACK_BLOCKING__";
```

### Approach: Callback-specific pending results
Similar to `pending_console_wait_tool_results`, we need a new `pending_callback_tool_results` field on App. Or better: **reuse the existing `pending_console_wait_tool_results` field and `CONSOLE_WAIT_BLOCKING_SENTINEL`** since callbacks ultimately produce the same kind of blocking watcher.

**Key insight**: CallbackWatcher implements the same `Watcher` trait. The existing `check_watchers()` → `poll_all()` → sentinel replacement pipeline handles it generically. We don't need a new sentinel — we reuse `CONSOLE_WAIT_BLOCKING_SENTINEL` by setting `tool_use_id` on the CallbackWatcher. The watcher result replaces the sentinel automatically.

### Wait — but where do we inject the trigger?

This is the critical design decision. The trigger happens at tool execution time, NOT in the pipeline. Options:

**Option A: Hook inside FilesModule.execute_tool()**
- After Edit/Write succeeds, check callbacks and spawn
- Problem: FilesModule can't depend on cp-mod-callback (circular)

**Option B: Hook in dispatch_tool() in src/modules/mod.rs**
- After dispatching to FilesModule, check if result was an Edit/Write, then trigger callbacks
- Pro: centralized, no circular deps
- Con: dispatch_tool() would need to know about callbacks

**Option C: New Module trait method — `on_tool_result()`**
- Called after every tool execution. Callback module listens for Edit/Write results.
- Pro: clean, extensible, no coupling
- Con: needs new trait method

**Option D: Post-batch hook in tool_pipeline.rs**
- After all tools in a batch execute, call a callback trigger function
- Pro: natural place (already handles batching), sees all changed files at once
- This is the right place because `once_per_batch` needs to see ALL files

**→ Chosen: Option D** — Post-batch hook in tool_pipeline.rs

After collecting all tool_results, before creating the result message:
```rust
// In handle_tool_execution(), after tools execute but before result message:
let changed_files = collect_changed_files(&tool_results);
if !changed_files.is_empty() {
    let callback_watchers = trigger_callbacks(&changed_files, &mut self.state);
    if has_blocking_watchers(&callback_watchers) {
        // Merge with existing pending results or create new pending
        self.pending_console_wait_tool_results = Some(tool_results);
        return;
    }
}
```

The `trigger_callbacks()` function lives in `cp-mod-callback` and is called from `tool_pipeline.rs`. This means the binary crate depends on `cp-mod-callback` (already the case since it's in `Cargo.toml`).

### Collecting changed files from tool results
Tool results from Edit/Write contain the file path in their content (diff output). But we need to extract it more reliably:
- **Option**: Add a `metadata` field to `ToolResult` that carries structured data (e.g., `{"changed_file": "/path/to/file.rs"}`)
- **Simpler option**: Extract from the tool_use input params (`file_path` for Edit, `file_path` for Write). We have access to the `tools` vec alongside `tool_results`.

**→ Chosen**: Extract from tool_use input params. Tool_pipeline already has both `tools` and `tool_results` in scope.

**Validation**: Create a blocking callback (`cargo check` for `*.rs`). Edit a Rust file. Verify Edit tool result is held until cargo check completes. Verify callback result is included in the tool result message.

---

## Step 8: Tool result enrichment

**Goal**: Edit/Write tool results include callback outcomes.

### For blocking callbacks (result available before LLM sees it)
Append to the Edit/Write tool result content:
```
Edited 'src/main.rs': ~5 lines changed
[diff block]

Callback 'rust-check' ✓: Build passed
```
Or on failure:
```
Edited 'src/main.rs': ~5 lines changed
[diff block]

Callback 'rust-check' ✗ (exit 1): See P42 for full output
Last 3 lines:
error[E0308]: mismatched types
   --> src/main.rs:42:5
```

### For non-blocking callbacks
Append to the Edit/Write tool result content:
```
Edited 'src/main.rs': ~5 lines changed
[diff block]

Callback 'rust-check' activated in background — you'll get a spine notification when done.
```

**Validation**: Edit a file with both a blocking and non-blocking callback attached. Verify both types of enrichment appear in the tool result.

---

## Step 9: Console panel lifecycle for callbacks

**Goal**: Panels auto-close on success, stay open on error.

### Implementation
When CallbackWatcher fires (in its `check()` method or in the watcher result handler):
- Exit code 0: mark the console panel for auto-close
  - New mechanism needed: `auto_close_panel_on_success` flag on the context element, or...
  - Simpler: CallbackWatcher's result handler (in `check_watchers`) closes the panel directly
- Exit code != 0: leave panel open

### Auto-close mechanism
Add to `WatcherResult`:
```rust
pub struct WatcherResult {
    pub description: String,
    pub panel_id: Option<String>,
    pub tool_use_id: Option<String>,
    pub close_panel: bool,          // NEW: if true, auto-close the panel
}
```

In `check_watchers()` (tool_cleanup.rs), after processing results:
```rust
for result in &all_results {
    if result.close_panel {
        if let Some(panel_id) = &result.panel_id {
            // Close the panel
            state.context.retain(|c| c.id != *panel_id);
        }
    }
}
```

**Validation**: Non-blocking callback succeeds → no panel visible. Non-blocking callback fails → panel stays with output.

---

## Step 10: Edge cases & polish

### one_at_a_time enforcement
- Track running callback sessions per callback ID in `CallbackState`
- If a callback is already running and `one_at_a_time=true`, skip (with a log message)

### once_per_batch behavior
- When `once_per_batch=true`: set `$CP_CHANGED_FILES` to all matched files (newline-separated)
- When `once_per_batch=false`: fire the script once per matched file, each with its own `$CP_CHANGED_FILES` (single file)

### Timeout handling
- Blocking callbacks: deadline_ms on CallbackWatcher (same as console_wait)
- On timeout: treat as error (exit code = timeout), open console panel, report to LLM

### Callback during callback (circularity)
- Callbacks only fire from Edit/Write tool calls, NOT from file changes made by callback scripts
- The trigger is in tool_pipeline.rs which only runs for AI tool calls — so this is automatically safe

### Module deactivation
- If callback module is deactivated: no callbacks fire, panel removed
- Callback definitions persist in config.json (they're global)
- Re-activation restores everything

---

## Implementation Order Summary

| Step | What | Files | Depends on |
|------|------|-------|------------|
| 0 | Scaffold crate | new crate + Cargo.toml + mod.rs | nothing |
| 1 | Data model & persistence | types.rs + lib.rs | Step 0 |
| 2 | Callback_upsert (create) | tools.rs | Step 1 |
| 3 | Callbacks panel | panel.rs + lib.rs | Step 1 |
| 4 | Full CRUD + toggle + editor | tools.rs | Steps 2, 3 |
| 5 | Pattern matching | matcher.rs | Step 1 |
| 6 | Non-blocking execution | executor.rs + types.rs | Steps 1, 5 |
| 7 | Blocking execution | tool_pipeline.rs + executor.rs | Steps 5, 6 |
| 8 | Result enrichment | tool_pipeline.rs | Step 7 |
| 9 | Panel auto-close | tool_cleanup.rs + types.rs | Steps 6, 7 |
| 10 | Edge cases & polish | various | Steps 6-9 |

---

## Files Modified (existing)

| File | Change |
|------|--------|
| `Cargo.toml` | Add workspace member + binary dependency |
| `src/modules/mod.rs` | Import + register CallbackModule |
| `src/app/run/tool_pipeline.rs` | Post-batch callback trigger hook |
| `src/app/run/tool_cleanup.rs` | Panel auto-close on watcher result |
| `crates/cp-base/src/state/watchers.rs` | Add `close_panel` to WatcherResult |
| `crates/cp-base/src/state/context.rs` | Add `ContextType::CALLBACK` constant |

## Files Created (new crate)

| File | Purpose |
|------|---------|
| `crates/cp-mod-callback/Cargo.toml` | Crate manifest |
| `crates/cp-mod-callback/src/lib.rs` | Module trait implementation |
| `crates/cp-mod-callback/src/types.rs` | CallbackDefinition, CallbackState, CallbackWatcher |
| `crates/cp-mod-callback/src/tools.rs` | Callback_upsert, Callback_toggle, editor tools |
| `crates/cp-mod-callback/src/panel.rs` | Callbacks panel (LLM context + TUI rendering) |
| `crates/cp-mod-callback/src/matcher.rs` | Glob pattern matching + caching |
| `crates/cp-mod-callback/src/executor.rs` | Script spawning + watcher registration |

---

## Testing Strategy

Each step has its own validation (see per-step sections). Additionally:

### Integration tests (after Step 9)
1. **Happy path**: Create callback → Edit file → callback fires → success message in tool result
2. **Error path**: Callback script fails → console panel opens → error in tool result
3. **Blocking + non-blocking mix**: One of each on same pattern → blocking holds, non-blocking notifies
4. **Pattern mismatch**: Edit file that doesn't match any callback → no callbacks fire
5. **Persistence**: Create callback → reload TUI → callback still exists and fires
6. **Worker isolation**: Worker A activates callback, Worker B doesn't → only A fires
7. **Timeout**: Blocking callback with `sleep 60` and 5s timeout → timeout error
8. **once_per_batch**: Batch of 3 Edit calls → callback fires once with all 3 paths
9. **one_at_a_time**: Rapid edits → second callback queued/skipped
10. **Delete + re-create**: Delete callback → script file gone → re-create → works

### CI compliance
- All new `.rs` files must be < 500 lines (`check-file-lengths.sh`)
- `crates/cp-mod-callback/src/` must have ≤ 8 entries (`check-folder-sizes.sh`)
- Both constraints satisfied by the planned file structure (7 files in src/)
