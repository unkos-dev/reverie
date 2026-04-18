//! Google Books adapter.
//!
//! Endpoint: `GET {base}/volumes?q=<query>&maxResults=<N>`.
//!
//! Without an API key Google caps anonymous traffic at ~1000 req/day across
//! the entire IP — the rate limiter is therefore intentionally conservative
//! (1 req/sec).

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
    L.get_or_init(|| RateLimiter::direct(Quota::per_second(NonZeroU32::new(1).expect("1 > 0"))))
}

pub struct GoogleBooks {
    base_url: String,
    api_key: Option<String>,
}

impl GoogleBooks {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            base_url: base_url.into(),
            api_key,
        }
    }
}

#[async_trait]
impl MetadataSource for GoogleBooks {
    fn id(&self) -> &'static str {
        "googlebooks"
    }

    fn enabled(&self) -> bool {
        true
    }

    async fn lookup(
        &self,
        ctx: &LookupCtx<'_>,
        key: &LookupKey,
    ) -> Result<Vec<SourceResult>, SourceError> {
        while let Err(not_ready) = limiter().check() {
            let wait = not_ready.wait_time_from(DefaultClock::default().now());
            tokio::time::sleep(wait).await;
        }

        let (query, max_results) = match key {
            LookupKey::Isbn(k) => {
                let isbn = k.strip_prefix("isbn:").unwrap_or(k);
                (format!("isbn:{isbn}"), 1_u32)
            }
            LookupKey::TitleAuthor { title, author } => (
                format!(
                    "intitle:{}+inauthor:{}",
                    super::encode_query_component(title),
                    super::encode_query_component(author),
                ),
                5_u32,
            ),
        };

        let mut url = format!(
            "{}/volumes?q={}&maxResults={}",
            self.base_url.trim_end_matches('/'),
            query,
            max_results,
        );
        if let Some(k) = &self.api_key {
            url.push_str("&key=");
            url.push_str(&super::encode_query_component(k));
        }

        let resp = ctx.http.get(&url).send().await.map_err(to_source_error)?;
        let status = resp.status();
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
        let match_type = match key {
            LookupKey::Isbn(_) => "isbn",
            LookupKey::TitleAuthor { .. } => "title_author_fuzzy",
        };
        Ok(map_volumes(&body, match_type))
    }
}

fn to_source_error(e: reqwest::Error) -> SourceError {
    if e.is_timeout() {
        SourceError::Timeout
    } else {
        SourceError::Other(anyhow::Error::from(e))
    }
}

fn map_volumes(body: &Value, match_type: &str) -> Vec<SourceResult> {
    let items = body.get("items").and_then(Value::as_array);
    let Some(items) = items else {
        return Vec::new();
    };
    let Some(first) = items.first() else {
        return Vec::new();
    };
    let info = first.get("volumeInfo").unwrap_or(&Value::Null);

    let mut out = Vec::new();

    if let Some(title) = info.get("title").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "title".into(),
            raw_value: json!(title),
            match_type: match_type.into(),
        });
    }
    if let Some(subtitle) = info.get("subtitle").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "subtitle".into(),
            raw_value: json!(subtitle),
            match_type: match_type.into(),
        });
    }
    if let Some(authors) = info.get("authors").and_then(Value::as_array) {
        let authors: Vec<String> = authors
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        if !authors.is_empty() {
            out.push(SourceResult {
                field_name: "creators".into(),
                raw_value: json!(authors),
                match_type: match_type.into(),
            });
        }
    }
    if let Some(publisher) = info.get("publisher").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "publisher".into(),
            raw_value: json!(publisher),
            match_type: match_type.into(),
        });
    }
    if let Some(published_date) = info.get("publishedDate").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "pub_date".into(),
            raw_value: json!(published_date),
            match_type: match_type.into(),
        });
    }
    if let Some(description) = info.get("description").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "description".into(),
            raw_value: json!(description),
            match_type: match_type.into(),
        });
    }
    if let Some(categories) = info.get("categories").and_then(Value::as_array) {
        let categories: Vec<String> = categories
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        if !categories.is_empty() {
            out.push(SourceResult {
                field_name: "subjects".into(),
                raw_value: json!(categories),
                match_type: match_type.into(),
            });
        }
    }
    if let Some(language) = info.get("language").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "language".into(),
            raw_value: json!(language),
            match_type: match_type.into(),
        });
    }
    if let Some(identifiers) = info.get("industryIdentifiers").and_then(Value::as_array) {
        for id in identifiers {
            let t = id.get("type").and_then(Value::as_str).unwrap_or("");
            let v = id.get("identifier").and_then(Value::as_str).unwrap_or("");
            match t {
                "ISBN_13" => out.push(SourceResult {
                    field_name: "isbn_13".into(),
                    raw_value: json!(v),
                    match_type: match_type.into(),
                }),
                "ISBN_10" => out.push(SourceResult {
                    field_name: "isbn_10".into(),
                    raw_value: json!(v),
                    match_type: match_type.into(),
                }),
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param_contains};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ctx<'a>(http: &'a reqwest::Client) -> LookupCtx<'a> {
        LookupCtx { http, cached: None }
    }

    fn sample_volume() -> serde_json::Value {
        json!({
            "items": [{
                "volumeInfo": {
                    "title": "Dune",
                    "authors": ["Frank Herbert"],
                    "publisher": "Ace",
                    "publishedDate": "1965-08-01",
                    "description": "Desert planet epic.",
                    "categories": ["Fiction", "Science Fiction"],
                    "language": "en",
                    "industryIdentifiers": [
                        {"type": "ISBN_13", "identifier": "9780441172719"},
                        {"type": "ISBN_10", "identifier": "0441172717"}
                    ]
                }
            }]
        })
    }

    #[tokio::test]
    async fn isbn_happy_path() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/volumes"))
            .and(query_param_contains("q", "isbn:9780441172719"))
            .respond_with(ResponseTemplate::new(200).set_body_json(sample_volume()))
            .mount(&server)
            .await;

        let adapter = GoogleBooks::new(server.uri(), None);
        let http = reqwest::Client::new();
        let out = adapter
            .lookup(&ctx(&http), &LookupKey::Isbn("isbn:9780441172719".into()))
            .await
            .unwrap();

        let fields: Vec<&str> = out.iter().map(|r| r.field_name.as_str()).collect();
        assert!(fields.contains(&"title"));
        assert!(fields.contains(&"creators"));
        assert!(fields.contains(&"isbn_13"));
        assert!(fields.contains(&"language"));
    }

    #[tokio::test]
    async fn empty_items_returns_empty() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/volumes"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"totalItems": 0})))
            .mount(&server)
            .await;

        let adapter = GoogleBooks::new(server.uri(), None);
        let http = reqwest::Client::new();
        let out = adapter
            .lookup(&ctx(&http), &LookupKey::Isbn("isbn:0000000000000".into()))
            .await
            .unwrap();
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn rate_limited_returns_with_retry_after() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/volumes"))
            .respond_with(ResponseTemplate::new(429).insert_header("Retry-After", "30"))
            .mount(&server)
            .await;

        let adapter = GoogleBooks::new(server.uri(), None);
        let http = reqwest::Client::new();
        let err = adapter
            .lookup(&ctx(&http), &LookupKey::Isbn("isbn:9780441172719".into()))
            .await
            .unwrap_err();
        match err {
            SourceError::RateLimited { retry_after } => {
                assert_eq!(retry_after, Some(Duration::from_secs(30)));
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }
}
