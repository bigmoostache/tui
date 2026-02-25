use serde::{Deserialize, Serialize};

/// Firecrawl scrape API response.
#[derive(Debug, Serialize, Deserialize)]
pub struct ScrapeResponse {
    pub success: bool,
    pub data: Option<ScrapeData>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScrapeData {
    pub markdown: Option<String>,
    pub html: Option<String>,
    pub links: Option<Vec<String>>,
    pub metadata: Option<ScrapeMetadata>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ScrapeMetadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub language: Option<String>,
    #[serde(rename = "sourceURL")]
    pub source_url: Option<String>,
    #[serde(rename = "statusCode")]
    pub status_code: Option<u16>,
}

/// Firecrawl search API response.
///
/// The `data` field can be either:
/// - A list of SearchResult (when scrapeOptions produce full results)
/// - An object like `{"web": [...], "images": [...]}` (when results aren't scraped)
///
/// We use serde_json::Value and parse manually.
#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResponse {
    pub success: bool,
    pub data: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchResult {
    pub url: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
    pub markdown: Option<String>,
    pub links: Option<Vec<String>>,
    pub metadata: Option<ScrapeMetadata>,
}

/// Firecrawl map API response.
#[derive(Debug, Serialize, Deserialize)]
pub struct MapResponse {
    pub success: bool,
    pub links: Option<Vec<MapLink>>,
    pub error: Option<String>,
}

/// A link discovered during map.
/// Title and description may not always be present.
#[derive(Debug, Serialize, Deserialize)]
pub struct MapLink {
    pub url: Option<String>,
    pub title: Option<String>,
    pub description: Option<String>,
}
