---
type: DESIGN
profile-version: 1
id: "REV-DESIGN-0019"
title: "Books list query contract"
satisfies:
  - "REV-REQ-0003"
  - "REV-REQ-0057"
  - "REV-REQ-0058"
  - "REV-REQ-0059"
governed-by:
  - "REV-ADR-0019"
  - "REV-ADR-0037"
  - "REV-ADR-0038"
---

# Books list query contract

This Design covers the server half of `GET /api/v1/books`: the typed per-column filter grammar and its
decode-versus-validation split, the whitelisted multi-level sort stack, the tagged opaque cursor family that binds a
page boundary to the exact sort stack and filter set it was minted under, and the dynamic query this all assembles. The
client half — how the browser represents, writes and round-trips this same state — is a separate subject, the Design
"Library filter and sort state", which disclaims owning any of the server-side contract this Design describes.

## Purpose and boundaries

This subject owns three cooperating modules and the list handler that ties them together, and is the server side of one
obligation this subject shares with a neighbour: REV-REQ-0003 is satisfied jointly with the Design "Library filter and
sort state", which owns the client's tolerance of a rejected value; this Design owns the server's rejection of it.
`backend/src/routes/library/filters.rs` owns the flat suffix-operator filter grammar, its semantic validation, and the
canonical string and fingerprint that bind a cursor to the active filter set. `backend/src/routes/sort_spec.rs` owns the
closed `SortColumn` whitelist and the `?sort=` grammar that resolves a client string against it.
`backend/src/routes/cursor.rs` owns the tagged, base64url opaque cursor family: `SortCursor` (tag `m`) for this
endpoint, and the two fixed-shape cursors that share its wire encoding and error type, `ShelfCursor` (tag `sh`) and
`ShelfItemCursor` (tag `si`). `list` in `backend/src/routes/library/mod.rs` owns the query assembly that spans all
three: decoding and validating the request, building the `WHERE`/`ORDER BY`/keyset-predicate clauses of one dynamic
`QueryBuilder`, and minting the following page's cursor from the result.

It does not own how those decoded values become response rows. Once `list` has its page of `manifestations` rows, the
batch loads that hydrate authors, contributors, tags, genres, moods, reading state, external identifiers and ratings,
and the `BookListRow` shape itself (`backend/src/models/library.rs`), are the list endpoint's response contract, not its
query contract; this Design names those functions only where they mark the query's own boundary. It does not own
`detail` or `work_detail` in the same file (`GET /api/v1/books/{id}` and `GET /api/v1/works/{id}`), which share no
filter, sort or cursor state with `list`. It does not own the Search and vocabulary suggest subject's ranked full-text
search and vocabulary-suggest endpoints, mounted by the same router (`backend/src/routes/library/search.rs`, merged into
`router()` via `search::router()`); a doc comment on `filters.rs`'s `MAX_TEXT_FILTER_CHARS` constant notes that the `q`
filter's length cap "mirrors the search endpoint's `MAX_Q_LEN`", a convention the two subjects share without either
owning the other. It does not own row-level security itself — the `app.current_user_id` mechanism, the pools, and the
policy census belong to the Design "Row-level security and database context" — nor the three-axis scope/role/ownership
model or the map of which resources rely on row-level security versus a handler-level predicate, which belongs to the
Design "Authorization axes"; this subject calls the first and matches a pattern the second records. It does not own
`reading_state`'s own read/write endpoints or its patch semantics, which belong to the Design "Reading state"; the
status and rating filters here only read that table. It does not own the client-side filter and sort state this contract
serves, or the `shelves`/`shelf_items` handlers that consume `ShelfCursor` and `ShelfItemCursor`, which belong to the
Design "Shelves".

