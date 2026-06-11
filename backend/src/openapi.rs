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

use utoipa::openapi::security::{ApiKey, ApiKeyValue, HttpAuthScheme, HttpBuilder, SecurityScheme};
use utoipa::{Modify, OpenApi};
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

/// Injects the API's `securitySchemes` into the generated OpenAPI document.
/// Paired with the document-level `security` default on [`ApiDoc`], it encodes a
/// deny-by-default authentication contract: every operation requires the
/// `session_cookie` scheme unless it explicitly opts out with `security(())`.
///
/// THREAT: documentation-time fail-safe for the documented surface. A handler
/// wired through `pilot_router` that omits a per-operation `security` annotation
/// inherits the global requirement and documents-as-authed — never as-public —
/// so an undocumented-public endpoint cannot silently enter the contract (OWASP
/// fail-safe defaults; matches the Checkov `CKV_OPENAPI_4` shape). Routes outside
/// `pilot_router` are not in the spec at all; runtime enforcement for every route
/// lives in `auth/` middleware — the spec is a contract signal, not a gate.
///
/// Schemes:
/// - `session_cookie` — `apiKey` in cookie `id`, the session cookie set by the
///   `SessionManagerLayer` (tracks tower-sessions' default name, which the layer
///   does not override via `.with_name`). Covers the JSON data API.
/// - `opds_basic` — HTTP Basic, for the OPDS feeds' Basic-auth path.
///
/// Both schemes carry a `description` documenting that HTTPS is mandatory in
/// production — Basic credentials and session cookies are otherwise exposed in
/// cleartext. Transport is enforced operationally (reverse-proxy TLS; the
/// session cookie is `Secure` when `behind_https`), not by the spec, so the
/// residual Checkov `CKV_OPENAPI_3` finding on `opds_basic` is a justified skip.
///
/// See `adr/2026-06-08-api-versioning-openapi.md`.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi
            .components
            .get_or_insert_with(utoipa::openapi::Components::default);
        components.add_security_scheme(
            "session_cookie",
            SecurityScheme::ApiKey(ApiKey::Cookie(ApiKeyValue::with_description(
                "id",
                "Session cookie issued by the server's session layer. Set with the Secure \
                 attribute when `behind_https` is enabled; MUST be served over HTTPS in \
                 production to prevent session hijacking.",
            ))),
        );
        components.add_security_scheme(
            "opds_basic",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Basic)
                    .description(Some(
                        "HTTP Basic authentication for the OPDS feeds. MUST be used over \
                         HTTPS in production — Basic credentials are otherwise exposed in \
                         transit (Checkov CKV_OPENAPI_3).",
                    ))
                    .build(),
            ),
        );
    }
}

/// Top-level OpenAPI document: metadata, shared schemas, security model, and
/// tags. Paths are contributed by each module's [`OpenApiRouter`] and merged in
/// `pilot_router`. The document-level `security` is the deny-by-default
/// requirement (see `SecurityAddon`); operational probes opt out per-operation.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Reverie API",
        version = API_VERSION,
        description = "HTTP API for Reverie, a self-hosted ebook library manager. \
                       Generated from the server handlers; do not edit by hand."
    ),
    modifiers(&SecurityAddon),
    security(("session_cookie" = [])),
    // `SortMode` is referenced only by the `library` list `IntoParams` (`?sort=`),
    // which utoipa does NOT auto-collect into components the way `routes!` collects
    // response-body schemas — register it explicitly so the `$ref` resolves (a
    // dangling ref passes the byte-drift gate but fails the docs-site `$ref` parse).
    components(schemas(ProblemDetails, crate::routes::cursor::SortMode)),
    tags(
        (name = "health", description = "Liveness and readiness probes."),
        (name = "library", description = "Books, works, and full-text search."),
        (name = "series", description = "Series and their ordered works."),
        (name = "dashboard", description = "Admin-only library-health aggregates."),
        (name = "shelves", description = "User-scoped curation shelves and their ordered items."),
        (name = "users", description = "Admin-only user management."),
        (name = "settings", description = "Admin-only runtime settings."),
        (name = "tokens", description = "Per-user device tokens for OPDS / Basic-auth clients.")
    )
)]
pub struct ApiDoc;

/// RFC 9457 `application/problem+json` error body, as emitted by the server's
/// `AppError` type. Documentation-only: it mirrors the response shape and is
/// never constructed or serialized at runtime. Registered as a shared component
/// describing the standard error envelope; the data routes that actually return
/// it are annotated to reference it in phase 2 (UNK-376).
#[derive(utoipa::ToSchema)]
pub struct ProblemDetails {
    /// Stable URI reference identifying the problem type.
    #[schema(example = "https://reverie.example/probs/not-found")]
    pub r#type: String,
    /// Short, human-readable summary of the problem type.
    pub title: String,
    /// HTTP status code, repeated in the body per RFC 9457.
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
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(crate::routes::health::router())
        .merge(crate::routes::library::router())
        .merge(crate::routes::series::router())
        .merge(crate::routes::dashboard::router())
        .merge(crate::routes::shelves::router())
        .merge(crate::routes::users::router())
        .merge(crate::routes::settings::router())
        .merge(crate::routes::tokens::router())
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
