# Enhancement Ideas — The Grand Manifesto

## Tier 1: Immediate High Impact

### Auto-Context Compactor
- Background daemon that keeps context lean automatically
- Detects when context approaches threshold (~70%)
- Auto-summarizes and closes oldest conversation histories (creating logs/memories first)
- Closes file panels not referenced in 3+ turns
- Collapses verbose console output to exit code + last 10 lines
- **Why**: ~20% of AI turns spent on context housekeeping. Biggest single efficiency gain.

### Semantic Code Search / Code Intelligence
- `code_search "where is the Panel trait defined"` — semantic, not string matching
- Lightweight symbol index from tree (function names, struct names, impl blocks)
- `find_references "ConsolePanel"` — show everywhere it's used
- Poor man's LSP. Even ctags-style index would be transformative.
- **Why**: AI wastes enormous context opening files just to find the right one.

### Diff Preview / Dry Run Mode
- `--dry-run` flag on Edit: returns diff without writing to disk
- Preview complex multi-line edits before committing them
- Avoid triggering expensive callbacks (cargo check) on bad edits
- **Why**: Reduces wasted callback cycles and failed edits.

---

## Tier 2: Strategic / Game-Changing

### Sub-Agents / Task Delegation
- Spawn child workers with own conversation, context, and tools
- Work on sub-tasks in parallel ("research this API and report back")
- Return summary to parent when done
- Scoped permissions (read-only explorer, write-only implementer)
- **Why**: 10x throughput on complex tasks. The multi-agent future.

### Checkpoint / Rollback System
- `checkpoint create "before-big-refactor"` — save files, git state, context
- `checkpoint restore` — roll back everything if it goes wrong
- Integrates with git (temp branch/stash) + saves context state
- **Why**: Safety net for bold refactors. The "undo" button for multi-file changes.

### Smart Callback Chains
- Callbacks that trigger other callbacks in sequence
- `edit → cargo check ✓ → cargo test ✓ → cargo clippy ✓`
- Visual pipeline status in panel
- Configurable: fail-fast or continue-on-error
- **Why**: Local CI pipeline triggered by a single file edit.

### Context Snapshots for Task Switching
- `context_snapshot "implementing-web-search"` — save open files, active todos, scratchpad
- Handle interruption, then `context_restore` to resume exactly where you left off
- Lighter than presets — just the working context, not the whole config
- **Why**: Prevents flow loss when user switches topic mid-task.

---

## Tier 3: Polish & Developer Experience

### Persistent Tool Results Cache
- Cache expensive, repetitive tool calls keyed by input hash
- `cargo test` on unchanged code, `brave_search` for same query
- Invalidate on file changes
- **Why**: Dramatically speeds up repeated workflows.

### Token Budget Awareness in Tools
- `Open` shows estimated token count before opening large files
- Large file warning: "This file is ~2000 tokens, open anyway?"
- Console output auto-truncation when exceeding threshold
- **Why**: Prevents tools from unexpectedly blowing up context.

### Inline Test Runner
- `test_run "pattern"` — run specific tests, structured pass/fail/skip results
- Auto-rerun failed tests after edits (watchlist for tests)
- Integration with todo: mark test todos as done when tests pass
- **Why**: Tighter feedback loop than spawning console for cargo test.

### Auto-Describe on Open
- When opening a file for the first time, auto-add tree_describe annotation
- Based on first few lines / module doc comments
- `[!]` marker already tracks staleness — this completes the loop
- **Why**: Keeps tree informative without manual annotation effort.

### Smart Context Pre-loading
- User says "fix the bug in console module" → auto-open relevant files
- Load related memories and recent logs based on keywords
- Pre-populate context with what the AI will likely need
- **Why**: Reduces the 3-4 turns of "let me find the right files" at task start.

---

## Tier 4: Visionary / Long-Term

### Visual Dependency Graph
- Panel showing module/file/crate dependency DAG
- Click a crate → see dependents and dependencies
- Impact analysis: "if I change this type, what breaks?"
- ASCII art or structured panel rendering

### Learning from Mistakes
- Track patterns in AI failures ("clippy doc formatting" happened 4 times)
- Auto-create skills or memories from repeated mistakes
- Project-specific "lessons learned" knowledge base that grows over time

### Conversation Branching
- Non-linear chat: branch to try approach A vs approach B
- Compare results, merge the winner
- Like git branches but for conversations

---

## Priority Ranking

| Rank | Feature | Impact |
|------|---------|--------|
| 🥇 | Auto-Context Compactor | ~30% efficiency gain, eliminates housekeeping turns |
| 🥈 | Sub-Agents | 10x throughput on complex multi-part tasks |
| 🥉 | Semantic Code Search | Eliminates biggest time sink: finding code |
| 4 | Smart Callback Chains | Local CI from single edit |
| 5 | Checkpoint / Rollback | Safety net for bold refactors |
| 6 | Context Snapshots | Flow preservation on task switches |
| 7 | Token Budget Awareness | Prevents context blowups |
| 8 | Diff Preview / Dry Run | Fewer wasted edit cycles |
| 9 | Learning from Mistakes | Ship gets smarter every voyage |
| 10 | Smart Context Pre-loading | Faster task startup |
