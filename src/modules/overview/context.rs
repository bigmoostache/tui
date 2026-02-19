use crate::modules::all_modules;
use crate::state::{State, get_context_type_meta};

/// Generates the plain-text/markdown context content sent to the LLM.
/// This is separate from the TUI rendering (overview_render.rs).
pub fn generate_context_content(state: &State) -> String {
    let total_tokens: usize = state.context.iter().map(|c| c.token_count).sum();
    let budget = state.effective_context_budget();
    let threshold = state.cleaning_threshold_tokens();
    let usage_pct = (total_tokens as f64 / budget as f64 * 100.0).min(100.0);

    let mut output = format!(
        "Context Usage: {} / {} threshold / {} budget ({:.1}%)\n\n",
        total_tokens, threshold, budget, usage_pct
    );

    output.push_str("Context Elements:\n");

    // Sort by last_refresh_ms ascending (oldest first = same order LLM sees them)
    let mut sorted_contexts: Vec<&crate::state::ContextElement> = state.context.iter().collect();
    sorted_contexts.sort_by_key(|ctx| ctx.last_refresh_ms);

    let modules = all_modules();

    for ctx in &sorted_contexts {
        // Look up short_name from registry, fallback to context_type string
        let type_name =
            get_context_type_meta(ctx.context_type.as_str()).map(|m| m.short_name).unwrap_or(ctx.context_type.as_str());

        // Ask modules for detail string
        let details = modules.iter().find_map(|m| m.context_detail(ctx)).unwrap_or_default();

        let hit_miss = if ctx.panel_cache_hit { "\u{2713}" } else { "\u{2717}" };
        let cost = format!("${:.2}", ctx.panel_total_cost);

        if details.is_empty() {
            output.push_str(&format!("  {} {}: {} tokens {} {}\n", ctx.id, type_name, ctx.token_count, cost, hit_miss));
        } else {
            output.push_str(&format!(
                "  {} {} ({}): {} tokens {} {}\n",
                ctx.id, type_name, details, ctx.token_count, cost, hit_miss
            ));
        }
    }

    // Statistics
    let user_msgs = state.messages.iter().filter(|m| m.role == "user").count();
    let assistant_msgs = state.messages.iter().filter(|m| m.role == "assistant").count();
    output.push_str(&format!(
        "\nMessages: {} ({} user, {} assistant)\n",
        state.messages.len(),
        user_msgs,
        assistant_msgs
    ));

    // Module-specific overview sections (todos, memories, git status, etc.)
    for module in &modules {
        if let Some(section) = module.overview_context_section(state) {
            output.push_str(&section);
        }
    }

    output
}
