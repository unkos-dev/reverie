ALTER TABLE public.ingestion_inputs
    ADD COLUMN rejection_reasons text [] NOT NULL DEFAULT '{}' CHECK (
        rejection_reasons <@ ARRAY[
            'unsafe_contents', 'damaged', 'invalid_structure', 'over_limits', 'unspecified'
        ]::text []
    );
