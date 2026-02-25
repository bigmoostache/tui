## 1. Overview

This document describes the design of the web search and content extraction tool layer for a real-time AI assistant chatbot. The stack combines two complementary APIs:

- **Brave Search API** — fast, independent web index for discovery and routing
- **Firecrawl** — deep content extraction and full-page scraping

The architecture follows a tiered escalation model: the assistant uses the cheapest, fastest tool that satisfies the query, escalating to heavier tools only when necessary. This keeps latency and cost under control for a real-time use case while preserving content depth and quality when it matters.

---

## 2. Design Principles

**Escalate, don't default to depth.** Most queries can be answered with snippets. Firecrawl is invoked only when the LLM determines snippets are insufficient.

**Intent-aware routing.** Goggles enable source-quality filtering at the search layer before any LLM processing, reducing noise without extra API calls. A built-in skill provides a curated goggle list + discovery guide.

**Parametric tools.** Every tool exposes explicit parameters rather than hardcoded defaults, so the LLM can adapt behavior to query context at runtime.

**Complementary, not redundant.** Brave handles *finding* the right content. Firecrawl handles *reading* it fully. These roles do not overlap.

---

## 3. Engineering Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Module architecture | **Two separate modules**: `cp-mod-brave` + `cp-mod-firecrawl` | Independent activation per API key. If only BRAVE_API_KEY is set, only Brave tools appear. If only FIRECRAWL_API_KEY is set, only Firecrawl tools appear. Cleaner maintenance, no coupling. |
| Shared code | Self-contained modules | Each module is self-contained. No shared web-common crate. Duplicate the few shared types if needed. Simple, no coupling. |
| APIs | Brave Search + Firecrawl | Brave: fastest (669ms), 40B-page independent index. Firecrawl: best extraction, JS rendering, structured output. Complementary roles. |
| API keys | Environment variables | `BRAVE_API_KEY` and `FIRECRAWL_API_KEY` in `.env` or shell. Same pattern as `GITHUB_TOKEN`. |
| Conditional activation | Per-key activation | Module only activates if its API key is present. No key → module doesn't appear → tools hidden. |
| Result presentation | Dynamic panels (standard) | One new panel per tool call. Standard ContextType panels included in LLM context. Existing pagination handles large content. |
| Panel content format | JSON→YAML for structured, markdown pass-through | Brave API returns JSON → convert to YAML for panel (more token-efficient, readable). Firecrawl scrape returns markdown → pass through as-is. |
| Content truncation | None — full content, paginated | Full API response goes into panel. Existing pagination system handles display. No artificial token budget truncation. |
| Tool count | 5 tools | `brave_search`, `brave_llm_context`, `firecrawl_scrape`, `firecrawl_search`, `firecrawl_map` — each maps to a distinct API endpoint with distinct use cases. |
| Tool naming | API-prefixed | `brave_*` and `firecrawl_*` — makes the provider explicit in tool names. |
| HTTP client | `reqwest` (already in workspace) | Async HTTP with JSON support, already used by typst package resolver. |
| Error strategy | Graceful degradation | 429 → return error to LLM (no client-side rate limiting). 403 → surface immediately. 5xx → retry 2x with 1s delay. Empty → transparent "no results" (never escalate blindly). |
| Rate limiting | No client-side limits | Let 429s happen and return the error to the LLM. Simplest implementation. |
| Retry strategy | Retry 5xx only (2x) | Retry 5xx errors up to 2 times with 1s delay. Return 4xx errors immediately. |
| Console guardrail | None | No blocking of curl/wget. Web tools are optional, user might not have keys. |
| Brave Goggles | Built-in skill | A curated skill with 10-15 recommended goggles + a guide for the AI to discover new ones via goggles.brave.com. Not hardcoded in tool defs. |

---

## 4. Tool Specifications

### 4.1 `brave_search`

Primary entry point for all queries. Returns snippets and URLs from Brave's 40-billion-page independent index.
Always sends `extra_snippets=true` for richer results (up to 5 extra excerpts per result).
Transparently auto-fetches rich results (stocks, weather, calculator, crypto, sports) when available.

