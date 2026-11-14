---
type: REQ
profile-version: 1
id: "REV-REQ-0079"
title: "Production GraphQL document admission"
governed-by:
  - "REV-ADR-0077"
---

# Production GraphQL document admission

## Statement

In production, the GraphQL server MUST execute only build-generated trusted documents from the manifest packaged with
its matching frontend operations. It MUST NOT execute or register arbitrary submitted query text or unknown document
IDs, including requests from cached clients.

## Rationale

An authenticated caller can still submit an expensive or unintended operation. A fixed set of reviewed operations bounds
the executable documents independently of the caller's permissions. Matching release artefacts keep browser operation
identifiers resolvable without runtime registration.

## Acceptance criteria

- Production executes known document IDs and refuses unknown IDs or arbitrary submitted text without registration.
  Variables still undergo schema, scalar and domain validation. Checked by `graphql_trusted_documents`.
- Production disables schema introspection and development retains it. Normal Relay `__typename` selections remain
  valid. Authentication, scope, CSRF, field visibility and execution limits apply independently of document admission.
  Checked by `graphql_trusted_documents` and `graphql_request_boundary`.
- Released frontend operations and the embedded server manifest agree. Generation and drift checks detect stale, missing
  or extra output, and packaging carries the matching manifest. Cached clients never enable raw-query fallback or
  runtime registration. Checked by the generation and drift lanes and `graphql_trusted_documents`.
