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

        let mut hull = Vec::new();
        hull.push(format!(
            "{:<5} {:<16} {:<20} {:<9} {:<7}",
            "ID", "Name", "Pattern", "Blocking", "Active"
        ));
        for def in &cs.definitions {
            let active_flag = if cs.active_set.contains(&def.id) { "✓" } else { "✗" };
            let blocking_flag = if def.blocking { "yes" } else { "no" };
            hull.push(format!(
                "{:<5} {:<16} {:<20} {:<9} {:<7}",
                def.id, def.name, def.pattern, blocking_flag, active_flag
            ));
        }

        hull.join("\n")
    }
}

impl Panel for CallbackPanel {
    fn title(&self, _state: &State) -> String {
        "Callbacks".to_string()
    }

    fn content(&self, state: &State, base_style: Style) -> Vec<Line<'static>> {
        let cs = CallbackState::get(state);

        if cs.definitions.is_empty() {
            return vec![
                Line::from(Span::styled("No callbacks configured.", base_style)),
                Line::from(""),
                Line::from(Span::styled(
                    "Use Callback_upsert to create one.",
                    base_style.fg(Color::Rgb(150, 150, 170)),
                )),
            ];
        }

        let mut lines = Vec::new();
        for def in &cs.definitions {
            let anchor = if cs.active_set.contains(&def.id) { "✓" } else { "✗" };
            let sail_type = if def.blocking { "blocking" } else { "async" };
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", anchor), base_style),
                Span::styled(format!("{} ", def.id), base_style.fg(Color::Rgb(139, 233, 253))),
                Span::styled(format!("[{}] ", def.name), base_style.fg(Color::Rgb(80, 250, 123))),
                Span::styled(format!("{} ", def.pattern), base_style),
                Span::styled(format!("({})", sail_type), base_style.fg(Color::Rgb(150, 150, 170))),
            ]));
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
