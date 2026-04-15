-- Remove accessibility_metadata column (safe regardless of row contents)
ALTER TABLE manifestations DROP COLUMN IF EXISTS accessibility_metadata;

-- NOTE: 'degraded' enum value CANNOT be removed from PostgreSQL without a full
-- type rebuild. To truly roll back, restore the DB from a backup taken before
-- the migration was applied, or rebuild the type:
--   ALTER TABLE manifestations ALTER COLUMN validation_status TYPE TEXT;
--   DROP TYPE validation_status;
--   CREATE TYPE validation_status AS ENUM ('pending', 'valid', 'invalid', 'repaired');
--   ALTER TABLE manifestations ALTER COLUMN validation_status
--     TYPE validation_status USING validation_status::validation_status;
-- Only do this if no rows have validation_status = 'degraded'.
