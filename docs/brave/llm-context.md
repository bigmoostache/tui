
Brave
Documentation
API Reference
Dashboard
Getting started

    Quickstart
    Pricing 

Basics

    Authentication
    Versioning
    Rate limiting 

Search APIs

    Web search
    LLM Context New
    News search
    Video search
    Image search
    Summarizer search
    Place search New

Answer APIs

    Answers 

Other APIs

    Autosuggest
    Spellcheck 

Resources

    Skills
    Help & Feedback
    Goggles
    Search operators
    Status updates
    Security
    Privacy notice
    Terms of service 

Service APIs

Pre-extracted web content optimized for AI agents, LLM grounding, and RAG pipelines
Overview

Brave LLM Context API delivers pre-extracted, relevance-scored web content optimized for grounding LLM responses in real-time search results. Unlike traditional web search APIs that return links and snippets, LLM Context extracts the actual page content—text chunks, tables, code blocks, and structured data so your LLM or AI agent can reason over it directly.

This makes it ideal for AI agents that need web access as a tool, RAG (Retrieval-Augmented Generation) pipelines, and any application that needs to ground LLM output in fresh, verifiable web content with a single API call.
Key Features
Pre-Extracted Content

Get actual page content (text, tables, code) ready for LLM consumption—no scraping needed
Token Budget Control

Fine-tune the amount of context with configurable token and URL limits
Relevance Filtering

Adjustable threshold modes ensure only relevant content reaches your LLM
Goggles Support

Control which sources ground your LLM using Brave’s unique Goggles re-ranking system
Local & POI Results

Location-aware queries with point-of-interest and map data
Fast Single-Search

Optimized for speed with a single search per request
API Reference
LLM Context API Documentation

View the complete API reference, including parameters and response schemas
Use Cases

LLM Context is perfect for:

    AI Agents: Give your agent a web search tool that returns ready-to-use content in a single call
    RAG Pipelines: Ground LLM responses in fresh, relevant web content
    AI Assistants & Chatbots: Provide factual answers backed by real sources
    Question Answering: Retrieve focused context for specific queries
    Fact Checking: Verify claims against current web content
    Content Research: Gather source material on any topic with one API call

Endpoint

GET https://api.search.brave.com/res/v1/llm/context
POST https://api.search.brave.com/res/v1/llm/context

Authentication: Include your API key in the X-Subscription-Token header.
Quick Start
GET Request

curl -X GET "https://api.search.brave.com/res/v1/llm/context?q=tallest+mountains+in+the+world" \
  -H "X-Subscription-Token: <YOUR_API_KEY>"

POST Request

POST accepts the same query parameters as a JSON request body with Content-Type: application/json. This is useful for complex queries or when parameters exceed URL length limits.

curl -s --compressed -X POST "https://api.search.brave.com/res/v1/llm/context" \
  -H "accept: application/json" \
  -H "Accept-Encoding: gzip" \
  -H "X-Subscription-Token: <YOUR_API_KEY>" \
  -H "Content-Type: application/json" \
  -d '{"q": "tallest mountains in the world"}'

Parameters
Query Parameters
Parameter	Type	Default	Range	Description
q	string	required	1-400 chars, max 50 words	The search query
country	string	us	2-char code	Country for search results
search_lang	string	en	2+ char code	Language preference for results
count	int	20	1-50	Maximum number of search results to consider
Context Size Parameters
Parameter	Type	Default	Range	Description
maximum_number_of_urls	int	20	1-50	Maximum URLs in the response
maximum_number_of_tokens	int	8192	1024-32768	Approximate maximum tokens in context
maximum_number_of_snippets	int	50	1-100	Maximum snippets across all URLs
maximum_number_of_tokens_per_url	int	4096	512-8192	Maximum tokens per individual URL
maximum_number_of_snippets_per_url	int	50	1-100	Maximum snippets per individual URL
Filtering & Local Parameters
Parameter	Type	Default	Options	Description
context_threshold_mode	string	balanced	strict, balanced, lenient, disabled	Relevance threshold for including content
enable_local	bool	null	true, false, null	Enable local recall for location-aware queries. When not set (null), auto-detects based on whether location headers are provided
goggles	string/list	null	—	Goggle URL or inline definition for custom re-ranking
Context Size Guidelines

