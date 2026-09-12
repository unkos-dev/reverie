---
type: REQ
profile-version: 1
id: "REV-REQ-0016"
title: "The browser client surfaces every non-2xx response as an error with its status"
---

# The browser client surfaces every non-2xx response as an error with its status

## Statement

WHEN the browser client receives, for an API call it made, a response whose status is outside the 200-299 range, it MUST
surface an error carrying that status, regardless of whether the response body is a Problem Details document, some other
JSON, non-JSON text, or absent.

## Rationale

The underlying browser `fetch` mechanism resolves for any HTTP response, success or failure alike, and does not itself
distinguish a 2xx status from a 4xx or 5xx one. A caller that skipped this distinction would treat a failed request as
if it had succeeded whenever the response happened to be readable at all, and a caller that trusted the response body to
always be well-formed Problem Details would break on the first proxy error page, timeout page, or truncated response it
met. Surfacing the status unconditionally, independent of the body's shape, is what lets every other caller branch on
"did this succeed" without separately handling every way a failure response can be malformed.

## Acceptance criteria

- A response with a well-formed Problem Details body surfaces an error carrying the response's status and the body's
  `type`, `title` and `detail`. Checked by `"401 unauthorized → ApiError with parsed type/title/detail"` in
  `frontend/src/api/fetch.test.ts`.
- A response with a Problem Details body reporting a server-side failure surfaces an error carrying that status and
  title. Checked by `"500 internal → ApiError with title='Internal Server Error'"` in the same file.
- A response whose body is not JSON at all still surfaces an error carrying the response's real status, falling back to
  the response's status text rather than failing to produce an error. Checked by
  `"non-JSON error body falls back to status-text title"` in the same file.
