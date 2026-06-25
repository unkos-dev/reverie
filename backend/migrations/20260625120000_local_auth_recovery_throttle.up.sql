-- Local-auth recovery + throttle + first-admin gate (S2 of the auth/authz arc).
--
-- Three tables backing the local-account-default capability that S2 builds on
-- top of S1's identity schema: email-less PIN password recovery, login
-- throttling that a separate CLI process can clear, and the database-enforced
-- single-first-admin bootstrap gate. No Rust query code ships in this commit
-- (tables only): the query path and the regenerated .sqlx cache land after this
-- migration is applied, to avoid the same-commit migration+query deadlock.
-- See adr/2026-06-23-auth-identity-pluggable-providers.md.

-- Email-less password recovery. The clear PIN is written to an
-- operator-readable host file (proof of host access); only an Argon2id HASH is
-- persisted here, alongside expiry and a consumed marker. A row is single-use
-- (consumed_at set) and short-lived (expires_at); at most one row stays active
-- per user (the request path supersedes prior active rows). Holds a secret
-- hash, so it mirrors local_credentials: app full access only, NO
-- reverie_readonly grant, no RLS.
CREATE TABLE public.password_reset_pins (
    id uuid DEFAULT uuidv7() NOT NULL,
    user_id uuid NOT NULL,
    pin_hash text NOT NULL,
    expires_at timestamptz NOT NULL,
    consumed_at timestamptz,
    created_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT password_reset_pins_pkey PRIMARY KEY (id),
    CONSTRAINT password_reset_pins_user_id_fkey
        FOREIGN KEY (user_id) REFERENCES public.users (id) ON DELETE CASCADE,
    CONSTRAINT password_reset_pins_created_at_ts_decode_range
        CHECK (created_at >= '0001-01-01 00:00:00+00' AND created_at < '10000-01-01 00:00:00+00'),
    CONSTRAINT password_reset_pins_expires_at_ts_decode_range
        CHECK (expires_at >= '0001-01-01 00:00:00+00' AND expires_at < '10000-01-01 00:00:00+00'),
    CONSTRAINT password_reset_pins_consumed_at_ts_decode_range
        CHECK (consumed_at >= '0001-01-01 00:00:00+00' AND consumed_at < '10000-01-01 00:00:00+00')
);

-- Active-PIN lookup and FK-cascade enumeration are keyed on user_id.
CREATE INDEX idx_password_reset_pins_user_id ON public.password_reset_pins USING btree (user_id);

-- Per-account login throttle (DB-backed so an out-of-band CLI unlock can clear
-- it; an in-memory map cannot be cleared cross-process). Keyed on the
-- normalized (lower-cased) email rather than user_id so the throttle exists
-- independent of whether the email resolves to an account, keeping the failed-
-- login path account-existence-uniform. Per-source (per-IP) rate limiting does
-- the hard blocking; this escalating backoff is the IP-independent backstop and
-- is reset on a successful login. Holds auth state: app full access only, NO
-- reverie_readonly grant.
CREATE TABLE public.local_login_throttle (
    email_lower text NOT NULL,
    fail_count integer NOT NULL DEFAULT 0,
    locked_until timestamptz,
    updated_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT local_login_throttle_pkey PRIMARY KEY (email_lower),
    CONSTRAINT local_login_throttle_locked_until_ts_decode_range
        CHECK (locked_until >= '0001-01-01 00:00:00+00' AND locked_until < '10000-01-01 00:00:00+00'),
    CONSTRAINT local_login_throttle_updated_at_ts_decode_range
        CHECK (updated_at >= '0001-01-01 00:00:00+00' AND updated_at < '10000-01-01 00:00:00+00')
);

-- Database-enforced first-administrator gate (invariant 1). A boolean PK
-- pinned to true by the singleton CHECK admits at most one row: the first-admin
-- transaction inserts the single true row in the SAME transaction as the admin
-- user, so a second concurrent bootstrap collides on the primary key and fails.
-- This serializes the zero->one admin transition across all three writers (HTTP
-- setup, CLI bootstrap, startup env-seed) without each cooperating on an
-- app-layer lock, closing the READ COMMITTED TOCTOU (CWE-367) that a
-- SELECT-EXISTS-then-INSERT leaves open. It is a one-shot transition guard, NOT
-- a permanent uniqueness rule: multiple admins are allowed later (S3). Holds
-- security-gate state: app only, NO reverie_readonly grant.
CREATE TABLE public.instance_bootstrap (
    id boolean NOT NULL DEFAULT true,
    bootstrapped_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT instance_bootstrap_pkey PRIMARY KEY (id),
    CONSTRAINT instance_bootstrap_singleton CHECK (id),
    CONSTRAINT instance_bootstrap_bootstrapped_at_ts_decode_range
        CHECK (bootstrapped_at >= '0001-01-01 00:00:00+00' AND bootstrapped_at < '10000-01-01 00:00:00+00')
);

-- Maintain updated_at on row mutation (the throttle upsert updates in place),
-- mirroring the trigger every other mutable table carries.
CREATE TRIGGER trg_local_login_throttle_updated_at
    BEFORE UPDATE ON public.local_login_throttle
    FOR EACH ROW EXECUTE FUNCTION public.set_updated_at();

-- Grants: all three hold a secret hash, auth state, or a security gate, so each
-- mirrors local_credentials/device_tokens (app role only, no readonly).
-- password_reset_pins and local_login_throttle mutate (consume / upsert /
-- clear); instance_bootstrap is insert-then-read only (the marker row is
-- immutable once written), so it takes the tighter SELECT,INSERT grant.
GRANT SELECT,INSERT,UPDATE,DELETE ON TABLE public.password_reset_pins TO reverie_app;
GRANT SELECT,INSERT,UPDATE,DELETE ON TABLE public.local_login_throttle TO reverie_app;
GRANT SELECT,INSERT ON TABLE public.instance_bootstrap TO reverie_app;
