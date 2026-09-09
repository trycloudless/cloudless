/// Local SQLite index entry representing a tracked file on the client device.
///
/// Scoped by `backup_config_id` so multiple backup configs can independently
/// track the same file path without collisions.
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct LocalIndexEntry {
    pub backup_config_id: Uuid,
    pub path: String,
    pub size: i64,
    pub mtime: DateTime<Utc>,
    pub content_hash: Option<String>,
    pub encrypted_name: Vec<u8>,
    pub encrypted_name_nonce: Vec<u8>,
    pub blind_index: Vec<u8>,
    pub remote_file_id: Option<Uuid>,
    pub last_backed_up_version: Option<i32>,
    pub synced_at: Option<DateTime<Utc>>,
}
