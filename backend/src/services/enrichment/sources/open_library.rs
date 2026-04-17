//! Open Library adapter.
//!
//! Endpoints:
//! * ISBN  — `GET {base}/isbn/{isbn}.json`
//! * Search — `GET {base}/search.json?title=...&author=...&limit=5`
//!
//! Rate-limited to a conservative 5 requests per minute at the module level.

#![allow(dead_code)] // wired in Phase C Task 21 (orchestrator)

use std::num::NonZeroU32;
use std::sync::OnceLock;
use std::time::Duration;

use async_trait::async_trait;
use governor::clock::{Clock, DefaultClock};
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use reqwest::StatusCode;
use serde_json::{Value, json};

use super::{LookupCtx, LookupKey, MetadataSource, SourceError, SourceResult};

type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

fn limiter() -> &'static Limiter {
    static L: OnceLock<Limiter> = OnceLock::new();
    L.get_or_init(|| RateLimiter::direct(Quota::per_minute(NonZeroU32::new(5).expect("5 > 0"))))
}

pub struct OpenLibrary {
    base_url: String,
}

impl OpenLibrary {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
        }
    }
}

#[async_trait]
impl MetadataSource for OpenLibrary {
    fn id(&self) -> &'static str {
        "openlibrary"
    }

    fn enabled(&self) -> bool {
        true
    }

    async fn lookup(
        &self,
        ctx: &LookupCtx<'_>,
        key: &LookupKey,
    ) -> Result<Vec<SourceResult>, SourceError> {
        // Rate-limit (non-blocking sleep until a token is available).
        while let Err(not_ready) = limiter().check() {
            let wait = not_ready.wait_time_from(DefaultClock::default().now());
            tokio::time::sleep(wait).await;
        }

        let url = match key {
            LookupKey::Isbn(k) => {
                let isbn = k.strip_prefix("isbn:").unwrap_or(k);
                format!("{}/isbn/{isbn}.json", self.base_url.trim_end_matches('/'))
            }
            LookupKey::TitleAuthor { title, author } => format!(
                "{}/search.json?title={}&author={}&limit=5",
                self.base_url.trim_end_matches('/'),
                urlencoding(title),
                urlencoding(author),
            ),
        };

        let resp = ctx.http.get(&url).send().await.map_err(to_source_error)?;
        let status = resp.status();

        if status == StatusCode::NOT_FOUND {
            return Ok(Vec::new());
        }
        if status == StatusCode::TOO_MANY_REQUESTS {
            let retry_after = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok())
                .map(Duration::from_secs);
            return Err(SourceError::RateLimited { retry_after });
        }
        if !status.is_success() {
            return Err(SourceError::Http(status));
        }

        let body: Value = resp.json().await.map_err(to_source_error)?;
        match key {
            LookupKey::Isbn(_) => Ok(map_isbn_response(&body)),
            LookupKey::TitleAuthor { .. } => Ok(map_search_response(&body)),
        }
    }
}

fn to_source_error(e: reqwest::Error) -> SourceError {
    if e.is_timeout() {
        SourceError::Timeout
    } else {
        SourceError::Other(anyhow::Error::from(e))
    }
}

fn urlencoding(s: &str) -> String {
    // Minimal encoder: replace a handful of reserved chars. Good enough for
    // title/author query params; the full-blown percent-encoding crate would
    // pull in 200+ LoC for zero functional difference.
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('&', "%26")
        .replace('#', "%23")
        .replace('?', "%3F")
        .replace('=', "%3D")
        .replace('+', "%2B")
}

