-- UNK-99: gate manifestations system policies on an explicit `app.system_context`
-- GUC instead of "user_id is unset".
--
-- The previous policies (added in 20260419000001) granted reverie_app full
-- access to every row whenever `app.current_user_id` was unset.  That made
-- the writeback worker work, but it also meant any future Axum handler that
-- forgot to `SET LOCAL app.current_user_id` would silently bypass user-level
-- RLS and gain access to every row.
--
-- The new policies require `app.system_context = 'writeback'`.  The writeback
-- worker uses a dedicated pool with an `after_connect` hook that sets that
-- GUC session-scoped on every connection.  No user-facing code path sets
-- `app.system_context`, so a handler that forgets `SET LOCAL app.current_user_id`
-- now matches zero policies and is denied — the desired safety property.

DROP POLICY IF EXISTS manifestations_select_system ON manifestations;
DROP POLICY IF EXISTS manifestations_update_system ON manifestations;

CREATE POLICY manifestations_select_system ON manifestations
    FOR SELECT
    TO reverie_app
    USING (current_setting('app.system_context', true) = 'writeback');

CREATE POLICY manifestations_update_system ON manifestations
    FOR UPDATE
    TO reverie_app
    USING (current_setting('app.system_context', true) = 'writeback')
    WITH CHECK (true);

COMMENT ON TABLE manifestations IS
    'RLS enabled. reverie_app handlers must SET LOCAL app.current_user_id in a transaction. The writeback worker connects via a dedicated pool that sets app.system_context = ''writeback'' (session-scoped via after_connect). reverie_ingestion has unconditional access. reverie (owner) bypasses RLS.';
