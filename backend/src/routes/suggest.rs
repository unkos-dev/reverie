//! `/api/v1/{genres,moods,tags,authors,series,publishers}/suggest` typeahead endpoints.
//!
//! Each is a prefix-then-similarity lookup over the metadata vocabularies,
//! scoped to values already in use on a manifestation the caller can see.
//!
//! # Invariants
//! - Same RLS seam as [`crate::routes::library`]:
//!   [`crate::db::acquire_with_rls`] sets `app.current_user_id` for the
//!   transaction, and every scope subquery below reaches the vocabulary
//!   through a join onto `manifestations`, so a child account's shelf-gated
//!   visibility narrows suggestions exactly like it narrows `/api/v1/books`.
//! - "In use" means linked to at least one manifestation row the caller can
//!   see; a vocabulary row with no linking rows (an orphaned `genres`/`moods`/
//!   `tags` entry, or a `series`/`authors` row with no visible manifestation)
//!   never surfaces, so the palette only offers values worth reusing.
//! - Two static query shapes per entity, chosen by input length: under three
//!   characters, trigram matching is unreliable (too few three-character
//!   windows), so only the `ILIKE` prefix leg runs; three characters or more
//!   adds a `word_similarity` fuzzy leg for typo tolerance.
//! - `q` is matched case-insensitively via `ILIKE`, and a caller cannot
//!   smuggle wildcard metacharacters into the prefix match: the raw input is
//!   bound as-is and `immutable_unaccent_like` backslash-escapes `%`/`_`/`\`
//!   in SQL, after accent folding. The order is load-bearing: the unaccent
//!   dictionary folds some Unicode punctuation (fullwidth `％`/`＿`/`＼`)
//!   into those very metacharacters, so an escape applied before folding
//!   would be undone by it. Postgres's default `LIKE`/`ILIKE` escape
//!   character is already backslash, so no explicit `ESCAPE` clause is
//!   needed. A needle the dictionary erases entirely (combining marks fold
//!   to the empty string) nulls the pattern via `nullif` and matches
//!   nothing rather than everything.
//! - Every match is also accent-insensitive: the vocabulary column is
//!   wrapped in `immutable_unaccent` and the bound query text folds through
//!   the same dictionary (the `ILIKE` prefix leg via
//!   `immutable_unaccent_like`, the `word_similarity`/`<%` fuzzy leg via
//!   `immutable_unaccent`), so `garcia` finds `García` without the caller
//!   stripping diacritics itself. Stored values are untouched; folding
//!   happens only at match time.

use crate::services::reader::{SeriesLabel, Suggestion};
use axum::Json;
use axum::extract::State;
use axum_extra::extract::{Query, QueryRejection};
use serde::{Deserialize, Serialize};
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;
use uuid::Uuid;

use crate::auth::middleware::CurrentUser;
use crate::error::AppError;
use crate::state::AppState;

/// Build the `/api/v1/*/suggest` router as an [`OpenApiRouter`] so each
/// handler's `#[utoipa::path]` contributes to the generated spec. Merged
/// into `crate::openapi::pilot_router`.
pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(suggest_genres))
        .routes(routes!(suggest_moods))
        .routes(routes!(suggest_tags))
        .routes(routes!(suggest_authors))
        .routes(routes!(suggest_series))
        .routes(routes!(suggest_publishers))
        .routes(routes!(authors_by_id))
        .routes(routes!(series_by_id))
}

/// `?q=` / `?limit=` query parameters shared by every suggest endpoint.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct SuggestParams {
    /// Free-form typeahead text. Trimmed before matching; empty after
    /// trimming, or longer than 100 characters, is rejected with 422.
    q: String,
    /// Maximum rows returned. Clamped to 1..=20 (default 10); never rejects.
    #[serde(default)]
    limit: Option<i64>,
}

/// `GET .../suggest` response envelope.
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct SuggestResponse {
    suggestions: Vec<Suggestion>,
}

/// `?id=` repeated query parameter for the authors batch-get. Required by
/// construction: an absent or empty `id` is a 422, never an unbounded list.
#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct AuthorsByIdParams {
    /// Author ids to resolve, repeated (`?id=<uuid>&id=<uuid>`). Required
    /// (empty is 422) and capped at 60; unknown or
    /// out-of-scope ids are silently omitted rather than 404'd.
    #[serde(default)]
    #[param(max_items = 60, max_length = 45)]
    id: Vec<Uuid>,
}

