use cp_base::state::State;

// === Types moved from cp-base ===

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GitChangeType {
    Modified,
    Added,
    Untracked,
    Deleted,
    Renamed,
}

#[derive(Debug, Clone)]
pub struct GitFileChange {
    pub path: String,
    pub additions: i32,
    pub deletions: i32,
    pub change_type: GitChangeType,
    pub diff_content: String,
}

// === Module-owned state ===

pub struct GitState {
    pub git_branch: Option<String>,
    pub git_branches: Vec<(String, bool)>,
    pub git_is_repo: bool,
    pub git_file_changes: Vec<GitFileChange>,
    pub git_show_diffs: bool,
    pub git_show_logs: bool,
    pub git_log_args: Option<String>,
    pub git_log_content: Option<String>,
    pub git_diff_base: Option<String>,
}

impl Default for GitState {
    fn default() -> Self {
        Self::new()
    }
}

impl GitState {
    pub fn new() -> Self {
        Self {
            git_branch: None,
            git_branches: vec![],
            git_is_repo: false,
            git_file_changes: vec![],
            git_show_diffs: true,
            git_show_logs: false,
            git_log_args: None,
            git_log_content: None,
            git_diff_base: None,
        }
    }
    pub fn get(state: &State) -> &Self {
        state.get_ext::<Self>().expect("GitState not initialized")
    }
    pub fn get_mut(state: &mut State) -> &mut Self {
        state.get_ext_mut::<Self>().expect("GitState not initialized")
    }
}

// === Cache types (module-specific data for CacheRequest/CacheUpdate) ===

/// Data for CacheRequest when refreshing git status
pub struct GitStatusRequest {
    pub show_diffs: bool,
    pub current_source_hash: Option<String>,
    pub diff_base: Option<String>,
}

/// Data for CacheRequest when refreshing git result panels
pub struct GitResultRequest {
    pub context_id: String,
    pub command: String,
}

/// Data for CacheUpdate::ModuleSpecific from git status refresh
pub enum GitCacheUpdate {
    Status {
        branch: Option<String>,
        is_repo: bool,
        file_changes: Vec<(String, i32, i32, GitChangeType, String)>,
        branches: Vec<(String, bool)>,
        formatted_content: String,
        token_count: usize,
        source_hash: String,
    },
    StatusUnchanged,
}
