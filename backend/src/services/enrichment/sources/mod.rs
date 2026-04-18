//! External metadata source adapters.
//!
//! Each adapter implements [`MetadataSource`] for one provider
//! (Open Library, Google Books, Hardcover).  The orchestrator fans out
//! to every enabled source in parallel and collects [`SourceResult`]s
//! into the journal.
//!
//! Per-source rate-limiting lives inside each adapter (module-level
//! `governor::RateLimiter`) so one misbehaving provider can't drain
//! another's quota.

#![allow(dead_code)] // wired in Phase C Task 21 (orchestrator)

pub mod google_books;
pub mod hardcover;
pub mod open_library;

use std::time::Duration;

use async_trait::async_trait;
use reqwest::StatusCode;
use serde_json::Value;

use super::cache::CachedResponse;

/// What we ask a source to look up.
#[derive(Debug, Clone)]
pub enum LookupKey {
    /// ISBN-10 or ISBN-13 in canonical form (from `lookup_key::isbn_key`).
    Isbn(String),
    /// Fuzzy title+author search when ISBN is unavailable.
    TitleAuthor { title: String, author: String },
}

impl LookupKey {
    /// Cache-key form used by [`super::cache`].
    pub fn cache_key(&self) -> String {
        match self {
            LookupKey::Isbn(k) => k.clone(),
            LookupKey::TitleAuthor { title, author } => {
                format!("ta:{title}|{author}")
            }
        }
    }

    pub fn match_type_for(&self) -> &'static str {
        match self {
            LookupKey::Isbn(_) => "isbn",
            LookupKey::TitleAuthor { .. } => "title_author_fuzzy",
        }
    }
}

/// A single field observation from a metadata source.
///
/// Each row the adapter emits becomes one candidate journal entry in the
/// enrichment pipeline.
#[derive(Debug, Clone)]
pub struct SourceResult {
    /// Canonical field name (`"title"`, `"description"`, `"isbn_13"`, ...).
    pub field_name: String,
    /// Raw JSON value to store in `metadata_versions.new_value`.
    pub raw_value: Value,
    /// Used by `confidence::match_modifier`.  One of `"isbn"`,
    /// `"title_author_exact"`, `"title_author_fuzzy"`, `"title"`.
    pub match_type: String,
}

/// Failure modes shared across adapters.
#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("not found")]
    NotFound,
    #[error("rate limited")]
    RateLimited { retry_after: Option<Duration> },
    #[error("http {0}")]
    Http(StatusCode),
    #[error("timeout")]
    Timeout,
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Context passed into every `lookup` call.  Non-owning so the orchestrator
/// can share one HTTP client across all adapters.
pub struct LookupCtx<'a> {
    pub http: &'a reqwest::Client,
    /// Optional cache hit supplied by the caller.  If present, the adapter
    /// MAY return its results directly from the cached payload instead of
    /// hitting the network; the orchestrator passes `None` when a miss is
    /// desired.
    pub cached: Option<&'a CachedResponse>,
}

/// One external metadata provider.
///
/// Adapters must be `Send + Sync` so the orchestrator can fan out across a
/// `join_all`.  Rate-limit bookkeeping is implementation-internal.
#[async_trait]
pub trait MetadataSource: Send + Sync {
    /// Stable ID used for `metadata_sources.id` / `metadata_versions.source`.
    fn id(&self) -> &'static str;

    /// Whether this source is configured and should be queried.
    /// e.g. Hardcover returns `false` when no API token is set.
    fn enabled(&self) -> bool;

    /// Perform a lookup.  Returns `Ok(vec![])` for a clean miss.
    ///
    /// The adapter is responsible for:
    /// * rate-limiting itself,
    /// * mapping HTTP + JSON errors to [`SourceError`],
    /// * normalising the payload into one [`SourceResult`] per field.
    async fn lookup(
        &self,
        ctx: &LookupCtx<'_>,
        key: &LookupKey,
    ) -> Result<Vec<SourceResult>, SourceError>;
}

/// Percent-encode a query-string component using RFC 3986 rules for the
/// reserved characters Tome actually emits (title/author/key values).
/// Prefer this over per-adapter encoders so we don't drift on which
/// characters each adapter forgets. Full percent-encoding isn't
/// required for the ASCII-dominated query terms Tome sends.
pub(super) fn encode_query_component(s: &str) -> String {
    s.replace('%', "%25")
        .replace(' ', "%20")
        .replace('&', "%26")
        .replace('#', "%23")
        .replace('?', "%3F")
        .replace('=', "%3D")
        .replace('+', "%2B")
}

#[cfg(test)]
mod tests {
    use super::encode_query_component;

    #[test]
    fn encode_query_component_escapes_reserved_chars() {
        assert_eq!(encode_query_component("a b"), "a%20b");
        assert_eq!(encode_query_component("C++"), "C%2B%2B");
        assert_eq!(encode_query_component("A&B"), "A%26B");
        assert_eq!(encode_query_component("50% off"), "50%25%20off");
        assert_eq!(encode_query_component("q=1"), "q%3D1");
    }
}