Depends on: `crate::db::acquire_with_rls` to scope the transaction the list query runs in; the
`manifestations_select_adult`/`manifestations_select_child` row-level-security policies that filter which rows the query
ever sees, whatever `WHERE` clause this subject builds; the `reading_state_owner` policy that scopes the status and
rating filters' `EXISTS` probes to the caller's own rows; the junction-table policies on `manifestation_tags`,
`manifestation_genres` and `manifestation_moods` that scope the vocabulary filters' probes to a visible manifestation;
and the operator setting `REVERIE_OPDS_PAGE_SIZE` (`state.config.opds.page_size`) for the page size, a setting this
endpoint shares with the OPDS catalogue subject despite its name.

Depended on by: the Design "Library filter and sort state", whose `frontend/src/routes/library-params.ts` codec and
`ListBooksParams` client target exactly this wire contract; the Design "Shelves", whose `add_shelf_item`, `ShelfCursor`
and `ShelfItemCursor` reuse this subject's cursor tag family and its `build_next_url`/`split_page` pagination helpers
even though shelves are ordered and filtered on their own terms; and every reader of the library grid, table and list
views, indirectly, through the client Design above.

## Structure

### Filter grammar (`filters.rs`)

`ListParams` (private to `library::mod`, `backend/src/routes/library/mod.rs`) is the wire shape: one field per suffix
condition (`title_contains`, `pages_gte`, `status_any`, and so on), decoded by `axum_extra::extract::Query` rather than
`axum::Query` so a repeated key such as `?tag=a&tag=b` extends a `Vec` instead of failing. Every field decodes at the
extractor boundary; a value that cannot decode into its declared type (an ill-formed UUID, a non-integer, a non-
boolean, or a date that is not `YYYY-MM-DD`) never reaches this module at all; the date fields route through
`filters::iso_date_opt::deserialize`, which accepts only the calendar-date spelling via `parse_iso_date` (three integer
components fed to `NaiveDate::from_ymd_opt`) so a datetime string or a two-component date fails deserialisation the same
as a non-numeric one.

`validate(&ListParams)` runs once the request has fully decoded, and is the sole home of the semantic-bound checks: a
multi-value list (`tag`, `genre`, `mood`, `author`, `status_any`/`status_none` and each list's `_any`/`_none` variants,
fourteen fields in total) longer than `MAX_TAG_FILTERS` (20, defined in `library::mod`); a trimmed text value (`q`,
`title_contains`, `title_eq`, `title_ne`, `subtitle_contains`, `isbn_13_contains`, `isbn_13_eq`) longer than
`MAX_TEXT_FILTER_CHARS` (200); a rating bound outside `1..=5`; a negative `pages_gte`/`pages_lte`; a
`status_any`/`status_none` token that is neither a `ReadingStatus::from_wire` name nor the literal `unread`; and,
checked after every individual bound, an inverted pair on each of the three two-sided ranges the grammar exposes:
`pages_gte` greater than `pages_lte`, `rating_gte` greater than `rating_lte`, and `created_at_gte` greater than
`created_at_lte`. Each of the three inverted-range checks returns its own message
(`"pages_gte must not exceed pages_lte"` and the equivalent wording for the rating and `created_at` pairs); equal bounds
pass every check.

`push_filter_predicates` is the single entry point that appends every active condition onto the query's `WHERE` clause,
in a fixed order (author triple, then `series`, then `shelf`, then the tag/genre/mood vocabulary triples, then text,
pages, `created_at`, status, rating, and finally `q`); the order does not change the AND-combined semantics. Each
condition family has its own pushing function (`push_author_predicates`, `push_vocab_predicates`,
`push_text_predicates`, `push_pages_predicates`, `push_created_at_predicates`, `push_status_predicates`,
`push_rating_predicates`, `push_q_predicate`), and every one binds its user-supplied value with `push_bind`; no
condition ever writes a column name from client input. `ListParams` carries no `#[serde(deny_unknown_fields)]`, so an
unrecognised suffix parameter is silently dropped by the `Query` extractor and never reaches this module, rather than
failing deserialisation. The `shelf` condition is the one filter whose predicate carries its own ownership check rather
than leaning on a table's row-level-security policy: `shelves` has none, so the pushed `EXISTS` joins `shelf_items` to
`shelves` and adds `AND s.user_id = current_setting('app.current_user_id', true)::uuid` inline, the same predicate shape
the Design "Shelves" records for that table's own handlers.

