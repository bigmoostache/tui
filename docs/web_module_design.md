## 1. Overview

This document describes the design of the web search and content extraction tool layer for a real-time AI assistant chatbot. The stack combines two complementary APIs:

- **Brave Search API** — fast, independent web index for discovery and routing
- **Firecrawl** — deep content extraction and full-page scraping

The architecture follows a tiered escalation model: the assistant uses the cheapest, fastest tool that satisfies the query, escalating to heavier tools only when necessary. This keeps latency and cost under control for a real-time use case while preserving content depth and quality when it matters.

---

## 2. Design Principles

**Escalate, don't default to depth.** Most queries can be answered with snippets. Firecrawl is invoked only when the LLM determines snippets are insufficient.

**Intent-aware routing.** Goggles enable source-quality filtering at the search layer before any LLM processing, reducing noise without extra API calls.

**Parametric tools.** Every tool exposes explicit parameters rather than hardcoded defaults, so the LLM can adapt behavior to query context at runtime.

**Complementary, not redundant.** Brave handles *finding* the right content. Firecrawl handles *reading* it fully. These roles do not overlap.

---

## 3. Engineering Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Module architecture | Single `cp-mod-web` crate | One activation toggle, shared config, simpler than two crates |
| APIs | Brave Search + Firecrawl | Brave: fastest (669ms), 40B-page independent index. Firecrawl: best extraction, JS rendering, structured output. Complementary roles. |
| API keys | Environment variables | `BRAVE_API_KEY` and `FIRECRAWL_API_KEY` in `.env` or shell. Same pattern as `GITHUB_TOKEN`. |
| Result presentation | Dynamic panels | Search/scrape results shown as refreshable panels (like git/gh results). Keeps tool responses lean, results browsable. |
| Tool count | 5 tools | `brave_search`, `brave_llm_context`, `firecrawl_scrape`, `firecrawl_search`, `firecrawl_map` — each maps to a distinct API endpoint with distinct use cases. |
| HTTP client | `reqwest` (already in workspace) | Async HTTP with JSON support, already used by typst package resolver. |
| Error strategy | Graceful degradation | 429 → backoff, 403 → surface, 5xx → retry 2x, empty → transparent "no results" (never escalate blindly). |

---

## 4. Tool Specifications

### 4.1 `brave_search`

Primary entry point for all queries. Returns snippets and URLs from Brave's 40-billion-page independent index.

```python
brave_search(
    query: str,                     # The search query
    count: int = 5,                 # Number of results (1–20). Keep ≤ 5 for real-time.
    freshness: str = None,          # Recency filter: "pd" | "pw" | "pm" | "py"
                                    #   pd = past day, pw = past week,
                                    #   pm = past month, py = past year
    country: str = "US",            # 2-letter ISO country code for result localization
    search_lang: str = "en",        # Language of results
    result_filter: str = "web",     # "web" | "news" | "videos" | "web,news"
    safe_search: str = "moderate",  # "off" | "moderate" | "strict"
    goggles_id: str = None          # ID of a Brave Goggle for domain re-ranking
                                    # (see Section 5 for Goggle design)
) -> list[SearchResult]
# Returns: [{title, url, snippet, published_date}]
```

**When to use:** Every query. This is always the first call. If snippets contain the answer, stop here.

**Cost:** ~$5 / 1,000 queries.  
**Latency:** ~669ms p90.

---

### 4.2 `brave_llm_context`

Brave's February 2026 LLM Context API. This is really cool!! Returns AI-ready structured "smart chunks" instead of raw snippets — clean text extraction, structured data, code blocks, forum threads, and YouTube captions within a configurable token budget. Use this as an intermediate step between snippets and full Firecrawl extraction.

```python
brave_llm_context(
    query: str,
    token_budget: int = 2000,       # Max tokens to return (500–8000).
                                    # Tune based on context window budget.
    content_types: list = ["web"],  # Content sources to include:
                                    #   "web" | "news" | "discussions" |
                                    #   "code" | "data"
    freshness: str = None,          # Same as brave_search
    country: str = "US"
) -> LLMContext
# Returns: structured chunks with source attribution, ready to inject into prompt
```

**When to use:** When snippets are insufficient but you don't yet need full-page content. Best for factual Q&A, summarisation, and citation-heavy answers. Avoids a Firecrawl round-trip in many cases.

**Cost:** Higher than `brave_search`; check current Brave API pricing.  
**Latency:** ~600ms p90.

---

### 4.3 `firecrawl_scrape`

Full-page content extraction from a single known URL. Renders JavaScript automatically using headless Chromium. Returns clean markdown that uses ~67% fewer tokens than raw HTML.

