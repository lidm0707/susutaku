# DuckDuckGo Web Search Setup

The agent's web search (`crates/core-agent/src/web_search.rs`) uses the
[DuckDuckGo Instant Answer API](https://api.duckduckgo.com/).

## Configuration

### Endpoint

Defined in code as `API_URL`:

```
https://api.duckduckgo.com/?q=<query>&format=json
```

No API key is required. The optional key env var `WEB_SEARCH_API_KEY`
(`ENV_API_KEY`) is only used for status reporting via `key_state()` →
`KeyState::Present | KeyState::Missing`.

### Optional API key

If you have a key, export it before running:

```sh
export WEB_SEARCH_API_KEY="<your-key>"
```

or put it in your shell profile (`~/.zshrc`) / launcher env. The agent never
reads `.env` files for this.

## Behavior

- `search(query)` GETs the API and parses the JSON into `SearchResult`s.
- `parse` extracts, in order:
  1. Direct answer box (`Answer` / `Heading` / `AbstractURL`)
  2. Definition (`Definition` / `DefinitionURL`)
  3. Abstract (main result)
  4. Related topics (`RelatedTopics`, one level of nesting)
- Hard caps (see constants in `web_search.rs`):
  - `MAX_RESULTS = 8` results max (prompt size guard)
  - `SNIPPET_MAX = 300` chars per snippet, longer ones are truncated with `…`
- Empty/malformed responses return an empty list, not an error.

## Usage

```rust
use crate::web_search::{self, KeyState};

if web_search::key_state() == KeyState::Missing {
    // search still works, but no key is set
}

let results = web_search::search("rust async runtime")?;
println!("{}", web_search::SearchResult::summarize(&results));
```

## Limits

The Instant Answer API returns instant answers and related topics — not full
SERP results. Queries that have no instant answer may return few or zero
results.
