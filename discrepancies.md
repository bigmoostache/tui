# Callback Implementation Audit — Discrepancies Report

Design doc: `callback-discussions.md`
Branch: `roadmap-day2` (diff vs `master`)
Date: 2026-02-20

**Total: 95/114 items match (83.3%)**

---

## A. DATA MODEL — CallbackDefinition Fields

**12/12 — No discrepancies.**

All fields implemented exactly as designed: id, name, description, pattern, blocking, timeout_secs, success_message, cwd, one_at_a_time, once_per_batch. Active/inactive is per-worker via `CallbackState.active_set`. Scripts stored at `.context-pilot/scripts/{name}.sh`.

---

## B. DATA MODEL — CallbackState Fields

**4/4 — No discrepancies.**

`definitions`, `next_id`, `active_set`, `editor_open` all present. (`editor_open` is dead code — see Section H.)

---

## C. STORAGE & PERSISTENCE

**8/8 — No discrepancies.**

`is_global()=true`, `save_module_data()` serializes definitions + next_id, `load_module_data()` deserializes, `save_worker_data()` / `load_worker_data()` handle per-worker `active_set`. Metadata in global config.json, scripts on disk, worker activation per-worker.

---

## D. TOOLS — Callback_upsert (create)

**14/14 — No discrepancies.**

All params implemented (name, description, pattern, script_content, blocking, timeout, success_message, cwd, one_at_a_time, once_per_batch). Shebang + env var docs header auto-prepended. Script written to `.context-pilot/scripts/{name}.sh` with `chmod +x`. New callbacks active by default. Metadata created in state.

Bonus (not in design): duplicate name check, glob pattern validation at creation time.

---

## E. TOOLS — Callback_upsert (update)

**5/6 — 1 discrepancy.**

| Item | Status | Detail |
|------|--------|--------|
| E1. Param: id (required) | ✅ | Validated with early return |
| E2. Can update any field | ✅ | All metadata fields handled |
| E3. Full script_content replacement | ✅ | Writes new script with shebang header |
| E4. Diff-based old_string/new_string | ✅ | Reads current script, does `replacen(old_str, new_str, 1)` |
| **E5. Diff-edit requires editor open** | **⚠️** | **Design: "requires editor open, else returns warning + auto-opens editor". Code: no check of `editor_open` state. Diff-edit reads from disk directly, works without editor being open. Impact: LOW — more permissive than designed.** |
| E6. Rename updates script filename | ✅ | `fs::rename()` when name changes |

---

## F. TOOLS — Callback_upsert (delete)

**5/5 — No discrepancies.**

Removes metadata, deletes script file, removes from active_set, closes editor if open.

---

## G. TOOLS — Callback_toggle

**3/3 — No discrepancies.**

Params id + active (bool). Only affects worker activation state. Does not modify definition.

---

## H. TOOLS — Editor (open/close)

**0/5 — 5 discrepancies. ENTIRE SUBSYSTEM NOT IMPLEMENTED.**

| Item | Status | Detail |
|------|--------|--------|
| **H1. Callback_open_editor / Callback_close_editor tools exist** | **❌** | **These tools DO NOT EXIST. `lib.rs` only defines 2 tools: Callback_upsert and Callback_toggle.** |
| **H2. Same pattern as Library_open_prompt_editor** | **❌** | **No equivalent panel rendering with editor mode.** |
| **H3. Opens callback metadata + script content in panel** | **❌** | **Panel only shows table view. No editor view exists. `editor_open` field is never set to `Some(...)` by any tool.** |
| **H4. Required before diff-based script editing** | **❌** | **Not enforced — diff editing works without editor open (relates to E5).** |
| **H5. Max one callback open at a time** | **❌** | **Not applicable since editor doesn't exist.** |

**Impact: HIGH** — The AI has no built-in way to view current script content. Must use the `Open` tool on the `.sh` file directly. The design envisioned a self-contained editor flow within the callback system. The `editor_open` field on `CallbackState` is dead code.

---

## I. PANEL — Fixed Panel Requirements

**3/5 — 2 discrepancies (cascading from Section H).**

| Item | Status | Detail |
|------|--------|--------|
| I1. Fixed panel, always visible | ✅ | `fixed_panel_types()` returns CALLBACK, closeable=false |
| I2. Table overview with key fields | ✅ | Actually shows 11 columns (exceeds the 6 in spec) — ID, Name, Pattern, Description, Blocking, Timeout, Active, 1-at-a-time, Once/batch, Success Msg, CWD |
| **I3. Editor open: script content below table** | **⚠️** | **No editor view exists. Panel only ever shows table. Ties to Section H.** |
| I4. Included in LLM context | ✅ | `context()` returns markdown table via `format_for_context()` |
| I5. Token counting works | ✅ | `refresh()` calls `estimate_tokens()` and updates `ctx.token_count` |