```
brave_search(
    query: str,                     # The search query. Supports operators: "exact", -exclude, site:domain, filetype:pdf
    count: int = 5,                 # Number of results (1–20). No pagination — single call.
    freshness: str = None,          # Recency filter: "pd" | "pw" | "pm" | "py" | custom range "2024-01-01to2024-06-30"
    country: str = "US",            # 2-letter ISO country code for result localization
    search_lang: str = "en",        # Language of results (ISO 639-1)
    safe_search: str = "moderate",  # "off" | "moderate" | "strict"
    goggles_id: str = None          # URL or inline Brave Goggle for domain re-ranking (see Goggles skill)
)
```

**API endpoint:** `GET https://api.search.brave.com/res/v1/web/search`
**Auth:** `X-Subscription-Token` header.
**Hardcoded:** `extra_snippets=true`, `enable_rich_callback=1`. No offset/pagination.
**Rich results:** If response contains `rich.hint.callback_key`, auto-fetches `GET /res/v1/web/rich?callback_key=...` and appends structured data (stocks, weather, calculator, crypto, sports, definitions, currency, package tracking).
**Response fields sent to panel:** Full API response auto-serialized to YAML.
**Tool result:** Lean summary: "Created panel Pxx: N results for 'query'" (panel has full content).
**Panel title:** `brave_search: <query>`

**Cost:** ~$5 / 1,000 queries.
**Latency:** ~669ms p90 (+~300ms if rich callback needed).
**Timeout:** 10 seconds.

---

### 4.2 `brave_llm_context`

Brave's LLM Context API (launched 2026-02-06). Returns pre-extracted, relevance-scored web content — text chunks, tables, code blocks, structured data — optimized for direct LLM consumption. No scraping needed.

```
brave_llm_context(
    query: str,                         # The search query
    maximum_number_of_tokens: int = 8192,  # Approx max tokens (1024–32768)
    count: int = 20,                    # Max search results to consider (1–50)
    context_threshold_mode: str = "balanced",  # "strict" | "balanced" | "lenient" | "disabled"
    freshness: str = None,              # Same as brave_search
    country: str = "US",                # 2-letter ISO country code
    goggles: str = None                 # Goggle URL or inline definition
)
```

**API endpoint:** `GET/POST https://api.search.brave.com/res/v1/llm/context`
**Auth:** `X-Subscription-Token` header.
**Hardcoded:** `maximum_number_of_urls=20`, `maximum_number_of_snippets=50`, `maximum_number_of_tokens_per_url=4096`. No local enrichments (enable_local skipped for v1).
**Response fields sent to panel:** Full grounding + sources response auto-serialized to YAML. Snippets may contain plain text OR JSON-serialized structured data (tables, schemas, code).
**Tool result:** Lean summary: "Created panel Pxx: N URLs, ~M tokens for 'query'"
**Panel title:** `brave_llm_context: <query>`

**Cost:** Check current Brave API pricing tier.
**Latency:** ~600ms p90.
**Timeout:** 10 seconds.

---

### 4.3 `firecrawl_scrape`

Full-page content extraction from a single known URL. Renders JavaScript via headless Chromium. Returns clean markdown (~67% fewer tokens than raw HTML) plus page links.

```
firecrawl_scrape(
    url: str,                       # Target URL to scrape
    formats: list = ["markdown", "links"],  # "markdown" | "html" | "rawHtml" | "screenshot" | "links" | "summary" | "images"
    location: object = null         # Optional: {country: "US", languages: ["en"]}. Default: US/en.
)
```

**API endpoint:** `POST https://api.firecrawl.dev/v2/scrape`
**Auth:** `Authorization: Bearer` header.
**Hardcoded:** `only_main_content=true` (always strip nav/footer/ads). No actions support (v1). No JSON extract mode (v1).
**Default formats:** `["markdown", "links"]` — clean text + all page URLs for follow-up scraping. LLM can override.
**Response handling:** Markdown content passed through as-is to panel. Links appended at bottom. Metadata included.
**Tool result:** Lean summary: "Created panel Pxx: scraped <url> (<title>)"
**Panel title:** `firecrawl_scrape: <url>`