Text matching is case-insensitive `ILIKE` throughout. The `_contains` legs on `title` and `subtitle`, and the `q`
filter, additionally fold accents through `immutable_unaccent`/`immutable_unaccent_like` on the SQL side, escaping after
folding rather than before, because the unaccent dictionary can map some Unicode punctuation into `%`/`_`/`\` and would
undo an escape applied first; `isbn_13_contains` and the `_eq`/`_ne` legs stay accent-sensitive and escape in Rust via
`escape_like` before binding.

`canonical_filter_string` and `filter_fingerprint` (SHA-256 truncated to the first 16 hex characters, 8 bytes) turn the
active filter set into the cursor's filter-fingerprint leg. Every multi-value list is sorted with repeats removed so
neither parameter order nor value order changes the string, and every free-text value and list entry is escaped through
`escape_canon` (backslash first, then `&`, `=` and `,`) so a value containing one of the string's own structural
delimiters cannot make two differently-shaped filter sets reduce to the same string and share a fingerprint.

### Sort whitelist (`sort_spec.rs`)

`SortColumn` is a closed, `#[non_exhaustive]` enum of four variants — `Title`, `Author`, `CreatedAt`, `Pages` — each
carrying its wire name (`wire_name`/`from_wire`), its fixed SQL expression (`sql_expr`, only ever a `&'static str`),
whether it is nullable (`nullable`, true only for `Author` and `Pages`), the row alias it is selected under for cursor
minting (`select_alias`), and the value domain a cursor boundary for it must carry (`value_kind`, one of
`SortValueKind::Text`/`Timestamp`/`Int`). The module's own doc comment states the entry condition for a fifth variant:
"a column enters `SortColumn` only once its ordering indexes exist," specifically both an ascending and a
`DESC NULLS LAST` composite index for a nullable column, because Postgres's default `DESC` ordering is `NULLS FIRST` and
a backward scan of an ascending index cannot supply `NULLS LAST` on its own. `Author` (`works.first_author_sort_name`)
and `Pages` (`manifestations.pages`) each carry both index shapes today
(`idx_works_first_author_sort_id`/`idx_works_first_author_sort_desc`,
`idx_manifestations_pages_keyset`/`idx_manifestations_pages_keyset_desc`). `Title` and `CreatedAt` are declared
`NOT NULL` in the schema, so each carries one composite index only (`idx_works_sort_title_id` ascending,
`idx_manifestations_recent_keyset` descending): a backward scan of either satisfies the opposite direction cleanly,
because there is no null bucket whose position could disagree between the two scan directions.

`SortSpec::parse` is the sole entry point from a caller-supplied `?sort=` string: it splits on `,`, caps the result at
`MAX_SORT_LEVELS` (3), strips a leading `-` to mean descending, resolves each remaining name through
`SortColumn::from_wire` (case-sensitive; an unmatched name, including an empty one from a stray comma or a lone `-`, is
`SortSpecError::UnknownField`), and rejects a column named twice (`SortSpecError::Duplicate`). `SortSpec::default()` is
a single descending `CreatedAt` level, matching the endpoint's behaviour when `?sort=` is absent. `SortSpec::canonical`
renders the parsed stack back to the same wire syntax it was parsed from, and is what both the `Link`/`next_cursor` echo
and the cursor's embedded spec string reuse, so a stack that parsed successfully always round-trips to the same string
it was parsed from.

### Cursor families (`cursor.rs`)

