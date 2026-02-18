# Console Process Lifecycle: Problem Setup

## Architecture

The TUI manages child processes (user commands, servers, docker containers) through a **console server** — a persistent daemon that owns the processes and communicates with the TUI over a Unix socket.

```
┌─────────┐    Unix Socket    ┌──────────────────┐    PTY/stdin    ┌─────────────┐
│   TUI   │ ◄──────────────► │  Console Server   │ ──────────────► │  script      │
│ (client)│   JSON protocol   │  (daemon)         │                │  └─ bash     │
└─────────┘                   └──────────────────┘                │     └─ cmd   │
                                                                   └─────────────┘
```

### Why a server?

When the TUI exits (for reload or crash), the OS closes **all** its file descriptors. If the TUI directly owned the child processes' stdin pipes, `script` would see EOF and exit. `mem::forget()` on the pipe handle doesn't help — the OS closes leaked fds on process exit regardless of Rust's drop semantics.

The server solves this: it holds the stdin pipes, so TUI exit doesn't affect running processes. After reload, the TUI reconnects to the server and resumes interaction.

## The Problem: Orphan Processes After Server Death

Child processes are spawned with `process_group(0)`, which detaches them into their own process group. This is necessary so that killing the `script` process doesn't propagate signals to unrelated processes (we learned the hard way that `kill -9 -{pid}` on a process group can take out SSH sessions).

However, this creates a problem: **when the server dies, its children survive.** They get reparented to PID 1 (init) and keep running forever with no parent to manage them.

### Scenarios where orphans appear:

1. **Server killed manually** (e.g., `kill $(cat .context-pilot/console/server.pid)`)
2. **Server crashes** (bug, OOM)
3. **Machine reboot where server dies before saving state**

### Current orphan cleanup (insufficient):

On TUI startup, `kill_orphaned_processes()` asks the server for its session list and removes sessions not in the TUI's saved state. But this only works when the server is still alive and remembers the sessions. If the server died and was respawned, the new server has an empty session list — it doesn't know about the old processes.

The `CONTEXT_PILOT_SESSION` env tag mechanism (scanning `/proc/*/environ`) was designed for this, but the **server doesn't set this tag on the processes it spawns**. The tag is only set when the TUI spawns processes directly (pre-server architecture, now dead code).

## What Needs to Happen

1. **Server must set `CONTEXT_PILOT_SESSION={cwd_hash}` env var** on every `script` process it spawns. The tag value should be passed from the TUI in the `create` command.

2. **On TUI startup**, the orphan scanner scans `/proc/*/environ` for processes with our tag that aren't known to the current server → kills them.

3. This handles all failure modes:
   - Server died and respawned → new server doesn't know old processes → env tag scan finds and kills them
   - TUI crashed → on restart, reconnects to server (sessions survive) OR server also died → env tag scan cleans up
   - Normal operation → server tracks everything, no orphans

## Constraints

- **Never kill process groups** (`kill -9 -{pid}`). Only kill individual PIDs. Process group kill has taken out SSH sessions.
- **Multiple TUI instances** can run on the same machine in different directories. The `cwd_hash` in the env tag scopes cleanup to the right workspace.
- **Linux only** — `/proc` scanning doesn't work on macOS. Graceful no-op on unsupported platforms.

## Analysis: Why Orphans Exist

The root cause is two interacting design decisions:

1. **`process_group(0)` on child processes** — Each `script` process was detached into its own process group. This was originally done so that killing a child wouldn't propagate signals sideways to other children or up to the server. But it also means server death doesn't propagate down to children — they get reparented to PID 1.

2. **`setsid()` failed silently** — The server was supposed to call `setsid()` to become a session leader. If it had worked, its death would send SIGHUP to all processes in the session. But `setsid()` was called *after* `process_group(0)` in the TUI's spawn code. Since `process_group(0)` makes PID == PGID (making the process a group leader), the subsequent `setsid()` fails with EPERM — `setsid()` requires the caller to NOT be a process group leader. The inline assembly implementation had no error handling, so this failure was silent.

### Signal propagation table (before fix)

| Event | Effect on children | Effect on server | Effect on SSH |
|-------|-------------------|-----------------|---------------|
| `kill {child_pid}` | That child dies | None | None |
| `kill {server_pid}` | **Nothing** (detached groups) | Server dies | None |
| `kill -9 -{server_pgid}` | **Nothing** (different groups) | Server dies | **DANGEROUS** if group leaks |
| Server crashes | **Nothing** — orphans | Server gone | None |

