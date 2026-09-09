use std::collections::HashMap;

use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
};

use crate::{
    app_env::AppEnv,
    core::subscription::payment_application,
};

/// Public routes — no JWT auth middleware.
pub fn create_router() -> Router<AppEnv> {
    Router::new().route("/dodo", post(dodo_webhook_handler))
}

/// Receive a Dodo webhook event.
///
/// Uses raw `Bytes` to preserve the exact body for HMAC verification.
/// Converts headers to a plain `HashMap<String, String>` before passing to
/// the application layer — the verifier in `DodoClient` extracts only the
/// Standard Webhooks headers it needs.
/// Responds 200 quickly; heavy processing runs in a background task.
async fn dodo_webhook_handler(
    State(env): State<AppEnv>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, crate::core::CoreError> {
    // Convert axum HeaderMap to a plain HashMap for the provider-agnostic verifier.
    let headers: HashMap<String, String> = headers
        .iter()
        .filter_map(|(name, value)| {
            value
                .to_str()
                .ok()
                .map(|v| (name.as_str().to_lowercase(), v.to_string()))
        })
        .collect();

    payment_application::handle_webhook(&env, headers, &body).await?;

    Ok(StatusCode::OK)
}
