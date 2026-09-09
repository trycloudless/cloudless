use serde::{Deserialize, Serialize};

/// Response returned after a successful media upload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadMediaResponse {
    /// Relative URL path to the uploaded media, e.g. `/media/abc123.png`
    pub url: String,
    /// The generated filename, e.g. `abc123.png`
    pub filename: String,
}
