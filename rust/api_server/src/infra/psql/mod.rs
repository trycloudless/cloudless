use sha2::{Digest, Sha256};

use crate::core::{CoreError, CoreResult};

pub mod pg_auth_repo;
pub mod pg_backup_config_repo;
pub mod pg_backup_job_repo;
pub mod pg_blog_repo;
pub mod pg_chunk_repo;
pub mod pg_dashboard_repo;
pub mod pg_email_template_repo;
pub mod pg_encrypted_dek_repo;
pub mod pg_gc_repo;
pub mod pg_local_device_repo;
pub mod pg_password_reset_token_repo;
pub mod pg_policy_repo;
pub mod pg_refresh_token_repo;
pub mod pg_remote_file_version;
pub mod pg_remote_storage_repo;
pub mod pg_restore_job_repo;
pub mod pg_security_event_repo;
pub mod pg_subscription_repo;
pub mod pg_user_repo;
pub mod pg_verification_code_repo;
pub mod user_role_type;

pub fn map_sqlx_error(error: sqlx::Error) -> CoreError {
    CoreError::database(error)
}

/// Maps a foreign-key violation on a `*_owned_by_user` composite FK to
/// CoreError::Forbidden: the caller supplied a foreign id that exists but
/// belongs to a different user — an authz failure, not an infra error.
pub fn map_ownership_violation(error: sqlx::Error) -> CoreError {
    if let sqlx::Error::Database(db_err) = &error {
        if db_err.constraint().is_some_and(|c| c.ends_with("_owned_by_user")) {
            return CoreError::forbidden(
                "one or more referenced resources do not belong to this account",
            );
        }
    }
    map_sqlx_error(error)
}

/// SHA-256 of the canonical JSON encoding of `value`, used to detect whether a
/// repeated idempotency key was sent with a semantically different request body.
/// serde_json serializes struct fields in declaration order deterministically,
/// so identical requests always produce identical bytes here.
pub fn fingerprint_request<T: serde::Serialize>(value: &T) -> CoreResult<Vec<u8>> {
    let bytes = serde_json::to_vec(value)
        .map_err(|e| CoreError::internal_with_source("Failed to serialize request for idempotency fingerprint", e))?;
    Ok(Sha256::digest(&bytes).to_vec())
}
