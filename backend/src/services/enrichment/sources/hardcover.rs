//! Hardcover adapter (GraphQL).
//!
//! POST to a single GraphQL endpoint with a Bearer token.  The adapter
//! reports `enabled() == false` when the token is missing so the orchestrator
//! can skip it entirely.
//!
//! Hardcover's schema is evolving; the queries below hit the conservatively-
//! stable `books_by_isbn(isbn: String!)` and `books(where: ..., limit: N)`
//! shapes documented at hardcover.app/api.  The orchestrator treats any
//! GraphQL error as [`SourceError::Other`].

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
    L.get_or_init(|| RateLimiter::direct(Quota::per_second(NonZeroU32::new(1).expect("1 > 0"))))
}

const ISBN_QUERY: &str = r#"
query BooksByIsbn($isbn: String!) {
  books(where: { isbns: { isbn: { _eq: $isbn } } }, limit: 1) {
    title
    subtitle
    description
    release_date
    language { code3 }
    publisher { name }
    contributions { author { name } contribution }
    isbns { isbn type }
    cached_tags
  }
}
"#;

const TITLE_AUTHOR_QUERY: &str = r#"
query SearchByTitleAuthor($title: String!, $author: String!) {
  books(where: { title: { _ilike: $title }, contributions: { author: { name: { _ilike: $author } } } }, limit: 1) {
    title
    subtitle
    description
    release_date
    language { code3 }
    publisher { name }
    contributions { author { name } contribution }
    isbns { isbn type }
    cached_tags
  }
}
"#;

pub struct Hardcover {
    base_url: String,
    token: Option<String>,
}

impl Hardcover {
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Self {
        Self {
            base_url: base_url.into(),
            token,
        }
    }
}

#[async_trait]
impl MetadataSource for Hardcover {
    fn id(&self) -> &'static str {
        "hardcover"
    }

    fn enabled(&self) -> bool {
        self.token.is_some()
    }

    async fn lookup(
        &self,
        ctx: &LookupCtx<'_>,
        key: &LookupKey,
    ) -> Result<Vec<SourceResult>, SourceError> {
        let Some(token) = self.token.as_deref() else {
            return Ok(Vec::new());
        };

        while let Err(not_ready) = limiter().check() {
            let wait = not_ready.wait_time_from(DefaultClock::default().now());
            tokio::time::sleep(wait).await;
        }

        let (query, variables, match_type) = match key {
            LookupKey::Isbn(k) => {
                let isbn = k.strip_prefix("isbn:").unwrap_or(k).to_string();
                (ISBN_QUERY, json!({"isbn": isbn}), "isbn")
            }
            LookupKey::TitleAuthor { title, author } => (
                TITLE_AUTHOR_QUERY,
                json!({"title": format!("%{title}%"), "author": format!("%{author}%")}),
                "title_author_fuzzy",
            ),
        };

        let payload = json!({ "query": query, "variables": variables });
        let resp = ctx
            .http
            .post(&self.base_url)
            .bearer_auth(token)
            .json(&payload)
            .send()
            .await
            .map_err(to_source_error)?;

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

        if let Some(errors) = body.get("errors").and_then(Value::as_array)
            && !errors.is_empty()
        {
            return Err(SourceError::Other(anyhow::anyhow!(
                "graphql errors: {errors:?}"
            )));
        }

        let book = body
            .get("data")
            .and_then(|d| d.get("books"))
            .and_then(Value::as_array)
            .and_then(|xs| xs.first());
        Ok(match book {
            Some(b) => map_book(b, match_type),
            None => Vec::new(),
        })
    }
}

fn to_source_error(e: reqwest::Error) -> SourceError {
    if e.is_timeout() {
        SourceError::Timeout
    } else {
        SourceError::Other(anyhow::Error::from(e))
    }
}

