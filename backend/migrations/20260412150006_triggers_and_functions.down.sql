DROP TRIGGER IF EXISTS trg_works_search_vector ON works;
DROP FUNCTION IF EXISTS works_search_vector_update();

DROP TRIGGER IF EXISTS trg_manifestations_updated_at ON manifestations;
DROP TRIGGER IF EXISTS trg_works_updated_at ON works;
DROP TRIGGER IF EXISTS trg_users_updated_at ON users;
DROP FUNCTION IF EXISTS set_updated_at();
