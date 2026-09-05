---
type: ADR
profile-version: 1
id: "REV-ADR-0001"
title: "Adopt Specful for requirements, designs, and decision records"
status: "accepted"
recorded-on: "2026-09-04"
decision-makers:
  - "John Unkovich"
---

# Adopt Specful for requirements, designs, and decision records

## Context and problem statement

Reverie's documentation records why durable choices were made, in architecture decision records, and what the product
looks like, in design documents. It has no record of what the software must do or how a subsystem currently works.
The repository's own rule that user-facing design comes from artifacts rather than from an agent's judgement has few
artifacts to point at, so every session that touches a subsystem re-derives its behaviour and its obligations from code,
and the re-derivation is lost when the session ends.

Which convention should hold requirements and current-state designs, and how should the repository check that they
stay accurate?

## Decision drivers

- A requirement must be stated once, be checkable, and be traceable to the design that satisfies it.
- A design must describe the current state of one subject, so a reader can change it without re-reading the code.
- Both must be validated mechanically, the same way generated API artifacts are, so drift is a failing check rather than
  a discovery.
- The convention must be plain files in the repository, readable and searchable without any tool, with a low exit cost.
- The tool that mechanises it must be first-party or otherwise fully under the maintainer's control, and pinned like
  every other repository tool.

## Considered options

- Adopt Specful: its convention, its command-line tool, and its validation gate.
- Adopt a hand-written convention: a `docs/specs` directory, a template, and no validator.
- Keep the current state: decision records only, with behaviour re-derived from code as needed.

## Decision outcome

Chosen option: **adopt Specful**, because it is the only option that gives requirements and designs stable identifiers,
a generated navigation index, requirement-to-design trace links, and a validator that fails the build when a document
loses its shape or its links, while the artifacts themselves stay Markdown with YAML frontmatter that any editor and any
search reads. The first corpus is one subsystem, the library filter and sort state, rather than a sweep of the codebase;
a partially documented repository is a valid state under the convention.

Records under `adr/` predate the Specful profile. They stay where they are and keep their shape until a separate review
decides, record by record, whether each still earns its place; only new decisions use the profile. That review is not
part of the adoption.

### Consequences

- Positive: a requirement, its design, and its governing decision link to one another, and `specful trace` shows the
  triangle for any identifier.
- Positive: the validator runs in pre-commit, in the infrastructure lint aggregate, and in the CI prose job, so a stale
  index or a broken link fails the change that caused it.
- Positive: the exit cost is deleting `.specful/` and the generated views; the documents remain readable Markdown.
- Negative: the repository carries two decision-record homes and two shapes until the review of the older records lands.
- Negative: the tool is pre-1.0 and the repository is its first adopter, so profile changes arrive with release notes
  and conversion steps rather than being absorbed silently; the mise pin holds the version until the maintainer moves
  it.
- Negative: two lint configurations changed to accommodate the artifact shape: the formatter ignores the generated
  catalog, and markdownlint no longer counts a frontmatter title as a second top-level heading.

## Pros and cons of the options

### Adopt Specful

- Positive: identifiers, index, trace, and validation come from one pinned binary with attested releases.
- Positive: the record model separates what must hold, how the system works, and why the choice was made, with a
  written boundary between an obligation and a decision.
- Negative: the ADR profile is stricter than the MADR shape the existing records follow, so those records cannot enter
  it without being rewritten.

### Hand-written convention

- Positive: no new binary, no pin, nothing to learn beyond a template.
- Negative: nothing checks that a document keeps its sections, that a link resolves, or that an index is current, so the
  corpus decays at the rate the generated API artifacts did before their drift gate existed.
- Negative: identifiers would be allocated by hand or not at all, so cross-references break on every rename.

### Keep the current state

- Positive: no change.
- Negative: the rule that design comes from artifacts stays unenforceable, and every session keeps paying the
  re-derivation cost.

## More information

The adoption page and the profiles are at <https://unkos-dev.github.io/specful/>. The first Design under the
convention is [Library filter and sort state](../specs/library/design/0001-library-filter-and-sort-state.md). Revisit
this record if the older decision records are migrated into the profile, at which point the two-homes consequence no
longer applies, or if a profile change makes the existing corpus invalid without a documented conversion.
