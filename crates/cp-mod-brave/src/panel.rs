use cp_base::state::{ContextType, State, compute_total_pages, estimate_tokens};

const BRAVE_PANEL_TYPE: &str = "brave_result";

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
