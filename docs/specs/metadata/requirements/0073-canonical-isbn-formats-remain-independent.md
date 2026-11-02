---
type: REQ
profile-version: 1
id: "REV-REQ-0073"
title: "Canonical ISBN formats remain independent"
---

# Canonical ISBN formats remain independent

## Statement

When accepting supplied ISBN metadata or an explicit clear, Reverie MUST apply each supplied value or clear only to its
corresponding ISBN format, without deriving, replacing or clearing the counterpart because of that operation.

## Rationale

An equivalent number calculated from one format is not a separately supplied observation. Cataloguers need to retain and
correct the two observations independently, including when they disagree.

## Acceptance criteria

1. Initial ingestion supplied with only a valid ISBN-10 leaves canonical ISBN-13 absent; ingestion supplied with only a
   valid ISBN-13 leaves canonical ISBN-10 absent.
2. When both supplied formats are accepted, each canonical field retains its supplied number, including when the numbers
   are not equivalent.
3. An accepted operation that sets or clears only one format leaves the counterpart's existing value or absence
   unchanged.
4. These independence conditions apply to ingestion, automated enrichment, manual editing, acceptance and reversion.
