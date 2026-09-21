---
type: REQ
profile-version: 1
id: "REV-REQ-0055"
title: "A metadata patch never removes a work's last author"
---

# A metadata patch never removes a work's last author

## Statement

WHEN a metadata patch changes a work's authors and the work holds at least one author before the patch, the patch MUST
be refused if it would leave the work with no authors; a work that already holds no author MAY be patched to remain
without one.

## Rationale

A work's author list backs the catalogue's byline display and its sort order, so every other authored work in the
library assumes at least one name is present. A cataloguer editing a byline depends on the guarantee to avoid silently
erasing authorship while making an unrelated correction. A stub with no author, typically an unmatched or partially
imported record, is common enough that a rule blocking every edit to it before an author is supplied would make
correcting its other fields needlessly hard.

## Acceptance criteria

- A patch that would empty the author list on a work that has at least one author is refused with a 422 status and
  changes nothing: the author list is unchanged afterwards. Checked by
  `patch_contributors_top_level_null_on_authored_work_returns_422` in `backend/src/routes/metadata.rs`.
- A patch that replaces a work's author list with a non-empty list succeeds. Checked by
  `patch_contributors_replace_sets_names_in_order` in `backend/src/routes/metadata.rs`.
- A patch that clears the author list, by top-level null or by an empty list, on a work that already has no author
  succeeds and leaves the work without an author (the boundary). Checked by
  `patch_contributors_top_level_null_on_authorless_stub_succeeds` and
  `patch_contributors_clear_authors_via_empty_array_on_stub_is_noop_ok` in `backend/src/routes/metadata.rs`.
- Whether accepting or reverting to a proposed value also honours the permission to leave a work that already has no
  author without one is not checked by any automated test: those two operations apply a stricter rule that refuses to
  leave any work with zero authors regardless of its author count before the change, so they satisfy this obligation
  without ever exercising its permissive branch.
