use api_types::error::ApiError;
use cloudless_core::model::app_error::AppError;
use cloudless_core::ports::api::ApiClientError;
use serde::Serialize;
use std::fmt;

#[derive(Debug, Serialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum TauriError {
    NotFound { message: String },
    Unauthorized { message: String },
    Validation { message: String },
    Internal { message: String },
    Conflict { message: String },
}

impl fmt::Display for TauriError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::NotFound { message }
            | Self::Unauthorized { message }
            | Self::Validation { message }
            | Self::Internal { message }
            | Self::Conflict { message } => message,
        };
        write!(f, "{}", msg)
    }
}

impl From<AppError> for TauriError {
    fn from(e: AppError) -> Self {
        tracing::error!(error = ?e, "Command failed");
        match e {
            AppError::NotFound { message, .. } => Self::NotFound { message },
            AppError::PermissionDenied { message, .. } => Self::Unauthorized { message },
            AppError::Validation { message, .. } => Self::Validation { message },
            AppError::Conflict { message, .. } => Self::Conflict { message },
            AppError::AuthTokenExpired { storage_id } => Self::Unauthorized {
                message: format!("Auth token expired for storage {}", storage_id),
            },
            AppError::Network { message, .. } | AppError::Internal { message, .. } => {
                Self::Internal { message }
            }
        }
    }
}

impl From<ApiClientError> for TauriError {
    fn from(e: ApiClientError) -> Self {
        tracing::error!(error = ?e, "API client error");
        match e {
            ApiClientError::Api(api_err) => match &api_err {
                ApiError::NotFound { .. } => Self::NotFound {
                    message: api_err.to_string(),
                },
                ApiError::Unauthorized | ApiError::Forbidden => Self::Unauthorized {
                    message: api_err.to_string(),
                },
                ApiError::Validation { .. } => Self::Validation {
                    message: api_err.to_string(),
                },
                ApiError::Conflict { .. } => Self::Conflict {
                    message: api_err.to_string(),
                },
                _ => Self::Internal {
                    message: api_err.to_string(),
                },
            },
            other => Self::Internal {
                message: other.to_string(),
            },
        }
    }
}

impl From<serde_json::Error> for TauriError {
    fn from(e: serde_json::Error) -> Self {
        tracing::error!(error = %e, "Serialization failed");
        Self::Internal {
            message: "Failed to process data. Please try again.".to_string(),
        }
    }
}

impl From<argon2::password_hash::Error> for TauriError {
    fn from(e: argon2::password_hash::Error) -> Self {
        tracing::error!(error = %e, "Password hash failed");
        Self::Internal {
            message: "Encryption operation failed. Please try again.".to_string(),
        }
    }
}

impl From<std::io::Error> for TauriError {
    fn from(e: std::io::Error) -> Self {
        tracing::error!(error = %e, "IO operation failed");
        Self::Internal {
            message: format!("File operation failed: {}", e),
        }
    }
}

impl From<String> for TauriError {
    fn from(s: String) -> Self {
        if is_expected_state_error(&s) {
            tracing::debug!(message = %s, "Command skipped (expected state)");
        } else {
            tracing::error!(message = %s, "Command failed");
        }
        Self::Internal { message: s }
    }
}

impl From<&str> for TauriError {
    fn from(s: &str) -> Self {
        if is_expected_state_error(s) {
            tracing::debug!(message = %s, "Command skipped (expected state)");
        } else {
            tracing::error!(message = %s, "Command failed");
        }
        Self::Internal {
            message: s.to_string(),
        }
    }
}

fn is_expected_state_error(msg: &str) -> bool {
    msg.contains("Encryption not unlocked")
        || msg.contains("Derived keys not available")
        || msg.contains("Device not registered")
}

pub type TauriResult<T> = Result<T, TauriError>;
