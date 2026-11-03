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
//! Rate-limited to 3 requests per second — `OpenLibrary`'s identified-request
//! tier, unlocked by the `User-Agent` set in `api_client`.

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
            reason = "NonZeroU32::new(3) — the literal 3 is a compile-time constant that is always non-zero; this cannot fail"
        )]
        RateLimiter::direct(Quota::per_second(NonZeroU32::new(3).expect("3 > 0")))
    })
}

/// The official `OpenLibrary` API base URL; the only supported endpoint.
pub const DEFAULT_BASE_URL: &str = "https://openlibrary.org";

/// `OpenLibrary` metadata adapter.
///
/// Uses two `OpenLibrary` endpoints: the `ISBN` lookup path (`/api/books`)
/// which returns inline author names without a second hop, and the full-text
/// search path (`/search.json`) for title+author queries.
pub struct OpenLibrary {
    base_url: String,
}

impl OpenLibrary {
    /// Creates a new `OpenLibrary` adapter targeting `base_url`.
    ///
    /// The adapter is always enabled; no API credentials are required.
    /// Rate limits are enforced internally at 3 req/sec, matching the
    /// `OpenLibrary` identified-request tier.
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
    ) -> Result<LookupOutcome, SourceError> {
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
            LookupKey::ExternalId { scheme, value } => {
                if scheme != "openlibrary" {
                    return Ok(LookupOutcome::default());
                }
                // The id travels as a structural path segment: works ids
                // (`OL…W`) resolve on /works, edition ids (`OL…M`) on
                // /books. Never string-concatenated into the path.
                let collection = if value.ends_with('W') {
                    "works"
                } else {
                    "books"
                };
                let mut url = reqwest::Url::parse(&self.base_url)
                    .map_err(|e| SourceError::Other(anyhow::anyhow!("invalid base url: {e}")))?;
                url.path_segments_mut()
                    .map_err(|()| {
                        SourceError::Other(anyhow::anyhow!("base url cannot hold a path"))
                    })?
                    .pop_if_empty()
                    .push(collection)
                    .push(&format!("{value}.json"));
                url.to_string()
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
        match key {
            LookupKey::Isbn(k) => {
                let isbn = k.strip_prefix("isbn:").unwrap_or(k);
                let bibkey = format!("ISBN:{isbn}");
                Ok(LookupOutcome::from_fields(map_api_books_response(
                    &body, &bibkey,
                )))
            }
            LookupKey::ExternalId { value, .. } => {
                if value.ends_with('W') {
                    Ok(LookupOutcome::from_fields(map_work_response(&body)))
                } else {
                    Ok(LookupOutcome::from_fields(map_edition_response(&body)))
                }
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
/// `OpenLibrary`'s behaviour for unknown ISBNs on this endpoint.
#[expect(
    clippy::too_many_lines,
    reason = "map_api_books_response maps 10+ data-view fields to observations; the per-field cases are mechanical and extracting would obscure the API-to-model mapping"
)]
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
                field_name: "contributors.author".into(),
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
        push_cross_provider_ids(ids, mt, &mut out);
    }

    // The record's own edition id ("key": "/books/OL…M").
    if let Some(olid) = entry
        .get("key")
        .and_then(Value::as_str)
        .and_then(|k| k.strip_prefix("/books/"))
        && olid.starts_with("OL")
        && olid.ends_with('M')
    {
        out.push(SourceResult {
            field_name: "identifiers.manifestation.openlibrary".into(),
            raw_value: json!(olid),
            match_type: mt.into(),
        });
    }

    out
}

/// Cross-provider ids piggyback on the data view's `identifiers` object:
/// goodreads and librarything ids are stored + displayed but never fetched.
/// Only clean numeric values are emitted; anything else is dropped here
/// rather than journaled and rejected at apply time.
fn push_cross_provider_ids(ids: &Value, mt: &str, out: &mut Vec<SourceResult>) {
    for (ol_key, field) in [
        ("goodreads", "identifiers.manifestation.goodreads"),
        ("librarything", "identifiers.manifestation.librarything"),
    ] {
        if let Some(v) = ids
            .get(ol_key)
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(Value::as_str)
            && !v.is_empty()
            && v.bytes().all(|b| b.is_ascii_digit())
        {
            out.push(SourceResult {
                field_name: field.into(),
                raw_value: json!(v),
                match_type: mt.into(),
            });
        }
    }
}

