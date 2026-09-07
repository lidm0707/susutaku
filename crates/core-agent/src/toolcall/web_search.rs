use std::env;
use std::fmt::Display;

use serde_json::Value;

pub const API_URL: &str = "https://api.duckduckgo.com/";
pub const ENV_API_KEY: &str = "WEB_SEARCH_API_KEY";
pub const FORMAT_PARAM: &str = "json";
pub const NO_KEY_NOTE: &str = "web_search: key present";

/// Hard cap on returned results (prompt size guard).
pub const MAX_RESULTS: usize = 8;
/// Snippets longer than this are truncated.
pub const SNIPPET_MAX: usize = 300;

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
}

impl Display for SearchResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} | {} | {}", self.title, self.url, self.snippet)
    }
}

impl SearchResult {
    pub fn summarize(items: &[Self]) -> String {
        items
            .iter()
            .map(|r| r.to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum KeyState {
    Present,
    Missing,
}

pub fn key_state() -> KeyState {
    match env::var(ENV_API_KEY) {
        Ok(v) if !v.trim().is_empty() => KeyState::Present,
        _ => KeyState::Missing,
    }
}

pub fn search(query: &str) -> Result<Vec<SearchResult>, String> {
    let url = build_url(query);
    let body = ureq::get(&url)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    Ok(parse(&body))
}

pub fn build_url(query: &str) -> String {
    let encoded: String = query
        .chars()
        .map(|c| match c {
            ' ' => "+".to_string(),
            c if c.is_ascii_alphanumeric() || "-._~".contains(c) => c.to_string(),
            c => format!("%{:02X}", c as u32),
        })
        .collect();
    format!("{API_URL}?q={encoded}&format={FORMAT_PARAM}")
}

pub fn parse(body: &str) -> Vec<SearchResult> {
    let Ok(v) = serde_json::from_str::<Value>(body) else {
        return Vec::new();
    };
    let mut out = Vec::new();

    // Direct answer box (typed queries like "42 km to miles").
    if let Some(answer) = clean(v["Answer"].as_str()) {
        out.push(SearchResult {
            title: format!("Answer: {}", v["Heading"].as_str().unwrap_or_default()),
            url: v["AbstractURL"].as_str().unwrap_or_default().to_string(),
            snippet: answer,
        });
    }

    // Definition (dictionary/encyclopedia style queries).
    if let (Some(def), Some(url)) = (clean(v["Definition"].as_str()), v["DefinitionURL"].as_str()) {
        out.push(SearchResult {
            title: format!("Definition: {}", v["Heading"].as_str().unwrap_or_default()),
            url: url.to_string(),
            snippet: def,
        });
    }

    // Main abstract for the query.
    if let (Some(text), Some(first_url)) =
        (clean(v["AbstractText"].as_str()), v["AbstractURL"].as_str())
    {
        out.push(SearchResult {
            title: v["Heading"].as_str().unwrap_or_default().to_string(),
            url: first_url.to_string(),
            snippet: text,
        });
    }

    // Official/top links ("Results" array).
    collect_topics(v["Results"].as_array().unwrap_or(&Vec::new()), &mut out);

    // Related topics, one level of nesting.
    collect_topics(
        v["RelatedTopics"].as_array().unwrap_or(&Vec::new()),
        &mut out,
    );

    out.truncate(MAX_RESULTS);
    out
}

fn collect_topics(topics: &[Value], out: &mut Vec<SearchResult>) {
    for topic in topics {
        let item = if topic["Topics"].is_array() {
            &topic["Topics"][0]
        } else {
            topic
        };
        if let (Some(text), Some(url)) = (clean(item["Text"].as_str()), item["FirstURL"].as_str()) {
            let title = text.split(" - ").next().unwrap_or(&text).to_string();
            let snippet = if text.len() > SNIPPET_MAX {
                format!("{}…", &text[..SNIPPET_MAX])
            } else {
                text
            };
            out.push(SearchResult {
                title,
                url: url.to_string(),
                snippet,
            });
        }
    }
}

/// Clean an optional API text field: skip empties, strip HTML tags, unescape entities.
fn clean(value: Option<&str>) -> Option<String> {
    let text = value?.trim();
    if text.is_empty() {
        return None;
    }
    Some(crate::toolcall::fetch::strip_tags(text))
}
