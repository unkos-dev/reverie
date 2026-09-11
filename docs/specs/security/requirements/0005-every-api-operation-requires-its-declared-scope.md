---
type: REQ
profile-version: 1
id: "REV-REQ-0005"
title: "Every API operation requires its declared scope"
governed-by:
  - "REV-ADR-0028"
---

# Every API operation requires its declared scope

## Statement

Every operation under `/api/v1` MUST refuse a request unless the request's credential carries the scope the operation
declares, or a higher scope in the order `read` < `write` < `admin`.

## Rationale

A narrowed credential, such as a read-only automation token, is a guarantee only if every operation checks the scope it
declares, and checks it at the boundary between adjacent levels. A scope declared in the API contract but not enforced,
or enforced one level looser than declared, would give that credential capability its owner never granted.

## Acceptance criteria

- Every `/api/v1` operation in the generated OpenAPI document declares a non-empty scope requirement. Checked by
  `every_api_v1_op_declares_a_scope` in `backend/src/authz_matrix.rs`.
- A credential holding a scope exactly one level below an operation's declared scope is refused with `403`, for every
  gated operation and for both device-token and JWT credentials. Checked by `scope_grid_enforces_the_hierarchy` and
  `jwt_scope_grid_enforces_the_hierarchy`.
- A credential holding the declared scope, or a higher one, is not refused on scope grounds. Checked by the positive
  controls in the same two tests.
- A credential carrying no scope is refused with `401` at authentication for every `/api/v1` operation, and no operation
  runs. Checked by `scopeless_token_rejected_by_every_op`, which writes an empty-scope token directly through the model
  layer so the check covers the authentication step itself.
