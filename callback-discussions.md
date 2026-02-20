# File Edit Callbacks — Design Document

## Phase A: Use Cases

### Problem Statement
The AI repeatedly performs the same manual verification loops after file edits (cargo check, lint, format, etc.). Callbacks would automate this feedback loop, giving the AI immediate results without manual intervention.

### Primary Use Cases
1. **Build verification** — auto-check after code edits (cargo check, tsc, go build, etc.)
2. **Code quality gates** — auto-lint, complexity checks, file length checks, folder size checks
3. **Artifact generation** — edit source → regenerate output (PDF from python, LaTeX build, YAML→output). The goal is the side effect, not pass/fail.
4. **Auto-formatting / auto-fix** — edit code → auto-format (rustfmt, prettier, black). Modifies the edited file itself.
5. **Any project-specific post-edit action** — the system must be general-purpose

### Key Insight: All archetypes are just "run a bash script"
The TUI doesn't need to distinguish between validation, generation, or mutation callbacks. They're all the same: run a script, capture exit code + output, report back. The script decides what to do. This keeps the TUI simple and project-agnostic.

### Auto-formatting circularity is a non-issue
When a formatting callback modifies a file the AI just edited, the file panel auto-refreshes (dynamic panels), and the callback result notification tells the AI what happened. The AI sees the updated file on its next turn. No special handling needed.

### Example use cases beyond programming (inspiration only — project configuration, not TUI code)
- **Notification/Alert**: edit calendar.ics → send desktop notification; edit deploy config → Slack webhook
- **Sync/Deploy**: edit config.yaml → kubectl apply; edit HTML → rsync to server
- **Data Pipeline**: edit SQL query → re-run query → save results.csv
- **Doc Generation**: edit source → re-generate API docs (rustdoc, typedoc, sphinx)
- **Validation (non-code)**: edit Dockerfile → hadolint; edit Terraform → terraform validate
- **Scoped Testing**: edit foo.rs → cargo test only that module (using $CP_CHANGED_FILES)

All of these are just bash scripts from the TUI's perspective. The intelligence lives in the scripts, not the TUI.

### Key Constraints
- **Project-agnostic**: no hardcoded language support. User configures callbacks per project.
- **AI edits only**: callbacks fire on Edit / Write / file deletion tool calls, NOT on external file changes.
- **Immediate feedback to LLM**: results flow back to the AI for course-correction without manual check cycles.

### Open Questions
- Should the AI be prompted/nudged to set up callbacks at the start of a session? (auto-behavior)
- Should there be a way to temporarily disable all callbacks? (e.g. during a big refactor)

---

## Phase B: Functional Requirements

### Callback Data Model
Each callback has:
- **ID**: auto-generated (CB1, CB2, CB3...)
- **Name**: user-chosen display name (e.g. "rust-check", "pdf-gen")
- **Description**: short explanation of what this callback does
- **Pattern**: gitignore-style positive glob rules (e.g. `*.rs`, `src/**/*.ts`, `seeds/*.yaml`)
- **Script**: bash script stored as `.context-pilot/scripts/{name}.sh`
- **Blocking**: bool — whether this callback blocks the Edit/Write tool result
- **Timeout**: max execution time in seconds (required for blocking, optional for non-blocking)
- **Success message**: custom message shown on success (e.g. "Build passed ✓")
- **Working directory**: directory to run the script from (defaults to project root)
- **one_at_a_time**: bool — won't run simultaneously with itself (queued if already running)
- **once_per_batch**: bool — fires once per batch with all matched files in $CP_CHANGED_FILES (vs once per matched file)
- **Active/Inactive**: per-worker toggle (stored worker-level). Callback definition is global (config-level).

### Storage
- **Metadata**: global `.context-pilot/config.json` (shared across all workers)
- **Scripts**: `.context-pilot/scripts/{name}.sh` (actual executable bash files)
- **Worker activation state**: per-worker in config.json

### Tools (3 tools)

