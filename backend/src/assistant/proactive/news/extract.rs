//! Fetch an article URL and extract its main text. Guards: http(s) only,
//! timeout (via the shared client), and a response-size cap. Any failure → None.

const MAX_BYTES: usize = 2_000_000;

/// Extract readable main text from an HTML string at `url`. Returns None when
/// extraction yields nothing usable.
pub fn extract_html(html: &str, url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let product = readability::extractor::extract(&mut html.as_bytes(), &parsed).ok()?;
    let text = product.text.trim();
    if text.len() < 200 { None } else { Some(text.to_string()) }
}

/// Fetch `url` and extract main text. Rejects non-http(s) and oversized bodies.
pub async fn fetch_main_text(client: &reqwest::Client, url: &str) -> Option<String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    let resp = client.get(url).send().await.ok()?.error_for_status().ok()?;
    let bytes = resp.bytes().await.ok()?;
    if bytes.len() > MAX_BYTES {
        tracing::warn!("news: {url} body too large ({} bytes), skipping", bytes.len());
        return None;
    }
    let html = String::from_utf8_lossy(&bytes);
    extract_html(&html, url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_returns_none_for_thin_content() {
        assert!(extract_html("<html><body><p>hi</p></body></html>", "https://ex.com/a").is_none());
    }

    #[test]
    fn extract_pulls_paragraph_text() {
        let body = format!("<html><body><article><p>{}</p></article></body></html>", "lorem ipsum ".repeat(40));
        let out = extract_html(&body, "https://ex.com/a");
        assert!(out.is_some());
        assert!(out.unwrap().contains("lorem ipsum"));
    }

    #[test]
    fn fetch_rejects_non_http() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = reqwest::Client::new();
        assert!(rt.block_on(fetch_main_text(&client, "ftp://ex.com/a")).is_none());
    }
}