**Cost:** 1 credit per page.
**Latency:** 1–3s depending on JavaScript complexity.
**Timeout:** 30 seconds.

---

### 4.4 `firecrawl_search`

Combined search + scrape in a single API call. Discovers URLs matching a query and returns full markdown content for the top results.

```
firecrawl_search(
    query: str,                     # The search query
    limit: int = 3,                 # Pages to scrape (1–10). Limit applies per source type.
    sources: list = ["web"],        # "web" | "news" | "images"
    categories: list = null,        # "github" | "research" | "pdf" — target specific domains
    tbs: str = null,                # Time filter: "qdr:h" | "qdr:d" | "qdr:w" | "qdr:m" | "qdr:y" | custom date ranges
    location: str = null            # Location string, e.g. "Germany", "San Francisco,California,United States"
)
```

**API endpoint:** `POST https://api.firecrawl.dev/v2/search`
**Auth:** `Authorization: Bearer` header.
**Hardcoded:** `scrapeOptions: {formats: ["markdown", "links"], only_main_content: true}`. Always scrapes results.
**Response handling:** Without scrape data → YAML (title/url/description list). With scrape data → markdown per page, concatenated with headers.
**Tool result:** Lean summary: "Created panel Pxx: N results for 'query'"
**Panel title:** `firecrawl_search: <query>`

**Cost:** 2 credits per 10 results + 1 credit per scraped page.
**Latency:** 2–5s.
**Timeout:** 30 seconds.

---

### 4.5 `firecrawl_map`

Discovers all URLs on a given domain. Primarily from sitemap, supplemented by SERP and cached crawl data.

```
firecrawl_map(
    url: str,                       # Root domain or subdomain to map
    limit: int = 50,                # Max URLs returned (1–5000)
    search: str = null,             # Optional keyword filter on discovered URLs
    include_subdomains: bool = false,
    location: object = null         # Optional: {country: "US", languages: ["en"]}
)
```

**API endpoint:** `POST https://api.firecrawl.dev/v2/map`
**Auth:** `Authorization: Bearer` header.
**Response fields sent to panel:** Full links array auto-serialized to YAML (url + title + description when available).
**Tool result:** Lean summary: "Created panel Pxx: N URLs discovered on <domain>"
**Panel title:** `firecrawl_map: <url>`

**Cost:** 1 credit per call regardless of URLs returned.
**Latency:** 2–10s depending on site size.
**Timeout:** 30 seconds.

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

### `maximum_number_of_tokens` in `brave_llm_context`

| Use Case | Recommended `maximum_number_of_tokens` |
|---|---|
| Quick factual answer | 2048 |
| Standard Q&A | 8192 (default) |
| Detailed research | 16384 |
| Deep analysis, complex topic | 32768 |

### `context_threshold_mode` in `brave_llm_context`

| Use Case | Recommended mode |
|---|---|
| Precise factual Q&A (fewer, more relevant results) | strict |
| General queries (default) | balanced |
| Broad research (more results, may include tangential) | lenient |
| Dump everything available | disabled |

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

```
# Brave Search API (both brave_search and brave_llm_context)
GET /res/v1/web/search?q=...
Headers:
  Accept: application/json
  Accept-Encoding: gzip
  X-Subscription-Token: <BRAVE_API_KEY>

# Firecrawl (scrape, search, map)
POST /v2/scrape
Headers:
  Authorization: Bearer <FIRECRAWL_API_KEY>
  Content-Type: application/json
```

### HTTP Client

- **Library:** `reqwest::blocking::Client` (sync). Same pattern as all existing modules.
- **Timeouts:** 10s for Brave endpoints, 30s for Firecrawl endpoints.
- **No client-side rate limiting.** API 429s are returned to the LLM as-is.
- **No caching.** Every tool call hits the API fresh.

### Error Handling

Both APIs return standard HTTP error codes. Implement the following strategy:

