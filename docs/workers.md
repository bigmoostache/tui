# Workers & Threading Architecture

This document describes all threads, workers, and concurrent processes in the TUI application.

## Overview

The application uses a **single-threaded main loop** with **background worker threads** for I/O-bound operations. Communication between the main thread and workers is done via `std::sync::mpsc` channels.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              MAIN THREAD                                     │
│                            (Tokio runtime)                                   │
│                                                                              │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │   Event     │  │   Render    │  │   State     │  │   Action    │         │
│  │   Loop      │  │   UI        │  │   Mgmt      │  │   Handler   │         │
│  └─────────────┘  └─────────────┘  └─────────────┘  └─────────────┘         │
│         │                                                                    │
│         ▼                                                                    │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                    Channel Receivers (polled)                        │    │
│  │  • rx: StreamEvent        • tldr_rx: TlDrResult                     │    │
│  │  • cache_rx: CacheUpdate  • watcher.poll_events()                   │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────────┘
                                    ▲
                                    │ mpsc channels
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                           WORKER THREADS                                     │
│                                                                              │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐              │
│  │  LLM Streaming  │  │  Cache Workers  │  │   TL;DR Worker  │              │
│  │  (1 at a time)  │  │  (many parallel)│  │  (1 at a time)  │              │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘              │
│                                                                              │
│  ┌─────────────────┐                                                         │
│  │  File Watcher   │  (internal thread from `notify` crate)                 │
│  └─────────────────┘                                                         │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Thread Spawning Locations

### 1. LLM Streaming (`src/llms/mod.rs`)

**Function:** `start_streaming()`

Spawns a thread for each LLM API call to stream responses.

```rust
thread::spawn(move || {
    let client = create_client(&provider);
    client.stream(tx, messages, tools, provider_config);
});
```

- **Trigger:** User sends a message
- **Lifetime:** Until stream completes or is cancelled
- **Channel:** `Sender<StreamEvent>` → `rx` in app.rs
- **Concurrency:** Only one active at a time (previous cancelled if new starts)

**Function:** `start_api_check()`

Spawns a thread to check API connectivity.

```rust
thread::spawn(move || {
    // Makes test API call
    tx.send(ApiCheckResult { ... });
});
```

- **Trigger:** Config panel API check action
- **Lifetime:** Short-lived (single request)
- **Channel:** `Sender<ApiCheckResult>`

### 2. Cache Workers (`src/cache.rs`)

**Function:** `process_cache_request()`

Spawns a new thread for EACH cache refresh operation:

```rust
pub fn process_cache_request(request: CacheRequest, sender: Sender<CacheUpdate>) {
    thread::spawn(move || {
        let update = match request {
            CacheRequest::RefreshFile { id, path } => { /* read file */ }
            CacheRequest::RefreshTree { id, ... } => { /* generate tree */ }
            CacheRequest::RefreshGlob { id, ... } => { /* compute glob */ }
            CacheRequest::RefreshGrep { id, ... } => { /* compute grep */ }
            CacheRequest::RefreshTmux { id, pane_id, lines } => { /* capture pane */ }
            CacheRequest::RefreshGitStatus { id } => { /* git status */ }
        };
        sender.send(update);
    });
}
```

| Request Type | Operation | External Processes |
|--------------|-----------|-------------------|
| `RefreshFile` | Read file from disk | None |
| `RefreshTree` | Generate directory tree string | None |
| `RefreshGlob` | Compute glob pattern matches | None |
| `RefreshGrep` | Search file contents with regex | None |
| `RefreshTmux` | Capture terminal pane content | `tmux capture-pane` |
| `RefreshGitStatus` | Get git repository status | Multiple `git` commands |

- **Trigger:** Various UI actions (file open, tree refresh, etc.)
- **Lifetime:** Short-lived per operation
- **Channel:** `Sender<CacheUpdate>` → `cache_rx` in app.rs
- **Concurrency:** Many can run in parallel

### 3. TL;DR Generation (`src/background.rs`)

**Function:** `generate_tldr()`

Spawns a thread for each message summarization request.

```rust
pub fn generate_tldr(message_id: String, content: String, sender: Sender<TlDrResult>) {
    thread::spawn(move || {
        // Call Anthropic API with blocking client
        let result = reqwest::blocking::Client::new()
            .post(ANTHROPIC_URL)
            .send();
        sender.send(TlDrResult { message_id, tldr });
    });
}
```

