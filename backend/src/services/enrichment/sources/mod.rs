//! External metadata source adapters.
//!
//! Each adapter implements `MetadataSource` for one provider
//! (Open Library, Google Books, Hardcover).  The orchestrator fans out
//! to every enabled source in parallel and collects `SourceResult`s
//! into the journal.
//!
//! Per-source rate-limiting lives inside each adapter (module-level
//! `governor::RateLimiter`) so one misbehaving provider can't drain
//! another's quota.

/// `Google Books` metadata adapter.
pub mod google_books;
/// `Hardcover` metadata adapter (`GraphQL`-backed; requires an API token).
pub mod hardcover;
/// `OpenLibrary` metadata adapter.
pub mod open_library;

use std::time::Duration;

use async_trait::async_trait;
use reqwest::StatusCode;
use serde_json::Value;
use tracing::warn;

use super::cache::CachedResponse;

/// What we ask a source to look up.
#[derive(Debug, Clone)]
pub enum LookupKey {
    /// ISBN-10 or ISBN-13 in canonical form (from `lookup_key::isbn_key`).
    Isbn(String),
    /// A provider-native identifier from the external-identifier registry.
    /// Dispatch is by `scheme` (the identifier namespace), never by the
    /// observer that recorded the value: a manually entered Google Books id
    /// still routes to the Google adapter.
    ExternalId {
        /// Identifier scheme (`identifier_schemes.id`, e.g. `"googlebooks"`).
        scheme: String,
        /// The canonical identifier value on that scheme.
        value: String,
    },
    /// Fuzzy title+author search when no stronger key is available.
    TitleAuthor {
        /// Book title to search for.
        title: String,
        /// Primary author name to search for.
        author: String,
    },
}

impl LookupKey {
    /// Cache-key form used by `cache`. Every variant is type-prefixed so a
    /// fallback attempt can never cache its result under another key form's
    /// entry (`isbn:` keys come pre-prefixed from `lookup_key::isbn_key`).
    pub fn cache_key(&self) -> String {
        match self {
            Self::Isbn(k) => k.clone(),
            Self::ExternalId { scheme, value } => format!("external:{scheme}:{value}"),
            Self::TitleAuthor { title, author } => {
                format!("ta:{title}|{author}")
            }
        }
    }

    /// Returns the confidence-scoring match type string for this key variant.
    ///
    /// The returned value corresponds to the `match_type` column written by
    /// adapters into `SourceResult` and consumed by `confidence::match_modifier`.
    pub const fn match_type_for(&self) -> &'static str {
        match self {
            Self::Isbn(_) => "isbn",
            Self::ExternalId { .. } => "external_id",
            Self::TitleAuthor { .. } => "title_author_fuzzy",
        }
    }
}

/// Whether identifiers on `scheme` can be resolved by an API adapter.
/// Only these schemes ever become [`LookupKey::ExternalId`] fetch keys;
/// every other scheme (`goodreads`, `asin`, `calibre`, ...) is stored and
/// displayed but never fetched.
pub fn is_fetchable_scheme(scheme: &str) -> bool {
    matches!(scheme, "openlibrary" | "googlebooks" | "hardcover")
}

/// Ratings deliberately travel outside the [`SourceResult`] stream: a
/// `SourceResult` becomes one `metadata_versions` journal row by contract,
/// and ratings are a refreshable direct-write cache that is never journaled,
/// locked, or written back. The type itself lives beside the table whose
/// CHECK constraints it mirrors; it is re-exported here so code that
/// consumes a [`RatingSignal`] can name the signal's payload types from the
/// module that defines it.
pub use crate::models::external_rating::{InvalidRatingObservation, RatingObservation};

