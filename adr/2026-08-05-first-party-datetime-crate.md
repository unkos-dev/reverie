---
status: "accepted"
date: 2026-08-05
supersedes: []
decision-makers: "John Unkovich"
consulted: []
informed: "Reverie contributors"
---

# chrono is the first-party datetime crate

## Context and Problem Statement

Reverie's JSON API conventions declare every timestamp to be an RFC 3339
string, `Z`-terminated, with variable sub-second precision. The backend
carried its instants as `time::OffsetDateTime`, whose serde implementation
emits a nine-element array of date and offset components rather than a
string. RFC 3339 output required a `#[serde(with = "time::serde::rfc3339")]`
attribute on each field individually.

That made the published contract a property of remembering an attribute
rather than a property of the type. The failure is silent: a new datetime
field compiles, serialises, and passes any test that does not inspect the
field's JSON shape, while emitting an array to clients that were promised a
string. It reached users twice. The first occurrence broke a book-detail
field; the second broke the shelves and settings surfaces, where the browser
client's schema validation rejected every response for an account that owned
a shelf, so the list and detail pages threw outright.

Both were fixed by adding the missing attributes. Neither fix addressed the
mechanism, and an audit at the time found that roughly a third of the
serialised datetime fields in the codebase were relying on the default rather
than an adapter. The question is therefore not how to repair the individual
fields, but whether the wire contract should keep depending on a per-field
convention at all.

A second, smaller force points the same way. `OffsetDateTime` carries an
arbitrary UTC offset at runtime, so "this value is UTC" was a convention the
codebase asserted and could not check. Every timestamp Reverie stores is
`timestamptz` and every timestamp it emits is UTC.

## Decision Drivers

- The published RFC 3339 wire contract must hold by construction, not by
  per-field discipline.
- The UTC invariant should be checkable rather than conventional.
- The database mapping must stay a first-party integration: `timestamptz`
  round-trips without a wrapper type or a conversion layer.
- The OpenAPI schema type for datetimes must be derivable, so the generated
  spec cannot drift from the code.
- Neither crate can be removed from the dependency graph regardless of the
  choice, so "fewer crates" is not available as a driver.

## Considered Options

- Keep `time` and mechanise the discipline with a static guard plus runtime
  contract tests
- Adopt `chrono` as the first-party datetime type
- Adopt `jiff` as the first-party datetime type

## Decision Outcome

Chosen option: "Adopt `chrono` as the first-party datetime type", because it
converts the wire contract from a convention into the default behaviour of
the type. `chrono::DateTime<Utc>` serialises to `Z`-terminated RFC 3339 and
deserialises from it without any field attribute, so the defect class that
shipped twice stops being representable rather than becoming better policed.
`DateTime<Utc>` also encodes UTC in its type parameter, which turns the
storage invariant into something the compiler checks.

The decisive practical point is that the integration cost of this choice is
zero. The database layer maps `DateTime<Utc>` to `timestamptz` natively, and
the OpenAPI generator maps it to `string` / `date-time` natively. Both are
first-party features of those crates, so nothing is wrapped, adapted, or
hand-maintained.

`time` is not removed. Three third-party signatures accept its types and no
others: the session layer's inactivity expiry, the cookie builder's
`max_age`, and the session record whose expiry field the session store
persists. Those sites keep `time`, and the last one converts explicitly at
the point where the value reaches the database. The value gates session
expiry, so the conversion refuses an unrepresentable instant rather than
rounding or truncating one.

### Consequences

- Good, because a datetime field added to a serialised struct is correct on
  the wire with no attribute, no review checklist, and no guard to forget.
- Good, because the per-field RFC 3339 adapters are deleted outright, which
  removes the thing that had to be remembered.
- Good, because `DateTime<Utc>` makes a non-UTC instant in a model, DTO, or
  signature a compile error rather than a convention violation.
- Good, because formatting a timestamp becomes infallible, which removes two
  error variants and two `Result` returns from the pagination cursors that
  existed only to carry a formatting failure that cannot occur.
