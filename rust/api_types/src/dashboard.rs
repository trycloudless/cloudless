use serde::{Deserialize, Serialize};

/// Aggregated dashboard statistics for a user.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDashboardStatsResponse {
    pub total_files_protected: i64,
    pub total_original_bytes: i64,
    pub total_uploaded_bytes: i64,
    pub total_deduplicated_bytes: i64,
    pub active_backup_configs: i64,
    pub total_backup_jobs: i64,
    pub total_restore_jobs: i64,
}
