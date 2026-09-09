-- Complete Cloudless schema.
-- Single migration combining all incremental migrations into one clean baseline.
-- Intended for fresh database deployments (local, staging, production).

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ── Custom types ───────────────────────────────────────────────────────────

CREATE TYPE user_role AS ENUM ('USER', 'SUPER_ADMIN');

CREATE TYPE subscription_tier AS ENUM ('free', 'starter', 'pro', 'lifetime');

CREATE TYPE subscription_status AS ENUM (
    'active',
    'on_hold',   -- Dodo payment retry in progress; user keeps paid-tier access
    'past_due',
    'canceled',
    'expired'
);

CREATE TYPE tier_change_reason AS ENUM (
    'initial',
    'upgrade',
    'downgrade',
    'renewal',
    'cancellation',
    'expiration',
    'reactivation',
    'lifetime_purchase',
    'admin_override'
);

-- ── Users & Auth ───────────────────────────────────────────────────────────

CREATE TABLE users (
    id                  UUID        PRIMARY KEY,
    email               TEXT        UNIQUE NOT NULL,
    name                TEXT        NOT NULL,
    role                user_role   NOT NULL DEFAULT 'USER',
    password_hash       TEXT        NOT NULL,
    email_verified      BOOLEAN     NOT NULL DEFAULT FALSE,
    bin_retention_days  INTEGER     NOT NULL DEFAULT 30,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE refresh_tokens (
    id          UUID        PRIMARY KEY,
    user_id     UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  TEXT        NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL,
    revoked     BOOLEAN     NOT NULL DEFAULT false
);

CREATE INDEX idx_refresh_tokens_user_id ON refresh_tokens(user_id);

CREATE TABLE password_reset_tokens (
    id          UUID        PRIMARY KEY,
    user_id     UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  TEXT        NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL,
    used_at     TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_password_reset_tokens_hash ON password_reset_tokens(token_hash);

CREATE TABLE encrypted_deks (
    id            UUID    PRIMARY KEY,
    user_id       UUID    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    key_type      TEXT    NOT NULL CHECK (key_type IN ('password', 'recovery')),
    encrypted_key BYTEA   NOT NULL,
    nonce         BYTEA   NOT NULL,
    salt          TEXT    NOT NULL,
    algorithm     TEXT    NOT NULL DEFAULT 'Aes256Gcm',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, key_type)
);

CREATE TABLE email_verification_codes (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash   TEXT        NOT NULL,
    expires_at  TIMESTAMPTZ NOT NULL,
    used_at     TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_email_verification_codes_user ON email_verification_codes(user_id);

-- ── Security audit ─────────────────────────────────────────────────────────

CREATE TABLE security_events (
    id                  UUID        PRIMARY KEY,
    user_id             UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    event_type          TEXT        NOT NULL CHECK (event_type IN (
                            'export_recovery_key',
                            'change_recovery_key',
                            'recover_with_recovery_key',
                            'change_password'
                        )),
    physical_device_id  TEXT        NOT NULL,
    ip_address          TEXT,
    user_agent          TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_security_events_user_id ON security_events(user_id);

-- ── Devices & Storage ──────────────────────────────────────────────────────

CREATE TABLE local_devices (
    id                 UUID        PRIMARY KEY,
    physical_device_id TEXT        NOT NULL,
    user_id            UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    display_name       TEXT,
    platform           TEXT        NOT NULL,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (physical_device_id, user_id)
);

CREATE TABLE remote_storages (
    id           UUID        PRIMARY KEY,
    user_id      UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         TEXT        NOT NULL,
    storage_type TEXT        NOT NULL,
    config       JSONB       NOT NULL,
    status       TEXT        NOT NULL DEFAULT 'Active',
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- ── Backup config ──────────────────────────────────────────────────────────

CREATE TABLE backup_config (
    id                     UUID    PRIMARY KEY,
    user_id                UUID    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    storage_id             UUID    NOT NULL REFERENCES remote_storages(id) ON DELETE CASCADE,
    local_device_id        UUID    NOT NULL REFERENCES local_devices(id) ON DELETE CASCADE,
    encrypted_source_dir   BYTEA   NOT NULL,
    source_dir_nonce       BYTEA   NOT NULL,
    source_dir_blind_index BYTEA   NOT NULL,
    display_name           TEXT    NOT NULL,
    is_active              BOOLEAN NOT NULL DEFAULT true,
    cleanup_type           JSONB   NOT NULL DEFAULT '"NoCleanup"',
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (storage_id, user_id, source_dir_blind_index, local_device_id)
);

-- ── Remote file versions & chunks ─────────────────────────────────────────

CREATE TABLE remote_file_versions (
    id                    UUID        PRIMARY KEY,
    user_id               UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id             UUID        NOT NULL REFERENCES local_devices(id) ON DELETE CASCADE,
    storage_id            UUID        NOT NULL REFERENCES remote_storages(id),
    encrypted_name        BYTEA       NOT NULL,
    name_nonce            BYTEA       NOT NULL,
    name_blind_index      BYTEA       NOT NULL,
    version               INTEGER     NOT NULL CHECK (version >= 1),
    size                  BIGINT      NOT NULL CHECK (size >= 0),
    status                JSONB       NOT NULL,
    status_history        JSONB       NOT NULL,
    local_file_updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at            TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, device_id, storage_id, name_blind_index, version)
);

CREATE INDEX idx_rfv_user_storage_blind_index ON remote_file_versions(user_id, storage_id, name_blind_index);
CREATE INDEX idx_rfv_user_device ON remote_file_versions(user_id, device_id);

CREATE TABLE chunks (
    id             UUID    PRIMARY KEY,
    hash           BYTEA   NOT NULL,
    size           INTEGER NOT NULL,
    user_id        UUID    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    storage_id     UUID    NOT NULL REFERENCES remote_storages(id) ON DELETE CASCADE,
    storage_meta   JSONB   NOT NULL,
    status_history JSONB   NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    CONSTRAINT unique_chunk_per_storage UNIQUE (hash, user_id, storage_id)
);

CREATE TABLE remote_file_version_chunks (
    remote_file_version_id UUID    NOT NULL REFERENCES remote_file_versions(id) ON DELETE CASCADE,
    chunk_id               UUID    NOT NULL REFERENCES chunks(id) ON DELETE CASCADE,
    chunk_index            INTEGER NOT NULL,
    PRIMARY KEY (remote_file_version_id, chunk_index)
);

-- ── Backup jobs ────────────────────────────────────────────────────────────

CREATE TABLE backup_jobs (
    id                      UUID        PRIMARY KEY,
    user_id                 UUID        NOT NULL REFERENCES users(id),
    backup_config_id        UUID        NOT NULL REFERENCES backup_config(id),
    status                  TEXT        NOT NULL CHECK (status IN ('running', 'completed', 'completed_with_errors', 'failed')),
    total_files             INTEGER     NOT NULL DEFAULT 0,
    total_files_succeeded   INTEGER     NOT NULL DEFAULT 0,
    total_files_failed      INTEGER     NOT NULL DEFAULT 0,
    original_bytes          BIGINT      NOT NULL DEFAULT 0,
    uploaded_bytes          BIGINT      NOT NULL DEFAULT 0,
    deduplicated_bytes      BIGINT      NOT NULL DEFAULT 0,
    deduplicated_chunks     INTEGER     NOT NULL DEFAULT 0,
    cleanup_files_deleted   INTEGER     NOT NULL DEFAULT 0,
    cleanup_bytes_freed     BIGINT      NOT NULL DEFAULT 0,
    error_message           TEXT,
    started_at              TIMESTAMPTZ NOT NULL,
    completed_at            TIMESTAMPTZ
);

CREATE TABLE backup_job_files (
    id                   UUID    PRIMARY KEY,
    job_id               UUID    NOT NULL REFERENCES backup_jobs(id),
    encrypted_name       BYTEA   NOT NULL,
    name_nonce           BYTEA   NOT NULL,
    blind_index          BYTEA   NOT NULL,
    original_size        BIGINT  NOT NULL,
    uploaded_size        BIGINT  NOT NULL DEFAULT 0,
    deduplicated_size    BIGINT  NOT NULL DEFAULT 0,
    total_chunks         INTEGER NOT NULL DEFAULT 0,
    deduplicated_chunks  INTEGER NOT NULL DEFAULT 0,
    status               TEXT    NOT NULL CHECK (status IN ('uploading', 'completed', 'failed')),
    error_message        TEXT
);

CREATE TABLE backup_job_cleanup_files (
    id             UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id         UUID        NOT NULL REFERENCES backup_jobs(id) ON DELETE CASCADE,
    encrypted_name BYTEA       NOT NULL,
    name_nonce     BYTEA       NOT NULL,
    blind_index    BYTEA       NOT NULL,
    size           BIGINT      NOT NULL,
    deleted_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_backup_jobs_config ON backup_jobs(backup_config_id);
CREATE INDEX idx_backup_job_files_job ON backup_job_files(job_id);
CREATE INDEX idx_bjcf_job ON backup_job_cleanup_files(job_id);

-- ── Restore jobs ───────────────────────────────────────────────────────────

CREATE TABLE restore_jobs (
    id                    UUID        PRIMARY KEY,
    user_id               UUID        NOT NULL REFERENCES users(id),
    backup_config_id      UUID        NOT NULL REFERENCES backup_config(id),
    overwrite_behavior    TEXT        NOT NULL CHECK (overwrite_behavior IN ('overwrite', 'keep_both', 'skip_if_exists')),
    status                TEXT        NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    total_files           INTEGER     NOT NULL DEFAULT 0,
    total_files_succeeded INTEGER     NOT NULL DEFAULT 0,
    total_files_failed    INTEGER     NOT NULL DEFAULT 0,
    original_bytes        BIGINT      NOT NULL DEFAULT 0,
    restored_bytes        BIGINT      NOT NULL DEFAULT 0,
    error_message         TEXT,
    started_at            TIMESTAMPTZ NOT NULL,
    completed_at          TIMESTAMPTZ
);

CREATE TABLE restore_job_files (
    id                     UUID    PRIMARY KEY,
    job_id                 UUID    NOT NULL REFERENCES restore_jobs(id),
    remote_file_version_id UUID    NOT NULL REFERENCES remote_file_versions(id),
    encrypted_name         BYTEA   NOT NULL,
    name_nonce             BYTEA   NOT NULL,
    blind_index            BYTEA   NOT NULL,
    original_size          BIGINT  NOT NULL,
    restored_size          BIGINT  NOT NULL DEFAULT 0,
    total_chunks           INTEGER NOT NULL DEFAULT 0,
    chunks_completed       INTEGER NOT NULL DEFAULT 0,
    status                 TEXT    NOT NULL CHECK (status IN ('pending', 'restoring', 'completed', 'failed')),
    error_message          TEXT
);

CREATE INDEX idx_restore_jobs_config ON restore_jobs(backup_config_id);
CREATE INDEX idx_restore_jobs_user ON restore_jobs(user_id);
CREATE INDEX idx_restore_job_files_job ON restore_job_files(job_id);

-- ── GC ────────────────────────────────────────────────────────────────────

CREATE TABLE gc_runs (
    id                  UUID        PRIMARY KEY,
    user_id             UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    status              TEXT        NOT NULL CHECK (status IN ('running', 'completed', 'failed')),
    versions_deleted    INTEGER     NOT NULL DEFAULT 0,
    chunks_deleted      INTEGER     NOT NULL DEFAULT 0,
    storage_freed_bytes BIGINT      NOT NULL DEFAULT 0,
    error_message       TEXT,
    started_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    completed_at        TIMESTAMPTZ
);

CREATE TABLE gc_run_versions (
    id              UUID    PRIMARY KEY,
    gc_run_id       UUID    NOT NULL REFERENCES gc_runs(id) ON DELETE CASCADE,
    file_version_id UUID    NOT NULL,
    encrypted_name  BYTEA   NOT NULL,
    name_nonce      BYTEA   NOT NULL,
    version         INTEGER NOT NULL,
    size            BIGINT  NOT NULL
);

CREATE TABLE gc_run_chunks (
    id                   UUID    PRIMARY KEY,
    gc_run_id            UUID    NOT NULL REFERENCES gc_runs(id) ON DELETE CASCADE,
    chunk_id             UUID    NOT NULL,
    storage_id           UUID    NOT NULL,
    size                 INTEGER NOT NULL,
    deleted_from_storage BOOLEAN NOT NULL DEFAULT false
);

CREATE INDEX idx_gc_runs_user ON gc_runs(user_id);
CREATE INDEX idx_gc_run_versions_run ON gc_run_versions(gc_run_id);
CREATE INDEX idx_gc_run_chunks_run ON gc_run_chunks(gc_run_id);

-- ── Blog & Email ───────────────────────────────────────────────────────────

CREATE TABLE blog_posts (
    id         UUID        PRIMARY KEY,
    user_id    UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    slug       TEXT        UNIQUE NOT NULL,
    title      TEXT        NOT NULL,
    content    TEXT        NOT NULL,
    summary    TEXT        NOT NULL DEFAULT '',
    published  BOOLEAN     NOT NULL DEFAULT false,
    is_page    BOOLEAN     NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_blog_posts_slug ON blog_posts(slug);
CREATE INDEX idx_blog_posts_published ON blog_posts(published) WHERE published = true;

CREATE TABLE email_templates (
    id            UUID        PRIMARY KEY,
    name          TEXT        NOT NULL,
    subject       TEXT        NOT NULL,
    body_html     TEXT        NOT NULL,
    template_type TEXT        NOT NULL CHECK (template_type IN ('password_reset', 'promotions', 'service')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_email_templates_type ON email_templates(template_type);

-- ── Policies ───────────────────────────────────────────────────────────────

CREATE TABLE policies (
    id          UUID        PRIMARY KEY,
    policy_type TEXT        UNIQUE NOT NULL CHECK (policy_type IN (
                    'privacy_policy',
                    'terms_of_service',
                    'cookie_policy',
                    'acceptable_use',
                    'refund_and_cancellation',
                    'security_and_data_handling',
                    'subprocessors',
                    'data_retention_and_deletion'
                )),
    title       TEXT        NOT NULL,
    description TEXT        NOT NULL DEFAULT '',
    is_required BOOLEAN     NOT NULL DEFAULT true,
    max_skips   INTEGER     NOT NULL DEFAULT 0,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE policy_versions (
    id           UUID        PRIMARY KEY,
    policy_id    UUID        NOT NULL REFERENCES policies(id) ON DELETE CASCADE,
    version      INTEGER     NOT NULL CHECK (version >= 1),
    content      TEXT        NOT NULL,
    status       TEXT        NOT NULL CHECK (status IN ('draft', 'published')),
    published_at TIMESTAMPTZ,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (policy_id, version)
);

CREATE INDEX idx_policy_versions_policy ON policy_versions(policy_id);
CREATE INDEX idx_policy_versions_published ON policy_versions(status) WHERE status = 'published';

CREATE TABLE user_policy_acceptances (
    id                  UUID        PRIMARY KEY,
    user_id             UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    policy_version_id   UUID        NOT NULL REFERENCES policy_versions(id) ON DELETE CASCADE,
    ip_address          TEXT,
    user_agent          TEXT,
    physical_device_id  TEXT,
    accepted_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (user_id, policy_version_id)
);

CREATE INDEX idx_user_policy_acceptances_user ON user_policy_acceptances(user_id);

CREATE TABLE user_policy_skips (
    id                  UUID        PRIMARY KEY,
    user_id             UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    policy_version_id   UUID        NOT NULL REFERENCES policy_versions(id) ON DELETE CASCADE,
    ip_address          TEXT,
    user_agent          TEXT,
    physical_device_id  TEXT,
    skipped_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_user_policy_skips_user ON user_policy_skips(user_id);
CREATE INDEX idx_user_policy_skips_version ON user_policy_skips(user_id, policy_version_id);

-- ── Subscriptions ──────────────────────────────────────────────────────────

CREATE TABLE subscriptions (
    id                       UUID              PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id                  UUID              NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tier                     subscription_tier NOT NULL DEFAULT 'free',
    status                   subscription_status NOT NULL DEFAULT 'active',
    -- Provider identity
    provider                 VARCHAR(50)       NOT NULL DEFAULT 'dodo',
    provider_subscription_id VARCHAR(255),
    provider_customer_id     VARCHAR(255),
    -- Provider metadata
    provider_status          VARCHAR(50),
    provider_product_id      VARCHAR(255),
    billing_currency         VARCHAR(10),
    next_billing_date        TIMESTAMPTZ,
    previous_billing_date    TIMESTAMPTZ,
    expires_at               TIMESTAMPTZ,
    trial_period_days        INTEGER,
    last_synced_at           TIMESTAMPTZ,
    last_webhook_at          TIMESTAMPTZ,
    -- Billing period
    current_period_start     TIMESTAMPTZ,
    current_period_end       TIMESTAMPTZ,
    cancel_at_period_end     BOOLEAN           NOT NULL DEFAULT FALSE,
    created_at               TIMESTAMPTZ       NOT NULL DEFAULT NOW(),
    updated_at               TIMESTAMPTZ       NOT NULL DEFAULT NOW(),
    UNIQUE (user_id)
);

CREATE INDEX idx_subscriptions_user_id ON subscriptions(user_id);
CREATE INDEX idx_subscriptions_provider_subscription_id ON subscriptions(provider_subscription_id);

CREATE TABLE subscription_events (
    id                  UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    subscription_id     UUID        NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    webhook_id          VARCHAR(255),
    event_type          VARCHAR(100) NOT NULL,
    provider            VARCHAR(50),
    raw_headers         JSONB,
    payload             JSONB,
    verification_status VARCHAR(20),
    processing_status   VARCHAR(20)  NOT NULL DEFAULT 'done',
    processing_error    TEXT,
    processed_at        TIMESTAMPTZ,
    external_event_id   VARCHAR(255),           -- kept for reference in history linkage
    created_at          TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    CONSTRAINT subscription_events_webhook_id_unique UNIQUE (webhook_id)
);

CREATE INDEX idx_subscription_events_subscription_id ON subscription_events(subscription_id);

CREATE TABLE subscription_history (
    id              UUID                PRIMARY KEY DEFAULT gen_random_uuid(),
    subscription_id UUID                NOT NULL REFERENCES subscriptions(id) ON DELETE CASCADE,
    user_id         UUID                NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    previous_tier   subscription_tier,
    new_tier        subscription_tier   NOT NULL,
    previous_status subscription_status,
    new_status      subscription_status NOT NULL,
    reason          tier_change_reason  NOT NULL,
    external_event_id VARCHAR(255),
    metadata        JSONB,
    changed_at      TIMESTAMPTZ         NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_subscription_history_user ON subscription_history(user_id);
CREATE INDEX idx_subscription_history_sub ON subscription_history(subscription_id);
CREATE INDEX idx_subscription_history_changed ON subscription_history(changed_at);

CREATE TABLE tier_limits (
    tier                    subscription_tier PRIMARY KEY,
    max_devices             INTEGER,
    max_backup_configs      INTEGER,
    auto_backup_enabled     BOOLEAN     NOT NULL DEFAULT FALSE,
    metadata_retention_days INTEGER,
    updated_at              TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO tier_limits (tier, max_devices, max_backup_configs, auto_backup_enabled, metadata_retention_days) VALUES
    ('free',     1,    1,    FALSE, 7),
    ('starter',  3,    3,    TRUE,  NULL),
    ('pro',      NULL, NULL, TRUE,  NULL),
    ('lifetime', NULL, NULL, TRUE,  NULL);

CREATE TABLE payment_checkout_sessions (
    id                           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id                      UUID        NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tier                         subscription_tier NOT NULL,
    provider                     VARCHAR(50) NOT NULL DEFAULT 'dodo',
    provider_checkout_session_id VARCHAR(255),
    provider_product_id          VARCHAR(255),
    status                       VARCHAR(50) NOT NULL DEFAULT 'pending',
    checkout_url                 TEXT,
    return_url                   TEXT,
    cancel_url                   TEXT,
    metadata                     JSONB,
    created_at                   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    completed_at                 TIMESTAMPTZ,
    expired_at                   TIMESTAMPTZ
);

CREATE INDEX idx_checkout_sessions_user_id ON payment_checkout_sessions(user_id);
CREATE INDEX idx_checkout_sessions_provider_checkout_session_id ON payment_checkout_sessions(provider_checkout_session_id);
