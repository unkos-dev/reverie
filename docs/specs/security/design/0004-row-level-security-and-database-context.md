---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0004"
title: "Row-level security and database context"
satisfies:
  - "REV-REQ-0007"
  - "REV-REQ-0008"
  - "REV-REQ-0009"
  - "REV-REQ-0010"
governed-by:
  - "REV-ADR-0028"
  - "REV-ADR-0017"
---

# Row-level security and database context

This Design covers how a request's or worker's Postgres connection is scoped to one user, or to the system, before it
touches a row-level-security-gated row: the `app.current_user_id` and `app.system_context` settings, `acquire_with_rls`,
the pool factories and the database roles they connect as, every row-level-security policy the migrations define, and
the grants that decide which policies a role can reach at all.

## Purpose and boundaries

This subject owns the mechanism that turns a caller with an identity into a database session that can see or change only
that caller's rows: the `app.current_user_id` and `app.system_context` settings (custom PostgreSQL parameters set with
`set_config`), `acquire_with_rls` in `backend/src/db.rs`, the pool factories `init_pool` and `init_writeback_pool` and
the role each pool connects as, the roles themselves (`docker/init-roles.sql`), and every policy and grant the
migrations create (`backend/migrations/20260810000000_initial_schema.up.sql` and
`backend/migrations/20260903000000_junction_table_rls.up.sql`).

It does not own the migration runner or the `reverie_migrator` identity that runs it (`run_migrations` and
`verify_schema_current` in `backend/src/db.rs`, and the `reverie migrate` entry point in `backend/src/lib.rs`), a
neighbouring subject with no Design yet. It does not own the scope, role and ownership model that assigns ownership to
the data layer; that is the Design "Authorization axes", and this subject is the mechanism behind the ownership axis
wherever that model uses row-level security. It does not own the list query that row-level security filters
(`backend/src/routes/library/mod.rs`, `backend/src/routes/library/filters.rs` and `backend/src/routes/sort_spec.rs`), or
the handler-level ownership predicate that `shelves` and `shelf_items` use instead of a policy
(`backend/src/routes/shelves/mod.rs`).

Depends on: the migrations, run as `reverie_migrator`, for the schema objects, grants and policies; the `users.role`
column the manifestation policies read.

Depended on by: every handler under `backend/src/routes/` that touches a gated table, each of which opens its
transaction through `acquire_with_rls` before querying it; the writeback worker, which takes its connections from
`init_writeback_pool`; and the ingestion pipeline, which reads and writes through the ingestion pool and the
`*_ingestion_full_access` policies instead of `acquire_with_rls`.

## Structure

### Database roles

`docker/init-roles.sql` creates four roles when the database container first starts. A fifth, `reverie`, is the
cluster's bootstrap superuser, which the PostgreSQL image creates from `POSTGRES_USER`.

| Role | Used by | Row-level security |
| ---- | ------- | ------------------ |
| `reverie` | Cluster bootstrap only; never at runtime | Bypasses it (superuser) |
| `reverie_migrator` | `reverie migrate` only (`NOSUPERUSER NOCREATEROLE NOBYPASSRLS`) | Applies, except as table owner |
| `reverie_app` | The request-handling pool and the writeback pool | Applies, scoped per user |
| `reverie_ingestion` | The ingestion pool, when it has its own credentials | Its own unconditional policies |
| `reverie_readonly` | Debugging and reporting connections made by hand | Applies; shares the read policies with `reverie_app` |

Every table is created, and so owned, by the role that runs the migrations: `reverie_migrator` in the shipped setup. No
migration changes ownership, and no table with row-level security enabled uses `FORCE ROW LEVEL SECURITY`. PostgreSQL
exempts a table's owner from its policies unless `FORCE` is set, so `reverie_migrator` can read every row of every table
it created. Requests and background workers never run as `reverie_migrator`: the application process connects as that
role only to apply migrations at startup, and only when `REVERIE_AUTO_MIGRATE` is on; by default migrations run out of
band through `reverie migrate`. A manual connection as that role, however, sees every row unfiltered.

### Pools and the settings contract

`init_pool` in `backend/src/db.rs` opens a plain `PgPool` with no per-connection setup. `AppState::pool`
(`backend/src/state.rs`) is one, connected as `reverie_app`. `AppState::ingestion_pool` is another, built the same way
from `DATABASE_URL_INGESTION`. When that variable is unset, `Config::from_figment` (`backend/src/config/mod.rs`) falls
back to the application's own connection string and sets `ingestion_dsn_defaulted`, and `run` logs a warning at startup:
the ingestion pool then connects as `reverie_app`, the separation between the two roles is gone, and the
`*_ingestion_full_access` policies no longer apply to it. Neither pool scopes a connection by itself; that is the job of
`acquire_with_rls`, per request.

