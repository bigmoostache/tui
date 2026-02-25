> ## Documentation Index
> Fetch the complete documentation index at: https://docs.firecrawl.dev/llms.txt
> Use this file to discover all available pages before exploring further.

# Search

> Search the web and get full content from results

Firecrawl's search API allows you to perform web searches and optionally scrape the search results in one operation.

* Choose specific output formats (markdown, HTML, links, screenshots)
* Search the web with customizable parameters (location, etc.)
* Optionally retrieve content from search results in various formats
* Control the number of results and set timeouts

For details, see the [Search Endpoint API Reference](https://docs.firecrawl.dev/api-reference/endpoint/search).

<Card title="Try it in the Playground" icon="play" href="https://www.firecrawl.dev/playground?endpoint=search">
  Test searching in the interactive playground — no code required.
</Card>

## Performing a Search with Firecrawl

### /search endpoint

Used to perform web searches and optionally retrieve content from the results.

### Installation

<CodeGroup>
  ```python Python theme={null}
  # pip install firecrawl-py

  from firecrawl import Firecrawl

  firecrawl = Firecrawl(api_key="fc-YOUR-API-KEY")
  ```

  ```js Node theme={null}
  # npm install @mendable/firecrawl-js

  import Firecrawl from '@mendable/firecrawl-js';

  const firecrawl = new Firecrawl({ apiKey: "fc-YOUR-API-KEY" });
  ```

  ```bash CLI theme={null}
  # Install globally with npm
  npm install -g firecrawl

  # Authenticate (one-time setup)
  firecrawl login
  ```
</CodeGroup>

### Basic Usage

<CodeGroup>
  ```python Python theme={null}
  from firecrawl import Firecrawl

  firecrawl = Firecrawl(api_key="fc-YOUR-API-KEY")

  results = firecrawl.search(
      query="firecrawl",
      limit=3,
  )
  print(results)
  ```

  ```js Node theme={null}
  import Firecrawl from '@mendable/firecrawl-js';

  const firecrawl = new Firecrawl({ apiKey: "fc-YOUR-API-KEY" });

  const results = await firecrawl.search('firecrawl', {
    limit: 3,
    scrapeOptions: { formats: ['markdown'] }
  });
  console.log(results);
  ```

  ```bash  theme={null}
  curl -s -X POST "https://api.firecrawl.dev/v2/search" \
    -H "Authorization: Bearer $FIRECRAWL_API_KEY" \
    -H "Content-Type: application/json" \
    -d '{
      "query": "firecrawl",
      "limit": 3
    }'
  ```

  ```bash CLI theme={null}
  # Search the web
  firecrawl search "firecrawl web scraping" --limit 5 --pretty
  ```
</CodeGroup>

### Response

SDKs will return the data object directly. cURL will return the complete payload.

```json JSON theme={null}
{
  "success": true,
  "data": {
    "web": [
      {
        "url": "https://www.firecrawl.dev/",
        "title": "Firecrawl - The Web Data API for AI",
        "description": "The web crawling, scraping, and search API for AI. Built for scale. Firecrawl delivers the entire internet to AI agents and builders.",
        "position": 1
      },
      {
        "url": "https://github.com/firecrawl/firecrawl",
        "title": "mendableai/firecrawl: Turn entire websites into LLM-ready ... - GitHub",
        "description": "Firecrawl is an API service that takes a URL, crawls it, and converts it into clean markdown or structured data.",
        "position": 2
      },
      ...
    ],
    "images": [
      {
        "title": "Quickstart | Firecrawl",
        "imageUrl": "https://mintlify.s3.us-west-1.amazonaws.com/firecrawl/logo/logo.png",
        "imageWidth": 5814,
        "imageHeight": 1200,
        "url": "https://docs.firecrawl.dev/",
        "position": 1
      },
      ...
    ],
    "news": [
      {
        "title": "Y Combinator startup Firecrawl is ready to pay $1M to hire three AI agents as employees",
        "url": "https://techcrunch.com/2025/05/17/y-combinator-startup-firecrawl-is-ready-to-pay-1m-to-hire-three-ai-agents-as-employees/",
        "snippet": "It's now placed three new ads on YC's job board for “AI agents only” and has set aside a $1 million budget total to make it happen.",
        "date": "3 months ago",
        "position": 1
      },
      ...
    ]
  }
}
```

## Search result types

In addition to regular web results, Search supports specialized result types via the `sources` parameter:

* `web`: standard web results (default)
* `news`: news-focused results
* `images`: image search results

You can request multiple sources in a single call (e.g., `sources: ["web", "news"]`). When you do, the `limit` parameter applies **per source type** — so `limit: 5` with `sources: ["web", "news"]` returns up to 5 web results and up to 5 news results (10 total). If you need different parameters per source (for example, different `limit` values or different `scrapeOptions`), make separate calls instead.

## Search Categories

Filter search results by specific categories using the `categories` parameter:

* `github`: Search within GitHub repositories, code, issues, and documentation
* `research`: Search academic and research websites (arXiv, Nature, IEEE, PubMed, etc.)
* `pdf`: Search for PDFs

### GitHub Category Search

Search specifically within GitHub repositories:

```bash cURL theme={null}
curl -X POST https://api.firecrawl.dev/v2/search \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer fc-YOUR_API_KEY" \
  -d '{
    "query": "web scraping python",
    "categories": ["github"],
    "limit": 10
  }'
```

### Research Category Search

Search academic and research websites:

```bash cURL theme={null}
curl -X POST https://api.firecrawl.dev/v2/search \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer fc-YOUR_API_KEY" \
  -d '{
    "query": "machine learning transformers",
    "categories": ["research"],
    "limit": 10
  }'
```

### Mixed Category Search

Combine multiple categories in one search:

```bash cURL theme={null}
curl -X POST https://api.firecrawl.dev/v2/search \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer fc-YOUR_API_KEY" \
  -d '{
    "query": "neural networks",
    "categories": ["github", "research"],
    "limit": 15
  }'
```

### Category Response Format

Each search result includes a `category` field indicating its source:

```json  theme={null}
{
  "success": true,
  "data": {
    "web": [
      {
        "url": "https://github.com/example/neural-network",
        "title": "Neural Network Implementation",
        "description": "A PyTorch implementation of neural networks",
        "category": "github"
      },
      {
        "url": "https://arxiv.org/abs/2024.12345",
        "title": "Advances in Neural Network Architecture",
        "description": "Research paper on neural network improvements",
        "category": "research"
      }
    ]
  }
}
```

Examples:

```bash cURL theme={null}
curl -X POST https://api.firecrawl.dev/v2/search \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer fc-YOUR_API_KEY" \
  -d '{
    "query": "openai",
    "sources": ["news"],
    "limit": 5
  }'
```

```bash cURL theme={null}
curl -X POST https://api.firecrawl.dev/v2/search \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer fc-YOUR_API_KEY" \
  -d '{
    "query": "jupiter",
    "sources": ["images"],
    "limit": 8
  }'
```

### HD Image Search with Size Filtering

Use images operators to find high-resolution images:

```bash cURL theme={null}
curl -X POST https://api.firecrawl.dev/v2/search \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer fc-YOUR_API_KEY" \
  -d '{
    "query": "sunset imagesize:1920x1080",
    "sources": ["images"],
    "limit": 5
  }'
```

```bash cURL theme={null}
curl -X POST https://api.firecrawl.dev/v2/search \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer fc-YOUR_API_KEY" \
  -d '{
    "query": "mountain wallpaper larger:2560x1440",
    "sources": ["images"],
    "limit": 8
  }'
```

**Common HD resolutions:**

* `imagesize:1920x1080` - Full HD (1080p)
* `imagesize:2560x1440` - QHD (1440p)
* `imagesize:3840x2160` - 4K UHD
* `larger:1920x1080` - HD and above
* `larger:2560x1440` - QHD and above

## Search with Content Scraping

Search and retrieve content from the search results in one operation.

<CodeGroup>
  ```python Python theme={null}
  from firecrawl import Firecrawl

  firecrawl = Firecrawl(api_key="fc-YOUR_API_KEY")

  # Search and scrape content
  results = firecrawl.search(
      "firecrawl web scraping",
      limit=3,
      scrape_options={
          "formats": ["markdown", "links"]
      }
  )
  ```

  ```js Node theme={null}
  import Firecrawl from '@mendable/firecrawl-js';

  const firecrawl = new Firecrawl({ apiKey: "fc-YOUR-API-KEY" });

  const results = await firecrawl.search('firecrawl', {
    limit: 3,
    scrapeOptions: { formats: ['markdown'] }
  });
  console.log(results);
  ```

  ```bash cURL theme={null}
  curl -X POST https://api.firecrawl.dev/v2/search \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer fc-YOUR_API_KEY" \
    -d '{
      "query": "firecrawl web scraping",
      "limit": 3,
      "scrapeOptions": {
        "formats": ["markdown", "links"]
      }
    }'
  ```

  ```bash CLI theme={null}
  # Search and scrape results
  firecrawl search "firecrawl" --scrape --scrape-formats markdown --limit 5 --pretty
  ```
</CodeGroup>

Every option in scrape endpoint is supported by this search endpoint through the `scrapeOptions` parameter.

### Response with Scraped Content

```json  theme={null}
{
  "success": true,
  "data": [
    {
      "title": "Firecrawl - The Ultimate Web Scraping API",
      "description": "Firecrawl is a powerful web scraping API that turns any website into clean, structured data for AI and analysis.",
      "url": "https://firecrawl.dev/",
      "markdown": "# Firecrawl\n\nThe Ultimate Web Scraping API\n\n## Turn any website into clean, structured data\n\nFirecrawl makes it easy to extract data from websites for AI applications, market research, content aggregation, and more...",
      "links": [
        "https://firecrawl.dev/pricing",
        "https://firecrawl.dev/docs",
        "https://firecrawl.dev/guides"
      ],
      "metadata": {
        "title": "Firecrawl - The Ultimate Web Scraping API",
        "description": "Firecrawl is a powerful web scraping API that turns any website into clean, structured data for AI and analysis.",
        "sourceURL": "https://firecrawl.dev/",
        "statusCode": 200
      }
    }
  ]
}
```

## Advanced Search Options

Firecrawl's search API supports various parameters to customize your search:

### Location Customization

<CodeGroup>
  ```python Python theme={null}
  from firecrawl import Firecrawl

  firecrawl = Firecrawl(api_key="fc-YOUR_API_KEY")

  # Search with location settings (Germany)
  search_result = firecrawl.search(
      "web scraping tools",
      limit=5,
      location="Germany"
  )

  # Process the results
  for result in search_result.data:
      print(f"Title: {result['title']}")
      print(f"URL: {result['url']}")
  ```

  ```js Node theme={null}
  import Firecrawl from '@mendable/firecrawl-js';

  const firecrawl = new Firecrawl({ apiKey: "fc-YOUR-API-KEY" });

  // Search with location settings (Germany)
  const results = await firecrawl.search('web scraping tools', {
    limit: 5,
    location: "Germany"
  });

  // Process the results
  console.log(results);
  ```

  ```bash cURL theme={null}
  curl -X POST https://api.firecrawl.dev/v2/search \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer fc-YOUR_API_KEY" \
    -d '{
      "query": "web scraping tools",
      "limit": 5,
      "location": "Germany"
    }'
  ```

  ```bash CLI theme={null}
  # Search with location
  firecrawl search "local restaurants" --location "San Francisco,California,United States" --country US --pretty
  ```
</CodeGroup>

### Time-Based Search

Use the `tbs` parameter to filter results by time. Note that `tbs` only applies to `web` source results — it does not filter `news` or `images` results. If you need time-filtered news, consider using a `web` source with the `site:` operator to target specific news domains.

<CodeGroup>
  ```python Python theme={null}
  from firecrawl import Firecrawl

  firecrawl = Firecrawl(api_key="fc-YOUR-API-KEY")

  results = firecrawl.search(
      query="firecrawl",
      limit=5,
      tbs="qdr:d",
  )
  print(len(results.get('web', [])))
  ```

  ```js Node theme={null}
  import Firecrawl from '@mendable/firecrawl-js';

  const firecrawl = new Firecrawl({ apiKey: "fc-YOUR-API-KEY" });

  const results = await firecrawl.search('firecrawl', {
    limit: 5,
    tbs: 'qdr:d', // past day
  });

  console.log(results.web);
  ```

  ```bash cURL theme={null}
  curl -X POST https://api.firecrawl.dev/v2/search \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer fc-YOUR_API_KEY" \
    -d '{
      "query": "latest web scraping techniques",
      "limit": 5,
      "tbs": "qdr:w"
    }'
  ```

  ```bash CLI theme={null}
  # Search with time filter (past week)
  firecrawl search "firecrawl updates" --tbs qdr:w --limit 5 --pretty
  ```
</CodeGroup>

Common `tbs` values:

* `qdr:h` - Past hour
* `qdr:d` - Past 24 hours
* `qdr:w` - Past week
* `qdr:m` - Past month
* `qdr:y` - Past year
* `sbd:1` - Sort by date (newest first)

For more precise time filtering, you can specify exact date ranges using the custom date range format:

<CodeGroup>
  ```python Python theme={null}
  from firecrawl import Firecrawl

  # Initialize the client with your API key
  firecrawl = Firecrawl(api_key="fc-YOUR_API_KEY")

  # Search for results from December 2024
  search_result = firecrawl.search(
      "firecrawl updates",
      limit=10,
      tbs="cdr:1,cd_min:12/1/2024,cd_max:12/31/2024"
  )
  ```

  ```js JavaScript theme={null}
  import Firecrawl from '@mendable/firecrawl-js';

  // Initialize the client with your API key
  const firecrawl = new Firecrawl({apiKey: "fc-YOUR_API_KEY"});

  // Search for results from December 2024
  firecrawl.search("firecrawl updates", {
    limit: 10,
    tbs: "cdr:1,cd_min:12/1/2024,cd_max:12/31/2024"
  })
  .then(searchResult => {
    console.log(searchResult.data);
  });
  ```

  ```bash cURL theme={null}
  curl -X POST https://api.firecrawl.dev/v2/search \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer fc-YOUR_API_KEY" \
    -d '{
      "query": "firecrawl updates",
      "limit": 10,
      "tbs": "cdr:1,cd_min:12/1/2024,cd_max:12/31/2024"
    }'
  ```
</CodeGroup>

You can combine `sbd:1` with time filters to get date-sorted results within a time range. For example, `sbd:1,qdr:w` returns results from the past week sorted newest first, and `sbd:1,cdr:1,cd_min:12/1/2024,cd_max:12/31/2024` returns results from December 2024 sorted by date.

### Custom Timeout

Set a custom timeout for search operations:

<CodeGroup>
  ```python Python theme={null}
  from firecrawl import FirecrawlApp

  # Initialize the client with your API key
  app = FirecrawlApp(api_key="fc-YOUR_API_KEY")

  # Set a 30-second timeout
  search_result = app.search(
      "complex search query",
      limit=10,
      timeout=30000  # 30 seconds in milliseconds
  )
  ```

  ```js JavaScript theme={null}
  import FirecrawlApp from '@mendable/firecrawl-js';

  // Initialize the client with your API key
  const app = new FirecrawlApp({apiKey: "fc-YOUR_API_KEY"});

  // Set a 30-second timeout
  app.search("complex search query", {
    limit: 10,
    timeout: 30000  // 30 seconds in milliseconds
  })
  .then(searchResult => {
    // Process results
    console.log(searchResult.data);
  });
  ```

  ```bash cURL theme={null}
  curl -X POST https://api.firecrawl.dev/v2/search \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer fc-YOUR_API_KEY" \
    -d '{
      "query": "complex search query",
      "limit": 10,
      "timeout": 30000
    }'
  ```
</CodeGroup>

## Cost Implications

The cost of a search is 2 credits per 10 search results. If scraping options are enabled, the standard scraping costs apply to each search result:

* **Basic scrape**: 1 credit per webpage
* **PDF parsing**: 1 credit per PDF page
* **Enhanced proxy mode**: 4 additional credits per webpage
* **JSON mode**: 4 additional credits per webpage

To help control costs:

* Set `parsers: []` if PDF parsing isn’t required
* Use `proxy: "basic"` instead of `"enhanced"` when possible, or set it to `"auto"`
* Limit the number of search results with the `limit` parameter

## Advanced Scraping Options

For more details about the scraping options, refer to the [Scrape Feature documentation](https://docs.firecrawl.dev/features/scrape). Everything except for the FIRE-1 Agent and Change-Tracking features are supported by this Search endpoint.
