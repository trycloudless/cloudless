use crate::{
    app_env::AppEnv,
    core::policy::application,
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::policy::*;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    routing::{get, post, put},
};
use uuid::Uuid;

/// Public routes — no authentication required.
pub fn public_router() -> Router<AppEnv> {
    Router::new().route("/public/{slug}", get(get_public_policy_handler))
}

/// Routes that require authentication but not admin role.
pub fn authenticated_router() -> Router<AppEnv> {
    Router::new()
        .route("/pending", get(get_pending_policies_handler))
        .route("/checkout-policy", get(get_checkout_policy_handler))
        .route("/accept", post(accept_policy_handler))
        .route("/skip", post(skip_policy_handler))
        .route("/version/{id}", get(get_version_handler))
}

/// Routes that require admin role (behind jwt_auth middleware).
pub fn admin_router() -> Router<AppEnv> {
    Router::new()
        .route("/", get(list_policies_handler).post(create_policy_handler))
        .route("/{id}", put(update_policy_handler))
        .route("/{id}/versions", get(list_versions_handler))
        .route("/version", post(create_version_handler))
        .route("/version/{id}", put(update_version_handler))
        .route("/version/{id}/publish", post(publish_version_handler))
}

async fn list_policies_handler(
    State(env): State<AppEnv>,
) -> WebAppResult<Json<ListPoliciesResponse>> {
    let response = application::list_policies(&env).await?;
    Ok(Json(response))
}

async fn create_policy_handler(
    State(env): State<AppEnv>,
    Json(req): Json<CreatePolicyRequest>,
) -> WebAppResult<Json<CreatePolicyResponse>> {
    let response = application::create_policy(&env, req).await?;
    Ok(Json(response))
}

async fn update_policy_handler(
    State(env): State<AppEnv>,
    Path(id): Path<Uuid>,
    Json(mut req): Json<UpdatePolicyRequest>,
) -> WebAppResult<Json<PolicyResponse>> {
    req.id = id;
    let response = application::update_policy(&env, req).await?;
    Ok(Json(response))
}

async fn list_versions_handler(
    State(env): State<AppEnv>,
    Path(id): Path<Uuid>,
) -> WebAppResult<Json<ListPolicyVersionsResponse>> {
    let response = application::list_versions(&env, id).await?;
    Ok(Json(response))
}

async fn create_version_handler(
    State(env): State<AppEnv>,
    Json(req): Json<CreatePolicyVersionRequest>,
) -> WebAppResult<Json<CreatePolicyVersionResponse>> {
    let response = application::create_version(&env, req).await?;
    Ok(Json(response))
}

async fn update_version_handler(
    State(env): State<AppEnv>,
    Path(id): Path<Uuid>,
    Json(mut req): Json<UpdatePolicyVersionRequest>,
) -> WebAppResult<Json<PolicyVersionResponse>> {
    req.id = id;
    let response = application::update_version(&env, req).await?;
    Ok(Json(response))
}

async fn publish_version_handler(
    State(env): State<AppEnv>,
    Path(id): Path<Uuid>,
) -> WebAppResult<Json<PolicyVersionResponse>> {
    let req = PublishPolicyVersionRequest { id };
    let response = application::publish_version(&env, req).await?;
    Ok(Json(response))
}

async fn get_version_handler(
    State(env): State<AppEnv>,
    Path(id): Path<Uuid>,
) -> WebAppResult<Json<PolicyVersionResponse>> {
    let response = application::get_version(&env, id).await?;
    Ok(Json(response))
}

async fn get_pending_policies_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<GetPendingPoliciesResponse>> {
    let response = application::get_pending_policies(&env, ctx.user_id).await?;
    Ok(Json(response))
}

async fn get_checkout_policy_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<GetCheckoutPolicyResponse>> {
    let response = application::get_checkout_policy(&env, ctx.user_id).await?;
    Ok(Json(response))
}

async fn accept_policy_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<AcceptPolicyRequest>,
) -> WebAppResult<Json<AcceptPolicyResponse>> {
    let response = application::accept_policy(&env, ctx.user_id, req).await?;
    Ok(Json(response))
}

async fn skip_policy_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<SkipPolicyRequest>,
) -> WebAppResult<Json<SkipPolicyResponse>> {
    let response = application::skip_policy(&env, ctx.user_id, req).await?;
    Ok(Json(response))
}

async fn get_public_policy_handler(
    State(env): State<AppEnv>,
    Path(slug): Path<String>,
) -> WebAppResult<Json<PublicPolicyResponse>> {
    let snake = slug.replace('-', "_");
    let policy_type = snake
        .parse::<PolicyType>()
        .map_err(|_| crate::core::CoreError::not_found("Unknown policy type"))?;
    let response = application::get_public_policy(&env, policy_type).await?;
    match response {
        Some(p) => Ok(Json(p)),
        None => Err(crate::core::CoreError::not_found(
            "No published version found for this policy",
        )),
    }
}
