---
type: ADR
profile-version: 1
id: "REV-ADR-0067"
title: "Publication dates preserve source precision"
status: "proposed"
recorded-on: "2026-10-31"
decision-makers:
  - "John Unkovich"
---

# Publication dates preserve source precision

## Context and problem statement

A publication observation may name a year, a month, a complete date or a timestamp. A calendar-date column cannot
express an unknown month or day. Padding partial observations invents bibliographic information; reducing timestamps to
dates loses supplied precision and can merge distinct journal observations. The catalogue, review surface and EPUB need
a representation that retains what the source actually supplies.

## Decision drivers

- Preserve known publication precision without inventing missing components.
- Keep one editable publication value across the API, catalogue and EPUB.
- Retain unusable source observations for correction while protecting canonical metadata.
- Reuse Chrono and the existing journal, transaction owners and writeback queue.

## Considered options

- Nullable publication text with deterministic validation and full-precision identity.
- A nullable calendar date with partial values padded or kept outside the canonical field.
- A publication-year integer.

## Decision outcome

Chosen option: **Nullable publication text with deterministic validation and full-precision identity**, because it
preserves every supported source precision in one field without supplying unknown components. The accepted W3C Date and
Time Formats grammar bounds conversion; Chrono validates calendar components. Accepted text retains its trimmed
spelling, offset and fractional digits. A complete date and a timestamp on that date have distinct journal identities;
identical observations retain existing deduplication.

Source observations use the existing journal before canonical promotion. Refused text stays recoverable through that
model. Automatic promotion retains the empty, unlocked-field policy. The existing writeback queue carries the changed
field so a publication clear remains distinguishable from an unrelated edit. This choice retains the existing
transaction owners and publication boundary.

### Consequences

- Positive: year, month, date and timestamp precision survive catalogue edits and EPUB writeback.
- Positive: invalid source text remains available for review without entering canonical metadata.
- Negative: text storage delegates calendar validity to application validation rather than a database date type.
- Negative: equivalent instants written differently can remain distinct observations; identity does not convert to UTC.
- Negative: rollback to a date column requires richer values to be resolved and publication work to drain, or a database
  snapshot to be restored.

## Pros and cons of the options

### A nullable calendar date

- Positive: database date validation and chronological operations are native.
- Negative: representing partial precision requires invented components or a second representation, and a calendar date
  cannot retain supplied time and offset information.

### A publication-year integer

- Positive: year-only observations fit directly and year comparisons are simple.
- Negative: supplied month, day and time information is discarded. A second field would be needed to preserve it.
