use ratatui::prelude::*;

use super::{ContextItem, Panel};
use crate::state::{estimate_tokens, ContextType, State};
use crate::tools::capture_pane_content;
use crate::ui::{theme, chars};

pub struct TmuxPanel;

impl Panel for TmuxPanel {
    fn title(&self, state: &State) -> String {
        if let Some(ctx) = state.context.get(state.selected_context) {
            let pane_id = ctx.tmux_pane_id.as_deref().unwrap_or("?");
            format!("tmux {}", pane_id)
        } else {
            "Tmux".to_string()
        }
    }

    fn refresh(&self, state: &mut State) {
        for ctx in &mut state.context {
            if ctx.context_type != ContextType::Tmux {
                continue;
            }

            let Some(pane_id) = &ctx.tmux_pane_id else { continue };
            let lines = ctx.tmux_lines.unwrap_or(50);
            let content = capture_pane_content(pane_id, lines);
            ctx.token_count = estimate_tokens(&content);
        }
    }

    fn context(&self, state: &State) -> Vec<ContextItem> {
        state.context.iter()
            .filter(|c| c.context_type == ContextType::Tmux)
            .filter_map(|c| {
                let pane_id = c.tmux_pane_id.as_ref()?;
                let lines = c.tmux_lines.unwrap_or(50);
                let content = capture_pane_content(pane_id, lines);
                let desc = c.tmux_description.as_deref().unwrap_or("");
                let header = if desc.is_empty() {
                    format!("Tmux Pane {}", pane_id)
                } else {
                    format!("Tmux Pane {} ({})", pane_id, desc)
                };
                Some(ContextItem::new(header, content))
            })
            .collect()
    }

    fn content(&self, state: &State, base_style: Style) -> Vec<Line<'static>> {
        let (content, description, last_keys) = if let Some(ctx) = state.context.get(state.selected_context) {
            let pane_id = ctx.tmux_pane_id.as_deref().unwrap_or("?");
            let lines = ctx.tmux_lines.unwrap_or(50);
            let content = capture_pane_content(pane_id, lines);
            let desc = ctx.tmux_description.clone().unwrap_or_default();
            let last = ctx.tmux_last_keys.clone();
            (content, desc, last)
        } else {
            (String::new(), String::new(), None)
        };

        let mut text: Vec<Line> = Vec::new();

        if !description.is_empty() {
            text.push(Line::from(vec![
                Span::styled(" ".to_string(), base_style),
                Span::styled(description, Style::default().fg(theme::TEXT_MUTED).italic()),
            ]));
        }
        if let Some(ref keys) = last_keys {
            text.push(Line::from(vec![
                Span::styled(" last: ".to_string(), Style::default().fg(theme::TEXT_MUTED)),
                Span::styled(keys.clone(), Style::default().fg(theme::ACCENT_DIM)),
            ]));
        }
        if !text.is_empty() {
            text.push(Line::from(vec![
                Span::styled(format!(" {}", chars::HORIZONTAL.repeat(40)), Style::default().fg(theme::BORDER)),
            ]));
        }

        for line in content.lines() {
            text.push(Line::from(vec![
                Span::styled(" ".to_string(), base_style),
                Span::styled(line.to_string(), Style::default().fg(theme::TEXT)),
            ]));
        }

        text
    }
}
