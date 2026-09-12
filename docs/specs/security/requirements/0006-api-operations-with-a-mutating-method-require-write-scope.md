---
type: REQ
profile-version: 1
id: "REV-REQ-0006"
title: "API operations with a mutating method require write scope"
governed-by:
  - "REV-ADR-0028"
---

# API operations with a mutating method require write scope

## Statement

WHEN an `/api/v1` operation uses the `POST`, `PUT`, `PATCH` or `DELETE` method, it MUST refuse any credential that does
not carry at least `write` scope, unless the operation is `POST /api/v1/manifestations/{id}/enrichment/dry-run`.

## Rationale

`read` is the floor every valid credential carries, and the mutating methods are how the API changes stored data. If an
operation using one of them accepted `read`, a credential its owner narrowed to read-only could make changes anyway. The
one exception, the enrichment dry run, lets a `read` credential have Reverie call the configured metadata providers and
write their responses to `api_cache`; it changes no manifestation.

## Acceptance criteria

- Every `/api/v1` operation using `POST`, `PUT`, `PATCH` or `DELETE` declares `write` or higher, except the operations
  on `METHOD_LINT_ALLOWLIST`, which holds only the enrichment dry run. Checked by
  `mutating_verb_ops_require_write_scope` in `backend/src/authz_matrix.rs`, which also fails when an allow-list entry
  matches no operation, so the list holds no stale route. The check reads route paths only; that the allow-listed
  operation still writes no manifestation, metadata version or writeback job is held by review.
- A credential carrying only `read` is refused with `403` by every operation declared at `write` or higher, for both
  device-token and JWT credentials. Checked by `read_token_blocked_from_every_mutation` and
  `jwt_read_scope_claim_blocked_from_every_mutation`.
- `POST /api/v1/manifestations/{id}/enrichment/dry-run` declares `read` and is reachable with a `read` credential.
