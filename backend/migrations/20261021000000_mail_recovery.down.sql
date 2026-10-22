DROP TABLE mail_jobs;
DROP TABLE account_email_proofs;
ALTER TABLE users DROP COLUMN email_verified_at;

CREATE TABLE public.password_reset_pins (
    id uuid DEFAULT uuidv7() NOT NULL,
    user_id uuid NOT NULL,
    pin_hash text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    consumed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT password_reset_pins_consumed_at_ts_decode_range
    CHECK ((
        (consumed_at >= '0001-01-01 00:00:00+00'::timestamp with time zone)
        AND (consumed_at < '10000-01-01 00:00:00+00'::timestamp with time zone)
    )),
    CONSTRAINT password_reset_pins_created_at_ts_decode_range
    CHECK ((
        (created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone)
        AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone)
    )),
    CONSTRAINT password_reset_pins_expires_at_ts_decode_range
    CHECK ((
        (expires_at >= '0001-01-01 00:00:00+00'::timestamp with time zone)
        AND (expires_at < '10000-01-01 00:00:00+00'::timestamp with time zone)
    ))
);

ALTER TABLE ONLY public.password_reset_pins
ADD CONSTRAINT password_reset_pins_pkey PRIMARY KEY (id);

ALTER TABLE ONLY public.password_reset_pins
ADD CONSTRAINT password_reset_pins_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users (id) ON DELETE CASCADE;

CREATE UNIQUE INDEX idx_password_reset_pins_active_unique ON public.password_reset_pins USING btree (user_id)
WHERE (consumed_at IS NULL);

COMMENT ON INDEX public.idx_password_reset_pins_active_unique IS
'Enforces at most one active (unconsumed) recovery PIN per user. Load-bearing for the single-active invariant '
'under concurrency; see models::password_reset_pin::rotate for the retry path on SQLSTATE 23505.';

CREATE INDEX idx_password_reset_pins_user_id ON public.password_reset_pins USING btree (user_id);

GRANT SELECT, INSERT, DELETE, UPDATE ON TABLE public.password_reset_pins TO reverie_app;
