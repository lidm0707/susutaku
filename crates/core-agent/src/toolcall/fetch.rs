//! Fetch a web page and reduce it to plain text for model context.

pub const USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/124.0 Safari/537.36";
/// Context-size guard: pages are reduced to at most this many chars.
pub const TEXT_MAX: usize = 4000;
const HTTP_PREFIX: &str = "http://";
const HTTPS_PREFIX: &str = "https://";

/// GET a URL and return readable text (tags stripped, scripts/styles dropped).
pub fn fetch(url: &str) -> Result<String, String> {
    if !url.starts_with(HTTP_PREFIX) && !url.starts_with(HTTPS_PREFIX) {
        return Err("only http(s) urls are supported".to_string());
    }
    let body = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    Ok(to_text(&body))
}

/// Reduce HTML to plain text: drop script/style, strip tags, unescape, collapse.
pub fn to_text(html: &str) -> String {
    let without_scripts = remove_blocks(html, "<script", "</script>");
    let without_styles = remove_blocks(&without_scripts, "<style", "</style>");
    let text = strip_tags(&without_styles);
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(&collapsed, TEXT_MAX)
}

fn remove_blocks(html: &str, open: &str, close: &str) -> String {
    let mut lower = html.to_string();
    lower.make_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    let mut lower_rest = lower.as_str();
    while let Some(start) = lower_rest.find(open) {
        out.push_str(&rest[..start]);
        let Some(end_rel) = lower_rest[start..].find(close) else {
            return out;
        };
        let end = start + end_rel + close.len();
        rest = &rest[end..];
        lower_rest = &lower_rest[end..];
    }
    out.push_str(rest);
    out
}

pub(crate) fn strip_tags(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    let collapsed = out.split_whitespace().collect::<Vec<_>>().join(" ");
    unescape(&collapsed)
}

pub(crate) fn unescape(text: &str) -> String {
    text.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ")
}

pub fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    format!("{cut}\u{2026}")
}
