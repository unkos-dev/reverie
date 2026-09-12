---
type: REQ
profile-version: 1
id: "REV-REQ-0013"
title: "Reserved-prefix paths never fall through to the SPA index"
---

# Reserved-prefix paths never fall through to the SPA index

## Statement

WHEN a request matches no configured route and its path, taken exactly as received without percent-decoding, equals
`/api`, `/auth`, `/health` or `/opds` or begins with one of them followed by `/`, the server MUST NOT answer with the
single-page application's `index.html` and MUST answer with a `404 Not Found` Problem Details document.

## Rationale

An unmatched request under one of these paths is far more likely to come from a stale or misconfigured program, such as
a script, a device-token client or an OPDS reader, than from a browser navigation the application's router should
handle. Answering it with `index.html` would hand a caller that expects machine-readable responses an HTML page it
cannot read as the error it is.

## Acceptance criteria

- A `GET` to exactly `/api`, `/auth`, `/health` or `/opds` that matches no route returns `404 Not Found` with
  `Content-Type: application/problem+json`, not `index.html`.
- A `GET` to one of those prefixes followed by `/` and an unmatched sub-path, for example `/api/v1/__nope__` or
  `/auth/__nope__`, returns the same `404` Problem Details response.
- A `GET` to a path that shares only a leading string with a reserved prefix, without the following `/`, for example
  `/apiology` or `/authed`, is outside this obligation and may be answered as an application route.

## More information

- The obligation binds the raw path as received. A percent-encoded path whose decoded form would fall under a reserved
  prefix, but whose raw form does not, is not covered.
- A request that matches a route under a reserved prefix with an unsupported method gets `405 Method Not Allowed` on
  that route and is outside this obligation.
- The sub-path case is covered by request tests. The bare-prefix case is checked through `is_reserved_prefix` in
  isolation, not through a request.
