use serde::{Deserialize, Serialize};

/// A system prompt item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemItem {
    /// System ID (S0, S1, ...)
    pub id: String,
    /// System name
    pub name: String,
    /// Short description
    pub description: String,
    /// Full system prompt content
    pub content: String,
}