- Bad, because `time` remains in the tree at three boundaries, so two
  datetime crates coexist and a contributor must know which applies where.
  A compiler lint is what keeps that boundary from spreading.
- Bad, because explicit RFC 3339 formatting has a trap: the crate's plain
  `to_rfc3339` writes a `+00:00` offset, while serde and the options-taking
  variant write `Z`. Code that formats a timestamp by hand must match what
  serde emits, which matters most where a timestamp is both serialised into
  a response body and formatted into an entity-tag derived from it.
- Neutral, because the two crates print sub-second digits differently, so a
  timestamp string minted before this change may not compare equal to the
  same instant rendered after it. Both affected surfaces already fail safe:
  a stale entity-tag produces a precondition failure and a stale pagination
  cursor produces a decode error.
- Neutral, because both crates already executed on production request paths
  before this decision and still do. Neither one entered or left the
  dependency graph.

### Confirmation

First-party code uses `chrono`. The `time` types are entered as
`disallowed-types` in the clippy configuration, and the three third-party
boundaries carry scoped `#[expect]` attributes naming the API that forces
each one.

A compiler lint rather than a text search, because the question is what a
path resolves to and not how it is spelled: an import, a fully-qualified
call, an absolute `::time::` path, a re-export reached through another
crate, and a renaming import are one identity and one entry, while
`std::time` is a different identity needing no exclusion clause. It also
covers the whole workspace and every target rather than one source
directory, and an exemption that stops being necessary fails the build, so
the carve-outs cannot outlive their justification the way a checked-in
allowlist can.

## Pros and Cons of the Options

### Keep `time` and mechanise the discipline

- Good, because it changes no types and needs no migration.
- Good, because `time` is what the three forced boundaries already speak, so
  the codebase would have exactly one datetime vocabulary.
- Neutral, because the crate does offer a UTC-typed instant, but the
  database layer does not map it, so the storage type would stay the
  offset-carrying one.
- Bad, because it keeps the wire contract dependent on a per-field
  attribute and adds machinery to police that dependency. The safe default
  stays the unsafe one, and the guard becomes a permanent tax.
- Bad, because the guard has to reason about which serialised fields are
  wire-facing, including flattened and route-local response types. That is
  a harder property to check than "this type is correct by construction".

### Adopt `chrono`

- Good, because RFC 3339 is the serde default in both directions.
- Good, because UTC is in the type parameter.
- Good, because the database and OpenAPI integrations are native and empty.
- Bad, because a second datetime crate remains at the forced boundaries.
- Bad, because the hand-formatting API has a `Z`-versus-offset trap.

### Adopt `jiff`

- Good, because it separates absolute time from civil time in the type
  system, which is a cleaner model than either incumbent, and it ships IANA
  time-zone database integration.
- Good, because the OpenAPI generator already supports it.
- Bad, because the database layer has no integration for it. A local
  wrapper type would be needed at every bind and every decode, and the
  orphan rule makes that boundary permanent rather than temporary. This was
  confirmed by building a working proof of concept, not assumed.
- Bad, because it is pre-1.0. The storage layer for a self-hosted library is
  a poor place to take an unstable-API dependency, and the wrapper cost
  above would be paid for as long as the dependency existed.

## More Information

The wire format itself is governed by the JSON API conventions record; this
decision covers only which crate carries the values behind it.

Reopen this decision if either of the following becomes true:

- The database layer gains first-party `jiff` support. That removes the
  wrapper boundary which is the sole reason `jiff` was declined, and the
  absolute-versus-civil split is a better model than the one adopted here.
- A product requirement introduces named time zones, for example rendering
  reading activity in a user's own zone rather than UTC. Named-zone support
  is a companion crate for the chosen option and native to `jiff`, so the
  comparison changes shape.

A future requirement for calendar-only values beyond publication dates would
not reopen this decision; the chosen crate already carries a distinct
calendar-date type, which is what publication dates use.
