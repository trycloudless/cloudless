use axum::{
    http::StatusCode,
    response::{Html, IntoResponse, Redirect, Response},
};
use cloudless_core::ports::api::ApiClientError;

#[derive(Debug)]
pub enum WebsiteError {
    Api(String),
    NotFound(String),
    Unauthorized,
}

impl std::fmt::Display for WebsiteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WebsiteError::Api(msg) => write!(f, "API error: {}", msg),
            WebsiteError::NotFound(msg) => write!(f, "Not found: {}", msg),
            WebsiteError::Unauthorized => write!(f, "Unauthorized"),
        }
    }
}

impl From<ApiClientError> for WebsiteError {
    fn from(err: ApiClientError) -> Self {
        match &err {
            ApiClientError::Api(api_types::error::ApiError::Unauthorized) => {
                WebsiteError::Unauthorized
            }
            ApiClientError::Api(api_types::error::ApiError::NotFound { resource }) => {
                WebsiteError::NotFound(resource.clone())
            }
            _ => WebsiteError::Api(err.to_string()),
        }
    }
}

impl IntoResponse for WebsiteError {
    fn into_response(self) -> Response {
        match self {
            WebsiteError::Unauthorized => Redirect::to("/login").into_response(),
            WebsiteError::NotFound(_) => {
                let html = crate::pages::not_found_page::render_404();
                (StatusCode::NOT_FOUND, Html(html)).into_response()
            }
            _ => {
                tracing::error!("Website error: {}", self);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Html("<h1>Internal Server Error</h1>".to_string()),
                )
                    .into_response()
            }
        }
    }
}
