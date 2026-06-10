---
severity: low
surfaces: [end-user, developer]
adopted: 2026-06-10
adopted-because: "recognised during UNK-376 series+dashboard OpenAPI coverage planning; pre-existing since the query handlers were written. Only library::list maps query-param rejection to application/problem+json; sibling endpoints emit axum's default plaintext 400"
lift-when-class: internal-refactor
lift-when: every query-param validation failure returns application/problem+json (RFC 9457) — achieved by switching the plain-Query handlers (library::search, dashboard::activity, all routes/opds/* query handlers) to Result<Query<_>, QueryRejection> + the existing From<QueryRejection> for AppError, or by adding a global query-rejection→Problem layer; then each affected OpenAPI operation documents 400 → ProblemDetails
---

# Inconsistent query-parameter error envelope (plaintext 400 vs problem+json)

## Constraint

Reverie's standard HTTP error envelope is RFC 9457 `application/problem+json`
(emitted by `AppError`). For malformed **query parameters**, that envelope is
produced only when a handler extracts `Result<Query<T>, QueryRejection>` and
relies on `From<QueryRejection> for AppError` (`backend/src/error/mod.rs:256`)
→ `AppError::MalformedQuery` → `400` problem+json.

Only `library::list` (`routes/library/mod.rs:151`) does this. The sibling query
handlers — `library::search`, `dashboard::activity`, and every `routes/opds/*`
feed handler — extract a plain `Query<T>`, so a malformed query param (e.g.
`?limit=abc`) yields **axum's default plaintext `400`**, not the problem+json
envelope every other error path returns. The API therefore advertises one error
contract but breaks it for input-validation errors on most query endpoints.

## Workaround

The OpenAPI coverage PRs (UNK-376) document only the responses each handler
actually emits through its own envelope, so these endpoints' specs carry **no**
`400` (documenting `400 → ProblemDetails` would misrepresent the plaintext wire;
documenting it bodyless adds noise). The inconsistency is surfaced, not papered
over — and not fixed, because unifying the envelope is a runtime behaviour change
out of scope for a doc-only coverage PR.

## Lift trigger

Switch the plain-`Query<T>` handlers (`library::search`, `dashboard::activity`,
`routes/opds/*`) to `Result<Query<T>, QueryRejection>` + `?` (reusing the existing
`From<QueryRejection>` impl), **or** add a global query-rejection→`Problem` layer.
Then add `(status = 400, … body = ProblemDetails)` to each affected operation's
`#[utoipa::path]` and assert it in the spec-coverage tests. Standards backing:
RFC 9457 (single machine-readable error envelope) + the consistency principle
(Zalando RESTful API Guidelines MUST-226). No Linear ticket yet — file when scheduled.
