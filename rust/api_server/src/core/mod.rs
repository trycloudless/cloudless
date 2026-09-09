use std::error::Error;
use std::time::SystemTimeError;

use rand::rand_core;

pub mod auth;
pub mod backup_config;
pub mod backup_job;
pub mod blog;
pub mod chunks;
pub mod dashboard;
pub mod email_template;
pub mod email_verification;
pub mod encrypted_dek;
pub mod gc;
pub mod local_device;
pub mod media;
pub mod model;
pub mod password_reset;
pub mod policy;
pub mod ports;
pub mod remote_file_version;
pub mod remote_storage;
pub mod restore_job;
pub mod security_event;
pub mod subscription;
pub mod user;

/// Core error type with business-specific variants.
///
/// Each variant carries a user-readable `message` and an optional `source`
/// error for structured logging. The `message` is safe to return in HTTP
/// responses (for 4xx) or to display generically (for 5xx). The `source`
/// captures the original error for debugging and is never exposed to clients.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("Not found: {message}")]
    NotFound {
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

    #[error("Authentication error: {message}")]
    Authentication {
        message: String,
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    #[error("Forbidden: {message}")]
    Forbidden {
        message: String,
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    #[error("Conflict: {message}")]
    Conflict {
        message: String,
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    #[error("Database error: {message}")]
    Database {
        message: String,
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    #[error("External service error: {message}")]
    ExternalService {
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
}

impl CoreError {
    pub fn not_found(message: impl Into<String>) -> Self {
        CoreError::NotFound {
            message: message.into(),
            source: None,
        }
    }

    pub fn validation(message: impl Into<String>) -> Self {
        CoreError::Validation {
            message: message.into(),
            source: None,
        }
    }

    pub fn authentication(message: impl Into<String>) -> Self {
        CoreError::Authentication {
            message: message.into(),
            source: None,
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        CoreError::Forbidden {
            message: message.into(),
            source: None,
        }
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        CoreError::Conflict {
            message: message.into(),
            source: None,
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        CoreError::Internal {
            message: message.into(),
            source: None,
        }
    }

    pub fn internal_with_source(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        CoreError::Internal {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }

    pub fn database(source: impl Error + Send + Sync + 'static) -> Self {
        CoreError::Database {
            message: "Database operation failed".into(),
            source: Some(Box::new(source)),
        }
    }

    pub fn external_service(
        message: impl Into<String>,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        CoreError::ExternalService {
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

impl From<SystemTimeError> for CoreError {
    fn from(error: SystemTimeError) -> Self {
        CoreError::internal_with_source("System time error", error)
    }
}

impl From<jsonwebtoken::errors::Error> for CoreError {
    fn from(error: jsonwebtoken::errors::Error) -> Self {
        CoreError::internal_with_source("JWT error", error)
    }
}

impl From<argon2::password_hash::Error> for CoreError {
    fn from(error: argon2::password_hash::Error) -> Self {
        CoreError::Internal {
            message: format!("Password hash error: {}", error),
            source: None,
        }
    }
}

impl From<rand_core::OsError> for CoreError {
    fn from(error: rand_core::OsError) -> Self {
        CoreError::internal_with_source("Random number generation failed", error)
    }
}

pub type CoreResult<T> = Result<T, CoreError>;
