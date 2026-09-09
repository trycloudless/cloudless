use std::error::Error;

use aes_gcm::aead::rand_core;
use sha2::digest::crypto_common;

use uuid::Uuid;

use crate::ports::api::ApiClientError;
use api_types::error::ApiError;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("Not found: {message}")]
    NotFound {
        message: String,
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    #[error("Permission denied: {message}")]
    PermissionDenied {
        message: String,
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    #[error("Network error: {message}")]
    Network {
        message: String,
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    #[error("Validation error: {message}")]
    Validation {
        message: String,
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    #[error("Internal error: {message}")]
    Internal {
        message: String,
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    /// Server rejected the request due to a stale `base_version` or a reused
    /// idempotency key with a different payload. Callers should treat this as a
    /// single-attempt failure — see `backup_file`/`backup_job` for why no
    /// in-process retry is needed.
    #[error("Conflict: {message}")]
    Conflict {
        message: String,
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    /// The OAuth refresh token for a remote storage has expired or been revoked.
    /// Callers should mark the storage as `AuthTokenExpired` via the API
    /// and surface a reauth prompt to the user.
    #[error("Auth token expired for storage {storage_id}")]
    AuthTokenExpired { storage_id: Uuid },
}

impl From<std::io::Error> for AppError {
    fn from(error: std::io::Error) -> Self {
        AppError::Internal {
            message: error.to_string(),
            source: Some(Box::new(error)),
        }
    }
}

impl From<rand_core::Error> for AppError {
    fn from(error: rand_core::Error) -> Self {
        AppError::Internal {
            message: "Random number generation failed".to_string(),
            source: Some(Box::new(error)),
        }
    }
}

impl From<aes_gcm::Error> for AppError {
    fn from(error: aes_gcm::Error) -> Self {
        AppError::Internal {
            message: "Encryption failed".to_string(),
            source: Some(Box::new(error)),
        }
    }
}

impl From<argon2::Error> for AppError {
    fn from(error: argon2::Error) -> Self {
        AppError::Internal {
            message: "Key derivation failed".to_string(),
            source: Some(Box::new(error)),
        }
    }
}

impl From<argon2::password_hash::Error> for AppError {
    fn from(error: argon2::password_hash::Error) -> Self {
        AppError::Internal {
            message: format!("Password hash error: {}", error),
            source: None,
        }
    }
}
impl From<crypto_common::InvalidLength> for AppError {
    fn from(error: crypto_common::InvalidLength) -> Self {
        AppError::Internal {
            message: "Invalid key length".to_string(),
            source: Some(Box::new(error)),
        }
    }
}

impl From<rand::rand_core::OsError> for AppError {
    fn from(error: rand::rand_core::OsError) -> Self {
        AppError::Internal {
            message: "Random number generation failed".to_string(),
            source: Some(Box::new(error)),
        }
    }
}

impl From<ApiClientError> for AppError {
    fn from(error: ApiClientError) -> Self {
        match error {
            ApiClientError::Transport(e) => AppError::Network {
                message: e.to_string(),
                source: Some(Box::new(e)),
            },
            ApiClientError::InvalidUrl(msg) => AppError::Internal {
                message: format!("Invalid URL: {}", msg),
                source: None,
            },
            ApiClientError::Api(api_error) => match &api_error {
                ApiError::NotFound { .. } => AppError::NotFound {
                    message: api_error.to_string(),
                    source: None,
                },
                ApiError::Unauthorized | ApiError::Forbidden => AppError::PermissionDenied {
                    message: api_error.to_string(),
                    source: None,
                },
                ApiError::Validation { .. } => AppError::Validation {
                    message: api_error.to_string(),
                    source: None,
                },
                ApiError::Conflict { .. } => AppError::Conflict {
                    message: api_error.to_string(),
                    source: None,
                },
                _ => AppError::Internal {
                    message: api_error.to_string(),
                    source: None,
                },
            },
        }
    }
}

impl From<serde_json::Error> for AppError {
    fn from(error: serde_json::Error) -> Self {
        AppError::Internal {
            message: format!("JSON serialization error: {}", error),
            source: Some(Box::new(error)),
        }
    }
}
