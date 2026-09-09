use api_types::error::ApiError;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use std::error::Error;

use crate::core::CoreError;

pub type WebAppResult<T> = Result<T, CoreError>;

/// Formats the full error chain: "message -> cause1 -> cause2 -> ..."
fn format_error_chain(err: &dyn Error) -> String {
    let mut chain = err.to_string();
    let mut current = err.source();
    while let Some(cause) = current {
        chain.push_str(" -> ");
        chain.push_str(&cause.to_string());
        current = cause.source();
    }
    chain
}

impl IntoResponse for CoreError {
    fn into_response(self) -> Response {
        let error_chain = format_error_chain(&self);

        let (status, api_error) = match &self {
            CoreError::NotFound { message, .. } => {
                tracing::warn!(error = %error_chain, "Not found");
                (
                    StatusCode::NOT_FOUND,
                    ApiError::NotFound {
                        resource: message.clone(),
                    },
                )
            }
            CoreError::Validation { message, .. } => {
                tracing::warn!(error = %error_chain, "Validation error");
                (
                    StatusCode::BAD_REQUEST,
                    ApiError::Validation {
                        message: message.clone(),
                    },
                )
            }
            CoreError::Authentication { .. } => {
                tracing::warn!(error = %error_chain, "Authentication error");
                (StatusCode::UNAUTHORIZED, ApiError::Unauthorized)
            }
            CoreError::Forbidden { .. } => {
                tracing::warn!(error = %error_chain, "Forbidden");
                (StatusCode::FORBIDDEN, ApiError::Forbidden)
            }
            CoreError::Conflict { message, .. } => {
                tracing::warn!(error = %error_chain, "Conflict");
                (
                    StatusCode::CONFLICT,
                    ApiError::Conflict {
                        message: message.clone(),
                    },
                )
            }
            CoreError::Database { .. } => {
                tracing::error!(error = %error_chain, "Database error");
                (StatusCode::INTERNAL_SERVER_ERROR, ApiError::Internal)
            }
            CoreError::ExternalService { .. } => {
                tracing::error!(error = %error_chain, "External service error");
                (StatusCode::BAD_GATEWAY, ApiError::Internal)
            }
            CoreError::Internal { .. } => {
                tracing::error!(error = %error_chain, "Internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, ApiError::Internal)
            }
        };

        (status, Json(api_error)).into_response()
    }
}
