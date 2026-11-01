---
type: REQ
profile-version: 1
id: "REV-REQ-0004"
title: "Web surfaces meet WCAG 2.2 Level AA"
governed-by:
  - "REV-ADR-0040"
---

# Web surfaces meet WCAG 2.2 Level AA

## Statement

Every web surface Reverie ships MUST conform to WCAG 2.2 Level AA. No element, role, or success criterion is exempt.

## Rationale

A self-hosted reading application has no vendor to fall back on for remediation: a reader who depends on sufficient
colour contrast, keyboard operability, or screen-reader semantics is served by the surface Reverie ships or not served
at all. The obligation covers the complete Level AA standard so that no surface loses accessibility coverage through a
subset chosen for convenience.

The brand's accent colour needs no exception: Reverie Gold clears the contrast thresholds as a fill with ink-on-gold
text, and wherever gold must read as text or as a hairline the design system uses the darker `accent-text` step, so the
restriction on where gold appears is a brand rule rather than an accessibility concession.

## Acceptance criteria

- Every web surface Reverie ships satisfies all applicable Level A and Level AA success criteria in WCAG 2.2. Automated
  scans and manual assessment establish conformance, including manual checks of scanned surfaces for criteria the
  automated gate cannot establish. Assessment covers every surface at each release tag and before any net-new view
  ships.