Every cursor in this module is a base64url-unpadded (`Base64UrlUnpadded`) encoding of `<tag>|<rest>`, and every
`parse`/`parse_for` checks its own tag first: `SortCursor::parse_for` accepts only `m`, `ShelfCursor::parse` only `sh`,
`ShelfItemCursor::parse` only `si`; any other tag, including a well-formed cursor from one of the other two families, is
`CursorError::UnknownTag` before any further field is read. All three share one `CursorError` enum, but `SortMismatch`
and `FilterMismatch` are constructed only inside `SortCursor::parse_for`: `ShelfCursor` and `ShelfItemCursor` carry no
embedded spec or filter fingerprint to mismatch, so neither variant is reachable from their `parse` functions.

`SortCursor` carries a JSON payload — the canonical spec string, one `CursorValue` per sort level in level order, the
`manifestations.id` tiebreaker, and the filter fingerprint (serialised as `f`) — rather than the pipe-delimited fields
`ShelfCursor` and `ShelfItemCursor` use, because a sort stack's boundary values are free text (a title or an author sort
key) that pipe-splitting cannot safely delimit. `CursorValue` is externally tagged (`Text(Option<String>)`,
`Ts(DateTime<Utc>)`, `Int(Option<i32>)`) so a forged JSON payload cannot smuggle a value of the wrong SQL type past the
type check; its `None` arms on `Text`/`Int` mark a boundary sitting in a nullable column's `NULLS LAST` bucket, kept
distinct from an empty string so a three-valued SQL comparison cannot collapse the two and drop rows.

`validate_keys` is the one key-count, kind and null-handling check shared by both directions: `SortCursor::for_spec`
(the mint path, `pub(crate)` so only server code can construct a cursor) and `SortCursor::parse_for` (the decode path)
both call it against the same `SortSpec`, so a cursor this module mints can never be one its own decode would reject.
Beyond that shared check, encoding is infallible in practice: `CursorError::SerializePayload` covers a key variant that
is not among the ones `SortCursor` holds today, so every value it holds serialises without error and no code path
returns this variant.

### Query assembly (`library/mod.rs`)

The `list` handler runs its checks in a fixed order before it ever opens a transaction: `Query<ListParams>` decode,
`filters::validate`, `SortSpec::parse` (with parse errors explicitly re-wrapped as `AppError::MalformedQuery`, not left
as `AppError::Validation`), the filter fingerprint, then `SortCursor::parse_for` against that spec and fingerprint. Only
once all of that has succeeded does it call `db::acquire_with_rls` and build the query: a fixed `SELECT` naming every
column the response needs plus a `LEFT JOIN LATERAL` that picks at most one series per work (lowest position,
`NULLS LAST`, then series id, so a work in several series always reports the same one across pages),
`filters::push_filter_predicates`, the general keyset predicate, the `ORDER BY`, and `LIMIT page_size + 1` — one extra
row fetched so `split_page` can tell whether a further page exists without a second query.

`push_cursor_predicate` builds the keyset "advance past the boundary" clause for an arbitrary sort stack as one
`OR`-chain of per-level branches: level `i`'s branch requires exact equality on every level before it
(`push_boundary_equality`) and a strict advance on level `i` itself (`push_boundary_advance`), and a final branch
requires equality on every level plus a strict advance on `m.id`, whose direction follows the stack's last level. A
nullable level whose own cursor boundary is already the `NULL` bucket contributes no branch of its own: `NULLS LAST`
means every `NULL` row already sits at the tail regardless of direction, so nothing can be "after" a `NULL` boundary
within that level, and a deeper level or the `id` tiebreaker is what continues the walk. `push_boundary_advance` wraps a
nullable, non-`NULL` boundary as `(expr OP $bind OR expr IS NULL)` in both directions, because `NULLS LAST` puts the
entire null bucket after every non-null value under either ordering, and a bare comparison would drop it the first time
a non-null boundary is used. `push_order_by` mirrors the same null-handling and tiebreaker-direction rules for the
`ORDER BY` clause itself.

