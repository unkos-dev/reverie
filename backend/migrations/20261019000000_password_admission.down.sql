TRUNCATE public.local_login_throttle;
ALTER TABLE public.local_login_throttle
DROP COLUMN completed_failures,
ADD COLUMN fail_count integer NOT NULL DEFAULT 0;
