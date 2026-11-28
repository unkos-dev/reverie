---
type: ADR
profile-version: 1
id: "REV-ADR-0081"
title: "TypeID global identifiers carried in browser URLs"
status: "accepted"
recorded-on: "2026-11-19"
decided-on: "2026-11-17"
decision-makers:
  - "John Unkovich"
---

# TypeID global identifiers carried in browser URLs

## Context and problem statement

Every GraphQL object that Relay normalises has a global `id`, and the browser also needs an identity for Book, Series
and Shelf routes and for books, filters and restoration anchors in Library visit state. Padded base64 of `Type:<uuid>`
is not safe in a URL path, forcing routes and visit state to carry raw UUID values and convert between representations
across client boundaries. The GraphQL specification treats global IDs as opaque values passed unaltered back to
`node(id:)`.

What encoding should global IDs use so that one identity serves Relay, browser routes and visit state without client
conversion?

## Decision drivers

- One identity per object across the Relay store, route parameters, links and visit state, passed unaltered.
- Safe in a URL path without escaping: only unreserved characters (RFC 3986 section 2.3).
- A wrong-type ID is refused at decode, as the existing type-tagged IDs are.
- A specified encoding with maintained implementations, not a Reverie-specific format.
- Parsing stays cheap and bounded for client-supplied input.
- HTTP contracts keep their UUID values, including OPDS feeds and file and cover URLs, which stay on HTTP.

## Considered options

- TypeID
- Base64url without padding of `Type:<uuid>`
- UUID values in URLs with a separate `uuid` field
- Padded base64 of `Type:<uuid>` with UUID routes

## Decision outcome

Chosen option: **TypeID**, because it is the only option that gives one path-safe, type-tagged identity defined by a
published specification with maintained implementations, so routes, links and visit state can carry the GraphQL ID
unaltered.

### Consequences

- Positive: route loaders pass path parameters unaltered to queries, and links, Library rows, filters and restoration
  anchors share the IDs queries return without client translation.
- Positive: the type prefix identifies the resource kind in URLs and logs, and wrong-type IDs fail parsing.
- Positive: UUIDv7 table primary keys preserve creation-time sort order inside the TypeID suffix without schema changes.
- Negative: switching browser URLs to TypeID invalidates bookmarked UUID and base64 routes.
- Negative: `/api/v1` endpoints, downloads, covers and OPDS keep UUID values, so links crossing the HTTP-to-GraphQL
  boundary still translate between UUID values and TypeIDs.
- Neutral: the suffix decodes directly to the underlying UUID, so TypeIDs are opaque by convention rather than secret.

## Pros and cons of the options

### Base64url without padding of `Type:<uuid>`

- Positive: path-safe and keeps the existing type tag with a one-line encoding change.
- Negative: a Reverie-specific format with no specification or shared implementations.
- Negative: 54 to 70 characters, unreadable to people looking at a URL or log.

### UUID values in URLs with a separate `uuid` field

- Positive: GraphQL caching guidance allows exposing a previous API's IDs in a separate field, and URLs stay unchanged.
- Negative: two identities per object, and routes need `uuid:` lookup roots per type instead of `node(id:)`.
- Negative: visit state and links keep converting between the two forms, which is the problem being solved.

### Padded base64 of `Type:<uuid>` with UUID routes

- Positive: no change.
- Negative: `+`, `/` and `=` are not path-safe, so every route and visit consumer converts at its boundary.

## More information

- [TypeID specification](https://github.com/jetify-com/typeid/tree/main/spec).
- Reconsider if a non-browser client moving to GraphQL needs IDs that match the UUID values HTTP endpoints carry, or if
  the TypeID specification changes its format incompatibly.
