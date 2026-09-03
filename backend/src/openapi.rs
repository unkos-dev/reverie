//! OpenAPI 3.1 contract for the Reverie HTTP API (docs-as-done;
//! `adr/2026-06-08-api-versioning-openapi.md`).
//!
//! The spec is generated code-first from the handlers: [`ApiDoc`] supplies the
//! document metadata and shared component schemas, and each documented module
//! contributes its paths through an [`OpenApiRouter`]. [`spec_json`] serializes
//! the merged document; the committed `docs/openapi.json` is drift-gated by
//! `tests/gen_openapi.rs`, and `starlight-openapi` renders it on the docs site.
//!
//! `health` was the original single pilot module proving the pipeline end to
//! end; every route module now documents its operations the same way.
//! Removing a `#[utoipa::path]` from a handler wired via `routes!` is a
//! compile error, so a module cannot silently regress to undocumented.
//!
//! # Coupling with `crate::error` (intentional)
//!
//! [`ProblemDetails`] is deliberately both the documented component schema
//! and the runtime error DTO: the `IntoResponse` impl on
//! [`crate::error::AppError`] constructs it, and its own `IntoResponse`
//! reads the request-path task-local from [`crate::error::instance`]. The
//! resulting openapi↔error circularity is the point — one struct owns the
//! RFC 9457 wire shape, so the spec and the bytes on the wire cannot drift.
//! Anyone splitting `ProblemDetails` out of this module must move both
//! halves together.

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
/// inherits the global requirement and documents-as-authed, never as-public,
/// so an undocumented-public endpoint cannot silently enter the contract (OWASP
/// fail-safe defaults). Routes outside
/// `pilot_router` are not in the spec at all; runtime enforcement for every route
/// lives in `auth/` middleware. The spec is a contract signal, not a gate.
///
/// Schemes:
/// - `session_cookie`: `apiKey` in cookie `id`, the session cookie set by the
///   `SessionManagerLayer` (tracks tower-sessions' default name, which the layer
///   does not override via `.with_name`). Covers the JSON data API.
/// - `opds_basic`: HTTP Basic, for the OPDS feeds' Basic-auth path.
/// - `device_token_bearer`: HTTP Bearer, the unified
///   `{prefix}{token_id}.{secret}` personal-token credential
///   ([`crate::auth::token::TOKEN_PREFIX`]). Resolves through the same
///   `resolve_device_token` indexed lookup as `opds_basic`; both transports
///   share one credential model.
/// - `oidc_jwt_bearer`: HTTP Bearer, an RFC 9068 resource-server access
///   token issued by the configured `IdP`. Resolves through
///   [`crate::auth::jwt::JwtValidator`] and a read-only `(iss, sub)` lookup;
///   inert (the scheme documents a surface that always 401s) when the
///   resource-server config is absent. Never provisions an account — see
///   the module docs on `crate::auth::middleware::verify_bearer`.
///
/// All four schemes carry a `description` documenting that HTTPS is mandatory
/// in production, since Basic credentials, Bearer tokens, and session cookies
/// are otherwise exposed in cleartext. Transport is enforced operationally
/// (reverse-proxy TLS; the session cookie is `Secure` when `behind_https`),
/// not by the spec, so the residual cleartext-credential findings are justified
/// skips: `owasp-no-http-basic` on `opds_basic`, and `owasp-jwt-best-practices`
/// on the Bearer schemes. Both are registered in `.vacuum.yaml`.
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
                         HTTPS in production; Basic credentials are otherwise exposed in \
                         transit.",
                    ))
                    .build(),
            ),
        );
        components.add_security_scheme(
            "device_token_bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .description(Some(
                        "Personal device-token credential (`{prefix}{token_id}.{secret}`). \
                         MUST be used over HTTPS in production; the token is otherwise \
                         exposed in transit.",
                    ))
                    .build(),
            ),
        );
        components.add_security_scheme(
            "oidc_jwt_bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some(
                        "RFC 9068 resource-server access token issued by the configured IdP. \
                         MUST be used over HTTPS in production; the token is otherwise exposed \
                         in transit.",
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
    // `FieldVersionChange` is registered explicitly: its only reference is
    // inside `UpdateMetadataResponse::fields`'s hand-built bounded-map
    // schema, which the normal derive traversal never walks into.
    components(schemas(ProblemDetails, crate::routes::metadata::FieldVersionChange)),
    tags(
        (name = "health", description = "Liveness and readiness probes."),
        (name = "library", description = "Books, works, and full-text search."),
        (name = "suggest", description = "Typeahead suggestions over the metadata vocabularies (genres, moods, tags, authors, series, publishers)."),
        (name = "series", description = "Series and their ordered works."),
        (name = "dashboard", description = "Admin-only library-health aggregates."),
        (name = "shelves", description = "User-scoped curation shelves and their ordered items."),
        (name = "users", description = "Admin-only user management."),
        (name = "settings", description = "Admin-only runtime settings."),
        (name = "tokens", description = "Per-user device tokens for OPDS / Basic-auth clients."),
        (name = "metadata", description = "Metadata review queue: accept / reject / revert / lock and manual edits."),
        (name = "reading", description = "Per-user reading state: status, rating, notes, and reading dates."),
        (name = "enrichment", description = "Enrichment pipeline controls and queue status."),
        (name = "ingestion", description = "Admin-only library-scan trigger."),
        (name = "auth", description = "OIDC login flow, session introspection, and theme preference."),
        (name = "opds", description = "OPDS 1.2 catalog (Atom XML) for e-reader clients, HTTP Basic auth. \
                                       Documented unconditionally; the routes are absent at runtime when the \
                                       operator sets `opds.enabled = false`.")
    )
)]
pub struct ApiDoc;

