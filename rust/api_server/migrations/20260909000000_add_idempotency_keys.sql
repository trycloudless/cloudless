CREATE TABLE file_version_idempotency_keys (
    idempotency_key     UUID        PRIMARY KEY,
    user_id             UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    request_fingerprint BYTEA       NOT NULL,
    response_snapshot   JSONB       NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_file_version_idempotency_keys_user_id ON file_version_idempotency_keys(user_id);

CREATE TABLE chunk_idempotency_keys (
    idempotency_key     UUID        PRIMARY KEY,
    user_id             UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    request_fingerprint BYTEA       NOT NULL,
    response_snapshot   JSONB       NOT NULL,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_chunk_idempotency_keys_user_id ON chunk_idempotency_keys(user_id);
