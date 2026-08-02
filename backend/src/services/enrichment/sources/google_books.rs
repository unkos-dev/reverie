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

/// `Google Books` metadata adapter.
///
/// Queries the `Google Books` Volumes `API` to retrieve bibliographic data.
/// An optional API key lifts the anonymous per-IP quota (~1 000 req/day);
/// without it the adapter remains functional but is rate-limited conservatively.
pub struct GoogleBooks {
    base_url: String,
    api_key: Option<String>,
}

impl GoogleBooks {
    /// Creates a new `GoogleBooks` adapter targeting `base_url`.
    ///
    /// `api_key` is optional; when `None` the adapter uses anonymous access,
    /// subject to the shared IP-level quota enforced by `Google`.
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
    ) -> Result<LookupOutcome, SourceError> {
        while let Err(not_ready) = limiter().check() {
            let wait = not_ready.wait_time_from(DefaultClock::default().now());
            tokio::time::sleep(wait).await;
        }

        // Native volume-id lookups fetch one Volume resource directly; the
        // id travels as a structural path segment, never string-concatenated
        // into the URL.
        if let LookupKey::ExternalId { scheme, value } = key {
            if scheme != "googlebooks" {
                return Ok(LookupOutcome::default());
            }
            return self.lookup_volume_by_id(ctx, value).await;
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
            LookupKey::ExternalId { .. } => unreachable!("handled above"),
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
        let match_type = key.match_type_for();
        let first = body
            .get("items")
            .and_then(Value::as_array)
            .and_then(|items| items.first());
        Ok(first.map_or_else(LookupOutcome::default, |volume| {
            map_volume(volume, match_type)
        }))
    }
}