/// `GET /api/v1/authors` response envelope: the resolved author labels for
/// the requested ids, as a plain collection.
#[derive(Debug, Serialize, utoipa::ToSchema)]
struct AuthorsResponse {
    items: Vec<Suggestion>,
}

#[derive(Debug, Deserialize, utoipa::IntoParams)]
#[into_params(parameter_in = Query)]
struct SeriesByIdParams {
    #[serde(default)]
    #[param(max_items = 60, max_length = 45)]
    id: Vec<Uuid>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
struct SeriesResponse {
    items: Vec<SeriesLabel>,
}

/// `GET /api/v1/genres/suggest`: genre names in use on a visible
/// manifestation, ranked prefix-first then by similarity.
///
/// Read-scope gated exactly like `GET /api/v1/books` (the least-privilege
/// floor); every other suggest handler below shares the same shape.
///
/// # Errors
/// - [`AppError::MalformedQuery`] when `?limit=` fails to parse as an
///   integer; HTTP 400 via the `From<QueryRejection>` impl.
/// - [`AppError::Validation`] when `q` is empty after trimming or exceeds
///   100 characters.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/genres/suggest",
    summary = "List genre suggestions",
    description = "Returns genre names attached to a manifestation the caller can see, ranked by prefix match against `q` and then by similarity. Scoped to the caller's own visible library.",
    tag = "suggest",
    params(SuggestParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Genre names in use on a visible manifestation, ranked prefix-first then by similarity", body = SuggestResponse),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Empty or over-length q", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn suggest_genres(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<SuggestParams>, QueryRejection>,
) -> Result<Json<SuggestResponse>, AppError> {
    let Query(params) = params?;
    let suggestions =
        crate::services::reader::suggest_genres(&current_user, &state, &params.q, params.limit)
            .await?;

    Ok(Json(SuggestResponse { suggestions }))
}

/// `GET /api/v1/moods/suggest`: mood names in use on a visible
/// manifestation, ranked prefix-first then by similarity.
///
/// # Errors
/// Same as [`suggest_genres`].
#[utoipa::path(
    get,
    path = "/api/v1/moods/suggest",
    summary = "List mood suggestions",
    description = "Returns mood names attached to a manifestation the caller can see, ranked by prefix match against `q` and then by similarity. Scoped to the caller's own visible library.",
    tag = "suggest",
    params(SuggestParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Mood names in use on a visible manifestation, ranked prefix-first then by similarity", body = SuggestResponse),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Empty or over-length q", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn suggest_moods(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<SuggestParams>, QueryRejection>,
) -> Result<Json<SuggestResponse>, AppError> {
    let Query(params) = params?;
    let suggestions =
        crate::services::reader::suggest_moods(&current_user, &state, &params.q, params.limit)
            .await?;

    Ok(Json(SuggestResponse { suggestions }))
}

/// `GET /api/v1/tags/suggest`: tag names in use on a visible
/// manifestation, ranked prefix-first then by similarity.
///
/// # Errors
/// Same as [`suggest_genres`].
#[utoipa::path(
    get,
    path = "/api/v1/tags/suggest",
    summary = "List tag suggestions",
    description = "Returns tag names attached to a manifestation the caller can see, ranked by prefix match against `q` and then by similarity. Scoped to the caller's own visible library.",
    tag = "suggest",
    params(SuggestParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Tag names in use on a visible manifestation, ranked prefix-first then by similarity", body = SuggestResponse),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Empty or over-length q", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn suggest_tags(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<SuggestParams>, QueryRejection>,
) -> Result<Json<SuggestResponse>, AppError> {
    let Query(params) = params?;
    let suggestions =
        crate::services::reader::suggest_tags(&current_user, &state, &params.q, params.limit)
            .await?;

    Ok(Json(SuggestResponse { suggestions }))
}

/// Author names in use on a visible manifestation, ranked prefix-first then by similarity.
///
/// Scoped to the `author` role (see [`query_authors`]), matching the
/// `?author=` filter it feeds.
///
/// # Errors
/// Same as [`suggest_genres`].
#[utoipa::path(
    get,
    path = "/api/v1/authors/suggest",
    summary = "List author suggestions",
    description = "Returns author names credited on a manifestation the caller can see, ranked by prefix match against `q` and then by similarity. Only the `author` credit role is matched, not editors, translators, or narrators. Scoped to the caller's own visible library.",
    tag = "suggest",
    params(SuggestParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Author names in use on a visible manifestation, ranked prefix-first then by similarity", body = SuggestResponse),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Empty or over-length q", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn suggest_authors(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<SuggestParams>, QueryRejection>,
) -> Result<Json<SuggestResponse>, AppError> {
    let Query(params) = params?;
    let suggestions =
        crate::services::reader::suggest_authors(&current_user, &state, &params.q, params.limit)
            .await?;

    Ok(Json(SuggestResponse { suggestions }))
}

/// Resolve author ids to display names so a restored filter chip can label itself.
///
/// The list payload carries author names, not ids, and a none-of filter hides
/// the author's rows entirely, so a chip restored from a bookmarked URL has
/// nothing else to label itself with.
///
/// The `id` filter is required and capped, keeping this route bounded by
/// construction: it is the seed of a future paginated authors collection,
/// but until that lands a bare `GET /api/v1/authors` is a 422, not a
/// list-all. Ids are RLS-scoped through the same in-use join as
/// `authors/suggest` and restricted to the `author` role, so the chip label
/// resolves the same author set the `?author=` filter matches; an id the
/// caller cannot see (or one that is not an author) is silently omitted
/// rather than 404'd, and unknown ids drop out the same way.
///
/// # Errors
/// - [`AppError::MalformedQuery`] when an `id` value is not a UUID.
/// - [`AppError::Validation`] when `id` is absent/empty or over
///   60 values.
/// - [`AppError::Internal`] on database errors.
#[utoipa::path(
    get,
    path = "/api/v1/authors",
    summary = "Resolve author ids to names",
    description = "Resolves the given `id` values to author display names, for labelling a restored filter chip. Ids are scoped to the caller's own visible library and to the `author` credit role; an id the caller cannot see, or one that is not an author, is silently omitted rather than reported as missing. Requires at least one `id` and rejects more than 60.",
    tag = "suggest",
    params(AuthorsByIdParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Resolved author labels for the requested ids", body = AuthorsResponse),
        (status = 400, description = "Malformed id parameter", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Missing id filter or too many ids", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn authors_by_id(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<AuthorsByIdParams>, QueryRejection>,
) -> Result<Json<AuthorsResponse>, AppError> {
    let Query(params) = params?;
    crate::services::reader::validate_label_count(params.id.len())?;
    let items = crate::services::reader::author_labels(&current_user, &state, &params.id).await?;
    Ok(Json(AuthorsResponse { items }))
}

#[utoipa::path(
    get,
    path = "/api/v1/series",
    summary = "Resolve series ids to names",
    description = "Resolves the requested series ids to display names. Only series attached to a manifestation in the caller's visible library are returned; hidden and unknown ids are omitted alike. Requires at least one id and rejects more than 60.",
    tag = "suggest",
    params(SeriesByIdParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Resolved series labels", body = SeriesResponse),
        (status = 400, description = "Malformed id parameter", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Missing id filter or too many ids", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn series_by_id(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<SeriesByIdParams>, QueryRejection>,
) -> Result<Json<SeriesResponse>, AppError> {
    let Query(params) = params?;
    crate::services::reader::validate_label_count(params.id.len())?;
    let items = crate::services::reader::series_labels(&current_user, &state, &params.id).await?;
    Ok(Json(SeriesResponse { items }))
}

/// `GET /api/v1/series/suggest`: series names in use on a visible
/// manifestation, ranked prefix-first then by similarity.
///
/// # Errors
/// Same as [`suggest_genres`].
#[utoipa::path(
    get,
    path = "/api/v1/series/suggest",
    summary = "List series suggestions",
    description = "Returns series names attached to a manifestation the caller can see, ranked by prefix match against `q` and then by similarity. Scoped to the caller's own visible library.",
    tag = "suggest",
    params(SuggestParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Series names in use on a visible manifestation, ranked prefix-first then by similarity", body = SuggestResponse),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Empty or over-length q", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn suggest_series(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<SuggestParams>, QueryRejection>,
) -> Result<Json<SuggestResponse>, AppError> {
    let Query(params) = params?;
    let suggestions =
        crate::services::reader::suggest_series(&current_user, &state, &params.q, params.limit)
            .await?;

    Ok(Json(SuggestResponse { suggestions }))
}

/// Distinct publisher values in use on a visible manifestation, ranked prefix-first then by similarity.
///
/// Every returned [`Suggestion`] carries `id: None` (publishers have no
/// dedicated vocabulary table; see [`query_publishers`]).
///
/// # Errors
/// Same as [`suggest_genres`].
#[utoipa::path(
    get,
    path = "/api/v1/publishers/suggest",
    summary = "List publisher suggestions",
    description = "Returns distinct publisher values recorded on a manifestation the caller can see, ranked by prefix match against `q` and then by similarity. Scoped to the caller's own visible library.",
    tag = "suggest",
    params(SuggestParams),
    security(("session_cookie" = ["read"]), ("device_token_bearer" = ["read"]), ("opds_basic" = ["read"])),
    responses(
        (status = 200, description = "Distinct publisher values in use on a visible manifestation, ranked prefix-first then by similarity", body = SuggestResponse),
        (status = 400, description = "Malformed query parameter", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 401, description = "Authentication required", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
        (status = 422, description = "Empty or over-length q", body = crate::openapi::ProblemDetails, content_type = "application/problem+json"),
    )
)]
async fn suggest_publishers(
    current_user: CurrentUser,
    State(state): State<AppState>,
    params: Result<Query<SuggestParams>, QueryRejection>,
) -> Result<Json<SuggestResponse>, AppError> {
    let Query(params) = params?;
    let suggestions =
        crate::services::reader::suggest_publishers(&current_user, &state, &params.q, params.limit)
            .await?;

    Ok(Json(SuggestResponse { suggestions }))
}

#[cfg(test)]
mod tests {
    use axum::http::{HeaderName, HeaderValue, StatusCode};
    use sqlx::PgPool;
    use uuid::Uuid;

    use crate::test_support;

    fn auth(header: &str) -> (HeaderName, HeaderValue) {
        (
            axum::http::header::AUTHORIZATION,
            HeaderValue::from_str(header).expect("ascii auth header"),
        )
    }

    async fn insert_linked_genre(pool: &PgPool, name: &str, manifestation_id: Uuid) -> Uuid {
        let genre_id: Uuid =
            sqlx::query_scalar!("INSERT INTO genres (name) VALUES ($1) RETURNING id", name,)
                .fetch_one(pool)
                .await
                .expect("insert genre");
        sqlx::query!(
            "INSERT INTO manifestation_genres (manifestation_id, genre_id) VALUES ($1, $2)",
            manifestation_id,
            genre_id,
        )
        .execute(pool)
        .await
        .expect("link genre");
        genre_id
    }

    fn suggestion_values(body: &serde_json::Value) -> Vec<String> {
        body["suggestions"]
            .as_array()
            .expect("suggestions array")
            .iter()
            .map(|s| s["value"].as_str().expect("value string").to_owned())
            .collect()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn rls_scopes_child_to_shelved_manifestation(pool: PgPool) {
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (_work_a, m_a) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "rls-a").await;
        let (_work_b, m_b) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "rls-b").await;
        insert_linked_genre(&ingestion_pool, "Alpha", m_a).await;
        insert_linked_genre(&ingestion_pool, "Bravo", m_b).await;
        let (child_id, child_basic) =
            test_support::db::create_child_user_and_basic_auth(&app_pool, "rls-child").await;
        let shelf_id = test_support::db::create_shelf(&app_pool, child_id, "Child shelf").await;
        test_support::db::add_to_shelf(&app_pool, shelf_id, m_a).await;
        let (_adult_id, adult_basic) =
            test_support::db::create_adult_and_basic_auth(&app_pool, "rls-adult").await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let child_sees_shelved = server
            .get("/api/v1/genres/suggest?q=Alp")
            .add_header(auth(&child_basic).0.clone(), auth(&child_basic).1.clone())
            .await;
        assert!(suggestion_values(&child_sees_shelved.json()).contains(&"Alpha".to_owned()));

        let child_hides_unshelved = server
            .get("/api/v1/genres/suggest?q=Bra")
            .add_header(auth(&child_basic).0, auth(&child_basic).1)
            .await;
        assert!(
            suggestion_values(&child_hides_unshelved.json()).is_empty(),
            "child must not see a genre from an unshelved manifestation"
        );

        let adult_sees_a = server
            .get("/api/v1/genres/suggest?q=Alp")
            .add_header(auth(&adult_basic).0.clone(), auth(&adult_basic).1.clone())
            .await;
        assert!(suggestion_values(&adult_sees_a.json()).contains(&"Alpha".to_owned()));

        let adult_sees_b = server
            .get("/api/v1/genres/suggest?q=Bra")
            .add_header(auth(&adult_basic).0, auth(&adult_basic).1)
            .await;
        assert!(suggestion_values(&adult_sees_b.json()).contains(&"Bravo".to_owned()));
    }

    /// The `items[]` labels from an authors batch-get response body.
    fn author_values(body: &serde_json::Value) -> Vec<String> {
        body["items"]
            .as_array()
            .expect("items array")
            .iter()
            .map(|s| s["value"].as_str().expect("value string").to_owned())
            .collect()
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn series_by_id_omits_out_of_rls_scope(pool: PgPool) {
        // Listed boundary: private labels cannot disclose an unseen manifestation.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (visible_work, visible_book) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "series-visible")
                .await;
        let (hidden_work, _) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "series-hidden").await;
        let visible = sqlx::query_scalar!(
            "INSERT INTO series (name, sort_name) VALUES ($1, $1) RETURNING id",
            "Visible series",
        )
        .fetch_one(&ingestion_pool)
        .await
        .unwrap();
        let hidden = sqlx::query_scalar!(
            "INSERT INTO series (name, sort_name) VALUES ($1, $1) RETURNING id",
            "Hidden series",
        )
        .fetch_one(&ingestion_pool)
        .await
        .unwrap();
        sqlx::query!(
            "INSERT INTO series_works (series_id, work_id) VALUES ($1, $2), ($3, $4)",
            visible,
            visible_work,
            hidden,
            hidden_work,
        )
        .execute(&ingestion_pool)
        .await
        .unwrap();
        let (child, basic) =
            test_support::db::create_child_user_and_basic_auth(&app_pool, "series-child").await;
        let shelf = test_support::db::create_shelf(&app_pool, child, "Visible books").await;
        test_support::db::add_to_shelf(&app_pool, shelf, visible_book).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);
        let unknown = Uuid::new_v4();
        let response = server
            .get(&format!(
                "/api/v1/series?id={visible}&id={hidden}&id={unknown}"
            ))
            .add_header(auth(&basic).0, auth(&basic).1)
            .await;
        assert_eq!(
            response.status_code(),
            StatusCode::OK,
            "{}",
            response.text()
        );
        assert_eq!(
            response.json::<serde_json::Value>(),
            serde_json::json!({"items": [{"id": visible, "value": "Visible series"}]})
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn authors_by_id_omits_out_of_rls_scope(pool: PgPool) {
        // An author linked only through a manifestation the caller cannot see
        // is omitted, mirroring the suggest endpoint's RLS scoping.
        let app_pool = test_support::db::app_pool_for(&pool).await;
        let ingestion_pool = test_support::db::ingestion_pool_for(&pool).await;
        let (work_a, m_a) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "authrls-a").await;
        let (work_b, _m_b) =
            test_support::db::insert_work_and_manifestation(&ingestion_pool, "authrls-b").await;
        let author_shelved = test_support::db::insert_contributor(
            &ingestion_pool,
            work_a,
            "Shelved Scribe",
            "author",
            0,
        )
        .await;
        let author_hidden = test_support::db::insert_contributor(
            &ingestion_pool,
            work_b,
            "Hidden Hand",
            "author",
            0,
        )
        .await;
        let (child_id, child_basic) =
            test_support::db::create_child_user_and_basic_auth(&app_pool, "authrls-child").await;
        let shelf_id = test_support::db::create_shelf(&app_pool, child_id, "Child shelf").await;
        test_support::db::add_to_shelf(&app_pool, shelf_id, m_a).await;
        let server = test_support::db::server_with_real_pools(&app_pool, &ingestion_pool);

        let r = server
            .get(&format!(
                "/api/v1/authors?id={author_shelved}&id={author_hidden}"
            ))
            .add_header(auth(&child_basic).0, auth(&child_basic).1)
            .await;
        assert_eq!(r.status_code(), StatusCode::OK, "body: {}", r.text());
        let values = author_values(&r.json());
        assert!(values.contains(&"Shelved Scribe".to_owned()), "{values:?}");
        assert!(
            !values.contains(&"Hidden Hand".to_owned()),
            "child must not resolve an author from an unshelved manifestation: {values:?}"
        );
    }
}
