use crate::{
    app_env::AppEnv,
    core::{
        ports::UserRepo,
        subscription::{application, payment_application},
    },
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::subscription::{
    CheckoutStatusResponse, CreateCheckoutRequest, CreateCheckoutResponse, CreatePortalResponse,
    SubscriptionHistoryResponse, SubscriptionResponse,
};
use api_types::user::UserRole;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    routing::post,
};

/// Protected routes always mounted regardless of billing mode.
/// Includes subscription status, history, and checkout session polling
/// (the status poll is harmless when billing is disabled).
pub fn protected_router() -> Router<AppEnv> {
    Router::new()
        .route("/current", get(get_subscription_handler))
        .route("/history", get(get_history_handler))
        .route(
            "/checkout/{checkout_id}/status",
            get(checkout_status_handler),
        )
}

/// Billing routes mounted only when billing is enabled (`BILLING_MODE=external`).
/// Absent in self-hosted mode — callers receive 404 rather than a billing-disabled error.
pub fn payment_router() -> Router<AppEnv> {
    Router::new()
        .route("/checkout", post(checkout_handler))
        .route("/portal", post(portal_handler))
}

/// Admin routes (behind jwt_auth middleware — caller must hold SuperAdmin role).
pub fn admin_router() -> Router<AppEnv> {
    Router::new().route("/{user_id}/reconcile", post(reconcile_handler))
}

async fn reconcile_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Path(user_id): Path<uuid::Uuid>,
) -> WebAppResult<StatusCode> {
    let caller = env.user_repo.find_by_id(ctx.user_id).await?;
    if caller.role != UserRole::SuperAdmin {
        return Err(crate::core::CoreError::forbidden("Super admin access required").into());
    }
    payment_application::reconcile_subscription(&env, user_id).await?;
    Ok(StatusCode::OK)
}

/// Get the current user's subscription with computed limits.
async fn get_subscription_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<SubscriptionResponse>> {
    let resp = application::get_subscription(&env, ctx.user_id).await?;
    Ok(Json(resp))
}

/// Get the current user's full subscription change history.
async fn get_history_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<SubscriptionHistoryResponse>> {
    let resp = application::get_history(&env, ctx.user_id).await?;
    Ok(Json(resp))
}

/// Create a Dodo hosted checkout session for the requested tier.
///
/// The client opens the returned `checkout_url` in the system browser.
/// Entitlement changes happen only after webhook delivery.
async fn checkout_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<CreateCheckoutRequest>,
) -> WebAppResult<Json<CreateCheckoutResponse>> {
    let user = env.user_repo.find_by_id(ctx.user_id).await?;
    let resp =
        payment_application::create_checkout(&env, ctx.user_id, user.email, user.name, req.tier)
            .await?;
    Ok(Json(resp))
}

/// Create a Dodo customer portal session.
///
/// The client opens the returned `portal_url` in the system browser.
async fn portal_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<CreatePortalResponse>> {
    let resp = payment_application::create_portal(&env, ctx.user_id).await?;
    Ok(Json(resp))
}

/// Return the current status of a checkout session.
///
/// Used by the billing return page to show real-time feedback instead of a
/// static "processing" message. Scoped to the authenticated user's own sessions.
async fn checkout_status_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Path(checkout_id): Path<uuid::Uuid>,
) -> WebAppResult<Json<CheckoutStatusResponse>> {
    use crate::core::ports::SubscriptionRepo;
    let status = env
        .subscription_repo
        .get_checkout_session_status(checkout_id, ctx.user_id)
        .await?
        .unwrap_or_else(|| "pending".to_string());
    Ok(Json(CheckoutStatusResponse { status }))
}
