//! Web adapters: DuckDuckGo search + page fetch, behind outbound ports.

use crate::domain::SearchResult;
use crate::port::outbound::{Fetcher, Searcher};

pub struct DuckDuckGo;

impl From<core_agent::toolcall::web_search::SearchResult> for SearchResult {
    fn from(r: core_agent::toolcall::web_search::SearchResult) -> Self {
        Self {
            title: r.title,
            url: r.url,
            snippet: r.snippet,
        }
    }
}

impl Searcher for DuckDuckGo {
    fn search(&self, query: &str) -> Result<Vec<SearchResult>, String> {
        core_agent::toolcall::web_search::search(query)
            .map(|hits| hits.into_iter().map(Into::into).collect())
    }
}

pub struct PageFetcher;

impl Fetcher for PageFetcher {
    fn fetch(&self, url: &str) -> Result<String, String> {
        ureq::get(url)
            .call()
            .map_err(|e| e.to_string())?
            .into_string()
            .map_err(|e| e.to_string())
    }
}
