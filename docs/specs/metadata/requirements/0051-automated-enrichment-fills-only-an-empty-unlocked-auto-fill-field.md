---
type: REQ
profile-version: 1
id: "REV-REQ-0051"
title: "Automated enrichment fills only an empty, unlocked, auto-fill field"
---

# Automated enrichment fills only an empty, unlocked, auto-fill field

## Statement

WHEN automated enrichment applies an incoming observation to a work's or a manifestation's metadata, it MUST change a
field's current value only if that field held no value when the enrichment run read the record's state, is not locked
against automatic change, and belongs to the set of fields the system fills automatically; content rating MUST NOT
belong to that set, so no automated observation ever sets or replaces the content rating, whether or not it already
holds a value. For an external-identifier field the emptiness check is repeated under the record lock at the moment of
the change; for every other field it is the value read at the start of the run, so a value written between that read and
the apply is not protected by this obligation.

## Rationale

An operator who has set a value, or locked a field against change, relies on automated enrichment leaving it alone;
silently overwriting a deliberate choice with no warning would destroy the operator's own edit and erode trust in the
catalogue. Content rating additionally gates what a child account can see, so a human reviewer, never an automated
source, must be the one who assigns or changes it; treating it like any other auto-fill field would let an external
source silently set the value the child-safety enforcement depends on.

## Acceptance criteria

- A field that already carries a value is not overwritten by an incoming observation; the observation is staged for
  review instead. Checked by `autofill_canonical_already_set_stages` in `backend/src/services/enrichment/policy.rs`.
- A field whose default handling is to stage for review, rather than fill automatically, is always staged, never applied
  directly, even when it holds no value. Checked by `propose_field_always_stages` in
  `backend/src/services/enrichment/policy.rs`.
- A field locked against automatic change receives no update at all, regardless of whether it would otherwise qualify to
  be filled automatically and regardless of whether it holds a value. Checked by `locked_field_is_noop` and
  `locked_overrides_even_autofill_with_empty_canonical` in `backend/src/services/enrichment/policy.rs`.
- Content rating is never set by automated enrichment, even when it holds no value. Checked by
  `content_rating_never_autofills` in `backend/src/services/enrichment/policy.rs`. No automated source observes a
  content rating today, so this criterion holds vacuously against every source that exists and constrains any source
  added to the system afterward.
- An unlocked field that holds no value and belongs to the set the system fills automatically is filled by the incoming
  observation. Checked by `autofill_empty_canonical_applies` in `backend/src/services/enrichment/policy.rs`.
