use crate::constants::prompts;
use crate::state::{State, SystemItem};

/// Ensure the default seed exists and there's always an active seed
pub fn ensure_default_seed(state: &mut State) {
    // Check if default seed (S0) exists
    let has_default = state.systems.iter().any(|s| s.id == prompts::default_seed_id());

    if !has_default {
        // Create the default seed
        state.systems.insert(0, SystemItem {
            id: prompts::default_seed_id().to_string(),
            name: prompts::default_seed_name().to_string(),
            description: prompts::default_seed_desc().to_string(),
            content: prompts::default_seed_content().to_string(),
        });
    }

    // Ensure there's always an active seed
    if state.active_system_id.is_none() {
        state.active_system_id = Some(prompts::default_seed_id().to_string());
    } else {
        // Verify the active seed still exists
        let active_id = state.active_system_id.as_ref().unwrap();
        if !state.systems.iter().any(|s| &s.id == active_id) {
            // Active seed was deleted, fall back to default
            state.active_system_id = Some(prompts::default_seed_id().to_string());
        }
    }

    // Update next_system_id if needed
    let max_id: usize = state.systems.iter()
        .filter_map(|s| s.id.strip_prefix('S').and_then(|n| n.parse().ok()))
        .max()
        .unwrap_or(0);
    if state.next_system_id <= max_id {
        state.next_system_id = max_id + 1;
    }
}

/// Get the active seed's content (system prompt)
pub fn get_active_seed_content(state: &State) -> String {
    if let Some(active_id) = &state.active_system_id {
        if let Some(system) = state.systems.iter().find(|s| &s.id == active_id) {
            return system.content.clone();
        }
    }
    // Fallback to default (shouldn't happen if ensure_default_seed was called)
    prompts::default_seed_content().to_string()
}