fn map_book(book: &Value, match_type: &str) -> Vec<SourceResult> {
    let mut out = Vec::new();

    if let Some(title) = book.get("title").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "title".into(),
            raw_value: json!(title),
            match_type: match_type.into(),
        });
    }
    if let Some(subtitle) = book.get("subtitle").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "subtitle".into(),
            raw_value: json!(subtitle),
            match_type: match_type.into(),
        });
    }
    if let Some(desc) = book.get("description").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "description".into(),
            raw_value: json!(desc),
            match_type: match_type.into(),
        });
    }
    if let Some(release_date) = book.get("release_date").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "pub_date".into(),
            raw_value: json!(release_date),
            match_type: match_type.into(),
        });
    }
    if let Some(lang) = book
        .get("language")
        .and_then(|l| l.get("code3"))
        .and_then(Value::as_str)
    {
        out.push(SourceResult {
            field_name: "language".into(),
            raw_value: json!(lang),
            match_type: match_type.into(),
        });
    }
    if let Some(pub_name) = book
        .get("publisher")
        .and_then(|p| p.get("name"))
        .and_then(Value::as_str)
    {
        out.push(SourceResult {
            field_name: "publisher".into(),
            raw_value: json!(pub_name),
            match_type: match_type.into(),
        });
    }
    if let Some(contributions) = book.get("contributions").and_then(Value::as_array) {
        let authors: Vec<String> = contributions
            .iter()
            .filter_map(|c| {
                let is_author = c
                    .get("contribution")
                    .and_then(Value::as_str)
                    .is_none_or(|s| s.eq_ignore_ascii_case("author"));
                if !is_author {
                    return None;
                }
                c.get("author")
                    .and_then(|a| a.get("name"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .collect();
        if !authors.is_empty() {
            out.push(SourceResult {
                field_name: "creators".into(),
                raw_value: json!(authors),
                match_type: match_type.into(),
            });
        }
    }
    if let Some(isbns) = book.get("isbns").and_then(Value::as_array) {
        for entry in isbns {
            let t = entry.get("type").and_then(Value::as_str).unwrap_or("");
            let v = entry.get("isbn").and_then(Value::as_str).unwrap_or("");
            if v.is_empty() {
                continue;
            }
            match t {
                "ISBN-13" | "isbn_13" => out.push(SourceResult {
                    field_name: "isbn_13".into(),
                    raw_value: json!(v),
                    match_type: match_type.into(),
                }),
                "ISBN-10" | "isbn_10" => out.push(SourceResult {
                    field_name: "isbn_10".into(),
                    raw_value: json!(v),
                    match_type: match_type.into(),
                }),
                _ => {}
            }
        }
    }
    if let Some(tags) = book.get("cached_tags").and_then(Value::as_array) {
        let tags: Vec<String> = tags
            .iter()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        if !tags.is_empty() {
            out.push(SourceResult {
                field_name: "subjects".into(),
                raw_value: json!(tags),
                match_type: match_type.into(),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ctx<'a>(http: &'a reqwest::Client) -> LookupCtx<'a> {
        LookupCtx { http, cached: None }
    }

    #[test]
    fn adapter_disabled_without_token() {
        let adapter = Hardcover::new("https://example.com", None);
        assert!(!adapter.enabled());
    }

    #[tokio::test]
    async fn missing_token_yields_empty() {
        let adapter = Hardcover::new("https://example.com", None);
        let http = reqwest::Client::new();
        let out = adapter
            .lookup(&ctx(&http), &LookupKey::Isbn("isbn:9780441172719".into()))
            .await
            .unwrap();
        assert!(out.is_empty());
    }

    #[tokio::test]
    async fn graphql_happy_path() {
        let server = MockServer::start().await;
        let body = json!({
            "data": {
                "books": [{
                    "title": "Dune",
                    "description": "Desert planet epic.",
                    "release_date": "1965-08-01",
                    "language": {"code3": "eng"},
                    "publisher": {"name": "Ace"},
                    "contributions": [
                        {"contribution": "author", "author": {"name": "Frank Herbert"}}
                    ],
                    "isbns": [{"type": "ISBN-13", "isbn": "9780441172719"}],
                    "cached_tags": ["Science Fiction"]
                }]
            }
        });
        Mock::given(method("POST"))
            .and(body_partial_json(
                json!({ "variables": { "isbn": "9780441172719" } }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let adapter = Hardcover::new(server.uri(), Some("test-token".into()));
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
        assert!(fields.contains(&"publisher"));
    }

    #[tokio::test]
    async fn graphql_errors_surface_as_other() {
        let server = MockServer::start().await;
        let body = json!({
            "errors": [{"message": "bad auth"}]
        });
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let adapter = Hardcover::new(server.uri(), Some("test-token".into()));
        let http = reqwest::Client::new();
        let err = adapter
            .lookup(&ctx(&http), &LookupKey::Isbn("isbn:9780441172719".into()))
            .await
            .unwrap_err();
        assert!(matches!(err, SourceError::Other(_)));
    }
}
