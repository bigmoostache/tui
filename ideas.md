# Tool Ideas — Armory Wishlist

## 🔴 Sorely Missing — Would Save Me Daily

### `web_search`
- When the AI doesn't know an API, a new syntax, or needs to check docs — it's sailing blind
- Current workaround: guess, or ask the user to paste info

### `web_fetch`
- Read a URL (docs page, Stack Overflow, GitHub raw files, READMEs)
- Current workaround: user has to copy-paste content

### `inline_grep`
- A grep that returns results directly in the tool response instead of creating a separate panel to read and close
- Current workaround: `console_easy_bash("grep -rn ...")` — works but panel overhead for a simple search

## 🟠 Would Be Really Handy

### `unified_delete`
- One tool: `Delete(M1, X1, L1, CB1, ...)` instead of juggling 6 different deletion patterns
- (`agent_delete`, `skill_delete`, `todo_update(delete:true)`, `memory_update(delete:true)`, `scratchpad_wipe`...)
- Current workaround: remember 6 different ways to delete things

### `symbol_search` / LSP
- Jump to definition, find all references, find all implementations
- When navigating a 15K+ line codebase, text grep only gets you so far
- Current workaround: `grep` + reading files + AI memory

### `file_patch`
- Apply multiple edits to one file in a single tool call
- When the AI needs to change 4 spots in one file, it makes 4 sequential `Edit` calls, each waiting for the previous
- Current workaround: chain of `Edit` calls, one at a time

### `context_snapshot`
- Save current context state (open panels, files) as a named snapshot to restore later
- When switching between tasks the AI loses its whole context setup
- Current workaround: manually re-open everything, or rely on presets which don't capture open files

## 🟡 Nice to Have — For the Long Voyages

### `screenshot` / `clipboard_read`
- Sometimes the user sees something on screen the AI can't see (rendering bug, UI glitch)
- Current workaround: user describes it in words

### `diff_review`
- Smart PR review: parse a branch diff into structured sections (files changed, hunks, stats) with annotations
- Current workaround: `git_execute("git diff main..branch")` — huge unstructured output

### `context_summary`
- Auto-summarize what's in the AI's context in plain English
- Sometimes the AI loses track of what panels are open and why
- Current workaround: look at the Statistics panel and reconstruct mentally

### `bookmark`
- Save a file + line range for quick re-opening
- When jumping between 5 files during a refactor, re-opening each time is slow
- Current workaround: keep files open (fills context) or re-open from memory

### `parallel_edit`
- Edit multiple different files in a single tool call
- During a refactor touching 6 files, bottlenecked by sequential calls
- Current workaround: chain of `Edit` calls across files, waiting for each

### `notification_schedule_on_pattern`
- "Notify me if X appears in console Y" as a persistent rule
- Like a callback but for console output, not file edits
- Current workaround: `console_watch` is one-shot; must re-register after each match

## 🟢 Dream Features — When We're the Flagship

### `sub_agent_spawn`
- Send a sub-task to another AI worker (already on the roadmap — the endgame!)

### `image_generate`
- Generate architecture diagrams, flowcharts, ERDs inline

### `teach_me`
- User can give the AI a new skill/behavior mid-conversation that persists
- Richer than memories, more like a live plugin

## Top 3 Priorities

1. **`web_search` + `web_fetch`** — biggest blindspot, can't look anything up
2. **`unified_delete`** — six different deletion patterns is six too many
3. **`file_patch` / `parallel_edit`** — sequential single-edit calls are the biggest speed bottleneck during refactors