`init_writeback_pool` differs: every connection it opens runs
`SELECT set_config('app.system_context', 'writeback', false)` once, in `after_connect`, before the pool hands it out. It
connects as `reverie_app`, the same role as the request-handling pool; the `system_context` setting, not a separate
role, is what the `manifestations_*_system` policies look for. `backend/src/lib.rs` builds this pool separately and
passes it straight to the writeback worker. It is not a field on `AppState`, so no request handler can reach it. The
setting lasts for a connection's life because each pooled connection stays one PostgreSQL session throughout: the
application pools its own connections in-process, not behind a pooler that shares sessions between transactions.

`acquire_with_rls(pool, user_id)` begins a transaction and runs
`SELECT set_config('app.current_user_id', $1::text, true)` on it. The third argument, `true`, makes the setting local to
the transaction: it ends at `COMMIT` or `ROLLBACK`, so the next borrower of the pooled connection does not inherit it.
Every user-facing handler that touches a gated table is expected to open its transaction this way; no lint, test or CI
check confirms that a new handler does. The routes that touch gated tables (`dashboard`, `enrichment`, `library/mod.rs`,
`library/search.rs`, `metadata.rs`, the `opds` handlers, `preferences`, `reading`, `series`, `shelves` and `suggest`)
each call `db::acquire_with_rls` with `CurrentUser::user_id` before their first query.

### Policy census

Twelve tables enable row-level security. They fall into four groups.

**`manifestations`** has eight policies:

| Policy | Operation | Roles | Rows it admits |
| ------ | --------- | ----- | -------------- |
| `manifestations_select_adult` | SELECT | `reverie_app`, `reverie_readonly` | Every row, when the caller's `users.role` is `admin` or `adult` |
| `manifestations_select_child` | SELECT | `reverie_app`, `reverie_readonly` | When the caller's role is `child`, rows held by a `shelf_items` row on one of the caller's own `shelves` |
| `manifestations_select_system` | SELECT | `reverie_app` | Every row, when `app.system_context` is `writeback` |
| `manifestations_insert` | INSERT | `reverie_app` | The caller's role is `admin` or `adult` |
| `manifestations_update` | UPDATE | `reverie_app` | The caller's role is `admin` or `adult` |
| `manifestations_update_system` | UPDATE | `reverie_app` | Every row, when `app.system_context` is `writeback` |
| `manifestations_delete` | DELETE | `reverie_app` | The caller's role is `admin` or `adult` |
| `manifestations_ingestion_full_access` | ALL | `reverie_ingestion` | Every row |

A `child` caller has no INSERT, UPDATE or DELETE policy on `manifestations`: children change what they can see through
their own `shelf_items` rows, never by editing a manifestation. The policies read the caller's `users.role` only;
`users.is_child` is kept consistent with `role` by a CHECK constraint on `users`, and no policy reads it.

**Junction and per-manifestation tables** (`manifestation_genres`, `manifestation_moods`, `manifestation_tags`,
`manifestation_external_identifiers` and `work_external_identifiers`) share one shape: a SELECT policy for `reverie_app`
and `reverie_readonly` that requires only a matching row in `manifestations` (joined on `work_id` for
`work_external_identifiers`), with no role check of its own; INSERT, UPDATE and DELETE policies for `reverie_app` that
add an `admin` or `adult` role check to that existence check; and an unconditional `*_ingestion_full_access` policy for
`reverie_ingestion`. The existence check is itself subject to the `manifestations` SELECT policies, so a child caller's
view of these tables reaches exactly as far as its view of `manifestations`: a row whose manifestation the child cannot
see fails that check, even though the junction policy names no role or shelf. `manifestation_external_ratings` has only
a SELECT policy and the ingestion policy, matching `reverie_app`'s `SELECT`-only grant on it; ratings are written by the
ingestion pipeline alone.

**Owner-scoped tables** (`reading_state` and `user_preferences`) each have one `ALL` policy (`reading_state_owner`,
`user_preferences_owner`) for `reverie_app` and `reverie_readonly`, matching `user_id = app.current_user_id` in both
`USING` and `WITH CHECK`, so a caller can neither read nor write another account's row. `reverie_ingestion` has no grant
on either table, so the pipeline cannot reach personal rows at all.

**Tables with row-level security enabled and no policy** (`reading_sessions`, `webhooks` and `webhook_deliveries`) deny
every operation to every role except their owner, whatever the grants say. No application role can reach them.

Every SELECT policy except `manifestations_select_system`, and both owner-scoped `ALL` policies, name `reverie_readonly`
alongside `reverie_app`; the INSERT, UPDATE and DELETE policies and the two system-context policies name `reverie_app`
alone. Because `reverie_readonly` holds only `SELECT` grants, a connection as that role sees the same filtered view a
`reverie_app` connection with the same `app.current_user_id` would.

