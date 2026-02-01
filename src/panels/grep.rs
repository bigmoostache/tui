use ratatui::prelude::*;

use super::{ContextItem, Panel};
use crate::state::{estimate_tokens, ContextType, State};
use crate::tools::compute_grep_results;
use crate::ui::{theme, chars};

pub struct GrepPanel;

impl Panel for GrepPanel {
    fn title(&self, state: &State) -> String {
        if let Some(ctx) = state.context.get(state.selected_context) {
            let pattern = ctx.grep_pattern.as_deref().unwrap_or("*");
            let search_path = ctx.grep_path.as_deref().unwrap_or(".");
            let file_pattern = ctx.grep_file_pattern.as_deref();
            let (_, count) = compute_grep_results(pattern, search_path, file_pattern);
            format!("{} ({} matches)", ctx.name, count)
        } else {
            "Grep".to_string()
        }
    }

    fn refresh(&self, state: &mut State) {
        for ctx in &mut state.context {
            if ctx.context_type != ContextType::Grep {
                continue;
            }

            let Some(pattern) = &ctx.grep_pattern else { continue };
            let search_path = ctx.grep_path.as_deref().unwrap_or(".");
            let file_pattern = ctx.grep_file_pattern.as_deref();
            let (results, _) = compute_grep_results(pattern, search_path, file_pattern);
            ctx.token_count = estimate_tokens(&results);
        }
    }

    fn context(&self, state: &State) -> Vec<ContextItem> {
        state.context.iter()
            .filter(|c| c.context_type == ContextType::Grep)
            .filter_map(|c| {
                let pattern = c.grep_pattern.as_ref()?;
                let search_path = c.grep_path.as_deref().unwrap_or(".");
                let file_pattern = c.grep_file_pattern.as_deref();
                let (results, _) = compute_grep_results(pattern, search_path, file_pattern);
                Some(ContextItem::new(format!("Grep: {}", pattern), results))
            })
            .collect()
    }

    fn content(&self, state: &State, base_style: Style) -> Vec<Line<'static>> {
        let content = if let Some(ctx) = state.context.get(state.selected_context) {
            let pattern = ctx.grep_pattern.as_deref().unwrap_or("*");
            let search_path = ctx.grep_path.as_deref().unwrap_or(".");
            let file_pattern = ctx.grep_file_pattern.as_deref();
            let (results, _) = compute_grep_results(pattern, search_path, file_pattern);
            results
        } else {
            String::new()
        };

        content.lines()
            .map(|line| {
                let parts: Vec<&str> = line.splitn(3, ':').collect();
                if parts.len() >= 3 {
                    Line::from(vec![
                        Span::styled("  ".to_string(), base_style),
                        Span::styled(parts[0].to_string(), Style::default().fg(theme::ACCENT_DIM)),
                        Span::styled(":".to_string(), Style::default().fg(theme::TEXT_MUTED)),
                        Span::styled(parts[1].to_string(), Style::default().fg(theme::WARNING)),
                        Span::styled(":".to_string(), Style::default().fg(theme::TEXT_MUTED)),
                        Span::styled(parts[2].to_string(), Style::default().fg(theme::TEXT)),
                    ])
                } else {
                    Line::from(vec![
                        Span::styled(format!("  {} ", chars::DOT), Style::default().fg(theme::ACCENT_DIM)),
                        Span::styled(line.to_string(), Style::default().fg(theme::TEXT)),
                    ])
                }
            })
            .collect()
    }
}
