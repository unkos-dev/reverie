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

- A `SELECT` against `device_tokens` over a connection authenticated as `reverie_readonly` fails with SQLSTATE `42501`
  (`insufficient_privilege`) rather than returning an empty or filtered result.
- A `SELECT` against `local_credentials` over a connection authenticated as `reverie_readonly` fails with SQLSTATE
  `42501` (`insufficient_privilege`) rather than returning an empty or filtered result.
- A `SELECT` of only the `token_hash` column of `device_tokens` over a connection authenticated as `reverie_readonly`
  fails with SQLSTATE `42501`, so a column-level grant on the hash does not satisfy the requirement.
- A `SELECT` of only the `password_hash` column of `local_credentials` over a connection authenticated as
  `reverie_readonly` fails with SQLSTATE `42501`, so a column-level grant on the hash does not satisfy the requirement.
- The backend test `db::tests::readonly_role_is_refused_credential_tables` asserts these four refusals against a
  migrated database, each in its own query, and a migration that grants the role `SELECT` on either table or on either
  hash column fails it.
