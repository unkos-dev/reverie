ALTER TABLE public.manifestations
    DROP CONSTRAINT manifestations_destination_owner_claim_fk,
    DROP CONSTRAINT manifestations_source_owner_claim_fk,
    DROP CONSTRAINT manifestations_recorded_owner_claim_fk,
    DROP CONSTRAINT manifestations_recorded_path_claim_fk;

DROP TABLE public.library_path_claims;

ALTER TABLE public.manifestations DROP CONSTRAINT manifestations_id_library_key;
