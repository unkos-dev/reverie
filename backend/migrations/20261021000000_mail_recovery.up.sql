ALTER TABLE users
ADD COLUMN email_verified_at timestamptz,
ADD CONSTRAINT users_email_verified_at_ts_decode_range CHECK (
    email_verified_at >= '0001-01-01 00:00:00+00'::timestamptz
    AND email_verified_at < '10000-01-01 00:00:00+00'::timestamptz
);

CREATE TABLE account_email_proofs (
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    purpose text NOT NULL,
    email text NOT NULL,
    token_hash bytea NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    expires_at timestamptz NOT NULL,
    CONSTRAINT account_email_proofs_pkey PRIMARY KEY (user_id, purpose),
    CONSTRAINT account_email_proofs_token_hash_key UNIQUE (token_hash),
    CONSTRAINT account_email_proofs_purpose CHECK (purpose IN ('verification', 'recovery')),
    CONSTRAINT account_email_proofs_token_hash_length CHECK (octet_length(token_hash) = 32),
    CONSTRAINT account_email_proofs_email_length CHECK (char_length(email) BETWEEN 1 AND 320),
    CONSTRAINT account_email_proofs_expiry_order CHECK (expires_at > created_at),
    CONSTRAINT account_email_proofs_created_at_ts_decode_range CHECK (
        created_at >= '0001-01-01 00:00:00+00'::timestamptz
        AND created_at < '10000-01-01 00:00:00+00'::timestamptz
    ),
    CONSTRAINT account_email_proofs_expires_at_ts_decode_range CHECK (
        expires_at >= '0001-01-01 00:00:00+00'::timestamptz
        AND expires_at < '10000-01-01 00:00:00+00'::timestamptz
    )
);

CREATE TABLE mail_jobs (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    purpose text NOT NULL,
    proof_digest bytea,
    authority_kind text,
    enqueued_at timestamptz NOT NULL,
    next_attempt_at timestamptz NOT NULL,
    attempt_count integer NOT NULL DEFAULT 0,
    expires_at timestamptz NOT NULL,
    state text NOT NULL DEFAULT 'pending',
    terminal_at timestamptz,
    failure_category text,
    nonce bytea,
    payload bytea,
    CONSTRAINT mail_jobs_purpose CHECK (purpose IN (
        'admin_link', 'verification', 'email_recovery',
        'notice_password_changed', 'notice_password_set', 'notice_address_verified',
        'notice_address_changed', 'notice_address_removed'
    )),
    CONSTRAINT mail_jobs_state CHECK (
        state IN ('pending', 'in_progress', 'accepted', 'failed', 'cancelled', 'expired')
    ),
    CONSTRAINT mail_jobs_authority_shape CHECK (
        CASE purpose
            WHEN 'admin_link' THEN authority_kind = 'credential_link' AND proof_digest IS NOT NULL
            WHEN 'verification' THEN authority_kind = 'verification' AND proof_digest IS NOT NULL
            WHEN 'email_recovery' THEN authority_kind = 'recovery' AND proof_digest IS NOT NULL
            ELSE authority_kind IS NULL AND proof_digest IS NULL
        END
    ),
    CONSTRAINT mail_jobs_proof_digest_length CHECK (
        proof_digest IS NULL OR octet_length(proof_digest) = 32
    ),
    CONSTRAINT mail_jobs_attempt_count_range CHECK (attempt_count BETWEEN 0 AND 5),
    CONSTRAINT mail_jobs_expiry_order CHECK (expires_at > enqueued_at),
    CONSTRAINT mail_jobs_terminal_shape CHECK (
        (state IN ('accepted', 'failed', 'cancelled', 'expired')) = (terminal_at IS NOT NULL)
    ),
    CONSTRAINT mail_jobs_failure_category CHECK (
        failure_category IS NULL OR failure_category IN (
            'connection', 'tls', 'authentication', 'temporary_refusal',
            'permanent_refusal', 'timeout', 'payload_unreadable'
        )
    ),
    CONSTRAINT mail_jobs_nonce_length CHECK (nonce IS NULL OR octet_length(nonce) = 12),
    CONSTRAINT mail_jobs_sealed_together CHECK ((nonce IS NULL) = (payload IS NULL)),
    CONSTRAINT mail_jobs_payload_length CHECK (payload IS NULL OR octet_length(payload) BETWEEN 16 AND 65536),
    CONSTRAINT mail_jobs_live_payload CHECK (
        state NOT IN ('pending', 'in_progress') OR payload IS NOT NULL
    ),
    CONSTRAINT mail_jobs_enqueued_at_ts_decode_range CHECK (
        enqueued_at >= '0001-01-01 00:00:00+00'::timestamptz
        AND enqueued_at < '10000-01-01 00:00:00+00'::timestamptz
    ),
    CONSTRAINT mail_jobs_next_attempt_at_ts_decode_range CHECK (
        next_attempt_at >= '0001-01-01 00:00:00+00'::timestamptz
        AND next_attempt_at < '10000-01-01 00:00:00+00'::timestamptz
    ),
    CONSTRAINT mail_jobs_expires_at_ts_decode_range CHECK (
        expires_at >= '0001-01-01 00:00:00+00'::timestamptz
        AND expires_at < '10000-01-01 00:00:00+00'::timestamptz
    ),
    CONSTRAINT mail_jobs_terminal_at_ts_decode_range CHECK (
        terminal_at >= '0001-01-01 00:00:00+00'::timestamptz
        AND terminal_at < '10000-01-01 00:00:00+00'::timestamptz
    )
);

CREATE INDEX idx_mail_jobs_due ON mail_jobs (next_attempt_at) WHERE state = 'pending';
CREATE INDEX idx_mail_jobs_account_purpose ON mail_jobs (user_id, purpose, enqueued_at);
CREATE INDEX idx_mail_jobs_proof_digest ON mail_jobs (proof_digest) WHERE proof_digest IS NOT NULL;
CREATE UNIQUE INDEX idx_mail_jobs_one_live_delivery_per_proof
ON mail_jobs (proof_digest)
WHERE proof_digest IS NOT NULL AND state IN ('pending', 'in_progress');
CREATE INDEX idx_mail_jobs_terminal_at ON mail_jobs (terminal_at) WHERE terminal_at IS NOT NULL;
CREATE INDEX idx_mail_jobs_payload_expiry ON mail_jobs (expires_at) WHERE payload IS NOT NULL;
CREATE INDEX idx_account_email_proofs_expires_at ON account_email_proofs (expires_at);

GRANT SELECT, INSERT, UPDATE, DELETE ON account_email_proofs TO reverie_app;
GRANT SELECT, INSERT, UPDATE, DELETE ON mail_jobs TO reverie_app;
