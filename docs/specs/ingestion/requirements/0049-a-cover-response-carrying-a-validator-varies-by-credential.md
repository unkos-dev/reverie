---
type: REQ
profile-version: 1
id: "REV-REQ-0049"
title: "A cover response carrying a validator varies by credential"
governed-by:
  - "REV-ADR-0025"
---

# A cover response carrying a validator varies by credential

## Statement

WHEN a cover endpoint returns a response that carries a cache validator, that response MUST carry
`Vary: Authorization, Cookie`, so that no shared cache serves one caller's cover in reply to a different caller's
request for the same URL.

## Rationale

Covers are scoped by row-level security, and the same cover URL can be a successful response for one account and a
not-found for another. A private browser cache that ignored credential would replay a cover a first account viewed to a
second account sharing the same browser, disclosing a cover its own permissions would hide. Partitioning the cache by
`Authorization` and `Cookie` keeps the caching benefit within one credential while closing that replay.

## Acceptance criteria

- A successful cover response carries `Cache-Control: private, max-age=86400`, a strong, quoted `ETag`, and
  `Vary: Authorization, Cookie`. Checked by `capability_cover_route_headers_authorisation_and_conditional_open` in
  `backend/src/routes/opds/tests.rs`.
- A `304 Not Modified` response to a matching `If-None-Match` carries `Vary: Authorization, Cookie` and an empty body.
  Checked by `capability_cover_route_headers_authorisation_and_conditional_open` in `backend/src/routes/opds/tests.rs`,
  which asserts the `Cache-Control` and `Vary` headers and the empty body on the `304`.
- The `404` returned when a cover's backing file is absent from disk carries `Cache-Control: private, max-age=60` and
  `Vary: Authorization, Cookie`. Checked by `capability_cover_route_headers_authorisation_and_conditional_open` in
  `backend/src/routes/opds/tests.rs`.
- The `404` returned when a manifestation declares no cover, is hidden by row-level security, or has its archive
  rejected carries neither a `Cache-Control` header nor a `Vary` header; that response shape is uncached by omission
  rather than partitioned by `Vary`. Not checked by any automated test.
