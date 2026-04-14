-- Users
CREATE TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    oidc_subject TEXT UNIQUE NOT NULL,
    display_name TEXT NOT NULL,
    email TEXT,
    role user_role NOT NULL DEFAULT 'adult',
    is_child BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- is_child is an admin-set flag (UI checkbox) that triggers child-related
    -- policies (RLS content filtering). It is intentionally separate from role:
    -- role controls permissions, is_child controls content visibility.
    -- They must stay in sync — enforced by this constraint.
    CONSTRAINT chk_child_role_sync CHECK (
        (is_child = TRUE AND role = 'child') OR (is_child = FALSE AND role != 'child')
    )
);

-- Works (abstract titles — FRBR model)
CREATE TABLE works (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    title TEXT NOT NULL,
    sort_title TEXT NOT NULL,
    description TEXT,
    language TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    search_vector TSVECTOR
);

-- Authors
CREATE TABLE authors (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    sort_name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Work-Author join
CREATE TABLE work_authors (
    work_id UUID NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    author_id UUID NOT NULL REFERENCES authors(id) ON DELETE CASCADE,
    role author_role NOT NULL DEFAULT 'author',
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (work_id, author_id, role)
);

-- Manifestations (concrete files — FRBR model)
CREATE TABLE manifestations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    work_id UUID NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    isbn_10 TEXT,
    isbn_13 TEXT,
    publisher TEXT,
    pub_date DATE,
    format manifestation_format NOT NULL,
    file_path TEXT NOT NULL UNIQUE,
    file_hash TEXT NOT NULL,
    file_size_bytes BIGINT NOT NULL,
    validation_status validation_status NOT NULL DEFAULT 'pending',
    ingestion_status ingestion_status NOT NULL DEFAULT 'pending',
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Grants: tome_app (full DML, all core tables)
GRANT SELECT, INSERT, UPDATE, DELETE ON users TO tome_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON works TO tome_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON authors TO tome_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON work_authors TO tome_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON manifestations TO tome_app;

-- Grants: tome_ingestion (pipeline tables only — no users access)
GRANT SELECT, INSERT, UPDATE, DELETE ON works TO tome_ingestion;
GRANT SELECT, INSERT, UPDATE, DELETE ON authors TO tome_ingestion;
GRANT SELECT, INSERT, UPDATE, DELETE ON work_authors TO tome_ingestion;
GRANT SELECT, INSERT, UPDATE, DELETE ON manifestations TO tome_ingestion;

-- Grants: tome_readonly (SELECT on all)
GRANT SELECT ON users TO tome_readonly;
GRANT SELECT ON works TO tome_readonly;
GRANT SELECT ON authors TO tome_readonly;
GRANT SELECT ON work_authors TO tome_readonly;
GRANT SELECT ON manifestations TO tome_readonly;
