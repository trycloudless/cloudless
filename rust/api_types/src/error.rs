use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", content = "data")]
pub enum ApiError {
    NotFound { resource: String },
    Unauthorized,
    Forbidden,
    Validation { message: String },
    Conflict { message: String },
    Internal, // generic, no details leaked
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ApiError::NotFound { resource } => {
                write!(f, "The requested {} was not found.", resource)
            }
            ApiError::Unauthorized => {
                write!(f, "Your session has expired. Please sign in again.")
            }
            ApiError::Forbidden => {
                write!(f, "You don't have permission to perform this action.")
            }
            ApiError::Validation { message } => write!(f, "{}", message),
            ApiError::Conflict { message } => write!(f, "{}", message),
            ApiError::Internal => write!(f, "Something went wrong. Please try again."),
        }
    }
}
