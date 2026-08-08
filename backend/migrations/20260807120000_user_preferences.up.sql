-- Per-user library display preferences: hidden columns, row density, view
-- choice, and default sort stack.
--
-- Every override column is nullable and carries no DEFAULT. NULL means "this
-- user has not customised this group", and the installation default is
-- resolved in Rust when the row is read. A column DEFAULT would instead copy
-- the current default into the row at create time, which permanently pins
-- every existing account to whatever the default was on the day their row
-- appeared; the settings ADR already records that copy-on-create failure mode
-- for system settings. Nullability is also what makes "reset this group"
-- expressible at all, and what tells a customised account apart from an
-- untouched one.
--
-- Rows are created lazily on first write, never at signup: the read path
-- treats a missing row as all-null, so neither account creation nor a
-- backfill for existing accounts is involved.

CREATE TYPE public.library_density AS ENUM (
    'comfortable',
    'compact'
);

CREATE TYPE public.library_view AS ENUM (
    'grid',
    'table'
);

CREATE TABLE public.user_preferences (
    user_id uuid NOT NULL,
    -- Column keys the table view hides, stored as written. The view
    -- intersects them with the columns it actually has, so a key left over
    -- from an older column catalog is inert rather than an error.
    hidden_columns text[],
    density public.library_density,
    view public.library_view,
    -- The user's default sort, in the same JSON:API wire grammar the
    -- `?sort=` parameter uses: comma-separated field names, a leading `-`
    -- marking descending, priority by position. This is the default only;
    -- the URL carries the current sort and always wins when present.
    sort_stack text,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT user_preferences_pkey PRIMARY KEY (user_id),
    CONSTRAINT user_preferences_user_id_fkey
        FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE,
    -- Shape backstop, not the validation gate: the handler parses
    -- `sort_stack` against the whitelisted sort columns and rejects an
    -- unknown field with 422. This CHECK is what keeps a value written by
    -- any other route inside the grammar. Three levels matches the sort
    -- parser's own cap.
    CONSTRAINT user_preferences_sort_stack_shape
        CHECK (sort_stack ~ '^-?[a-z_]+(,-?[a-z_]+){0,2}$'),
    -- Bounds how much one account can store here. The column catalog offers
    -- eight hideable columns; the cap is loose enough to outlive several
    -- catalog revisions and tight enough that the column is not an
    -- unbounded write target.
    CONSTRAINT user_preferences_hidden_columns_bounded
        CHECK (array_length(hidden_columns, 1) <= 64),
    CONSTRAINT user_preferences_updated_at_ts_decode_range
        CHECK (updated_at >= '0001-01-01 00:00:00+00' AND updated_at < '10000-01-01 00:00:00+00')
);

-- ---- row-level security ----

-- ENABLE is load-bearing: CREATE POLICY alone is inert, and a table with
-- policies but no ENABLE stays fully readable and writable through its
-- grants. ENABLE-only (never FORCE) matches the rest of the schema, which is
-- sound because the application connects as non-owner roles.
--
-- The predicate is `reading_state_owner` verbatim, the schema's established
-- owner-scoped shape. `current_setting(..., true)` passes missing_ok, so an
-- unset GUC resolves to NULL and the comparison fails closed (deny) instead
-- of raising. WITH CHECK repeats USING so a caller can neither insert a row
-- owned by someone else nor move one out of their own ownership.
ALTER TABLE public.user_preferences ENABLE ROW LEVEL SECURITY;

CREATE POLICY user_preferences_owner ON public.user_preferences
    TO reverie_app, reverie_readonly
    USING ((user_id = ((SELECT current_setting('app.current_user_id'::text, true)))::uuid))
    WITH CHECK ((user_id = ((SELECT current_setting('app.current_user_id'::text, true)))::uuid));

-- ---- grants ----

-- No DELETE: resetting a group writes NULL into it, and no code path removes
-- a preferences row. Account deletion reaches it through the FK cascade,
-- which needs no grant. reverie_readonly gets SELECT per the role table, and
-- the policy above confines it to the same owner scope.
GRANT SELECT,INSERT,UPDATE ON TABLE public.user_preferences TO reverie_app;
GRANT SELECT ON TABLE public.user_preferences TO reverie_readonly;
