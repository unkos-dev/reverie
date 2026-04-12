-- Generic updated_at trigger function.
-- Apply to any table with an updated_at column.
CREATE OR REPLACE FUNCTION set_updated_at() RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_users_updated_at
    BEFORE UPDATE ON users
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_works_updated_at
    BEFORE UPDATE ON works
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

CREATE TRIGGER trg_manifestations_updated_at
    BEFORE UPDATE ON manifestations
    FOR EACH ROW
    EXECUTE FUNCTION set_updated_at();

-- NOTE: reading_positions also has an updated_at column but is a reserved
-- table with no logic yet. Add the trigger when the table gets active use
-- (Phase 2: reading sync adapters).

-- Auto-update search_vector on works
CREATE OR REPLACE FUNCTION works_search_vector_update() RETURNS TRIGGER AS $$
BEGIN
    NEW.search_vector := to_tsvector('english', COALESCE(NEW.title, '') || ' ' || COALESCE(NEW.description, ''));
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_works_search_vector
    BEFORE INSERT OR UPDATE OF title, description ON works
    FOR EACH ROW
    EXECUTE FUNCTION works_search_vector_update();
