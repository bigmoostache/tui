use std::fs;

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::highlight::highlight_file;
use crate::state::{ContextType, MessageStatus, MessageType, State};
use crate::tools::{compute_glob_results, generate_directory_tree};

// Warm theme with original color palette
mod theme {
    use ratatui::style::Color;

    // Primary brand colors
    pub const ACCENT: Color = Color::Rgb(218, 118, 89);        // #DA7659 - warm orange
    pub const ACCENT_DIM: Color = Color::Rgb(178, 98, 69);     // Dimmed warm orange
    pub const SUCCESS: Color = Color::Rgb(134, 188, 111);      // Soft green
    pub const WARNING: Color = Color::Rgb(229, 192, 123);      // Warm amber

    // Text colors
    pub const TEXT: Color = Color::Rgb(240, 240, 240);         // #f0f0f0 - primary text
    pub const TEXT_SECONDARY: Color = Color::Rgb(180, 180, 180); // Secondary text
    pub const TEXT_MUTED: Color = Color::Rgb(144, 144, 144);   // #909090 - muted text

    // Background colors
    pub const BG_BASE: Color = Color::Rgb(34, 34, 32);         // #222220 - darkest background
    pub const BG_SURFACE: Color = Color::Rgb(51, 51, 49);      // #333331 - content panels
    pub const BG_ELEVATED: Color = Color::Rgb(66, 66, 64);     // Elevated elements
    pub const BG_INPUT: Color = Color::Rgb(58, 58, 56);        // #3a3a38 - input field

    // Border colors
    pub const BORDER: Color = Color::Rgb(66, 66, 64);          // Subtle border
    pub const BORDER_FOCUS: Color = Color::Rgb(218, 118, 89);  // Accent color for focus

    // Role-specific colors
    pub const USER: Color = Color::Rgb(218, 118, 89);          // Warm orange for user
    pub const ASSISTANT: Color = Color::Rgb(144, 144, 144);    // Muted for assistant
}

// Box drawing and UI characters
mod chars {
    pub const HORIZONTAL: &str = "─";
    pub const BLOCK_FULL: &str = "█";
    pub const BLOCK_LIGHT: &str = "░";
    pub const DOT: &str = "●";
    pub const ARROW_RIGHT: &str = "▸";
}

pub fn render(frame: &mut Frame, state: &mut State) {
    let area = frame.area();

    // Fill base background
    frame.render_widget(
        Block::default().style(Style::default().bg(theme::BG_BASE)),
        area
    );

    // Main layout: body + footer (no header)
    let main_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),     // Body
            Constraint::Length(1),  // Status bar
        ])
        .split(area);

    render_body(frame, state, main_layout[0]);
    render_status_bar(frame, state, main_layout[1]);
}

fn render_body(frame: &mut Frame, state: &mut State, area: Rect) {
    // Body layout: sidebar + main content
    let body_layout = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(36),  // Sidebar
            Constraint::Min(1),      // Main content
        ])
        .split(area);

    render_sidebar(frame, state, body_layout[0]);
    render_main_content(frame, state, body_layout[1]);
}