- **429 Too Many Requests:** Return error immediately to LLM. No client-side rate limiting or backoff. Let the LLM decide whether to retry or try a different approach.
- **403 Forbidden:** Return error immediately. Key issue — surface the error message clearly.
- **5xx Server Error:** Retry up to 2 times with 1s delay, then return error to LLM.
- **4xx (other):** Return error immediately, no retries.
- **Empty results:** Do not escalate blindly. Return a transparent "no results found" response rather than calling progressively heavier tools on a dead-end query.

### Skipped Features (v1)

These features exist in the APIs but are intentionally excluded from v1 to keep scope manageable:

| Feature | API | Reason for skipping |
|---|---|---|
| Firecrawl actions (page interaction) | Firecrawl scrape | Too complex for v1. Requires action DSL. |
| Firecrawl JSON extract mode | Firecrawl scrape | LLM-powered extraction adds 4 credits/page. Niche. |
| Firecrawl batch scraping | Firecrawl | Same cost, adds async polling complexity. |
| Brave local enrichments (POI) | Brave search | 2-step flow with ephemeral IDs. Complex. |
| Brave pagination (offset) | Brave search | Single call sufficient. Max 20 results per call. |
| Firecrawl screenshot format | Firecrawl scrape | Binary data, not useful for LLM context. |
| Firecrawl branding extraction | Firecrawl scrape | Very niche use case. |

---

## 10. Module Structure

Two independent crates, each self-contained:

```
crates/cp-mod-brave/
├── Cargo.toml
└── src/
    ├── lib.rs              # Module trait impl, tool definitions, conditional activation
    ├── api.rs              # Brave Search API + LLM Context API HTTP client
    ├── tools.rs            # Tool dispatch: parse params → call API → format result
    ├── panel.rs            # Dynamic panel rendering (YAML-formatted results)
    └── types.rs            # SearchResult, LLMContext response structs

crates/cp-mod-firecrawl/
├── Cargo.toml
└── src/
    ├── lib.rs              # Module trait impl, tool definitions, conditional activation
    ├── api.rs              # Firecrawl scrape/search/map HTTP client
    ├── tools.rs            # Tool dispatch: parse params → call API → format result
    ├── panel.rs            # Dynamic panel rendering (markdown pass-through / YAML)
    └── types.rs            # ScrapedPage, MapResult response structs
```

### Crate Dependencies (each module)

- `reqwest` — async HTTP client (already in workspace)
- `serde` / `serde_json` — JSON serialization (already in workspace)
- `serde_yaml` — for JSON→YAML conversion of structured results
- `cp-base` — shared types, Module trait, panel infrastructure

### Conditional Activation

Each module checks for its API key at activation time:
- `cp-mod-brave`: activates only if `BRAVE_API_KEY` is set
- `cp-mod-firecrawl`: activates only if `FIRECRAWL_API_KEY` is set
- No key → module doesn't appear in module list → tools hidden from LLM

### Environment Variables

| Variable | Required For | Description |
|---|---|---|
| `BRAVE_API_KEY` | `cp-mod-brave` | Brave Search API subscription token |
| `FIRECRAWL_API_KEY` | `cp-mod-firecrawl` | Firecrawl API bearer token |

### Panel Behavior

All 5 tools create **standard dynamic panels** (one new panel per tool call):
- `brave_search` → YAML panel with title/url/snippet list
- `brave_llm_context` → YAML panel with structured chunks
- `firecrawl_scrape` → markdown panel with extracted page content
- `firecrawl_search` → markdown panel with multiple scraped pages
- `firecrawl_map` → YAML panel with URL list

Panels are standard ContextType panels included in LLM context. Existing pagination handles display of large content. Panels do NOT auto-refresh (web results are point-in-time snapshots).

### Brave Goggles Skill

A built-in skill (not loaded by default) containing:
- Curated list of 10-15 recommended goggles with IDs and descriptions
- Guide for the AI to discover new goggles via goggles.brave.com
- Examples of when to use each goggle category

---

## 11. Open Questions

- [ ] Brave Goggles skill: research and curate the initial 10-15 goggles.
- [ ] Caching: should we cache search results to avoid duplicate queries? If so, TTL? (Current decision: no caching.)
