---
severity: low
surfaces: [security, server-operator]
adopted: 2026-06-27
adopted-because: PR #511 review (CodeRabbit); rotate() supersedes in-tx but the schema has no DB-level uniqueness
lift-when-class: internal-refactor
lift-when: a partial unique index on password_reset_pins(user_id) WHERE consumed_at IS NULL ships, with rotate() unique-violation handling and a concurrency regression test
---

# Recovery-PIN single-active invariant is app-enforced, not DB-enforced

`password_reset_pin::rotate` supersedes a user's prior unconsumed PINs by
running `supersede_active` (a `DELETE WHERE consumed_at IS NULL`) and an
`INSERT` inside one transaction. The intended invariant is "at most one active
PIN per user," but the schema only carries the non-unique
`idx_password_reset_pins_user_id`.

Under READ COMMITTED, two concurrent `POST /auth/forgot-password` for the same
account can each run the supersede `DELETE` without seeing the other's
uncommitted `INSERT`, then both commit, leaving two live PINs for one user.

The exposure is low: both PINs land in the legitimate user's recovery channel
(not an attacker's), each is high-entropy, `consume` is atomic
(`consumed_at IS NULL AND expires_at > now()`), and the per-account throttle
bounds guess attempts. This is an invariant-correctness gap, not an open
exploit, so it is accepted until the DB-backed constraint lands.

Lift by adding a partial unique index
(`CREATE UNIQUE INDEX ... ON password_reset_pins (user_id) WHERE consumed_at IS NULL`)
and handling the resulting unique violation in `rotate` (treat the losing
concurrent issuer as a benign race returning the same generic success, never a
500), with a test asserting two concurrent `rotate` calls leave exactly one
active row. A follow-up issue carries the scheduled fix.
