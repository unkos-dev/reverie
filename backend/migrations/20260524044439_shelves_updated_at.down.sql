DROP TRIGGER IF EXISTS shelves_set_updated_at ON shelves;
DROP FUNCTION IF EXISTS set_shelves_updated_at();
ALTER TABLE shelves DROP COLUMN IF EXISTS updated_at;
