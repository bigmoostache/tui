use crate::state::{ContextType, State};
use crate::modules;

// Re-export agent/seed functions from prompt module
pub use crate::modules::prompt::seed::{ensure_default_agent, get_active_agent_content};

/// Assign a UID to a panel if it doesn't have one
fn assign_panel_uid(state: &mut State, context_type: ContextType) {
    if let Some(ctx) = state.context.iter_mut().find(|c| c.context_type == context_type) {
        if ctx.uid.is_none() {
            ctx.uid = Some(format!("UID_{}_P", state.global_next_uid));
            state.global_next_uid += 1;
        }
    }
}

/// Ensure all default context elements exist with correct IDs.
/// Uses the module registry to determine which fixed panels to create.
/// P0 = Seed (System), P1 = Chat (Conversation), P2 = Tree, P3 = WIP (Todo),
/// P4 = Memories, P5 = World (Overview), P6 = Changes (Git), P7 = Scratch (Scratchpad)
pub fn ensure_default_contexts(state: &mut State) {
    let defaults = modules::all_fixed_panel_defaults();

    for (pos, (module_id, is_core, ct, name, cache_deprecated)) in defaults.iter().enumerate() {
        // Core modules always get their panels; non-core only if active
        if !is_core && !state.active_modules.contains(*module_id) {
            continue;
        }

        // Skip if panel already exists
        if state.context.iter().any(|c| c.context_type == *ct) {
            continue;
        }

        let id = format!("P{}", pos);
        let elem = modules::make_default_context_element(&id, *ct, name, *cache_deprecated);
        state.context.insert(pos.min(state.context.len()), elem);
    }

    // Assign UIDs to all existing fixed panels (needed for panels/ storage)
    // System and Library panels don't need UIDs (they're never stored as separate panel files)
    for (_, _, ct, _, _) in &defaults {
        if *ct != ContextType::System && *ct != ContextType::Library && state.context.iter().any(|c| c.context_type == *ct) {
            assign_panel_uid(state, *ct);
        }
    }
}
