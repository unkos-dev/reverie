-- Postgres cannot drop a single enum value; rebuild the type without
-- 'failed'. Rows holding 'failed' are remapped to 'pending' (validation
-- state unknown — truthful fallback). DROP DEFAULT before ALTER COLUMN TYPE
-- because the default expression must type-check against the column type.
UPDATE manifestations SET validation_status = 'pending' WHERE validation_status = 'failed';

ALTER TYPE public.validation_status RENAME TO validation_status_old;
CREATE TYPE public.validation_status AS ENUM ('pending', 'clean', 'repaired', 'degraded');

ALTER TABLE manifestations ALTER COLUMN validation_status DROP DEFAULT;
ALTER TABLE manifestations
    ALTER COLUMN validation_status
    TYPE public.validation_status
    USING validation_status::text::public.validation_status;
ALTER TABLE manifestations ALTER COLUMN validation_status SET DEFAULT 'pending';

DROP TYPE public.validation_status_old;