fn render_sidebar(frame: &mut Frame, state: &State, area: Rect) {
    let base_style = Style::default().bg(theme::BG_BASE);

    // Sidebar layout: context list + help hints
    let sidebar_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),      // Context list
            Constraint::Length(6),   // Help hints
        ])
        .split(area);

    // Context list
    let mut lines: Vec<Line> = vec![
        Line::from(vec![
            Span::styled("  ", base_style),
            Span::styled("CONTEXT", Style::default().fg(theme::TEXT_MUTED).bold()),
        ]),
        Line::from(""),
    ];

    let total_tokens: usize = state.context.iter().map(|c| c.token_count).sum();
    let max_tokens = 100_000; // Approximate max for visual bar

    // Calculate shortcut width for alignment (P1-P9 = 2 chars, P10+ = 3 chars)
    let shortcut_width = if state.context.len() >= 10 { 3 } else { 2 };

    for (i, ctx) in state.context.iter().enumerate() {
        let is_selected = i == state.selected_context;
        let icon = ctx.context_type.icon();

        // Build the line with right-aligned shortcut
        let shortcut = format!("{:>width$}", format!("P{}", i + 1), width = shortcut_width);
        let name = truncate_string(&ctx.name, 18);
        let tokens = format_number(ctx.token_count);

        let line_style = if is_selected {
            Style::default().bg(theme::BG_ELEVATED)
        } else {
            base_style
        };

        let indicator = if is_selected { chars::ARROW_RIGHT } else { " " };

        lines.push(Line::from(vec![
            Span::styled(format!(" {}", indicator), Style::default().fg(theme::ACCENT).bg(if is_selected { theme::BG_ELEVATED } else { theme::BG_BASE })),
            Span::styled(format!(" {} ", shortcut), Style::default().fg(theme::TEXT_MUTED).bg(if is_selected { theme::BG_ELEVATED } else { theme::BG_BASE })),
            Span::styled(format!("{} ", icon), line_style.fg(theme::ACCENT)),
            Span::styled(format!("{:<18}", name), line_style.fg(if is_selected { theme::TEXT } else { theme::TEXT_SECONDARY })),
            Span::styled(format!("{:>6}", tokens), line_style.fg(theme::ACCENT_DIM)),
            Span::styled(" ", line_style),
        ]));
    }

    // Separator
    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(format!(" {}", chars::HORIZONTAL.repeat(34)), Style::default().fg(theme::BORDER)),
    ]));

    // Token usage bar - full width
    let usage_pct = (total_tokens as f64 / max_tokens as f64 * 100.0).min(100.0);
    let bar_width = 34; // Full sidebar width minus margins
    let filled = ((usage_pct / 100.0) * bar_width as f64) as usize;
    let empty = bar_width - filled;

    let bar_color = if usage_pct > 80.0 {
        theme::WARNING
    } else {
        theme::ACCENT
    };

    // Format: "12.5K/100K (45%)"
    let current = format_number(total_tokens);
    let max = format_number(max_tokens);
    let pct = format!("{:.0}%", usage_pct);

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled(" ", base_style),
        Span::styled(&current, Style::default().fg(theme::TEXT).bold()),
        Span::styled("/", Style::default().fg(theme::TEXT_MUTED)),
        Span::styled(&max, Style::default().fg(theme::ACCENT).bold()),
        Span::styled(format!(" ({})", pct), Style::default().fg(theme::TEXT_MUTED)),
    ]));
    lines.push(Line::from(vec![
        Span::styled(" ", base_style),
        Span::styled(chars::BLOCK_FULL.repeat(filled), Style::default().fg(bar_color)),
        Span::styled(chars::BLOCK_LIGHT.repeat(empty), Style::default().fg(theme::BG_ELEVATED)),
    ]));

    let paragraph = Paragraph::new(lines)
        .style(base_style);
    frame.render_widget(paragraph, sidebar_layout[0]);

    // Help hints at bottom of sidebar
    let help_lines = vec![
        Line::from(""),
        Line::from(vec![
            Span::styled("  ", base_style),
            Span::styled("Shift+Enter", Style::default().fg(theme::ACCENT)),
            Span::styled(" send", Style::default().fg(theme::TEXT_MUTED)),
        ]),
        Line::from(vec![
            Span::styled("  ", base_style),
            Span::styled("Ctrl+L", Style::default().fg(theme::ACCENT)),
            Span::styled(" clear", Style::default().fg(theme::TEXT_MUTED)),
        ]),
        Line::from(vec![
            Span::styled("  ", base_style),
            Span::styled("Ctrl+Y", Style::default().fg(theme::ACCENT)),
            Span::styled(" copy mode", Style::default().fg(theme::TEXT_MUTED)),
        ]),
        Line::from(vec![
            Span::styled("  ", base_style),
            Span::styled("Ctrl+Q", Style::default().fg(theme::ACCENT)),
            Span::styled(" quit", Style::default().fg(theme::TEXT_MUTED)),
        ]),
    ];

    let help_paragraph = Paragraph::new(help_lines)
        .style(base_style);
    frame.render_widget(help_paragraph, sidebar_layout[1]);
}

fn render_main_content(frame: &mut Frame, state: &mut State, area: Rect) {
    // Calculate input height based on content
    let input_lines = state.input.lines().count().max(1);
    let input_height = (input_lines as u16 + 2).clamp(4, 12);

    let content_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(1),                    // Content panel
            Constraint::Length(input_height),      // Input area
        ])
        .split(area);

    render_content_panel(frame, state, content_layout[0]);
    render_input(frame, state, content_layout[1]);
}

