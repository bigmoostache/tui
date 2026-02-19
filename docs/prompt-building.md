# Prompt Building Pipeline

How Context Pilot assembles the final LLM prompt from system prompts, panels, conversation messages, and tools.

## Overview

Every time the LLM is called, Context Pilot builds a single prompt from four sources:

1. **System prompt** — the active agent's content (e.g., default, pirate-coder)
2. **Panels** — all context elements (files, tree, todos, memories, etc.) injected as fake tool call/result pairs
3. **Conversation** — real user/assistant messages, tool calls, and tool results
4. **Tools** — JSON schema definitions of all enabled tools

The assembly is orchestrated by `prepare_stream_context()` in `src/app/context.rs`, then formatted per-provider in `src/llms/`.

## Phase 1: Stream Trigger

When a user sends a message or tools finish executing, the app calls either the initial stream or `continue_streaming()` in `src/app/run/streaming.rs`. Both follow the same path:

```rust
let ctx = prepare_stream_context(&mut self.state, true);
let system_prompt = get_active_agent_content(&self.state);

start_streaming(StreamParams {
    provider, model, system_prompt,
    messages: ctx.messages,
    context_items: ctx.context_items,
    tools: ctx.tools,
    ...
});
```

## Phase 2: Context Preparation

`prepare_stream_context()` in `src/app/context.rs` is the central orchestration function. It runs these steps in order:

### Step 1: Mark notifications processed

Marks all `UserMessage` spine notifications as processed. This prevents the spine from firing a redundant auto-continuation for messages the LLM already sees in the rebuilt context.

### Step 2: Detach old conversation chunks

`detach_conversation_chunks()` checks if the active conversation exceeds thresholds:

| Constant | Value | Purpose |
|----------|-------|---------|
| `DETACH_CHUNK_MIN_MESSAGES` | 25 | Minimum messages in chunk to detach |
| `DETACH_CHUNK_MIN_TOKENS` | 5,000 | Minimum tokens in chunk to detach |
| `DETACH_KEEP_MIN_MESSAGES` | 25 | Minimum messages to keep in live conversation |
| `DETACH_KEEP_MIN_TOKENS` | 7,500 | Minimum tokens to keep in live conversation |

All four constraints must be met. The function finds a safe **turn boundary** (after a complete assistant turn) and splits the oldest messages into a frozen `ConversationHistory` panel. Token counting uses `estimate_message_tokens()` which accounts for content, tool_uses (name + JSON input), and tool_results.

### Step 3: Refresh conversation token count

`refresh_conversation_context()` recalculates the Conversation context element's `token_count` using `estimate_message_tokens()` for each active message. This is the count shown in the sidebar.

### Step 4: Refresh all panels

Iterates all panels and calls `panel.refresh(state)` on each. Panels update their `cached_content` and `token_count`. Some panels derive content from state (todos, memories, scratchpad), others rely on background caching (files, tree, git results).

### Step 5: Collect context items

Iterates all unique context types, calls `panel.context(state)` → returns `Vec<ContextItem>`:

```rust
pub struct ContextItem {
    pub id: String,           // e.g., "P1", "P5", "chat"
    pub header: String,       // e.g., "Todo List", "File: src/main.rs"
    pub content: String,      // the actual panel content
    pub last_refresh_ms: u64, // timestamp of last content change
}
```

### Step 6: Sort by timestamp

Context items are sorted by `last_refresh_ms` ascending — oldest/most stable panels first, newest closest to the conversation. This ordering optimizes Anthropic's prefix caching: earlier panels are more likely to be cache hits since they change less frequently.

### Step 7: Cache cost tracking

Each panel's content is hashed and compared to the previous tick's hash list via prefix matching. Panels whose hash matches the same position in the previous list are marked as cache hits. Per-panel costs are accumulated based on cache hit/miss token pricing for the current model.

### Step 8: Filter messages

Removes empty messages (no content, no tool_uses, no tool_results) and returns:

```rust
StreamContext {
    messages: Vec<Message>,          // filtered conversation messages
    context_items: Vec<ContextItem>, // sorted panel content
    tools: Vec<ToolDefinition>,      // all tool definitions
}
```

## Phase 3: Provider-Specific Assembly

The `StreamContext` is wrapped in an `LlmRequest` and dispatched to the active provider. Each provider formats the prompt differently.

### Anthropic (Primary Provider)

Source: `src/llms/anthropic.rs`, function `messages_to_api()`

#### System Prompt

The active agent's content is sent as the top-level `system` field in the Anthropic API request (not a message).

#### Panel Injection

