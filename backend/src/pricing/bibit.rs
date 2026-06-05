use super::PriceError;
use rust_decimal::Decimal;
use std::str::FromStr;

/// A fund NAV quote: price per unit in IDR and the NAV date printed on the page.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NavQuote {
    pub price: Decimal,
    pub as_of: String, // "YYYY-MM-DD" from productDetail.nav.date
}

/// Extract the NAV from a Bibit product page. The page is Next.js
/// server-rendered: full fund data sits in a `<script id="__NEXT_DATA__">`
/// JSON blob at `props.pageProps.productDetail.nav.{value,date}`.
#[allow(dead_code)]
pub fn parse_nav(html: &str) -> Result<NavQuote, PriceError> {
    let marker = "<script id=\"__NEXT_DATA__\"";
    let start = html.find(marker).ok_or_else(|| PriceError::Parse("__NEXT_DATA__ script not found".into()))?;
    let body = &html[start..];
    let open = body.find('>').ok_or_else(|| PriceError::Parse("__NEXT_DATA__ tag malformed".into()))?;
    let body = &body[open + 1..];
    let end = body.find("</script>").ok_or_else(|| PriceError::Parse("__NEXT_DATA__ not closed".into()))?;
    let v: serde_json::Value = serde_json::from_str(&body[..end])
        .map_err(|e| PriceError::Parse(format!("__NEXT_DATA__ not valid JSON: {e}")))?;
    let nav = v.pointer("/props/pageProps/productDetail/nav")
        .ok_or_else(|| PriceError::Parse("productDetail.nav missing".into()))?;
    let as_of = nav.get("date").and_then(|d| d.as_str())
        .ok_or_else(|| PriceError::Parse("nav.date missing".into()))?
        .to_string();
    // Go through the raw JSON number's string form, not f64, to avoid float artifacts.
    let raw = nav.get("value").ok_or_else(|| PriceError::Parse("nav.value missing".into()))?;
    let price = Decimal::from_str(&raw.to_string())
        .map_err(|e| PriceError::Parse(format!("nav.value not a decimal: {e}")))?;
    if price <= Decimal::ZERO {
        return Err(PriceError::Parse(format!("nav.value not positive: {price}")));
    }
    Ok(NavQuote { price, as_of })
}

/// Fetches NAV from Bibit's public product page (no auth; desktop UA).
#[allow(dead_code)]
pub struct BibitNav {
    base: String,
    client: reqwest::Client,
}

#[allow(dead_code)]
impl BibitNav {
    pub fn new() -> Self {
        Self { base: "https://bibit.id/reksadana".into(), client: reqwest::Client::new() }
    }

    /// `code` is the fund's RDCODE, e.g. "RD1436". The slug segment after the
    /// code is cosmetic — any value routes.
    pub async fn latest(&self, code: &str) -> Result<NavQuote, PriceError> {
        let url = format!("{}/{}/x", self.base, code);
        let resp = self.client.get(&url)
            .header("User-Agent", "Mozilla/5.0")
            .send().await
            .map_err(|e| PriceError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(PriceError::Http(format!("status {} for {code}", resp.status())));
        }
        let html = resp.text().await.map_err(|e| PriceError::Http(e.to_string()))?;
        parse_nav(&html)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    /// Trimmed to the real shape of bibit.id/reksadana/RD1436 (captured 2026-06-05).
    fn page(nav_json: &str) -> String {
        format!(
            "<html><head><title>x</title></head><body><div id=\"app\">...</div>\
             <script id=\"__NEXT_DATA__\" type=\"application/json\">\
             {{\"props\":{{\"pageProps\":{{\"productDetail\":{{\"symbol\":\"RD1436\",\
             \"name\":\"Sucorinvest Bond Fund\",\"type\":\"Obligasi\",{nav_json}}}}}}},\
             \"page\":\"/reksadana/[id]/[slug]\"}}</script></body></html>"
        )
    }

    #[test]
    fn parses_nav_value_and_date() {
        let html = page("\"nav\":{\"date\":\"2026-06-04\",\"first_date\":\"2016-12-08\",\"value\":1697.22}");
        let q = parse_nav(&html).unwrap();
        assert_eq!(q.price, dec!(1697.22));
        assert_eq!(q.as_of, "2026-06-04");
    }

    #[test]
    fn missing_nav_is_parse_error() {
        let html = page("\"aum\":{\"value\":1.0}");
        assert!(matches!(parse_nav(&html), Err(PriceError::Parse(_))));
    }

    #[test]
    fn garbage_html_is_parse_error() {
        assert!(matches!(parse_nav("<html>nope</html>"), Err(PriceError::Parse(_))));
        assert!(matches!(parse_nav(""), Err(PriceError::Parse(_))));
    }

    #[test]
    fn non_positive_nav_is_parse_error() {
        let html = page("\"nav\":{\"date\":\"2026-06-04\",\"value\":0}");
        assert!(matches!(parse_nav(&html), Err(PriceError::Parse(_))));
    }

    #[test]
    fn missing_date_is_parse_error() {
        let html = page("\"nav\":{\"value\":1697.22}");
        assert!(matches!(parse_nav(&html), Err(PriceError::Parse(_))));
    }
}
