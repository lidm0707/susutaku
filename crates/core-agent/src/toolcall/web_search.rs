use std::fmt::Display;

use serde_json::Value;

pub const API_URL: &str = "https://api.duckduckgo.com/";
pub const PARAM_QUERY: &str = "q";
pub const PARAM_FORMAT: &str = "format";
pub const FORMAT_PARAM: &str = "json";
pub const PARAM_NO_HTML: &str = "no_html";

/// Hard cap on returned results (prompt size guard).
pub const MAX_RESULTS: usize = 5;
/// Shown when the Instant Answer API yields nothing useful.
pub const NO_RESULTS_NOTE: &str = "No useful DuckDuckGo Instant Answer results were found for: ";
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

pub fn search(query: &str) -> Result<Vec<SearchResult>, String> {
    let results = request(&build_url(query))?;
    if results.is_empty() {
        return Err(format!("{NO_RESULTS_NOTE}{query}"));
    }
    Ok(results)
}

pub fn request(url: &str) -> Result<Vec<SearchResult>, String> {
    let body = ureq::get(url)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    Ok(parse(&body))
}

pub fn build_url(query: &str) -> String {
    let encoded = encode_query(query);
    format!("{API_URL}?{PARAM_QUERY}={encoded}&{PARAM_FORMAT}={FORMAT_PARAM}&{PARAM_NO_HTML}=1")
}

/// Form-style percent-encoding: spaces as `+`, unreserved bytes as-is,
/// everything else percent-encoded over the raw UTF-8 bytes.
fn encode_query(query: &str) -> String {
    const UNRESERVED: &str = "-._~";
    let mut out = String::with_capacity(query.len());
    for &b in query.as_bytes() {
        match b {
            b' ' => out.push('+'),
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' => out.push(b as char),
            b if UNRESERVED.as_bytes().contains(&b) => out.push(b as char),
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
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
            let snippet = truncate(&text);
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

/// Char-boundary-safe truncation to `SNIPPET_MAX` chars.
fn truncate(text: &str) -> String {
    if text.chars().count() <= SNIPPET_MAX {
        return text.to_string();
    }
    let head: String = text.chars().take(SNIPPET_MAX).collect();
    format!("{head}\u{2026}")
}
