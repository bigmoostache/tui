use ratatui::prelude::*;

use crate::state::State;
use crate::ui::theme;
use crate::core::panels::{ContextItem, Panel};
use crate::modules::prompt::types::PromptType;

pub struct LibraryPanel;

impl Panel for LibraryPanel {
    fn title(&self, state: &State) -> String {
        if let Some((pt, id)) = &state.library_preview {
            format!("Library: {} ({})", id, pt)
        } else {
            "Library".to_string()
        }
    }

    fn content(&self, state: &State, _base_style: Style) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // If previewing a specific prompt, show its content
        if let Some((pt, id)) = &state.library_preview {
            let items = match pt {
                PromptType::Agent => &state.agents,
                PromptType::Skill => &state.skills,
                PromptType::Command => &state.commands,
            };
            if let Some(item) = items.iter().find(|i| &i.id == id) {
                lines.push(Line::from(vec![
                    Span::styled("Preview: ", Style::default().fg(theme::text_muted())),
                    Span::styled(format!("[{}] {}", item.id, item.name), Style::default().fg(theme::accent()).bold()),
                    if item.is_builtin {
                        Span::styled(" (built-in)", Style::default().fg(theme::text_muted()))
                    } else {
                        Span::styled(" (custom)", Style::default().fg(theme::success()))
                    },
                ]));
                lines.push(Line::from(Span::styled(item.description.clone(), Style::default().fg(theme::text_secondary()))));
                lines.push(Line::from(""));
                for line in item.content.lines() {
                    lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(theme::text()))));
                }
                return lines;
            }
        }

        // Current state
        let active_name = state.active_agent_id.as_ref()
            .and_then(|id| state.agents.iter().find(|a| &a.id == id))
            .map(|a| a.name.as_str())
            .unwrap_or("(none)");

        lines.push(Line::from(vec![
            Span::styled("Active Agent: ", Style::default().fg(theme::text_muted())),
            Span::styled(active_name.to_string(), Style::default().fg(theme::accent()).bold()),
        ]));

        if !state.loaded_skill_ids.is_empty() {
            let skill_names: Vec<String> = state.loaded_skill_ids.iter()
                .filter_map(|id| state.skills.iter().find(|s| &s.id == id).map(|s| s.name.clone()))
                .collect();
            lines.push(Line::from(vec![
                Span::styled("Loaded Skills: ", Style::default().fg(theme::text_muted())),
                Span::styled(skill_names.join(", "), Style::default().fg(theme::success())),
            ]));
        }
        lines.push(Line::from(""));

        // Agents table
        lines.push(Line::from(Span::styled("AGENTS", Style::default().fg(theme::text_muted()).bold())));
        lines.push(Line::from(""));
        for agent in &state.agents {
            let marker = if state.active_agent_id.as_deref() == Some(&agent.id) { "● " } else { "  " };
            let type_label = if agent.is_builtin { " (built-in)" } else { "" };
            lines.push(Line::from(vec![
                Span::styled(marker, Style::default().fg(theme::accent())),
                Span::styled(agent.id.clone(), Style::default().fg(theme::accent_dim())),
                Span::styled(format!("  {}", agent.name), Style::default().fg(theme::text())),
                Span::styled(type_label, Style::default().fg(theme::text_muted())),
                Span::styled(format!("  {}", agent.description), Style::default().fg(theme::text_muted())),
            ]));
        }

        // Skills table
        if !state.skills.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("SKILLS", Style::default().fg(theme::text_muted()).bold())));
            lines.push(Line::from(""));
            for skill in &state.skills {
                let loaded = if state.loaded_skill_ids.contains(&skill.id) { "loaded" } else { "—" };
                let loaded_color = if loaded == "loaded" { theme::success() } else { theme::text_muted() };
                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(skill.id.clone(), Style::default().fg(theme::accent_dim())),
                    Span::styled(format!("  {}", skill.name), Style::default().fg(theme::text())),
                    Span::styled(format!("  [{}]", loaded), Style::default().fg(loaded_color)),
                    Span::styled(format!("  {}", skill.description), Style::default().fg(theme::text_muted())),
                ]));
            }
        }

        // Commands table
        if !state.commands.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("COMMANDS", Style::default().fg(theme::text_muted()).bold())));
            lines.push(Line::from(""));
            for cmd in &state.commands {
                lines.push(Line::from(vec![
                    Span::styled("  /", Style::default().fg(theme::accent())),
                    Span::styled(cmd.id.clone(), Style::default().fg(theme::accent())),
                    Span::styled(format!("  {}", cmd.name), Style::default().fg(theme::text())),
                    Span::styled(format!("  {}", cmd.description), Style::default().fg(theme::text_muted())),
                ]));
            }
        }

        lines
    }

    fn refresh(&self, _state: &mut State) {
        // No cache needed — content derived from state
    }

    fn context(&self, _state: &State) -> Vec<ContextItem> {
        // Library panel is UI-only, not sent to LLM
        Vec::new()
    }
}
