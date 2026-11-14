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

Every web surface Reverie ships MUST conform to WCAG 2.2 Level AA, except where an exception in the
[approved exceptions register](../design/0002-accessibility-gate-and-manual-audit.md#approved-exceptions) covers that
success criterion on that surface.

## Rationale

A self-hosted reading application has no vendor to fall back on for remediation: a reader who depends on sufficient
colour contrast, keyboard operability, or screen-reader semantics is served by the surface Reverie ships or not served
at all. The obligation covers the complete Level AA standard. An exception exists for the case where meeting one success
criterion on one surface would worsen the experience for every user, or would need custom code that the project must
maintain. The maintainer approves each exception individually and the register records it, so no surface loses
accessibility coverage through a subset chosen for convenience, and every gap stays visible.

The brand's periwinkle accent needs no exception: it clears the contrast thresholds as a fill with `fg-on-accent` text.
Accent-coloured text and hairlines use the `accent-text` step.

## Acceptance criteria

- Every web surface Reverie ships satisfies all applicable Level A and Level AA success criteria in WCAG 2.2, except a
  success criterion that an approved exception covers on that surface.
- Every entry in the approved exceptions register names the success criterion, the affected surfaces, the reason, and
  the date the maintainer approved it.
- Every surface the automated gate scans is also manually assessed against the applicable criteria that the automated
  checks cannot fully establish.
- Every surface the automated gate does not scan is manually assessed against all applicable Level A and Level AA
  success criteria.
- Manual assessment covers every surface at each release tag.