1. **`Callback_upsert`** — Create, update, or delete a callback
   - Param `action`: `create` / `update` / `delete`
   - **Create**: params: name, description, pattern, script_content, blocking, timeout, success_message, cwd, one_at_a_time, once_per_batch. Creates metadata + writes script to `.context-pilot/scripts/{name}.sh` with auto-prepended shebang + env var docs header.
   - **Update**: params: id + any fields to change. For script content: pass full new script_content OR use old_string/new_string diff-based editing (requires editor open). If editor not open and diff-edit attempted: returns warning + auto-opens editor.
   - **Delete**: params: id. Removes metadata + deletes script file.
   - New callbacks are active by default for the creating worker.

2. **`Callback_toggle`** — Activate/deactivate callbacks for current worker
   - Params: id, active (bool)
   - Only affects the current worker's activation state — does NOT modify the callback definition.

3. **`Callback_open_editor` / `Callback_close_editor`** — Open/close the callback editor
   - Same pattern as `Library_open_prompt_editor` / `Library_close_prompt_editor`
   - Opens callback metadata + script content in a panel for viewing/editing
   - Required before diff-based script editing via Callback_upsert(action=update, old_string/new_string)
   - Max one callback open at a time

### Callbacks Panel (always visible)
- Fixed panel in sidebar, always visible (like Library)
- Table overview: ID, Name, Description, Pattern, Blocking, Active/Inactive (for current worker)
- When editor is open: full callback details + script content shown below the table
- Included in LLM context so the AI knows what callbacks exist and which are active

### Trigger Mechanism
- **Edit**, **Write**, and **file deletion** tool calls trigger callback evaluation
- After a batch of parallel tool calls completes:
  1. Collect all changed/created/deleted file paths from the batch
  2. For each active callback, check if any file path matches its pattern
  3. Fire all matching callbacks (parallel in V1)
- **Blocking flow**: all blocking callbacks must finish before ANY tool results are returned to the AI. Each tool result includes its own edit outcome + any relevant callback results.
- **Non-blocking flow**: tool results return immediately with a note ("Callback 'xyz' activated in background"). Process spawned via console crate, watcher registered, spine notification on completion.

---

## Phase C: Non-Functional Requirements & Design

### Architecture
- New module crate: `cp-mod-callback`
- Depends on `cp-base` (Module trait, State, tools, watchers) and `cp-mod-console` (SessionHandle for script execution)
- Owns: callback data model, tools, panel, trigger logic, pattern matching, editor

### Pattern matching
- Uses `globset` crate — already a workspace dependency (used by cp-mod-tree)
- Gitignore-style positive-match globs: `*.rs`, `src/**/*.ts`, `seeds/*.yaml`
- Zero new dependencies

### Script execution
- Scripts spawned via existing `SessionHandle::spawn()` infrastructure (cp-mod-console)
- Console server manages the process, TUI polls for output
- Blocking callbacks: similar to `easy_bash` pattern (spawn + blocking watcher with timeout)
- Non-blocking callbacks: similar to `console_watch` pattern (spawn + async watcher + spine notification)
- Concrete examples for THIS project:
  - `cargo check` after `*.rs` edits
  - `check-file-lengths.sh` after any file edit
  - `check-folder-sizes.sh` after file creation

### Script parametrization: environment variables
- `$CP_CHANGED_FILES` — newline-separated list of changed file paths (relative to project root)
- `$CP_PROJECT_ROOT` — absolute path to project root
- `$CP_CALLBACK_NAME` — name of the callback rule that triggered
- Script decides what to do with these — env vars are additive, not required (existing scripts work as-is)

### Visibility: panel only on error
- **Success** (exit code 0): no panel opened. Tool result / notification shows success message.
- **Error** (non-zero exit): console panel automatically opened with full output. Tool result / notification shows which edit triggered it, last 3-5 lines of output, and points to the panel.
- A UUID is always associated with each callback run — LLM can use it to open the console panel later to inspect even successful output.
- Applies to BOTH blocking and non-blocking callbacks.

### Multi-match execution
- **V1**: all matching callbacks run in full parallel. Simple, good enough for 2-3 callbacks.
- **Future**: job queue managed by the console server. Callback jobs have lower priority than directly-invoked console commands. Separate feature — doesn't change the callback contract.

### Guard rails
- Blocking callbacks require a max timeout (same pattern as console_wait)
- Non-blocking callbacks appear in Spine panel's Active Watchers section
- File edit always succeeds even if callback fails — callback failure is informational, not fatal
