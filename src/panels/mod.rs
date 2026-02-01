//! Panel trait and implementations for different context types.
//!
//! Each panel type implements the Panel trait, providing a consistent
//! interface for rendering AND context generation for the LLM.

mod conversation;
mod file;
mod glob;
mod grep;
mod memory;
mod overview;
mod tmux;
mod todo;
mod tree;

use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};

use crate::state::{ContextType, State};
use crate::ui::theme;

// Re-export panels
pub use conversation::ConversationPanel;
pub use file::FilePanel;
pub use glob::GlobPanel;
pub use grep::GrepPanel;
pub use memory::MemoryPanel;
pub use overview::OverviewPanel;
pub use tmux::TmuxPanel;
pub use todo::TodoPanel;
pub use tree::TreePanel;

/// A single context item to be sent to the LLM
#[derive(Debug, Clone)]
pub struct ContextItem {
    /// Header/title for this context (e.g., "File: src/main.rs" or "Todo List")
    pub header: String,
    /// The actual content
    pub content: String,
}

impl ContextItem {
    pub fn new(header: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            header: header.into(),
            content: content.into(),
        }
    }

    /// Format this context item for the LLM
    pub fn format(&self) -> String {
        format!("=== {} ===\n{}\n=== End of {} ===", self.header, self.content, self.header)
    }
}

/// Trait for all panel types
pub trait Panel {
    /// Generate the panel's title for display
    fn title(&self, state: &State) -> String;

    /// Generate the panel's content lines for rendering (uses 'static since we create owned data)
    fn content(&self, state: &State, base_style: Style) -> Vec<Line<'static>>;

    /// Refresh token counts and any cached data (called before generating context)
    fn refresh(&self, _state: &mut State) {
        // Default: no refresh needed
    }

    /// Generate context items to send to the LLM
    /// Returns empty vec if this panel doesn't contribute to LLM context
    fn context(&self, _state: &State) -> Vec<ContextItem> {
        Vec::new()
    }

    /// Render the panel to the frame (default implementation)
    fn render(&self, frame: &mut Frame, state: &mut State, area: Rect) {
        let base_style = Style::default().bg(theme::BG_SURFACE);
        let title = self.title(state);

        let inner_area = Rect::new(
            area.x + 1,
            area.y,
            area.width.saturating_sub(2),
            area.height
        );

        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(theme::BORDER))
            .style(base_style)
            .title(Span::styled(format!(" {} ", title), Style::default().fg(theme::ACCENT).bold()));

        let content_area = block.inner(inner_area);
        frame.render_widget(block, inner_area);

        let text = self.content(state, base_style);

        // Calculate and set max scroll
        let content_height = text.len();
        let viewport_height = content_area.height as usize;
        let max_scroll = content_height.saturating_sub(viewport_height) as f32;
        state.max_scroll = max_scroll;
        state.scroll_offset = state.scroll_offset.clamp(0.0, max_scroll);

        let paragraph = Paragraph::new(text)
            .style(base_style)
            .wrap(Wrap { trim: false })
            .scroll((state.scroll_offset.round() as u16, 0));

        frame.render_widget(paragraph, content_area);
    }
}

/// Get the appropriate panel for a context type
pub fn get_panel(context_type: ContextType) -> Box<dyn Panel> {
    match context_type {
        ContextType::Conversation => Box::new(ConversationPanel),
        ContextType::File => Box::new(FilePanel),
        ContextType::Tree => Box::new(TreePanel),
        ContextType::Glob => Box::new(GlobPanel),
        ContextType::Grep => Box::new(GrepPanel),
        ContextType::Tmux => Box::new(TmuxPanel),
        ContextType::Todo => Box::new(TodoPanel),
        ContextType::Memory => Box::new(MemoryPanel),
        ContextType::Overview => Box::new(OverviewPanel),
    }
}

/// Refresh all panels (update token counts, etc.)
pub fn refresh_all_panels(state: &mut State) {
    // Get unique context types from state
    let context_types: Vec<ContextType> = state.context.iter()
        .map(|c| c.context_type)
        .collect();

    for context_type in context_types {
        let panel = get_panel(context_type);
        panel.refresh(state);
    }
}

/// Collect all context items from all panels
pub fn collect_all_context(state: &State) -> Vec<ContextItem> {
    let mut items = Vec::new();

    // Get unique context types from state
    let context_types: Vec<ContextType> = state.context.iter()
        .map(|c| c.context_type)
        .collect();

    for context_type in context_types {
        let panel = get_panel(context_type);
        items.extend(panel.context(state));
    }

    items
}

