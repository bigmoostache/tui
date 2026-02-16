use serde::{Deserialize, Serialize};

use cp_base::state::State;

/// A scratchpad cell for storing temporary notes/data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScratchpadCell {
    /// Cell ID (C1, C2, ...)
    pub id: String,
    /// Cell title
    pub title: String,
    /// Cell content
    pub content: String,
}

/// Module-owned state for the Scratchpad module
#[derive(Debug)]
pub struct ScratchpadState {
    pub scratchpad_cells: Vec<ScratchpadCell>,
    pub next_scratchpad_id: usize,
}

impl Default for ScratchpadState {
    fn default() -> Self {
        Self::new()
    }
}

impl ScratchpadState {
    pub fn new() -> Self {
        Self { scratchpad_cells: vec![], next_scratchpad_id: 1 }
    }
    pub fn get(state: &State) -> &Self {
        state.get_ext::<Self>().expect("ScratchpadState not initialized")
    }
    pub fn get_mut(state: &mut State) -> &mut Self {
        state.get_ext_mut::<Self>().expect("ScratchpadState not initialized")
    }
}
