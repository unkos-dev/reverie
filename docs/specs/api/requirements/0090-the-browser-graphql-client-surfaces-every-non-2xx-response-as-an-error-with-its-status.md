---
type: REQ
profile-version: 1
id: "REV-REQ-0090"
title: "The browser GraphQL client surfaces every non-2xx response as an error with its status"
---

# The browser GraphQL client surfaces every non-2xx response as an error with its status

## Statement

WHEN the browser GraphQL client receives a response to `/api/graphql` whose status is outside the 200-299 range, it MUST
surface an error carrying that status, whether the body is a Problem Details document, some other JSON, non-JSON text,
or absent. For a well-formed Problem Details body, the error MUST also carry the body's `type`, `title` and `detail`,
and its `errors` object when present.

## Rationale

The browser `fetch` mechanism resolves for any HTTP response, and a proxy error page or a truncated body can arrive in
place of Problem Details. A GraphQL client that read such a response as a GraphQL envelope would report a transport
fault as a malformed execution result, or publish nothing and report nothing. Surfacing the status, independent of the
body's shape, keeps transport faults separate from execution faults.

## Acceptance criteria

- Responses with status 422 and a Problem Details body, 502 with a text body, 503 with no body, and 409 with other JSON
  each surface an error carrying their status. Checked by
  `"GraphQL network preserves failures, CSRF recovery and account isolation"` in
  `frontend/tests/browser/api/graphql.test.tsx`.
- A 422 Problem Details body surfaces its `type`, `title`, `detail` and `errors` unchanged. Checked by the same test.
