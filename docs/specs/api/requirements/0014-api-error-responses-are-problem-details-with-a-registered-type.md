---
type: REQ
profile-version: 1
id: "REV-REQ-0014"
title: "API error responses are Problem Details with a registered type"
governed-by:
  - "REV-ADR-0011"
---

# API error responses are Problem Details with a registered type

## Statement

WHEN a request to an operation under `/api/v1`, or a request under `/api/v1` matching no registered operation, is
answered with a status in the 4xx or 5xx range, the response MUST carry the `application/problem+json` media type and a
JSON body whose `type` field is a URI ending in one of a fixed, closed set of registered problem-type slugs (for example
`.../not-found`, `.../validation`, `.../internal`).

## Rationale

A caller that branches on error type needs `type` drawn from a closed, predictable set rather than free-form text, so it
can match a known problem class and treat anything else as unrecognised.
[RFC 9457](https://www.rfc-editor.org/rfc/rfc9457) §3.1.1 makes `type` the mechanism for this: the URI identifies the
problem type and need not dereference, but it must be stable enough to use as a lookup key. A per-handler, ad hoc `type`
string would defeat that: every consumer, including Reverie's own browser client, would have to fall back to matching on
`status` and free-text `detail` alone.

## Acceptance criteria

- Every sampled failure response from an `/api/v1` operation carries `Content-Type: application/problem+json` and a
  `type` field ending in one of the registered slugs (`not-found`, `unauthorized`, `forbidden`, `validation`,
  `csrf-missing`, `csrf-mismatch`, `if-match-required`, `if-match-mismatch`, `system-shelf-immutable`,
  `malformed-query`, `malformed-header`, `malformed-path`, `invalid-request-body`, `rate-limited`,
  `setup-already-complete`, `email-conflict`, `method-not-allowed`, `internal`). Checked per problem class in
  `backend/src/error/mod.rs`'s test module, for example `not_found_returns_404_problem`,
  `csrf_missing_returns_428_problem`, `method_not_allowed_returns_405_problem` and
  `internal_returns_500_without_leaking_details`.
- A request under `/api/v1` matching no registered operation answers the same way, not with a generic or
  framework-default not-found body. Checked by `unmatched_api_route_returns_problem_with_instance` in
  `backend/src/lib.rs`.
- The `type` value is always the fixed base URI followed by the slug, never a bare slug or a different host per
  occurrence. Checked for two representative slugs by `problem_type_assembles_full_uri` in
  `backend/src/error/problems.rs`.
