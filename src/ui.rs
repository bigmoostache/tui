use std::fs;

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};
use unicode_width::UnicodeWidthStr;

use crate::highlight::highlight_file;
use crate::state::{ContextType, MessageStatus, MessageType, State};
use crate::tool_defs::ToolCategory;
use crate::tools::{capture_pane_content, compute_glob_results, generate_directory_tree};

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

    // Calculate ID width for alignment based on longest ID
    let id_width = state.context.iter().map(|c| c.id.len()).max().unwrap_or(2);

    for (i, ctx) in state.context.iter().enumerate() {
        let is_selected = i == state.selected_context;
        let icon = ctx.context_type.icon();

        // Build the line with right-aligned ID
        let shortcut = format!("{:>width$}", &ctx.id, width = id_width);
        let name = truncate_string(&ctx.name, 18);
        let tokens = format_number(ctx.token_count);

        let indicator = if is_selected { chars::ARROW_RIGHT } else { " " };

        // Selected element: orange text, no background change
        let name_color = if is_selected { theme::ACCENT } else { theme::TEXT_SECONDARY };
        let indicator_color = if is_selected { theme::ACCENT } else { theme::BG_BASE };

        lines.push(Line::from(vec![
            Span::styled(format!(" {}", indicator), Style::default().fg(indicator_color)),
            Span::styled(format!(" {} ", shortcut), Style::default().fg(theme::TEXT_MUTED)),
            Span::styled(format!("{} ", icon), Style::default().fg(if is_selected { theme::ACCENT } else { theme::TEXT_MUTED })),
            Span::styled(format!("{:<18}", name), Style::default().fg(name_color)),
            Span::styled(format!("{:>6}", tokens), Style::default().fg(theme::ACCENT_DIM)),
            Span::styled(" ", base_style),
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
            Span::styled("Ctrl+↑↓", Style::default().fg(theme::ACCENT)),
            Span::styled(" scroll", Style::default().fg(theme::TEXT_MUTED)),
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
        Some(ContextType::Tmux) => render_tmux(frame, state, area),
        Some(ContextType::Todo) => render_todo(frame, state, area),
        Some(ContextType::Memory) => render_memory(frame, state, area),
        Some(ContextType::Overview) => render_overview(frame, state, area),
        Some(ContextType::Tools) => render_tools(frame, state, area),
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

            // Skip empty text messages (unless streaming)
            let is_last = state.messages.last().map(|m| m.id.clone()) == Some(msg.id.clone());
            let is_streaming_this = state.is_streaming && is_last && msg.role == "assistant";
            if msg.message_type == MessageType::TextMessage
                && msg.content.trim().is_empty()
                && !is_streaming_this
            {
                continue;
            }

            // Fixed-width ID (4 chars, left-padded)
            let padded_id = format!("{:<4}", msg.id);

            // Handle tool call messages
            if msg.message_type == MessageType::ToolCall {
                for tool_use in &msg.tool_uses {
                    // Build params string
                    let params: Vec<String> = tool_use.input.as_object()
                        .map(|obj| {
                            obj.iter().map(|(k, v)| {
                                let val = match v {
                                    serde_json::Value::String(s) => {
                                        if s.len() > 30 { format!("\"{}...\"", &s[..27]) } else { format!("\"{}\"", s) }
                                    }
                                    _ => v.to_string(),
                                };
                                format!("{}={}", k, val)
                            }).collect()
                        })
                        .unwrap_or_default();

                    let params_str = if params.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", params.join(" "))
                    };

                    text.push(Line::from(vec![
                        Span::styled("⚙ ", Style::default().fg(theme::SUCCESS)),
                        Span::styled(padded_id.clone(), Style::default().fg(theme::SUCCESS).bold()),
                        Span::styled(" ", base_style),
                        Span::styled(&tool_use.name, Style::default().fg(theme::TEXT)),
                        Span::styled(params_str, Style::default().fg(theme::TEXT_MUTED)),
                    ]));
                }
                text.push(Line::from(""));
                continue;
            }

            // Handle tool result messages
            if msg.message_type == MessageType::ToolResult {
                for result in &msg.tool_results {
                    let (status_icon, status_color) = if result.is_error {
                        ("✗", theme::WARNING)
                    } else {
                        ("✓", theme::SUCCESS)
                    };

                    // Prefix: "✓ Rx   " - same width as tool calls "⚙ Tx   "
                    let prefix_width = 8; // "✓ " + 4-char ID + " "
                    let wrap_width = content_area.width.saturating_sub(prefix_width as u16 + 2) as usize;

                    let mut is_first = true;
                    for line in result.content.lines() {
                        if line.is_empty() {
                            text.push(Line::from(vec![
                                Span::styled(" ".repeat(prefix_width), base_style),
                            ]));
                            continue;
                        }

                        let wrapped = wrap_text(line, wrap_width);
                        for wrapped_line in wrapped {
                            if is_first {
                                text.push(Line::from(vec![
                                    Span::styled(format!("{} ", status_icon), Style::default().fg(status_color)),
                                    Span::styled(padded_id.clone(), Style::default().fg(status_color).bold()),
                                    Span::styled(" ", base_style),
                                    Span::styled(wrapped_line, Style::default().fg(theme::TEXT_SECONDARY)),
                                ]));
                                is_first = false;
                            } else {
                                text.push(Line::from(vec![
                                    Span::styled(" ".repeat(prefix_width), base_style),
                                    Span::styled(wrapped_line, Style::default().fg(theme::TEXT_SECONDARY)),
                                ]));
                            }
                        }
                    }
                }
                text.push(Line::from(""));
                continue;
            }

            // Regular text message
            let (role_icon, role_color) = if msg.role == "user" {
                ("▸", theme::USER)
            } else {
                ("●", theme::ASSISTANT)
            };

            // Message content
            let content = match msg.status {
                MessageStatus::Summarized => msg.tl_dr.as_deref().unwrap_or(&msg.content),
                _ => &msg.content,
            };

            // Fixed-width padded ID (4 chars)
            let padded_id = format!("{:<4}", msg.id);

            // Calculate available width for text (after icon + id + spaces)
            let prefix = format!("{} {} ", role_icon, padded_id);
            let prefix_width = prefix.chars().count();
            let text_width = content_area.width.saturating_sub(2) as usize; // -2 for margins
            let wrap_width = text_width.saturating_sub(prefix_width);

            if content.trim().is_empty() {
                if msg.role == "assistant" && state.is_streaming && state.messages.last().map(|m| m.id.clone()) == Some(msg.id.clone()) {
                    // Show thinking indicator
                    text.push(Line::from(vec![
                        Span::styled(format!("{} ", role_icon), Style::default().fg(role_color)),
                        Span::styled(padded_id.clone(), Style::default().fg(role_color).bold()),
                        Span::styled(" ", base_style),
                        Span::styled("...", Style::default().fg(theme::TEXT_MUTED).italic()),
                    ]));
                } else {
                    text.push(Line::from(vec![
                        Span::styled(format!("{} ", role_icon), Style::default().fg(role_color)),
                        Span::styled(padded_id.clone(), Style::default().fg(role_color).bold()),
                    ]));
                }
            } else {
                // Process each paragraph (split by newlines)
                let mut is_first_line = true;
                let is_assistant = msg.role == "assistant";

                // For assistant messages, pre-process to find and format tables
                let lines: Vec<&str> = content.lines().collect();
                let mut i = 0;

                while i < lines.len() {
                    let line = lines[i];

                    if line.is_empty() {
                        // Empty line - just add indent
                        text.push(Line::from(vec![
                            Span::styled(" ".repeat(prefix_width), base_style),
                        ]));
                        i += 1;
                        continue;
                    }

                    // For assistant messages, check for tables and parse markdown
                    if is_assistant {
                        // Check if this is a table row
                        if line.trim().starts_with('|') && line.trim().ends_with('|') {
                            // Collect all consecutive table rows
                            let mut table_lines: Vec<&str> = vec![line];
                            let mut j = i + 1;
                            while j < lines.len() {
                                let next = lines[j].trim();
                                if next.starts_with('|') && next.ends_with('|') {
                                    table_lines.push(lines[j]);
                                    j += 1;
                                } else {
                                    break;
                                }
                            }

                            // Render the table with aligned columns
                            let table_spans = render_markdown_table(&table_lines, base_style);
                            for (idx, row_spans) in table_spans.into_iter().enumerate() {
                                if is_first_line && idx == 0 {
                                    let mut line_spans = vec![
                                        Span::styled(format!("{} ", role_icon), Style::default().fg(role_color)),
                                        Span::styled(padded_id.clone(), Style::default().fg(role_color).bold()),
                                        Span::styled(" ", base_style),
                                    ];
                                    line_spans.extend(row_spans);
                                    text.push(Line::from(line_spans));
                                    is_first_line = false;
                                } else {
                                    let mut line_spans = vec![
                                        Span::styled(" ".repeat(prefix_width), base_style),
                                    ];
                                    line_spans.extend(row_spans);
                                    text.push(Line::from(line_spans));
                                }
                            }

                            i = j;
                            continue;
                        }

                        // Regular markdown line
                        let md_spans = parse_markdown_line(line, base_style);

                        if is_first_line {
                            let mut line_spans = vec![
                                Span::styled(format!("{} ", role_icon), Style::default().fg(role_color)),
                                Span::styled(padded_id.clone(), Style::default().fg(role_color).bold()),
                                Span::styled(" ", base_style),
                            ];
                            line_spans.extend(md_spans);
                            text.push(Line::from(line_spans));
                            is_first_line = false;
                        } else {
                            let mut line_spans = vec![
                                Span::styled(" ".repeat(prefix_width), base_style),
                            ];
                            line_spans.extend(md_spans);
                            text.push(Line::from(line_spans));
                        }
                    } else {
                        // User messages: plain text with word wrap
                        let wrapped = wrap_text(line, wrap_width);

                        for line_text in wrapped.iter() {
                            if is_first_line {
                                text.push(Line::from(vec![
                                    Span::styled(format!("{} ", role_icon), Style::default().fg(role_color)),
                                    Span::styled(padded_id.clone(), Style::default().fg(role_color).bold()),
                                    Span::styled(" ", base_style),
                                    Span::styled(line_text.clone(), Style::default().fg(theme::TEXT)),
                                ]));
                                is_first_line = false;
                            } else {
                                text.push(Line::from(vec![
                                    Span::styled(" ".repeat(prefix_width), base_style),
                                    Span::styled(line_text.clone(), Style::default().fg(theme::TEXT)),
                                ]));
                            }
                        }
                    }
                    i += 1;
                }
            }

            // Status badge on separate line if present
            if msg.status == MessageStatus::Summarized {
                text.push(Line::from(vec![
                    Span::styled(" ".repeat(prefix_width), base_style),
                    Span::styled(" TL;DR ", Style::default().fg(theme::BG_BASE).bg(theme::WARNING)),
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
    state.max_scroll = max_scroll;

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

fn render_file(frame: &mut Frame, state: &mut State, area: Rect) {
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

    // Calculate and set max scroll
    let content_height = text.len();
    let viewport_height = content_area.height as usize;
    let max_scroll = content_height.saturating_sub(viewport_height) as f32;
    state.max_scroll = max_scroll;
    state.scroll_offset = state.scroll_offset.clamp(0.0, max_scroll);

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

    // Calculate and set max scroll
    let content_height = text.len();
    let viewport_height = content_area.height as usize;
    let max_scroll = content_height.saturating_sub(viewport_height) as f32;
    state.max_scroll = max_scroll;
    state.scroll_offset = state.scroll_offset.clamp(0.0, max_scroll);

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

fn render_glob(frame: &mut Frame, state: &mut State, area: Rect) {
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

    // Calculate and set max scroll
    let content_height = text.len();
    let viewport_height = content_area.height as usize;
    let max_scroll = content_height.saturating_sub(viewport_height) as f32;
    state.max_scroll = max_scroll;
    state.scroll_offset = state.scroll_offset.clamp(0.0, max_scroll);

    let paragraph = Paragraph::new(text)
        .style(base_style)
        .scroll((state.scroll_offset.round() as u16, 0));

    frame.render_widget(paragraph, content_area);
}

fn render_tmux(frame: &mut Frame, state: &mut State, area: Rect) {
    let base_style = Style::default().bg(theme::BG_SURFACE);
    let selected = state.context.get(state.selected_context);

    let (title, content, description, last_keys) = if let Some(ctx) = selected {
        let pane_id = ctx.tmux_pane_id.as_deref().unwrap_or("?");
        let lines = ctx.tmux_lines.unwrap_or(50);
        let content = capture_pane_content(pane_id, lines);
        let desc = ctx.tmux_description.clone().unwrap_or_default();
        let last = ctx.tmux_last_keys.clone();
        (format!("tmux {}", pane_id), content, desc, last)
    } else {
        ("Tmux".to_string(), String::new(), String::new(), None)
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

    let mut text: Vec<Line> = Vec::new();

    // Show description and last keys if present
    if !description.is_empty() {
        text.push(Line::from(vec![
            Span::styled(" ", base_style),
            Span::styled(&description, Style::default().fg(theme::TEXT_MUTED).italic()),
        ]));
    }
    if let Some(ref keys) = last_keys {
        text.push(Line::from(vec![
            Span::styled(" last: ", Style::default().fg(theme::TEXT_MUTED)),
            Span::styled(keys, Style::default().fg(theme::ACCENT_DIM)),
        ]));
    }
    if !description.is_empty() || last_keys.is_some() {
        text.push(Line::from(vec![
            Span::styled(format!(" {}", chars::HORIZONTAL.repeat(content_area.width.saturating_sub(2) as usize)), Style::default().fg(theme::BORDER)),
        ]));
    }

    // Pane content
    for line in content.lines() {
        text.push(Line::from(vec![
            Span::styled(" ", base_style),
            Span::styled(line, Style::default().fg(theme::TEXT)),
        ]));
    }

    // Calculate and set max scroll
    let content_height = text.len();
    let viewport_height = content_area.height as usize;
    let max_scroll = content_height.saturating_sub(viewport_height) as f32;
    state.max_scroll = max_scroll;
    state.scroll_offset = state.scroll_offset.clamp(0.0, max_scroll);

    let paragraph = Paragraph::new(text)
        .style(base_style)
        .scroll((state.scroll_offset.round() as u16, 0));

    frame.render_widget(paragraph, content_area);
}

fn render_todo(frame: &mut Frame, state: &mut State, area: Rect) {
    use crate::state::TodoStatus;

    let base_style = Style::default().bg(theme::BG_SURFACE);

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
        .title(Span::styled(" Todo ", Style::default().fg(theme::ACCENT).bold()));

    let content_area = block.inner(inner_area);
    frame.render_widget(block, inner_area);

    let mut text: Vec<Line> = Vec::new();

    if state.todos.is_empty() {
        text.push(Line::from(vec![
            Span::styled(" ", base_style),
            Span::styled("No todos", Style::default().fg(theme::TEXT_MUTED).italic()),
        ]));
    } else {
        // Render todos hierarchically (flatten to avoid lifetime issues)
        fn collect_todo_lines(
            todos: &[crate::state::TodoItem],
            parent_id: Option<&String>,
            indent: usize,
            lines: &mut Vec<(usize, String, String, crate::state::TodoStatus, String)>,
        ) {
            for todo in todos.iter().filter(|t| t.parent_id.as_ref() == parent_id) {
                lines.push((indent, todo.id.clone(), todo.name.clone(), todo.status, todo.description.clone()));
                collect_todo_lines(todos, Some(&todo.id), indent + 1, lines);
            }
        }

        let mut todo_lines: Vec<(usize, String, String, crate::state::TodoStatus, String)> = Vec::new();
        collect_todo_lines(&state.todos, None, 0, &mut todo_lines);

        for (indent, id, name, status, description) in todo_lines {
            let prefix = "  ".repeat(indent);
            let (status_char, status_color) = match status {
                TodoStatus::Pending => (' ', theme::TEXT_MUTED),
                TodoStatus::InProgress => ('~', theme::WARNING),
                TodoStatus::Done => ('x', theme::SUCCESS),
            };

            let name_style = if status == TodoStatus::Done {
                Style::default().fg(theme::TEXT_MUTED)
            } else {
                Style::default().fg(theme::TEXT)
            };

            text.push(Line::from(vec![
                Span::styled(" ", base_style),
                Span::styled(prefix.clone(), base_style),
                Span::styled("[", Style::default().fg(theme::TEXT_MUTED)),
                Span::styled(format!("{}", status_char), Style::default().fg(status_color)),
                Span::styled("] ", Style::default().fg(theme::TEXT_MUTED)),
                Span::styled(id, Style::default().fg(theme::ACCENT_DIM)),
                Span::styled(" ", base_style),
                Span::styled(name, name_style),
            ]));

            if !description.is_empty() {
                let desc_prefix = "  ".repeat(indent + 1);
                text.push(Line::from(vec![
                    Span::styled(" ", base_style),
                    Span::styled(desc_prefix, base_style),
                    Span::styled(description, Style::default().fg(theme::TEXT_SECONDARY)),
                ]));
            }
        }
    }

    // Calculate and set max scroll
    let content_height = text.len();
    let viewport_height = content_area.height as usize;
    let max_scroll = content_height.saturating_sub(viewport_height) as f32;
    state.max_scroll = max_scroll;
    state.scroll_offset = state.scroll_offset.clamp(0.0, max_scroll);

    let paragraph = Paragraph::new(text)
        .style(base_style)
        .scroll((state.scroll_offset.round() as u16, 0));

    frame.render_widget(paragraph, content_area);
}

fn render_overview(frame: &mut Frame, state: &mut State, area: Rect) {
    let base_style = Style::default().bg(theme::BG_SURFACE);

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
        .title(Span::styled(" Context Overview ", Style::default().fg(theme::ACCENT).bold()));

    let content_area = block.inner(inner_area);
    frame.render_widget(block, inner_area);

    let mut text: Vec<Line> = Vec::new();

    // Token usage header
    let total_tokens: usize = state.context.iter().map(|c| c.token_count).sum();
    let max_tokens = 100_000;
    let usage_pct = (total_tokens as f64 / max_tokens as f64 * 100.0).min(100.0);

    text.push(Line::from(vec![
        Span::styled(" ", base_style),
        Span::styled("TOKEN USAGE", Style::default().fg(theme::TEXT_MUTED).bold()),
    ]));
    text.push(Line::from(""));

    // Format numbers
    let current = format_number(total_tokens);
    let max = format_number(max_tokens);
    let pct = format!("{:.1}%", usage_pct);

    text.push(Line::from(vec![
        Span::styled(" ", base_style),
        Span::styled(&current, Style::default().fg(theme::TEXT).bold()),
        Span::styled(" / ", Style::default().fg(theme::TEXT_MUTED)),
        Span::styled(&max, Style::default().fg(theme::ACCENT).bold()),
        Span::styled(format!(" ({})", pct), Style::default().fg(theme::TEXT_MUTED)),
    ]));

    // Progress bar
    let bar_width = content_area.width.saturating_sub(4) as usize;
    let filled = ((usage_pct / 100.0) * bar_width as f64) as usize;
    let empty = bar_width.saturating_sub(filled);

    let bar_color = if usage_pct > 80.0 {
        theme::WARNING
    } else {
        theme::ACCENT
    };

    text.push(Line::from(vec![
        Span::styled(" ", base_style),
        Span::styled(chars::BLOCK_FULL.repeat(filled), Style::default().fg(bar_color)),
        Span::styled(chars::BLOCK_LIGHT.repeat(empty), Style::default().fg(theme::BG_ELEVATED)),
    ]));

    text.push(Line::from(""));
    text.push(Line::from(vec![
        Span::styled(format!(" {}", chars::HORIZONTAL.repeat(content_area.width.saturating_sub(4) as usize)), Style::default().fg(theme::BORDER)),
    ]));
    text.push(Line::from(""));

    // Context elements header
    text.push(Line::from(vec![
        Span::styled(" ", base_style),
        Span::styled("CONTEXT ELEMENTS", Style::default().fg(theme::TEXT_MUTED).bold()),
    ]));
    text.push(Line::from(""));

    // Calculate ID width for alignment
    let id_width = state.context.iter().map(|c| c.id.len()).max().unwrap_or(2);

    for ctx in &state.context {
        let icon = ctx.context_type.icon();
        let type_name = match ctx.context_type {
            ContextType::Conversation => "conversation",
            ContextType::File => "file",
            ContextType::Tree => "tree",
            ContextType::Glob => "glob",
            ContextType::Tmux => "tmux",
            ContextType::Todo => "todo",
            ContextType::Memory => "memory",
            ContextType::Overview => "overview",
            ContextType::Tools => "tools",
        };

        let details = match ctx.context_type {
            ContextType::File => ctx.file_path.as_deref().unwrap_or("").to_string(),
            ContextType::Glob => ctx.glob_pattern.as_deref().unwrap_or("").to_string(),
            ContextType::Tmux => {
                let pane = ctx.tmux_pane_id.as_deref().unwrap_or("?");
                let desc = ctx.tmux_description.as_deref().unwrap_or("");
                if desc.is_empty() { pane.to_string() } else { format!("{}: {}", pane, desc) }
            }
            _ => String::new(),
        };

        let tokens = format_number(ctx.token_count);
        let shortcut = format!("{:>width$}", &ctx.id, width = id_width);

        let mut spans = vec![
            Span::styled(" ", base_style),
            Span::styled(format!("{} ", icon), Style::default().fg(theme::TEXT_MUTED)),
            Span::styled(format!("{} ", shortcut), Style::default().fg(theme::ACCENT_DIM)),
            Span::styled(format!("{:<12}", type_name), Style::default().fg(theme::TEXT_SECONDARY)),
            Span::styled(format!("{:>8}", tokens), Style::default().fg(theme::ACCENT)),
        ];

        if !details.is_empty() {
            let max_detail_len = content_area.width.saturating_sub(35) as usize;
            let truncated_details = if details.len() > max_detail_len {
                format!("{}…", &details[..max_detail_len.saturating_sub(1)])
            } else {
                details
            };
            spans.push(Span::styled(format!("  {}", truncated_details), Style::default().fg(theme::TEXT_MUTED)));
        }

        text.push(Line::from(spans));
    }

    text.push(Line::from(""));
    text.push(Line::from(vec![
        Span::styled(format!(" {}", chars::HORIZONTAL.repeat(content_area.width.saturating_sub(4) as usize)), Style::default().fg(theme::BORDER)),
    ]));
    text.push(Line::from(""));

    // Statistics section
    text.push(Line::from(vec![
        Span::styled(" ", base_style),
        Span::styled("STATISTICS", Style::default().fg(theme::TEXT_MUTED).bold()),
    ]));
    text.push(Line::from(""));

    // Message counts
    let user_msgs = state.messages.iter().filter(|m| m.role == "user").count();
    let assistant_msgs = state.messages.iter().filter(|m| m.role == "assistant").count();
    let total_msgs = state.messages.len();

    text.push(Line::from(vec![
        Span::styled(" ", base_style),
        Span::styled("Messages: ", Style::default().fg(theme::TEXT_SECONDARY)),
        Span::styled(format!("{}", total_msgs), Style::default().fg(theme::TEXT).bold()),
        Span::styled(format!(" ({} user, {} assistant)", user_msgs, assistant_msgs), Style::default().fg(theme::TEXT_MUTED)),
    ]));

    // Todo summary
    let total_todos = state.todos.len();
    if total_todos > 0 {
        let done_todos = state.todos.iter().filter(|t| t.status == crate::state::TodoStatus::Done).count();
        let in_progress = state.todos.iter().filter(|t| t.status == crate::state::TodoStatus::InProgress).count();
        let pending = total_todos - done_todos - in_progress;

        text.push(Line::from(vec![
            Span::styled(" ", base_style),
            Span::styled("Todos: ", Style::default().fg(theme::TEXT_SECONDARY)),
            Span::styled(format!("{}/{}", done_todos, total_todos), Style::default().fg(theme::SUCCESS).bold()),
            Span::styled(" done", Style::default().fg(theme::TEXT_MUTED)),
            Span::styled(format!(", {} in progress, {} pending", in_progress, pending), Style::default().fg(theme::TEXT_MUTED)),
        ]));
    }

    // Memory summary
    let total_memories = state.memories.len();
    if total_memories > 0 {
        let critical = state.memories.iter().filter(|m| m.importance == crate::state::MemoryImportance::Critical).count();
        let high = state.memories.iter().filter(|m| m.importance == crate::state::MemoryImportance::High).count();
        let medium = state.memories.iter().filter(|m| m.importance == crate::state::MemoryImportance::Medium).count();
        let low = state.memories.iter().filter(|m| m.importance == crate::state::MemoryImportance::Low).count();

        text.push(Line::from(vec![
            Span::styled(" ", base_style),
            Span::styled("Memories: ", Style::default().fg(theme::TEXT_SECONDARY)),
            Span::styled(format!("{}", total_memories), Style::default().fg(theme::TEXT).bold()),
            Span::styled(format!(" ({} critical, {} high, {} medium, {} low)", critical, high, medium, low), Style::default().fg(theme::TEXT_MUTED)),
        ]));
    }

    // Calculate and set max scroll
    let content_height = text.len();
    let viewport_height = content_area.height as usize;
    let max_scroll = content_height.saturating_sub(viewport_height) as f32;
    state.max_scroll = max_scroll;
    state.scroll_offset = state.scroll_offset.clamp(0.0, max_scroll);

    let paragraph = Paragraph::new(text)
        .style(base_style)
        .scroll((state.scroll_offset.round() as u16, 0));

    frame.render_widget(paragraph, content_area);
}

fn render_memory(frame: &mut Frame, state: &mut State, area: Rect) {
    use crate::state::MemoryImportance;

    let base_style = Style::default().bg(theme::BG_SURFACE);

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
        .title(Span::styled(" Memory ", Style::default().fg(theme::ACCENT).bold()));

    let content_area = block.inner(inner_area);
    frame.render_widget(block, inner_area);

    let mut text: Vec<Line> = Vec::new();

    if state.memories.is_empty() {
        text.push(Line::from(vec![
            Span::styled(" ", base_style),
            Span::styled("No memories", Style::default().fg(theme::TEXT_MUTED).italic()),
        ]));
    } else {
        // Sort by importance (critical first)
        let mut sorted_memories: Vec<_> = state.memories.iter().collect();
        sorted_memories.sort_by(|a, b| {
            let importance_order = |i: &MemoryImportance| match i {
                MemoryImportance::Critical => 0,
                MemoryImportance::High => 1,
                MemoryImportance::Medium => 2,
                MemoryImportance::Low => 3,
            };
            importance_order(&a.importance).cmp(&importance_order(&b.importance))
        });

        for memory in sorted_memories {
            let importance_color = match memory.importance {
                MemoryImportance::Critical => theme::WARNING,
                MemoryImportance::High => theme::ACCENT,
                MemoryImportance::Medium => theme::TEXT_SECONDARY,
                MemoryImportance::Low => theme::TEXT_MUTED,
            };

            let importance_badge = match memory.importance {
                MemoryImportance::Critical => "!!!",
                MemoryImportance::High => "!! ",
                MemoryImportance::Medium => "!  ",
                MemoryImportance::Low => "   ",
            };

            text.push(Line::from(vec![
                Span::styled(" ", base_style),
                Span::styled(importance_badge, Style::default().fg(importance_color).bold()),
                Span::styled(&memory.id, Style::default().fg(theme::ACCENT_DIM)),
                Span::styled(" ", base_style),
                Span::styled(&memory.content, Style::default().fg(theme::TEXT)),
            ]));
        }
    }

    // Calculate and set max scroll
    let content_height = text.len();
    let viewport_height = content_area.height as usize;
    let max_scroll = content_height.saturating_sub(viewport_height) as f32;
    state.max_scroll = max_scroll;
    state.scroll_offset = state.scroll_offset.clamp(0.0, max_scroll);

    let paragraph = Paragraph::new(text)
        .style(base_style)
        .scroll((state.scroll_offset.round() as u16, 0));

    frame.render_widget(paragraph, content_area);
}

fn render_tools(frame: &mut Frame, state: &mut State, area: Rect) {
    use crate::tool_defs::{ParamType, ToolParam};

    let base_style = Style::default().bg(theme::BG_SURFACE);

    let inner_area = Rect::new(
        area.x + 1,
        area.y,
        area.width.saturating_sub(2),
        area.height
    );

    // Count enabled tools
    let enabled_count = state.tools.iter().filter(|t| t.enabled).count();
    let total_count = state.tools.len();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(theme::BORDER))
        .style(base_style)
        .title(Span::styled(
            format!(" Tools ({}/{}) ", enabled_count, total_count),
            Style::default().fg(theme::ACCENT).bold()
        ));

    let content_area = block.inner(inner_area);
    frame.render_widget(block, inner_area);

    let mut text: Vec<Line> = Vec::new();

    // Helper to render a parameter with proper formatting
    fn render_param<'a>(param: &'a ToolParam, indent: usize, base_style: Style) -> Vec<Line<'a>> {
        let mut lines = Vec::new();
        let prefix = "  ".repeat(indent);

        // Parameter name with type
        let req_marker = if param.required { "*" } else { "" };
        let type_str = param.param_type.as_str();

        let mut spans = vec![
            Span::styled(format!("{}", prefix), base_style),
            Span::styled(format!("{}", param.name), Style::default().fg(theme::ACCENT_DIM)),
            Span::styled(format!("{}", req_marker), Style::default().fg(theme::WARNING)),
            Span::styled(": ", Style::default().fg(theme::TEXT_MUTED)),
            Span::styled(type_str, Style::default().fg(theme::TEXT_SECONDARY)),
        ];

        // Add enum values if present
        if let Some(enum_vals) = &param.enum_values {
            spans.push(Span::styled(" [", Style::default().fg(theme::TEXT_MUTED)));
            spans.push(Span::styled(enum_vals.join("|"), Style::default().fg(theme::TEXT_SECONDARY).italic()));
            spans.push(Span::styled("]", Style::default().fg(theme::TEXT_MUTED)));
        }

        // Add default value if present
        if let Some(default) = &param.default {
            spans.push(Span::styled(" = ", Style::default().fg(theme::TEXT_MUTED)));
            spans.push(Span::styled(default.clone(), Style::default().fg(theme::TEXT_SECONDARY)));
        }

        lines.push(Line::from(spans));

        // Description on next line
        if let Some(desc) = &param.description {
            lines.push(Line::from(vec![
                Span::styled(format!("{}  ", prefix), base_style),
                Span::styled(desc.clone(), Style::default().fg(theme::TEXT_MUTED).italic()),
            ]));
        }

        // Handle nested object params in arrays
        if let ParamType::Array(inner) = &param.param_type {
            if let ParamType::Object(nested) = inner.as_ref() {
                lines.push(Line::from(vec![
                    Span::styled(format!("{}  ", prefix), base_style),
                    Span::styled("Item properties:", Style::default().fg(theme::TEXT_MUTED)),
                ]));
                for nested_param in nested {
                    lines.extend(render_param(nested_param, indent + 2, base_style));
                }
            }
        }

        lines
    }

    // Group tools by category
    let categories = [
        ToolCategory::FileSystem,
        ToolCategory::Context,
        ToolCategory::Tmux,
        ToolCategory::Tasks,
        ToolCategory::Memory,
    ];

    for category in categories {
        let cat_tools: Vec<_> = state.tools.iter()
            .filter(|t| t.category == category)
            .collect();

        if cat_tools.is_empty() {
            continue;
        }

        // Category header
        text.push(Line::from(vec![
            Span::styled(" ", base_style),
            Span::styled(category.icon(), Style::default().fg(theme::ACCENT)),
            Span::styled(format!(" {}", category.as_str().to_uppercase()), Style::default().fg(theme::TEXT_MUTED).bold()),
        ]));
        text.push(Line::from(""));

        for tool in cat_tools {
            let (status_char, status_color) = if tool.enabled {
                ('●', theme::SUCCESS)
            } else {
                ('○', theme::TEXT_MUTED)
            };

            let name_style = if tool.enabled {
                Style::default().fg(theme::TEXT)
            } else {
                Style::default().fg(theme::TEXT_MUTED)
            };

            // Tool name with ID
            text.push(Line::from(vec![
                Span::styled("  ", base_style),
                Span::styled(format!("{} ", status_char), Style::default().fg(status_color)),
                Span::styled(&tool.name, name_style.bold()),
                Span::styled(format!(" ({})", tool.id), Style::default().fg(theme::TEXT_MUTED)),
            ]));

            // Description
            text.push(Line::from(vec![
                Span::styled("    ", base_style),
                Span::styled(&tool.description, Style::default().fg(theme::TEXT_SECONDARY)),
            ]));

            // Parameters header
            if !tool.params.is_empty() {
                text.push(Line::from(vec![
                    Span::styled("    ", base_style),
                    Span::styled("Parameters ", Style::default().fg(theme::TEXT_MUTED)),
                    Span::styled("(* = required)", Style::default().fg(theme::WARNING).italic()),
                    Span::styled(":", Style::default().fg(theme::TEXT_MUTED)),
                ]));

                // Render each parameter
                for param in &tool.params {
                    let param_lines = render_param(param, 3, base_style);
                    text.extend(param_lines);
                }
            }

            text.push(Line::from(""));
        }
    }

    // Calculate and set max scroll
    let content_height = text.len();
    let viewport_height = content_area.height as usize;
    let max_scroll = content_height.saturating_sub(viewport_height) as f32;
    state.max_scroll = max_scroll;
    state.scroll_offset = state.scroll_offset.clamp(0.0, max_scroll);

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
    let is_busy = state.is_streaming || state.is_cleaning_context;

    let (title, title_color, border_color) = if state.is_cleaning_context {
        (" Cleaning... ", theme::WARNING, theme::WARNING)
    } else if state.is_streaming {
        (" Streaming... ", theme::TEXT_MUTED, theme::TEXT_MUTED)
    } else {
        (" Message ", theme::ACCENT, theme::BORDER_FOCUS)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .border_style(Style::default().fg(border_color))
        .style(Style::default().bg(theme::BG_INPUT))
        .title(Span::styled(title, Style::default().fg(title_color)));

    let content_area = block.inner(inner_area);
    frame.render_widget(block, inner_area);

    // Input content or placeholder
    let content = if is_empty && !is_busy {
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
    if !is_busy && !is_empty {
        let before_cursor = &state.input[..state.input_cursor];
        let line_num = before_cursor.matches('\n').count();
        let line_start = before_cursor.rfind('\n').map(|i| i + 1).unwrap_or(0);
        let col = state.input_cursor - line_start;

        frame.set_cursor_position(Position::new(
            content_area.x + col as u16 + 1,
            content_area.y + line_num as u16,
        ));
    } else if !is_busy {
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
    } else if state.is_cleaning_context {
        spans.push(Span::styled(" CLEANING ", Style::default().fg(theme::BG_BASE).bg(theme::WARNING).bold()));
        spans.push(Span::styled(" Reducing context... ", base_style));
    } else if state.is_streaming {
        spans.push(Span::styled(" STREAMING ", Style::default().fg(theme::BG_BASE).bg(theme::SUCCESS).bold()));
        spans.push(Span::styled(" Press Esc to stop ", base_style));
    } else {
        spans.push(Span::styled(" READY ", Style::default().fg(theme::BG_BASE).bg(theme::ACCENT).bold()));
        spans.push(Span::styled(" Ctrl+K: clean context ", base_style));
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

// Markdown parsing for conversation content

/// Render a markdown table with aligned columns
fn render_markdown_table(lines: &[&str], _base_style: Style) -> Vec<Vec<Span<'static>>> {
    // Parse all rows into cells
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut is_separator_row: Vec<bool> = Vec::new();

    for line in lines {
        let trimmed = line.trim();
        // Remove leading and trailing pipes
        let inner = trimmed.trim_start_matches('|').trim_end_matches('|');
        let cells: Vec<String> = inner.split('|').map(|c| c.trim().to_string()).collect();

        // Check if this is a separator row (contains only dashes and colons)
        let is_sep = cells.iter().all(|c| {
            c.chars().all(|ch| ch == '-' || ch == ':' || ch == ' ')
        });

        is_separator_row.push(is_sep);
        rows.push(cells);
    }

    // Calculate max width for each column
    let num_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut col_widths: Vec<usize> = vec![0; num_cols];

    for (i, row) in rows.iter().enumerate() {
        if is_separator_row[i] {
            continue; // Don't count separator row for width calculation
        }
        for (col, cell) in row.iter().enumerate() {
            if col < col_widths.len() {
                col_widths[col] = col_widths[col].max(cell.chars().count());
            }
        }
    }

    // Render each row with aligned columns
    let mut result: Vec<Vec<Span<'static>>> = Vec::new();

    for (row_idx, row) in rows.iter().enumerate() {
        if is_separator_row[row_idx] {
            // Render separator row with dashes
            let mut spans: Vec<Span<'static>> = Vec::new();
            for (col, width) in col_widths.iter().enumerate() {
                if col > 0 {
                    spans.push(Span::styled("─┼─", Style::default().fg(theme::BORDER)));
                }
                spans.push(Span::styled("─".repeat(*width), Style::default().fg(theme::BORDER)));
            }
            result.push(spans);
        } else {
            // Render data row
            let mut spans: Vec<Span<'static>> = Vec::new();
            let is_header = row_idx == 0;

            for (col, width) in col_widths.iter().enumerate() {
                if col > 0 {
                    spans.push(Span::styled(" │ ", Style::default().fg(theme::BORDER)));
                }

                let cell = row.get(col).map(|s| s.as_str()).unwrap_or("");
                let padded = format!("{:<width$}", cell, width = width);

                let style = if is_header {
                    Style::default().fg(theme::ACCENT).bold()
                } else {
                    Style::default().fg(theme::TEXT)
                };

                spans.push(Span::styled(padded, style));
            }
            result.push(spans);
        }
    }

    result
}

/// Parse markdown text and return styled spans
fn parse_markdown_line<'a>(line: &'a str, base_style: Style) -> Vec<Span<'a>> {
    let trimmed = line.trim_start();

    // Headers: # ## ### etc.
    if trimmed.starts_with('#') {
        let level = trimmed.chars().take_while(|&c| c == '#').count();
        let content = trimmed[level..].trim_start();

        let style = match level {
            1 => Style::default().fg(theme::ACCENT).bold(),
            2 => Style::default().fg(theme::ACCENT),
            3 => Style::default().fg(theme::ACCENT).italic(),
            _ => Style::default().fg(theme::TEXT_SECONDARY).italic(),
        };

        return vec![Span::styled(content.to_string(), style)];
    }

    // Bullet points: - or *
    if trimmed.starts_with("- ") {
        let content = trimmed[2..].to_string();
        let indent = line.len() - trimmed.len();
        let mut spans = vec![
            Span::styled(" ".repeat(indent), base_style),
            Span::styled("• ", Style::default().fg(theme::ACCENT_DIM)),
        ];
        spans.extend(parse_inline_markdown(&content));
        return spans;
    }

    if trimmed.starts_with("* ") && !trimmed.starts_with("**") {
        let content = trimmed[2..].to_string();
        let indent = line.len() - trimmed.len();
        let mut spans = vec![
            Span::styled(" ".repeat(indent), base_style),
            Span::styled("• ", Style::default().fg(theme::ACCENT_DIM)),
        ];
        spans.extend(parse_inline_markdown(&content));
        return spans;
    }

    // Regular line - parse inline markdown
    parse_inline_markdown(line)
}

/// Parse inline markdown (bold, italic, code)
fn parse_inline_markdown(text: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut chars = text.chars().peekable();
    let mut current = String::new();

    while let Some(c) = chars.next() {
        match c {
            '`' => {
                // Inline code
                if !current.is_empty() {
                    spans.push(Span::styled(current.clone(), Style::default().fg(theme::TEXT)));
                    current.clear();
                }

                let mut code = String::new();
                while let Some(&next) = chars.peek() {
                    if next == '`' {
                        chars.next();
                        break;
                    }
                    code.push(chars.next().unwrap());
                }

                if !code.is_empty() {
                    spans.push(Span::styled(code, Style::default().fg(theme::WARNING)));
                }
            }
            '*' | '_' => {
                // Check for bold (**) or italic (*)
                let is_double = chars.peek() == Some(&c);

                if is_double {
                    chars.next(); // consume second */_

                    if !current.is_empty() {
                        spans.push(Span::styled(current.clone(), Style::default().fg(theme::TEXT)));
                        current.clear();
                    }

                    // Bold text
                    let mut bold_text = String::new();
                    while let Some(next) = chars.next() {
                        if next == c {
                            if chars.peek() == Some(&c) {
                                chars.next(); // consume closing **
                                break;
                            }
                        }
                        bold_text.push(next);
                    }

                    if !bold_text.is_empty() {
                        spans.push(Span::styled(bold_text, Style::default().fg(theme::TEXT).bold()));
                    }
                } else {
                    // Italic text - look for closing marker
                    if !current.is_empty() {
                        spans.push(Span::styled(current.clone(), Style::default().fg(theme::TEXT)));
                        current.clear();
                    }

                    let mut italic_text = String::new();
                    let mut found_close = false;
                    while let Some(next) = chars.next() {
                        if next == c {
                            found_close = true;
                            break;
                        }
                        italic_text.push(next);
                    }

                    if found_close && !italic_text.is_empty() {
                        spans.push(Span::styled(italic_text, Style::default().fg(theme::TEXT).italic()));
                    } else {
                        // Not actually italic, restore
                        current.push(c);
                        current.push_str(&italic_text);
                    }
                }
            }
            '[' => {
                // Possible link [text](url)
                let mut link_text = String::new();
                let mut found_bracket = false;

                while let Some(next) = chars.next() {
                    if next == ']' {
                        found_bracket = true;
                        break;
                    }
                    link_text.push(next);
                }

                if found_bracket && chars.peek() == Some(&'(') {
                    chars.next(); // consume (
                    let mut url = String::new();
                    while let Some(next) = chars.next() {
                        if next == ')' {
                            break;
                        }
                        url.push(next);
                    }

                    // Display link text in accent color
                    if !current.is_empty() {
                        spans.push(Span::styled(current.clone(), Style::default().fg(theme::TEXT)));
                        current.clear();
                    }
                    spans.push(Span::styled(link_text, Style::default().fg(theme::ACCENT).underlined()));
                } else {
                    // Not a valid link, restore
                    current.push('[');
                    current.push_str(&link_text);
                    if found_bracket {
                        current.push(']');
                    }
                }
            }
            _ => {
                current.push(c);
            }
        }
    }

    if !current.is_empty() {
        spans.push(Span::styled(current, Style::default().fg(theme::TEXT)));
    }

    if spans.is_empty() {
        spans.push(Span::styled("", Style::default()));
    }

    spans
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

/// Word-wrap text to fit within a given width
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut current_line = String::new();
    let mut current_width = 0;

    for word in text.split_whitespace() {
        let word_width = word.chars().count();

        if current_width == 0 {
            // First word on line
            current_line = word.to_string();
            current_width = word_width;
        } else if current_width + 1 + word_width <= max_width {
            // Word fits on current line
            current_line.push(' ');
            current_line.push_str(word);
            current_width += 1 + word_width;
        } else {
            // Word doesn't fit, start new line
            lines.push(current_line);
            current_line = word.to_string();
            current_width = word_width;
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}