### The grant boundary

Row-level security decides which rows a query sees; the grant decides whether the role may run the query at all, and the
two checks are independent. Five tables have no policy and are granted to `reverie_app` alone, with no grant to
`reverie_readonly` or `reverie_ingestion`: `device_tokens`, `local_credentials`, `instance_bootstrap`,
`local_login_throttle` and `password_reset_pins`. For `device_tokens` and `local_credentials`, which hold hashed token
and password material, the missing `reverie_readonly` grant is the whole boundary against a reporting connection; there
is no policy on either table to narrow or widen.

`reverie_app` holds `SELECT`, `INSERT`, `UPDATE` and `DELETE` on every table it is granted except eight: `SELECT` only
on `identifier_schemes`, `metadata_sources`, `rating_sources`, `manifestation_external_ratings` and the migration
history table `_sqlx_migrations`; `SELECT` and `UPDATE` on `settings`; `SELECT` and `INSERT` on `instance_bootstrap`;
and everything except `DELETE` on `user_preferences`.

`reverie_ingestion` is granted the catalogue and pipeline tables: `works`, `authors`, `work_authors`, `manifestations`,
`series`, `series_works`, `omnibus_contents`, `metadata_versions`, `metadata_sources`, `field_locks`, `tags`,
`manifestation_tags`, `genres`, `manifestation_genres`, `moods`, `manifestation_moods`, `api_cache`, `ingestion_jobs`,
`writeback_jobs`, `identifier_schemes`, `rating_sources`, `manifestation_external_identifiers`,
`manifestation_external_ratings` and `work_external_identifiers`. It has no grant on any account, credential, shelf,
reading, preference, settings or webhook table.

## Interfaces and dependencies

- `acquire_with_rls(pool: &PgPool, user_id: Uuid) -> Result<Transaction, sqlx::Error>` (`backend/src/db.rs`) is the one
  sanctioned way for a user-facing handler to open a scoped transaction.
- `init_pool(database_url, max_connections)` and `init_writeback_pool(database_url, max_connections)` are the two pool
  constructors. The role a pool connects as comes from the connection string it is given, not from the constructor:
  `init_pool` builds both `AppState::pool` and `AppState::ingestion_pool`.
- The PostgreSQL settings interface: `set_config('app.current_user_id', <uuid text>, true)` and
  `current_setting('app.current_user_id', true)`, whose second argument, `missing_ok`, suppresses the
  `unrecognized configuration parameter` error; and the same pair for `app.system_context`, which the policies compare
  with the literal `'writeback'` instead of casting.
- `docker/init-roles.sql` defines the four application roles and their connect and schema-level grants; the migrations
  add the per-table `GRANT` and `CREATE POLICY` statements.

## Data and state

The settings this subject uses exist only in the session memory of a PostgreSQL connection; no table stores them.
`app.current_user_id` is local to the transaction `acquire_with_rls` opened: it takes effect for that transaction only
and ends when the transaction commits or rolls back, so a later borrower of the same pooled connection starts without
it. `app.system_context` is set once per connection, when the writeback pool opens it, not per transaction: it lasts for
the connection's lifetime and `acquire_with_rls` never touches it. A request-handling connection never carries it,
because nothing on the request path sets it.

Outside `init_writeback_pool`, the only code in `backend/src` that sets `app.system_context` is test support
(`backend/src/test_support.rs`). Test support is also the only code that connects as `reverie_readonly`, through its
`readonly_pool_for` factory; no production pool uses that role.

## Runtime behaviour

**An adult or admin reading a manifestation**, for example through `GET /api/v1/books/{id}`:

1. The handler calls `db::acquire_with_rls(&state.pool, current_user.user_id)`, which opens a transaction on the
   `reverie_app` pool and runs `set_config('app.current_user_id', <id>, true)` on it.
2. The handler's `SELECT` against `manifestations` runs. PostgreSQL evaluates the table's SELECT policies;
   `manifestations_select_adult` admits the row because the `users` row for `app.current_user_id` has role `admin` or
   `adult`.
3. The handler commits or rolls back, and `app.current_user_id` ends with the transaction, before the connection returns
   to the pool.

**A child reading the same endpoint** follows the same first and last steps. At step 2, `manifestations_select_adult`
does not match, because the caller's role is `child`, and `manifestations_select_child` applies instead: it also
requires a `shelf_items` row holding the manifestation on a `shelves` row owned by `app.current_user_id`. A
manifestation the child has not been given on a shelf satisfies neither policy and is excluded.

**The writeback worker updating a manifestation's file record:**