## Resolution: Proper Session Isolation via `pre_exec` + `setsid()`

### The fix (3 changes):

1. **TUI spawns server with `pre_exec(setsid())`** instead of `process_group(0)`. The `setsid()` call happens in the `pre_exec` hook — before the new process becomes a group leader — so it succeeds. The server becomes a session leader with its own session AND its own process group (setsid creates both). Uses `libc::setsid()` for reliability instead of inline assembly.

2. **Server does NOT call `process_group(0)` on children.** Children inherit the server's session and process group. They are direct members of the server's session.

3. **Server removes its inline `setsid()` call.** No longer needed — already done at spawn time by the TUI's `pre_exec` hook.

### Signal propagation table (after fix)

| Event | Effect on children | Effect on server | Effect on SSH |
|-------|-------------------|-----------------|---------------|
| `kill {child_pid}` | That child dies | None | None |
| `kill {server_pid}` | **SIGHUP → all die** | Server dies | None |
| Server crashes | **SIGHUP → all die** | Server gone | None |
| TUI exits/reloads | None (server is independent session) | None | None |

### Why this is safe:

- **Server's session is fully isolated from SSH.** `setsid()` creates a new session — the server and its children are in a completely separate session from the SSH terminal. No signal path exists between them.
- **We never kill process groups.** All kill commands target individual PIDs (`kill {pid}`), never group IDs (`kill -{pgid}`). A child's death cannot affect siblings, the server, or SSH.
- **Children die with the server.** Since children are in the server's session, kernel sends SIGHUP to all of them when the session leader (server) dies. No orphans in any failure mode.
- **Standard pattern.** This is the same isolation model used by systemd, supervisord, and other process managers.

















 1. Basic sanity — consoles still work

  - Launch TUI, create a console (console_create)
  - Run a command (e.g. ls, echo hello)
  - Verify output appears in the console panel
  - Kill the console, verify it shows exited

  2. Server death kills children

  This is the main thing to verify.

  # In TUI: create a console running a long-lived process
  # e.g. "sleep 9999" or "tail -f /dev/null"

  # In a separate terminal:
  ps aux | grep sleep          # note the sleep PID
  cat .context-pilot/console/server.pid   # note server PID

  kill <server-pid>

  # Wait 1-2 seconds, then:
  ps aux | grep sleep          # should be gone
  kill -0 <sleep-pid>          # should say "No such process"

  If the sleep process is gone, the fix works.

  3. Server death kills multiple sessions

  - Create 2-3 consoles with different long-lived commands (sleep 111, sleep 222, sleep 333)
  - Note all PIDs
  - Kill the server
  - Verify all child processes are gone

  4. Normal TUI exit does NOT kill children

  - Create a console with sleep 9999
  - Exit the TUI normally (quit)
  - ps aux | grep sleep — should still be running (server survives TUI exit)
  - Relaunch TUI — console should reconnect

  5. Graceful shutdown still works

  - Create a console
  - Send shutdown command (or trigger it from TUI if there's a path)
  - Verify sessions are cleaned up (the explicit kill loop in handle_shutdown still runs)

  What "pass" looks like

  ┌─────┬─────────────────────────────┬─────────────────────────────────────────────────────────────┐
  │  #  │            Test             │                       Pass condition                        │
  ├─────┼─────────────────────────────┼─────────────────────────────────────────────────────────────┤
  │ 1   │ Basic sanity                │ Console creates, runs commands, kills cleanly               │
  ├─────┼─────────────────────────────┼─────────────────────────────────────────────────────────────┤
  │ 2   │ Server death → children die │ kill <server-pid> makes all child processes exit within ~1s │
  ├─────┼─────────────────────────────┼─────────────────────────────────────────────────────────────┤
  │ 3   │ Multiple sessions           │ All children from all sessions die on server kill           │
  ├─────┼─────────────────────────────┼─────────────────────────────────────────────────────────────┤
  │ 4   │ TUI exit                    │ Children survive TUI exit, server stays alive               │
  ├─────┼─────────────────────────────┼─────────────────────────────────────────────────────────────┤
  │ 5   │ Graceful shutdown           │ shutdown command still kills sessions and exits server      │
  └─────┴─────────────────────────────┴─────────────────────────────────────────────────────────────┘

  Test #2 is the critical one — that's the bug this commit fixes.
