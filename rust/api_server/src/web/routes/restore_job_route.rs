use crate::{
    app_env::AppEnv,
    core::restore_job::application,
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::restore_job::{
    CompleteRestoreJobFileRequest, CompleteRestoreJobRequest, CreateRestoreJobFileRequest,
    CreateRestoreJobFileResponse, CreateRestoreJobRequest, CreateRestoreJobResponse,
    GetLatestRestoreJobResponse, GetRestoreJobDetailRequest, GetRestoreJobDetailResponse,
    GetResumableRestoreJobResponse, ListRestoreJobsRequest, ListRestoreJobsResponse,
};
use axum::{
    Extension, Json, Router,
    extract::State,
    routing::{get, post},
};

pub fn create_router() -> Router<AppEnv> {
    Router::new()
        .route("/create", post(create_job))
        .route("/file/create", post(create_job_file))
        .route("/file/complete", post(complete_job_file))
        .route("/complete", post(complete_job))
        .route("/list", post(list_jobs))
        .route("/detail", post(get_job_detail))
        .route("/latest", get(get_latest_job))
        .route("/resumable", get(get_resumable_job))
}

async fn create_job(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<CreateRestoreJobRequest>,
) -> WebAppResult<Json<CreateRestoreJobResponse>> {
    let response = application::create_job(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn create_job_file(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<CreateRestoreJobFileRequest>,
) -> WebAppResult<Json<CreateRestoreJobFileResponse>> {
    let response = application::create_job_file(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn complete_job_file(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<CompleteRestoreJobFileRequest>,
) -> WebAppResult<Json<()>> {
    application::complete_job_file(env, ctx.user_id, payload).await?;
    Ok(Json(()))
}

async fn complete_job(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<CompleteRestoreJobRequest>,
) -> WebAppResult<Json<()>> {
    application::complete_job(env, ctx.user_id, payload).await?;
    Ok(Json(()))
}

async fn list_jobs(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<ListRestoreJobsRequest>,
) -> WebAppResult<Json<ListRestoreJobsResponse>> {
    let response = application::list_jobs(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn get_job_detail(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<GetRestoreJobDetailRequest>,
) -> WebAppResult<Json<GetRestoreJobDetailResponse>> {
    let response = application::get_job_detail(env, ctx.user_id, payload.id).await?;
    Ok(Json(response))
}

async fn get_latest_job(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<GetLatestRestoreJobResponse>> {
    let response = application::get_latest_job(env, ctx.user_id).await?;
    Ok(Json(response))
}

async fn get_resumable_job(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<GetResumableRestoreJobResponse>> {
    let response = application::get_resumable_job(env, ctx.user_id).await?;
    Ok(Json(response))
}
