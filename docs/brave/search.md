
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

Search from a large index of web pages with optional local and rich data enrichments
Overview

Web Search provides access to our comprehensive index of web pages, enabling you to retrieve relevant results from across the internet. Our service crawls and indexes billions of web pages, ensuring fresh and accurate search results for your applications.
Key Features
Comprehensive Index

Search across billions of indexed web pages with fast, reliable results
Fresh Results

Regularly updated index ensures you get the most current information
Local Enrichments

Enhanced results with local business data and geographic context
Rich Data Enrichments

3rd party data integration for rich real-time results

Local enrichments and rich 3rd party data enrichments require the Search plan. View pricing to learn more.
API Reference
Web Search API Documentation

View the complete API reference, including endpoints, parameters, and example requests
Use Cases

Web Search is perfect for:

    Search Applications: Build custom search experiences for your users
    Content Aggregation: Gather information from multiple web sources
    Market Research: Track mentions, trends, and competitor activity
    Data Enrichment: Supplement your data with web-sourced information

Freshness Filtering

Web Search offers powerful date-based filtering to help you find the most relevant content:

    Last 24 Hours (pd): Get the latest updates and recent content
    Last 7 Days (pw): Track weekly trends and recent discussions
    Last 31 Days (pm): Monitor monthly developments
    Last Year (py): Search content from the past year
    Custom Date Range: Specify exact timeframes (e.g., 2022-04-01to2022-07-30)

Example request filtering for web pages from the past week:

curl "https://api.search.brave.com/res/v1/web/search?q=machine+learning+tutorials&freshness=pw" \
  -H "X-Subscription-Token: <YOUR_API_KEY>"

Country and Language Targeting

Customize your web search results by specifying:

    Country: Target results from specific countries using 2-character country codes
    Search Language: Filter results by content language
    UI Language: Set the preferred language for response metadata

Example request for German content from Germany:

curl "https://api.search.brave.com/res/v1/web/search?q=nachhaltige+energie&country=DE&search_lang=de" \
  -H "X-Subscription-Token: <YOUR_API_KEY>"

Extra Snippets

The extra snippets feature provides up to 5 additional excerpts per search result, giving you more context from each web page. This is particularly useful for:

    Comprehensive content preview before clicking through
    Better relevance assessment for search applications
    Enhanced user experience with richer result cards

To enable extra snippets, add the extra_snippets query parameter set to true:

curl "https://api.search.brave.com/res/v1/web/search?q=python+web+frameworks&extra_snippets=true" \
  -H "X-Subscription-Token: <YOUR_API_KEY>"

When enabled, each result in the web.results array will include an additional extra_snippets property containing an array of alternative excerpts:

{
  "web": {
    "results": [
      {
        "title": "Python Web Frameworks",
        "url": "https://example.com/python-frameworks",
        "description": "Main snippet text...",
        "extra_snippets": [
          "First additional excerpt from the page...",
          "Second additional excerpt from the page...",
          "Third additional excerpt from the page..."
        ]
      }
    ]
  }
}

Goggles Support

Web Search supports Goggles, which allow you to apply custom re-ranking on top of search results. You can:

    Boost or demote specific websites and domains
    Filter by custom criteria
    Create personalized ranking algorithms

Goggles can be provided as a URL or inline definition, and multiple goggles can be combined.
Search Operators

Web Search supports search operators to refine your queries. These operators are included directly within the q query parameter itself, not as separate API parameters:

    Use quotes for exact phrase matching: "climate change solutions"
    Exclude terms with minus: javascript -jquery
    Site-specific searches: site:github.com rust tutorials
    File type searches: filetype:pdf machine learning

For example, to search for PDF files about machine learning:

curl "https://api.search.brave.com/res/v1/web/search?q=machine+learning+filetype:pdf" \
  -H "X-Subscription-Token: <YOUR_API_KEY>"

Pagination

Efficiently paginate through web search results:

    count: Maximum number of results per page (max 20, default 20). The actual number of results returned may be less than count.
    offset: Starting position for results (0-based, max 9)

Example request for page 2 with up to 20 results per page:

curl "https://api.search.brave.com/res/v1/web/search?q=open+source+projects&count=20&offset=1" \
  -H "X-Subscription-Token: <YOUR_API_KEY>"

Best Practice: Check more_results_available

Rather than blindly iterating with increasing offset values, check the more_results_available field in the response to determine if additional pages exist. This field is located in the query object of the response:

