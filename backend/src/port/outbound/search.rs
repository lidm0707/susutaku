//! Web access ports.

/// Port: finds web results for a query.
pub trait Searcher: Send + Sync + 'static {
    fn search(&self, query: &str) -> Result<Vec<crate::domain::SearchResult>, String>;
}

/// Port: reads a web page as text.
pub trait Fetcher: Send + Sync + 'static {
    fn fetch(&self, url: &str) -> Result<String, String>;
}
