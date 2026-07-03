---
severity: medium
surfaces: [end-user]
adopted: 2026-07-03
adopted-because: "PR #574 promotes the EPUB subtitle to a canonical, PATCH-editable field, but opf_rewrite's Target has no subtitle arm; teaching the rewriter about title-type refines is EPUB3 surface work out of scope for that PR"
lift-when-class: internal-refactor
lift-when: opf_rewrite handles title-type=subtitle refined dc:title elements (rewrite from canonical subtitle, or skip), with a round-trip test proving extract → writeback → re-extract preserves the subtitle
---

# OPF writeback rewrites every dc:title, destroying the declared subtitle

## Constraint

`opf_rewrite::Target` carries no subtitle, and `target_text_for_dc`
maps every `dc:title` element to `target.title`. An EPUB3 that
declares a subtitle (a second `dc:title` plus a `title-type=subtitle`
refine) loses it on the first metadata writeback: both title elements
get the canonical main title. The refine survives but its text does
not, so a later re-extraction of the same file journals
subtitle == title. The all-titles rewrite predates PR #574, but that
PR makes the loss matter: subtitle is now a canonical field, and the
subtitle PATCH itself enqueues the destructive rewrite.

## Workaround

None in code. Immutable ingestion bounds the damage: the source file
is never touched, only the managed library copy degrades, and a
re-ingest restores it.

## Lift trigger

Extend `Target` with the canonical subtitle and teach `opf_rewrite`
to rewrite subtitle-refined `dc:title` elements from it (or skip
them). Add a round-trip test: extract subtitle, write back, re-extract,
assert the subtitle is unchanged.
