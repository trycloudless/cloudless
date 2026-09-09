use crate::{
    app_env::AppEnv,
    core::{
        CoreError,
        media::{application, env::MediaEnv},
    },
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::media::UploadMediaResponse;
use axum::{
    Extension, Json, Router,
    body::Body,
    extract::{Multipart, Path, State},
    http::{HeaderValue, Response, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use cloudless_core::model::file::ObjectKey;
use cloudless_core::ports::storage::StoragePort;

/// Protected route: requires JWT auth.
pub fn protected_router() -> Router<AppEnv> {
    Router::new().route("/upload", post(upload_handler))
}

/// Public route: serves uploaded media files.
pub fn public_router() -> Router<AppEnv> {
    Router::new().route("/{filename}", get(serve_media_handler))
}

/// Accepts a multipart file upload, stores it in S3, and returns the URL.
async fn upload_handler(
    State(env): State<AppEnv>,
    Extension(_ctx): Extension<AuthContext>,
    mut multipart: Multipart,
) -> WebAppResult<Json<UploadMediaResponse>> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| CoreError::validation(format!("Invalid multipart data: {}", e)))?
    {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name != "file" {
            continue;
        }

        let original_name = field.file_name().unwrap_or("upload").to_string();

        let data = field
            .bytes()
            .await
            .map_err(|e| CoreError::validation(format!("Failed to read file: {}", e)))?
            .to_vec();

        let resp = application::upload_media(&env, &original_name, data).await?;
        return Ok(Json(resp));
    }

    Err(CoreError::validation("No file field found in upload"))
}

/// Fetches a media file from S3 and returns it with appropriate headers.
async fn serve_media_handler(
    State(env): State<AppEnv>,
    Path(filename): Path<String>,
) -> Result<Response<Body>, impl IntoResponse> {
    // Validate filename format to prevent path traversal
    if filename.contains('/') || filename.contains("..") {
        return Err((StatusCode::BAD_REQUEST, "Invalid filename"));
    }

    let key = ObjectKey::new(format!("media/{}", filename));

    let data = StoragePort::get(env.media_storage(), &key)
        .await
        .map_err(|_| (StatusCode::NOT_FOUND, "Media not found"))?;

    let content_type = mime_from_extension(&filename);

    let mut response = Response::new(Body::from(data));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or(HeaderValue::from_static("application/octet-stream")),
    );
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=31536000, immutable"),
    );

    Ok(response)
}

fn mime_from_extension(filename: &str) -> &'static str {
    match filename
        .rsplit('.')
        .next()
        .map(|e| e.to_lowercase())
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        _ => "application/octet-stream",
    }
}
