---
severity: low
surfaces: [developer, ci]
adopted: 2026-09-30
adopted-because: the pinned SQL linter flags ordinary function calls when shorthand casting style is required.
lift-when-class: dep-unblocks
lift-when: A pinned SQL linter release passes the library-relative location and path-claims migrations with all six line-scoped CV11 annotations removed, and SQL lint and schema checks pass.
---

# CV11 reports ordinary functions as casts

The library-relative location migration excludes CV11 on four lines containing `uuidv7`, `left`, `right`, `strpos` and
`chr`. The path-claims migration excludes CV11 on two policy lines containing `current_setting`. These calls perform no
casts. `sqruff` 0.40.0 reports them as casting-style violations because its
[CV11 implementation](https://github.com/quarylabs/sqruff/blob/v0.40.0/crates/lib/src/rules/convention/cv11.rs)
skips non-cast functions only when the configured style is `consistent`.

Both SQL lint configurations retain `preferred_type_casting_style = shorthand`. CV11 remains enforced on every other
line of both migrations; all other rules and parser checks remain active on the annotated lines.

Remove the six annotations and this entry when a corrected pinned release passes `just infra::sqruff` and
`just rust::schema-check` without them.
