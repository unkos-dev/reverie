# Implementation Report

**Plan**: `.claude/PRPs/plans/hybrid-migration-entrypoints-and-role.plan.md`
**Linear**: UNK-332 (v0.1.0 gate; blocks UNK-331)
**Branch**: `feature/unk-332-reverie-migrator-role-migrate-subcommand`
**Date**: 2026-06-03
**Status**: COMPLETE (pending `backend/CLAUDE.md` approval + commit)

---

## Summary

Replaced the cluster-superuser migration identity with a dedicated
least-privilege `reverie_migrator` role and split migration invocation into a
`reverie migrate` subcommand (out-of-band, the shipped default) plus an opt-in
`REVERIE_AUTO_MIGRATE` in-process startup flag (default false). The default
server path now holds no migration credential and instead runs a read-only
`db::verify_schema_current` check on the app pool, fail-closed in both
directions (schema-ahead AND schema-behind) plus a never-migrated
`NotInitialized` case.

---

## Assessment vs Reality

| Metric     | Predicted     | Actual      | Reasoning                                                              |
| ---------- | ------------- | ----------- | --------------------------------------------------------------------- |
| Complexity | MEDIUM-HIGH   | MEDIUM      | Mechanical 7-site Config ripple; shared set-diff helper kept runner intact |
| Confidence | —             | High        | All tasks landed in dependency order; no pivots; 709 tests green (incl. review-round additions) |

No deviation from the plan's design. One environment workaround: the dev DB is
reachable from this Coder workspace only via the container hostname
`reverie-postgres:5432` (DooD — host port 5433 is not `localhost` here), and
`reverie_migrator` was re-seeded into the running cluster via `docker cp` +
`psql -f` (the bind-mount init caveat).

---

## Tasks Completed

| #  | Task                                                       | Files                                              | Status |
| -- | ---------------------------------------------------------- | -------------------------------------------------- | ------ |
| 1  | `reverie_migrator` role + grants                           | `docker/init-roles.sql`                            | ✅     |
| 2  | Config migration-url tests rewritten + flag tests (TDD)    | `backend/src/config.rs`                            | ✅     |
| 3  | Migration DSN → `Option`; `auto_migrate` flag              | `backend/src/config.rs`                            | ✅     |
| 4  | Config-literal ripple (test_support + 5 service tests)     | `test_support.rs`, 5 `services/**`                 | ✅     |
| 5  | `verify_schema_current` tests first (TDD)                  | `backend/src/db.rs`                                | ✅     |
| 6  | `GRANT SELECT ON _sqlx_migrations TO reverie_app`          | `backend/migrations/20260526000000_initial_schema.up.sql` | ✅ |
| 7  | `verify_schema_current` + `SchemaBehind`/`NotInitialized`  | `backend/src/db.rs`                                | ✅     |
| 8  | `run_migrate()` + flag gating + `main.rs` dispatch         | `backend/src/lib.rs`, `backend/src/main.rs`        | ✅     |
| 9  | Docs (`.env.example`, `docs/schema.md`, new deployment doc, ADR) | see below                                    | ⏳ CLAUDE.md pending approval |
| 10 | Role-attribute + migration-set posture tests              | `backend/src/db.rs`                                | ✅     |
| 11 | Full validation + sqlx cache + ADR Confirmation           | —                                                  | ✅     |

---

## Validation Results

| Check                              | Result | Details                          |
| ---------------------------------- | ------ | -------------------------------- |
| `cargo fmt --check`                | ✅     | clean (one line auto-wrapped)    |
| `cargo clippy --workspace --all-targets --locked -D warnings` | ✅ | 0 warnings |
| `cargo sqlx prepare --workspace --check -- --tests` | ✅ | no drift             |
| `cargo nextest run -p reverie-api` | ✅     | 709 passed, 1 skipped            |
| broken-intra-doc-links             | ✅     | exit 0 (3 pre-existing private-link warns elsewhere) |

New tests: 4 × `verify_schema_current` (ok / ahead / behind / table-absent),
5 × config (optional DSN + flag matrix), `migrator_role_is_least_privilege`,
`migration_set_has_no_superuser_only_operations`.

---

## Key design choices

- **Shared set-diff helper (`diverged_versions`) is pure** — raises no errors;
  the runner consumes only `ahead` (applies `behind`), the verifier refuses
  both. This keeps `run_migrations_inner` behaviour identical (the existing
  `fresh_database_applies_all_migrations` / `schema_ahead_detection` tests pass
  unchanged).
- **Catalog probe before version SELECT** in `verify_schema_current`, so a
  never-migrated DB yields legible `NotInitialized`, not a raw missing-relation
  error.
- **No wildcard in `main.rs` dispatch** — a typo errors non-zero rather than
  silently booting the server (matters in a compose `service_completed_successfully` slot).
- **`run_migrate` installs a best-effort tracing subscriber** — it bypasses
  `run()`, so without it every migration log would drop silently.

---

## Security review

Touches DB roles, config, migration SQL, entrypoint. Will this stand up to
security review? The change is net-hardening: it removes the cluster-superuser
credential from the long-lived application process entirely (default path),
and the migration identity is provably least-privilege (`NOSUPERUSER
NOCREATEROLE NOBYPASSRLS`, asserted by test + `pg_roles`). The schema-behind
refusal is a deliberate fail-closed posture for an exposed multi-user instance.
No secret values surfaced.

---

## Not in this plan (tracked elsewhere)

- Compose migrate sidecar + canonical operator `compose.yml` — UNK-330.
- Homelab env template / `REVERIE_MIGRATOR_PASSWORD` secret / ansible — separate
  cross-repo pass (UNK-331).
- Staging cutover (recreate) — homelab-side.

---

## Next Steps

- [ ] Approve `backend/CLAUDE.md` edits (proposed in session).
- [ ] Commit + push + open PR with `Closes UNK-332`.
- [ ] User reviews and merges.
