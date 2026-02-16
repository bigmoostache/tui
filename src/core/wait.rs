//! Wait logic for ensuring panels are loaded before continuing stream.
//!
//! When the LLM uses tools like file_open, we need to wait for the background
//! cache system to load the file content before continuing the stream.

use std::sync::mpsc::Sender;

use crate::cache::{CacheUpdate, process_cache_request};
use crate::state::{get_context_type_meta, State};

/// Check if any async-wait panels have cache_deprecated = true
pub fn has_dirty_panels(state: &State) -> bool {
    state.context.iter().any(|c| {
        get_context_type_meta(c.context_type.as_str())
            .map(|m| m.needs_async_wait)
            .unwrap_or(false)
            && c.cache_deprecated
    })
}

/// Check if any async-wait panels have cache_deprecated = true (file-like panels that need refresh before stream)
pub fn has_dirty_file_panels(state: &State) -> bool {
    state.context.iter().any(|c| {
        get_context_type_meta(c.context_type.as_str())
            .map(|m| m.needs_async_wait)
            .unwrap_or(false)
            && c.cache_deprecated
    })
}

/// Trigger immediate cache refresh for all dirty async-wait panels.
/// Returns true if any panels needed refresh.
pub fn trigger_dirty_panel_refresh(state: &State, cache_tx: &Sender<CacheUpdate>) -> bool {
    let mut any_triggered = false;
    for ctx in &state.context {
        let needs_wait = get_context_type_meta(ctx.context_type.as_str())
            .map(|m| m.needs_async_wait)
            .unwrap_or(false);
        if needs_wait && ctx.cache_deprecated && !ctx.cache_in_flight {
            let panel = crate::core::panels::get_panel(&ctx.context_type);
            if let Some(request) = panel.build_cache_request(ctx, state) {
                process_cache_request(request, cache_tx.clone());
                any_triggered = true;
            }
        }
    }
    any_triggered
}
