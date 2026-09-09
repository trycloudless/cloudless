use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use api_types::media::UploadMediaResponse;
use axum::{Json, extract::Multipart, http::StatusCode, response::IntoResponse};
use axum_extra::extract::cookie::CookieJar;

/// Maximum upload size: 10 MiB.
const MAX_UPLOAD_BYTES: usize = 10 * 1024 * 1024;

/// Allowed image MIME types.
const ALLOWED_CONTENT_TYPES: &[&str] = &[
    "image/jpeg",
    "image/png",
    "image/gif",
    "image/webp",
    "image/svg+xml",
];

/// Sanitises a user-supplied filename to prevent path traversal and other
/// injection attacks. Returns only the file stem and extension, replacing
/// any non-alphanumeric/hyphen/underscore/dot characters with underscores.
fn sanitize_filename(name: &str) -> String {
    let basename = name.rsplit('/').next().unwrap_or(name);
    let basename = basename.rsplit('\\').next().unwrap_or(basename);
    basename
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Accepts a multipart file upload from the browser, forwards it to the API
/// server's media upload endpoint, and returns JSON with the media URL.
pub async fn upload_image_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    mut multipart: Multipart,
) -> Result<Json<UploadMediaResponse>, impl IntoResponse> {
    let auth = extract_auth(&jar);
    if !verify_auth(&env, &auth).await.is_admin() {
        return Err((StatusCode::FORBIDDEN, "Unauthorized").into_response());
    }

    while let Ok(Some(field)) = multipart.next_field().await {
        let field_name = field.name().unwrap_or("").to_string();
        if field_name != "file" {
            continue;
        }

        let original_name = sanitize_filename(field.file_name().unwrap_or("upload"));
        let content_type = field
            .content_type()
            .unwrap_or("application/octet-stream")
            .to_string();

        // Validate content type against allowlist
        if !ALLOWED_CONTENT_TYPES.contains(&content_type.as_str()) {
            return Err((
                StatusCode::BAD_REQUEST,
                format!(
                    "File type '{}' is not allowed. Accepted types: JPEG, PNG, GIF, WebP, SVG.",
                    content_type
                ),
            )
                .into_response());
        }

        let data = match field.bytes().await {
            Ok(d) => d,
            Err(e) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("Failed to read file: {}", e),
                )
                    .into_response());
            }
        };

        // Enforce size limit
        if data.len() > MAX_UPLOAD_BYTES {
            return Err((
                StatusCode::PAYLOAD_TOO_LARGE,
                format!(
                    "File too large ({:.1} MiB). Maximum allowed size is {} MiB.",
                    data.len() as f64 / (1024.0 * 1024.0),
                    MAX_UPLOAD_BYTES / (1024 * 1024)
                ),
            )
                .into_response());
        }

        let url = env.http_api().get_url("api/media/upload").map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("URL error: {}", e),
            )
                .into_response()
        })?;

        let part = reqwest::multipart::Part::bytes(data.to_vec())
            .file_name(original_name)
            .mime_str(&content_type)
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Multipart error: {}", e),
                )
                    .into_response()
            })?;

        let form = reqwest::multipart::Form::new().part("file", part);
        let request = env.http_api().client.post(url).multipart(form);

        let resp: UploadMediaResponse =
            env.http_api().send_with_auth(request).await.map_err(|e| {
                tracing::error!("Media upload failed: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Upload failed: {}", e),
                )
                    .into_response()
            })?;

        return Ok(Json(resp));
    }

    Err((StatusCode::BAD_REQUEST, "No file field found in upload").into_response())
}

/// Proxies media file requests to the API server.
pub async fn serve_media_handler(
    RequestEnv(env): RequestEnv,
    axum::extract::Path(filename): axum::extract::Path<String>,
) -> Result<axum::response::Response, impl IntoResponse> {
    if filename.contains('/') || filename.contains("..") {
        return Err((StatusCode::BAD_REQUEST, "Invalid filename").into_response());
    }

    let url = env
        .http_api()
        .get_url(&format!("media/{}", filename))
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("URL error: {}", e),
            )
                .into_response()
        })?;

    let resp = env.http_api().client.get(url).send().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Failed to fetch media: {}", e),
        )
            .into_response()
    })?;

    if !resp.status().is_success() {
        return Err((StatusCode::NOT_FOUND, "Media not found").into_response());
    }

    let content_type = resp
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("application/octet-stream")
        .to_string();

    let cache_control = resp
        .headers()
        .get(axum::http::header::CACHE_CONTROL)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("public, max-age=31536000, immutable")
        .to_string();

    let bytes = resp.bytes().await.map_err(|e| {
        (
            StatusCode::BAD_GATEWAY,
            format!("Failed to read media: {}", e),
        )
            .into_response()
    })?;

    let mut response = axum::response::Response::new(axum::body::Body::from(bytes));
    response.headers_mut().insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_str(&content_type).unwrap_or(
            axum::http::HeaderValue::from_static("application/octet-stream"),
        ),
    );
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_str(&cache_control).unwrap_or(
            axum::http::HeaderValue::from_static("public, max-age=31536000, immutable"),
        ),
    );

    Ok(response)
}
