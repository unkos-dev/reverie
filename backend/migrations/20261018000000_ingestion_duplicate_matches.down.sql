-- noqa: disable=RF03
CREATE OR REPLACE VIEW public.ingestion_input_classes WITH (security_invoker = TRUE) AS
SELECT
    i.id,
    i.status,
    latest.outcome,
    c.reason_class
FROM public.ingestion_inputs AS i
LEFT JOIN LATERAL (
    SELECT j.outcome
    FROM public.ingestion_jobs AS j
    WHERE j.input_id = i.id AND j.input_generation = i.generation AND j.outcome IS NOT NULL
    ORDER BY j.created_at DESC, j.id DESC
    LIMIT 1
) AS latest ON TRUE
INNER JOIN LATERAL (
    SELECT
        CASE i.status
            WHEN 'rejected' THEN coalesce(i.rejection_reasons[1], 'unspecified') -- noqa: CV11
            WHEN 'not_accepted' THEN 'format_not_accepted'
            WHEN 'operational_failure' THEN
                CASE
                    WHEN latest.outcome = 'needs_change' THEN 'needs_change'
                    WHEN latest.outcome = 'transient_input' AND i.retries_exhausted_at IS NOT NULL
                        THEN 'retries_exhausted'
                END
        END AS reason_class
) AS c ON TRUE
WHERE c.reason_class IS NOT NULL;
-- noqa: enable=RF03

ALTER TABLE public.ingestion_inputs DROP COLUMN matched_manifestation_id;