fn map_isbn_response(body: &Value) -> Vec<SourceResult> {
    let mut out = Vec::new();
    let mt = "isbn";

    if let Some(title) = body.get("title").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "title".into(),
            raw_value: json!(title),
            match_type: mt.into(),
        });
    }
    if let Some(subtitle) = body.get("subtitle").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "subtitle".into(),
            raw_value: json!(subtitle),
            match_type: mt.into(),
        });
    }
    if let Some(publishers) = body.get("publishers").and_then(Value::as_array)
        && let Some(first) = publishers.first().and_then(Value::as_str)
    {
        out.push(SourceResult {
            field_name: "publisher".into(),
            raw_value: json!(first),
            match_type: mt.into(),
        });
    }
    if let Some(publish_date) = body.get("publish_date").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "pub_date".into(),
            raw_value: json!(publish_date),
            match_type: mt.into(),
        });
    }
    if let Some(subjects) = body.get("subjects").and_then(Value::as_array) {
        let subjects: Vec<String> = subjects
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        if !subjects.is_empty() {
            out.push(SourceResult {
                field_name: "subjects".into(),
                raw_value: json!(subjects),
                match_type: mt.into(),
            });
        }
    }
    // Open Library ISBN endpoint returns `authors: [{ key: "/authors/OL..." }]`.
    // A second fetch would be needed to resolve the name; skip here — Google
    // Books and Hardcover cover authors.
    if let Some(isbn_13s) = body.get("isbn_13").and_then(Value::as_array)
        && let Some(v) = isbn_13s.first().and_then(Value::as_str)
    {
        out.push(SourceResult {
            field_name: "isbn_13".into(),
            raw_value: json!(v),
            match_type: mt.into(),
        });
    }
    if let Some(isbn_10s) = body.get("isbn_10").and_then(Value::as_array)
        && let Some(v) = isbn_10s.first().and_then(Value::as_str)
    {
        out.push(SourceResult {
            field_name: "isbn_10".into(),
            raw_value: json!(v),
            match_type: mt.into(),
        });
    }
    if let Some(description) = body.get("description") {
        // `description` may be a plain string or `{ "value": "..." }`.
        let text = description.as_str().map(str::to_owned).or_else(|| {
            description
                .get("value")
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
        if let Some(text) = text {
            out.push(SourceResult {
                field_name: "description".into(),
                raw_value: json!(text),
                match_type: mt.into(),
            });
        }
    }
    out
}

fn map_search_response(body: &Value) -> Vec<SourceResult> {
    let mt = "title_author_fuzzy";
    let doc = body
        .get("docs")
        .and_then(Value::as_array)
        .and_then(|docs| docs.first());
    let Some(doc) = doc else {
        return Vec::new();
    };
    let mut out = Vec::new();

    if let Some(title) = doc.get("title").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "title".into(),
            raw_value: json!(title),
            match_type: mt.into(),
        });
    }
    if let Some(authors) = doc.get("author_name").and_then(Value::as_array) {
        let authors: Vec<String> = authors
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        if !authors.is_empty() {
            out.push(SourceResult {
                field_name: "creators".into(),
                raw_value: json!(authors),
                match_type: mt.into(),
            });
        }
    }
    if let Some(first_pub) = doc.get("first_publish_year").and_then(Value::as_i64) {
        out.push(SourceResult {
            field_name: "pub_date".into(),
            raw_value: json!(format!("{first_pub:04}")),
            match_type: mt.into(),
        });
    }
    if let Some(subjects) = doc.get("subject").and_then(Value::as_array) {
        let subjects: Vec<String> = subjects
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .take(10)
            .collect();
        if !subjects.is_empty() {
            out.push(SourceResult {
                field_name: "subjects".into(),
                raw_value: json!(subjects),
                match_type: mt.into(),
            });
        }
    }
    if let Some(isbns) = doc.get("isbn").and_then(Value::as_array) {
        for v in isbns {
            let Some(s) = v.as_str() else { continue };
            let field = if s.len() == 13 { "isbn_13" } else { "isbn_10" };
            out.push(SourceResult {
                field_name: field.into(),
                raw_value: json!(s),
                match_type: mt.into(),
            });
            break; // take the first ISBN only
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ctx<'a>(http: &'a reqwest::Client) -> LookupCtx<'a> {
        LookupCtx { http, cached: None }
    }

    #[tokio::test]
    async fn isbn_happy_path_maps_fields() {
        let server = MockServer::start().await;
        let body = json!({
            "title": "Dune",
            "subtitle": "A Novel",
            "publishers": ["Ace"],
            "publish_date": "1965",
            "subjects": ["Science Fiction"],
            "isbn_13": ["9780441172719"],
            "isbn_10": ["0441172717"],
            "description": {"value": "Desert planet epic."}
        });
        Mock::given(method("GET"))
            .and(path("/isbn/9780441172719.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let adapter = OpenLibrary::new(server.uri());
        let http = reqwest::Client::new();
        let out = adapter
            .lookup(&ctx(&http), &LookupKey::Isbn("isbn:9780441172719".into()))
            .await
            .unwrap();

        let fields: Vec<&str> = out.iter().map(|r| r.field_name.as_str()).collect();
        assert!(fields.contains(&"title"));
        assert!(fields.contains(&"subtitle"));
        assert!(fields.contains(&"publisher"));
        assert!(fields.contains(&"isbn_13"));
        assert!(fields.contains(&"description"));
    }

    #[tokio::test]
    async fn isbn_404_is_clean_empty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/isbn/0000000000000.json"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let adapter = OpenLibrary::new(server.uri());
        let http = reqwest::Client::new();
        let out = adapter
            .lookup(&ctx(&http), &LookupKey::Isbn("isbn:0000000000000".into()))
            .await
            .unwrap();
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn isbn_429_maps_to_rate_limited_with_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/isbn/9780441172719.json"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "60"))
            .mount(&server)
            .await;

        let adapter = OpenLibrary::new(server.uri());
        let http = reqwest::Client::new();
        let err = adapter
            .lookup(&ctx(&http), &LookupKey::Isbn("isbn:9780441172719".into()))
            .await
            .unwrap_err();
        match err {
            SourceError::RateLimited { retry_after } => {
                assert_eq!(retry_after, Some(Duration::from_secs(60)));
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn isbn_500_maps_to_http_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/isbn/9780441172719.json"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let adapter = OpenLibrary::new(server.uri());
        let http = reqwest::Client::new();
        let err = adapter
            .lookup(&ctx(&http), &LookupKey::Isbn("isbn:9780441172719".into()))
            .await
            .unwrap_err();
        assert!(matches!(
            err,
            SourceError::Http(StatusCode::INTERNAL_SERVER_ERROR)
        ));
    }
}
