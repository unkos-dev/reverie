CREATE TABLE credential_links (
    user_id uuid PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    token_hash bytea NOT NULL UNIQUE,
    expires_at timestamptz NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT credential_links_expires_at_ts_decode_range CHECK (
        expires_at >= '0001-01-01 00:00:00+00'::timestamptz
        AND expires_at < '10000-01-01 00:00:00+00'::timestamptz
    ),
    CONSTRAINT credential_links_created_at_ts_decode_range CHECK (
        created_at >= '0001-01-01 00:00:00+00'::timestamptz
        AND created_at < '10000-01-01 00:00:00+00'::timestamptz
    )
);

GRANT SELECT, INSERT, UPDATE, DELETE ON credential_links TO reverie_app;
