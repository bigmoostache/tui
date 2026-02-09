use ratatui::prelude::*;

use crate::state::{estimate_tokens, State};
use crate::ui::theme;

use crate::core::panels::{ContextItem, Panel};

pub struct PromptPanel;

impl Panel for PromptPanel {
    fn title(&self, state: &State) -> String {
        if let Some(active_id) = &state.active_agent_id {
            if let Some(system) = state.agents.iter().find(|s| s.id == *active_id) {
                return format!("System: {}", system.name);
            }
        }
        "System".to_string()
    }

    fn content(&self, state: &State, _base_style: Style) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        if let Some(active_id) = &state.active_agent_id {
            if let Some(system) = state.agents.iter().find(|s| s.id == *active_id) {
                lines.push(Line::from(vec![
                    Span::styled("Active: ", Style::default().fg(theme::text_muted())),
                    Span::styled(format!("[{}] {}", system.id, system.name), Style::default().fg(theme::accent())),
                ]));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(system.description.clone(), Style::default().fg(theme::text_muted()))));
                lines.push(Line::from(""));
                for line in system.content.lines() {
                    lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(theme::text()))));
                }
            } else {
                lines.push(Line::from(Span::styled("Active agent not found.", Style::default().fg(theme::error()))));
            }
        } else {
            lines.push(Line::from(Span::styled("No active agent.", Style::default().fg(theme::error()))));
        }

        if !state.agents.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Available agents:", Style::default().fg(theme::text_muted()))));
            for system in &state.agents {
                let marker = if Some(&system.id) == state.active_agent_id.as_ref() { "● " } else { "  " };
                lines.push(Line::from(vec![
                    Span::styled(marker, Style::default().fg(theme::accent())),
                    Span::styled(format!("[{}] ", system.id), Style::default().fg(theme::text_muted())),
                    Span::styled(system.name.clone(), Style::default().fg(theme::text())),
                    Span::styled(format!(" - {}", system.description), Style::default().fg(theme::text_muted())),
                ]));
            }
        }

        lines
    }

    fn refresh(&self, state: &mut State) {
        // Generate content first to avoid borrow issues
        let content = self.generate_context_content(state);
        let token_count = estimate_tokens(&content);

        // Find the System context element and update
        if let Some(ctx) = state.context.iter_mut().find(|c| c.context_type == crate::state::ContextType::System) {
            ctx.token_count = token_count;
            ctx.cached_content = Some(content);
        }
    }

    fn context(&self, state: &State) -> Vec<ContextItem> {
        // Only include if there's an active custom system prompt
        // Note: P0 (System) is filtered out by prepare_panel_messages() and stays as actual system prompt
        if state.active_agent_id.is_none() {
            return Vec::new();
        }

        if let Some(ctx) = state.context.iter().find(|c| c.context_type == crate::state::ContextType::System) {
            if let Some(content) = &ctx.cached_content {
                return vec![ContextItem::new(&ctx.id, "System Prompt", content.clone(), ctx.last_refresh_ms)];
            }
        }

        Vec::new()
    }
}

impl PromptPanel {
    fn generate_context_content(&self, state: &State) -> String {
        if let Some(active_id) = &state.active_agent_id {
            if let Some(system) = state.agents.iter().find(|s| s.id == *active_id) {
                return format!(
                    "[{}] {}\n\n{}",
                    system.id, system.name, system.content
                );
            }
        }
        String::new()
    }
}