- **Trigger:** User summarizes a message (context_message_status)
- **Lifetime:** Until API call completes
- **Channel:** `Sender<TlDrResult>` → `tldr_rx` in app.rs
- **Concurrency:** Multiple can run in parallel

### 4. File Watcher (`src/watcher.rs`)

**Struct:** `FileWatcher` using `notify::RecommendedWatcher`

The `notify` crate internally spawns a background thread for filesystem monitoring.

```rust
impl FileWatcher {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        let watcher = RecommendedWatcher::new(
            move |res| { tx.send(res).ok(); },
            Config::default()
        );
        // ...
    }
}
```

- **Trigger:** Created at app startup
- **Lifetime:** Entire application lifetime
- **Channel:** Internal `mpsc::channel()` polled via `poll_events()`
- **OS APIs:** inotify (Linux), FSEvents (macOS), ReadDirectoryChangesW (Windows)

## External Process Spawning

### Synchronous (blocking main thread)

| Location | Process | Purpose |
|----------|---------|---------|
| `src/actions.rs` | `tmux send-keys` | Send keystrokes to terminal |
| `src/tools/tmux.rs` | `tmux` (various) | Console management |
| `src/tools/git.rs` | `git` (various) | Git operations |
| `src/tools/close_context.rs` | `tmux kill-pane` | Close console panels |

### In Worker Threads (non-blocking)

| Location | Process | Purpose |
|----------|---------|---------|
| `src/cache.rs` (RefreshTmux) | `tmux capture-pane` | Capture terminal output |
| `src/cache.rs` (RefreshGitStatus) | `git rev-parse`, `git status`, `git branch`, `git diff`, `git show` | Git status panel |

## Thread-Safe Shared State

These use synchronization primitives but don't spawn workers:

| Location | Primitive | Purpose |
|----------|-----------|---------|
| `src/config.rs` | `RwLock<String>` | Active theme ID |
| `src/highlight.rs` | `Arc<Mutex<HashMap>>` | Syntax highlighting cache |
| `src/perf.rs` | `AtomicBool`, `AtomicU64`, `RwLock` | Performance metrics |
| `src/watcher.rs` | `Arc<Mutex<HashMap>>` | Watched files/dirs tracking |

## Channel Summary

| Channel | Sender Location | Receiver Location | Message Type |
|---------|-----------------|-------------------|--------------|
| LLM Stream | `src/llms/mod.rs` | `src/core/app.rs` (`rx`) | `StreamEvent` |
| Cache Update | `src/cache.rs` | `src/core/app.rs` (`cache_rx`) | `CacheUpdate` |
| TL;DR Result | `src/background.rs` | `src/core/app.rs` (`tldr_rx`) | `TlDrResult` |
| File Watch | `src/watcher.rs` (notify callback) | `src/watcher.rs` (`event_rx`) | `notify::Event` |

## Main Loop Polling

The main event loop in `src/core/app.rs` polls all channels non-blocking:

```rust
// Simplified main loop structure
loop {
    // 1. Poll terminal events (keyboard/mouse)
    if let Ok(true) = event::poll(Duration::from_millis(16)) {
        // Handle input
    }
    
    // 2. Poll LLM stream events
    while let Ok(event) = rx.try_recv() {
        process_stream_event(event);
    }
    
    // 3. Poll TL;DR results
    while let Ok(result) = tldr_rx.try_recv() {
        process_tldr_result(result);
    }
    
    // 4. Poll cache updates
    while let Ok(update) = cache_rx.try_recv() {
        process_cache_update(update);
    }
    
    // 5. Poll file watcher
    for event in watcher.poll_events() {
        process_watcher_event(event);
    }
    
    // 6. Render UI
    terminal.draw(|f| render(f, &state))?;
}
```

## Files With No Threading/Concurrency

The following files contain no worker threads, channels, or concurrency primitives:

- `src/api.rs` - Re-export module
- `src/constants.rs` - Static constants
- `src/events.rs` - Event mapping logic
- `src/state.rs` - Data structures
- `src/tool_defs.rs` - Tool definitions
- `src/typewriter.rs` - Text animation buffer
- `src/core/context.rs` - Context info struct
- `src/core/init.rs` - State initialization
- `src/core/mod.rs` - Module exports
- `src/help/*.rs` - Help UI rendering
- `src/panels/*.rs` - Panel UI rendering
- `src/tools/*.rs` - Tool implementations (sync process spawning only)
- `src/ui/*.rs` - UI components