Panels are injected as **fake `dynamic_panel` tool call/result pairs** at the start of the message list, before any real conversation. The Conversation panel (`id="chat"`) is excluded — it's sent as real messages.

`prepare_panel_messages()` in `src/llms/mod.rs` filters and formats panels:

```
For each panel (sorted oldest → newest):

  ASSISTANT message:
    - Text block: header text (first panel includes "Beginning of dynamic panel display...")
    - Text block: "Panel automatically generated at {iso_time}"
    - ToolUse block: { id: "panel_{ID}", name: "dynamic_panel", input: { id: "{ID}" } }

  USER message:
    - ToolResult block: {
        tool_use_id: "panel_{ID}",
        content: "======= [{ID}] {Panel Name} =======\n{content}"
      }
```

After all panels, a footer pair is appended:

```
  ASSISTANT message:
    - Text block: footer (message timestamps, current datetime)
    - ToolUse block: { id: "panel_footer", name: "dynamic_panel", input: { action: "end_panels" } }

  USER message:
    - ToolResult block: {
        tool_use_id: "panel_footer",
        content: "Panel display complete. Proceeding with conversation."
      }
```

#### Seed Re-injection

After all panels and before conversation messages, the system prompt is repeated:

```
  USER message:
    "System instructions (repeated for emphasis):\n\n{system_prompt}"

  ASSISTANT message:
    "Understood. I will follow these instructions."
```

This ensures the agent's instructions remain salient after potentially hundreds of panel messages.

#### Conversation Messages

Real conversation messages are appended after panels:

| Message Type | API Representation |
|---|---|
| User text | `{ role: "user", content: [Text(...)] }` |
| Assistant text | `{ role: "assistant", content: [Text(...)] }` |
| Tool call | `ToolUse` blocks merged into the preceding assistant message |
| Tool result | `{ role: "user", content: [ToolResult(...)] }` |
| Summarized message | Uses `tl_dr` text instead of full `content` |
| Deleted / Detached | Skipped entirely |

**Orphan tool call handling**: Tool calls without matching tool results (e.g., truncated by `max_tokens`) are silently skipped to avoid Anthropic API errors about "insufficient tool messages".

#### Tool Definitions

All enabled tools are converted to Anthropic's tool JSON schema format via `build_api_tools()` and sent in the top-level `tools` field.

### OpenAI-Compatible Providers (Grok, Groq, DeepSeek)

Source: `src/llms/openai_compat.rs`, function `build_messages()`

Same conceptual structure with format differences:

| Aspect | Anthropic | OpenAI-compat |
|--------|-----------|---------------|
| System prompt | Top-level `system` field | `{ role: "system" }` message (first) |
| Tool calls | `ContentBlock::ToolUse` in assistant content | `tool_calls: [{ function: { name, arguments } }]` |
| Tool results | `ContentBlock::ToolResult` in user message | `{ role: "tool", tool_call_id: "..." }` |
| Seed re-injection | Yes, after panels | No |

Tool call merging: consecutive tool call messages from the same assistant turn are merged into a single assistant message with multiple `tool_calls` entries (required by OpenAI APIs).

## Final API Request Structure (Anthropic)

```json
{
  "model": "claude-sonnet-4-20250514",
  "max_tokens": 128000,
  "stream": true,
  "system": "{active agent content}",
  "messages": [
    // ─── PANELS (fake tool calls, sorted by last_refresh_ms) ───
    { "role": "assistant", "content": [
        { "type": "text", "text": "Beginning of dynamic panel display..." },
        { "type": "text", "text": "Panel automatically generated at 2026-02-19T14:00:00Z" },
        { "type": "tool_use", "id": "panel_P1", "name": "dynamic_panel", "input": { "id": "P1" } }
    ]},
    { "role": "user", "content": [
        { "type": "tool_result", "tool_use_id": "panel_P1", "content": "======= [P1] Todo List =======\n..." }
    ]},
    // ... more panels ...
    { "role": "assistant", "content": [
        { "type": "text", "text": "End of dynamic panel displays...\nCurrent datetime: ..." },
        { "type": "tool_use", "id": "panel_footer", "name": "dynamic_panel", "input": { "action": "end_panels" } }
    ]},
    { "role": "user", "content": [
        { "type": "tool_result", "tool_use_id": "panel_footer", "content": "Panel display complete..." }
    ]},

    // ─── SEED RE-INJECTION ───
    { "role": "user", "content": [
        { "type": "text", "text": "System instructions (repeated for emphasis):\n\n{agent content}" }
    ]},
    { "role": "assistant", "content": [
        { "type": "text", "text": "Understood. I will follow these instructions." }
    ]},

    // ─── CONVERSATION (real messages) ───
    { "role": "user", "content": [{ "type": "text", "text": "Hello!" }] },
    { "role": "assistant", "content": [
        { "type": "text", "text": "Let me check that file." },
        { "type": "tool_use", "id": "T1", "name": "Open", "input": { "path": "src/main.rs" } }
    ]},
    { "role": "user", "content": [
        { "type": "tool_result", "tool_use_id": "T1", "content": "Opened 'src/main.rs' as P10" }
    ]},
    // ... more conversation ...
  ],
  "tools": [
    {
      "name": "Open",
      "description": "Opens a file and adds it to context...",
      "input_schema": { "type": "object", "properties": { "path": { "type": "string" } }, "required": ["path"] }
    }
    // ... more tool definitions ...
  ]
}
```

