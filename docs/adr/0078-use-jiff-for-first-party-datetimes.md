---
type: ADR
profile-version: 1
id: "REV-ADR-0078"
title: "Use Jiff for first-party datetimes"
status: "accepted"
recorded-on: "2026-11-18"
decision-makers:
  - "John Unkovich"
supersedes:
  - "REV-ADR-0046"
---

# Use Jiff for first-party datetimes

## Context and problem statement

RustSec marks Chrono unmaintained across all releases. Reverie needs a maintained datetime library that preserves UTC
API timestamps and works with compile-time checked PostgreSQL queries.
[RustSec advisory RUSTSEC-2026-0543](https://rustsec.org/advisories/RUSTSEC-2026-0543.html)

## Decision drivers

- Serialise UTC instants as RFC 3339 with a trailing `Z` by default.
- Derive OpenAPI datetime schemas from the Rust types.
- Keep the database schema and compile-time SQL checks.
- Preserve the precision and spelling of bibliographic dates.

## Considered options

- Adopt Jiff 1.0 with its SQLx adapter.
- Use `time` for first-party datetimes.

## Decision outcome

Chosen option: **Adopt Jiff 1.0 with its SQLx adapter**, because `jiff::Timestamp` carries an instant and emits the
required UTC format through Serde. The maintained `jiff-sqlx` PostgreSQL adapter provides typed binds and decoding.
Reverie uses SQLx type overrides and keeps adapter values at the SQL boundary. `time` integrates directly with SQLx. Its
default serialisation would require additional code to enforce the API format.

Use Jiff civil dates to validate the supplied calendar components of publication text. Keep `time` at the session and
cookie interfaces that require it.

### Consequences

- Positive: The stored timestamp and published API types remain stable.
- Positive: Operational instants and bibliographic calendar dates use distinct types.
- Negative: SQL code converts between Jiff and the SQLx adapter. Some third-party interfaces still use other datetime
  types.
- Negative: The SQL constraints allow a small part of year 9999 beyond Jiff's maximum. A row carrying a timestamp in
  that window returns a decode error, and normal writes use representable instants. The constraints retain the existing
  four-digit year bounds and exclude infinity.
- Negative: Fractional seconds can have a different spelling. An old reading entity tag can become stale and follows the
  usual conditional-write recovery.
