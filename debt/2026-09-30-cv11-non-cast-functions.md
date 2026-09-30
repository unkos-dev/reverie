---
severity: low
surfaces: [developer, ci]
adopted: 2026-09-30
adopted-because: the pinned SQL linter flags ordinary function calls when shorthand casting style is required.
lift-when-class: dep-unblocks
lift-when: A pinned SQL linter release passes the library-relative location migration with all four line-scoped CV11 annotations removed, and SQL lint and schema checks pass.
---

# CV11 reports ordinary functions as casts

The library-relative location migration excludes CV11 on four lines containing `uuidv7`, `left`, `right`, `strpos` and
`chr`. These calls perform no casts. `sqruff` 0.40.0 reports them as casting-style violations because its
[CV11 implementation](https://github.com/quarylabs/sqruff/blob/v0.40.0/crates/lib/src/rules/convention/cv11.rs)
skips non-cast functions only when the configured style is `consistent`.

Both SQL lint configurations retain `preferred_type_casting_style = shorthand`. CV11 remains enforced on every other
line of the migration; all other rules and parser checks remain active on the annotated lines.

Remove the four annotations and this entry when a corrected pinned release passes `just infra::sqruff` and
`just rust::schema-check` without them.
