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

Every GraphQL object that Relay normalises has a global `id`, and the browser also needs an identity for each Book,
Series and Shelf route and for the books, filters and restoration anchor in Library visit state. Padded base64 of
`Type:<uuid>` is not safe in a URL path, so routes and visit state carried UUID values. The client then converted
between the two forms at about fifty call sites. The GraphQL server specification says IDs are opaque, and that the only
value passed to `node(id:)` is the unaltered result of querying `id`.

What encoding should global IDs use so that one identity serves Relay, browser routes and visit state without client
conversion?

## Decision drivers

- One identity per object across the Relay store, route parameters, links and visit state, passed unaltered.
- Safe in a URL path without escaping: only unreserved characters (RFC 3986 section 2.3).
- A wrong-type ID is refused at decode, as the existing type-tagged IDs are.
- A specified encoding with maintained implementations, not a Reverie-specific format.
- Parsing stays cheap and bounded for client-supplied input.
- Retained HTTP, OPDS and token-facing contracts keep their UUID values.

## Considered options

- TypeID
- Base64url without padding of `Type:<uuid>`
- UUID values in URLs with a separate `uuid` field
- Padded base64 of `Type:<uuid>` with UUID routes

## Decision outcome

Chosen option: **TypeID**, because it is the only option that gives one path-safe, type-tagged identity defined by a
published specification with maintained implementations, so routes, links and visit state can carry the GraphQL ID
unaltered.

An ID is a lowercase type prefix, `_`, then the record's UUID in 26 base32 characters (TypeID specification 0.3.0). The
prefixes are `book` (Manifestation), `work`, `series`, `shelf`, `author`, `tag`, `genre`, `mood`, `metadata_version` and
`user`. The backend binds each prefix to its type at compile time through the `type-safe-id` crate. Retained HTTP
responses, downloads, covers and OPDS keep UUID values.

### Consequences

- Positive: route loaders pass the path parameter to their query unaltered, and links, Library rows, filters and the
  restoration anchor use the IDs that queries return. Most client conversions disappear.
- Positive: the prefix shows the type in a URL or log line, and a wrong-type ID fails to parse.
- Positive: Reverie's tables already default to UUIDv7, so IDs sort by creation time without a schema change.
- Negative: every ID and stored browser URL changes at once. UUID and base64 URLs from before the change resolve to
  not-found pages.
- Negative: the crate's parser copies a mismatched prefix of any length, so the server keeps an 80-character cap before
  parsing.
- Negative: retained HTTP responses still carry UUID values, so the client keeps a codec for Book links built from them
  and for short-UUID display fallbacks until those consumers migrate.
- Neutral: the suffix decodes to the UUID, so IDs are opaque by convention, not secret. Base64 IDs decoded just as
  easily.

## Pros and cons of the options

### Base64url without padding of `Type:<uuid>`

- Positive: path-safe and keeps the existing type tag with a one-line encoding change.
- Negative: a Reverie-specific format with no specification or shared implementations.
- Negative: 55 to 67 characters, unreadable to people looking at a URL or log.

### UUID values in URLs with a separate `uuid` field

- Positive: GraphQL caching guidance allows exposing a previous API's IDs in a separate field, and URLs stay unchanged.
- Negative: two identities per object, and routes need `uuid:` lookup roots per type instead of `node(id:)`.
- Negative: visit state and links keep converting between the two forms, which is the problem being solved.

### Padded base64 of `Type:<uuid>` with UUID routes

- Positive: no change.
- Negative: `+`, `/` and `=` are not path-safe, so every route and visit consumer converts at its boundary.

## More information

- [TypeID specification](https://github.com/jetify-com/typeid/tree/main/spec).
- Reconsider if a third-party client needs GraphQL IDs that match the retained HTTP UUID values, or if the TypeID
  specification changes its format incompatibly.
