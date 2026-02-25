pub mod api;
pub mod panel;
pub mod tools;
pub mod types;

use cp_base::modules::Module;
use cp_base::panels::Panel;
use cp_base::state::{ContextType, ContextTypeMeta, State};
use cp_base::tools::{ParamType, ToolDefinition, ToolParam};
use cp_base::tools::{ToolResult, ToolUse};

pub struct FirecrawlModule;

impl Module for FirecrawlModule {
    fn id(&self) -> &'static str {
        "firecrawl"
    }

    fn name(&self) -> &'static str {
        "Firecrawl"
    }

    fn description(&self) -> &'static str {
        "Web scraping and content extraction via Firecrawl API"
    }

    fn dependencies(&self) -> &[&'static str] {
        &["core"]
    }

    fn is_global(&self) -> bool {
        true
    }

    fn context_type_metadata(&self) -> Vec<ContextTypeMeta> {
        vec![ContextTypeMeta {
            context_type: "firecrawl_result",
            icon_id: "scrape",
            is_fixed: false,
            needs_cache: false,
            fixed_order: None,
            display_name: "firecrawl",
            short_name: "firecrawl",
            needs_async_wait: false,
        }]
    }

    fn dynamic_panel_types(&self) -> Vec<ContextType> {
        vec![ContextType::new("firecrawl_result")]
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                id: "firecrawl_scrape".to_string(),
                name: "Firecrawl Scrape".to_string(),
                short_desc: "Scrape a URL for full content".to_string(),
                description: concat!(
                    "Full-page content extraction from a single known URL. ",
                    "Renders JavaScript via headless Chromium. Returns clean markdown ",
                    "(~67% fewer tokens than raw HTML) plus page links. ",
                    "Use when you need the full content of a specific page (Tier 3a).\n\n",
                    "ESCALATION: Use after brave_search/brave_llm_context when ",
                    "snippets are insufficient and you have a specific URL to extract.\n\n",
                    "FORMATS: 'markdown' (default, clean text), 'html', 'links' (all page URLs), ",
                    "'summary', 'images'. Default: ['markdown', 'links']."
                )
                .to_string(),
                params: vec![
                    ToolParam::new("url", ParamType::String).desc("Target URL to scrape").required(),
                    ToolParam::new("formats", ParamType::Array(Box::new(ParamType::String))).desc(
                        "Output formats: 'markdown'|'html'|'links'|'summary'|'images' (default ['markdown','links'])",
                    ),
                    ToolParam::new(
                        "location",
                        ParamType::Object(vec![
                            ToolParam::new("country", ParamType::String).desc("2-letter country code"),
                            ToolParam::new("languages", ParamType::Array(Box::new(ParamType::String)))
                                .desc("Language codes"),
                        ]),
                    )
                    .desc("Optional location for geo-targeted scraping"),
                ],
                enabled: true,
                category: "Web Scrape".to_string(),
            },
            ToolDefinition {
                id: "firecrawl_search".to_string(),
                name: "Firecrawl Search".to_string(),
                short_desc: "Search and scrape in one call".to_string(),
                description: concat!(
                    "Combined search + scrape in a single API call. Discovers URLs matching a query ",
                    "and returns full markdown content for the top results. ",
                    "Use when you need full pages but don't have specific URLs yet (Tier 3b).\n\n",
                    "ESCALATION: Use after brave_search when snippets are insufficient ",
                    "and you need deep content from multiple pages.\n\n",
                    "CATEGORIES: 'github' (repos/code), 'research' (arxiv/nature/ieee), 'pdf'.\n",
                    "TBS (time filter): 'qdr:h' (hour), 'qdr:d' (day), 'qdr:w' (week), ",
                    "'qdr:m' (month), 'qdr:y' (year)."
                )
                .to_string(),
                params: vec![
                    ToolParam::new("query", ParamType::String).desc("Search query").required(),
                    ToolParam::new("limit", ParamType::Integer).desc("Pages to scrape (1-10, default 3)"),
                    ToolParam::new("sources", ParamType::Array(Box::new(ParamType::String)))
                        .desc("Source types: 'web'|'news'|'images' (default ['web'])"),
                    ToolParam::new("categories", ParamType::Array(Box::new(ParamType::String)))
                        .desc("Target categories: 'github'|'research'|'pdf'"),
                    ToolParam::new("tbs", ParamType::String)
                        .desc("Time filter: 'qdr:h'|'qdr:d'|'qdr:w'|'qdr:m'|'qdr:y'"),
                    ToolParam::new("location", ParamType::String)
                        .desc("Location string, e.g. 'Germany', 'San Francisco,California'"),
                ],
                enabled: true,
                category: "Web Scrape".to_string(),
            },
            ToolDefinition {
                id: "firecrawl_map".to_string(),
                name: "Firecrawl Map".to_string(),
                short_desc: "Discover all URLs on a domain".to_string(),
                description: concat!(
                    "Discovers all URLs on a given domain. Primarily from sitemap, ",
                    "supplemented by SERP and cached crawl data. ",
                    "Use to explore a site's structure before targeted scraping (Tier 4).\n\n",
                    "ESCALATION: Use when you need to understand a site's structure ",
                    "to find the right pages to scrape. Typically followed by firecrawl_scrape.\n\n",
                    "COST: 1 credit per call regardless of URLs returned. Very cheap for exploration."
                )
                .to_string(),
                params: vec![
                    ToolParam::new("url", ParamType::String).desc("Root domain or subdomain to map").required(),
                    ToolParam::new("limit", ParamType::Integer).desc("Max URLs returned (1-5000, default 50)"),
                    ToolParam::new("search", ParamType::String).desc("Optional keyword filter on discovered URLs"),
                    ToolParam::new("include_subdomains", ParamType::Boolean).desc("Include subdomains (default false)"),
                    ToolParam::new(
                        "location",
                        ParamType::Object(vec![
                            ToolParam::new("country", ParamType::String).desc("2-letter country code"),
                            ToolParam::new("languages", ParamType::Array(Box::new(ParamType::String)))
                                .desc("Language codes"),
                        ]),
                    )
                    .desc("Optional location for geo-targeted mapping"),
                ],
                enabled: true,
                category: "Web Scrape".to_string(),
            },
        ]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        tools::dispatch(tool, state)
    }

    fn create_panel(&self, context_type: &ContextType) -> Option<Box<dyn Panel>> {
        if context_type.as_str() == panel::FIRECRAWL_PANEL_TYPE {
            Some(Box::new(panel::FirecrawlResultPanel))
        } else {
            None
        }
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Web Scrape", "Web scraping and content extraction via Firecrawl")]
    }
}
