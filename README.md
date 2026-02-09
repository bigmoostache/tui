<div align="center">

# Context Pilot

### Your AI doesn't forget what it just read.

![Rust](https://img.shields.io/badge/rust-1.83+-orange.svg)
![License](https://img.shields.io/badge/license-AGPL--3.0-blue.svg)
![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)

<!-- TODO: screenshot or gif here -->

</div>

---

You know the drill. You open Cursor, Copilot, ChatGPT. You paste code. You explain the project. You paste more code. The AI forgets what you showed it 5 messages ago. You paste it again. The context fills up. You start a new chat. Repeat.

**Context Pilot is what happens when you let the AI manage its own brain.**

It explores your codebase on its own. It opens files, reads them, takes notes, closes them. It searches, greps, runs commands in the terminal. When the conversation gets long, it archives old messages automatically. It never runs out of space because it **cleans up after itself.**

> *"I explored 90 files in one session and ended at 14% context usage. I read everything, understood it, wrote descriptions, and freed the space."*
> — The AI, after its first session ([full review](docs/retex.md))

## What can it do?

**Explore** — It reads files, navigates your directory tree, searches with glob and regex. It annotates what it finds so it remembers later.

**Build** — It edits files, creates new ones, runs terminal commands, manages git branches, opens PRs.

**Think** — It keeps todo lists, scratchpad notes, memories. It plans before it acts.

**Stay sharp** — It tracks every token it's using. When things get heavy, it summarizes old messages, closes files it doesn't need, archives history. You never have to say "you're running out of context."

35 tools. 5 LLM providers (Claude, DeepSeek, Grok, Groq). Runs in your terminal. No Electron. No browser. No VS Code plugin.

## Get started

```bash
git clone https://github.com/bigmoostache/context-pilot.git
cd context-pilot
echo "ANTHROPIC_API_KEY=your_key" > .env
cargo build --release
./run.sh
```

That's it. Talk to it.

## Contribute

This project is young. Your PR won't sit in a queue for 3 months.

Ideas: new LLM provider, new module, color theme (it's YAML), better markdown rendering, tutorials, or just an issue with something you'd want to build.

See [CONTRIBUTING.md](CONTRIBUTING.md) for details.

## License

[AGPL-3.0](LICENSE) for open source. Commercial license available.

---

<div align="center">

**Built with Rust. Runs in your terminal. The AI thinks for itself.**

[Get Started](#get-started) · [Read the AI's Review](docs/retex.md) · [Contribute](#contribute)

</div>
