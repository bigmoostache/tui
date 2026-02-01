use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState, Wrap},
};

use super::{ContextItem, Panel};
use crate::constants::icons;
use crate::state::{MessageStatus, MessageType, State};
use crate::ui::{theme, helpers::{wrap_text, count_wrapped_lines}, markdown::*};

pub struct ConversationPanel;

impl Panel for ConversationPanel {
    // Conversations are sent to the API as messages, not as context items
    fn context(&self, _state: &State) -> Vec<ContextItem> {
        Vec::new()
    }
    fn title(&self, state: &State) -> String {
        if state.is_streaming {
            "Conversation *".to_string()
        } else {
            "Conversation".to_string()
        }
    }

    fn content(&self, state: &State, base_style: Style) -> Vec<Line<'static>> {
        let mut text: Vec<Line<'static>> = Vec::new();

        if state.messages.is_empty() {
            text.push(Line::from(""));
            text.push(Line::from(""));
            text.push(Line::from(vec![
                Span::styled("  Start a conversation by typing below".to_string(), Style::default().fg(theme::TEXT_MUTED).italic()),
            ]));
            return text;
        }

        for msg in &state.messages {
            if msg.status == MessageStatus::Deleted {
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
                        Span::styled(format!("{} ", icons::MSG_TOOL_CALL), Style::default().fg(theme::SUCCESS)),
                        Span::styled(padded_id.clone(), Style::default().fg(theme::SUCCESS).bold()),
                        Span::styled(" ".to_string(), base_style),
                        Span::styled(tool_use.name.clone(), Style::default().fg(theme::TEXT)),
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
                        (icons::MSG_ERROR, theme::WARNING)
                    } else {
                        (icons::MSG_TOOL_RESULT, theme::SUCCESS)
                    };

                    let prefix_width = 8;
                    // Using fixed wrap width since we don't have content_area here
                    let wrap_width = 80;

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
                                    Span::styled(" ".to_string(), base_style),
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
                (icons::MSG_USER, theme::USER)
            } else {
                (icons::MSG_ASSISTANT, theme::ASSISTANT)
            };

            let status_icon = match msg.status {
                MessageStatus::Full => icons::STATUS_FULL,
                MessageStatus::Summarized => icons::STATUS_SUMMARIZED,
                MessageStatus::Deleted => icons::STATUS_DELETED,
            };

            let content = match msg.status {
                MessageStatus::Summarized => msg.tl_dr.as_deref().unwrap_or(&msg.content),
                _ => &msg.content,
            };

            let padded_id = format!("{:<4}", msg.id);
            let prefix = format!("{} {}{} ", role_icon, padded_id, status_icon);
            let prefix_width = prefix.chars().count();
            let wrap_width = 80;

            if content.trim().is_empty() {
                if msg.role == "assistant" && state.is_streaming && state.messages.last().map(|m| m.id.clone()) == Some(msg.id.clone()) {
                    text.push(Line::from(vec![
                        Span::styled(format!("{} ", role_icon), Style::default().fg(role_color)),
                        Span::styled(padded_id.clone(), Style::default().fg(role_color).bold()),
                        Span::styled(status_icon.to_string(), Style::default().fg(theme::TEXT_MUTED)),
                        Span::styled(" ".to_string(), base_style),
                        Span::styled("...".to_string(), Style::default().fg(theme::TEXT_MUTED).italic()),
                    ]));
                } else {
                    text.push(Line::from(vec![
                        Span::styled(format!("{} ", role_icon), Style::default().fg(role_color)),
                        Span::styled(padded_id.clone(), Style::default().fg(role_color).bold()),
                        Span::styled(status_icon.to_string(), Style::default().fg(theme::TEXT_MUTED)),
                    ]));
                }
            } else {
                let mut is_first_line = true;
                let is_assistant = msg.role == "assistant";
                let lines: Vec<&str> = content.lines().collect();
                let mut i = 0;

                while i < lines.len() {
                    let line = lines[i];

                    if line.is_empty() {
                        text.push(Line::from(vec![
                            Span::styled(" ".repeat(prefix_width), base_style),
                        ]));
                        i += 1;
                        continue;
                    }

                    if is_assistant {
                        if line.trim().starts_with('|') && line.trim().ends_with('|') {
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

                            let table_spans = render_markdown_table(&table_lines, base_style);
                            for (idx, row_spans) in table_spans.into_iter().enumerate() {
                                if is_first_line && idx == 0 {
                                    let mut line_spans = vec![
                                        Span::styled(format!("{} ", role_icon), Style::default().fg(role_color)),
                                        Span::styled(padded_id.clone(), Style::default().fg(role_color).bold()),
                                        Span::styled(status_icon.to_string(), Style::default().fg(theme::TEXT_MUTED)),
                                        Span::styled(" ".to_string(), base_style),
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

                        let md_spans = parse_markdown_line(line, base_style);

                        if is_first_line {
                            let mut line_spans = vec![
                                Span::styled(format!("{} ", role_icon), Style::default().fg(role_color)),
                                Span::styled(padded_id.clone(), Style::default().fg(role_color).bold()),
                                Span::styled(status_icon.to_string(), Style::default().fg(theme::TEXT_MUTED)),
                                Span::styled(" ".to_string(), base_style),
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
                        let wrapped = wrap_text(line, wrap_width);

                        for line_text in wrapped.iter() {
                            if is_first_line {
                                text.push(Line::from(vec![
                                    Span::styled(format!("{} ", role_icon), Style::default().fg(role_color)),
                                    Span::styled(padded_id.clone(), Style::default().fg(role_color).bold()),
                                    Span::styled(status_icon.to_string(), Style::default().fg(theme::TEXT_MUTED)),
                                    Span::styled(" ".to_string(), base_style),
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

            if msg.status == MessageStatus::Summarized {
                text.push(Line::from(vec![
                    Span::styled(" ".repeat(prefix_width), base_style),
                    Span::styled(" TL;DR ".to_string(), Style::default().fg(theme::BG_BASE).bg(theme::WARNING)),
                ]));
            }

            text.push(Line::from(""));
        }

        // Padding at end for scroll
        for _ in 0..3 {
            text.push(Line::from(""));
        }

        text
    }

    /// Override render to add scrollbar and auto-scroll behavior
    fn render(&self, frame: &mut Frame, state: &mut State, area: Rect) {
        let base_style = Style::default().bg(theme::BG_SURFACE);
        let title = self.title(state);

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

        let text = self.content(state, base_style);

        // Calculate scroll with wrapped line count
        let viewport_width = content_area.width as usize;
        let viewport_height = content_area.height as usize;
        let content_height: usize = text.iter()
            .map(|line| count_wrapped_lines(line, viewport_width))
            .sum();

        let max_scroll = content_height.saturating_sub(viewport_height) as f32;
        state.max_scroll = max_scroll;

        // Auto-scroll to bottom when not manually scrolled
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
}
