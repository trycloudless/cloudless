/// Client-side backup candidate detected by comparing the filesystem against the local SQLite index.
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct ClientBackupCandidate {
    pub path: String,
    pub size: i64,
    pub mtime: DateTime<Utc>,
    /// Last version the local index has recorded for this file. `None` if the
    /// file has never been backed up before. Sent to the server as the
    /// optimistic-concurrency check — the server assigns the actual next version.
    pub base_version: Option<u32>,
    pub encrypted_name: Vec<u8>,
    pub encrypted_name_nonce: Vec<u8>,
    pub blind_index: Vec<u8>,
    pub remote_file_id: Option<Uuid>,
}