`next_cursor_for_row` mints the following page's cursor from the last row of a page that had more rows than `page_size`:
it reads each level's boundary value back off the row via its `select_alias` and `decode_cursor_column` (a `try_get`
wrapper, because this dynamic-`QueryBuilder` path has no compile-time schema check and a decode failure must become a
clean `AppError::Internal` rather than a panic), builds the matching `CursorValue`, and calls `SortCursor::for_spec`
with the same filter fingerprint the request was validated against. `build_next_url` rewrites the request's own query
string for both the `Link: rel="next"` header and the body's `next_cursor` echo, replacing or appending `cursor` and
leaving every other parameter, including `sort`, untouched.

## Interfaces and dependencies

`GET /api/v1/books` (`list`, `#[utoipa::path]` on it) is the interface this subject builds: `ListParams`'s
`#[derive(utoipa::IntoParams)]` documents every filter, `sort` and `cursor` parameter in the generated OpenAPI spec, and
the `#[utoipa::path]` annotation declares the `400`, `401` and `422` response classes this Design's failure modes
produce, plus the `200` body (`BookListResponse`) and its `Link` header. `axum_extra::extract::QueryRejection` is the
framework boundary this subject relies on for decode failures; the crate-wide `impl From<QueryRejection> for AppError`
(`backend/src/error/mod.rs`) is what turns a rejected field into `AppError::MalformedQuery` with a `400`, a mapping the
Design "API error contract and OpenAPI" owns and this subject only consumes.

`crate::db::acquire_with_rls(pool, user_id)` is the sole transaction-scoping call `list` makes; its settings and the
policies it activates belong to the Design "Row-level security and database context".
`state.settings.read().await.provider_visibility`, read once per request to build the `hidden_providers` set for the
external-identifier and -rating lookups that follow the page query, is part of the response contract this subject hands
off rather than owns.

`SortSpec::canonical` and `SortCursor`/`CursorValue` are consumed on the client side only as opaque strings: the Design
"Library filter and sort state" builds the `sort` query parameter from its own resolved sort stack and treats `cursor`
and `next_cursor` as values to store and replay, never to parse.

## Data and state

Nothing here is durable. `ListParams`, the parsed `SortSpec`, the filter fingerprint and the decoded `SortCursor` are
all built fresh from one request's query string and discarded once that request's response is written; the server holds
no session or store keyed to a cursor. The cursor itself carries every piece of state its own continuation needs — the
spec string, one typed boundary value per level, the `id` tiebreaker, and the filter fingerprint — encoded with no
signature: `cursor.rs`'s own module doc records this plainly ("No HMAC — same trust model as the OPDS cursor").

`MAX_SORT_LEVELS` (3), `MAX_TAG_FILTERS` (20) and `MAX_TEXT_FILTER_CHARS` (200) are compile-time constants; changing any
of them is a code change, not a runtime setting. The one runtime-configurable value this subject reads is the page size,
`state.config.opds.page_size` (`REVERIE_OPDS_PAGE_SIZE`, default 50) — named for the OPDS catalogue subject it was first
added for, but read identically here, so an operator who changes it changes both surfaces' page size at once.

## Runtime behaviour

**A typed range filter**, `GET /api/v1/books?pages_gte=200&pages_lte=400`:

1. `Query<ListParams>` decodes both values as `i32`; a non-integer value here would instead reject at this step, with
   `list` never running past its first line (`let Query(params) = params?;`).
2. `filters::validate` checks each bound is non-negative, then compares the two bounds against each other: `pages_gte`
   (200) does not exceed `pages_lte` (400), so this range passes. Had the request instead named `pages_gte` above
   `pages_lte`, `validate` would return `AppError::Validation("pages_gte must not exceed pages_lte")`, a `422`, and
   `push_filter_predicates` would never run.
3. `push_filter_predicates` calls `push_pages_predicates`, which appends `AND m.pages >= $1` then `AND m.pages <= $2`
   (each `$n` a bound parameter, never interpolated text) onto the query built so far, alongside whatever other filters,
   the keyset predicate and the `ORDER BY`/`LIMIT` that follow.
