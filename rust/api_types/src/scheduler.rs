use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Events emitted by the auto-backup scheduler for the UI layer to consume.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SchedulerEvent {
    /// A scheduled backup cycle is starting for a specific config.
    BackupStarting {
        config_id: Uuid,
        config_name: String,
    },
    /// A scheduled backup completed successfully for a config.
    BackupCompleted { config_id: Uuid },
    /// A scheduled backup was skipped (already running).
    BackupSkipped { config_id: Uuid, reason: String },
    /// A scheduled backup failed for a config.
    BackupFailed { config_id: Uuid, reason: String },
    /// The next scheduled backup time.
    NextBackupAt { timestamp: String },
    /// The scheduler was enabled or disabled.
    StateChanged { enabled: bool, interval_secs: u64 },
}