{
  "query": {
    "original": "open source projects",
    "more_results_available": true
  }
}

Only request the next page if more_results_available is true. This prevents unnecessary API calls when no more results are available.
Safe Search

Control adult content filtering with the safesearch parameter:

    off: No filtering
    moderate: Filter explicit content (default)
    strict: Filter explicit and suggestive content

Local enrichments

Local enrichments provide extra information about places of interest (POI), such as images and the websites where the POI is mentioned. The Local Search API is a separate endpoint from Web Search, requiring a two-step process (similar to the Summarizer API).
Step 1: Query Web Search for Locations

First, make a request to the web search endpoint with a location-based query:

curl "https://api.search.brave.com/res/v1/web/search?q=greek+restaurants+in+san+francisco" \
  -H "X-Subscription-Token: <YOUR_API_KEY>"

If the query returns a list of locations, each location result includes an id field — a temporary identifier that can be used to retrieve extra information:

{
  "locations": {
    "results": [
      {
        "id": "1520066f3f39496780c5931d9f7b26a6",
        "title": "Pangea Banquet Mediterranean Food",
        ...
      },
      {
        "id": "d00b153c719a427ea515f9eacf4853a2",
        "title": "Park Mediterranean Grill",
        ...
      },
      {
        "id": "4b943b378725432aa29f019def0f0154",
        "title": "The Halal Mediterranean Co.",
        ...
      }
    ]
  }
}

Step 2: Fetch Local POI Details

Use the id values to fetch detailed POI information from the Local Search API endpoints. The ids query parameter accepts up to 20 location IDs:

curl "https://api.search.brave.com/res/v1/local/pois?ids=1520066f3f39496780c5931d9f7b26a6&ids=d00b153c719a427ea515f9eacf4853a2" \
  -H "X-Subscription-Token: <YOUR_API_KEY>"

To fetch AI-generated descriptions for locations:

curl "https://api.search.brave.com/res/v1/local/descriptions?ids=1520066f3f39496780c5931d9f7b26a6&ids=d00b153c719a427ea515f9eacf4853a2" \
  -H "X-Subscription-Token: <YOUR_API_KEY>"

Local POIs Parameters

The Local POIs endpoint (/local/pois) supports the following parameters:
Parameter	Type	Description
ids (required)	array	Location IDs from the web search response (max 20)
search_lang	string	Search language preference (ISO 639-1, default: en)
ui_lang	string	UI language for response (e.g., en-US)
units	string	Measurement units: metric or imperial

Additionally, you can provide location hints via headers:

    x-loc-lat: Client latitude (-90.0 to +90.0)
    x-loc-long: Client longitude (-180.0 to +180.0)

Local Descriptions Parameters

The Local Descriptions endpoint (/local/descriptions) accepts only the ids parameter (same format as above, max 20).

For complete API documentation, see the Local POIs API Reference and Local Descriptions API Reference.

Note that the id fields of POIs are ephemeral and will expire after approximately 8 hours. Do not store them for later use.
Rich Data Enrichments

Rich Search API responses provide accurate, real-time information about the intent of the query. This data is sourced from 3rd-party API providers and includes verticals such as sports, stocks, and weather.

A request must be made to the web search endpoint with the query parameter enable_rich_callback=1. An example cURL request for the query weather in munich is given below.

curl "https://api.search.brave.com/res/v1/web/search?q=weather+in+munich&enable_rich_callback=1" \
  -H "X-Subscription-Token: <YOUR_API_KEY>"

The Web Search API response contains a rich field if the query is expected to return rich results. An example of the rich field is given below.

{
  "rich": {
    "type": "rich",
    "hint": {
      "vertical": "weather",
      "callback_key": "86d06abffc884e9ea281a40f62e0a5a6"
    }
  }
}

The rich field of Web Search API response contains a callback_key field which can be used to fetch the rich results. An example cURL request to fetch the rich results is given below.

curl "https://api.search.brave.com/res/v1/web/rich?callback_key=86d06abffc884e9ea281a40f62e0a5a6" \
  -H "X-Subscription-Token: <YOUR_API_KEY>"

Supported Rich Result Types

The Rich Search API provides detailed information across multiple verticals, matching the query intent. Each result includes a type field (always set to rich) and a subtype field indicating the specific vertical.

Some of these providers will require attribution for showing this data.
Calculator

Calculator results for mathematical expressions. Use this for queries involving arithmetic operations, complex calculations, and mathematical expressions.
Definitions

