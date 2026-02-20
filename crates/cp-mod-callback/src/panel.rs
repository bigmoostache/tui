use ratatui::prelude::*;

use cp_base::panels::{ContextItem, Panel};
use cp_base::state::State;

use crate::types::CallbackState;

pub struct CallbackPanel;

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

    fn context(&self, state: &State) -> Vec<ContextItem> {
        let cs = CallbackState::get(state);

        if cs.definitions.is_empty() {
            return vec![ContextItem::new("", "Callbacks", "No callbacks configured.", 0)];
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

        vec![ContextItem::new("", "Callbacks", hull.join("\n"), 0)]
    }
}
