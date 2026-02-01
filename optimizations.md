# UX Optimizations

## High Impact

### 1. Dirty Flag Rendering
Currently renders every loop iteration (~125fps). Only re-render when state changes.

```rust
// In App struct
dirty: bool,

// In loop
if self.dirty {
    terminal.draw(|frame| ui::render(frame, &mut self.state))?;
    self.dirty = false;
}

// Set dirty = true when state mutates
```

### 2. Async Tool Execution
Tools block the main thread. Move I/O-heavy tools to background:

```rust
// Read-only tools that don't mutate state can run async:
// - open_file (file I/O)
// - glob (directory walking)
// - grep (file searching)
// - tmux_capture (process spawning)

// Show immediate "Loading..." placeholder, update when ready
```

### 3. Optimistic UI Updates
Show user message immediately, don't wait for stream to start:

```rust
// Currently: User presses Enter → API call starts → message appears
// Better: User presses Enter → message appears instantly → API call starts
```

### 4. Virtualized Scrolling
For long conversations, only render visible messages:

```rust
// Calculate visible range based on scroll position
let visible_range = calculate_visible_messages(scroll_offset, viewport_height);
for msg in &messages[visible_range] {
    render_message(msg);
}
```

## Medium Impact

### 5. Debounced Syntax Highlighting
Cache highlighted output, only re-highlight on content change:

```rust
struct HighlightCache {
    content_hash: u64,
    highlighted: Vec<Line>,
}
```

### 6. Typewriter Tuning
Current settings feel slightly sluggish. Consider:

```rust
// Faster minimum delay
pub const TYPEWRITER_MIN_DELAY_MS: f64 = 2.0;  // was 5.0

// Faster catch-up when stream is done
if self.stream_done {
    chars_to_release.max(10)  // was 2
}
```

### 7. Input Echo Latency
Ensure keystroke → screen is always < 16ms:

```rust
// Process input BEFORE heavy operations
if event::poll(Duration::ZERO)? {  // Non-blocking check
    handle_input();
}
// Then do everything else
```

### 8. Progressive Context Loading
Load file contents lazily, show skeleton first:

```rust
// Instead of blocking on file read:
ContextElement {
    content: ContentState::Loading,  // Show placeholder
}
// Background thread loads, updates to ContentState::Ready(...)
```

## Low Impact (Polish)

### 9. Smooth Scrolling
Animate scroll position instead of jumping:

```rust
// Lerp toward target scroll position each frame
scroll_position += (target_scroll - scroll_position) * 0.3;
```

### 10. Loading Indicators
Visual feedback during operations:
- Spinner while API connects
- Progress bar for file operations
- Pulsing cursor while waiting

### 11. Input Buffering
Queue keystrokes during blocking operations, replay after:

```rust
input_buffer: VecDeque<Event>,
// During tool execution, buffer input
// After completion, process buffered events
```

---

**Biggest wins**: #1 (dirty rendering) and #2 (async tools) would have the most noticeable impact.