4. The query executes inside the RLS-scoped transaction, returning every visible manifestation whose `pages` value falls
   between 200 and 400 inclusive.

**A three-level sort stack with a nullable level, minted then replayed under a changed stack**,
`GET /api/v1/books?sort=-created_at,title,pages`:

1. `SortSpec::parse` splits on `,`, matches three distinct wire names against the whitelist, and returns levels
   `[CreatedAt(Desc), Title(Asc), Pages(Asc)]` — three levels, at the cap, no duplicates.
2. `push_order_by` emits `ORDER BY m.created_at DESC, w.sort_title ASC, m.pages ASC NULLS LAST, m.id ASC`: `Pages` is
   nullable so it carries the explicit `NULLS LAST`, and the final tiebreaker takes the last level's direction,
   ascending.
3. Once a full page has fetched with more rows remaining, `next_cursor_for_row` reads the last row's `created_at`,
   `sort_title` and `pages` (the last possibly `None`, if that row has no page count) via each level's `select_alias`,
   and calls `SortCursor::for_spec` with the canonical spec string `"-created_at,title,pages"` and the request's filter
   fingerprint; the result encodes as `m|<json>` under base64url.
4. The client replays that cursor on the next request with the same `?sort=` value. `SortCursor::parse_for` decodes the
   tag (`m`, matching), the JSON payload, confirms `cursor.spec` equals the freshly parsed `spec.canonical()` and
   `cursor.filter_fp` equals the current request's fingerprint, then `validate_keys` re-confirms three levels of the
   right kinds. `push_cursor_predicate` builds the three-branch-plus-tiebreaker `OR` cascade; if the boundary row's
   `pages` value was `None`, that level's own branch is skipped (nothing sorts "after" the null bucket at that level)
   and the walk continues on the `id` tiebreaker or a still-deeper level.
5. If the client instead requests `?sort=title,-created_at,pages` and replays the same cursor string unchanged,
   `cursor.spec` (`"-created_at,title,pages"`) no longer equals the new `spec.canonical()`
   (`"title,-created_at,pages"`). `parse_for` returns `CursorError::SortMismatch`, `list` maps it to
   `AppError::Validation` (a `422`), and the request never reaches `acquire_with_rls` or touches the database.

## Failure and recovery

- **A filter or path-level query parameter that fails to decode** (an ill-formed UUID on `author`/`series`/`shelf`, a
  non-integer on a numeric filter, a non-`YYYY-MM-DD` value on a date filter, an unrecognised boolean literal on an
  `_empty` flag) is rejected by the `Query<ListParams>` extractor before `list`'s body runs, as
  `AppError::MalformedQuery` (`400`), via the crate-wide `From<QueryRejection>` conversion.
- **A `?sort=` value that names an unwhitelisted field, repeats a column, or exceeds three levels** decodes as a
  syntactically valid string (`sort` is a plain `Option<String>` at the extractor boundary) but fails inside
  `SortSpec::parse`; `list` re-wraps every `SortSpecError` as `AppError::MalformedQuery`, the same `400` class as a
  decode failure, rather than as `AppError::Validation`. This differs from `filters::validate`'s semantic-bound checks,
  which use `422`, even though an unwhitelisted sort field and an over-cap filter list are both "the value named
  something outside what the server accepts."
- **A filter value that decodes but violates a semantic bound** — an over-cap multi-value list, over-long trimmed text,
  an out-of-range rating, a negative page bound, an inverted `pages`, `rating` or `created_at` range, or a
  `status_any`/`status_none` token that is neither a real status nor `unread` — is rejected by `filters::validate` as
  `AppError::Validation` (`422`), before any query is built.
