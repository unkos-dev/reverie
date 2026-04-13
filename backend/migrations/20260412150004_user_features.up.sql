-- Shelves (per-user collections)
CREATE TABLE shelves (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    is_system BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Shelf items
CREATE TABLE shelf_items (
    shelf_id UUID NOT NULL REFERENCES shelves(id) ON DELETE CASCADE,
    manifestation_id UUID NOT NULL REFERENCES manifestations(id) ON DELETE CASCADE,
    added_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (shelf_id, manifestation_id)
);

-- Device tokens (OPDS/reader auth)
CREATE TABLE device_tokens (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    last_used_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    revoked_at TIMESTAMPTZ
);

-- Grants: tome_app only (user-facing tables — no ingestion access)
GRANT SELECT, INSERT, UPDATE, DELETE ON shelves TO tome_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON shelf_items TO tome_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON device_tokens TO tome_app;

-- Grants: tome_readonly (SELECT — excludes device_tokens to protect token_hash)
GRANT SELECT ON shelves TO tome_readonly;
GRANT SELECT ON shelf_items TO tome_readonly;
