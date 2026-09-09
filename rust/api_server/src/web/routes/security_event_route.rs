use crate::{
    app_env::AppEnv,
    core::security_event::application,
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::security_event::{
    ListSecurityEventsResponse, StoreSecurityEventRequest, StoreSecurityEventResponse,
};
use axum::{
    Extension, Json, Router,
    extract::State,
    routing::{get, post},
};

pub fn create_router() -> Router<AppEnv> {
    Router::new()
        .route("/store", post(store_security_event))
        .route("/list", get(list_security_events))
}

async fn store_security_event(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<StoreSecurityEventRequest>,
) -> WebAppResult<Json<StoreSecurityEventResponse>> {
    let response = application::store(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn list_security_events(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<ListSecurityEventsResponse>> {
    let response = application::list(env, ctx.user_id).await?;
    Ok(Json(response))
}
