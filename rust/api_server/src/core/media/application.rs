use crate::core::{CoreError, CoreResult, media::env::MediaEnv};
use api_types::media::UploadMediaResponse;
use cloudless_core::model::file::ObjectKey;
use cloudless_core::ports::storage::StoragePort;
use std::path::Path;
use uuid::Uuid;

const MAX_FILE_SIZE: usize = 5 * 1024 * 1024; // 5 MB
const ALLOWED_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "gif", "webp", "svg"];

/// Validates and uploads a media file to object storage.
///
/// Returns the public URL path and generated filename.
pub async fn upload_media<E: MediaEnv>(
    env: &E,
    original_filename: &str,
    data: Vec<u8>,
) -> CoreResult<UploadMediaResponse> {
    let extension = Path::new(original_filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();

    if !ALLOWED_EXTENSIONS.contains(&extension.as_str()) {
        return Err(CoreError::validation(format!(
            "Invalid file type '.{}'. Allowed: {}",
            extension,
            ALLOWED_EXTENSIONS.join(", ")
        )));
    }

    if data.len() > MAX_FILE_SIZE {
        return Err(CoreError::validation(format!(
            "File too large ({:.1} MB). Maximum is {} MB.",
            data.len() as f64 / (1024.0 * 1024.0),
            MAX_FILE_SIZE / (1024 * 1024)
        )));
    }

    let uuid = Uuid::now_v7();
    let filename = format!("{}.{}", uuid, extension);
    let key = ObjectKey::new(format!("media/{}", filename));

    tracing::info!(
        "Uploading media to S3: key='{}', size={} bytes",
        key.as_str(),
        data.len()
    );
    StoragePort::put(env.media_storage(), &key, data)
        .await
        .map_err(|e| {
            tracing::error!("S3 put failed for key '{}': {:#?}", key.as_str(), e);
            CoreError::internal(format!("Failed to upload media: {}", e))
        })?;

    tracing::info!("Media uploaded: /media/{}", filename);

    Ok(UploadMediaResponse {
        url: format!("/media/{}", filename),
        filename,
    })
}
