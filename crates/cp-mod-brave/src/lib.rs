pub mod api;
pub mod panel;
pub mod tools;
pub mod types;

use cp_base::modules::Module;
use cp_base::panels::Panel;
use cp_base::state::{ContextType, ContextTypeMeta, State};
use cp_base::tools::{ParamType, ToolDefinition, ToolParam};
use cp_base::tools::{ToolResult, ToolUse};

pub struct BraveModule;

impl Module for BraveModule {
    fn id(&self) -> &'static str {
        "brave"
    }

    fn name(&self) -> &'static str {
        "Brave Search"
    }

    fn description(&self) -> &'static str {
        "Web search and LLM context via Brave Search API"
    }

    fn dependencies(&self) -> &[&'static str] {
        &["core"]
    }

    fn is_global(&self) -> bool {
        true
    }

    fn context_type_metadata(&self) -> Vec<ContextTypeMeta> {
        vec![ContextTypeMeta {
            context_type: "brave_result",
            icon_id: "search",
            is_fixed: false,
            needs_cache: false,
            fixed_order: None,
            display_name: "brave",
            short_name: "brave",
            needs_async_wait: false,
        }]
    }

    fn dynamic_panel_types(&self) -> Vec<ContextType> {
        vec![ContextType::new("brave_result")]
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                id: "brave_search".to_string(),
                name: "Brave Search".to_string(),
                short_desc: "Search the web via Brave".to_string(),
                description: concat!(
                    "Search the web using Brave's independent 40-billion-page index. ",
                    "Returns snippets, URLs, and optional rich results (stocks, weather, calculator, crypto, sports). ",
                    "Always the FIRST tool to try for any web query (Tier 1). ",
                    "Supports search operators in query: \"exact phrase\", -exclude, site:domain, filetype:pdf.\n\n",
                    "ESCALATION: Start here. If snippets are insufficient, escalate to brave_llm_context (Tier 2) ",
                    "or firecrawl_scrape/firecrawl_search (Tier 3). Stop at the lowest tier that answers the query.\n\n",
                    "PARAMETER GUIDANCE:\n",
                    "- count: 3 for simple facts, 5 for multi-faceted, 5-10 for comparative/research\n",
                    "- freshness: 'pd' (past day), 'pw' (past week), 'pm' (past month), 'py' (past year), or custom 'YYYY-MM-DDtoYYYY-MM-DD'\n",
                    "- goggles_id: URL of a Brave Goggle for domain re-ranking. Load the 'brave-goggles' skill for curated recommendations."
                ).to_string(),
                params: vec![
                    ToolParam::new("query", ParamType::String)
                        .desc("Search query. Supports operators: \"exact\", -exclude, site:domain, filetype:pdf")
                        .required(),
                    ToolParam::new("count", ParamType::Integer)
                        .desc("Number of results (1-20, default 5)"),
                    ToolParam::new("freshness", ParamType::String)
                        .desc("Recency filter: 'pd'|'pw'|'pm'|'py' or 'YYYY-MM-DDtoYYYY-MM-DD'"),
                    ToolParam::new("country", ParamType::String)
                        .desc("2-letter ISO country code (default 'US')"),
                    ToolParam::new("search_lang", ParamType::String)
                        .desc("Result language ISO 639-1 (default 'en')"),
                    ToolParam::new("safe_search", ParamType::String)
                        .desc("'off'|'moderate'|'strict' (default 'moderate')")
                        .enum_vals(&["off", "moderate", "strict"]),
                    ToolParam::new("goggles_id", ParamType::String)
                        .desc("Brave Goggle URL for domain re-ranking"),
                ],
                enabled: true,
                category: "Web Search".to_string(),
            },
            ToolDefinition {
                id: "brave_llm_context".to_string(),
                name: "Brave LLM Context".to_string(),
                short_desc: "Get LLM-optimized web content from Brave".to_string(),
                description: concat!(
                    "Brave's LLM Context API returns pre-extracted, relevance-scored web content — ",
                    "text chunks, tables, code blocks, structured data — optimized for direct LLM consumption. ",
                    "No scraping needed. Use when brave_search snippets are insufficient (Tier 2).\n\n",
                    "ESCALATION: Use after brave_search when you need deeper content. ",
                    "If you need the full page, escalate to firecrawl_scrape (Tier 3).\n\n",
                    "PARAMETER GUIDANCE:\n",
                    "- maximum_number_of_tokens: 2048 for quick facts, 8192 default, 16384 for research, 32768 for deep analysis\n",
                    "- context_threshold_mode: 'strict' for precise Q&A, 'balanced' default, 'lenient' for broad research, 'disabled' to dump everything"
                ).to_string(),
                params: vec![
                    ToolParam::new("query", ParamType::String)
                        .desc("Search query")
                        .required(),
                    ToolParam::new("maximum_number_of_tokens", ParamType::Integer)
                        .desc("Approx max tokens (1024-32768, default 8192)"),
                    ToolParam::new("count", ParamType::Integer)
                        .desc("Max search results to consider (1-50, default 20)"),
                    ToolParam::new("context_threshold_mode", ParamType::String)
                        .desc("Relevance threshold: 'strict'|'balanced'|'lenient'|'disabled' (default 'balanced')")
                        .enum_vals(&["strict", "balanced", "lenient", "disabled"]),
                    ToolParam::new("freshness", ParamType::String)
                        .desc("Recency filter: 'pd'|'pw'|'pm'|'py' or 'YYYY-MM-DDtoYYYY-MM-DD'"),
                    ToolParam::new("country", ParamType::String)
                        .desc("2-letter ISO country code (default 'US')"),
                    ToolParam::new("goggles", ParamType::String)
                        .desc("Brave Goggle URL or inline definition"),
                ],
                enabled: true,
                category: "Web Search".to_string(),
            },
        ]
    }

    fn execute_tool(&self, tool: &ToolUse, state: &mut State) -> Option<ToolResult> {
        tools::dispatch(tool, state)
    }

    fn create_panel(&self, context_type: &ContextType) -> Option<Box<dyn Panel>> {
        if context_type.as_str() == panel::BRAVE_PANEL_TYPE { Some(Box::new(panel::BraveResultPanel)) } else { None }
    }

    fn tool_category_descriptions(&self) -> Vec<(&'static str, &'static str)> {
        vec![("Web Search", "Search the web via Brave Search API")]
    }
}
