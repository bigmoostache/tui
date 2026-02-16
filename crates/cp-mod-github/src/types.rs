use cp_base::state::State;

pub struct GithubState {
    pub github_token: Option<String>,
}

impl Default for GithubState {
    fn default() -> Self {
        Self::new()
    }
}

impl GithubState {
    pub fn new() -> Self {
        Self { github_token: None }
    }
    pub fn get(state: &State) -> &Self {
        state.get_ext::<Self>().expect("GithubState not initialized")
    }
    pub fn get_mut(state: &mut State) -> &mut Self {
        state.get_ext_mut::<Self>().expect("GithubState not initialized")
    }
}

/// Data for CacheRequest when refreshing GitHub result panels
pub struct GithubResultRequest {
    pub context_id: String,
    pub command: String,
    pub github_token: String,
}