fn render_content_panel(frame: &mut Frame, state: &mut State, area: Rect) {
    let selected = state.context.get(state.selected_context);

    match selected.map(|c| c.context_type) {
        Some(ContextType::Conversation) => render_conversation(frame, state, area),
        Some(ContextType::File) => render_file(frame, state, area),
        Some(ContextType::Tree) => render_tree(frame, state, area),
        Some(ContextType::Glob) => render_glob(frame, state, area),
        None => render_conversation(frame, state, area),
    }
}

fn render_conversation(frame: &mut Frame, state: &mut State, area: Rect) {
    let base_style = Style::default().bg(theme::BG_SURFACE);

    // Add margin around the panel
    let inner_area = Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(2),
        area.height
    );

    // Panel with rounded border
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .style(base_style)
        .title(Span::styled(
            if state.is_streaming { " Conversation ● " } else { " Conversation " },
            Style::default().fg(theme::ACCENT).bold()
        ))
        .title_alignment(Alignment::Left);

    let content_area = block.inner(inner_area);
    frame.render_widget(block, inner_area);

    // Build conversation content
    let mut text: Vec<Line> = Vec::new();

    if state.messages.is_empty() {
        // Empty state
        text.push(Line::from(""));
        text.push(Line::from(""));
        text.push(Line::from(vec![
            Span::styled("  Start a conversation by typing below", Style::default().fg(theme::TEXT_MUTED).italic()),
        ]));
    } else {
        for msg in &state.messages {
            if msg.status == MessageStatus::Forgotten {
                continue;
            }

            // Handle tool call messages differently
            if msg.message_type == MessageType::ToolCall {
                // Render each tool use in this message
                for tool_use in &msg.tool_uses {
                    // Tool header with id
                    let label = msg.id.clone();

                    text.push(Line::from(vec![
                        Span::styled("⚙ ", Style::default().fg(theme::SUCCESS)),
                        Span::styled(label, Style::default().fg(theme::SUCCESS).bold()),
                        Span::styled(format!("  {}", tool_use.name), Style::default().fg(theme::TEXT_SECONDARY)),
                    ]));

                    // Tool parameters
                    if let Some(obj) = tool_use.input.as_object() {
                        for (key, value) in obj {
                            let value_str = match value {
                                serde_json::Value::String(s) => {
                                    if s.len() > 60 {
                                        format!("\"{}...\"", &s[..57])
                                    } else {
                                        format!("\"{}\"", s)
                                    }
                                }
                                _ => value.to_string(),
                            };
                            text.push(Line::from(vec![
                                Span::styled("    ", base_style),
                                Span::styled(format!("{}: ", key), Style::default().fg(theme::TEXT_MUTED)),
                                Span::styled(value_str, Style::default().fg(theme::TEXT_SECONDARY)),
                            ]));
                        }
                    }

                    text.push(Line::from(""));
                }
                continue;
            }

            // Handle tool result messages - show result info
            if msg.message_type == MessageType::ToolResult {
                for result in &msg.tool_results {
                    let status_icon = if result.is_error { "✗" } else { "✓" };
                    let status_color = if result.is_error { theme::WARNING } else { theme::SUCCESS };

                    text.push(Line::from(vec![
                        Span::styled(format!(" {} ", status_icon), Style::default().fg(status_color)),
                        Span::styled("Result", Style::default().fg(status_color).bold()),
                    ]));

                    // Show truncated result content
                    let content_preview: String = result.content.lines().next().unwrap_or("").chars().take(80).collect();
                    if !content_preview.is_empty() {
                        let display = if result.content.len() > 80 || result.content.lines().count() > 1 {
                            format!("{}...", content_preview)
                        } else {
                            content_preview
                        };
                        text.push(Line::from(vec![
                            Span::styled("    ", base_style),
                            Span::styled(display, Style::default().fg(theme::TEXT_SECONDARY)),
                        ]));
                    }

                    text.push(Line::from(""));
                }
                continue;
            }

            // Regular text message
            let (role_label, role_icon, role_color) = if msg.role == "user" {
                (msg.id.clone(), "▸", theme::USER)
            } else {
                (msg.id.clone(), "●", theme::ASSISTANT)
            };

            let status_badge = match msg.status {
                MessageStatus::Summarized => Some((" TL;DR ", theme::WARNING)),
                MessageStatus::Forgotten => Some((" hidden ", theme::TEXT_MUTED)),
                MessageStatus::Full => None,
            };

            let mut header_spans = vec![
                Span::styled(format!(" {} ", role_icon), Style::default().fg(role_color)),
                Span::styled(role_label.clone(), Style::default().fg(role_color).bold()),
            ];

            if let Some((badge_text, badge_color)) = status_badge {
                header_spans.push(Span::styled(" ", base_style));
                header_spans.push(Span::styled(
                    badge_text,
                    Style::default().fg(theme::BG_BASE).bg(badge_color)
                ));
            }

            text.push(Line::from(header_spans));

            // Message content
            let content = match msg.status {
                MessageStatus::Summarized => msg.tl_dr.as_deref().unwrap_or(&msg.content),
                _ => &msg.content,
            };

            if !content.trim().is_empty() {
                for line in content.lines() {
                    text.push(Line::from(vec![
                        Span::styled("    ", base_style),
                        Span::styled(line.to_string(), Style::default().fg(theme::TEXT)),
                    ]));
                }
            } else if msg.role == "assistant" && state.is_streaming && state.messages.last().map(|m| m.id.clone()) == Some(msg.id.clone()) {
                // Show thinking indicator only for the last message if streaming
                text.push(Line::from(vec![
                    Span::styled("    ", base_style),
                    Span::styled("Thinking...", Style::default().fg(theme::TEXT_MUTED).italic()),
                ]));
            }

            text.push(Line::from(""));
        }
    }

    // Padding at end for scroll
    for _ in 0..3 {
        text.push(Line::from(""));
    }

    // Calculate scroll
    let viewport_width = content_area.width.saturating_sub(2) as usize;
    let viewport_height = content_area.height as usize;

    let content_height: usize = text.iter()
        .map(|line| {
            let char_count: usize = line.spans.iter()
                .map(|span| span.content.chars().count())
                .sum();
            if char_count == 0 || viewport_width == 0 { 1 } else { (char_count + viewport_width - 1) / viewport_width }
        })
        .sum();

    let max_scroll = content_height.saturating_sub(viewport_height) as f32;

    if state.user_scrolled && state.scroll_offset >= max_scroll - 0.5 {
        state.user_scrolled = false;
    }
    if !state.user_scrolled {
        state.scroll_offset = max_scroll;
    }
    state.scroll_offset = state.scroll_offset.clamp(0.0, max_scroll);

    let paragraph = Paragraph::new(text)
        .style(base_style)
        .wrap(Wrap { trim: false })
        .scroll((state.scroll_offset.round() as u16, 0));

    frame.render_widget(paragraph, content_area);

    // Scrollbar
    if content_height > viewport_height {
        let scrollbar = Scrollbar::default()
            .orientation(ScrollbarOrientation::VerticalRight)
            .style(Style::default().fg(theme::BG_ELEVATED))
            .thumb_style(Style::default().fg(theme::ACCENT_DIM));

        let mut scrollbar_state = ScrollbarState::new(max_scroll as usize)
            .position(state.scroll_offset.round() as usize);

        frame.render_stateful_widget(
            scrollbar,
            inner_area.inner(Margin { horizontal: 0, vertical: 1 }),
            &mut scrollbar_state
        );
    }
}