impl GoogleBooks {
    /// `GET {base}/volumes/{id}` — direct Volume fetch by native id.
    /// A 404 is a clean miss so the orchestrator can fall through to the
    /// next eligible key.
    async fn lookup_volume_by_id(
        &self,
        ctx: &LookupCtx<'_>,
        id: &str,
    ) -> Result<LookupOutcome, SourceError> {
        let mut url = reqwest::Url::parse(&self.base_url)
            .map_err(|e| SourceError::Other(anyhow::anyhow!("invalid base url: {e}")))?;
        url.path_segments_mut()
            .map_err(|()| SourceError::Other(anyhow::anyhow!("base url cannot hold a path")))?
            .pop_if_empty()
            .push("volumes")
            .push(id);
        if let Some(k) = &self.api_key {
            url.query_pairs_mut().append_pair("key", k);
        }

        let resp = ctx
            .http
            .get(url.as_str())
            .send()
            .await
            .map_err(to_source_error)?;
        let status = resp.status();
        if status == StatusCode::NOT_FOUND {
            return Ok(LookupOutcome::default());
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
        Ok(map_volume(&body, "external_id"))
    }
}

fn to_source_error(e: reqwest::Error) -> SourceError {
    if e.is_timeout() {
        SourceError::Timeout
    } else {
        SourceError::Other(anyhow::Error::from(e))
    }
}

/// Map one Volume resource (an `items[]` element or a direct `/volumes/{id}`
/// body) into field observations, its native volume id, and any inline
/// aggregate rating. `averageRating` is per-Volume (edition-level), which is
/// exactly the granularity the manifestation-keyed ratings cache stores.
#[expect(
    clippy::too_many_lines,
    reason = "map_volume maps 10+ Volume fields to observations; the per-field cases are mechanical and extracting would obscure the API-to-model mapping"
)]
fn map_volume(volume: &Value, match_type: &str) -> LookupOutcome {
    let info = volume.get("volumeInfo").unwrap_or(&Value::Null);

    let mut out = Vec::new();

    if let Some(volume_id) = volume.get("id").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "identifiers.manifestation.googlebooks".into(),
            raw_value: json!(volume_id),
            match_type: match_type.into(),
        });
    }

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
                field_name: "contributors.author".into(),
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

    // Google reports ratings on a 5-point scale.
    let rating = super::rating_signal(
        info.get("averageRating").and_then(Value::as_f64),
        info.get("ratingsCount").and_then(Value::as_i64),
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
        reason = "bare reqwest::Client::new() against wiremock on loopback is ADR-exempt (adr/2026-05-18-outbound-http-user-agent.md): wiremock does not score User-Agents and no WAF sits in the path"
    )]
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param_contains};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn ctx(http: &reqwest::Client) -> LookupCtx<'_> {
        LookupCtx { http, cached: None }
    }

    fn sample_volume_info() -> serde_json::Value {
        json!({
            "title": "Dune",
            "authors": ["Frank Herbert"],
            "publisher": "Ace",
            "publishedDate": "1965-08-01",
            "description": "Desert planet epic.",
            "categories": ["Fiction", "Science Fiction"],
            "language": "en",
            "averageRating": 4.5,
            "ratingsCount": 1234,
            "industryIdentifiers": [
                {"type": "ISBN_13", "identifier": "9780441172719"},
                {"type": "ISBN_10", "identifier": "0441172717"}
            ]
        })
    }

    fn sample_volume() -> serde_json::Value {
        json!({
            "items": [{
                "id": "zyTZAAAAYAAJ",
                "volumeInfo": sample_volume_info()
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

        let fields: Vec<&str> = out.fields.iter().map(|r| r.field_name.as_str()).collect();
        assert!(fields.contains(&"title"));
        assert!(fields.contains(&"contributors.author"));
        assert!(fields.contains(&"isbn_13"));
        assert!(fields.contains(&"language"));
        assert!(fields.contains(&"identifiers.manifestation.googlebooks"));

        let crate::services::enrichment::sources::RatingSignal::Reported(rating) = out.rating
        else {
            panic!(
                "averageRating maps to a reported rating, got {:?}",
                out.rating
            );
        };
        assert!((rating.rating() - 4.5).abs() < f32::EPSILON);
        assert!((rating.rating_scale() - 5.0).abs() < f32::EPSILON);
        assert_eq!(rating.review_count(), 1234);
    }

    #[tokio::test]
    async fn volume_id_lookup_uses_structural_path_and_maps() {
        let server = MockServer::start().await;
        // The volume id must arrive as its own path segment; a query-string
        // or concatenated form would not match this path expectation.
        Mock::given(method("GET"))
            .and(path("/volumes/zyTZAAAAYAAJ"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "zyTZAAAAYAAJ",
                "volumeInfo": sample_volume_info()
            })))
            .mount(&server)
            .await;

        let adapter = GoogleBooks::new(server.uri(), None);
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

        let title = out.fields.iter().find(|r| r.field_name == "title").unwrap();
        assert_eq!(title.raw_value, json!("Dune"));
        assert_eq!(title.match_type, "external_id");
        assert!(matches!(
            out.rating,
            crate::services::enrichment::sources::RatingSignal::Reported(_)
        ));
    }

    #[tokio::test]
    async fn volume_id_404_is_clean_miss() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/volumes/unknownVOL"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&server)
            .await;

        let adapter = GoogleBooks::new(server.uri(), None);
        let http = reqwest::Client::new();
        let out = adapter
            .lookup(
                &ctx(&http),
                &LookupKey::ExternalId {
                    scheme: "googlebooks".into(),
                    value: "unknownVOL".into(),
                },
            )
            .await
            .unwrap();
        assert!(out.is_empty(), "404 on a native id must be a clean miss");
    }

    #[tokio::test]
    async fn volume_without_rating_signals_absent() {
        // A Volume record is rating-capable: fetching one that omits
        // averageRating means the provider has no rating, which must clear
        // a previously cached value rather than leave it stale.
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/volumes/noRatingVol"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id": "noRatingVol",
                "volumeInfo": {"title": "Unrated"}
            })))
            .mount(&server)
            .await;

        let adapter = GoogleBooks::new(server.uri(), None);
        let http = reqwest::Client::new();
        let out = adapter
            .lookup(
                &ctx(&http),
                &LookupKey::ExternalId {
                    scheme: "googlebooks".into(),
                    value: "noRatingVol".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(
            out.rating,
            crate::services::enrichment::sources::RatingSignal::Absent
        );
    }

    #[tokio::test]
    async fn foreign_scheme_is_clean_miss_without_network() {
        // No mock server at all: a non-googlebooks scheme must return empty
        // without issuing a request (a request would error and fail the test).
        let adapter = GoogleBooks::new("http://127.0.0.1:9", None);
        let http = reqwest::Client::new();
        let out = adapter
            .lookup(
                &ctx(&http),
                &LookupKey::ExternalId {
                    scheme: "openlibrary".into(),
                    value: "OL7353617M".into(),
                },
            )
            .await
            .unwrap();
        assert!(out.is_empty());
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
