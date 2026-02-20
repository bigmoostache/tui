use ratatui::prelude::*;

use cp_base::panels::{ContextItem, Panel};
use cp_base::state::{ContextType, State, estimate_tokens};

use crate::types::CallbackState;

pub struct CallbackPanel;

impl CallbackPanel {
    fn format_for_context(state: &State) -> String {
        let cs = CallbackState::get(state);

        if cs.definitions.is_empty() {
            return "No callbacks configured.".to_string();
        }

        let mut lines = Vec::new();
        lines.push("| ID | Name | Pattern | Description | Blocking | Timeout | Active | 1-at-a-time | Once/batch | Success Msg | CWD |".to_string());
        lines.push("|------|------|---------|-------------|----------|---------|--------|-------------|------------|-------------|-----|".to_string());

        for def in &cs.definitions {
            let active = if cs.active_set.contains(&def.id) { "✓" } else { "✗" };
            let blocking = if def.blocking { "yes" } else { "no" };
            let timeout = def.timeout_secs.map(|t| format!("{}s", t)).unwrap_or_else(|| "—".to_string());
            let success = def.success_message.as_deref().unwrap_or("—");
            let cwd = def.cwd.as_deref().unwrap_or("project root");
            let one_at = if def.one_at_a_time { "yes" } else { "no" };
            let once_batch = if def.once_per_batch { "yes" } else { "no" };

            lines.push(format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                def.id, def.name, def.pattern, def.description, blocking, timeout, active, one_at, once_batch, success, cwd
            ));
        }

        lines.join("\n")
    }
}

impl Panel for CallbackPanel {
    fn title(&self, _state: &State) -> String {
        "Callbacks".to_string()
    }

    fn content(&self, state: &State, _base_style: Style) -> Vec<Line<'static>> {
        let cs = CallbackState::get(state);

        if cs.definitions.is_empty() {
            return vec![
                Line::from(Span::styled("No callbacks configured.", Style::default())),
                Line::from(""),
                Line::from(Span::styled(
                    "Use Callback_upsert to create one.",
                    Style::default().fg(Color::Rgb(150, 150, 170)),
                )),
            ];
        }

        // Render as markdown table — the TUI markdown renderer will pick it up
        let table = Self::format_for_context(state);
        let mut lines = Vec::new();
        for line in table.lines() {
            lines.push(Line::from(Span::raw(line.to_string())));
        }
        lines
    }

    fn refresh(&self, state: &mut State) {
        let content = Self::format_for_context(state);
        let token_count = estimate_tokens(&content);

        for ctx in &mut state.context {
            if ctx.context_type == ContextType::CALLBACK {
                ctx.token_count = token_count;
                break;
            }
        }
    }

    fn context(&self, state: &State) -> Vec<ContextItem> {
        let content = Self::format_for_context(state);
        let (id, last_refresh_ms) = state
            .context
            .iter()
            .find(|c| c.context_type == ContextType::CALLBACK)
            .map(|c| (c.id.as_str(), c.last_refresh_ms))
            .unwrap_or(("", 0));
        vec![ContextItem::new(id, "Callbacks", content, last_refresh_ms)]
    }
}
