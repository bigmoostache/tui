# Callbacks

Callbacks are bash scripts that auto-fire when the AI edits files matching a glob pattern. They run after every `Edit` or `Write` tool call.

## Overview

A callback has:
- **Name** — unique identifier (e.g. `rust-check`)
- **Pattern** — gitignore-style glob (e.g. `*.rs`, `src/**/*.ts`)
- **Script** — bash script stored in `.context-pilot/scripts/{name}.sh`
- **Blocking** — if true, holds the AI pipeline until the script finishes
- **Timeout** — max execution time in seconds (optional)
- **One-at-a-time** — prevents concurrent runs of the same callback
- **Success message** — custom message shown on exit 0
- **CWD** — working directory for the script (defaults to project root)

## Lifecycle

1. AI calls `Edit` or `Write` on a file
2. After execution, the file path is matched against all active callback patterns
3. Matching callbacks fire their scripts automatically
4. Results appear inline in the tool result:

```
Callbacks:
· rust-check passed (Build passed). Log: .context-pilot/console/cb_42.log
· structure-check running
```

## Blocking vs Non-blocking

**Blocking** callbacks hold the AI pipeline — the AI waits for the script to finish before continuing. Use for fast checks that the AI should react to immediately (e.g. `cargo check`).

**Non-blocking** callbacks run in the background. The AI sees "running" and continues working. Results arrive as spine notifications when complete.

When both types fire on the same edit, they share a single `Callbacks:` block in the tool result.

## Tools

### Callback_upsert

Creates, updates, or deletes a callback definition.

```
action: "create" | "update" | "delete"
name: "rust-check"
pattern: "*.rs"
script_content: "cargo check 2>&1"
blocking: true
timeout: 30
one_at_a_time: false
success_message: "Build passed"
cwd: "/path/to/dir"
```

For updates, use `old_string`/`new_string` to diff-edit the script (requires `Callback_open_editor` first).

### Callback_toggle

Activates or deactivates a callback for the current worker. Does not modify the definition — only controls whether it fires.

```
id: "CB1"
active: true | false
```

### Callback_open_editor / Callback_close_editor

Opens a callback's script in the Callbacks panel for reading and editing. Required before using diff-based script updates.

## Script Environment

Scripts receive these environment variables:

| Variable | Description |
|----------|-------------|
| `CP_CHANGED_FILES` | Newline-separated list of changed file paths (relative) |
| `CP_PROJECT_ROOT` | Absolute path to the project root |
| `CP_CALLBACK_NAME` | Name of the callback being executed |

## skip_callbacks

The `Edit` and `Write` tools accept an optional `skip_callbacks` parameter — an array of callback names to skip for that specific edit:

```json
{
  "file_path": "src/main.rs",
  "old_string": "...",
  "new_string": "...",
  "skip_callbacks": ["rust-check"]
}
```

Use sparingly. Callbacks exist to help; prefer letting them run. Valid use cases:
- Mid-refactor when you know the build will fail
- Actively debugging a callback script itself

Warnings are injected into the tool result for:
- Names that don't match any defined callback
- Names that match a callback whose pattern wouldn't have triggered anyway

## Panel Display

The Callbacks panel shows all defined callbacks in a table:

```
| ID  | Name            | Pattern | Blocking | Timeout | Active | 1-at-a-time |
|-----|-----------------|---------|----------|---------|--------|-------------|
| CB1 | rust-check      | *.rs    | yes      | 30s     | ✓      | no          |
| CB2 | structure-check | *       | no       | 15s     | ✓      | yes         |
```

## Result Format

Callback results appear in the tool result with colored status words (TUI only):

| Status | Color | Meaning |
|--------|-------|---------|
| `passed` | green | Script exited 0 |
| `FAILED` | red | Script exited non-zero |
| `running` | blue | Non-blocking, still executing |
| `TIMED OUT` | red | Exceeded timeout |
| `skipped` | gray | Skipped via skip_callbacks |

On **success**: no panel is created. A log file path is provided for inspection.

On **failure**: a console panel is automatically created showing the script output. Error lines appear indented under the FAILED status.

## One-at-a-time

When `one_at_a_time` is true, a second trigger of the same callback while the first is still running will be skipped. This prevents pile-ups for slow callbacks on rapid edits.

## File Storage

- Callback definitions: persisted in `.context-pilot/state.json` (per-worker)
- Scripts: `.context-pilot/scripts/{name}.sh`
- Logs: `.context-pilot/console/{session_key}.log`
