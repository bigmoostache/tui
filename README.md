<div align="center">

# Context Pilot

[![Stars](https://img.shields.io/github/stars/bigmoostache/context-pilot?style=social)](https://github.com/bigmoostache/context-pilot/stargazers)
[![CI](https://github.com/bigmoostache/context-pilot/actions/workflows/rust.yml/badge.svg)](https://github.com/bigmoostache/context-pilot/actions)
![Rust](https://img.shields.io/badge/rust-1.83+-orange.svg)
![License](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)

<br/>

<img src="docs/image copy.png" alt="Context Pilot" width="900"/>

<br/>

### Your AI coding assistant has amnesia. This one doesn't.

[Get Started](#install) · [How It Works](#how-it-works) · [Website](https://bigmoostache.github.io/context-pilot/)

</div>

---

## The idea

Every AI coding tool has the same problem: context is invisible. You paste code, the AI forgets it three messages later, you paste it again. The context window fills up and nobody — not you, not the AI — knows what's in it or what got pushed out.

Context Pilot makes context **visible**. Every piece of information the AI touches — every file, search result, terminal pane, memory — is a **panel** with a live token count in a sidebar. The AI can see its own brain. It opens what it needs, closes what it doesn't, takes notes on what it read, and when the conversation gets long, it archives old messages to make room.

The result: **90+ files explored in a single session, ending at 14% context usage.** It read everything, understood it, annotated it, and freed the space. Not because we told it to — because it could see it needed to. ([Full writeup](docs/retex.md))

## How it works

```
┌── Sidebar ──────┐  ┌── Main Panel ──────────────────────────┐
│                  │  │                                        │
│  ◉ Conversation  │  │   Currently viewing: src/core/app.rs   │
│  P1 Todo         │  │                                        │
│  P2 Library      │  │   fn handle_action(&mut self, ...) {   │
│  P3 Overview     │  │       match action {                   │
│  P4 Tree         │  │           Action::Key(key) => {        │
│  P5 Memory       │  │               self.process_key(key);   │
│  P6 Spine        │  │           }                            │
│  P7 Logs         │  │           ...                          │
│  P8 Git          │  │       }                                │
│  P9 Scratchpad   │  │   }                                    │
│  ─────────────── │  │                                        │
│  P10 app.rs  6K  │  │                                        │
│  P11 grep    2K  │  │                                        │
│  P12 git log 1K  │  │                                        │
│  P13 tmux %1 3K  │  │                                        │
│                  │  │                                        │
│  8,231 / 200K    │  │                                        │
│  ████░░░░░░ 4%   │  │                                        │
└──────────────────┘  └────────────────────────────────────────┘
```

**Fixed panels** (P1–P9) are always there — todos, memories, tree, git, scratchpad. **Dynamic panels** (P10+) are created and destroyed by the AI as it works: open a file, run a search, start a terminal, check a PR.

The token count at the bottom is real. The AI reads it. When `app.rs` is eating 6K tokens and it's done reading, it closes the panel. When conversation history grows too large, it gets automatically archived into browsable history panels. No manual context management, ever.

## What makes it different

**The AI manages its own context.** This isn't a feature — it's the architecture. Other tools give the AI a hidden context window and hope for the best. Context Pilot gives the AI a visible, manipulable workspace with 47 tools:

- **Explore** — open files, navigate directories, glob and grep. Annotate everything with descriptions that persist after closing.
- **Edit** — surgical text replacement. The AI sees exact file content and matches it.
- **Run** — full tmux integration. Terminal panes as context panels. Build, test, interact with running processes.
- **Git** — full git + GitHub CLI. Branch, commit, diff, push, open PRs. Mutating commands auto-refresh affected panels.
- **Remember** — memories persist across sessions. Todos, scratchpad, timestamped logs. Old conversation chunks get archived, not lost.
- **Configure** — switch agent personalities, load skill documents, save/restore workspace presets, enable/disable individual tools.

<details>
<summary><b>Full tool list (47)</b></summary>

| Category | Tools |
|----------|-------|
| **Context** | `context_close` · `system_reload` · `tool_manage` · `module_toggle` · `panel_goto_page` |
| **Agents & Skills** | `agent_create` · `agent_edit` · `agent_delete` · `agent_load` · `skill_create` · `skill_edit` · `skill_delete` · `skill_load` · `skill_unload` · `command_create` · `command_edit` · `command_delete` |
| **Files** | `file_open` · `file_edit` · `file_write` · `file_glob` · `file_grep` |
| **Tree** | `tree_filter` · `tree_toggle` · `tree_describe` |
| **Git & GitHub** | `git_execute` · `git_configure_p6` · `gh_execute` |
| **Terminal** | `console_create` · `console_edit` · `console_send_keys` · `console_sleep` |
| **Notes** | `todo_create` · `todo_update` · `todo_move` · `memory_create` · `memory_update` · `scratchpad_create_cell` · `scratchpad_edit_cell` · `scratchpad_wipe` |
| **Presets** | `preset_snapshot_myself` · `preset_load` |
| **Spine** | `notification_mark_processed` · `spine_configure` |
| **Logs** | `log_create` · `log_summarize` · `log_toggle` · `close_conversation_history` |

</details>

## Under the hood

Rust. Single binary. ~15K lines. [Ratatui](https://github.com/ratatui/ratatui) + crossterm.

- **14 modules** — each provides tools and panels: core, files, git, github, glob, grep, logs, memory, preset, prompt, scratchpad, spine, tmux, todo, tree
- **5 LLM providers** — Anthropic, Claude Code (OAuth), DeepSeek, Grok (xAI), Groq
- **Smart caching** — SHA-256 change detection, background refresh, inotify file watching. Open files auto-update when changed on disk.
- **Autonomous mode** — the Spine module can auto-continue across multiple turns with guard rails: token limits, cost caps, duration limits, message caps
- **Conversation detachment** — old messages are automatically archived into browsable history panels based on both message count and token thresholds

## Install

### Prerequisites
- **Rust 1.83+** — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **tmux** — `apt install tmux` / `brew install tmux`
- **An API key** — Anthropic, xAI, Groq, or DeepSeek

### Setup

```bash
git clone https://github.com/bigmoostache/context-pilot.git
cd context-pilot

# Add your API key(s)
cat > .env << 'EOF'
ANTHROPIC_API_KEY=your_key_here
# XAI_API_KEY=your_key
# GROQ_API_KEY=your_key
# DEEPSEEK_API_KEY=your_key
# GITHUB_TOKEN=your_token
EOF

cargo build --release
./run.sh
```

### First session

Just talk to it:

```
> explore this codebase and tell me what you find
> find all TODO comments and create a plan to fix them
> create a branch, implement the fix, and open a PR
```

Watch the sidebar. You'll see it open files, read them, annotate the tree, close them, and move on. That's the whole point.

## Contribute

This project is young and moving fast.

- 🆕 New LLM providers (OpenAI, Gemini, Ollama)
- 🎨 Color themes (see `yamls/themes.yaml` — 14 built-in)
- 🧪 Test coverage
- 📖 Tutorials and guides
- 🐛 Bug reports and feature requests

See [CONTRIBUTING.md](CONTRIBUTING.md) for details.

## License

[AGPL-3.0](LICENSE) — open source. Commercial license available — [open an issue](https://github.com/bigmoostache/context-pilot/issues/new).

---

<div align="center">

⭐ **Star the repo if this is useful** — it helps others find it.

</div>