```python
firecrawl_scrape(
    url: str,                               # Target URL to scrape
    formats: list = ["markdown"],           # "markdown" | "html" | "screenshot"
                                            # | "links" | "extract"
    only_main_content: bool = True,         # Strip nav, footer, ads, sidebars
    include_tags: list = None,              # Force-include specific HTML tags
                                            # e.g. ["article", "table", "pre"]
    exclude_tags: list = [                  # Force-exclude HTML tags
        "nav", "footer", "aside",
        "header", ".cookie-banner"
    ],
    wait_for: int = 0,                      # Milliseconds to wait before scraping.
                                            # Increase for JS-heavy SPAs (e.g. 2000)
    token_budget: int = 4000                # Truncate output to ~N tokens before
                                            # returning to LLM (apply server-side).
                                            # Full pages can exceed 15K tokens.
) -> ScrapedPage
# Returns: {url, markdown_content, metadata}
```

**When to use:** After `brave_search` or `brave_llm_context` identified the right URL and full content is needed. Primary use cases: long documentation pages, articles, data tables, legal/technical content.

**Cost:** ~$0.83 / 1,000 credits.  
**Latency:** 1–3s depending on JavaScript complexity.

> ⚠️ **Token budget discipline:** Always apply `token_budget` before injecting scraped content into the LLM context. A full page without truncation can consume the entire context window. 4,000 tokens is a reasonable default for most Q&A tasks; increase to 8,000 for detailed research tasks.

---

### 4.4 `firecrawl_search`

Combined search + scrape in a single API call. Discovers URLs matching a query and returns full markdown content for the top results. More expensive and slower than `brave_search` but eliminates the need for a separate scrape step.

```python
firecrawl_search(
    query: str,
    limit: int = 3,                         # Pages to scrape (1–10).
                                            # Keep ≤ 3 for real-time use cases.
    scrape_formats: list = ["markdown"],
    only_main_content: bool = True,
    country: str = "US",
    lang: str = "en",
    freshness_hours: int = None             # Filter results to last N hours.
                                            # Useful for news / time-sensitive queries.
) -> list[ScrapedPage]
# Returns: [{url, title, markdown_content}]
```

**When to use:** When `brave_search` snippets are clearly insufficient *and* the LLM doesn't have a specific target URL yet. Avoids two sequential API calls (search → scrape). Not recommended as a default — higher latency and cost.

**Cost:** ~$1.66 / 1,000 credits (~2 credits per call at `limit=3`).  
**Latency:** 2–5s.

---

### 4.5 `firecrawl_map`

Discovers all URLs on a given domain. Use for site exploration or when the assistant needs to locate a specific page within a known website.

```python
firecrawl_map(
    url: str,                               # Root domain or subdomain to map
    limit: int = 50,                        # Max URLs returned (1–5000)
    search: str = None,                     # Optional keyword filter on discovered URLs
                                            # e.g. "pricing", "changelog", "api"
    include_subdomains: bool = False
) -> list[str]
# Returns: list of discovered URLs
```

**When to use:** Situational. Triggered when the user asks about a specific website's structure, e.g. "find the changelog for X", "where is their API documentation?", "list all docs pages for Y". After mapping, pass relevant URLs to `firecrawl_scrape`.

**Cost:** Depends on site size; billed per URL discovered.  
**Latency:** 2–10s depending on site size.

---

## 5. Brave Goggles

### 5.1 What Are Goggles?

Goggles are Brave's mechanism for re-ranking search results by domain preference. They're defined as text files hosted at a public URL and referenced by ID. They allow source-quality filtering *at the search layer* — before any LLM processing.

### 5.2 Goggle Examples for Tool Definitions

Include a few high-value goggles directly in tool descriptions so the LLM knows they exist:

- **Tech/programming:** Prioritize official docs, Stack Overflow, GitHub, MDN
- **Academic/research:** Prioritize arxiv, scholar, university domains
- **News:** Prioritize established news outlets, filter blogs/opinion

### 5.3 Intent Detection → Goggle Mapping

Put in the tool defs a few main goggle examples. suggest to AI to go and fetch relevant ones for the project by itself if needed

---

## 6. Escalation Tiers

The LLM should follow this escalation pattern:

| Tier | Tool | When | Typical Latency |
|------|------|------|-----------------|
| 1 | `brave_search` | Always first | ~669ms |
| 2 | `brave_llm_context` | Snippets insufficient, need structured content | ~600ms |
| 3a | `firecrawl_scrape` | Need full page from known URL | 1–3s |
| 3b | `firecrawl_search` | Need full pages, no specific URL yet | 2–5s |
| 4 | `firecrawl_map` | Need to explore a site's structure | 2–10s |

**Stop at the lowest tier that answers the query.** Most queries should resolve at Tier 1 or 2.

---

## 7. Parameter Tuning Reference

