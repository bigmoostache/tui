# Custom Tool Result Visualization

Modules can register custom visualizers to control how their tool results render in the conversation UI. If no visualizer is registered, the core renderer falls back to plain text with wrapping.

## Architecture

```
Module::tool_visualizers() → Vec<(tool_name, VisualizerFn)>
        ↓
build_visualizer_registry() → HashMap<String, VisualizerFn>
        ↓
conversation_render.rs → lookup tool_name → apply or fallback
```

## API

### 1. Define a visualizer function

A visualizer is a function with this signature:

```rust
pub type ToolVisualizer = fn(content: &str, width: usize) -> Vec<ratatui::text::Line<'static>>;
```

- `content`: The raw tool result string (what `ToolResult.content` contains)
- `width`: Available display width in characters
- Returns: Styled terminal lines for the TUI

### 2. Implement the function in your module crate

```rust
fn my_visualizer(content: &str, width: usize) -> Vec<ratatui::text::Line<'static>> {
    use ratatui::prelude::*;

    let mut lines = Vec::new();
    for line in content.lines() {
        // Apply custom styling per line
        let style = if line.starts_with("OK") {
            Style::default().fg(Color::Green)
        } else if line.starts_with("ERR") {
            Style::default().fg(Color::Red)
        } else {
            Style::default().fg(Color::Gray)
        };

        // Truncate to available width
        let display = if line.len() > width {
            format!("{}...", &line[..line.floor_char_boundary(width.saturating_sub(3))])
        } else {
            line.to_string()
        };
        lines.push(Line::from(Span::styled(display, style)));
    }
    lines
}
```

### 3. Register it in your Module trait impl

```rust
use cp_base::modules::ToolVisualizer;

impl Module for MyModule {
    // ... other trait methods ...

    fn tool_visualizers(&self) -> Vec<(&'static str, ToolVisualizer)> {
        vec![
            ("my_tool_name", my_visualizer as ToolVisualizer),
            ("other_tool", my_visualizer as ToolVisualizer),  // same visualizer for multiple tools
        ]
    }
}
```

The key is the **tool name** (the `id` field from `ToolDefinition`), not the display name.

## How it works

1. At startup, `build_visualizer_registry()` collects all `tool_visualizers()` from every module into a `HashMap<String, ToolVisualizer>`.
2. The registry is built lazily (once) via `OnceLock` in `conversation_render.rs`.
3. When rendering a `ToolResult` message, the renderer checks `result.tool_name` against the registry.
4. If a visualizer is found, it's called and the returned `Line`s are rendered with the standard status icon prefix.
5. If no visualizer is found (or `tool_name` is empty for legacy results), plain text fallback with wrapping is used.

## Fallback behavior

- **Legacy tool results** (before `tool_name` was added): `tool_name` defaults to `""` via `#[serde(default)]`, so the plain text fallback is used. No breakage.
- **Unknown tools**: Tools without a registered visualizer use the same plain text fallback.
- **Error results**: The status icon shows error style regardless of visualizer. The visualizer only controls content styling.

## Example: Files module diff visualizer

The `cp-mod-files` crate registers a diff visualizer for `file_edit` and `Write` tools:

```rust
fn visualize_diff(content: &str, width: usize) -> Vec<Line<'static>> {
    // Parses ```diff blocks
    // Lines starting with "- " → red
    // Lines starting with "+ " → green
    // Other lines → secondary color
    // ``` markers are hidden
}
```

This keeps diff-specific rendering logic inside the files module, not in the core conversation renderer.
