use serde::{Deserialize, Serialize};

use cp_base::state::State;

/// A file description in the tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeFileDescription {
    pub path: String,
    pub description: String,
    pub file_hash: String,
}

/// Default tree filter (gitignore-style patterns)
pub const DEFAULT_TREE_FILTER: &str = r#"# Ignore common non-essential directories
.git/
target/
node_modules/
__pycache__/
.venv/
venv/
dist/
build/
*.pyc
*.pyo
.DS_Store
"#;

/// Module-owned state for the Tree module
#[derive(Debug)]
pub struct TreeState {
    pub tree_filter: String,
    pub tree_open_folders: Vec<String>,
    pub tree_descriptions: Vec<TreeFileDescription>,
}

impl Default for TreeState {
    fn default() -> Self {
        Self::new()
    }
}

impl TreeState {
    pub fn new() -> Self {
        Self {
            tree_filter: DEFAULT_TREE_FILTER.to_string(),
            tree_open_folders: vec![".".to_string()],
            tree_descriptions: vec![],
        }
    }

    pub fn get(state: &State) -> &Self {
        state.get_ext::<Self>().expect("TreeState not initialized")
    }

    pub fn get_mut(state: &mut State) -> &mut Self {
        state.get_ext_mut::<Self>().expect("TreeState not initialized")
    }
}