/// What one lookup learned about the provider's aggregate rating.
///
/// The distinction between [`Self::Unknown`] and [`Self::Absent`] is
/// load-bearing for cache invalidation: an Open Library works/editions
/// record never carries rating data, so resolving through one says nothing
/// about the provider's rating and must not erase a value cached from an
/// earlier run. A rating-capable record that omits the rating is the
/// provider saying it no longer has one. [`Self::Unusable`] is a third,
/// distinct case: a rating-capable record reported a rating, but the
/// reported values fail [`RatingObservation::new`], so nothing usable came
/// back either.
#[derive(Debug, Clone, Default, PartialEq)]
pub enum RatingSignal {
    /// This lookup path carries no rating data either way; the cached
    /// value (if any) is left untouched.
    #[default]
    Unknown,
    /// A rating-capable record was fetched and reports no rating; any
    /// previously cached rating for this provider is removed.
    Absent,
    /// The provider reports this rating; the cache row is refreshed.
    Reported(RatingObservation),
    /// A rating-capable record reported values the ratings cache's own
    /// invariant rejects; any previously cached rating for this provider is
    /// removed rather than served stale.
    Unusable(InvalidRatingObservation),
}

/// Everything one `lookup` call produced: journal-bound field observations
/// plus a non-journaled rating signal for the ratings cache.
#[derive(Debug, Clone, Default)]
pub struct LookupOutcome {
    /// Field observations; each becomes a candidate journal entry.
    pub fields: Vec<SourceResult>,
    /// What this lookup learned about the provider's aggregate rating.
    pub rating: RatingSignal,
}

impl LookupOutcome {
    /// Build an outcome carrying only field observations, from a path that
    /// says nothing about ratings.
    pub const fn from_fields(fields: Vec<SourceResult>) -> Self {
        Self {
            fields,
            rating: RatingSignal::Unknown,
        }
    }

    /// `true` when the lookup produced neither fields nor a rating signal
    /// worth acting on (a miss for fallback purposes). Both [`Reported`] and
    /// [`Unusable`] count as a hit: an out-of-range rating is still evidence
    /// the record carries rating data, even though [`Unusable`] cannot be
    /// cached as-is.
    ///
    /// [`Reported`]: RatingSignal::Reported
    /// [`Unusable`]: RatingSignal::Unusable
    pub const fn is_empty(&self) -> bool {
        if !self.fields.is_empty() {
            return false;
        }
        // Exhaustive on purpose: a new signal variant must decide its
        // hit-or-miss classification here at compile time, or fallback and
        // cache-kind selection silently diverge from the orchestrator's own
        // handling of the variant.
        match self.rating {
            RatingSignal::Unknown | RatingSignal::Absent => true,
            RatingSignal::Reported(_) | RatingSignal::Unusable(_) => false,
        }
    }
}

/// Build a [`RatingSignal`] from a rating-capable record's optional average
/// and count on the given scale. Shared by the adapters so the numeric
/// coercion lives in one place. Call this only when the fetched record
/// would carry the rating if the provider had one; paths that never carry
/// ratings keep [`RatingSignal::Unknown`].
pub(super) fn rating_signal(average: Option<f64>, count: Option<i64>, scale: f32) -> RatingSignal {
    let Some(avg) = average else {
        return RatingSignal::Absent;
    };
    #[expect(
        clippy::cast_possible_truncation,
        reason = "provider ratings are small values on a 5-point scale; f64→f32 is lossless in that range"
    )]
    let rating = avg as f32;
    // `None` means the provider didn't report a count at all, which is
    // silently 0 (unreported, not invalid). A reported count the cache
    // cannot store, negative or beyond `i32`, is also treated as unreported
    // but earns a log line, so a bogus count alone never invalidates an
    // otherwise valid rating.
    let review_count = count.map_or(0, |n| match i32::try_from(n) {
        Ok(stored) if stored >= 0 => stored,
        _ => {
            warn!(
                reported_count = n,
                "enrichment: provider review count not storable; treating as unreported"
            );
            0
        }
    });
    match RatingObservation::new(rating, scale, review_count) {
        Ok(observation) => RatingSignal::Reported(observation),
        Err(err) => RatingSignal::Unusable(err),
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
    /// The provider returned no record for the requested key (clean miss).
    #[error("not found")]
    NotFound,
    /// The provider signalled quota exhaustion (`HTTP 429`).
    #[error("rate limited")]
    RateLimited {
        /// Suggested retry delay from the `Retry-After` response header, if present.
        retry_after: Option<Duration>,
    },
    /// The provider returned an unexpected non-success `HTTP` status code.
    #[error("http {0}")]
    Http(StatusCode),
    /// The outbound `HTTP` request exceeded the configured timeout.
    #[error("timeout")]
    Timeout,
    /// Any other error (network failure, `JSON` parse error, `GraphQL` error payload).
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Context passed into every `lookup` call.  Non-owning so the orchestrator
/// can share one HTTP client across all adapters.
pub struct LookupCtx<'a> {
    /// Shared `HTTP` client; adapters must not construct their own to avoid bypassing pool limits.
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

    /// Perform a lookup.  Returns an empty [`LookupOutcome`] for a clean
    /// miss, including for a [`LookupKey`] variant the adapter cannot
    /// resolve (a miss lets the orchestrator fall through to the next key).
    ///
    /// The adapter is responsible for:
    /// * rate-limiting itself,
    /// * mapping HTTP + JSON errors to [`SourceError`],
    /// * normalising the payload into one `SourceResult` per field, with any
    ///   provider-reported aggregate rating carried separately in
    ///   [`LookupOutcome::rating`] (never as a field).
    async fn lookup(
        &self,
        ctx: &LookupCtx<'_>,
        key: &LookupKey,
    ) -> Result<LookupOutcome, SourceError>;
}

