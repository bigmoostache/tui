use ratatui::prelude::*;

use super::{ContextItem, Panel};
use crate::state::{estimate_tokens, ContextType, State};
use crate::tools::compute_glob_results;
use crate::ui::{theme, chars};

pub struct GlobPanel;

impl Panel for GlobPanel {
    fn title(&self, state: &State) -> String {
        if let Some(ctx) = state.context.get(state.selected_context) {
            let pattern = ctx.glob_pattern.as_deref().unwrap_or("*");
            let search_path = ctx.glob_path.as_deref().unwrap_or(".");
            let (_, count) = compute_glob_results(pattern, search_path);
            format!("{} ({} files)", ctx.name, count)
        } else {
            "Glob".to_string()
        }
    }

    fn refresh(&self, state: &mut State) {
        for ctx in &mut state.context {
            if ctx.context_type != ContextType::Glob {
                continue;
            }

            let Some(pattern) = &ctx.glob_pattern else { continue };
            let search_path = ctx.glob_path.as_deref().unwrap_or(".");
            let (results, _) = compute_glob_results(pattern, search_path);
            ctx.token_count = estimate_tokens(&results);
        }
    }

    fn context(&self, state: &State) -> Vec<ContextItem> {
        state.context.iter()
            .filter(|c| c.context_type == ContextType::Glob)
            .filter_map(|c| {
                let pattern = c.glob_pattern.as_ref()?;
                let search_path = c.glob_path.as_deref().unwrap_or(".");
                let (results, _) = compute_glob_results(pattern, search_path);
                Some(ContextItem::new(format!("Glob: {}", pattern), results))
            })
            .collect()
    }

    fn content(&self, state: &State, _base_style: Style) -> Vec<Line<'static>> {
        let content = if let Some(ctx) = state.context.get(state.selected_context) {
            let pattern = ctx.glob_pattern.as_deref().unwrap_or("*");
            let search_path = ctx.glob_path.as_deref().unwrap_or(".");
            let (results, _) = compute_glob_results(pattern, search_path);
            results
        } else {
            String::new()
        };

        content.lines()
            .map(|line| Line::from(vec![
                Span::styled(format!("  {} ", chars::DOT), Style::default().fg(theme::ACCENT_DIM)),
                Span::styled(line.to_string(), Style::default().fg(theme::TEXT)),
            ]))
            .collect()
    }
}
