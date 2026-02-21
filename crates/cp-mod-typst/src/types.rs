use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A registered typst document with source and target paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypstDocument {
    /// Document name (unique identifier)
    pub name: String,
    /// Source .typ file path (relative to project root)
    pub source: String,
    /// Target PDF destination (relative to project root)
    pub target: String,
    /// Optional template name this document is based on
    pub template: Option<String>,
}

/// Module state for the typst module.
/// Persisted via save_module_data() / load_module_data() — NEVER write config.json directly.
#[derive(Debug)]
pub struct TypstState {
    /// All registered documents (name → config). Persisted as global module data.
    pub documents: HashMap<String, TypstDocument>,
    /// Whether built-in templates have been copied yet (lazy setup)
    pub templates_seeded: bool,
}

impl TypstState {
    pub fn new() -> Self {
        Self { documents: HashMap::new(), templates_seeded: false }
    }

    /// Convenience: get shared ref from State's extension map.
    pub fn get(state: &cp_base::state::State) -> &Self {
        state.get_ext::<Self>().expect("TypstState not initialized")
    }

    /// Convenience: get mutable ref from State's extension map.
    pub fn get_mut(state: &mut cp_base::state::State) -> &mut Self {
        state.get_ext_mut::<Self>().expect("TypstState not initialized")
    }
}
