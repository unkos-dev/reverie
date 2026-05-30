---
status: lifted
severity: low
surfaces: [developer, end-user]
adopted: 2026-05-23
adopted-because: "11b (PR #314) ships filter query params parsed by axum_extra::extract::Query; malformed UUIDs trigger framework default rejection (400 plain text) instead of RFC 7807 error shape"
lift-when-class: internal-refactor
lift-when: PR implements From<QueryRejection> for AppError so malformed filter UUIDs return 400 with RFC 7807 body; same PR adds tests for ?author=garbage, ?series=garbage, ?shelf=garbage
lifted: 2026-05-30
superseded-by: https://github.com/unkos-dev/reverie/pull/380
---

# Malformed UUID in filter query params returns non-RFC 7807 error

## Constraint

Filter query parameters (`?author=`, `?series=`, `?shelf=`) expect
UUID values. When a malformed UUID is passed, `axum_extra::extract::Query`
deserialization fails and emits the framework's default rejection: a
400 response with plain text body. All other error paths in the API
return RFC 7807 `application/problem+json`.

This is the same inconsistency as the `Path<Uuid>` rejection path
(see `detail_endpoint_malformed_uuid_returns_400` test), but for
query params.

## Workaround

None — the inconsistent error shape ships as-is. Clients receiving
400 on malformed filter params get plain text instead of structured
JSON. Functional behaviour (rejecting bad input) is correct; only
the error envelope shape is wrong.

## Lift trigger

Implement `From<QueryRejection> for AppError` (or a similar
extractor override) so query param validation failures route through
the same RFC 7807 error path as all other API errors.
