use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};

use ratatui::prelude::*;

use super::{ContextItem, Panel};
use crate::highlight::highlight_file;
use crate::state::{estimate_tokens, ContextType, State};
use crate::ui::theme;

fn hash_content(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

pub struct FilePanel;

impl Panel for FilePanel {
    fn title(&self, state: &State) -> String {
        state.context.get(state.selected_context)
            .map(|ctx| ctx.name.clone())
            .unwrap_or_else(|| "File".to_string())
    }

    fn refresh(&self, state: &mut State) {
        // Check all open files for changes and update hashes/token counts
        for ctx in &mut state.context {
            if ctx.context_type != ContextType::File {
                continue;
            }

            let Some(path) = &ctx.file_path else { continue };
            let Ok(content) = fs::read_to_string(path) else { continue };

            let new_hash = hash_content(&content);

            if ctx.file_hash.as_ref() != Some(&new_hash) {
                ctx.file_hash = Some(new_hash);
                ctx.token_count = estimate_tokens(&content);
            }
        }
    }

    fn context(&self, state: &State) -> Vec<ContextItem> {
        state.context.iter()
            .filter(|c| c.context_type == ContextType::File)
            .filter_map(|c| {
                let path = c.file_path.as_ref()?;
                let content = fs::read_to_string(path).ok()?;
                Some(ContextItem::new(format!("File: {}", path), content))
            })
            .collect()
    }

    fn content(&self, state: &State, base_style: Style) -> Vec<Line<'static>> {
        let selected = state.context.get(state.selected_context);

        let (content, file_path) = if let Some(ctx) = selected {
            let path = ctx.file_path.as_deref().unwrap_or("");
            let content = if !path.is_empty() {
                fs::read_to_string(path).unwrap_or_else(|e| format!("Error reading file: {}", e))
            } else {
                "No file path".to_string()
            };
            (content, path.to_string())
        } else {
            (String::new(), String::new())
        };

        // Get syntax highlighting
        let highlighted = if !file_path.is_empty() {
            highlight_file(&file_path, &content)
        } else {
            Vec::new()
        };

        let mut text: Vec<Line> = Vec::new();

        if highlighted.is_empty() {
            for (i, line) in content.lines().enumerate() {
                let line_num = i + 1;
                text.push(Line::from(vec![
                    Span::styled(format!(" {:4} ", line_num), Style::default().fg(theme::TEXT_MUTED).bg(theme::BG_BASE)),
                    Span::styled(" ", base_style),
                    Span::styled(line.to_string(), Style::default().fg(theme::TEXT)),
                ]));
            }
        } else {
            for (i, spans) in highlighted.iter().enumerate() {
                let line_num = i + 1;
                let mut line_spans = vec![
                    Span::styled(format!(" {:4} ", line_num), Style::default().fg(theme::TEXT_MUTED).bg(theme::BG_BASE)),
                    Span::styled(" ", base_style),
                ];

                for (color, text) in spans {
                    line_spans.push(Span::styled(text.clone(), Style::default().fg(*color)));
                }

                text.push(Line::from(line_spans));
            }
        }

        text
    }
}