/// RFC 9457 `application/problem+json` error body — the single runtime DTO
/// for the error envelope. The `IntoResponse` impl on
/// [`crate::error::AppError`] constructs it for every Problem-Details
/// variant, and operational probes (readiness) build it directly, so the
/// documented component schema and the bytes on the wire come from the same
/// struct and cannot drift.
#[derive(serde::Serialize, utoipa::ToSchema)]
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
    /// Omitted (RFC 9457 §3.1 permits this) outside an HTTP request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
}

impl axum::response::IntoResponse for ProblemDetails {
    /// Serialize as `application/problem+json` with the status taken from
    /// the body's `status` field. When `instance` is unset, it is populated
    /// from the request-path task-local captured by
    /// [`crate::error::instance::problem_instance_layer`] (and stays omitted
    /// outside an HTTP request, e.g. unit tests).
    fn into_response(mut self) -> axum::response::Response {
        use axum::http::{HeaderValue, StatusCode, header};

        if self.instance.is_none() {
            self.instance = crate::error::instance::current_request_uri();
        }
        let status = StatusCode::from_u16(self.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let mut response = (status, axum::Json(self)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        response
    }
}

/// The pilot router seeded with [`ApiDoc`] metadata, merging every documented
/// module that is mounted unconditionally. Built once and consumed in two
/// ways: [`router()`] takes its runtime [`axum::Router`] half, [`spec_json()`]
/// takes its [`OpenApi`] half — so the served routes and the generated spec
/// are always the same registration.
///
/// The OPDS feed routes are the one deliberate exception: their runtime mount
/// is config-gated (`opds.enabled`) in `crate::build_router`, while the spec
/// documents them unconditionally — [`spec_json`] merges
/// [`crate::routes::opds::openapi_router`] on top of this router before
/// splitting. The dual-mounted cover handlers' API half
/// ([`crate::routes::opds::covers_router`]) is always mounted, so it lives
/// here with the rest.
fn pilot_router() -> OpenApiRouter<AppState> {
    OpenApiRouter::with_openapi(ApiDoc::openapi())
        .merge(crate::routes::health::router())
        .merge(crate::routes::library::router())
        .merge(crate::routes::suggest::router())
        .merge(crate::routes::series::router())
        .merge(crate::routes::dashboard::router())
        .merge(crate::routes::shelves::router())
        .merge(crate::routes::users::router())
        .merge(crate::routes::settings::router())
        .merge(crate::routes::tokens::router())
        .merge(crate::routes::metadata::router())
        .merge(crate::routes::reading::router())
        .merge(crate::routes::enrichment::router())
        .merge(crate::routes::ingestion::router())
        .merge(crate::routes::auth::router())
        .merge(crate::routes::preferences::router())
        .merge(crate::routes::opds::covers_router())
}

/// Runtime router for the OpenAPI-documented modules, ready to merge into the
/// main router. Discards the spec half (see [`spec_json`]). Does NOT include
/// the OPDS feed routes — `crate::build_router` mounts those separately,
/// gated on `opds.enabled`.
pub fn router() -> axum::Router<AppState> {
    pilot_router().split_for_parts().0
}

/// Serialize the OpenAPI document as pretty-printed JSON (trailing newline,
/// matching the committed artifact and the `print-config-schema` convention).
///
/// Merges the config-gated OPDS routes into the document first: the contract
/// documents the full surface; per-instance availability is noted on the
/// `opds` tag description.
///
/// # Errors
///
/// Returns an error if the document cannot be serialized to JSON.
pub fn spec_json() -> anyhow::Result<String> {
    use anyhow::Context as _;

    let (_, api) = pilot_router()
        .merge(crate::routes::opds::openapi_router())
        .split_for_parts();
    let mut json = api
        .to_pretty_json()
        .context("serialize OpenAPI document to JSON")?;
    json.push('\n');
    Ok(json)
}