Word definitions and meanings.

Data provided by Wordnik.
Unit Conversion

Unit conversion calculations and results. Convert between different measurement units (length, weight, volume, temperature, etc.).
Unix Timestamp

Unix timestamp conversion results. Convert between Unix timestamps and human-readable date/time formats.
Package Tracker

Package tracking information. Track shipments and delivery status from various carriers.
Stock

Stock market information and price data. Access real-time stock quotes and intraday changes.

Data provided by FMP.
Currency

Currency conversion results. Provides exchange rates and conversion between different currencies.

Data provided by Fixer.
Cryptocurrency

Cryptocurrency information and pricing data. Get real-time prices, market data, and trends for digital currencies.

Data provided by CoinGecko.
Weather

Weather forecast and current conditions. Get detailed weather information including temperature, precipitation, wind, and extended forecasts.

Data provided by OpenWeatherMap.
American Football

American football scores, schedules, and statistics.

Supported Leagues:

    NFL (USA)
    CFB (USA)

Data provided by Stats Perform.
Baseball

Baseball scores, schedules, and statistics.

Supported Leagues:

    MLB (USA)

Data provided by API Sports.
Basketball

Basketball scores, schedules, and statistics.

Supported Leagues:

    ABA League (Europe)
    BBL: Basket Bundesliga (Germany)
    NBA: National Basketball Association (US & Canada)
    Liga ACB (Spain)
    Eurobasket (Europe)
    Euroleague (Europe)
    NBL (Australia)
    LNB (France)
    WNBA (USA)
    NBA-G (USA)
    Korisliiga (Finland)
    Basket League (Greece)
    Lega A (Italy)
    LKL (Lithuania)
    LNBP (Mexico)
    LEB Oro (Spain)
    LEB Plata (Spain)
    Super Ligi (Turkey)
    BBL (United Kingdom)

Data provided by API Sports.
Cricket

Cricket scores, schedules, and statistics.

Supported Leagues:

    IPL (India)
    PSL (Pakistan)

Data provided by Stats Perform.
Football (Soccer)

Football scores, schedules, and statistics.

Supported Leagues:

    Major League Soccer (USA)
    English Premier League (UK)
    Bundesliga (Germany)
    La Liga (Spain)
    Serie A (Italy)
    UEFA Champions League (International)
    UEFA Europa League (International)
    UEFA European Championship (International)
    FIFA World Cup (International)
    FIFA Women’s World Cup (International)
    CONMEBOL Copa America (International)
    CONMEBOL Libertadores (International)
    Ligue 1 (France)
    Serie A (Brazil)
    Serie B (Brazil)
    Copa do Brasil (Brazil)
    Primeira Liga (Portugal)
    Primera Division (Argentina)
    Tipp3 Bundesliga (Austria)
    Primera A (Colombia)
    NWSL (USA)
    Liga MX (Mexico)
    Primera Division (Chile)
    Primera Division (Peru)
    Saudi Arabia Pro League (Saudi Arabia)
    Indian Super League (India)
    Premier Division (Ireland)
    Premier League (Malta)
    Campeonato Paulista (Brazil)
    Campeonato Paranaense (Brazil)
    Campeonato Carioca (Brazil)
    Campeonato Mineiro (Brazil)
    Eredivisie (Netherlands)

Data provided by API Sports.
Ice Hockey

Ice hockey scores, schedules, and statistics.

Supported Leagues:

    NHL: National Hockey League (US & Canada)
    Liiga (Finland)

Data provided by API Sports.
Changelog

This changelog outlines all significant changes to the Brave Web Search API in chronological order.

    2023-01-01 Add Brave Web Search API resource.
    2023-04-14 Change SearchResult restaurant property to location.
    2023-10-11 Add spellcheck flag.
    2024-06-11 Add Brave Local Search API resource.
    2025-02-20 Add Brave Rich Search API resource.

On this page

    Overview
    Key Features
    API Reference
    Use Cases
    Freshness Filtering
    Country and Language Targeting
    Extra Snippets
    Goggles Support
    Search Operators
    Pagination
    Best Practice: Check `more_results_available`
    Safe Search
    Local enrichments
    Step 1: Query Web Search for Locations
    Step 2: Fetch Local POI Details
    Local POIs Parameters
    Local Descriptions Parameters
    Rich Data Enrichments
    Supported Rich Result Types
    Changelog

Web search - Brave Search API
