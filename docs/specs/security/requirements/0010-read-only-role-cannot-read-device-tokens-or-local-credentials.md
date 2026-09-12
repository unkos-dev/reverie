---
type: REQ
profile-version: 1
id: "REV-REQ-0010"
title: "Read-only role cannot read device tokens or local credentials"
---

# Read-only role cannot read device tokens or local credentials

## Statement

The `reverie_readonly` database role MUST NOT hold `SELECT` on the `device_tokens` table or on the `local_credentials`
table.

## Rationale

`reverie_readonly` exists for debugging and reporting connections and can read most of the schema. `device_tokens` and
`local_credentials` hold hashed token secrets and password hashes, material a leaked or careless reporting credential
could otherwise read and attack offline, even though that credential can never sign in as an application user.
Withholding the grant on these two tables keeps that material out of every reporting connection, whatever row-level
security applies elsewhere.

## Acceptance criteria

- Against a migrated database, `SELECT has_table_privilege('reverie_readonly', 'public.device_tokens', 'SELECT')`
  returns `false`.
- Against a migrated database, `SELECT has_table_privilege('reverie_readonly', 'public.local_credentials', 'SELECT')`
  returns `false`.
- A `SELECT` against either table over a connection authenticated as `reverie_readonly` fails with a permission-denied
  error rather than returning an empty or filtered result.

## More information

- Nothing automated checks these criteria. They are inspections run by hand against a migrated database, so a migration
  that widened the role's grants would pass every gate in the repository.
