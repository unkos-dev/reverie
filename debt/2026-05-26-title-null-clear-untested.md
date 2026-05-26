---
status: active
severity: low
surfaces: [developer]
adopted: 2026-05-24
adopted-because: "11c (PR #316) ships the merge-patch handler but does not test the path where title is explicitly set to null (should 422); edge case deferred from the slice"
lift-when-class: internal-refactor
lift-when: PR adds a test asserting that PATCH with `"title": null` returns 422 (title is a required field and cannot be cleared)
lifted: ~
superseded-by: ~
---

# `title` null-clear via PATCH returns 422 but path is untested

## Constraint

RFC 7396 JSON Merge Patch allows setting a field to `null` to remove
it. For `title`, which is NOT NULL in the schema, the handler should
reject the request with 422. The rejection likely works today via the
database constraint, but no test asserts the behaviour or the error
shape.

## Workaround

None — the path is simply untested. Risk is low because the DB
constraint is the backstop, but the error shape reaching the client
is unverified (could be a raw sqlx error instead of RFC 7807).

## Lift trigger

Add an integration test: `PATCH /api/books/{id}/metadata` with
`{"title": null}` asserts 422 with RFC 7807 body. If the current
error shape is a raw DB error, add explicit validation before the
query.
