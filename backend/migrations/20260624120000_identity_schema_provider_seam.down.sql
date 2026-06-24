-- Reverse of the identity-schema/provider-seam migration, in inverse order.
--
-- Local-dev reversibility only, not a production rollback: re-adding NOT NULL
-- and UNIQUE on users.oidc_subject fails if any credential-only (NULL subject)
-- or duplicate-subject row exists. That is expected; this down path targets a
-- clean-ish dev DB, and these migrations roll up into the base schema before
-- the first release.

ALTER TABLE public.device_tokens DROP COLUMN scopes;

ALTER TABLE public.users
    ADD CONSTRAINT users_oidc_subject_key UNIQUE (oidc_subject);
ALTER TABLE public.users ALTER COLUMN oidc_subject SET NOT NULL;

DROP TABLE public.local_credentials;
DROP TABLE public.user_identities;
DROP TYPE public.identity_provider;