fn render_file(frame: &mut Frame, state: &State, area: Rect) {
    let base_style = Style::default().bg(theme::BG_SURFACE);
    let selected = state.context.get(state.selected_context);

    let (title, content, file_path) = if let Some(ctx) = selected {
        let path = ctx.file_path.as_deref().unwrap_or("");
        let content = if !path.is_empty() {
            fs::read_to_string(path).unwrap_or_else(|e| format!("Error reading file: {}", e))
        } else {
            "No file path".to_string()
        };
        (ctx.name.clone(), content, path.to_string())
    } else {
        ("File".to_string(), String::new(), String::new())
    };

    let inner_area = Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(2),
        area.height
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .style(base_style)
        .title(Span::styled(format!(" {} ", title), Style::default().fg(theme::ACCENT).bold()));

    let content_area = block.inner(inner_area);
    frame.render_widget(block, inner_area);

    // Get syntax highlighting
    let highlighted = if !file_path.is_empty() {
        highlight_file(&file_path, &content)
    } else {
        Vec::new()
    };

    let mut text: Vec<Line> = Vec::new();

    if highlighted.is_empty() {
        for (i, line) in content.lines().enumerate() {
            let line_num = i + 1;
            text.push(Line::from(vec![
                Span::styled(format!(" {:4} ", line_num), Style::default().fg(theme::TEXT_MUTED).bg(theme::BG_BASE)),
                Span::styled(" ", base_style),
                Span::styled(line, Style::default().fg(theme::TEXT)),
            ]));
        }
    } else {
        for (i, spans) in highlighted.iter().enumerate() {
            let line_num = i + 1;
            let mut line_spans = vec![
                Span::styled(format!(" {:4} ", line_num), Style::default().fg(theme::TEXT_MUTED).bg(theme::BG_BASE)),
                Span::styled(" ", base_style),
            ];

            for (color, text) in spans {
                line_spans.push(Span::styled(text.clone(), Style::default().fg(*color)));
            }

            text.push(Line::from(line_spans));
        }
    }

    let paragraph = Paragraph::new(text)
        .style(base_style)
        .scroll((state.scroll_offset.round() as u16, 0));

    frame.render_widget(paragraph, content_area);
}