## Panel Content Sources

Each panel type generates its `ContextItem` content differently:

| Panel | Source | Cache Strategy |
|-------|--------|----------------|
| Todo (P1) | `TodoState` in memory | State-derived, no background cache |
| Library (P2) | `PromptState` — agents, skills, commands | State-derived |
| Statistics (P3) | Aggregated stats from all modules | State-derived |
| Tools (P4) | `state.tools` — enabled/disabled tool list | State-derived |
| Tree (P5) | `TreeState` — directory listing | File watcher triggers re-cache |
| Memories (P6) | `MemoryState` in memory | State-derived |
| Spine (P7) | `SpineState` — notifications, config | State-derived |
| Logs (P8) | `LogState` — timestamped entries | State-derived |
| Scratchpad (P9) | `ScratchpadState` — cells | State-derived |
| Files | Disk reads | File watcher triggers re-cache |
| Git results | `git` CLI output | Timer-based refresh (120s) |
| GitHub results | `gh` CLI output | Timer-based refresh (120s) |
| Console | Process stdout/stderr | Live streaming |
| ConversationHistory | Frozen message chunks | Static after creation |
| Skills | Skill content from `PromptState` | State-derived |

## Prompt Text Constants

Defined in `yamls/prompts.yaml`, accessed via `src/infra/constants.rs`:

| Constant | Content |
|----------|---------|
| `panel.header` | "Beginning of dynamic panel display. All content displayed below may be considered up to date." |
| `panel.timestamp` | "Panel automatically generated at {iso_time}" |
| `panel.footer` | "End of dynamic panel displays..." + message timestamps + current datetime |
| `panel.footer_ack` | "Panel display complete. Proceeding with conversation." |

## Key Design Decisions

### Why fake tool calls for panels?

Injecting panels as `dynamic_panel` tool call/result pairs (rather than user messages or system prompt sections) has several advantages:
- The LLM interprets them as information it requested, not instructions from the user
- Tool results are naturally "data" — the model treats them as reference material
- The assistant/user alternation is maintained without polluting the real conversation
- Each panel is clearly delineated with its own tool call ID

### Why sort panels by timestamp?

Anthropic's prompt caching uses prefix matching — if the first N tokens of a request match a cached prefix, those tokens are served from cache at reduced cost. By putting the oldest/most stable panels first (tree, todos, memories), the cache hit rate is maximized. Frequently-changing panels (conversation history, console output) go last.

### Why re-inject the system prompt?

After potentially dozens of panel injections (which can be tens of thousands of tokens), the original system prompt in the `system` field may lose salience. The seed re-injection immediately before conversation messages ensures the agent's personality and instructions are fresh in the model's attention window.

### Why exclude `chat` from panels?

The Conversation context element (`id="chat"`) represents the live message stream. Its messages are sent as real conversation messages (with proper role alternation, tool calls, etc.), not as a single panel blob. This preserves the natural conversation structure that LLMs are trained on.

## Source Files

| File | Role |
|------|------|
| `src/app/context.rs` | `prepare_stream_context()` — central orchestration |
| `src/app/run/streaming.rs` | Stream lifecycle — trigger, retry, finalize |
| `src/app/panels.rs` | Panel trait, `refresh_all_panels()`, `collect_all_context()` |
| `src/llms/anthropic.rs` | Anthropic-specific message assembly |
| `src/llms/openai_compat.rs` | OpenAI-compatible message assembly (Grok, Groq, DeepSeek) |
| `src/llms/mod.rs` | `prepare_panel_messages()`, `panel_header_text()`, `panel_footer_text()` |
| `src/infra/constants.rs` | Prompt text constants loaded from YAML |
| `yamls/prompts.yaml` | Panel header, footer, timestamp templates |
| `src/modules/conversation/refresh.rs` | `estimate_message_tokens()` — token counting for messages |