/// Parse a `/works/{OL…W}.json` work record fetched by native id.
fn map_work_response(body: &Value) -> Vec<SourceResult> {
    let mt = "external_id";
    let mut out = Vec::new();

    if let Some(title) = body.get("title").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "title".into(),
            raw_value: json!(title),
            match_type: mt.into(),
        });
    }
    // `description` is either a bare string or `{"type": ..., "value": s}`.
    let description = match body.get("description") {
        Some(Value::String(s)) => Some(s.as_str()),
        Some(obj) => obj.get("value").and_then(Value::as_str),
        None => None,
    };
    if let Some(desc) = description {
        out.push(SourceResult {
            field_name: "description".into(),
            raw_value: json!(desc),
            match_type: mt.into(),
        });
    }
    if let Some(subjects) = body.get("subjects").and_then(Value::as_array) {
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
    out
}

/// Parse a `/books/{OL…M}.json` edition record fetched by native id.
///
/// The raw edition endpoint returns author references as `/authors/OL…A`
/// keys without names, so no contributor observation is emitted from this
/// path (resolving names would need a second hop).
fn map_edition_response(body: &Value) -> Vec<SourceResult> {
    let mt = "external_id";
    let mut out = Vec::new();

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
    if let Some(publisher) = body
        .get("publishers")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(Value::as_str)
    {
        out.push(SourceResult {
            field_name: "publisher".into(),
            raw_value: json!(publisher),
            match_type: mt.into(),
        });
    }
    if let Some(pub_date) = body.get("publish_date").and_then(Value::as_str) {
        out.push(SourceResult {
            field_name: "pub_date".into(),
            raw_value: json!(pub_date),
            match_type: mt.into(),
        });
    }
    if let Some(pages) = body.get("number_of_pages").and_then(Value::as_i64) {
        out.push(SourceResult {
            field_name: "pages".into(),
            raw_value: json!(pages),
            match_type: mt.into(),
        });
    }
    for (field, ol_key) in [("isbn_13", "isbn_13"), ("isbn_10", "isbn_10")] {
        if let Some(v) = body
            .get(ol_key)
            .and_then(Value::as_array)
            .and_then(|arr| arr.first())
            .and_then(Value::as_str)
        {
            out.push(SourceResult {
                field_name: field.into(),
                raw_value: json!(v),
                match_type: mt.into(),
            });
        }
    }
    // Link the edition to its parent work's OL id so the work-level slot
    // can fill from an edition lookup.
    if let Some(work_key) = body
        .get("works")
        .and_then(Value::as_array)
        .and_then(|arr| arr.first())
        .and_then(|w| w.get("key"))
        .and_then(Value::as_str)
        .and_then(|k| k.strip_prefix("/works/"))
        && work_key.starts_with("OL")
        && work_key.ends_with('W')
    {
        out.push(SourceResult {
            field_name: "identifiers.work.openlibrary".into(),
            raw_value: json!(work_key),
            match_type: mt.into(),
        });
    }
    out
}

fn map_search_response(body: &Value) -> LookupOutcome {
    let mt = "title_author_fuzzy";
    let doc = body
        .get("docs")
        .and_then(Value::as_array)
        .and_then(|docs| docs.first());
    let Some(doc) = doc else {
        return LookupOutcome::default();
    };
    let mut out = Vec::new();

    // The matched work's own OL id ("key": "/works/OL…W").
    if let Some(work_key) = doc
        .get("key")
        .and_then(Value::as_str)
        .and_then(|k| k.strip_prefix("/works/"))
        && work_key.starts_with("OL")
        && work_key.ends_with('W')
    {
        out.push(SourceResult {
            field_name: "identifiers.work.openlibrary".into(),
            raw_value: json!(work_key),
            match_type: mt.into(),
        });
    }

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
                field_name: "contributors.author".into(),
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

    // Search docs carry the work-level aggregate rating on a 5-point scale.
    let rating = super::rating_signal(
        doc.get("ratings_average").and_then(Value::as_f64),
        doc.get("ratings_count").and_then(Value::as_i64),
        5.0,
    );

    LookupOutcome {
        fields: out,
        rating,
    }
}
