use crate::{
    app_env::AppEnv,
    core::backup_job::application,
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::backup_job::{
    AbandonStaleJobsResponse, CompleteBackupJobFileRequest, CompleteBackupJobRequest,
    CreateBackupJobFileRequest, CreateBackupJobFileResponse, CreateBackupJobRequest,
    CreateBackupJobResponse, GetBackupJobDetailRequest, GetBackupJobDetailResponse,
    GetLatestBackupJobResponse, GetResumableBackupJobRequest, GetResumableBackupJobResponse,
    ListBackupJobsRequest, ListBackupJobsResponse, LogCleanupFilesRequest,
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
        .route("/cleanup/log", post(log_cleanup_files))
        .route("/list", post(list_jobs))
        .route("/detail", post(get_job_detail))
        .route("/latest", get(get_latest_job))
        .route("/resumable", post(get_resumable_job))
        .route("/abandon_stale", post(abandon_stale_jobs))
}

async fn create_job(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<CreateBackupJobRequest>,
) -> WebAppResult<Json<CreateBackupJobResponse>> {
    let response = application::create_job(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn create_job_file(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<CreateBackupJobFileRequest>,
) -> WebAppResult<Json<CreateBackupJobFileResponse>> {
    let response = application::create_job_file(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn complete_job_file(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<CompleteBackupJobFileRequest>,
) -> WebAppResult<Json<()>> {
    application::complete_job_file(env, ctx.user_id, payload).await?;
    Ok(Json(()))
}

async fn complete_job(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<CompleteBackupJobRequest>,
) -> WebAppResult<Json<()>> {
    application::complete_job(env, ctx.user_id, payload).await?;
    Ok(Json(()))
}

async fn list_jobs(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<ListBackupJobsRequest>,
) -> WebAppResult<Json<ListBackupJobsResponse>> {
    let response = application::list_jobs(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn get_job_detail(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<GetBackupJobDetailRequest>,
) -> WebAppResult<Json<GetBackupJobDetailResponse>> {
    let response = application::get_job_detail(env, ctx.user_id, payload.id).await?;
    Ok(Json(response))
}

async fn get_latest_job(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<GetLatestBackupJobResponse>> {
    let response = application::get_latest_job(env, ctx.user_id).await?;
    Ok(Json(response))
}

async fn get_resumable_job(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<GetResumableBackupJobRequest>,
) -> WebAppResult<Json<GetResumableBackupJobResponse>> {
    let response = application::get_resumable_job(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn log_cleanup_files(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<LogCleanupFilesRequest>,
) -> WebAppResult<Json<()>> {
    application::log_cleanup_files(env, ctx.user_id, payload).await?;
    Ok(Json(()))
}

async fn abandon_stale_jobs(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
) -> WebAppResult<Json<AbandonStaleJobsResponse>> {
    let response = application::abandon_stale_jobs(env, ctx.user_id).await?;
    Ok(Json(response))
}
