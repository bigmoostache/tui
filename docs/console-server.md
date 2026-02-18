# Console Server Architecture

The console module uses a **client-server architecture** to persist child processes across TUI reloads. The server (`cp-console-server`) is a standalone daemon that owns the actual processes. The TUI (client) communicates with it over a Unix domain socket using JSON-line protocol.

## Why a separate server?

When the TUI reloads (hot-reload, crash recovery, `reload_tui`), its process exits. Any child processes it owned directly would receive SIGHUP and die. The server solves this by outliving the TUI — it holds the `script` process handles and stdin pipes, so sessions survive TUI restarts.

## Process hierarchy

```mermaid
graph TD
    TUI["TUI binary<br/>(cp-mod-console client)"]
    SRV["cp-console-server<br/>(daemon, own session via setsid)"]
    S1["sh -c 'cmd1'<br/>(stdout/stderr → log1)"]
    S2["sh -c 'cmd2'<br/>(stdout/stderr → log2)"]
    C1["child process<br/>(e.g. ssh, cargo build)"]
    C2["child process<br/>(e.g. npm run dev)"]
    LOG1[".context-pilot/console/c_1.log"]
    LOG2[".context-pilot/console/c_2.log"]

    TUI -- "JSON over Unix socket" --> SRV
    SRV -- "owns stdin pipe" --> S1
    SRV -- "owns stdin pipe" --> S2
    S1 --> C1
    S2 --> C2
    S1 -- "stdout/stderr redirect" --> LOG1
    S2 -- "stdout/stderr redirect" --> LOG2
    TUI -. "file poller reads" .-> LOG1
    TUI -. "file poller reads" .-> LOG2
```

Key points:
- The server calls `setsid()` so it is not a child of the TUI.
- On SIGTERM/SIGINT/SIGHUP, the server kills all children and exits cleanly. On abnormal death (SIGKILL), children become orphans — the TUI's orphan cleanup handles them on next startup.
- Output capture: stdout/stderr are redirected to a log file at spawn time. The TUI polls that file into a `RingBuffer` for display.
- Input: the server holds each session's `ChildStdin`. The TUI sends keystrokes via the `send` command, and the server writes them to stdin.

## Communication protocol

```mermaid
sequenceDiagram
    participant TUI as TUI (client)
    participant SRV as cp-console-server

    Note over TUI: On startup
    TUI->>SRV: {"cmd": "ping"}
    SRV-->>TUI: {"ok": true}

    Note over TUI: Create a session
    TUI->>SRV: {"cmd": "create", "key": "c_1", "command": "bash", "log_path": "..."}
    SRV-->>TUI: {"ok": true, "pid": 12345}

    Note over TUI: Send keystrokes
    TUI->>SRV: {"cmd": "send", "key": "c_1", "input": "ls -la\\n"}
    SRV-->>TUI: {"ok": true}

    Note over TUI: Poll status
    TUI->>SRV: {"cmd": "status", "key": "c_1"}
    SRV-->>TUI: {"ok": true, "status": "running"}

    Note over TUI: After TUI reload
    TUI->>SRV: {"cmd": "list"}
    SRV-->>TUI: {"ok": true, "sessions": [...]}
    TUI->>SRV: {"cmd": "status", "key": "c_1"}
    SRV-->>TUI: {"ok": true, "status": "running"}
```

### Commands

| Command    | Fields                                      | Description                                          |
|------------|---------------------------------------------|------------------------------------------------------|
| `ping`     |                                             | Health check. Returns `{"ok": true}`.                |
| `create`   | `key`, `command`, `log_path`, `cwd?`        | Spawn a `sh -c` process. Returns `pid`.              |
| `send`     | `key`, `input`                              | Write bytes to session stdin (escape sequences interpreted). |
| `kill`     | `key`, `force?`                             | SIGTERM then SIGKILL the `script` process.           |
| `remove`   | `key`, `force?`                             | Kill (if running) + remove session from server map.  |
| `status`   | `key`                                       | Poll and return session status + exit code.          |
| `list`     |                                             | Return all sessions with status.                     |
| `shutdown` |                                             | Kill all sessions and exit the server process.       |

## TUI reload lifecycle

```mermaid
sequenceDiagram
    participant TUI1 as TUI (old)
    participant SRV as Server
    participant TUI2 as TUI (new)

    Note over TUI1: save_module_data()
    TUI1->>TUI1: Persist live session metadata<br/>(key, pid, command, log_path)
    TUI1->>TUI1: leak_stdin() to avoid EOF

    Note over TUI1: TUI exits (reload/crash)

    Note over TUI2: init or load_module_data()
    TUI2->>SRV: ping (find_or_create_server)
    SRV-->>TUI2: ok
    TUI2->>SRV: list
    SRV-->>TUI2: sessions: [c_1, c_2, ...]
    Note over TUI2: Compare server sessions<br/>vs saved state
    TUI2->>SRV: status c_1
    SRV-->>TUI2: running
    Note over TUI2: Reconnect: create<br/>SessionHandle + pollers
    Note over TUI2: Orphans (in server,<br/>not in saved state):<br/>remove if terminal
```

## File layout

```
.context-pilot/console/
  server.sock          # Unix domain socket
  server.pid           # Server PID (for manual kill)
  c_1.log              # Output log for session c_1
  c_2.log              # Output log for session c_2
```

## Rebuilding & restarting the server

The server is a long-lived daemon. Unlike the TUI binary which picks up changes on relaunch, **the server keeps running the old binary until explicitly killed**. After changing code in `crates/cp-mod-console/src/server/main.rs`:

```sh
# 1. Build
cargo build --release -p cp-mod-console

# 2. Kill the running server
kill $(cat .context-pilot/console/server.pid)

# 3. Clean stale socket/pid (the old process held the socket)
rm -f .context-pilot/console/server.sock .context-pilot/console/server.pid

# 4. Relaunch TUI — find_or_create_server() spawns the new binary automatically
```

If you need to restart without relaunching the TUI, you can also send the shutdown command directly:

```sh
echo '{"cmd":"shutdown"}' | socat - UNIX-CONNECT:.context-pilot/console/server.sock
rm -f .context-pilot/console/server.sock .context-pilot/console/server.pid
```

The TUI's next `server_request()` call will fail, triggering `find_or_create_server()` which spawns the new binary.

### Binary resolution order

The client (`manager.rs:server_binary_path()`) looks for `cp-console-server` in:

1. **Next to the TUI binary** — deployed/installed scenario
2. **`target/release/`** — `cargo run --release`
3. **`target/debug/`** — `cargo run`

So `cargo build --release -p cp-mod-console` puts the binary where `cargo run --release` will find it. For debug builds, use `cargo build -p cp-mod-console`.

### Process cleanup on server death

On SIGTERM/SIGINT/SIGHUP, the server iterates all sessions, sends SIGTERM (then SIGKILL if needed) to each child, and exits. This is the supervisord/systemd approach — explicit cleanup, no kernel magic. On abnormal death (SIGKILL), children become orphans reparented to PID 1. The TUI's orphan cleanup handles them on next startup.
