---
type: REQ
profile-version: 1
id: "REV-REQ-0019"
title: "The browser client echoes a resource's last ETag as If-Match on its PATCH"
---

# The browser client echoes a resource's last ETag as If-Match on its PATCH

## Statement

WHEN the browser client sends a `PATCH` request for a book's metadata or for a book's reading state and the caller did
not already set an `If-Match` header on that request, the client MUST send the last `ETag` value it received for that
resource as `If-Match`, and MUST send no `If-Match` header when it holds no such value for that resource.

## Rationale

[RFC 9110 §13.1.1](https://www.rfc-editor.org/rfc/rfc9110#section-13.1.1) defines `If-Match` as a precondition the
client attaches to guard a write against a change it has not seen; the client can only supply a value it captured from a
prior response. Automating the capture and replay removes the need for every call site that issues a `PATCH` to thread
the header by hand, so a call site cannot forget to protect a write it has the means to protect, while a caller that
explicitly sets its own `If-Match` is never overridden.

## Acceptance criteria

- A `PATCH` for a resource follows a `GET` of that same resource: the `PATCH` request carries the `ETag` value the `GET`
  response carried, as `If-Match`. Checked by "a GET response's ETag is echoed as If-Match on that resource's PATCH" in
  `frontend/src/api/fetch.test.ts`.
- After a successful `PATCH` on a resource, a further `PATCH` on that same resource carries the `ETag` value the
  successful `PATCH`'s own response carried, not the one from an earlier response. Checked by "a successful PATCH's own
  ETag replaces the retained tag for the next PATCH" in `frontend/src/api/fetch.test.ts`.
- After a `412` response on a resource, a further `PATCH` on that same resource carries the current `ETag` value the
  `412` response carried, not the stale value that caused the mismatch. Checked by "a 412's current ETag replaces the
  stale retained tag" in `frontend/src/api/fetch.test.ts`.
- A `PATCH` for a resource the client has not previously seen an `ETag` for carries no `If-Match` header at all. Checked
  by "a resource with no retained tag PATCHes without If-Match" in `frontend/src/api/fetch.test.ts`.
- A last-seen `ETag` for one resource is never sent as `If-Match` on a `PATCH` to a different resource, even when both
  are in flight in the same session. Checked by "a retained tag on a manifestation's metadata resource is not sent for
  an unrelated shelves PATCH" in `frontend/src/api/fetch.test.ts`.
