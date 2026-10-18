CREATE TABLE public.sign_in_cohorts (
    session_id text NOT NULL,
    won_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL, -- noqa: CV11
    CONSTRAINT sign_in_cohorts_pkey PRIMARY KEY (session_id),
    CONSTRAINT sign_in_cohorts_session_id_fkey FOREIGN KEY (session_id)
        REFERENCES tower_sessions.session(id) ON DELETE CASCADE,
    CONSTRAINT sign_in_cohorts_won_at_ts_decode_range
    CHECK ((
        (won_at >= '0001-01-01 00:00:00+00'::timestamp with time zone)
        AND (won_at < '10000-01-01 00:00:00+00'::timestamp with time zone)
    )),
    CONSTRAINT sign_in_cohorts_created_at_ts_decode_range
    CHECK ((
        (created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone)
        AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone)
    ))
);

CREATE TABLE public.oidc_sign_in_attempts (
    session_id text NOT NULL,
    state_digest bytea NOT NULL,
    pkce_verifier text NOT NULL,
    nonce text NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL, -- noqa: CV11
    CONSTRAINT oidc_sign_in_attempts_pkey PRIMARY KEY (session_id),
    CONSTRAINT oidc_sign_in_attempts_session_id_fkey FOREIGN KEY (session_id)
        REFERENCES public.sign_in_cohorts(session_id) ON DELETE CASCADE,
    CONSTRAINT oidc_sign_in_attempts_state_digest_length CHECK ((octet_length(state_digest) = 32)), -- noqa: CV11
    CONSTRAINT oidc_sign_in_attempts_expires_at_ts_decode_range
    CHECK ((
        (expires_at >= '0001-01-01 00:00:00+00'::timestamp with time zone)
        AND (expires_at < '10000-01-01 00:00:00+00'::timestamp with time zone)
    )),
    CONSTRAINT oidc_sign_in_attempts_created_at_ts_decode_range
    CHECK ((
        (created_at >= '0001-01-01 00:00:00+00'::timestamp with time zone)
        AND (created_at < '10000-01-01 00:00:00+00'::timestamp with time zone)
    ))
);

CREATE INDEX idx_oidc_sign_in_attempts_expires_at ON public.oidc_sign_in_attempts USING btree (expires_at);

GRANT SELECT, INSERT, DELETE, UPDATE ON TABLE public.sign_in_cohorts TO reverie_app;
GRANT SELECT, INSERT, DELETE, UPDATE ON TABLE public.oidc_sign_in_attempts TO reverie_app;
