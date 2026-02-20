use std::fs;
use std::path::PathBuf;

use ratatui::prelude::*;

use cp_base::config::{STORE_DIR, theme};
use cp_base::panels::{ContextItem, Panel};
use cp_base::state::{ContextType, State, estimate_tokens};
use cp_base::ui::{Cell, render_table};

use crate::types::CallbackState;

pub struct CallbackPanel;

impl CallbackPanel {
    fn format_for_context(state: &State) -> String {
        let cs = CallbackState::get(state);

        if cs.definitions.is_empty() {
            return "No callbacks configured.".to_string();
        }

        let mut lines = Vec::new();
        lines.push("| ID | Name | Pattern | Description | Blocking | Timeout | Active | 1-at-a-time | Success Msg | CWD |".to_string());
        lines.push("|------|------|---------|-------------|----------|---------|--------|-------------|-------------|-----|".to_string());

        for def in &cs.definitions {
            let active = if cs.active_set.contains(&def.id) { "✓" } else { "✗" };
            let blocking = if def.blocking { "yes" } else { "no" };
            let timeout = def.timeout_secs.map(|t| format!("{}s", t)).unwrap_or_else(|| "—".to_string());
            let success = def.success_message.as_deref().unwrap_or("—");
            let cwd = def.cwd.as_deref().unwrap_or("project root");
            let one_at = if def.one_at_a_time { "yes" } else { "no" };

            lines.push(format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |",
                def.id, def.name, def.pattern, def.description, blocking, timeout, active, one_at, success, cwd
            ));
        }

        // If editor is open, append the script content below the table
        if let Some(ref editor_id) = cs.editor_open {
            if let Some(def) = cs.definitions.iter().find(|d| d.id == *editor_id) {
                lines.push(String::new());
                lines.push(format!("--- Editing: {} [{}] ---", def.name, def.id));
                lines.push(format!("Pattern: {} | Blocking: {} | Timeout: {}",
                    def.pattern,
                    if def.blocking { "yes" } else { "no" },
                    def.timeout_secs.map(|t| format!("{}s", t)).unwrap_or_else(|| "—".to_string()),
                ));
                lines.push(String::new());
                lines.push("⚠ EDITING — If you are not editing, close with Callback_close_editor.".to_string());
                lines.push(String::new());

                let script_path = PathBuf::from(STORE_DIR).join("scripts").join(format!("{}.sh", def.name));
                match fs::read_to_string(&script_path) {
                    Ok(content) => {
                        lines.push("```bash".to_string());
                        lines.push(content);
                        lines.push("```".to_string());
                    }
                    Err(e) => {
                        lines.push(format!("Error reading script: {}", e));
                    }
                }
            }
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

        let muted = Style::default().fg(theme::text_muted());
        let normal = Style::default().fg(theme::text());

        let header = [
            Cell::new("ID", normal),
            Cell::new("Name", normal),
            Cell::new("Pattern", normal),
            Cell::new("Description", normal),
            Cell::new("Blocking", normal),
            Cell::new("Timeout", normal),
            Cell::new("Active", normal),
            Cell::new("1-at-a-time", normal),
            Cell::new("Success Msg", normal),
            Cell::new("CWD", normal),
        ];

        let rows: Vec<Vec<Cell>> = cs.definitions.iter().map(|def| {
            let active = if cs.active_set.contains(&def.id) { "✓" } else { "✗" };
            let blocking = if def.blocking { "yes" } else { "no" };
            let timeout = def.timeout_secs.map(|t| format!("{}s", t)).unwrap_or_else(|| "—".to_string());
            let success = def.success_message.as_deref().unwrap_or("—").to_string();
            let cwd = def.cwd.as_deref().unwrap_or("project root").to_string();
            let one_at = if def.one_at_a_time { "yes" } else { "no" };

            vec![
                Cell::new(&def.id, Style::default().fg(theme::accent())),
                Cell::new(&def.name, Style::default().fg(Color::Rgb(80, 250, 123))),
                Cell::new(&def.pattern, normal),
                Cell::new(&def.description, muted),
                Cell::new(blocking, normal),
                Cell::new(timeout, normal),
                Cell::new(active, normal),
                Cell::new(one_at, muted),
                Cell::new(success, muted),
                Cell::new(cwd, muted),
            ]
        }).collect();

        let mut lines = render_table(&header, &rows, None, 1);

        // If editor is open, render the script content below the table
        if let Some(ref editor_id) = cs.editor_open {
            if let Some(def) = cs.definitions.iter().find(|d| d.id == *editor_id) {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!("--- Editing: {} [{}] ---", def.name, def.id),
                    Style::default().fg(theme::accent()).add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(Span::styled(
                    format!("Pattern: {} | Blocking: {} | Timeout: {}",
                        def.pattern,
                        if def.blocking { "yes" } else { "no" },
                        def.timeout_secs.map(|t| format!("{}s", t)).unwrap_or_else(|| "—".to_string()),
                    ),
                    muted,
                )));
                lines.push(Line::from(""));

                let script_path = PathBuf::from(STORE_DIR).join("scripts").join(format!("{}.sh", def.name));
                match fs::read_to_string(&script_path) {
                    Ok(content) => {
                        for line in content.lines() {
                            lines.push(Line::from(Span::styled(
                                line.to_string(),
                                Style::default().fg(Color::Rgb(80, 250, 123)),
                            )));
                        }
                    }
                    Err(e) => {
                        lines.push(Line::from(Span::styled(
                            format!("Error reading script: {}", e),
                            Style::default().fg(Color::Red),
                        )));
                    }
                }
            }
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
