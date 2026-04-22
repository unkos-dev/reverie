# Plan: OPDS 1.2 Catalog (BLUEPRINT Step 9)

## Summary

Implement OPDS 1.2 (Atom XML) catalog endpoints so any OPDS-compatible reader
(Moon+ Reader, KOReader, Librera, KyBook 3) can browse and download books from
Reverie using HTTP Basic auth with a device token. Scope is **URL-based** — the
tree bifurcates into `/opds/library/*` (whole library) and
`/opds/shelves/{id}/*` (shelf-scoped); child accounts are further filtered
underneath by the existing `manifestations` RLS policies. Downloads and
OpenSearch discovery live under `/opds/*` (Basic-only). Cover images are
dual-mounted at `/opds/books/{id}/cover{,/thumb}` (BasicOnly, emitted in OPDS
feeds so credentials stay in the paired protection space) and
`/api/books/{id}/cover{,/thumb}` (CurrentUser session-or-Basic, consumed by the
future Step 10 web frontend). Handler body is shared.

## User Story

As a self-hosted Reverie user,
I want to pair my Android / iOS / e-ink reader against Reverie via OPDS,
So that I can browse the catalog, search, and download EPUBs
directly in the reader app without using the web UI.

## Problem → Solution

**Current state:** Step 8 guarantees the managed EPUB's OPF + cover reflect
canonical metadata. But no non-browser client can browse the library. There
is no OPDS endpoint, no download route, no cover image route, and no
Basic-only authentication (device tokens work through `CurrentUser`, but no
extractor emits the `WWW-Authenticate` challenge mobile clients need).

**Desired state:** paired OPDS clients can reach a root catalog, navigate
by author / series / newly-added, search, and stream EPUB bytes. Scope is
determined entirely by the feed URL (`/opds/library/…` vs
`/opds/shelves/{id}/…`), so a device paired at `/opds/shelves/{abc}` sees
only that shelf — no hidden preference, no per-token policy. Child accounts
inherit the existing `manifestations_select_child` RLS filter underneath.

## Metadata

- **Complexity**: Large (~30 files, ~2500 LOC Rust + tests)
- **Source PRD**: `plans/BLUEPRINT.md` lines 1282–1701
- **Estimated Files**: 30 (19 new Rust modules, 5 updated existing modules,
  1 config update, 1 `.env.example` update, 4 test files)
- **Branch**: `feat/opds-catalog`
- **Depends on**: Step 8 merged. Confirms `manifestations.current_file_hash`,
  `manifestations.cover_path`, and the Step-5 EPUB parsers
  (`services::epub::{zip_layer, container_layer, opf_layer}`) are in place.
- **Parallel with**: Step 10 (separate worktree). Covers endpoint is a
  cross-step contract; Step 10 consumes it.
- **Tier**: Strong (XML wire format + auth challenge semantics + RLS under
  URL-based scope + on-disk cover cache + path-traversal guard).

---

## UX Design

### Before State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                              BEFORE STATE                                     ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌─────────────┐       ┌─────────────┐       ┌──────────────────┐            ║
║   │ KOReader /  │──OPDS→│   404 Not   │       │   Web UI only    │            ║
║   │ Moon+ Rdr   │       │    Found    │       │  (Step 10 TBD)   │            ║
║   └─────────────┘       └─────────────┘       └──────────────────┘            ║
║                                                                               ║
║   USER_FLOW: open reader → add OPDS catalog → URL returns 404 → dead end      ║
║   PAIN_POINT: no way to consume Reverie from a reader app                     ║
║   DATA_FLOW: manifestations RLS works, but no HTTP surface exposes it         ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### After State

```
╔═══════════════════════════════════════════════════════════════════════════════╗
║                               AFTER STATE                                     ║
╠═══════════════════════════════════════════════════════════════════════════════╣
║                                                                               ║
║   ┌────────────┐ Basic  ┌──────────────┐   ┌────────────────────────────┐     ║
║   │  KOReader  │───401──│  /opds/*     │   │ Atom feed: navigation +    │     ║
║   │  Moon+ Rdr │→creds→ │  BasicOnly   │─→ │  acquisition; kind param   │     ║
║   │  Librera   │        │  extractor   │   │  in Content-Type; absolute │     ║
║   │  KyBook 3  │        └──────┬───────┘   │  hrefs against PUBLIC_URL  │     ║
║   └────────────┘               │           └──────────┬─────────────────┘     ║
║                                ▼                      ▼                       ║
║                    ┌───────────────────┐   ┌─────────────────────────┐        ║
║                    │ acquire_with_rls  │   │ GET /opds/books/:id/    │        ║
║                    │ (GUC = user_id)   │   │   file  → streamed EPUB │        ║
║                    │ scope builder     │   │ (Content-Disposition:   │        ║
║                    │  + pagination     │   │  attachment RFC 6266)   │        ║
║                    └─────────┬─────────┘   └─────────────────────────┘        ║
║                              │                                                ║
║                              ▼                                                ║
║                 ┌────────────────────────┐   ┌───────────────────────┐        ║
║                 │ quick-xml Writer       │   │ GET /api/books/:id/   │        ║
║                 │ auto-escape user text  │   │  cover{,/thumb}       │        ║
║                 │ + strip XML-invalid    │   │  → disk cache, LRU-   │        ║
║                 │   control chars        │   │  free, hash-keyed     │        ║
║                 └────────────────────────┘   └───────────────────────┘        ║
║                                                                               ║
║   USER_FLOW: reader pairs at /opds → 401 with challenge → credentials →       ║
║              root feed → library/shelf feeds → search → download              ║
║   VALUE_ADD: first-class reader integration without browser intermediary      ║
║   DATA_FLOW: Basic creds → BasicOnly → RLS tx → scope filter → Atom bytes     ║
║                                                                               ║
╚═══════════════════════════════════════════════════════════════════════════════╝
```

### Interaction Changes

| Location | Before | After | Impact |
|---|---|---|---|
| `/opds/*` (all routes) | 404 | Atom feeds + downloads behind Basic auth | Reader apps can browse / download |
| `/opds/books/:id/cover{,/thumb}` + `/api/books/:id/cover{,/thumb}` | 404 | JPEG/PNG/WebP from on-disk cache (shared handler; `/opds/*` under BasicOnly, `/api/*` under CurrentUser) | OPDS clients load covers inside paired protection space; Step 10 web UI loads them with session cookie |
| 401 responses on `/opds/*` | N/A | `WWW-Authenticate: Basic realm="Reverie OPDS", charset="UTF-8"` | Clients prompt for credentials per RFC 7617 |
| `auth::middleware::CurrentUser` `last_used_at` spawn | Always fires | Skipped when `last_used_at > now() - 5m` | Reduced DB write amplification under polling |
| Config | No OPDS vars | `REVERIE_OPDS_ENABLED`, `REVERIE_OPDS_PAGE_SIZE`, `REVERIE_OPDS_REALM`, `REVERIE_PUBLIC_URL` (required when OPDS on) | Fail-fast when absolute-URL origin missing |

---

## Mandatory Reading

Implementation agent MUST read these before starting any task.

| Priority | File | Lines | Why |
|---|---|---|---|
| P0 | `plans/BLUEPRINT.md` | 1282–1701 | Step 9 spec — the contract this plan implements verbatim |
| P0 | `backend/src/auth/middleware.rs` | 1–110 | `CurrentUser` extractor; BasicOnly is a specialisation that rejects session + emits challenge |
| P0 | `backend/src/auth/token.rs` | 22–27 | `verify_device_token` (constant-time SHA-256). Must not be re-implemented |
| P0 | `backend/src/models/device_token.rs` | 6–48, 113 | `list_for_user` filters `revoked_at IS NULL`; `update_last_used` is the debounce target |
| P0 | `backend/src/db.rs` | 44–54 | `acquire_with_rls` contract — SET LOCAL GUC, auto-resets on commit/rollback |
| P0 | `backend/src/error.rs` | 1–36 | `AppError` + its `IntoResponse`. JSON-shaped — OPDS routes need a variant that emits `WWW-Authenticate` and non-JSON bodies where appropriate |
| P0 | `backend/src/services/epub/zip_layer.rs` | 1–80 | `ZipHandle` + `read_entry` — raw byte retrieval from an opened EPUB |
| P0 | `backend/src/services/epub/container_layer.rs` | 15–45 | `validate` returns `Option<String>` OPF path |
| P0 | `backend/src/services/epub/opf_layer.rs` | 62–348 | `validate` produces `OpfData { manifest: HashMap<id, href>, opf_path, … }` |
| P0 | `backend/src/services/epub/cover_layer.rs` | 8–48 | `find_cover_href` priority list + `entry_path` resolution (copy the logic into covers service; see Task 16) |
| P0 | `backend/src/test_support.rs` | all | `#[sqlx::test]` scaffold, `app_pool_for`, `create_admin_and_basic_auth`, `server_with_real_pools`, `insert_work_and_manifestation` |
| P0 | `backend/src/services/writeback/opf_rewrite.rs` | 56–57, 265–301 | `quick_xml::Writer` + `BytesStart::new(name).push_attribute((k,v))` + `.into_inner()` canonical pattern |
| P1 | `backend/src/routes/tokens.rs` | 33–59, 101–183 | Representative authenticated handler shape + 401 test pattern via `test_support::test_server()` |
| P1 | `backend/src/routes/metadata.rs` | 93–98, 212–215 | `acquire_with_rls` usage in a handler; commit vs drop-rollback |
| P1 | `backend/src/main.rs` | 38–87 | Where to mount `routes::opds::router()` in the `Router::new().merge(...)` chain |
| P1 | `backend/src/config.rs` | 82–310 | `ConfigError::MissingVar` / `Invalid` pattern; `env::var().ok().filter(\|s\| !s.is_empty())` for optional strings |
| P1 | `backend/src/services/ingestion/orchestrator.rs` | 770–795 | `make_minimal_epub()` — test EPUB fixture pattern |
| P1 | `backend/migrations/20260412150007_search_rls_and_reserved.up.sql` | 44–119 | `manifestations_select_adult` and `_select_child` policies underneath everything |
| P1 | `backend/src/services/enrichment/cover_download.rs` | 183–228 | `image` 0.25 decode + `write_atomically` NamedTempFile + persist |
| P2 | `backend/Cargo.toml` | 1–45 | Dependency versions (axum 0.8, quick-xml 0.37, image 0.25, time 0.3, sqlx 0.8, tokio-util 0.7) |