- **A cursor that fails to decode, names the wrong tag, or disagrees with the request's sort or filter set** — bad
  base64, non-UTF-8 bytes, a missing tag delimiter, a tag other than `m`, a JSON payload that will not parse or whose
  key count, kinds or null-handling disagree with the freshly parsed `SortSpec`, an embedded spec string that does not
  match the current `?sort=`, or an embedded filter fingerprint that does not match the current filter set — all map to
  the single `list` call site `.map_err(|e| AppError::Validation(format!("invalid cursor: {e}")))`, a `422`. Unlike the
  query-parameter split above, every shape of cursor failure lands on the same status and error variant; a malformed
  cursor and a stale-but-well-formed one are indistinguishable to the caller by status code alone.
- **A decode failure on `content_rating`, `validation_status`, or the `series_id`/`series_name`/`series_position`
  triple.** Each is read via the fallible `try_get` rather than the panicking `Row::get`, because a Rust enum or a
  numeric type here cannot represent every value its database column could someday hold; a decode failure surfaces as
  `AppError::Internal` (`500`), logged with `tracing::warn!`/`tracing::error!` at the call site, rather than panicking.
  Every other column the row-to-response mapping reads (`title`, `subtitle`, `isbn_13`, `pages`, `created_at`,
  `work_id`, `id`, and the raw `ingestion_status`/`enrichment_status` strings `parse_ingestion`/`parse_enrichment`
  parse) uses the panicking `Row::get`, which would panic rather than return an error if the query's own `SELECT` list
  and this code's expectations of it ever drifted apart.
- **Any lower-level `sqlx::Error`** from the query itself (a connection failure, a constraint the dynamic query somehow
  violates) surfaces the same way, as `AppError::Internal` (`500`).

## Security and operations

Column names are never client input on either the filter or the sort side: `filters.rs`'s own module doc states the
invariant plainly ("Injection is unrepresentable by construction, matching the sort whitelist"), and every predicate
this subject builds writes a fixed `&'static str` fragment for the column and passes every value through `push_bind`.
The same holds for `sort_spec.rs`: `SortColumn::sql_expr` returns only compile-time constants, and `SortSpec::parse`
resolves a client string against `SortColumn::from_wire` before any of it reaches a `QueryBuilder`.

The cursor carries no signature, by the same trust model the OPDS cursor uses: a hand-crafted cursor cannot expose a row
the same request without one could not already return, because the underlying `SELECT` is scoped by row-level security
before the cursor's own predicate is layered on top, and the cursor only ever narrows or moves the window within that
already-visible set — it never widens it. Forging one lets a caller start their own walk at an arbitrary point in the
sort order, which is no more than choosing which of their own visible rows to see first.

The `shelf` filter is the one place in this subject that enforces ownership itself rather than leaning on a table's
row-level-security policy, because `shelves` has none: its `EXISTS` predicate joins `shelves` and checks
`s.user_id = current_setting('app.current_user_id', true)::uuid` inline, matching the ownership predicate the Design
"Shelves" records for that table's own handlers. Every other filter that reaches a table row-level security does not
cover — the tag, genre and mood vocabulary probes against their junction tables, and the status and rating probes
against `reading_state` — relies on that table's own policy (a junction table's `SELECT` policy requires a matching,
visible `manifestations` row; `reading_state_owner` scopes to the caller) rather than repeating a `user_id` check here.

`list` declares `security(("session_cookie" = ["read"]), ...)` in its OpenAPI annotation and calls no `require_scope`,
`require_admin` or `require_not_child` of its own: every resolved credential carries at least `read` scope, so no
in-handler gate is needed for a read-only operation, a pattern the Design "Authorization axes" records project-wide.
Row-level security, not this handler, is what narrows a child account's visible rows (`manifestations_select_child`) to
those reachable through the child's own shelves.

This subject exposes no operational surface of its own beyond the shared `REVERIE_OPDS_PAGE_SIZE` setting recorded under
Data and state; the transaction, connection and role it runs under are the operational concern of the Design "Row-level
security and database context".