fn render_tree(frame: &mut Frame, state: &mut State, area: Rect) {
    let base_style = Style::default().bg(theme::BG_SURFACE);
    let tree_content = generate_directory_tree(state);

    let inner_area = Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(2),
        area.height
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .style(base_style)
        .title(Span::styled(" Directory Tree ", Style::default().fg(theme::ACCENT).bold()));

    let content_area = block.inner(inner_area);
    frame.render_widget(block, inner_area);

    let mut text: Vec<Line> = Vec::new();
    for line in tree_content.lines() {
        if let Some(size_start) = find_size_pattern(line) {
            let (main_part, size_part) = line.split_at(size_start);
            text.push(Line::from(vec![
                Span::styled(format!(" {}", main_part), Style::default().fg(theme::TEXT)),
                Span::styled(size_part.trim_end(), Style::default().fg(theme::ACCENT_DIM)),
            ]));
        } else {
            text.push(Line::from(vec![
                Span::styled(format!(" {}", line), Style::default().fg(theme::TEXT)),
            ]));
        }
    }

    let paragraph = Paragraph::new(text)
        .style(base_style)
        .scroll((state.scroll_offset.round() as u16, 0));

    frame.render_widget(paragraph, content_area);
}

fn find_size_pattern(line: &str) -> Option<usize> {
    let trimmed = line.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    let last_char = trimmed.chars().last()?;
    if !matches!(last_char, 'B' | 'K' | 'M') {
        return None;
    }

    let bytes = trimmed.as_bytes();
    let mut num_start = bytes.len() - 1;

    while num_start > 0 && bytes[num_start - 1].is_ascii_digit() {
        num_start -= 1;
    }

    if num_start > 0 && bytes[num_start - 1] == b' ' {
        Some(num_start - 1)
    } else {
        None
    }
}

fn render_glob(frame: &mut Frame, state: &State, area: Rect) {
    let base_style = Style::default().bg(theme::BG_SURFACE);
    let selected = state.context.get(state.selected_context);

    let (title, content) = if let Some(ctx) = selected {
        let pattern = ctx.glob_pattern.as_deref().unwrap_or("*");
        let search_path = ctx.glob_path.as_deref().unwrap_or(".");
        let (results, count) = compute_glob_results(pattern, search_path);
        (format!("{} ({} files)", ctx.name, count), results)
    } else {
        ("Glob".to_string(), String::new())
    };

    let inner_area = Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(2),
        area.height
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .style(base_style)
        .title(Span::styled(format!(" {} ", title), Style::default().fg(theme::ACCENT).bold()));

    let content_area = block.inner(inner_area);
    frame.render_widget(block, inner_area);

    let text: Vec<Line> = content.lines()
        .map(|line| Line::from(vec![
            Span::styled(format!("  {} ", chars::DOT), Style::default().fg(theme::ACCENT_DIM)),
            Span::styled(line, Style::default().fg(theme::TEXT)),
        ]))
        .collect();

    let paragraph = Paragraph::new(text)
        .style(base_style)
        .scroll((state.scroll_offset.round() as u16, 0));

    frame.render_widget(paragraph, content_area);
}

