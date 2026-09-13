---
type: REQ
profile-version: 1
id: "REV-REQ-0027"
title: "A work's author sort key tracks its current first author"
---

# A work's author sort key tracks its current first author

## Statement

WHEN a work's contributors change, the sort key the library's author sort reads MUST name the work's current first
author-role contributor.

## Rationale

The key is a redundant copy kept by application code at every contributor write, so a stale copy would sort a work under
an author it no longer has.

## Acceptance criteria

- Wiring a stub work's first author-role contributor sets the work's sort key to that contributor's sort name. Checked
  by `upgrade_stub_wires_per_role_source_version_id_and_sort_name` in `backend/src/models/work.rs`.
- Replacing a work's authors through a metadata patch sets the sort key to the new first author's sort name, in the
  replaced order. Checked by `patch_contributors_replace_sets_names_in_order` in `backend/src/routes/metadata.rs`.
- Removing a work's only author-role contributor clears the key, and a change that touches no author-role row leaves it
  unchanged. No automated check exercises either case; both follow from `refresh_first_author_sort` recomputing the key
  from the current rows, verified by inspection.

## More information

The sort that reads this key is the Design "Library filter and sort state".
