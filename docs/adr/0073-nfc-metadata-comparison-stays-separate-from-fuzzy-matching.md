---
type: ADR
profile-version: 1
id: "REV-ADR-0073"
title: "NFC metadata comparison stays separate from fuzzy matching"
status: "accepted"
recorded-on: "2026-11-12"
decided-on: "2026-10-11"
decision-makers:
  - "John Unkovich"
---

# NFC metadata comparison stays separate from fuzzy matching

## Context and problem statement

Metadata sources can encode the same accented text with composed characters or combining sequences. Byte comparison
distinguishes those representations, even when they are canonically equivalent. Fuzzy matching answers a broader
question: whether values with different case, punctuation or internal whitespace might identify the same entity.
Metadata comparison needs a boundary between these two meanings of equivalence.

## Decision drivers

- Recognise canonical Unicode equivalence across manual edits and automated observations.
- Preserve case, punctuation and internal-whitespace distinctions in value comparison.
- Keep entity-resolution policy separate from metadata value hashing.
- Use a maintained Unicode implementation rather than maintaining composition tables.

## Considered options

- NFC plus surrounding-whitespace trimming.
- Byte comparison after surrounding-whitespace trimming.
- Fuzzy folding within metadata value hashing.

## Decision outcome

Chosen option: **NFC plus surrounding-whitespace trimming**, because it recognises canonical Unicode equivalence without
equating different bibliographic spellings through fuzzy rules. A pinned direct dependency on `unicode-normalization`
supplies NFC before trimming every string field and string list item. Case folding, internal-whitespace collapse and
punctuation stripping remain in search and matching.

### Consequences

- Positive: canonically equivalent observations contribute to the same normalised value comparison.
- Positive: fuzzy matching can evolve without changing the meaning of metadata comparison hashes.
- Negative: the application takes responsibility for pinning and updating its Unicode dependency.
- Negative: comparison hashes change for decomposed values and scalar fields newly subject to trimming. No pre-release
  hash backfill is required.

## Pros and cons of the options

### Byte comparison after surrounding-whitespace trimming

- Positive: no Unicode dependency or composition step is needed.
- Negative: equivalent Unicode representations can produce false disagreement between metadata sources.

### Fuzzy folding within metadata value hashing

- Positive: more spelling variations contribute to agreement.
- Negative: matching heuristics erase bibliographic distinctions and redefine comparison whenever those heuristics
  change.

## More information

[Unicode UAX #15](https://unicode.org/reports/tr15/) defines NFC. The
[W3C string-matching guidance](https://www.w3.org/TR/charmod-norm/) discusses vocabulary-specific matching and
tailoring; NFC plus trimming is this project's selected policy.
