//! OpenAPI 3.1 contract for the Reverie HTTP API (docs-as-done, UNK-370;
//! `adr/2026-06-08-api-versioning-openapi.md`).
//!
//! The spec is generated code-first from the handlers: [`ApiDoc`] supplies the
//! document metadata and shared component schemas, and each documented module
//! contributes its paths through an [`OpenApiRouter`]. [`spec_json`] serializes
//! the merged document; the committed `docs/openapi.json` is drift-gated by
//! `tests/gen_openapi.rs`, and `starlight-openapi` renders it on the docs site.
//!
//! Phase 1 is a single pilot module — `health` — proving the pipeline end to
//! end. Removing a `#[utoipa::path]` from a handler wired via `routes!` is a
//! compile error, which is the coverage mechanism the remaining route modules
//! adopt module-by-module in phase 2.

use utoipa::OpenApi;
use utoipa_axum::router::OpenApiRouter;

use crate::state::AppState;

/// Version of the API *contract* surfaced in the spec's `info.version`.
///
/// Deliberately decoupled from the crate/release version (`CARGO_PKG_VERSION`):
/// the committed `docs/openapi.json` is byte-for-byte drift-gated, so tying it
/// to the release-please-managed crate version would turn every version bump
/// into a spec regeneration. It tracks the API surface, not the binary — the
/// URL-path major version (`/api/v1`, phase 2) is the unit of breaking change.
const API_VERSION: &str = "0.1.0";

/// Top-level OpenAPI document: metadata, shared schemas, and tags. Paths are
/// contributed by each module's [`OpenApiRouter`] and merged in `pilot_router`.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Reverie API",
        version = API_VERSION,
        description = "HTTP API for Reverie, a self-hosted ebook library manager. \
                       Generated from the server handlers; do not edit by hand."
    ),
    components(schemas(ProblemDetails)),
    tags((name = "health", description = "Liveness and readiness probes."))
)]
pub struct ApiDoc;

/// RFC 7807 `application/problem+json` error body, as emitted by the server's
/// `AppError` type. Documentation-only: it mirrors the response shape so the
/// spec can reference it, and is not constructed at runtime.
#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct ProblemDetails {
    /// Stable URI reference identifying the problem type.
    #[schema(example = "https://reverie.example/probs/not-found")]
    pub r#type: String,
    /// Short, human-readable summary of the problem type.
    pub title: String,
    /// HTTP status code, repeated in the body per RFC 7807.
    #[schema(example = 404)]
    pub status: u16,
    /// Human-readable explanation specific to this occurrence.
    pub detail: String,
    /// URI reference identifying the specific request, when available.
    pub instance: Option<String>,
}

/// The pilot router seeded with [`ApiDoc`] metadata, merging every documented
/// module. Built once and consumed in two ways: [`router()`] takes its runtime
/// [`axum::Router`] half, [`spec_json()`] takes its [`OpenApi`] half — so the
/// served routes and the generated spec are always the same registration.
fn pilot_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi()).merge(crate::routes::health::router())
}

/// Runtime router for the OpenAPI-documented modules, ready to merge into the
/// main router. Discards the spec half (see [`spec_json`]).
pub fn router() -> axum::Router<AppState> {
    pilot_router().split_for_parts().0
}

/// Serialize the OpenAPI document as pretty-printed JSON (trailing newline,
/// matching the committed artifact and the `print-config-schema` convention).
///
/// # Errors
///
/// Returns an error if the document cannot be serialized to JSON.
pub fn spec_json() -> anyhow::Result<String> {
    use anyhow::Context as _;

    let (_, api) = pilot_router().split_for_parts();
    let mut json = api
        .to_pretty_json()
        .context("serialize OpenAPI document to JSON")?;
    json.push('\n');
    Ok(json)
}
