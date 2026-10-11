ALTER TABLE public.manifestations
    ADD COLUMN enrichment_retry_not_before timestamp with time zone,
    ADD CONSTRAINT manifestations_enrichment_retry_not_before_ts_decode_range
    CHECK ((
        (enrichment_retry_not_before >= '0001-01-01 00:00:00+00'::timestamp with time zone)
        AND (enrichment_retry_not_before < '10000-01-01 00:00:00+00'::timestamp with time zone)
    ));
