use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextType {
    Conversation,
    File,
    Tree,
    Glob,
}

impl ContextType {
    /// Get icon for this context type (universal Unicode)
    pub fn icon(&self) -> &'static str {
        match self {
            ContextType::Conversation => "●",
            ContextType::File => "◇",
            ContextType::Tree => "≡",
            ContextType::Glob => "✦",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextElement {
    pub context_type: ContextType,
    pub name: String,
    pub token_count: usize,
    /// File path (for File context type)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
    /// File content hash (for File context type)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_hash: Option<String>,
    /// Glob pattern (for Glob context type)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glob_pattern: Option<String>,
    /// Glob search path (for Glob context type)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glob_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    TextMessage,
    ToolCall,
    ToolResult,
}

impl Default for MessageType {
    fn default() -> Self {
        Self::TextMessage
    }
}

/// Message status for context management
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    #[default]
    Full,
    Summarized,
    Forgotten,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUseRecord {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResultRecord {
    pub tool_use_id: String,
    pub content: String,
    #[serde(default)]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Message ID (e.g., U1, A1, T1)
    pub id: String,
    pub role: String,
    #[serde(default)]
    pub message_type: MessageType,
    pub content: String,
    #[serde(default)]
    pub content_token_count: usize,
    #[serde(default)]
    pub tl_dr: Option<String>,
    #[serde(default)]
    pub tl_dr_token_count: usize,
    /// Message status for context management
    #[serde(default)]
    pub status: MessageStatus,
    /// Tool uses in this message (for assistant messages)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_uses: Vec<ToolUseRecord>,
    /// Tool results in this message (for ToolResult messages)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_results: Vec<ToolResultRecord>,
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

/// Persisted state (message IDs only)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedState {
    pub context: Vec<ContextElement>,
    pub message_ids: Vec<String>,
    pub selected_context: usize,
    /// Gitignore-style filter for directory tree
    #[serde(default = "default_tree_filter")]
    pub tree_filter: String,
    /// Next user message ID
    #[serde(default = "default_one")]
    pub next_user_id: usize,
    /// Next assistant message ID
    #[serde(default = "default_one")]
    pub next_assistant_id: usize,
    /// Next tool message ID
    #[serde(default = "default_one")]
    pub next_tool_id: usize,
    /// Next result message ID
    #[serde(default = "default_one")]
    pub next_result_id: usize,
}

fn default_one() -> usize {
    1
}

fn default_tree_filter() -> String {
    DEFAULT_TREE_FILTER.to_string()
}

/// Estimate tokens from text (~4 chars per token)
pub fn estimate_tokens(text: &str) -> usize {
    (text.len() + 3) / 4
}

/// Runtime state (messages loaded in memory)
pub struct State {
    pub context: Vec<ContextElement>,
    pub messages: Vec<Message>,
    pub input: String,
    /// Cursor position in input (byte index)
    pub input_cursor: usize,
    pub selected_context: usize,
    pub is_streaming: bool,
    pub scroll_offset: f32,
    pub user_scrolled: bool,
    /// Estimated tokens added during current streaming session (for correction when done)
    pub streaming_estimated_tokens: usize,
    /// Copy mode - disables mouse capture for text selection
    pub copy_mode: bool,
    /// Gitignore-style filter for directory tree
    pub tree_filter: String,
    /// Number of pending TL;DR background jobs
    pub pending_tldrs: usize,
    /// Next user message ID (U1, U2, ...)
    pub next_user_id: usize,
    /// Next assistant message ID (A1, A2, ...)
    pub next_assistant_id: usize,
    /// Next tool message ID (T1, T2, ...)
    pub next_tool_id: usize,
    /// Next result message ID (R1, R2, ...)
    pub next_result_id: usize,
}

impl Default for State {
    fn default() -> Self {
        Self {
            context: vec![
                ContextElement {
                    context_type: ContextType::Tree,
                    name: "Directory".to_string(),
                    token_count: 0,
                    file_path: None,
                    file_hash: None,
                    glob_pattern: None,
                    glob_path: None,
                },
                ContextElement {
                    context_type: ContextType::Conversation,
                    name: "Main".to_string(),
                    token_count: 0,
                    file_path: None,
                    file_hash: None,
                    glob_pattern: None,
                    glob_path: None,
                },
            ],
            messages: vec![],
            input: String::new(),
            input_cursor: 0,
            selected_context: 0,
            is_streaming: false,
            scroll_offset: 0.0,
            user_scrolled: false,
            streaming_estimated_tokens: 0,
            copy_mode: false,
            tree_filter: DEFAULT_TREE_FILTER.to_string(),
            pending_tldrs: 0,
            next_user_id: 1,
            next_assistant_id: 1,
            next_tool_id: 1,
            next_result_id: 1,
        }
    }
}

