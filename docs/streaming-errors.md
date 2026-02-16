# Streaming Error Debugging Guide

## reqwest Hides Real Errors

reqwest wraps HTTP body read errors in 3 layers of indirection:

```
io::Error { kind: Other, message: "error decoding response body" }
  └── caused by: hyper error
        └── caused by: TimedOut    ← the real cause
```

`e.kind()` returns `Other`, not `TimedOut`. `e.to_string()` returns the useless
`"error decoding response body"`. The only way to find the real cause is walking
the `std::error::Error::source()` chain:

```rust
let mut source: Option<&dyn std::error::Error> = std::error::Error::source(&e);
while let Some(s) = source {
    println!("caused by: {}", s);
    source = std::error::Error::source(s);
}
```

## Known Root Causes

| Root cause in chain | Meaning | Fix |
|---|---|---|
| `TimedOut` | Model paused too long (thinking before generating large output) | `Client::builder().timeout(None)` |
| `ConnectionReset` | Server killed connection (rate limit, internal error) | Check `anthropic-ratelimit-*` response headers |
| `UnexpectedEof` | Chunked transfer ended prematurely | Server-side issue, retry |

## The file_write Timeout Bug (Feb 2026)

### Symptom

`file_write` (now `Write`) tool calls failed consistently when the LLM tried to create
large files. Small tool calls worked fine. Error message: `"error decoding response body"`.

### Investigation

1. Added verbose error logging to `.context-pilot/errors/` — saw "error decoding response body", no detail
2. Switched from `BufReader::lines()` to manual `read_line()` loop — captured stream position, in-flight tool state, last SSE lines
3. Logged response headers — revealed rate limit info but not the cause
4. Logged `e.kind()` and `std::error::Error::source()` chain — finally found `caused by: TimedOut`

### Root Cause

reqwest's blocking `Client::new()` has a default read timeout (~30 seconds). When the LLM
generates a `Write` tool call, it streams the `file_path` field first, then pauses to
"think" about the large `content` field. The pause exceeds the timeout → reqwest kills
the read → wraps it in a misleading "error decoding response body" error.

The server sends `ping` events as keepalives, but reqwest's timeout is on the HTTP body
decoder level, not on individual reads — the pings don't prevent the timeout.

### Fix

```rust
let client = Client::builder()
    .timeout(None)  // SSE streams need no timeout — server sends pings
    .build()?;
```

## Error Logging Infrastructure

Every API error (including retry attempts) is logged to `.context-pilot/errors/error_N.txt` with:
- Attempt number and retry status
- Provider and model name
- Error kind + full source chain
- Stream position (bytes read, lines read)
- In-flight tool name and partial input byte count
- Response headers (reveals rate-limit status)
- Last 5 SSE data lines before the error

The status bar shows a `RETRY N/M` badge during retry attempts.
