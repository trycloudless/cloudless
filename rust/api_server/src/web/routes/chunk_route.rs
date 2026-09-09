use crate::{
    app_env::AppEnv,
    core::chunks::application,
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::{
    chunk::{CreateChunkRequest, CreateChunkResponse, UpdateChunkStorageMetaRequest},
    restore_file_info::{GetFileVersionChunksRequest, GetFileVersionChunksResponse},
};
use axum::{Extension, Json, Router, extract::State, routing::post};

pub fn create_router() -> Router<AppEnv> {
    Router::new()
        .route("/create", post(create_chunk))
        .route("/version-chunks", post(get_version_chunks))
        .route("/update-storage-meta", post(update_storage_meta))
}

async fn create_chunk(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<CreateChunkRequest>,
) -> WebAppResult<Json<CreateChunkResponse>> {
    let chunk = application::create_chunk(env, ctx.user_id, payload).await?;
    Ok(Json(chunk))
}

/// Returns the ordered list of chunks for a given file version.
/// Used by the client during restore to know which chunks to download.
async fn get_version_chunks(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<GetFileVersionChunksRequest>,
) -> WebAppResult<Json<GetFileVersionChunksResponse>> {
    let response = application::get_chunks_for_version(env, ctx.user_id, payload).await?;
    Ok(Json(response))
}

async fn update_storage_meta(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(payload): Json<UpdateChunkStorageMetaRequest>,
) -> WebAppResult<()> {
    application::update_chunk_storage_meta(env, ctx.user_id, payload).await?;
    Ok(())
}
