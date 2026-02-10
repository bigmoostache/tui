# Ideas — What's Missing

## 1. Diff Review Before Commit — The AI Reviews Itself

Right now the flow is: AI edits → build → test → commit. What's missing is a **structured self-review step**. Before any commit, the AI should be able to diff its own changes, assess them against the original intent, catch mistakes, and either fix them or flag concerns.

Not just "run the tests." A deliberate pause where it looks at what it actually did vs. what was asked. This is the thing that separates a junior dev from a senior one — the senior reviews their own PR before submitting it.

This could be a built-in command or a revery that triggers automatically. The key insight: **the AI that wrote the code is the worst reviewer of it** — so this should ideally be a worker with a separate context, reviewing the diff cold.

---

## 2. Conversation Forking

You can't branch conversations right now. If you want to explore two different approaches to a problem, you have to pick one. You should be able to **fork a conversation** at any point — same context, same history, divergent paths. Try approach A in one fork, approach B in another, then kill the loser.

This is different from workers. Workers are parallel tasks with isolated state. Forking is "what if I went a different direction from this exact point?" It's version control for conversations.

---

## 3. Cost Tracking & Token Economics Dashboard

Every LLM call has a dollar cost. Users need to see:
- Cost per message, per conversation, per session, cumulative
- Token usage broken down by context panels vs. messages vs. tool calls
- Which panels are eating the most tokens relative to their usefulness
- Cache hit rates (prompt caching is implemented — are users getting value from it?)

This isn't just nice-to-have. It's what lets power users optimize their workflow. "Oh, the git panel costs me $0.02 per message and I rarely look at it — let me unload it." Make the economics visible and people will use the tool smarter.

---

## 4. Project Onboarding — Automatic Context Discovery

When you first open Context Pilot in a new repo, it should **automatically explore and build understanding**. Read the README, scan the directory structure, identify the language/framework, find the entry point, understand the build system, check for CI config, look at recent git history.

Then it should populate memories, tree descriptions, and maybe a scratchpad cell with a project summary — so every subsequent conversation starts with real understanding instead of a blank slate.

The cold start is the biggest friction point for adoption. The first 5 minutes of every new project shouldn't be "here's my codebase, let me explain everything."

---

## 5. Checkpoint & Rollback

The AI makes changes, sometimes bad ones. Git is there, but it's coarse-grained. What if Context Pilot took **automatic snapshots** before every file edit — a lightweight checkpoint system that lets you say "undo the last 3 things the AI did" without messing with git history?

A local undo stack specifically for AI-made changes. `git stash` on steroids. The AI itself should be able to use this: "that approach didn't work, let me rollback to before I started and try differently."

---

## 6. MCP Server Support

Model Context Protocol is becoming the standard for tool interoperability. If Context Pilot can **consume MCP servers**, it instantly inherits every tool anyone builds for MCP — database connectors, API clients, Kubernetes, AWS, Jira, Slack, whatever. Instead of building every integration as a module, you define the integration surface once and the ecosystem fills it.

The module system is already close to this conceptually — the gap is just the protocol layer.

---

## 7. Session Replay & Teaching

Record entire sessions — every message, tool call, file edit, panel state — and make them **replayable**. Not just a log. A full timeline you can scrub through, seeing exactly what the AI saw and did at each step.

Why this matters: it turns Context Pilot into a **teaching tool**. Senior devs can record themselves solving a problem with AI assistance, and juniors can replay it step by step. Teams can audit how the AI was used. You can debug bad AI behavior by replaying the exact context it had.

---

## 8. Logging Panel

A dedicated module for tailing log files or log streams. Point it at a file path, a docker container, or a `journalctl` unit, and it becomes a live panel — the AI sees the latest N lines of output and can react to errors, warnings, or patterns in real time.

Different from the console module: consoles are interactive terminals you send commands to. The logging panel is passive observation of output streams. It should support filtering (grep-style), log level highlighting, and auto-scrolling. Combined with notifications (#4), the AI could watch logs and re-engage when it spots an exception — "I see a NullPointerException in the Spring Boot logs, looks like the service I just modified is crashing. Let me check the stacktrace."

This is the missing observability layer. The AI can edit code, run builds, and check git — but it can't watch what happens when the code actually runs in a long-lived process.

---

## The Killer Combo

The one thing that would set Context Pilot apart from Cursor, Windsurf, Copilot Workspace, and everything else:

**Workers (#10) + Notifications (#4) + Self-Review (#1 above) combined: Autonomous multi-agent development with self-review.**

The AI gets a task → spins up a worker → makes changes on a branch → a separate reviewer worker examines the diff → if good, opens a PR → notifications alert you → you approve or reject with a one-liner.

Nobody does this well today. Everyone has single-agent, single-thread, human-in-the-loop-for-everything. The project that cracks **trustworthy autonomous multi-step development** — where the AI can work independently for 20 minutes and come back with a clean PR — that's the one that wins.

The infrastructure for it is already 80% there. The module system, presets, git integration, panel architecture — it's all built. Workers and notifications are the missing pieces, and self-review is the missing workflow.
