-- Reverses 20260421000002_writeback_system_context_guc.up.sql by restoring
-- the previous "user_id is unset" gating from 20260419000001.

DROP POLICY IF EXISTS manifestations_select_system ON manifestations;
DROP POLICY IF EXISTS manifestations_update_system ON manifestations;

CREATE POLICY manifestations_select_system ON manifestations
    FOR SELECT
    TO reverie_app
    USING (
        current_setting('app.current_user_id', true) IS NULL
        OR current_setting('app.current_user_id', true) = ''
    );

CREATE POLICY manifestations_update_system ON manifestations
    FOR UPDATE
    TO reverie_app
    USING (
        current_setting('app.current_user_id', true) IS NULL
        OR current_setting('app.current_user_id', true) = ''
    )
    WITH CHECK (true);

COMMENT ON TABLE manifestations IS
    'RLS enabled. reverie_app and reverie_readonly must SET LOCAL app.current_user_id in a transaction. reverie_ingestion has unconditional access. reverie (owner) bypasses RLS.';