/// Percent-encode a query-string component using RFC 3986 rules for the
/// reserved characters Reverie actually emits (title/author/key values).
/// Prefer this over per-adapter encoders so we don't drift on which
/// characters each adapter forgets. Full percent-encoding isn't
/// required for the ASCII-dominated query terms Reverie sends.
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
    use super::{
        LookupOutcome, RatingObservation, RatingSignal, encode_query_component, rating_signal,
    };

    #[test]
    fn encode_query_component_escapes_reserved_chars() {
        assert_eq!(encode_query_component("a b"), "a%20b");
        assert_eq!(encode_query_component("C++"), "C%2B%2B");
        assert_eq!(encode_query_component("A&B"), "A%26B");
        assert_eq!(encode_query_component("50% off"), "50%25%20off");
        assert_eq!(encode_query_component("q=1"), "q%3D1");
    }

    #[test]
    fn rating_signal_reports_valid_average() {
        let signal = rating_signal(Some(4.5), Some(100), 5.0);
        let RatingSignal::Reported(observation) = signal else {
            panic!("expected Reported, got {signal:?}");
        };
        assert!((observation.rating() - 4.5).abs() < f32::EPSILON);
        assert!((observation.rating_scale() - 5.0).abs() < f32::EPSILON);
        assert_eq!(observation.review_count(), 100);
    }

    #[test]
    fn rating_signal_absent_when_no_average() {
        assert_eq!(rating_signal(None, Some(10), 5.0), RatingSignal::Absent);
    }

    #[test]
    fn rating_signal_unusable_when_average_out_of_range() {
        let signal = rating_signal(Some(6.0), Some(10), 5.0);
        assert!(
            matches!(signal, RatingSignal::Unusable(_)),
            "expected Unusable, got {signal:?}"
        );
    }

    #[test]
    fn rating_signal_count_above_i32_max_maps_to_zero_but_stays_reported() {
        let signal = rating_signal(Some(4.0), Some(i64::from(i32::MAX) + 1), 5.0);
        let RatingSignal::Reported(observation) = signal else {
            panic!("expected Reported, got {signal:?}");
        };
        assert_eq!(
            observation.review_count(),
            0,
            "an unrepresentable count is dropped, not the whole rating"
        );
    }

    #[test]
    fn rating_signal_negative_count_maps_to_zero_but_stays_reported() {
        let signal = rating_signal(Some(4.0), Some(-1), 5.0);
        let RatingSignal::Reported(observation) = signal else {
            panic!("expected Reported, got {signal:?}");
        };
        assert_eq!(
            observation.review_count(),
            0,
            "a negative count is dropped, not the whole rating"
        );
    }

    #[test]
    fn lookup_outcome_unusable_rating_is_not_empty() {
        let observation_err =
            RatingObservation::new(6.0, 5.0, 1).expect_err("6.0 is out of [0, 5.0]");
        let outcome = LookupOutcome {
            fields: vec![],
            rating: RatingSignal::Unusable(observation_err),
        };
        assert!(
            !outcome.is_empty(),
            "an unusable rating is still evidence the record carries rating data"
        );
    }
}
