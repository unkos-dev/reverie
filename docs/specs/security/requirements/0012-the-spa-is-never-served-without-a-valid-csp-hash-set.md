---
type: REQ
profile-version: 1
id: "REV-REQ-0012"
title: "The SPA is never served without a valid CSP hash set"
governed-by:
  - "REV-ADR-0003"
---

# The SPA is never served without a valid CSP hash set

## Statement

WHEN the server is configured with a frontend build directory to serve, it MUST confirm before accepting any HTTP
request that the directory carries a CSP hash set (the inline-script hashes the HTML policy's `script-src` directive
permits) that is present, well formed and non-empty, and MUST exit without accepting any request if it does not.

## Rationale

The HTML policy's `script-src` allowlist is built from this hash set. A missing or empty set leaves a policy under which
the browser blocks every inline script, including the application's own bootstrap, and a malformed entry, such as the
wrong encoding or an embedded control character, can be dropped by the browser instead of rejected, leaving a policy
that looks populated but allows nothing. Either failure would otherwise first appear as an error in a reader's browser,
far from the operator mistake that caused it. Refusing to start shows the failure to the operator, where it happened.

## Acceptance criteria

- Starting the server with a configured build directory whose hash set is missing (no sidecar file, or one that cannot
  be read) exits with a non-zero status before the server accepts any connection.
- Starting with a hash set that exists but is malformed (not valid JSON, missing the expected array field, that field
  not an array, or an empty array) exits non-zero the same way.
- Starting with a hash set containing an entry that is not `sha256-`, `sha384-` or `sha512-` followed by standard,
  non-URL-safe base64 with optional padding, for example a base64url digest, an unsupported algorithm prefix or an entry
  containing a carriage return or line feed, exits non-zero the same way.
- Starting with a present, well-formed, non-empty hash set succeeds, and the server then serves the application.
- With no build directory configured, the server never serves the application, so this obligation holds without a hash
  set being checked.

## More information

- This obligation covers the hash set's validity when the server accepts its first request. It does not require the
  served document to keep matching that set later; a build replaced under a running process is a separate condition.
