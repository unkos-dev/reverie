-- Enforce the `session_version` force-logout counter's invariant at the schema
-- layer. `session_version` is only ever incremented (security-event bumps in
-- `routes/users`), never set to an arbitrary value; a negative value would mean
-- corruption or an errant reset, and a reset toward/below an already-issued
-- value could revive sessions that force-logout had invalidated.
ALTER TABLE users
    ADD CONSTRAINT users_session_version_nonneg CHECK (session_version >= 0);
