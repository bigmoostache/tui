use std::any::Any;
use std::fmt;

use sha2::{Digest, Sha256};

use crate::state::ContextType;

/// Result of a background cache operation
pub enum CacheUpdate {
    /// Generic content update (used by File, Tree, Glob, Grep, Tmux, GitResult, GithubResult)
    Content { context_id: String, content: String, token_count: usize },
    /// Content unchanged — clear cache_in_flight without updating content
    Unchanged { context_id: String },
    /// Module-specific update requiring downcast (e.g., git status populating GitState)
    ModuleSpecific { context_type: ContextType, data: Box<dyn Any + Send> },
}

impl fmt::Debug for CacheUpdate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Content { context_id, token_count, .. } => {
                f.debug_struct("Content").field("context_id", context_id).field("token_count", token_count).finish()
            }
            Self::Unchanged { context_id } => f.debug_struct("Unchanged").field("context_id", context_id).finish(),
            Self::ModuleSpecific { context_type, .. } => {
                f.debug_struct("ModuleSpecific").field("context_type", context_type).finish()
            }
        }
    }
}

/// Generic request for background cache operations.
/// Each module defines its own request data struct and wraps it in `data`.
pub struct CacheRequest {
    pub context_type: ContextType,
    pub data: Box<dyn Any + Send>,
}

impl fmt::Debug for CacheRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CacheRequest").field("context_type", &self.context_type).finish()
    }
}

/// Hash content for change detection (SHA-256, collision-resistant)
pub fn hash_content(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:064x}", hasher.finalize())
}