### `count` in `brave_search`

| Query Type | Recommended `count` |
|---|---|
| Simple factual question | 3 |
| Multi-faceted question | 5 |
| Comparative / research query | 5–10 |
| Real-time news lookup | 3 (add `freshness="pd"`) |

### `token_budget` in `firecrawl_scrape`

| Use Case | Recommended `token_budget` |
|---|---|
| Quick factual extraction | 2,000 |
| Standard Q&A | 4,000 |
| Technical documentation | 6,000 |
| Deep research / analysis | 8,000 |

### `wait_for` in `firecrawl_scrape`

| Page Type | Recommended `wait_for` (ms) |
|---|---|
| Static HTML | 0 |
| Partially dynamic | 1,000 |
| Full SPA (React, Vue, Angular) | 2,000–3,000 |
| Data dashboards | 3,000+ |

---

## 8. Cost Model

Approximate costs at scale. Verify against current API pricing pages before budgeting.

| Tool | Unit Cost | Typical Calls/Query | Est. Cost/Query |
|---|---|---|---|
| `brave_search` | $5 / 1K queries | 1 | $0.005 |
| `brave_llm_context` | TBC (check Brave docs) | 0–1 | ~$0.005–0.01 |
| `firecrawl_scrape` | $0.83 / 1K credits | 0–1 | ~$0.001 |
| `firecrawl_search` | $1.66 / 1K credits | 0–1 | ~$0.005 |
| `firecrawl_map` | Varies by site | 0–1 (rare) | ~$0.001–0.01 |

**Best case (Tier 1 only):** ~$0.005 / query  
**Typical case (Tier 1 + 2):** ~$0.01–0.015 / query  
**Deep extraction (Tier 1 + 3):** ~$0.006–0.015 / query  

At 100K queries/month, expected spend is **$500–$1,500** depending on escalation rate.

---

## 9. Implementation Notes

### Authentication

Both APIs use header-based authentication:

```python
# Brave Search API
headers = {
    "Accept": "application/json",
    "Accept-Encoding": "gzip",
    "X-Subscription-Token": BRAVE_API_KEY
}

# Firecrawl
headers = {
    "Authorization": f"Bearer {FIRECRAWL_API_KEY}",
    "Content-Type": "application/json"
}
```

### Error Handling

Both APIs return standard HTTP error codes. Implement the following strategy:

- **429 Too Many Requests:** Exponential backoff with jitter. Start at 1s, cap at 30s.
- **403 Forbidden:** Key issue — surface to monitoring, do not retry automatically.
- **5xx Server Error:** Retry up to 2 times with 2s delay, then fall back gracefully.
- **Empty results:** Do not escalate blindly. Return a transparent "I couldn't find reliable information on this" response rather than calling progressively heavier tools on a dead-end query.

---

## 10. Module Structure

```
crates/cp-mod-web/
├── Cargo.toml
└── src/
    ├── lib.rs              # Module trait impl, tool definitions, activation
    ├── brave.rs            # Brave Search API + LLM Context API client
    ├── firecrawl.rs        # Firecrawl scrape/search/map client
    ├── tools.rs            # Tool dispatch: parse params → call API → format result
    ├── panel.rs            # Dynamic panel rendering for search/scrape results
    └── types.rs            # SearchResult, ScrapedPage, LLMContext structs
```

### Crate Dependencies

- `reqwest` — async HTTP client (already in workspace)
- `serde` / `serde_json` — JSON serialization (already in workspace)
- `cp-base` — shared types, Module trait, panel infrastructure

### Environment Variables

| Variable | Required | Description |
|---|---|---|
| `BRAVE_API_KEY` | For Brave tools | Brave Search API subscription token |
| `FIRECRAWL_API_KEY` | For Firecrawl tools | Firecrawl API bearer token |

Tools that require a missing key should return a clear error message pointing to the env var name, not silently fail.

### Panel Behavior

All 5 tools create **dynamic panels** showing results:
- `brave_search` → panel with title/url/snippet list
- `brave_llm_context` → panel with structured chunks
- `firecrawl_scrape` → panel with extracted markdown content
- `firecrawl_search` → panel with multiple scraped pages
- `firecrawl_map` → panel with URL list

Panels are read-only and do NOT auto-refresh (web results are point-in-time snapshots, not live data).

---

## 11. Open Questions

- [ ] Brave Goggles: which specific goggles to bundle? Need to research available public goggles.
- [ ] brave_llm_context: verify API availability and exact pricing (Feb 2026 launch).
- [ ] Rate limiting: should we implement client-side rate limiting or rely on API 429 responses?
- [ ] Caching: should we cache search results to avoid duplicate queries? If so, TTL?
- [ ] Console guardrail: should we block `curl`-based web requests in console and redirect to web tools?
