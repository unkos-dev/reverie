-- Identity schema + provider seam (S1 of the auth/authz consolidation arc).
--
-- Establishes one canonical `users` identity with pluggable provider links,
-- and retires OIDC first-user auto-promotion (the application-side change in
-- models/user.rs). External identity is keyed on (issuer, subject): an OIDC
-- `sub` is unique only within its issuer per OIDC Core, so the issuer
-- namespaces the subject and the schema is correct across multiple issuers and
-- IdP migration. See adr/2026-06-23-auth-identity-pluggable-providers.md.
--
-- No backfill: a SQL migration cannot read the configured issuer, so
-- `user_identities` starts empty and existing accounts re-provision on next
-- OIDC login. `users.oidc_subject` is kept as a vestigial nullable column (not
-- dropped) and is no longer a source of truth or a write target for new users.

-- Mechanism tag for an external identity link. Static today (federated OIDC
-- only); local password credentials live in `local_credentials`, not here.
CREATE TYPE public.identity_provider AS ENUM ('oidc');

-- External-provider identity links. One canonical user may hold several rows
-- (one per provider identity). UNIQUE (issuer, subject) is the spec-correct
-- identity key; the same `sub` under two issuers is two distinct identities.
-- `email_verified` carries the per-identity verification state seeded false
-- (fail-closed; the real OIDC claim capture is a later step).
CREATE TABLE public.user_identities (
    id uuid DEFAULT uuidv7() NOT NULL,
    user_id uuid NOT NULL,
    provider public.identity_provider NOT NULL,
    issuer text NOT NULL,
    subject text NOT NULL,
    email_verified boolean DEFAULT false NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT user_identities_pkey PRIMARY KEY (id),
    CONSTRAINT user_identities_user_id_fkey
        FOREIGN KEY (user_id) REFERENCES public.users (id) ON DELETE CASCADE,
    CONSTRAINT user_identities_issuer_subject_key UNIQUE (issuer, subject),
    CONSTRAINT user_identities_created_at_ts_decode_range
        CHECK (created_at >= '0001-01-01 00:00:00+00' AND created_at < '10000-01-01 00:00:00+00'),
    CONSTRAINT user_identities_updated_at_ts_decode_range
        CHECK (updated_at >= '0001-01-01 00:00:00+00' AND updated_at < '10000-01-01 00:00:00+00')
);

-- Resolution lookup is keyed on (issuer, subject) via the unique constraint;
-- this index supports the FK-cascade and per-user identity enumeration.
CREATE INDEX idx_user_identities_user_id ON public.user_identities USING btree (user_id);

-- Local password credentials. Seam for the local-login step; no password
-- logic ships now. One hash per user (PK is user_id). Holds a SECRET, so it
-- mirrors device_tokens: app full access only, NO reverie_readonly grant, and
-- no RLS (users/device_tokens carry none either).
CREATE TABLE public.local_credentials (
    user_id uuid NOT NULL,
    password_hash text NOT NULL,
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT local_credentials_pkey PRIMARY KEY (user_id),
    CONSTRAINT local_credentials_user_id_fkey
        FOREIGN KEY (user_id) REFERENCES public.users (id) ON DELETE CASCADE,
    CONSTRAINT local_credentials_created_at_ts_decode_range
        CHECK (created_at >= '0001-01-01 00:00:00+00' AND created_at < '10000-01-01 00:00:00+00'),
    CONSTRAINT local_credentials_updated_at_ts_decode_range
        CHECK (updated_at >= '0001-01-01 00:00:00+00' AND updated_at < '10000-01-01 00:00:00+00')
);

-- `oidc_subject` stops being the identity key. Make it nullable (a
-- credential-only or freshly-provisioned user has none) and drop the unique
-- constraint; the identity key now lives on user_identities (issuer, subject).
ALTER TABLE public.users ALTER COLUMN oidc_subject DROP NOT NULL;
ALTER TABLE public.users DROP CONSTRAINT users_oidc_subject_key;

-- Per-token authorization scopes. Column-only seam for a later step; no Rust
-- struct/query reads it yet. Added NOT NULL DEFAULT so existing rows pick up
-- '{read}' automatically with no separate backfill.
ALTER TABLE public.device_tokens ADD COLUMN scopes text[] NOT NULL DEFAULT '{read}';

-- Grants: user_identities mirrors users (identity metadata; readonly may
-- SELECT). local_credentials mirrors device_tokens (holds hashes; app only).
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.user_identities TO reverie_app;
GRANT SELECT ON TABLE public.user_identities TO reverie_readonly;
GRANT SELECT,INSERT,DELETE,UPDATE ON TABLE public.local_credentials TO reverie_app;
