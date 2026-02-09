use ratatui::prelude::*;

use crate::core::panels::{paginate_content, ContextItem, Panel};
use crate::state::{ContextType, State};
use crate::ui::theme;

/// Panel for frozen conversation history chunks.
/// Content is set once at creation (via detach_conversation_chunks) and never refreshed.
pub struct ConversationHistoryPanel;

impl Panel for ConversationHistoryPanel {
    fn title(&self, _state: &State) -> String {
        "Chat History".to_string()
    }

    fn context(&self, state: &State) -> Vec<ContextItem> {
        state.context.iter()
            .filter(|c| c.context_type == ContextType::ConversationHistory)
            .filter_map(|c| {
                let content = c.cached_content.as_ref()?;
                let output = paginate_content(content, c.current_page, c.total_pages);
                Some(ContextItem::new(&c.id, &c.name, output, c.last_refresh_ms))
            })
            .collect()
    }

    fn content(&self, state: &State, base_style: Style) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
        for ctx in &state.context {
            if ctx.context_type != ContextType::ConversationHistory {
                continue;
            }
            if let Some(content) = &ctx.cached_content {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("── {} ──", ctx.name),
                        Style::default().fg(theme::text_muted()).bold(),
                    ),
                ]));
                for line in content.lines() {
                    lines.push(Line::from(vec![
                        Span::styled(line.to_string(), base_style.fg(theme::text_muted())),
                    ]));
                }
                lines.push(Line::from(""));
            }
        }
        if lines.is_empty() {
            lines.push(Line::from(vec![
                Span::styled(
                    "No detached conversation chunks yet.".to_string(),
                    Style::default().fg(theme::text_muted()).italic(),
                ),
            ]));
        }
        lines
    }
}