Adjust context parameters based on your task complexity. For agent tool calls, smaller budgets keep responses fast; for deep research, increase them:
Task Type	count	max_tokens	Example
Simple factual	5	2048	“What year was Python created?”
Standard queries	20	8192	“Best practices for React hooks”
Complex research	50	16384	“Compare AI frameworks for production”

Larger context windows provide more information but increase latency and cost (of your inference). Start with the defaults and adjust based on your use case.
Threshold Modes

The context_threshold_mode parameter controls how aggressively the API filters content for relevance:
Mode	Behavior
strict	Higher threshold — fewer but more relevant results
balanced	Default — good balance between coverage and relevance
lenient	Lower threshold — more results, may include less relevant content
disabled	No threshold filtering — return all extracted content
Local Recall

The enable_local parameter controls whether location-aware recall is used:
Value	Behavior
null (not set)	Auto-detect — default, local recall is enabled automatically when any location header is provided
true	Force local — always use local recall, even without location headers
false	Force standard — always use standard web ranking, even when location headers are present

For most use cases, you can omit enable_local entirely and let the API auto-detect based on whether you provide location headers. Set it explicitly only when you need to override the default behavior.
Location-Aware Queries

For local queries (restaurants, businesses, directions), provide location context via headers. Local recall will be enabled automatically when location headers are present, or you can set enable_local=true explicitly.
Header	Type	Description
X-Loc-Lat	float	Latitude (-90.0 to 90.0)
X-Loc-Long	float	Longitude (-180.0 to 180.0)
X-Loc-City	string	City name
X-Loc-State	string	State/region code (ISO 3166-2)
X-Loc-State-Name	string	State/region name
X-Loc-Country	string	2-letter country code
X-Loc-Postal-Code	string	Postal code

Not all headers are required. If you have coordinates (X-Loc-Lat and X-Loc-Long), those alone are usually sufficient. Otherwise, provide whichever place-name headers you have available (city, state, country, etc.).

Example using coordinates:

curl -X GET "https://api.search.brave.com/res/v1/llm/context" \
  -H "X-Subscription-Token: <YOUR_API_KEY>" \
  -H "X-Loc-Lat: 37.7749" \
  -H "X-Loc-Long: -122.4194" \
  -G \
  --data-urlencode "q=best coffee shops near me"

Example using place name:

curl -X GET "https://api.search.brave.com/res/v1/llm/context" \
  -H "X-Subscription-Token: <YOUR_API_KEY>" \
  -H "X-Loc-City: San Francisco" \
  -H "X-Loc-State: CA" \
  -H "X-Loc-Country: US" \
  -G \
  --data-urlencode "q=best coffee shops near me"

Example with explicit enable_local=true (no location headers needed):

curl -X GET "https://api.search.brave.com/res/v1/llm/context" \
  -H "X-Subscription-Token: <YOUR_API_KEY>" \
  -G \
  --data-urlencode "q=best coffee shops in san francisco" \
  --data-urlencode "enable_local=true"

When local recall is active, the response may include poi (point of interest) and map fields in addition to the standard generic array.
Goggles (Custom Source Ranking)

Goggles let you tailor which sources ground your LLM to better match your use case. You can restrict results to trusted domains, exclude user-generated content, or boost authoritative sources.

Goggles can be provided as a URL pointing to a hosted goggle file, or as an inline definition passed directly in the goggles parameter. For example, to restrict results to specific documentation sites, use an inline goggle with site rules.

For detailed syntax and examples, see the Goggles documentation.
Response Format

The response contains extracted web content organized into grounding data (by content type) and sources metadata.
Standard Response

{
  "grounding": {
    "generic": [
      {
        "url": "https://example.com/page",
        "title": "Page Title",
        "snippets": [
          "Relevant text chunk extracted from the page...",
          "Another relevant passage from the same page..."
        ]
      }
    ],
    "map": []
  },
  "sources": {
    "https://example.com/page": {
      "title": "Page Title",
      "hostname": "example.com",
      "age": ["Monday, January 15, 2024", "2024-01-15", "380 days ago"]
    }
  }
}

