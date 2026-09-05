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

use std::num::NonZeroU32;
use std::sync::OnceLock;
use std::time::Duration;

use async_trait::async_trait;
use governor::clock::{Clock, DefaultClock};
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};
use reqwest::StatusCode;
use serde_json::{Value, json};

use super::{LookupCtx, LookupKey, LookupOutcome, MetadataSource, SourceError, SourceResult};

type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

fn limiter() -> &'static Limiter {
    static L: OnceLock<Limiter> = OnceLock::new();
    L.get_or_init(|| {
        #[expect(
            clippy::expect_used,
            reason = "NonZeroU32::new(1) — the literal 1 is a compile-time constant that is always non-zero; this cannot fail"
        )]
        RateLimiter::direct(Quota::per_second(NonZeroU32::new(1).expect("1 > 0")))
    })
}

const BOOK_FIELDS: &str = r"
    id
    slug
    title
    subtitle
    description
    release_date
    rating
    ratings_count
    language { code3 }
    publisher { name }
    contributions { author { name } contribution }
    isbns { isbn type }
    cached_tags
";

/// Compose one of the fixed query shapes with the shared field selection.
/// The id/slug/isbn/title/author values are always bound as typed GraphQL
/// variables against these fixed documents, never interpolated into the
/// query text.
fn query_doc(head: &str) -> String {
    format!("{head} {{ {BOOK_FIELDS} }} }}")
}

fn isbn_query() -> String {
    query_doc(
        "query BooksByIsbn($isbn: String!) { books(where: { isbns: { isbn: { _eq: $isbn } } }, limit: 1)",
    )
}

fn title_author_query() -> String {
    query_doc(
        "query SearchByTitleAuthor($title: String!, $author: String!) { books(where: { title: { _ilike: $title }, contributions: { author: { name: { _ilike: $author } } } }, limit: 1)",
    )
}

fn book_by_id_query() -> String {
    query_doc("query BookById($id: Int!) { books(where: { id: { _eq: $id } }, limit: 1)")
}

fn book_by_slug_query() -> String {
    query_doc("query BookBySlug($slug: String!) { books(where: { slug: { _eq: $slug } }, limit: 1)")
}

/// `Hardcover` metadata adapter (`GraphQL`-backed).
///
/// Issues `GraphQL` queries against the `Hardcover` API to retrieve
/// bibliographic data. The adapter is disabled when no API token is configured;
/// see [`MetadataSource::enabled`].
pub struct Hardcover {
    base_url: String,
    token: Option<String>,
}

impl Hardcover {
    /// Creates a new `Hardcover` adapter targeting `base_url`.
    ///
    /// `token` is a `Hardcover` API bearer token. When `None`, [`MetadataSource::enabled`]
    /// returns `false` and `lookup` returns an empty result without hitting the network.
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
    ) -> Result<LookupOutcome, SourceError> {
        let Some(token) = self.token.as_deref() else {
            return Ok(LookupOutcome::default());
        };

        while let Err(not_ready) = limiter().check() {
            let wait = not_ready.wait_time_from(DefaultClock::default().now());
            tokio::time::sleep(wait).await;
        }

        let (query, variables, match_type) = match key {
            LookupKey::Isbn(k) => {
                let isbn = k.strip_prefix("isbn:").unwrap_or(k).to_string();
                (isbn_query(), json!({"isbn": isbn}), "isbn")
            }
            LookupKey::ExternalId { scheme, value } => {
                if scheme != "hardcover" {
                    return Ok(LookupOutcome::default());
                }
                // Numeric ids bind as Int, slugs as String; either way the
                // value is a typed GraphQL variable against a fixed query
                // document, never spliced into the query text.
                value.parse::<i64>().map_or_else(
                    |_| (book_by_slug_query(), json!({"slug": value}), "external_id"),
                    |id| (book_by_id_query(), json!({"id": id}), "external_id"),
                )
            }
            LookupKey::TitleAuthor { title, author } => {
                // Strip existing '%' so an incoming value of "%" or long
                // wildcard runs can't coerce Hardcover into a full-table
                // LIKE scan.  Require at least 3 post-strip characters;
                // shorter queries produce too much noise to be useful.
                let t = sanitise_ilike_term(title);
                let a = sanitise_ilike_term(author);
                if t.chars().count() < 3 || a.chars().count() < 3 {
                    return Ok(LookupOutcome::default());
                }
                (
                    title_author_query(),
                    json!({"title": format!("%{t}%"), "author": format!("%{a}%")}),
                    "title_author_fuzzy",
                )
            }
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
        Ok(book.map_or_else(LookupOutcome::default, |b| map_book(b, match_type)))
    }
}

/// Remove ILIKE wildcards and trim whitespace.  Hardcover's GraphQL
/// `_ilike` filter treats `%` as match-anything; a user-controlled
/// value like `%` on its own matches every row in the table and burns
/// Hardcover's quota.  Stripping wildcards keeps the caller in control
/// of what each term matches.
fn sanitise_ilike_term(s: &str) -> String {
    s.replace(['%', '_'], "").trim().to_string()
}

