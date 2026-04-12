-- Series with self-referential nesting
CREATE TABLE series (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    sort_name TEXT NOT NULL,
    parent_id UUID REFERENCES series(id) ON DELETE SET NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Series-Works join
CREATE TABLE series_works (
    series_id UUID NOT NULL REFERENCES series(id) ON DELETE CASCADE,
    work_id UUID NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    position NUMERIC,
    is_omnibus BOOLEAN NOT NULL DEFAULT FALSE,
    note TEXT,
    PRIMARY KEY (series_id, work_id)
);

-- Omnibus contents mapping
CREATE TABLE omnibus_contents (
    omnibus_manifestation_id UUID NOT NULL REFERENCES manifestations(id) ON DELETE CASCADE,
    contained_work_id UUID NOT NULL REFERENCES works(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (omnibus_manifestation_id, contained_work_id)
);

-- Metadata versioning
CREATE TABLE metadata_versions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    manifestation_id UUID NOT NULL REFERENCES manifestations(id) ON DELETE CASCADE,
    source metadata_source NOT NULL,
    field_name TEXT NOT NULL,
    old_value JSONB,
    new_value JSONB,
    status metadata_review_status NOT NULL DEFAULT 'draft',
    confidence_score REAL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    resolved_at TIMESTAMPTZ,
    resolved_by UUID REFERENCES users(id) ON DELETE SET NULL
);

-- Tags
CREATE TABLE tags (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    tag_type tag_type NOT NULL,
    UNIQUE (name, tag_type)
);

-- Manifestation-Tags join
CREATE TABLE manifestation_tags (
    manifestation_id UUID NOT NULL REFERENCES manifestations(id) ON DELETE CASCADE,
    tag_id UUID NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (manifestation_id, tag_id)
);

-- Grants: tome_app (full DML)
GRANT SELECT, INSERT, UPDATE, DELETE ON series TO tome_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON series_works TO tome_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON omnibus_contents TO tome_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON metadata_versions TO tome_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON tags TO tome_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON manifestation_tags TO tome_app;

-- Grants: tome_ingestion (pipeline tables)
GRANT SELECT, INSERT, UPDATE, DELETE ON series TO tome_ingestion;
GRANT SELECT, INSERT, UPDATE, DELETE ON series_works TO tome_ingestion;
GRANT SELECT, INSERT, UPDATE, DELETE ON omnibus_contents TO tome_ingestion;
GRANT SELECT, INSERT, UPDATE, DELETE ON metadata_versions TO tome_ingestion;
GRANT SELECT, INSERT, UPDATE, DELETE ON tags TO tome_ingestion;
GRANT SELECT, INSERT, UPDATE, DELETE ON manifestation_tags TO tome_ingestion;

-- Grants: tome_readonly (SELECT)
GRANT SELECT ON series TO tome_readonly;
GRANT SELECT ON series_works TO tome_readonly;
GRANT SELECT ON omnibus_contents TO tome_readonly;
GRANT SELECT ON metadata_versions TO tome_readonly;
GRANT SELECT ON tags TO tome_readonly;
GRANT SELECT ON manifestation_tags TO tome_readonly;
