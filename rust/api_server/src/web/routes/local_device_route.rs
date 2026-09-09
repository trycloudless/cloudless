use crate::{
    app_env::AppEnv,
    core::local_device::application,
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::local_device::{
    CreateLocalDeviceRequest, CreateLocalDeviceResponse, GetLocalDeviceByPhysicalIdRequest,
    GetLocalDeviceByPhysicalIdResponse, GetOrCreateLocalDeviceRequest,
    GetOrCreateLocalDeviceResponse, ListAllDevicesResponse, ListDevicesByPlatformRequest,
    ListDevicesByPlatformResponse,
};
use axum::{Extension, Json, Router, extract::State, routing::post};

pub fn create_router() -> Router<AppEnv> {
    Router::new()
        .route("/create", post(create_local_device))
        .route("/get", post(get_local_device))
        .route("/get-or-create", post(get_or_create_local_device))
        .route("/list-by-platform", post(list_devices_by_platform))
        .route("/list-all", post(list_all_devices))
}

async fn create_local_device(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<CreateLocalDeviceRequest>,
) -> WebAppResult<Json<CreateLocalDeviceResponse>> {
    let user = application::create_local_device(env, ctx.user_id, payload).await?;
    Ok(Json(user))
}

async fn get_local_device(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<GetLocalDeviceByPhysicalIdRequest>,
) -> WebAppResult<Json<GetLocalDeviceByPhysicalIdResponse>> {
    let device = application::get_by_physical_id(env, ctx.user_id, payload).await?;
    Ok(Json(device))
}

async fn get_or_create_local_device(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<GetOrCreateLocalDeviceRequest>,
) -> WebAppResult<Json<GetOrCreateLocalDeviceResponse>> {
    let device = application::get_or_create(env, ctx.user_id, payload).await?;
    Ok(Json(device))
}

async fn list_devices_by_platform(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<ListDevicesByPlatformRequest>,
) -> WebAppResult<Json<ListDevicesByPlatformResponse>> {
    let response = application::list_by_platform(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn list_all_devices(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<ListAllDevicesResponse>> {
    let response = application::list_all(env, ctx.user_id).await?;
    Ok(Json(response))
}