1. The worker's connection comes from `init_writeback_pool`, so `app.system_context` was set to `writeback` when the
   pool opened the connection, before any transaction on it began.
2. Its `UPDATE manifestations` runs. `manifestations_update` does not match, because nothing on this connection calls
   `acquire_with_rls` and `app.current_user_id` is unset; `manifestations_update_system` matches, because
   `app.system_context` reads back as `writeback`.
3. The row updates whoever owns or can see it; the system-context policies have no condition beyond that setting.

**Two handlers read through the ingestion pool instead of `acquire_with_rls`**, deliberately:

- The admin ingestion scan (`scan` in `backend/src/routes/ingestion.rs`) calls `CurrentUser::require_admin` and then
  `services::ingestion::scan_once(&state.config, &state.ingestion_pool)`. A scan covers a whole directory with no single
  manifestation or owning caller to scope a transaction to, so the admin check is the only boundary on this path.
- The enrichment dry run (`dry_run` in `backend/src/routes/enrichment.rs`) checks visibility first: it opens an
  `acquire_with_rls` transaction on `state.pool` and runs `SELECT id FROM manifestations WHERE id = $1`, returning
  `AppError::NotFound` when that comes back empty, so a manifestation the caller cannot see looks the same as one that
  does not exist. Only then does it drop the transaction and call
  `services::enrichment::dry_run::preview(&state.ingestion_pool, &state.config, id)`, which reads the manifestation and
  several joined tables over the ingestion pool with no `app.current_user_id` at all. The visibility check and the read
  run on different connections; the ingestion role's unconditional policies are what let the second read succeed.

## Failure and recovery

A read of a gated table in a transaction where `app.current_user_id` is unset never returns a gated row, but it can end
in one of two ways, and which one depends on the pooled connection's history, not on the query:

- On a connection that has never set `app.current_user_id`, PostgreSQL has no value for the custom parameter, so
  `current_setting('app.current_user_id', true)` returns `NULL`. The policies' comparison against `NULL::uuid` is never
  true, so every gated row is excluded and the query returns zero rows.
- On a connection where an earlier transaction set `app.current_user_id` and has since finished, the parameter already
  exists in that session, and once the transaction-local value ends it reads back as the empty string, not `NULL`. A
  later transaction that sets no new value gets `''` from `current_setting`, and the policies' `::uuid` cast fails with
  `invalid input syntax for type uuid`, so the query errors instead of returning rows.

Both outcomes keep gated rows hidden, but the failure is not always quiet, and which one a handler that forgot
`acquire_with_rls` meets depends on what the borrowed connection did before. Nothing catches the omission itself: the
defences are the documentation on `acquire_with_rls` and review of each new handler.

The two ingestion-pool handlers above are not this failure. Both read through `state.ingestion_pool`, whose
unconditional policies give `reverie_ingestion` access without any user context, as long as the pool has its own
credentials. When `DATABASE_URL_INGESTION` is unset and the pool connects as `reverie_app`, its reads meet the ordinary
policies with no user context set, and fail in one of the two ways above.

## Security and operations

The `manifestations_*_system` policies are the only policies that read `app.system_context` instead of
`app.current_user_id`, and outside test support only the connection setup in `init_writeback_pool` sets that value. No
request-handling path sets it, so no request, whatever its credential, role or scope, reaches the system-context
policies. The database does not prevent a `reverie_app` connection from setting the value; the guarantee rests on the
application never doing so outside the writeback pool. The ingestion pool, when it has its own credentials, is the other
path around per-user row-level security: the `reverie_ingestion` role's policies admit every row, and its grants confine
it to the catalogue and pipeline tables.

The ownership axis that REV-ADR-0028 assigns to the data layer takes two forms. `reading_state` and `user_preferences`
enforce it with row-level security, in this subject. `shelves` and `shelf_items` enforce it with a `WHERE user_id = …`
predicate in handler code (`backend/src/routes/shelves/mod.rs`), which this subject relies on but does not own.

An operator who runs the ingestion pipeline needs `DATABASE_URL_INGESTION` set to the `reverie_ingestion` role's
credentials; the startup warning is the signal that it is missing. `reverie_readonly` is for connections made by hand,
for debugging or reporting. Such a connection is refused with a permission error on `device_tokens` and
`local_credentials`, rather than seeing zero rows, because neither table has a policy and the role has no grant on them.

## More information

- [Database migrations](../../../deployment/database-migrations.md): the migration runner and the `reverie_migrator`
  identity this subject depends on.
- [CodeGuard deviation register](../../../security/codeguard/README.md), deviation 4: its compensating controls for EPUB
  ingestion include running the EPUB parser on the ingestion pool.
- [Backend README](../../../../backend/README.md): the per-role local connection strings for development.