Local Response (with enable_local)

When local recall is active, the response may include POI and map data:

{
  "grounding": {
    "generic": [...],
    "poi": {
      "name": "Business Name",
      "url": "https://business.com",
      "title": "Title of business.com website",
      "snippets": ["Business details and information..."]
    },
    "map": [
      {
        "name": "Place Name",
        "url": "https://place.com",
        "title": "Title of place.com website",
        "snippets": ["Place information and details..."]
      }
    ]
  },
  "sources": {
    "https://business.com": {
      "title": "Business Name",
      "hostname": "business.com",
      "age": null
    }
  }
}

Response Fields
Field	Type	Description
grounding	object	Container for all grounding content by type
grounding.generic	array	Array of URL objects with extracted content (main grounding data)
grounding.generic[].url	string	Source URL
grounding.generic[].title	string	Page title
grounding.generic[].snippets	array	Extracted text chunks from the page relevant to the query
grounding.poi	object/null	Point of interest data (only with local recall enabled)
grounding.map	array	Map/place results (only with local recall enabled)
sources	object	Metadata for all referenced URLs, keyed by URL
sources[url].title	string	Page title
sources[url].hostname	string	Source hostname
sources[url].age	array/null	Page modification dates (when available)

Snippets may contain plain text or JSON-serialized structured data (tables, schemas, code blocks). LLMs handle this mixed format well, but you should be prepared for both when post-processing.
LLM Context vs Answers

Brave offers several complementary approaches for AI-powered search:
LLM Context

Raw extracted content for your own LLM pipeline. Best for AI agents, RAG, and applications where you control the model.
Answers

Direct AI answers using OpenAI-compatible endpoint. Best for chat interfaces that need instant, grounded AI responses.

When to use LLM Context:

    Giving your AI agent a web search tool it can call autonomously
    Building RAG pipelines with your own LLM
    Need full control over how context is processed and presented
    Want raw extracted content without AI-generated summaries
    Optimizing for speed with single-search retrieval
    Need fine-grained control over token budgets and source filtering

When to use Answers:

    Want end-to-end AI answers with citations
    Need OpenAI SDK compatibility
    Building conversational AI agents or chatbots with built-in search
    Require research mode for thorough, multi-search answers

Learn more about Answers and Summarizer Search.
Best Practices
Token Budget Tuning

    Start with defaults (maximum_number_of_tokens=8192, count=20) for most queries
    Reduce for simple factual lookups to save latency and cost (of your inference)
    Increase for complex research tasks that benefit from more context

Source Quality

    Use Goggles to restrict context to trusted, authoritative sources
    Set context_threshold_mode=strict when precision matters more than recall

Error Handling

    Set a 30-second timeout for requests
    Handle empty grounding.generic arrays gracefully—this means no relevant content was found
    Implement retry logic with exponential backoff for transient failures
    Check rate limit headers and respect the 1-second sliding window

Performance

    Use the smallest count and maximum_number_of_tokens values that meet your needs
    For local queries, provide as many location headers as possible for better results

Changelog

This changelog outlines all significant changes to the Brave LLM Context API in chronological order.
2026-02-06

    Launch Brave LLM Context API at /v1/llm/context
    Support for both GET and POST methods
    Single-search context retrieval with configurable token budgets
    Support for Goggles, local/POI queries, and relevance threshold modes

On this page

    Overview
    Key Features
    API Reference
    Use Cases
    Endpoint
    Quick Start
    GET Request
    POST Request
    Parameters
    Query Parameters
    Context Size Parameters
    Filtering & Local Parameters
    Context Size Guidelines
    Threshold Modes
    Local Recall
    Location-Aware Queries
    Goggles (Custom Source Ranking)
    Response Format
    Standard Response
    Local Response (with `enable_local`)
    Response Fields
    LLM Context vs Answers
    Best Practices
    Token Budget Tuning
    Source Quality
    Error Handling
    Performance
    Changelog
    2026-02-06

LLM Context - Brave Search API
