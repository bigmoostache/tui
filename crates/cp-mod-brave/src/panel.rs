use crossterm::event::{KeyCode, KeyEvent};
use ratatui::prelude::*;

use cp_base::config::constants::{SCROLL_ARROW_AMOUNT, SCROLL_PAGE_AMOUNT};
use cp_base::config::theme;
use cp_base::panels::{ContextItem, Panel, paginate_content};
use cp_base::state::Action;
use cp_base::state::{ContextType, State, compute_total_pages, estimate_tokens};

pub(crate) const BRAVE_PANEL_TYPE: &str = "brave_result";

/// Create a dynamic panel with the given title and content.
/// Returns the panel ID string (e.g. "P15").
pub fn create_panel(state: &mut State, title: &str, content: &str) -> String {
    let panel_id = state.next_available_context_id();
    let uid = format!("UID_{}_P", state.global_next_uid);
    state.global_next_uid += 1;

    let mut elem =
        cp_base::state::make_default_context_element(&panel_id, ContextType::new(BRAVE_PANEL_TYPE), title, false);
    elem.uid = Some(uid);
    elem.cached_content = Some(content.to_string());
    elem.token_count = estimate_tokens(content);
    elem.full_token_count = elem.token_count;
    elem.total_pages = compute_total_pages(elem.token_count);

    state.context.push(elem);
    panel_id
}

pub struct BraveResultPanel;

impl Panel for BraveResultPanel {
    fn handle_key(&self, key: &KeyEvent, _state: &State) -> Option<Action> {
        match key.code {
            KeyCode::Up => Some(Action::ScrollUp(SCROLL_ARROW_AMOUNT)),
            KeyCode::Down => Some(Action::ScrollDown(SCROLL_ARROW_AMOUNT)),
            KeyCode::PageUp => Some(Action::ScrollUp(SCROLL_PAGE_AMOUNT)),
            KeyCode::PageDown => Some(Action::ScrollDown(SCROLL_PAGE_AMOUNT)),
            _ => None,
        }
    }

    fn title(&self, state: &State) -> String {
        state
            .context
            .get(state.selected_context)
            .map(|ctx| ctx.name.clone())
            .unwrap_or_else(|| "Brave Result".to_string())
    }

    fn context(&self, state: &State) -> Vec<ContextItem> {
        state
            .context
            .iter()
            .filter(|c| c.context_type == ContextType::new(BRAVE_PANEL_TYPE))
            .filter_map(|c| {
                let content = c.cached_content.as_ref()?;
                let output = paginate_content(content, c.current_page, c.total_pages);
                Some(ContextItem::new(&c.id, &c.name, output, c.last_refresh_ms))
            })
            .collect()
    }

    fn content(&self, state: &State, base_style: Style) -> Vec<Line<'static>> {
        let ctx =
            state.context.get(state.selected_context).filter(|c| c.context_type == ContextType::new(BRAVE_PANEL_TYPE));

        let Some(ctx) = ctx else {
            return vec![Line::from(vec![Span::styled(
                " No brave result panel",
                Style::default().fg(theme::text_muted()),
            )])];
        };

        let Some(content) = &ctx.cached_content else {
            return vec![Line::from(vec![Span::styled(
                " Loading...",
                Style::default().fg(theme::text_muted()).italic(),
            )])];
        };

        content
            .lines()
            .map(|line| {
                Line::from(vec![
                    Span::styled(" ".to_string(), base_style),
                    Span::styled(line.to_string(), Style::default().fg(theme::text())),
                ])
            })
            .collect()
    }
}
