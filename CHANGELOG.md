# Changelog

All notable changes to Context Pilot will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.0] - 2026-02-09

### Added
- **5 LLM providers** — Anthropic (direct API), Claude Code (OAuth), DeepSeek, Grok (xAI), Groq
- **35 tools** across 9 categories — files, search, git, GitHub, terminal, memory, todos, scratchpad, presets
- **AI-driven context management** — the AI opens, reads, annotates, and closes files on its own
- **Real-time token tracking** — sidebar shows per-element token counts with live progress bar
- **Message lifecycle** — summarize old messages to free context, delete irrelevant ones
- **Directory tree** with persistent annotations and gitignore-style filtering
- **Glob and grep search** with background refresh and pagination
- **Full git integration** — execute any git command, smart cache invalidation via regex rules
- **Full GitHub CLI** — PRs, issues, releases, actions via `gh` with ETag-based polling
- **Tmux terminal management** — create panes, send keys, monitor output
- **Persistent memory** — memories, todos, and scratchpad survive across sessions
- **Preset system** — save and load complete workspace configurations
- **Custom system prompts** — create, edit, and switch AI personalities
- **14 color themes** — tokyo-night, dracula, catppuccin, gruvbox, nord, solarized, and more
- **Command palette** — fuzzy search across all commands (Ctrl-P)
- **Performance monitoring** — F12 overlay with FPS, CPU, RAM, per-operation stats
- **Smart caching** — SHA-256 hash-based change detection, background refresh, inotify file watching
- **Syntax highlighting** — syntect-based with LRU cache
- **Markdown rendering** — tables, inline formatting, headers, bullet points
- **Module system** — activate/deactivate capabilities, dependency validation
- **Multi-worker architecture** — designed for concurrent AI agents (foundation laid)

### Architecture
- Built in **Rust** with Ratatui for terminal UI
- Single-threaded event loop with background workers
- ~15K lines of Rust across 70+ files
- Sub-50ms frame times
- Modular design: 14 independent modules with shared trait interface
