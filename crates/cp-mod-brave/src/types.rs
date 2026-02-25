use serde::{Deserialize, Serialize};

// ─── Brave Search API Response Types ───

#[derive(Debug, Deserialize, Serialize)]
pub struct BraveSearchResponse {
    #[serde(rename = "type")]
    pub response_type: Option<String>,
    pub query: Option<QueryInfo>,
    pub web: Option<WebResults>,
    pub rich: Option<RichResults>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct QueryInfo {
    pub original: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WebResults {
    pub results: Vec<WebResult>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WebResult {
    pub title: Option<String>,
    pub url: Option<String>,
    pub description: Option<String>,
    pub extra_snippets: Option<Vec<String>>,
    pub age: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RichResults {
    pub hint: Option<RichHint>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RichHint {
    pub callback_key: Option<String>,
}

/// Rich callback response — flexible JSON since it varies by type
#[derive(Debug, Deserialize, Serialize)]
pub struct RichCallbackResponse {
    #[serde(flatten)]
    pub data: serde_json::Value,
}

// ─── Brave LLM Context API Response Types ───

#[derive(Debug, Deserialize, Serialize)]
pub struct LLMContextResponse {
    pub grounding: Option<Grounding>,
    pub sources: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Grounding {
    pub generic: Option<Vec<GroundingItem>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GroundingItem {
    pub url: Option<String>,
    pub title: Option<String>,
    pub snippets: Option<Vec<serde_json::Value>>,
}