---

## J. TRIGGER MECHANISM

**7/8 — 1 discrepancy.**

| Item | Status | Detail |
|------|--------|--------|
| J1. Edit triggers evaluation | ✅ | `collect_changed_files()` matches "Edit" |
| J2. Write triggers evaluation | ✅ | `collect_changed_files()` matches "Write" |
| **J3. File deletion triggers evaluation** | **⚠️** | **Design mentions "file deletion tool calls". Code only matches "Edit" and "Write". No Delete tool exists in the codebase, so this is a theoretical gap. Impact: LOW.** |
| J4. Triggers after batch completes | ✅ | Callback code runs after all tools in batch have executed |
| J5. Collect changed file paths | ✅ | `collect_changed_files(&tools)` gathers file_path values |
| J6. Check pattern match per active callback | ✅ | `match_callbacks()` iterates definitions, checks active_set, compiles glob |
| J7. Fire all matching callbacks | ✅ | Both async and blocking paths called |
| J8. Extract from tool_use input params | ✅ | `tool.input.get("file_path")` — reads from input, not result |

---

## K. PATTERN MATCHING

**4/4 — No discrepancies.**

Uses `globset` crate (already workspace dependency). Gitignore-style positive-match globs. Also matches against filename component for patterns like `*.rs`. Zero new dependencies.

---

## L. SCRIPT EXECUTION — Non-blocking

**4/5 — 1 discrepancy.**

| Item | Status | Detail |
|------|--------|--------|
| L1. Spawned via SessionHandle::spawn() | ✅ | Direct call in `fire_callback()` |
| L2. Console server manages process | ✅ | Handle stored in `ConsoleState.sessions` |
| L3. Similar to console_watch pattern | ✅ | CallbackWatcher registered → spine notification on completion |
| **L4. Tool results return with note** | **⚠️** | **Design: "tool results return immediately with a note ('Callback xyz activated in background')". Code: `let _summaries = fire_async_callbacks(...)` — summaries are DISCARDED. The Edit/Write tool result contains NO mention of async callbacks. AI only learns about them when the watcher fires a spine notification. Impact: MEDIUM.** |
| L5. Watcher registered | ✅ | CallbackWatcher registered in WatcherRegistry |

---

## M. SCRIPT EXECUTION — Blocking

**4/5 — 1 discrepancy.**

| Item | Status | Detail |
|------|--------|--------|
| M1. Must finish before ANY tool results returned | ✅ | CONSOLE_WAIT_BLOCKING_SENTINEL defers all results |
| M2. Similar to easy_bash pattern | ✅ | CallbackWatcher with blocking=true, deadline_ms |
| **M3. Each tool result includes edit outcome + callback results** | **⚠️** | **Design: "Each tool result includes its own edit outcome + any relevant callback results" (merged). Code: blocking callback result comes as a SEPARATE tool_result with synthetic tool_use_id "cb_block_N". AI sees (1) Edit result, (2) separate callback result. Not merged. Impact: LOW — both pieces of info are delivered, just in separate messages.** |
| M4. Blocking callbacks require max timeout | ✅ | Validated at creation, check_timeout() with deadline_ms |
| M5. ALL blocking must finish before ANY results | ✅ | Sentinel blocks entire batch |

---

## N. SCRIPT PARAMETRIZATION

**4/5 — 1 discrepancy.**

| Item | Status | Detail |
|------|--------|--------|
| N1. $CP_CHANGED_FILES | ✅ | `build_changed_files_env()` joins with "\n" |
| N2. $CP_PROJECT_ROOT | ✅ | `std::env::current_dir()` |
| N3. $CP_CALLBACK_NAME | ✅ | `def.name` |
| N4. Env vars are additive | ✅ | Inline shell `KEY=val bash script.sh`, scripts don't need to use them |
| **N5. File paths relative to project root** | **⚠️** | **Design: "relative to project root". Code: extracts file_path directly from tool input. Only strips "./" prefix. If AI passes an absolute path (e.g. "/home/user/project/src/main.rs"), it goes through unmodified. No normalization to ensure relativity. Impact: LOW — AI almost always uses relative paths in practice.** |

---

## O. VISIBILITY — Panel Lifecycle

**4/7 — 3 discrepancies.**

