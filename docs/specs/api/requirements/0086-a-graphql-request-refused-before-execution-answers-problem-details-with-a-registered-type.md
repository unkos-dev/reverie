---
type: REQ
profile-version: 1
id: "REV-REQ-0086"
title: "A GraphQL request refused before execution answers Problem Details with a registered type"
---

# A GraphQL request refused before execution answers Problem Details with a registered type

## Statement

WHEN the GraphQL endpoint refuses a request to `/api/graphql` with a status in the 4xx or 5xx range before execution
begins, the response MUST carry the `application/problem+json` media type and a JSON body whose `type` field is a URI
ending in one of the registered problem-type slugs listed in REV-REQ-0014.

## Rationale

A refusal before execution, such as a missing credential, a missing CSRF token or a body that does not decode, never
reaches the GraphQL executor, so it has no GraphQL response envelope to carry it. The browser client handles these
refusals through the same Problem Details path it uses for the REST API: CSRF recovery matches on `type`, and the
unauthenticated handler matches on status. A refusal in any other format would bypass that path and reach the caller as
an unrecognised failure.

## Acceptance criteria

The checks below are made by `graphql_request_boundary` in `backend/src/graphql/mod.rs`. Each refusal carries
`Content-Type: application/problem+json` and the `type` named.

| Refusal                                                 | Status | `type` ends in         |
| ------------------------------------------------------- | ------ | ---------------------- |
| No credential, or an invalid bearer credential          | 401    | `unauthorized`         |
| A browser session request without a CSRF token          | 428    | `csrf-missing`         |
| A browser session request with the wrong CSRF token     | 403    | `csrf-mismatch`        |
| A request body that fails to decode, as in REV-REQ-0087 | 4xx    | `invalid-request-body` |
| A document ID absent from the trusted-document manifest | 422    | `validation`           |

- Each refusal in the table carries its media type and `type`.
