---
type: REQ
profile-version: 1
id: "REV-REQ-0033"
title: "An authentication failure carries the challenge for the route's credential type"
governed-by:
  - "REV-ADR-0028"
---

# An authentication failure carries the challenge for the route's credential type

## Statement

An authentication failure MUST answer with HTTP status 401 and a `WWW-Authenticate` challenge matching the route's
credential type: on an API route, `Bearer`, with `error="invalid_token"` added when a credential was presented and
rejected; on an OPDS route, `Basic` with a realm. The response body MUST be identical whether no credential was
presented or a credential was presented and rejected.

## Rationale

[RFC 9110 §11.6.1](https://www.rfc-editor.org/rfc/rfc9110#section-11.6.1) requires a 401 response to carry a
`WWW-Authenticate` challenge, and [RFC 6750 §3](https://www.rfc-editor.org/rfc/rfc6750#section-3) and
[§3.1](https://www.rfc-editor.org/rfc/rfc6750#section-3.1) define the `Bearer` challenge and its `error="invalid_token"`
parameter; [RFC 7617 §2](https://www.rfc-editor.org/rfc/rfc7617#section-2) defines the `Basic` challenge. OPDS reader
applications prompt the user for credentials only on a `Basic` challenge, so an API-shaped `Bearer` challenge on an OPDS
route would leave the reader silently failing rather than prompting. An identical response body for "no credential" and
"rejected credential" denies an attacker an oracle on which check failed; only the challenge header, present on both,
distinguishes the two cases for a legitimate client.

## Acceptance criteria

- No credential presented on an API route answers 401 with the bare challenge `Bearer`. Checked by
  `unauthorized_carries_bare_bearer_challenge` in `backend/src/error/mod.rs`.
- A credential presented and rejected on an API route answers 401 with the challenge `Bearer error="invalid_token"`.
  Checked by `invalid_credential_carries_invalid_token_challenge` in `backend/src/error/mod.rs`.
- The response body is identical whether no credential was presented or a credential was presented and rejected. Checked
  by `invalid_credential_returns_same_body_as_unauthorized` in `backend/src/error/mod.rs`.
- No credential presented on an OPDS route answers 401 with a `Basic` challenge naming the configured realm, for example
  `Basic realm="Reverie OPDS", charset="UTF-8"`. Checked by `unauthenticated_returns_challenge` in
  `backend/src/routes/opds/tests.rs`.
- A credential presented and rejected on an OPDS route answers with a `Basic` challenge, never a `Bearer` one. Checked
  by `wrong_password_returns_challenge` in `backend/src/routes/opds/tests.rs`.
