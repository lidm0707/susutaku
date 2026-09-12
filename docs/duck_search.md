# DuckDuckGo Web Search Setup

The agent's web search (`crates/core-agent/src/toolcall/web_search.rs`) uses the
[DuckDuckGo Instant Answer API](https://api.duckduckgo.com/).

## No API key

**The DuckDuckGo Instant Answer API does not require an API key, and this
implementation does not require `WEB_SEARCH_API_KEY`.** Search works out of the
box with no environment variables. (The old `key_state()` / `NO_KEY_NOTE`
plumbing was removed — it never gated anything and was misleading.)

## Endpoint

Defined in code as `API_URL`, built by `build_url`:

```
https://api.duckduckgo.com/?q=<url-encoded query>&format=json&no_html=1
```

The query is form-style percent-encoded (spaces as `+`, non-unreserved UTF-8
bytes as `%XX`).

## What this API is (and is not)

This is an **Instant Answer API, not a full SERP / search-results API**. It
returns instant answers, abstracts, definitions and related topics — not a
Google-style list of web results. **Search coverage may be limited for
arbitrary technical queries**; many queries return few or zero results.

## Behavior

- `search(query)` GETs the API and parses the JSON into `SearchResult`s.
  Network/API failures return `Err`; a valid but empty response returns
  `Err(NO_RESULTS_NOTE + query)` ("No useful DuckDuckGo Instant Answer results
  were found for: <query>") — never a key-related message.
- `parse` extracts, in order:
  1. Direct answer box (`Answer` / `Heading` / `AbstractURL`)
  2. Definition (`Definition` / `DefinitionURL`)
  3. Abstract (`AbstractText` / `Heading` / `AbstractURL`)
  4. Official links (`Results` array)
  5. Related topics (`RelatedTopics`, one level of nesting)
- Hard caps (see constants in `web_search.rs`):
  - `MAX_RESULTS = 5` results max (prompt size guard)
  - `SNIPPET_MAX = 300` chars per snippet (char-boundary safe), truncated with `…`
- Empty/malformed JSON returns an empty list from `parse`, not a panic.
- Execution happens in the backend process (e.g. via `spawn_blocking` in
  `backend/src/app/chat.rs`), outside the agent sandbox — the sandbox itself
  stays loopback-only with no internet.

## Usage

```rust
use core_agent::toolcall::web_search;

let results = web_search::search("rust async runtime")?;
println!("{}", web_search::SearchResult::summarize(&results));
```

Model-side protocol (unchanged): the model replies with `TOOL: SEARCH <query>`;
the backend runs at most `TOOL_ROUNDS_MAX = 2` tool rounds.
