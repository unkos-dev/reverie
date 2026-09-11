-- A session ID is the bearer credential the session cookie carries, so the
-- reporting role keeps only the expiry column.
REVOKE SELECT (id) ON TABLE tower_sessions.session FROM reverie_readonly;
