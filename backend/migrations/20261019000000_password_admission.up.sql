TRUNCATE public.local_login_throttle;
ALTER TABLE public.local_login_throttle
DROP COLUMN fail_count,
ADD COLUMN completed_failures timestamp with time zone[] NOT NULL DEFAULT '{}',
ADD CONSTRAINT local_login_throttle_failure_count CHECK (cardinality(completed_failures) <= 10),
ADD CONSTRAINT local_login_throttle_failure_shape CHECK (
    cardinality(completed_failures) = 0 OR (
        array_ndims(completed_failures) = 1 AND array_lower(completed_failures, 1) = 1
        AND array_position(completed_failures, NULL) IS NULL
    )
),
ADD CONSTRAINT local_login_throttle_failure_ts_decode_range CHECK (
    tstzrange('0001-01-01 00:00:00+00', '10000-01-01 00:00:00+00', '[)') @> all(completed_failures)
);
