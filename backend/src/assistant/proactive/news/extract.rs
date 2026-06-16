//! Fetch an article URL and extract its main text + OpenGraph metadata.
//! Guards: http(s) only, timeout (via the shared client), response-size cap.
//! Any failure yields an empty `Extracted` (never panics).

const MAX_BYTES: usize = 2_000_000;

/// What we pull from an article page. All fields are best-effort.
#[derive(Debug, Default, Clone)]
pub struct Extracted {
    /// Readability main text, only when it's substantial (>= 200 chars).
    pub text: Option<String>,
    /// OpenGraph/meta description — a decent fallback when `text` is absent.
    pub og_description: Option<String>,
    /// OpenGraph image URL for a thumbnail.
    pub image_url: Option<String>,
}

/// First non-empty `content` of a `<meta property|name="{key}">` tag.
fn meta_content(doc: &scraper::Html, key: &str) -> Option<String> {
    for attr in ["property", "name"] {
        let sel = scraper::Selector::parse(&format!(r#"meta[{attr}="{key}"]"#)).ok()?;
        if let Some(el) = doc.select(&sel).next() {
            if let Some(c) = el.value().attr("content") {
                let c = c.trim();
                if !c.is_empty() {
                    return Some(c.to_string());
                }
            }
        }
    }
    None
}

/// Extract readable main text (>=200 chars) + OG image/description from HTML.
pub fn extract_html(html: &str, url: &str) -> Extracted {
    let mut out = Extracted::default();
    if let Ok(parsed) = url::Url::parse(url) {
        if let Ok(product) = readability::extractor::extract(&mut html.as_bytes(), &parsed) {
            let t = product.text.trim();
            if t.len() >= 200 {
                out.text = Some(t.to_string());
            }
        }
    }
    let doc = scraper::Html::parse_document(html);
    out.og_description =
        meta_content(&doc, "og:description").or_else(|| meta_content(&doc, "description"));
    out.image_url = meta_content(&doc, "og:image");
    out
}

/// Fetch `url` and extract. Rejects non-http(s)/oversized bodies → empty Extracted.
pub async fn fetch_article(client: &reqwest::Client, url: &str) -> Extracted {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Extracted::default();
    }
    let Ok(resp) = client.get(url).send().await.and_then(|r| r.error_for_status()) else {
        return Extracted::default();
    };
    let Ok(bytes) = resp.bytes().await else { return Extracted::default() };
    if bytes.len() > MAX_BYTES {
        tracing::warn!("news: {url} body too large ({} bytes), skipping", bytes.len());
        return Extracted::default();
    }
    let html = String::from_utf8_lossy(&bytes);
    extract_html(&html, url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_main_text_for_rich_html() {
        let body = format!("<html><body><article><p>{}</p></article></body></html>", "lorem ipsum ".repeat(40));
        let ex = extract_html(&body, "https://ex.com/a");
        assert!(ex.text.is_some());
        assert!(ex.text.unwrap().contains("lorem ipsum"));
    }

    #[test]
    fn extracts_og_image_and_description() {
        let html = r#"<html><head>
          <meta property="og:image" content="https://ex.com/img.png">
          <meta property="og:description" content="Deskripsi singkat artikel.">
        </head><body><p>hi</p></body></html>"#;
        let ex = extract_html(html, "https://ex.com/a");
        assert_eq!(ex.image_url.as_deref(), Some("https://ex.com/img.png"));
        assert_eq!(ex.og_description.as_deref(), Some("Deskripsi singkat artikel."));
        assert!(ex.text.is_none()); // thin body
    }

    #[test]
    fn fetch_rejects_non_http() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = reqwest::Client::new();
        let ex = rt.block_on(fetch_article(&client, "ftp://ex.com/a"));
        assert!(ex.text.is_none() && ex.image_url.is_none());
    }
}