fn render_input(frame: &mut Frame, state: &State, area: Rect) {
    let inner_area = Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(2),
        area.height
    );

    let is_empty = state.input.is_empty();
    let border_color = if state.is_streaming {
        theme::TEXT_MUTED
    } else {
        theme::BORDER_FOCUS
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(theme::BG_INPUT))
        .title(Span::styled(
            if state.is_streaming { " Streaming... " } else { " Message " },
            Style::default().fg(if state.is_streaming { theme::TEXT_MUTED } else { theme::ACCENT })
        ));

    let content_area = block.inner(inner_area);
    frame.render_widget(block, inner_area);

    // Input content or placeholder
    let content = if is_empty && !state.is_streaming {
        vec![Line::from(vec![
            Span::styled(" Type your message here...", Style::default().fg(theme::TEXT_MUTED).italic()),
        ])]
    } else {
        state.input.split('\n')
            .map(|line| Line::from(vec![
                Span::styled(" ", Style::default()),
                Span::styled(line, Style::default().fg(theme::TEXT)),
            ]))
            .collect()
    };

    let paragraph = Paragraph::new(content)
        .style(Style::default().bg(theme::BG_INPUT));

    frame.render_widget(paragraph, content_area);

    // Cursor positioning
    if !state.is_streaming && !is_empty {
        let before_cursor = &state.input[..state.input_cursor];
        let line_num = before_cursor.matches('\n').count();
        let line_start = before_cursor.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col = state.input_cursor - line_start;

        frame.set_cursor_position(Position::new(
            content_area.x + col as u16 + 1,
            content_area.y + line_num as u16,
        ));
    } else if !state.is_streaming {
        // Cursor at start for empty input
        frame.set_cursor_position(Position::new(
            content_area.x + 1,
            content_area.y,
        ));
    }
}

fn render_status_bar(frame: &mut Frame, state: &State, area: Rect) {
    let base_style = Style::default().bg(theme::BG_BASE).fg(theme::TEXT_MUTED);

    let mut spans = vec![
        Span::styled(" ", base_style),
    ];

    // Mode indicator
    if state.copy_mode {
        spans.push(Span::styled(" COPY ", Style::default().fg(theme::BG_BASE).bg(theme::WARNING).bold()));
        spans.push(Span::styled(" Press Esc to exit copy mode ", base_style));
    } else if state.is_streaming {
        spans.push(Span::styled(" STREAMING ", Style::default().fg(theme::BG_BASE).bg(theme::SUCCESS).bold()));
        spans.push(Span::styled(" Press Esc to stop ", base_style));
    } else {
        spans.push(Span::styled(" READY ", Style::default().fg(theme::BG_BASE).bg(theme::ACCENT).bold()));
    }

    // TL;DR indicator
    if state.pending_tldrs > 0 {
        spans.push(Span::styled(
            format!("  {} Summarizing {} message{}...", chars::DOT, state.pending_tldrs, if state.pending_tldrs > 1 { "s" } else { "" }),
            Style::default().fg(theme::WARNING)
        ));
    }

    // Right side info
    let char_count = state.input.chars().count();
    let right_info = if char_count > 0 {
        format!("{} chars ", char_count)
    } else {
        String::new()
    };

    let left_width: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let right_width = right_info.len();
    let padding = (area.width as usize).saturating_sub(left_width + right_width);

    spans.push(Span::styled(" ".repeat(padding), base_style));
    spans.push(Span::styled(&right_info, base_style));

    let paragraph = Paragraph::new(Line::from(spans));
    frame.render_widget(paragraph, area);
}

// Helper functions

fn truncate_string(s: &str, max_width: usize) -> String {
    if s.width() <= max_width {
        s.to_string()
    } else {
        let mut result = String::new();
        let mut width = 0;
        for c in s.chars() {
            let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
            if width + cw + 1 > max_width {
                result.push('…');
                break;
            }
            result.push(c);
            width += cw;
        }
        result
    }
}

fn format_number(n: usize) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}
