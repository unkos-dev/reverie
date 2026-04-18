//! Open Library adapter.
//!
//! Endpoints:
//! * ISBN  — `GET {base}/api/books?bibkeys=ISBN:{isbn}&jscmd=data&format=json`
//! * Search — `GET {base}/search.json?title=...&author=...&limit=5`
//!
//! The ISBN path uses the humanised `jscmd=data` view so authors arrive as
//! inline names (the older `/isbn/{isbn}.json` endpoint only returned
//! `/authors/OL...` keys and required a second hop).
//!
//! Rate-limited to 3 requests per second — OpenLibrary's identified-request
//! tier, unlocked by the `User-Agent` set in [`super::super::http::api_client`].

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
    L.get_or_init(|| RateLimiter::direct(Quota::per_second(NonZeroU32::new(3).expect("3 > 0"))))
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
                format!(
                    "{}/api/books?bibkeys=ISBN:{isbn}&jscmd=data&format=json",
                    self.base_url.trim_end_matches('/'),
                )
            }
            LookupKey::TitleAuthor { title, author } => format!(
                "{}/search.json?title={}&author={}&limit=5",
                self.base_url.trim_end_matches('/'),
                super::encode_query_component(title),
                super::encode_query_component(author),
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
            LookupKey::Isbn(k) => {
                let isbn = k.strip_prefix("isbn:").unwrap_or(k);
                let bibkey = format!("ISBN:{isbn}");
                Ok(map_api_books_response(&body, &bibkey))
            }
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

/// Parse an `/api/books?bibkeys=ISBN:X&jscmd=data` response.
///
/// The response is a map keyed by bibkey (e.g. `"ISBN:9780441172719"`).  A
/// missing key is treated as a clean miss (empty vec), matching
/// OpenLibrary's behaviour for unknown ISBNs on this endpoint.
fn map_api_books_response(body: &Value, isbn_key: &str) -> Vec<SourceResult> {
    let mut out = Vec::new();
    let mt = "isbn";

    let Some(entry) = body.get(isbn_key) else {
        return out;
    };

    if let Some(title) = entry.get("title").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "title".into(),
            raw_value: json!(title),
            match_type: mt.into(),
        });
    }

    if let Some(authors) = entry.get("authors").and_then(Value::as_array) {
        let names: Vec<String> = authors
            .iter()
            .filter_map(|a| a.get("name").and_then(Value::as_str).map(str::to_owned))
            .collect();
        if !names.is_empty() {
            out.push(SourceResult {
                field_name: "creators".into(),
                raw_value: json!(names),
                match_type: mt.into(),
            });
        }
    }

    if let Some(publishers) = entry.get("publishers").and_then(Value::as_array)
        && let Some(name) = publishers
            .first()
            .and_then(|p| p.get("name"))
            .and_then(Value::as_str)
    {
        out.push(SourceResult {
            field_name: "publisher".into(),
            raw_value: json!(name),
            match_type: mt.into(),
        });
    }

    if let Some(pub_date) = entry.get("publish_date").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "pub_date".into(),
            raw_value: json!(pub_date),
            match_type: mt.into(),
        });
    }

    if let Some(subjects) = entry.get("subjects").and_then(Value::as_array) {
        let names: Vec<String> = subjects
            .iter()
            .filter_map(|s| s.get("name").and_then(Value::as_str).map(str::to_owned))
            .collect();
        if !names.is_empty() {
            out.push(SourceResult {
                field_name: "subjects".into(),
                raw_value: json!(names),
                match_type: mt.into(),
            });
        }
    }

    if let Some(cover) = entry.get("cover") {
        // Prefer the largest available size.  Skip empty strings.
        for size in ["large", "medium", "small"] {
            if let Some(url) = cover.get(size).and_then(Value::as_str)
                && !url.is_empty()
            {
                out.push(SourceResult {
                    field_name: "cover_url".into(),
                    raw_value: json!(url),
                    match_type: mt.into(),
                });
                break;
            }
        }
    }

    if let Some(ids) = entry.get("identifiers") {
        if let Some(isbn_13) = ids
            .get("isbn_13")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(Value::as_str)
        {
            out.push(SourceResult {
                field_name: "isbn_13".into(),
                raw_value: json!(isbn_13),
                match_type: mt.into(),
            });
        }
        if let Some(isbn_10) = ids
            .get("isbn_10")
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(Value::as_str)
        {
            out.push(SourceResult {
                field_name: "isbn_10".into(),
                raw_value: json!(isbn_10),
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
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ctx<'a>(http: &'a reqwest::Client) -> LookupCtx<'a> {
        LookupCtx { http, cached: None }
    }

    // ── Unit tests for the pure parser ───────────────────────────────────

    #[test]
    fn map_api_books_response_happy_emits_full_field_set() {
        let body = json!({
            "ISBN:9780441172719": {
                "title": "Dune",
                "authors": [
                    {"url": "https://openlibrary.org/authors/OL1A/Frank_Herbert",
                     "name": "Frank Herbert"}
                ],
                "publishers": [{"name": "Ace"}],
                "publish_date": "June 1, 1990",
                "subjects": [{"name": "Science Fiction", "url": "x"}],
                "cover": {
                    "small": "https://covers.openlibrary.org/b/id/1-S.jpg",
                    "medium": "https://covers.openlibrary.org/b/id/1-M.jpg",
                    "large": "https://covers.openlibrary.org/b/id/1-L.jpg"
                },
                "identifiers": {
                    "isbn_10": ["0441172717"],
                    "isbn_13": ["9780441172719"]
                }
            }
        });
        let out = map_api_books_response(&body, "ISBN:9780441172719");
        let fields: Vec<&str> = out.iter().map(|r| r.field_name.as_str()).collect();
        assert!(fields.contains(&"title"));
        assert!(fields.contains(&"creators"));
        assert!(fields.contains(&"publisher"));
        assert!(fields.contains(&"pub_date"));
        assert!(fields.contains(&"subjects"));
        assert!(fields.contains(&"cover_url"));
        assert!(fields.contains(&"isbn_10"));
        assert!(fields.contains(&"isbn_13"));

        // Cover prefers the largest size.
        let cover = out.iter().find(|r| r.field_name == "cover_url").unwrap();
        assert_eq!(
            cover.raw_value,
            json!("https://covers.openlibrary.org/b/id/1-L.jpg")
        );
    }

    #[test]
    fn map_api_books_response_missing_key_is_clean_miss() {
        let body = json!({});
        let out = map_api_books_response(&body, "ISBN:0000000000000");
        assert!(out.is_empty());
    }

    #[test]
    fn map_api_books_response_partial_returns_only_present_fields() {
        let body = json!({
            "ISBN:9780441172719": {
                "title": "Dune"
            }
        });
        let out = map_api_books_response(&body, "ISBN:9780441172719");
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].field_name, "title");
    }

    #[test]
    fn map_api_books_response_skips_author_without_name() {
        let body = json!({
            "ISBN:9780441172719": {
                "authors": [
                    {"url": "https://openlibrary.org/authors/OL1A"},
                    {"name": "Frank Herbert"}
                ]
            }
        });
        let out = map_api_books_response(&body, "ISBN:9780441172719");
        let creators = out.iter().find(|r| r.field_name == "creators").unwrap();
        assert_eq!(creators.raw_value, json!(["Frank Herbert"]));
    }

    #[test]
    fn map_api_books_response_skips_empty_cover_urls() {
        let body = json!({
            "ISBN:9780441172719": {
                "cover": {"small": "", "medium": "", "large": ""}
            }
        });
        let out = map_api_books_response(&body, "ISBN:9780441172719");
        assert!(out.iter().all(|r| r.field_name != "cover_url"));
    }

    // ── Wiremock integration tests ───────────────────────────────────────

    fn api_books_body(isbn: &str, title: &str) -> serde_json::Value {
        json!({
            format!("ISBN:{isbn}"): {
                "title": title,
                "authors": [{"name": "Frank Herbert", "url": "x"}],
                "publishers": [{"name": "Ace"}],
                "publish_date": "1965",
                "subjects": [{"name": "Science Fiction", "url": "y"}],
                "identifiers": {"isbn_10": ["0441172717"], "isbn_13": [isbn]}
            }
        })
    }

    #[tokio::test]
    async fn isbn_happy_path_hits_api_books_and_maps_fields() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/books"))
            .and(query_param("bibkeys", "ISBN:9780441172719"))
            .and(query_param("jscmd", "data"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(api_books_body("9780441172719", "Dune")),
            )
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
        assert!(fields.contains(&"creators"));
        assert!(fields.contains(&"publisher"));
        assert!(fields.contains(&"isbn_13"));
    }

    #[tokio::test]
    async fn isbn_missing_key_is_clean_empty() {
        let server = MockServer::start().await;
        // OpenLibrary responds 200 with `{}` when the ISBN is unknown on
        // the `/api/books` endpoint (no per-ISBN entry in the map).
        Mock::given(method("GET"))
            .and(path("/api/books"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
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
    async fn isbn_404_is_clean_empty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/books"))
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
            .and(path("/api/books"))
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
            .and(path("/api/books"))
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