### External Documentation

| Source | Section | Why |
|---|---|---|
| [OPDS Catalog 1.2](https://specs.opds.io/opds-1.2) | whole spec | Atom elements, rel strings, Content-Type `kind` param |
| [RFC 7617](https://datatracker.ietf.org/doc/html/rfc7617) | §2 + §2.1 | `WWW-Authenticate: Basic realm="…", charset="UTF-8"` exact syntax |
| [RFC 6266](https://datatracker.ietf.org/doc/html/rfc6266) | §4.1, §4.2 | `Content-Disposition: attachment; filename*=UTF-8''…` + ASCII fallback |
| [RFC 5987](https://datatracker.ietf.org/doc/html/rfc5987) | §3.2.1 | `attr-char` set; everything else must be percent-encoded |
| [OpenSearch 1.1 Draft 6](https://github.com/dewitt/opensearch/blob/master/opensearch-1-1-draft-6.md) | whole spec | `<OpenSearchDescription>` required fields, `{searchTerms}` template |
| [W3C XML 1.0 §2.2](https://www.w3.org/TR/xml/#charsets) | Char production | Illegal codepoints to strip before serialisation |
| [quick-xml 0.37.5](https://docs.rs/quick-xml/0.37.5/quick_xml/) | `Writer`, `BytesText`, `BytesDecl` | `BytesText::new` auto-escapes the 5 entities; `BytesText::from_escaped` does NOT — never use the latter for user data |
| [axum 0.8 Body](https://docs.rs/axum/0.8.4/axum/body/struct.Body.html) | `from_stream` | `File::open` → `ReaderStream::new` → `Body::from_stream` canonical pattern |
| [image 0.25 DynamicImage](https://docs.rs/image/0.25.6/image/enum.DynamicImage.html) | `resize`, `write_to` | `write_to` requires `Write + Seek`; wrap `Vec<u8>` in `Cursor` |

### Client Compatibility Gotchas (from observed issues)

- **KyBook 3** rejects downloads served without `Content-Disposition: attachment` — always include. [kolyvan/kybook#438]
- **KyBook 3** rejects feeds with `Content-Type` other than `application/atom+xml` (profile/kind params optional for KyBook) — send the full parameterised type. [seblucas/cops#259]
- **Moon+ Reader** fails with "not well-formed" when `&` is unescaped — never call `BytesText::from_escaped` on user data. [Ubooquity forum v3.0.1-beta]
- **KOReader** mishandled `<?xml-stylesheet?>` PIs historically — do not emit any PI other than the XML declaration. [koreader#9372]

---

## Patterns to Mirror

**EXTRACTOR_SHAPE** (`BasicOnly` is a specialisation of `CurrentUser`):

```rust
// SOURCE: backend/src/auth/middleware.rs:26-103
// MIRROR: extract the Basic-auth branch only; skip the session branch;
// return AppError::BasicAuthRequired(realm) on failure.
impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // Try session cookie via axum-login (populated by AuthManagerLayer)
        if let Ok(auth_session) =
            <AuthCtx as FromRequestParts<AppState>>::from_request_parts(parts, state).await
            && let Some(u) = auth_session.user
        { /* … */ }

        // Fall back to Basic auth: username = user_id UUID, password = device token
        if let Some(auth) = parts.headers.get(axum::http::header::AUTHORIZATION)
            && let Ok(auth_str) = auth.to_str()
            && let Some(credentials) = auth_str.strip_prefix("Basic ")
        {
            use base64ct::Encoding;
            let mut buf = vec![0u8; credentials.len()];
            let decoded = base64ct::Base64::decode(credentials.as_bytes(), &mut buf)
                .map_err(|_| AppError::Unauthorized)?;
            /* … iterate every token, constant-time compare … */
        }
        Err(AppError::Unauthorized)
    }
}
```

**RLS_ACQUIRE** (every OPDS read handler must open one):

```rust
// SOURCE: backend/src/db.rs:44-54 (full function)
pub async fn acquire_with_rls(
    pool: &PgPool,
    user_id: uuid::Uuid,
) -> Result<sqlx::Transaction<'_, sqlx::Postgres>, sqlx::Error> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('app.current_user_id', $1::text, true)")
        .bind(user_id.to_string())
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

// SOURCE: backend/src/routes/metadata.rs:93-97 — caller shape
let mut tx = db::acquire_with_rls(&state.pool, current_user.user_id)
    .await
    .map_err(|e| AppError::Internal(e.into()))?;
// … queries against &mut *tx …
// Drop auto-rolls-back; call tx.commit() only when writing
```

**QUICK_XML_WRITER** (Atom feed construction):

```rust
// SOURCE: backend/src/services/writeback/opf_rewrite.rs:265-301
// MIRROR: BytesStart::new(name) + push_attribute((k,v)) + BytesText::new(text)
// + write_event pairs. Use BytesText::new (NOT from_escaped) for any DB-sourced text.
fn write_new_isbn_identifier(
    writer: &mut Writer<Cursor<Vec<u8>>>,
    isbn: &str,
) -> Result<(), WritebackError> {
    let mut start = BytesStart::new("dc:identifier");
    start.push_attribute(("opf:scheme", "ISBN"));
    writer.write_event(Event::Start(start))?;
    writer.write_event(Event::Text(BytesText::new(isbn)))?;
    writer.write_event(Event::End(BytesEnd::new("dc:identifier")))?;
    Ok(())
}
```

**EPUB_ZIP_READ** (extract cover bytes):

```rust
// SOURCE: backend/src/services/epub/cover_layer.rs:17-25
// MIRROR: OPF-relative path resolution + read_entry.
let opf_dir = opf.opf_path.rfind('/').map(|i| &opf.opf_path[..i]).unwrap_or("");
let entry_path = if opf_dir.is_empty() { href.clone() } else { format!("{opf_dir}/{href}") };
let Some(bytes) = zip_layer::read_entry(handle, &entry_path) else { return None };
```

**IMAGE_RESIZE** (cover thumbnail + full):

```rust
// SOURCE: backend/src/services/enrichment/cover_download.rs:183-189 (decode)
// + docs.rs/image/0.25.6 for resize
let img = image::load_from_memory(&bytes)
    .map_err(|e| CoverError::Decode(e.to_string()))?;
let resized = img.resize(max_edge, max_edge, image::imageops::FilterType::Lanczos3);
// write_to requires Write + Seek — wrap Vec in Cursor
let mut out: Vec<u8> = Vec::new();
resized.write_to(&mut std::io::Cursor::new(&mut out), detected_format)?;
```

**ATOMIC_WRITE** (cover cache populate):

```rust
// SOURCE: backend/src/services/enrichment/cover_download.rs:216-228
fn write_atomically(dir: &Path, filename: &str, data: &[u8]) -> Result<PathBuf, std::io::Error> {
    use std::io::Write;
    let tmp = tempfile::NamedTempFile::new_in(dir)?;
    let (mut file, tmp_path) = tmp.into_parts();
    file.write_all(data)?;
    file.flush()?;
    drop(file);
    let dest = dir.join(filename);
    tmp_path.persist(&dest).map_err(|e| e.error)?;
    Ok(dest)
}
```

**FILE_STREAM** (download handler — not present in codebase; build from scratch):

```rust
// DERIVED FROM: axum 0.8 docs + tokio-util 0.7 (already a dep)
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use axum::{body::Body, response::Response, http::header};

let file = File::open(&canonical_path).await.map_err(|_| AppError::NotFound)?;
let stream = ReaderStream::new(file);
let body = Body::from_stream(stream);
Response::builder()
    .header(header::CONTENT_TYPE, "application/epub+zip")
    .header(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{ascii_fallback}\"; filename*=UTF-8''{rfc5987}"),
    )
    .body(body)
    .map_err(|e| AppError::Internal(e.into()))?
```

**TEST_SCAFFOLD** (`#[sqlx::test]` + real router):

```rust
// SOURCE: backend/src/routes/metadata.rs:643-695 (pattern)
#[sqlx::test(migrations = "./migrations")]
async fn opds_root_returns_atom(pool: sqlx::PgPool) {
    let app_pool = test_support::db::app_pool_for(&pool).await;
    let ing_pool = test_support::db::ingestion_pool_for(&pool).await;
    let (_user_id, basic) = test_support::db::create_admin_and_basic_auth(&app_pool).await;
    let marker = Uuid::new_v4().simple().to_string();
    let (_work_id, _m_id) =
        test_support::db::insert_work_and_manifestation(&ing_pool, &marker).await;

    let server = test_support::db::server_with_real_pools(&app_pool, &ing_pool);
    let response = server.get("/opds").add_header(AUTHORIZATION, basic).await;
    assert_eq!(response.status_code(), StatusCode::OK);
    assert!(response.header(CONTENT_TYPE).to_str().unwrap()
        .starts_with("application/atom+xml"));
}
```

**TEST_EPUB_FIXTURE** (download + cover tests need a real EPUB on disk):

```rust
// SOURCE: backend/src/services/ingestion/orchestrator.rs:770-795
// Re-use / expose as test_support::make_minimal_epub_with_cover() — see Task 35.
fn make_minimal_epub() -> Vec<u8> {
    use std::io::Write as _;
    use zip::write::{ExtendedFileOptions, FileOptions};
    /* mimetype + META-INF/container.xml + OEBPS/content.opf */
}
```

---

## Files to Change

### New files

| File | Purpose |
|---|---|
| `backend/src/auth/basic_only.rs` | `BasicOnly` extractor; rejects session; 401 with WWW-Authenticate |
| `backend/src/routes/opds/mod.rs` | Route group; mounts all `/opds/*` under `BasicOnly` |
| `backend/src/routes/opds/feed.rs` | Atom feed builder (`FeedBuilder`, entry helpers, link helpers) |
| `backend/src/routes/opds/scope.rs` | Scope enum + SQL-fragment + bind-param helper; always `EXISTS visible` join |
| `backend/src/routes/opds/cursor.rs` | base64url cursor encode/parse; sort key `(created_at DESC, id DESC)` |
| `backend/src/routes/opds/xml.rs` | XML 1.0 invalid-character sanitiser |
| `backend/src/routes/opds/root.rs` | `GET /opds` handler |
| `backend/src/routes/opds/library.rs` | `/opds/library/{new,authors,authors/:id,series,series/:id,search}` handlers |
| `backend/src/routes/opds/shelves.rs` | `/opds/shelves{,/,:id,/…}` handlers (mirror of library under shelf scope) |
| `backend/src/routes/opds/opensearch.rs` | `/opds/library/opensearch.xml` + `/opds/shelves/:id/opensearch.xml` |
| `backend/src/routes/opds/download.rs` | `GET /opds/books/:id/file` — streamed EPUB download with canonicalisation guard |
| `backend/src/routes/opds/covers.rs` | Shared `serve_cover` handler, dual-mounted at `/opds/books/:id/cover{,/thumb}` (BasicOnly) and `/api/books/:id/cover{,/thumb}` (CurrentUser — session or Basic) |
| `backend/src/services/covers/mod.rs` | Public API: `get_or_create(manifestation_id, size) -> PathBuf`, `CoverSize` enum |
| `backend/src/services/covers/extract.rs` | Open EPUB, run container + OPF layers, find cover via manifest priority list, return bytes + format |
| `backend/src/services/covers/resize.rs` | Lanczos3 resize to thumb (300px long-edge) / full (1200px long-edge); preserve JPEG/PNG/WebP |
| `backend/src/services/covers/cache.rs` | On-disk cache under `${library_path}/_covers/cache/`; tempfile + atomic rename |
| `backend/src/services/covers/error.rs` | `CoverError` enum (NoCover, Decode, Zip, Io, UnsupportedFormat) |

### Updated files

| File | Change |
|---|---|
| `backend/src/auth/mod.rs` | Add `pub mod basic_only;` |
| `backend/src/auth/middleware.rs` | Skip `update_last_used` spawn when `last_used_at > now() - 5m` (Task 2). Expose a helper `verify_basic(state, parts) -> Result<CurrentUser, AppError>` that `BasicOnly` reuses (zero hash-logic duplication) |
| `backend/src/models/device_token.rs` | `update_last_used` gains a SQL predicate to debounce writes to one UPDATE per token per 5 minutes (Task 6b); unit-test added confirming `list_for_user` excludes revoked rows |
| `backend/src/services/epub/cover_layer.rs` | Promote `find_cover_href` from `fn` to `pub(crate) fn`; used by `services::covers::extract` |
| `backend/src/services/epub/mod.rs` | Ensure `ZipHandle` is `pub(crate)` or exposed through a helper usable by `services::covers` |
| `backend/src/error.rs` | Add `AppError::BasicAuthRequired { realm: String }` variant; `IntoResponse` emits 401 + `WWW-Authenticate: Basic realm="…", charset="UTF-8"` + empty body |
| `backend/src/routes/mod.rs` | Add `pub mod opds;` |
| `backend/src/main.rs` | `.merge(routes::opds::router())` and `.merge(routes::opds::covers_router())` (or single merged router) when `config.opds.enabled` |
| `backend/src/config.rs` | Add `OpdsConfig { enabled, page_size, realm, public_url }`; fail-fast when `enabled && public_url.is_none()` |
| `backend/src/state.rs` | No structural change — `config` already in `AppState` |
| `backend/src/test_support.rs` | Add `make_minimal_epub_with_cover()` helper; add `create_child_user_and_basic_auth`; add `create_shelf_with_manifestations` |
| `backend/.env.example` | Append the four `REVERIE_OPDS_*` entries + `REVERIE_PUBLIC_URL` |
| `backend/Cargo.toml` | Declare `url = "2"` and `percent-encoding = "2"` explicitly (both transitively available via `reqwest`/`openidconnect`; declare for directly-imported crates) |

### Test files

| File | Purpose |
|---|---|
| `backend/src/routes/opds/feed.rs` (mod tests) | Unit tests for `FeedBuilder`: namespace decls, absolute hrefs, XML-invalid char stripping |
| `backend/src/routes/opds/scope.rs` (mod tests) | SQL fragment shape tests per scope/role combination |
| `backend/src/routes/opds/cursor.rs` (mod tests) | encode → parse round-trip; malformed input rejection |
| `backend/src/routes/opds/mod.rs` (mod tests) | Integration tests 20–34 from BLUEPRINT Task List |
| `backend/src/services/covers/mod.rs` (mod tests) | Extract → resize → cache round-trip |

---

## NOT Building (Scope Limits)

Explicit exclusions to prevent scope creep:

- **OPDS 2.0 / JSON variant.** OPDS 1.2 Atom XML only. OPDS 2.0 is a separate spec with JSON-based feeds; not in MVP.
- **OAuth2 / PKCE for device tokens.** Basic auth with device token as password is the only mechanism. OIDC session is explicitly rejected on `/opds/*`.
- **LRU cache eviction or size cap** for `_covers/cache/`. Orphan cleanup deferred to Step 11.
- **Per-user preference for "library vs shelf" default scope.** Scope is URL-based; pair against the URL you want.
- **Cover upload / override UI.** Covers are extracted from the EPUB only; enrichment-sourced sidecar covers at `manifestations.cover_path` are NOT served here (they live on a different path; plan uses EPUB-embedded cover to match BLUEPRINT).
- **Facets** (`rel="http://opds-spec.org/facet"`). Blueprint does not require facets; MVP feeds are flat.
- **`opensearch:totalResults` / `startIndex` / `itemsPerPage`** in search response feeds. Optional per spec; skip for MVP.
- **HMAC-signed cursors.** Homelab trust model; cursor leaks timestamps that are already in the feed body.
- **Web reader.** Deferred to Phase 2.
- **Range-request support on `/opds/books/:id/file`.** axum's `Body::from_stream` over `ReaderStream` does not honour `Range` headers; most OPDS clients download whole files. Out of scope.
- **Content negotiation on `/opds/*`.** Every `/opds/*` route returns Atom XML; clients that send `Accept: application/json` still get Atom.

---

## Step-by-Step Tasks

Execute in order. Each task is atomic, independently verifiable, and validated immediately with `cargo check -p reverie-api` (fast) plus targeted tests where applicable. Final Level-3 run comes at the end.

### Phase A — Config + error surface

#### Task 1: UPDATE `backend/src/config.rs` — add `OpdsConfig`

- **ACTION**: Add `pub struct OpdsConfig { enabled: bool, page_size: u32, realm: String, public_url: Option<url::Url> }` and parse in `Config::from_env`.
- **DEFAULTS**: `enabled=true`, `page_size=50`, `realm="Reverie OPDS"`, `public_url` required when `enabled=true` (fail-fast).
- **FAIL_FAST**: if `enabled && public_url.is_none()` → `ConfigError::Invalid { var: "REVERIE_PUBLIC_URL", reason: "required when REVERIE_OPDS_ENABLED=true" }`.
- **VALIDATION**: `page_size` in `1..=500`.
- **URL PARSE**: `url::Url::parse(&s).map_err(|e| ConfigError::Invalid{ var: "REVERIE_PUBLIC_URL", reason: e.to_string() })`. Declare `url = "2"` in `backend/Cargo.toml` explicitly — it's transitively available via `openidconnect`/`reqwest`, but we never rely on a transitive dep for types we import directly.
- **MIRROR**: `backend/src/config.rs:126-140` — `match env::var(...).unwrap_or_else(...)` pattern.
- **VALIDATE**: `cargo check -p reverie-api`

#### Task 2: UPDATE `backend/.env.example`

- **ACTION**: Append
  ```text
  # OPDS catalog
  REVERIE_OPDS_ENABLED=true
  REVERIE_OPDS_PAGE_SIZE=50
  REVERIE_OPDS_REALM=Reverie OPDS
  REVERIE_PUBLIC_URL=http://localhost:3000
  ```
- **VALIDATE**: grep confirms lines present.

#### Task 3: UPDATE `backend/src/error.rs` — add `BasicAuthRequired`

- **ACTION**: Add variant `BasicAuthRequired { realm: String }` to `AppError`. In `IntoResponse`, emit status 401 with header `WWW-Authenticate: Basic realm="<realm>", charset="UTF-8"` and empty body (text/plain empty is acceptable per RFC 7617).
- **ESCAPING**: realm comes from trusted config — no escaping beyond basic quote-safety; reject realms containing `"` at startup (Task 1 validation).
- **VALIDATE**: `cargo check -p reverie-api && cargo clippy -p reverie-api -- -D warnings`.

#### Task 4: UPDATE `backend/src/test_support.rs`

- **ACTION**: Extend `test_config()` to include `OpdsConfig` (default `enabled=false` for non-OPDS tests; OPDS tests override).
- **VALIDATE**: existing tests still compile.

### Phase B — Auth

#### Task 5: CREATE `backend/src/auth/basic_only.rs`

- **ACTION**: Define `pub struct BasicOnly(pub CurrentUser)` implementing `FromRequestParts<AppState>`. Logic: ignore any session, require `Authorization: Basic`; on success return `BasicOnly(CurrentUser { ... })`; on failure return `AppError::BasicAuthRequired { realm: state.config.opds.realm.clone() }`.
- **REUSE**: call into a new `auth::middleware::verify_basic(state, parts)` helper extracted from existing `CurrentUser` (Task 6). Zero duplication of hashing or constant-time comparison.
- **REGISTER**: add `pub mod basic_only;` in `src/auth/mod.rs`.
- **MIRROR**: `backend/src/auth/middleware.rs:63-102`.
- **VALIDATE**: `cargo check -p reverie-api`.

#### Task 6: UPDATE `backend/src/auth/middleware.rs` and `backend/src/models/device_token.rs`

- **ACTION** (a): Extract the Basic-auth branch into `pub(crate) async fn verify_basic(state: &AppState, parts: &Parts) -> Result<CurrentUser, AppError>`. Both `CurrentUser` and `BasicOnly` call it.
- **ACTION** (b): Push the debounce into SQL so it's atomic under concurrent requests. Change `device_token::update_last_used` to predicate-filter on the 5-minute window:
  ```sql
  UPDATE device_tokens SET last_used_at = now()
  WHERE id = $1
    AND (last_used_at IS NULL OR last_used_at < now() - interval '5 minutes')
  ```
  Every authenticated request still fires one UPDATE; the predicate turns it into a no-op when a previous update landed within the window. No Rust-side policy, no `should_update_last_used` helper, no unit test for the policy branch — the SQL is the policy.
- **VALIDATE**: `cargo test -p reverie-api auth::middleware`.

### Phase C — OPDS feed primitives

#### Task 7: CREATE `backend/src/routes/opds/xml.rs`

- **ACTION**: `pub fn sanitise_xml_text(s: &str) -> String` strips any char NOT in the XML 1.0 `Char` production (`#x9 | #xA | #xD | [#x20–#xD7FF] | [#xE000–#xFFFD] | [#x10000–#x10FFFF]`).
- **IMPL**: `s.chars().filter(|&c| matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}')).collect()`.
- **UNIT_TESTS**: strip `\x01`; preserve tab, LF, CR; preserve emoji (`\u{1F600}`); preserve `<` and `&` unchanged (escaping is quick-xml's job).
- **VALIDATE**: `cargo test -p reverie-api opds::xml`.

#### Task 8: CREATE `backend/src/routes/opds/cursor.rs`

- **ACTION**: `pub struct Cursor { pub created_at: OffsetDateTime, pub id: Uuid }` with `encode(&self) -> String` (base64url of `"<RFC3339>|<uuid-hyphenated>"`) and `parse(s: &str) -> Result<Cursor, CursorError>`.
- **CRATES**: `base64ct` (already a dep) with `Base64UrlUnpadded`; `time` for RFC 3339.
- **UNIT_TESTS**: round-trip; reject malformed base64; reject payload missing `|`; reject bad RFC 3339; reject bad UUID.
- **VALIDATE**: `cargo test -p reverie-api opds::cursor`.

#### Task 9: CREATE `backend/src/routes/opds/scope.rs`

- **ACTION**: `pub enum Scope { Library, Shelf(Uuid) }`. Helper
  `pub fn push_scope(qb: &mut QueryBuilder<'_, Postgres>, scope: &Scope, manifestation_alias: &str)`
  that pushes SQL + bind slots directly into the caller's `QueryBuilder`. `Library` pushes nothing.
  `Shelf(uuid)` pushes literally
  `EXISTS (SELECT 1 FROM shelf_items si JOIN shelves s ON s.id = si.shelf_id WHERE si.manifestation_id = {alias}.id AND s.id = `
  followed by `qb.push_bind(*uuid)`, followed by
  ` AND s.user_id = current_setting('app.current_user_id', true)::uuid)`.
- **RATIONALE**: returning a `(String, Vec<Uuid>)` pair does NOT compose with `QueryBuilder::push_bind`'s managed placeholder numbering — the caller would have to hand-number the embedded `$N` to stay consistent with the other binds, which the search handler (`q` + shelf_id + cursor_ts + cursor_id + limit) would silently drift against. Pushing fragments + binds directly through the caller's builder keeps all numbering in one place and is the only pattern that scales.
- **VISIBLE_EXISTS**: provide
  `pub fn push_visible_manifestation(qb: &mut QueryBuilder<'_, Postgres>, scope: &Scope, parent_alias_for_work_id_column: &str)`
  that pushes `EXISTS (SELECT 1 FROM manifestations m WHERE m.work_id = {parent}.work_id`, then — when `scope != Library` — ` AND ` followed by a delegated `push_scope(qb, scope, "m")`, then `)`. Used on `works` / `authors` / `series` navigation feeds.
- **RLS_NOTE**: comment at top reminding readers the scope is ALWAYS applied inside `acquire_with_rls` — never a substitute for RLS.
- **UNIT_TESTS**: build a fresh `QueryBuilder`, call `push_scope(&mut qb, &Scope::Library, "m")`, assert the SQL buffer is unchanged; call again with `Scope::Shelf(uuid)` and assert the SQL contains `shelf_items` + `s.user_id = current_setting(...)` and that exactly one bind slot was consumed (check via `qb.into_sql()` rendered output).
- **VALIDATE**: `cargo test -p reverie-api opds::scope`.

#### Task 10: CREATE `backend/src/routes/opds/feed.rs`

- **ACTION**: Pure builder. Public API:
  - `pub struct FeedBuilder { writer: Writer<Cursor<Vec<u8>>>, base_url: Url, self_href: String, kind: FeedKind }`
  - `pub enum FeedKind { Navigation, Acquisition }`
  - `pub fn new(base_url: &Url, self_path: &str, kind: FeedKind, title: &str, updated: OffsetDateTime) -> Self` — writes XML decl + open `<feed>` with namespaces (`xmlns`, `xmlns:opds`, `xmlns:dc`, `xmlns:opensearch`) + `<id>urn:reverie:feed:{path}</id>` + `<title>` + `<updated>` + `<atom:author><atom:name>Reverie</atom:name></atom:author>` (feed-level author is required by RFC 4287 §4.1.1 unless every entry carries its own — navigation feeds don't, so emit unconditionally) + self/start/up links (configurable).
  - `pub fn add_navigation_entry(&mut self, id: &str, title: &str, href: &str, rel_subsection: bool)`
  - `pub fn add_acquisition_entry(&mut self, entry: &AcquisitionEntry)` where `AcquisitionEntry` carries manifestation_id, work title, creators, description, language, tags, dc:identifier (ISBN preferred, fallback UUID), updated_at, acquisition href, cover href, thumb href.
  - `pub fn add_next_link(&mut self, href: &str)` — acquisition feeds only.
  - `pub fn add_search_link(&mut self, opensearch_xml_href: &str)`
  - `pub fn finish(self) -> Vec<u8>` — writes `</feed>` + `into_inner().into_inner()`.
- **ABSOLUTE_URLS**: every href is `base_url.join(path)`. Internal helper `abs(path: &str) -> String`.
- **TEXT_SAFETY**: every DB-sourced string — whether rendered as a text node OR as an attribute value (including `<atom:category term="…" label="…"/>` term/label, entry `<id>` suffixes, `href` path segments built from mutable columns) — passes through `sanitise_xml_text` before reaching quick-xml. `BytesText::new` and `push_attribute` auto-escape the five-entity set (`& < > " '`) but do NOT strip XML 1.0 `Char`-production violations (control codepoints like `\x01`). Strict clients (Moon+, KyBook 3) reject a feed containing those, so the filter applies to attributes just as strictly as to text.
- **ENTRY_ID_FORMAT**: stable URN per entry source so OPDS client-side bookmarks survive feed regeneration:
  - manifestation (acquisition): `urn:reverie:manifestation:<uuid>`
  - author (navigation): `urn:reverie:author:<uuid>`
  - series (navigation): `urn:reverie:series:<uuid>`
  - shelf (navigation, root-feed entries): `urn:reverie:shelf:<uuid>`
  - subcatalog root (`/opds/library`, `/opds/library/new`, `/opds/shelves/:id`, etc.): `urn:reverie:feed:<path>` (mirrors the feed-level `<id>`)
- **UNIT_TESTS**:
  - namespace decls present on feed element.
  - entry id format matches the URN contract above (one test per source — manifestation, author, series, shelf, subcatalog root).
  - acquisition entry has all three rel links (acquisition + image + image/thumbnail) with correct `type` attrs.
  - text with `\x01` + `<` round-trips without `\x01` and with `&lt;`.
  - attribute value with `\x01` (e.g. `<atom:category term="foo\x01bar"/>`) round-trips without `\x01`.
- **VALIDATE**: `cargo test -p reverie-api opds::feed`.

#### Task 11: CREATE `backend/src/routes/opds/mod.rs`

- **ACTION**: Module root. Exports:
  - `pub fn router() -> Router<AppState>` — every `/opds/*` handler under `BasicOnly`, **including** `/opds/books/:id/cover{,/thumb}`. The OPDS feed emits cover URLs under this mount so Basic credentials stay within the paired protection space (RFC 7617 §2.2).
  - `pub fn covers_router() -> Router<AppState>` — `/api/books/:id/cover{,/thumb}` under `CurrentUser` (session OR Basic). Consumed by Step 10's web UI with a session cookie. Handler body is shared with the `/opds` mount per Task 23.
- **GATE**: `pub fn router_enabled(config: &OpdsConfig) -> Option<Router<AppState>>` — returns `None` when `!config.enabled` so `main.rs` can omit mounting the `/opds/*` tree. `covers_router()` is always mounted (see Task 24) because Step 10 needs it even when OPDS is disabled.
- **SUBMODULES**: `mod feed; mod scope; mod cursor; mod xml; mod root; mod library; mod shelves; mod opensearch; mod download; mod covers;`.
- **VALIDATE**: `cargo check -p reverie-api`.

### Phase D — Feeds

#### Task 12: CREATE `backend/src/routes/opds/root.rs`

- **ACTION**: `GET /opds` handler. Signature `async fn opds_root(BasicOnly(user): BasicOnly, State(state): State<AppState>) -> Result<Response, AppError>`.
- **BODY**: navigation feed with:
  - `rel="subsection"` entry → `/opds/library`
  - one entry per row of `SELECT id, name FROM shelves WHERE user_id = current_setting('app.current_user_id', true)::uuid ORDER BY name ASC` → `/opds/shelves/{id}`
  - `<link rel="search" type="application/opensearchdescription+xml" href="{base}/opds/library/opensearch.xml"/>`
- **RESPONSE**: `Content-Type: application/atom+xml;profile=opds-catalog;kind=navigation`.
- **VALIDATE**: integration test will assert.

#### Task 13: CREATE `backend/src/routes/opds/library.rs` and `shelves.rs` root handlers

- **ACTION**: `GET /opds/library` and `GET /opds/shelves/:shelf_id` each emit a navigation feed with entries for `new`, `authors`, `series` and a `rel="search"` link.
- **OWNERSHIP**: shelf root handler runs inside `acquire_with_rls` and executes `SELECT 1 FROM shelves WHERE id = $1 AND user_id = current_setting(...)::uuid LIMIT 1`; on empty → return `AppError::NotFound` (per BLUEPRINT: cross-user access returns 404, not 403).
- **VALIDATE**: integration tests.

#### Task 14: CREATE library/shelf acquisition + navigation handlers

Shared handlers parameterised by `Scope`. One handler per subcatalog:

- **`/new` (acquisition)**: `SELECT m.id, w.title, w.description, w.language, m.updated_at, m.isbn_13, m.isbn_10, w.id AS work_id FROM manifestations m JOIN works w ON w.id = m.work_id WHERE <push_scope(m)> AND (m.created_at, m.id) < ($cursor_ts, $cursor_id) ORDER BY m.created_at DESC, m.id DESC LIMIT $page_size + 1`. Trailing +1 row drives the `rel="next"` decision.
- **`/authors` (navigation)**: `SELECT a.id, a.name FROM authors a WHERE EXISTS (SELECT 1 FROM work_authors wa JOIN manifestations m ON m.work_id = wa.work_id WHERE wa.author_id = a.id AND <push_scope(m)>) ORDER BY a.sort_name ASC LIMIT $page_size + 1`.
- **`/authors/:id` (acquisition)**: books by author; same projection as `/new` + `AND w.id IN (SELECT wa.work_id FROM work_authors wa WHERE wa.author_id = $1)` + `push_scope(m)`.
- **`/series` (navigation)**: mirror authors pattern against `series` + `series_works`.
- **`/series/:id` (acquisition)**: ORDER BY `sw.position ASC NULLS LAST, m.created_at DESC, m.id DESC`.
- **`/search?q=` (acquisition)**: single-page — `WHERE w.search_vector @@ plainto_tsquery('english', $1) AND <push_scope(m)> ORDER BY ts_rank_cd(w.search_vector, plainto_tsquery('english', $1)) DESC, m.created_at DESC, m.id DESC LIMIT $page_size`. **No `rel="next"`**: a cursor cannot encode a `ts_rank` boundary without embedding the rank (leaks scoring internals, brittle across query changes), and OPDS clients overwhelmingly don't walk search results. Users see the top `$page_size` matches by relevance. Empty `q` → empty feed (not 500) per Moon+ quirk.
- **SQL_COMPOSITION**: every handler builds its query with `sqlx::QueryBuilder::<Postgres>::new(...).push(...).push_bind(...)` and calls `push_scope` / `push_visible_manifestation` (Task 9) to splice the scope predicate. All placeholder numbering is managed by `QueryBuilder`.
- **FOR EVERY HANDLER**: `db::acquire_with_rls(&state.pool, user.user_id)`; fetch creators for each manifestation with a single follow-up query `SELECT work_id, name FROM work_authors wa JOIN authors a ON a.id = wa.author_id WHERE wa.work_id = ANY($1::uuid[])` grouped in Rust; fetch tags similarly from `metadata_tags` if present, else omit.
- **VALIDATE**: integration tests.

#### Task 15: Acquisition entry — OPDS rel links

- **ACTION**: `FeedBuilder::add_acquisition_entry` emits:
  - `<link rel="http://opds-spec.org/acquisition" type="application/epub+zip" href="{base}/opds/books/{id}/file"/>`
  - `<link rel="http://opds-spec.org/image" type="{detected_mime}" href="{base}/opds/books/{id}/cover"/>`
  - `<link rel="http://opds-spec.org/image/thumbnail" type="{detected_mime}" href="{base}/opds/books/{id}/cover/thumb"/>`
  - `<dc:identifier>urn:isbn:{isbn}</dc:identifier>` (fall back to `urn:uuid:{manifestation_id}` when no ISBN).
  - `<dc:language>{language}</dc:language>` when `works.language IS NOT NULL`.
  - `<atom:summary type="text">{description}</atom:summary>` when non-null — text passes through `sanitise_xml_text` per Task 10.
  - `<atom:category term="{sanitised(tag)}" label="{sanitised(tag)}"/>` for tags (omit if none) — `term` and `label` are DB-sourced and must pass through `sanitise_xml_text` per Task 10's TEXT_SAFETY clause.
  - `<atom:author><atom:name>{sanitised(name)}</atom:name></atom:author>` per creator.
- **URL_MOUNT**: cover hrefs target `/opds/books/:id/cover{,/thumb}` (not `/api/*`) so OPDS clients' Basic credentials stay within the paired protection space per RFC 7617 §2.2 — see Task 23 for the dual-mount rationale.
- **MIME_DETECT**: cover image type is unknown until extraction; set `type="image/jpeg"` defensively on the feed side — the actual response Content-Type will be correct. (Spec allows the feed `type` attribute to be advisory; clients re-check on fetch.)
- **VALIDATE**: feed.rs unit tests.

### Phase E — OpenSearch

#### Task 16: CREATE `backend/src/routes/opds/opensearch.rs`

- **ACTION**: `GET /opds/library/opensearch.xml` + `GET /opds/shelves/:shelf_id/opensearch.xml`. Returns an `<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">` with `<ShortName>`, `<Description>`, and one `<Url type="application/atom+xml;profile=opds-catalog;kind=acquisition" template="{base}/opds/{library|shelves/{id}}/search?q={searchTerms}"/>`.
- **SHORTNAME_LIMIT**: 16 chars per OpenSearch spec. Use `"Reverie"` or `"Reverie Shelf"` (13 chars) — under the cap.
- **CONTENT_TYPE**: `application/opensearchdescription+xml`.
- **SHELF**: shelf variant runs the same ownership check as Task 13 (404 on foreign shelf).
- **VALIDATE**: integration tests 23.

### Phase F — Download

#### Task 17: CREATE `backend/src/routes/opds/download.rs`

- **ACTION**: `GET /opds/books/:manifestation_id/file` handler.
- **LOOKUP**: inside `acquire_with_rls`, `SELECT m.file_path, w.title FROM manifestations m JOIN works w ON w.id = m.work_id WHERE m.id = $1`. RLS denies unauthorised users → empty → return `AppError::NotFound`.
- **PATH_GUARD**: canonicalize both paths inside `spawn_blocking` to avoid blocking the async runtime. Match on `io::Error.kind()` explicitly — do NOT use `?`, because the existing `From<anyhow::Error>` for `AppError::Internal` would map `NotFound` to 500 and contradict the edge-case checklist below (line 709 "Path does not exist on disk → 404"):
  ```rust
  let canonical = match std::fs::canonicalize(&m.file_path) {
      Ok(p) => p,
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(AppError::NotFound),
      Err(e) => return Err(AppError::Internal(e.into())),
  };
  ```
  Same pattern applies to `fs::canonicalize(&state.config.library_path)` (though `library_path` must exist at startup — NotFound there is operator misconfiguration, map to `Internal`) and to `fs::metadata(&canonical)` for `Content-Length`. If `!canonical.starts_with(&library)` → return `AppError::Forbidden` (403).
- **STREAM**: `File::open` → `ReaderStream::new` → `Body::from_stream`. Status 200.
- **HEADERS**:
  - `Content-Type: application/epub+zip`
  - `Content-Disposition: attachment; filename="{ascii_fallback}"; filename*=UTF-8''{rfc5987}` — RFC 5987 encode work title; ASCII fallback is alnum + dash from title, fall back to `reverie-{short_uuid}.epub` if empty.
  - `Content-Length` from `fs::metadata(&canonical).len()` when available (apply the same ErrorKind-aware mapping).
- **MIRROR**: pattern under "FILE_STREAM" above.
- **VALIDATE**: integration test 31.

### Phase G — Covers service

#### Task 18: CREATE `backend/src/services/covers/error.rs`

- **ACTION**: `pub enum CoverError { NoCover, Decode(String), Zip(zip::result::ZipError), Io(std::io::Error), UnsupportedFormat(String) }` + `From` impls and `thiserror::Error`.

#### Task 19: CREATE `backend/src/services/covers/extract.rs`

- **ACTION**: `pub fn extract_cover_bytes(epub_path: &Path) -> Result<(Vec<u8>, image::ImageFormat), CoverError>`.
- **IMPL**: `tokio::task::spawn_blocking`-callable (synchronous). Read file into `Vec<u8>`, build `ZipHandle`, run `container_layer::validate`, `opf_layer::validate`, reuse `cover_layer::find_cover_href` (promote to `pub(crate)` if private). Match Step 5 detection semantics **exactly** — the four-id priority list `["cover-image", "cover", "Cover", "Cover-Image"]` and nothing else. Do NOT add an "any image/* manifest item" fallback: divergence from `cover_layer::validate` would have enrichment report "no cover" while OPDS reports "cover" for the same EPUB, which is a silent correctness hazard. Resolve OPF-relative path, `zip_layer::read_entry`. Detect format with `image::guess_format`. Return `CoverError::NoCover` when no cover present; never fall back to placeholder image per BLUEPRINT.
- **MIRROR**: `backend/src/services/epub/cover_layer.rs:8-48`.

#### Task 20: CREATE `backend/src/services/covers/resize.rs`

- **ACTION**: `pub fn resize_cover(bytes: &[u8], fmt: image::ImageFormat, size: CoverSize) -> Result<Vec<u8>, CoverError>` where `CoverSize::{Full, Thumb}` map to `1200px` / `300px` long-edge caps.
- **IMPL**: `image::load_from_memory_with_format(bytes, fmt)?.resize(cap, cap, FilterType::Lanczos3)` — `resize` preserves aspect ratio. Skip resize entirely when source is already under the cap. `write_to(&mut Cursor::new(&mut out), fmt)`.
- **FORMAT_PRESERVE**: JPEG → JPEG, PNG → PNG, WebP → WebP. Anything else (GIF, BMP) → `CoverError::UnsupportedFormat`.

#### Task 21: CREATE `backend/src/services/covers/cache.rs`

- **ACTION**: `pub struct CoverCache { root: PathBuf }` with `pub fn cached_path(&self, manifestation_id: Uuid, file_hash_prefix: &str, size: CoverSize, ext: &str) -> PathBuf` (builds `{root}/{uuid}-{hash16}-{size}.{ext}`) and `pub fn write(&self, key: &Path, bytes: &[u8]) -> Result<(), CoverError>` using tempfile + atomic rename.
- **DIR_INIT**: `fs::create_dir_all(&self.root)` on startup.
- **MIRROR**: `enrichment::cover_download::write_atomically`.

#### Task 22: CREATE `backend/src/services/covers/mod.rs`

- **ACTION**: Public API `pub async fn get_or_create(state: &AppState, manifestation_id: Uuid, user_id: Uuid, size: CoverSize) -> Result<PathBuf, CoverError>`.
- **FLOW**:
  1. `acquire_with_rls`; `SELECT file_path, current_file_hash FROM manifestations WHERE id = $1` — RLS denies → `NoCover` (caller maps to 404 unconditionally to avoid existence leak).
  2. Compute cache key with `current_file_hash[..16]`.
  3. If cache file exists → return its path. No mtime update needed.
  4. Else `spawn_blocking(move || extract_cover_bytes(&path))` → if `NoCover`, persist a zero-byte marker? No — just return error; response handler emits 404. (Marker files would complicate Step 11's orphan scan; keep it simple.)
  5. `resize_cover(bytes, fmt, size)`; write atomically; return path.
- **CONCURRENCY**: last-writer-wins on same key is benign (content identical per spec). No mutex.

#### Task 23: CREATE `backend/src/routes/opds/covers.rs`

- **ACTION**: dual-mount cover handlers so OPDS clients stay within their paired protection space (RFC 7617 §2.2) while Step 10's web UI uses the session-authed path:
  - `GET /opds/books/:manifestation_id/cover{,/thumb}` under `BasicOnly` — emitted in OPDS acquisition entries (Task 15). Mounted as part of `routes::opds::router()` (Task 11).
  - `GET /api/books/:manifestation_id/cover{,/thumb}` under `CurrentUser` (session OR Basic) — consumed by Step 10's web UI with a session cookie. Mounted as part of `routes::opds::covers_router()` (Task 11).
  Handler body is shared: a single `async fn serve_cover(user: CurrentUser, Path((id, size)): Path<(Uuid, CoverSize)>, State(state)) -> Result<Response, AppError>`. The `BasicOnly` mount wraps it with `|BasicOnly(u), p, s| serve_cover(u, p, s)`; the `CurrentUser` mount calls it directly. Zero logic duplication.
- **BODY**: call `covers::get_or_create`; on `Ok(path)` stream via the same `File::open` → `ReaderStream` → `Body::from_stream` pattern. Content-Type from file extension (`.jpg` → `image/jpeg`, `.png` → `image/png`, `.webp` → `image/webp`). On `Err(CoverError::NoCover)` → 404. Any other error → 500.
- **HEADERS**: `Cache-Control: no-store`. Rationale: the server-side on-disk cache is already content-addressed via `current_file_hash` (so Reverie itself never serves stale bytes after a Step 8 writeback), but a browser-side HTTP cache keys on URL alone — with a stable `/books/:id/cover` URL, any `max-age` would let clients display the pre-writeback cover for the duration of the cache window. Homelab-scale bandwidth cost of re-downloading is trivial (a few MB per catalog browse over LAN); never-stale is the simpler trade.

### Phase H — Wiring

#### Task 24: UPDATE `backend/src/routes/mod.rs` + `main.rs`

- **ACTION (routes/mod.rs)**: add `pub mod opds;`.
- **ACTION (main.rs)**: after building the base router:
  - `if let Some(opds) = routes::opds::router_enabled(&config.opds) { router = router.merge(opds); }` — mounts the full `/opds/*` tree (feeds + downloads + the `/opds/books/:id/cover{,/thumb}` cover mount) behind `BasicOnly`. Gated by `config.opds.enabled`.
  - `router = router.merge(routes::opds::covers_router());` — mounts `/api/books/:id/cover{,/thumb}` behind `CurrentUser`. **Always mounted** (no per-feature gate) because Step 10's web UI needs it even when `config.opds.enabled = false`. Document this in the `OpdsConfig` docstring.
- **VALIDATE**: `cargo check -p reverie-api`.

### Phase I — Test fixtures + integration tests

#### Task 25: UPDATE `backend/src/test_support.rs`

- **ACTION**: Add public helpers:
  - `pub fn make_minimal_epub_with_cover() -> Vec<u8>` — fork `orchestrator::tests::make_minimal_epub` to include a 2x2 JPEG at `OEBPS/cover.jpg` and a manifest item `<item id="cover-image" href="cover.jpg" media-type="image/jpeg"/>`.
  - `pub async fn create_child_user_and_basic_auth(app_pool: &PgPool, name: &str) -> (Uuid, String)` — inserts user with `role='child', is_child=TRUE` + a device token; returns `(user_id, "Basic …")`.
  - `pub async fn create_shelf(app_pool: &PgPool, user_id: Uuid, name: &str) -> Uuid`.
  - `pub async fn add_to_shelf(app_pool: &PgPool, shelf_id: Uuid, manifestation_id: Uuid)`.
- **NOTE**: shelves + shelf_items are `reverie_app`-role tables; always pass `app_pool`.

#### Task 26: Integration tests (BLUEPRINT Tasks 20–34)

Place under `backend/src/routes/opds/mod.rs` `#[cfg(test)] mod tests`. One `#[sqlx::test]` per criterion. Shared setup helpers live in `test_support`.

| # | Test name | Asserts |
|---|---|---|
| 20 | `root_feed_happy_path` | 200 + `application/atom+xml` + self/start links + `rel="subsection"` to `/opds/library` |
| 21 | `unauthenticated_returns_challenge` | 401 + `WWW-Authenticate: Basic realm="Reverie OPDS", charset="UTF-8"` byte-for-byte |
| 22 | `revoked_token_rejected` | set `device_tokens.revoked_at`, re-request with same token → 401 |
| 23 | `opensearch_descriptor_has_searchTerms` | GET `/opds/library/opensearch.xml` → parses → Url template contains `{searchTerms}`; same for shelf |
| 24 | `search_roundtrip` | seed 3 titles; `?q=<substring>` returns only matching entry |
| 25 | `child_sees_only_whitelisted_manifestations` | two shelves under one child, one mf per shelf; GET `/opds/library/new` returns exactly 2 entries; `/opds/library/authors` lists matching authors |
| 26 | `adult_shelf_scoped_feed` | adult, shelves A (3 books) + B (2 books); `/opds/shelves/A/new` → 3 entries; `/opds/shelves/B/new` → 2 entries |
| 27 | `cross_user_shelf_returns_404` | adult requests another user's shelf → 404 (not 403) |
| 28 | `pagination_walk_125` | seed 125 manifestations; walk `rel="next"` until absent; assert every id appears once |
| 29 | `xml_robustness_control_char` | title contains `<`, `&`, emoji, `\x01`; feed parses; title round-trips stripped of `\x01` and escaped |
| 30 | `search_reflection_xss_safe` | `?q=<script>alert(1)</script>`; response XML parses; contains `&lt;script&gt;` |
| 31 | `download_streams_and_path_traversal_403` | owning user → EPUB bytes + sha256 matches `current_file_hash`; `file_path` pointing outside `library_path` → 403; manifestation row whose `file_path` file is deleted from disk → 404 (not 500) |
| 32 | `cover_cache_populates_and_serves` | first GET at `/opds/books/:id/cover` populates on-disk cache file (inspect fs); second GET returns same bytes without re-zipping (assert no extract-layer tracing event, or compare content bytes); 404 when EPUB has no cover; response carries `Cache-Control: no-store`; same endpoint also served at `/api/books/:id/cover` and accepts session cookie OR Basic there |
| 33 | `cover_resize_size_caps` | thumb long-edge ≤ 300; full long-edge ≤ 1200; format preserved |
| 34 | _(removed — debounce moved to SQL)_ | Task 6b now pushes the 5-minute debounce into the `update_last_used` SQL predicate, so there is no Rust-side policy to unit-test. A DB-roundtrip test (fire `update_last_used` twice back-to-back, assert `last_used_at` didn't move the second time) could be added but is low-value: the single SQL statement is the policy. Skip for MVP. |

**PATH_GUARD_FIXTURE (Test 31 part 2)**: create a manifestation with `file_path` set to a symlink target outside `library_path`, or to an absolute path outside it. `acquire_with_rls` will still return the row (policies check user role, not path); the handler's `canonicalize` + `starts_with` check rejects with 403.

**TEST_EPUB_ON_DISK**: ingestion tests inject a tempdir; OPDS tests that exercise download need `fs::write(&path, make_minimal_epub_with_cover())` to the path they insert into `manifestations.file_path`. Use `tempfile::TempDir::new()` per test — no shared state.

---

## Testing Strategy

### Unit tests
- `routes/opds/cursor.rs` — encode/parse round-trip + reject malformed.
- `routes/opds/scope.rs` — `push_scope` / `push_visible_manifestation` SQL + bind-slot shape per scope.
- `routes/opds/feed.rs` — namespace decls, absolute URLs, rel strings, XML-char stripping in text AND attribute values, entry-id URN contract per source.
- `routes/opds/xml.rs` — sanitiser preserves/strips correct ranges.
- `services/covers/resize.rs` — size caps for each format.

### Integration tests (`#[sqlx::test]`)
See Task 26 table.

### Edge cases checklist
- [ ] Empty search query → empty feed, not 500 (Moon+ quirk).
- [ ] Shelf you don't own → 404 (scope fragment + `s.user_id = current_user` returns zero rows; handler returns NotFound).
- [ ] Revoked device token → 401 with challenge.
- [ ] XML-invalid control chars in `works.title` → stripped.
- [ ] EPUB with no cover item → 404 (no placeholder).
- [ ] EPUB cover that fails to decode → 500 (not 404 — the cover exists but is broken; don't mask).
- [ ] `file_path` outside `library_path` → 403.
- [ ] Path does not exist on disk (partial delete, etc.) → 404.
- [ ] `REVERIE_OPDS_ENABLED=true` without `REVERIE_PUBLIC_URL` → startup fails non-zero.
- [ ] `REVERIE_OPDS_ENABLED=false` → `/opds/*` returns 404 (router not mounted), `/api/books/:id/cover` still works.
- [ ] `realm` string containing `"` rejected at startup.
- [ ] Title-only search (no author index in `search_vector`) documented in test comment.
- [ ] Pagination cursor pointing to deleted row → treated as "before" boundary; next page starts from there; no crash.

---

## Validation Commands

Run from `backend/`. All commands must return exit 0.

### Level 1: Static analysis
```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

### Level 2: Unit tests
```bash
cargo test --lib opds
cargo test --lib covers
cargo test --lib auth::middleware
```

### Level 3: Full suite + build
```bash
cargo test
cargo build --release
```

### Level 4: Manual smoke (post-merge, against a live instance)
1. Start server with `REVERIE_PUBLIC_URL` set to a reachable host URL.
2. Create a device token via the web UI (Step 3 routes); record `user_id` + plaintext token.
3. `xh -a "{user_id}:{token}" GET localhost:3000/opds` → 200, navigation feed.
4. `xh -i GET localhost:3000/opds` → 401 with `WWW-Authenticate: Basic realm="Reverie OPDS", charset="UTF-8"`.
5. Pair KOReader + one of {Moon+, Librera, KyBook 3} at `http://host:3000/opds` — browse, search, download a book.
6. Pair a second catalog at `http://host:3000/opds/shelves/{shelf_id}` — verify feed is scoped to that shelf.
7. Confirm `${library_path}/_covers/cache/` populates on first browse; subsequent browses hit cache (no re-zip-open in tracing logs).

---

## Acceptance Criteria

From BLUEPRINT.md §"Exit Criteria":

- [ ] Root catalog navigable in KOReader **and** one of {Moon+, Librera, KyBook 3} when paired at `/opds`.
- [ ] Same clients pair at `/opds/shelves/{id}` and see only that shelf's content.
- [ ] Search works end-to-end via OpenSearch auto-discovery in both clients.
- [ ] Downloads deliver bytes with `Content-Type: application/epub+zip` and UTF-8-encoded `Content-Disposition` filename.
- [ ] Unauthenticated `/opds/*` requests return 401 with `WWW-Authenticate` challenge.
- [ ] Revoked device tokens rejected with 401.
- [ ] Child account feed at `/opds/library/*` yields only whitelisted manifestations (RLS).
- [ ] Adult shelf-scoped feed at `/opds/shelves/{id}/*` yields only that shelf's books; cross-user shelf access returns 404.
- [ ] Pagination walk of 125 manifestations visits each exactly once.
- [ ] Cover cache populated on first request; served from cache on subsequent requests; thumbnail and full-size within caps.
- [ ] Feed renders cleanly when a manifestation carries XML-invalid control characters in its title.
- [ ] Startup fails fast when `REVERIE_OPDS_ENABLED=true` without `REVERIE_PUBLIC_URL`.

---

## Completion Checklist

- [ ] All tasks completed in dependency order (Phase A → I).
- [ ] Level 1 (fmt + clippy -D warnings) passes.
- [ ] Level 2 (targeted unit tests) passes.
- [ ] Level 3 (full test suite + release build) passes.
- [ ] Level 4 manual smoke against live instance completed.
- [ ] All 15 acceptance criteria verified.
- [ ] `.env.example` + `plans/BLUEPRINT.md` line references match implementation.
- [ ] PR description explains URL-based scoping rationale + Basic-only auth decision.
- [ ] No regressions in existing metadata / enrichment / writeback test suites.

---

## Risks and Mitigations

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| `std::fs::canonicalize` blocks the async runtime under many concurrent downloads | MED | MED | Run inside `tokio::task::spawn_blocking`; alternatively accept it — downloads are already I/O-bound |
| Path-traversal guard false positives on exotic filesystems (case-insensitive macOS dev env) | LOW | LOW | Production is Linux; document macOS dev caveat |
| OPDS client ignores `charset="UTF-8"` and mis-decodes non-ASCII creds | MED | LOW | Client bug (documented in KOReader #13111); no server-side fix; document in README |
| `quick-xml` silently panics on invalid UTF-8 in an `&str` passed to `BytesText` | LOW | MED | Impossible — `&str` is guaranteed valid UTF-8; DB columns are TEXT (UTF-8 enforced by Postgres) |
| Cover cache fills disk over time (no LRU) | MED | LOW | Step 11 health dashboard will surface size; manual wipe is safe (regenerates on demand) |
| Cursor pointing at a deleted row produces surprising skip/repeat | LOW | LOW | BLUEPRINT explicitly documents stability-under-additions only; test 28 asserts walk termination, not stability-under-deletes |
| Port-50x-line-wrapping CI failures from feed bytes containing bare `\n` | LOW | LOW | `FeedBuilder` emits without indentation by default; indented mode gated behind `cfg(test)` only |
| `cover_embed` from Step 8 rewrites `current_file_hash` → cache keys turn over → brief uncached period on writeback-affected books | LOW | LOW | Intentional per BLUEPRINT; cache regenerates on next request |
| `manifestations.cover_path` (sidecar from enrichment) is ignored in favour of EPUB-embedded cover | LOW | MED | Intentional per BLUEPRINT architecture (§Cover cache). Sidecar serves a different purpose (enrichment preview UI). Document in `services/covers/mod.rs` module docstring |
| 401 response must emit `WWW-Authenticate` but existing `AppError::Unauthorized` returns JSON | HIGH (if overlooked) | HIGH | Dedicated `AppError::BasicAuthRequired { realm }` variant (Task 3); `BasicOnly` extractor uses ONLY this variant on failure |
| Cold-start memory spike under cover fanout: `zip_layer::validate` reads the whole EPUB into `Vec<u8>` on every cache miss. N concurrent first-view requests against 100 MB EPUBs → N × 100 MB transient allocation | MED | LOW | Cache amortises after first view per manifestation (write-once-read-many). If measured as dominant under real cold-start load, swap to a lighter cover-only ZIP open in `services/covers/extract.rs` that uses `File::open` + seek rather than pre-reading into `Vec<u8>` — does not affect the existing `zip_layer` used by ingestion |

---

## Notes

### Design decisions worth flagging for review

1. **BasicOnly vs CurrentUser.** We keep `CurrentUser` for cookie-or-Basic routes (covers, Step 10 API) and introduce `BasicOnly` for `/opds/*`. Both call into a shared `verify_basic` helper so hash-logic and constant-time comparison live in exactly one place. Alternative (a single extractor with a mode flag) considered and rejected: loses the Axum-extractor type-safety benefit.

2. **AppError::BasicAuthRequired.** Chosen over a separate `OpdsError` type. Rationale: `IntoResponse` already lives on `AppError`; adding one more variant with its own rendering branch is two lines of diff. A parallel error type would duplicate the whole `From<anyhow::Error>` + `IntoResponse` chain.

3. **URL-based scope, not per-token scope.** BLUEPRINT explicit: pair one device at the library URL, another at a shelf URL; no hidden state decides feed contents. Keeps the model auditable (`curl` shows exactly what the reader will see).

4. **Covers dual-mounted at `/opds/*` and `/api/*`.** The OPDS feed emits `/opds/books/:id/cover{,/thumb}` so Basic credentials stay within the paired protection space per RFC 7617 §2.2 (some mobile clients scope creds strictly to the paired prefix). Step 10's web UI hits `/api/books/:id/cover{,/thumb}` with a session cookie. Handler body is shared between both mounts — the two routes differ only in the extractor. Downloads remain `/opds/*` only (web UI doesn't download EPUBs directly in MVP).

5. **No HMAC on cursors.** Homelab trust model; cursor timestamps already leak via the feed body. Un-signed base64url is simpler and no less secure.

6. **`image::imageops::FilterType::Lanczos3`** over `thumbnail()`: Lanczos3 is ~3× slower than Triangle but produces visibly better covers. Since the cache is write-once-read-many, the one-time cost is acceptable. Revisit if profile shows resize dominating p99 latency.

7. **EPUB-embedded cover, not enrichment sidecar.** Blueprint explicit. The sidecar at `manifestations.cover_path` may differ from the EPUB-embedded cover (enrichment may have pulled a higher-res cover); surfacing sidecar covers in OPDS would require an orthogonal decision about which one is "the cover" — out of scope.

8. **No range-request / resumable download.** `Body::from_stream` + `ReaderStream` does not honour `Range`. OPDS clients overwhelmingly GET full files. Acceptable for MVP; if a client breaks on large files, revisit with `tower-http::services::ServeFile` behind an auth layer.

9. **Omitting OPDS navigation-feed pagination.** Spec says `rel="next"` is defined for acquisition feeds only. Navigation feeds (`/authors`, `/series`, `/shelves`) load fully into memory. If a library has >50k authors this will need revisiting; Step 11 dashboard will surface the row count.

10. **`sanitise_xml_text` is applied uniformly.** Anywhere a string from `works`/`authors`/`series`/`metadata_tags` reaches quick-xml — whether as a text node (`BytesText::new`) or as an attribute value (`push_attribute`, e.g. `atom:category term`/`label`) — `sanitise_xml_text` is called first. Simpler than scanning ingestion paths to keep control codepoints out of the DB, and quick-xml's five-entity escaping does NOT cover XML 1.0 `Char`-production violations.

11. **SQL-side debounce on `update_last_used`.** The 5-minute "don't re-update" rule lives in the `WHERE` predicate of the UPDATE itself, not in a Rust-side `should_update_last_used` helper. Atomic under concurrent polling, no branch to unit-test, one source of truth. Slight cost: every authenticated request still fires an UPDATE (mostly a no-op). Trivial at homelab scale.

12. **`Cache-Control: no-store` on covers.** The server's on-disk cache is already content-addressed via `current_file_hash`, so Reverie itself never serves stale bytes. A browser-side HTTP cache, though, keys on URL alone — any `max-age` on a stable URL would let clients display pre-writeback covers until the cache window elapses. The bandwidth cost of re-downloading (~5 MB per catalog browse over LAN) is unmeasurable at homelab scale; the staleness risk is user-visible. Trade-off tilts toward never-stale. Revisit with URL fingerprinting (`?v={hash16}`) if bandwidth ever matters.

### Future considerations

- OPDS 2.0 JSON feeds (separate spec; consumers exist but minority today).
- Web reader (Phase 2).
- Facets (`rel="http://opds-spec.org/facet"`) for read/unread state once reading positions are surfaced.
- `opensearch:totalResults` / `itemsPerPage` once search UX actually needs them.
- LRU cache eviction under Step 11.
