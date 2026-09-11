---
type: REQ
profile-version: 1
id: "REV-REQ-0011"
title: "HTML and dist-file responses carry the HTML CSP"
governed-by:
  - "REV-ADR-0003"
---

# HTML and dist-file responses carry the HTML CSP

## Statement

WHEN the server answers a request with the single-page application's `index.html`, or with a file from the frontend
build's output directory (under its `/assets` path or elsewhere in the directory), the server MUST set the response's
`Content-Security-Policy` header to the HTML policy: a `script-src` directive limited to `'self'` and the build's
declared inline-script hashes, with no `'unsafe-inline'` or `'unsafe-eval'` source for scripts.

## Rationale

The application's document and every static file it loads, its script bundles, fonts, brand assets and favicons, are
rendered or run in the same browsing context, so a gap in the policy on any one of them is a gap for all of them.
Holding every file the build ships to the HTML policy, whichever code path serves it, stops a font or an icon served by
a different mechanism from arriving with a weaker policy or none.

## Acceptance criteria

- A response carrying the application's `index.html` has a `Content-Security-Policy` header whose `script-src` directive
  contains no `'unsafe-inline'` or `'unsafe-eval'` and includes at least one `sha256-`, `sha384-` or `sha512-` source.
- A response carrying a file under the build's `/assets` path has the same `Content-Security-Policy` value as the
  `index.html` response.
- A response carrying a build file outside `/assets`, such as a font, a brand asset or a favicon, has that same value.
- A response in any of these three classes with status `304 Not Modified` or `405 Method Not Allowed` still has that
  same value.

## More information

- Responses that serve none of these three classes, such as a Problem Details document or a plain "not found" with no
  document or file behind it, are outside this obligation.