| Item | Status | Detail |
|------|--------|--------|
| **O1. Success: no panel opened** | **⚠️** | **Design: "no panel opened". Code: panel IS created by `fire_callback()`, then auto-closed on success via `close_panel=true`. Net effect is identical (no panel visible after success), but mechanism differs: create-then-close vs never-create. Impact: NEGLIGIBLE.** |
| O2. Error: console panel auto-opened | ✅ | `close_panel=false` on error, panel stays open |
| O3. UUID per callback run | ✅ | `uid = format!("UID_{}_P", ...)` assigned |
| **O4. UUID for later inspection of successful runs** | **⚠️** | **Design: "LLM can use UUID to open console panel later to inspect even successful output". Code: on success, panel is AUTO-CLOSED and session KILLED. UUID generated but panel+session destroyed. No way to inspect successful runs after the fact. Impact: MEDIUM.** |
| O5. Applies to both blocking and non-blocking | ✅ | Same CallbackWatcher used for both paths |
| O6. Success message in result/notification | ✅ | "Callback '{name}': {success_message} (exit 0)" |
| **O7. Error shows edit trigger, 3-5 lines, points to panel** | **⚠️** | **Design: "which edit triggered it, last 3-5 lines of output, and points to panel". Code: error format is `"Callback '{name}' FAILED (exit {code})\nLast output:\n{lines}"`. Missing: (a) which files triggered it, (b) "see panel P{X}" reference. Uses 10 lines instead of 3-5. Impact: LOW — core error info is present.** |

---

## P. RESULT ENRICHMENT

**2/5 — 2 discrepancies, 1 partial.**

| Item | Status | Detail |
|------|--------|--------|
| **P1. Blocking: edit outcome + callback merged** | **⚠️** | **Same as M3. Separate tool_result, not merged into Edit/Write result. Impact: LOW.** |
| **P2. Non-blocking: note in tool result** | **⚠️** | **Same as L4. Summaries discarded with `let _summaries`. No note in Edit/Write tool result. Impact: MEDIUM.** |
| P3. Non-blocking: spine notification | ✅ | `check_watchers()` → `SpineState::create_notification()` |
| P4. Success message in result/notification | ✅ | Included in watcher description |
| **P5. Error output last 3-5 lines** | **⚠️** | **Uses `last_n_lines(10)` — 10 lines instead of 3-5. More generous than spec. Impact: NEGLIGIBLE.** |

---

## Q. ARCHITECTURE & NFR

**6/6 — No discrepancies.**

New `cp-mod-callback` crate, depends on `cp-base` + `cp-mod-console`, uses `globset` (zero new deps), project-agnostic, AI-edits-only trigger, immediate feedback to LLM via blocking pipeline and spine notifications.

---

## R. EDGE CASES & GUARD RAILS

**6/7 — 1 partial discrepancy.**

| Item | Status | Detail |
|------|--------|--------|
| R1. Blocking require max timeout | ✅ | Validated at creation + check_timeout() |
| R2. Non-blocking in Spine Active Watchers | ✅ | Registered as Watcher, shown in spine panel |
| R3. File edit always succeeds | ✅ | Edit/Write execute first, callback runs after |
| R4. Callback failure is informational | ✅ | No panics, no rollback, no blocking |
| R5. V1: all run in parallel | ✅ | Processes spawned concurrently via SessionHandle |
| R6. one_at_a_time works | ✅ | `has_watcher_with_tag()` check before spawning |
| **R7. once_per_batch flag** | **⚠️** | **Field exists and is stored/loaded, but trigger engine ALWAYS fires once-per-batch regardless of flag value. No per-file firing path exists. Comment says "V1 always uses once_per_batch=true". Flag gives false impression of per-file support. Impact: LOW — default is true anyway.** |

---

## Summary — All Discrepancies by Impact

### 🔴 High Impact

1. **H1-H5**: Entire editor subsystem not implemented (5 items). No `Callback_open_editor` / `Callback_close_editor` tools. No panel editor view. `editor_open` field is dead code. Cascades to E5 and I3.

### ⚠️ Medium Impact

2. **L4/P2**: Async callback summaries discarded — AI doesn't know an async callback was triggered until completion notification.
3. **O4**: Successful callback runs not inspectable after completion — UUID is generated but panel+session destroyed on success.
4. **M3/P1**: Blocking callback result delivered as separate tool_result, not merged into Edit/Write result.

### ⚡ Low Impact

5. **E5**: Diff-edit works without editor open (more permissive than designed).
6. **J3**: No file deletion trigger (but no Delete tool exists).
7. **N5**: No absolute→relative path normalization for $CP_CHANGED_FILES.
8. **O7**: Error message missing "which edit triggered it" and panel reference; uses 10 lines instead of 3-5.
9. **R7**: `once_per_batch` flag is dead code — behavior hardcoded to always-once-per-batch.

### 🟢 Negligible

10. **O1**: Panel create-then-close mechanism vs never-create (same end result).
11. **P5**: 10 output lines in errors instead of 3-5 (more generous).