fn to_source_error(e: reqwest::Error) -> SourceError {
    if e.is_timeout() {
        SourceError::Timeout
    } else {
        SourceError::Other(anyhow::Error::from(e))
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "map_book maps 10+ Hardcover API fields to SourceResults; the per-field cases are mechanical and extracting would obscure the API→model mapping"
)]
fn map_book(book: &Value, match_type: &str) -> LookupOutcome {
    let mut out = Vec::new();

    // The book's own Hardcover id: prefer the stable slug, fall back to the
    // numeric id. Hardcover models a "book" at the work level.
    let own_id = book
        .get("slug")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            book.get("id")
                .and_then(Value::as_i64)
                .map(|n| n.to_string())
        });
    if let Some(id) = own_id {
        out.push(SourceResult {
            field_name: "identifiers.work.hardcover".into(),
            raw_value: json!(id),
            match_type: match_type.into(),
        });
    }

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
                field_name: "contributors.author".into(),
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

    // Hardcover reports ratings on a 5-point scale.
    let rating = super::rating_signal(
        book.get("rating").and_then(Value::as_f64),
        book.get("ratings_count").and_then(Value::as_i64),
        5.0,
    );

    LookupOutcome {
        fields: out,
        rating,
    }
}

#[cfg(test)]
mod tests {
    #![expect(
        clippy::disallowed_methods,
        reason = "bare reqwest::Client::new() against wiremock on loopback is ADR-exempt (docs/adr/0007-outbound-http-clients-send-an-explicit-user-agent.md): wiremock does not score User-Agents and no WAF sits in the path"
    )]
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, method};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ctx(http: &reqwest::Client) -> LookupCtx<'_> {
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

        let fields: Vec<&str> = out.fields.iter().map(|r| r.field_name.as_str()).collect();
        assert!(fields.contains(&"title"));
        assert!(fields.contains(&"contributors.author"));
        assert!(fields.contains(&"isbn_13"));
        assert!(fields.contains(&"language"));
        assert!(fields.contains(&"publisher"));
    }

    #[tokio::test]
    async fn book_id_lookup_binds_typed_int_variable() {
        let server = MockServer::start().await;
        let body = json!({
            "data": {
                "books": [{
                    "id": 431,
                    "slug": "dune",
                    "title": "Dune",
                    "rating": 4.31,
                    "ratings_count": 999
                }]
            }
        });
        // The id must arrive as a typed GraphQL variable, not inside the
        // query text.
        Mock::given(method("POST"))
            .and(body_partial_json(json!({ "variables": { "id": 431 } })))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let adapter = Hardcover::new(server.uri(), Some("test-token".into()));
        let http = reqwest::Client::new();
        let out = adapter
            .lookup(
                &ctx(&http),
                &LookupKey::ExternalId {
                    scheme: "hardcover".into(),
                    value: "431".into(),
                },
            )
            .await
            .unwrap();

        let title = out.fields.iter().find(|r| r.field_name == "title").unwrap();
        assert_eq!(title.match_type, "external_id");
        let own_id = out
            .fields
            .iter()
            .find(|r| r.field_name == "identifiers.work.hardcover")
            .expect("book emits its own hardcover id");
        assert_eq!(own_id.raw_value, json!("dune"), "slug preferred over id");

        let crate::services::enrichment::sources::RatingSignal::Reported(rating) = out.rating
        else {
            panic!("rating maps to a reported rating, got {:?}", out.rating);
        };
        assert!((rating.rating() - 4.31).abs() < 1e-6);
        assert_eq!(rating.review_count(), 999);
    }

    #[tokio::test]
    async fn book_slug_lookup_binds_typed_string_variable() {
        let server = MockServer::start().await;
        let body = json!({ "data": { "books": [{ "id": 431, "slug": "dune", "title": "Dune" }] } });
        Mock::given(method("POST"))
            .and(body_partial_json(
                json!({ "variables": { "slug": "dune" } }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(&body))
            .mount(&server)
            .await;

        let adapter = Hardcover::new(server.uri(), Some("test-token".into()));
        let http = reqwest::Client::new();
        let out = adapter
            .lookup(
                &ctx(&http),
                &LookupKey::ExternalId {
                    scheme: "hardcover".into(),
                    value: "dune".into(),
                },
            )
            .await
            .unwrap();
        assert!(out.fields.iter().any(|r| r.field_name == "title"));
    }

    #[tokio::test]
    async fn foreign_scheme_is_clean_miss_without_network() {
        let adapter = Hardcover::new("http://127.0.0.1:9", Some("test-token".into()));
        let http = reqwest::Client::new();
        let out = adapter
            .lookup(
                &ctx(&http),
                &LookupKey::ExternalId {
                    scheme: "googlebooks".into(),
                    value: "zyTZAAAAYAAJ".into(),
                },
            )
            .await
            .unwrap();
        assert!(out.is_empty());
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
