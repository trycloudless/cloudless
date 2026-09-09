use chrono::{DateTime, Utc};
use mime::Mime;

#[derive(Debug, Clone)]
pub enum FileLocation {
    Local { path: std::path::PathBuf },
    S3 { bucket: String, key: String },
    GoogleDrive { parent_id: String, file_id: String },
}

#[derive(Debug, Clone)]
pub enum CheckSum {
    Md5(String),
    Sha256(String),
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct ObjectKey(String);
impl ObjectKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn new(s: String) -> Self {
        Self(s)
    }
}

#[derive(Debug, Clone)]
pub struct CheckSums(pub Vec<CheckSum>);
#[derive(Debug, Clone)]
pub struct MediaType(pub Mime);

#[derive(Debug, Clone)]
pub struct FileMetadata {
    pub name: String,
    pub size: u64,
    pub checksums: CheckSums, // For integrity checks
    pub modified_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
    pub location: FileLocation,
    pub media_type: MediaType,
    pub is_directory: bool,
}
