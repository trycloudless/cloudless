//! Auto-backup scheduler.
//!
//! Runs a periodic loop that triggers backups for all active backup configs
//! on the current device. Platform-independent — only the lifecycle management
//! (start/stop, event forwarding) lives in the Tauri layer.

use chrono::{Duration as ChronoDuration, Utc};
use std::time::Duration;
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    applications::backup::{backup_job::start_backup, env::BackupEnv},
    domain::{dek::Dek, derived_keys::DerivedKeys},
    ports::api::{
        backup_config_api_port::BackupConfigApiPort, backup_job_api_port::BackupJobApiPort,
    },
};
use api_types::{
    backup::BackupResult, backup_config::ListBackupConfigWithRemoteStorageRequest,
    backup_job::GetResumableBackupJobRequest,
};

// Re-export from api_types so downstream crates (tauri, leptos_ui) use the same type.
pub use api_types::scheduler::SchedulerEvent;

/// Configuration for the auto-backup scheduler. Sent via `watch` channel
/// so the running loop picks up changes without restart.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub interval: Duration,
    pub enabled: bool,
}

/// Runs the auto-backup scheduler loop.
///
/// This function blocks until the `cancel` token is triggered. It:
/// 1. Waits for the configured interval
/// 2. Lists all active backup configs for the device
/// 3. For each config, checks if a backup is already running
/// 4. Runs backups sequentially (one at a time)
/// 5. Emits `SchedulerEvent`s for the UI
///
/// The `config` watch channel allows dynamic reconfiguration (interval changes,
/// enable/disable) without restarting the loop.
pub async fn run_scheduler_loop<E: BackupEnv + Send + Sync + 'static>(
    env: E,
    dek: Dek,
    derived_keys: DerivedKeys,
    device_id: Uuid,
    physical_device_id: String,
    mut config_rx: watch::Receiver<SchedulerConfig>,
    event_tx: mpsc::Sender<SchedulerEvent>,
    cancel: CancellationToken,
) {
    info!("Auto-backup scheduler started");

    loop {
        // Read the current config
        let current_config = config_rx.borrow_and_update().clone();

        if !current_config.enabled {
            // Wait for config change or cancellation
            tokio::select! {
                _ = cancel.cancelled() => {
                    info!("Scheduler cancelled while disabled");
                    break;
                }
                _ = config_rx.changed() => {
                    continue;
                }
            }
        }

        // Calculate and emit next backup time
        let next_at = Utc::now()
            + ChronoDuration::from_std(current_config.interval).unwrap_or(ChronoDuration::hours(1));
        let _ = event_tx
            .send(SchedulerEvent::NextBackupAt {
                timestamp: next_at.to_rfc3339(),
            })
            .await;

        // Sleep for the interval, but wake on cancel or config change
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("Scheduler cancelled during sleep");
                break;
            }
            _ = config_rx.changed() => {
                // Config changed — re-check from the top
                continue;
            }
            _ = tokio::time::sleep(current_config.interval) => {
                // Time to run backups
            }
        }

        // Re-check if still enabled after sleep
        let current_config = config_rx.borrow().clone();
        if !current_config.enabled {
            continue;
        }

        // Run one backup cycle
        run_backup_cycle(
            &env,
            &dek,
            &derived_keys,
            device_id,
            &physical_device_id,
            &event_tx,
        )
        .await;
    }

    info!("Auto-backup scheduler stopped");
}

/// Runs one cycle: lists configs, runs backups sequentially.
pub async fn run_backup_cycle<E: BackupEnv + Send + Sync + 'static>(
    env: &E,
    dek: &Dek,
    derived_keys: &DerivedKeys,
    device_id: Uuid,
    physical_device_id: &str,
    event_tx: &mpsc::Sender<SchedulerEvent>,
) {
    info!("Scheduler: starting backup cycle");

    // List active backup configs for this device
    let configs = match env
        .backup_config_api()
        .list_with_remote_storage(ListBackupConfigWithRemoteStorageRequest {
            physical_device_id: physical_device_id.to_string(),
        })
        .await
    {
        Ok(resp) => resp.list,
        Err(e) => {
            warn!("Scheduler: failed to list backup configs: {}", e);
            return;
        }
    };

    let active_configs: Vec<_> = configs.into_iter().filter(|c| c.is_active).collect();

    if active_configs.is_empty() {
        info!("Scheduler: no active backup configs found, skipping cycle");
        return;
    }

    info!(
        "Scheduler: found {} active config(s), running backups sequentially",
        active_configs.len()
    );

    for config in active_configs {
        let config_id = config.config_id;
        let config_name = config.display_name.clone();

        // Check if a backup is already running for this config
        match env
            .backup_job_api()
            .get_resumable_job(GetResumableBackupJobRequest {
                backup_config_id: config_id,
            })
            .await
        {
            Ok(resp) if resp.job.is_some() => {
                info!(
                    "Scheduler: skipping config {} (backup already in progress)",
                    config_id
                );
                let _ = event_tx
                    .send(SchedulerEvent::BackupSkipped {
                        config_id,
                        reason: "Backup already in progress".to_string(),
                    })
                    .await;
                continue;
            }
            Err(e) => {
                warn!(
                    "Scheduler: failed to check resumable job for {}: {}",
                    config_id, e
                );
                // Continue anyway — start_backup handles this internally too
            }
            _ => {}
        }

        // Start the backup
        let _ = event_tx
            .send(SchedulerEvent::BackupStarting {
                config_id,
                config_name: config_name.clone(),
            })
            .await;

        let env_clone = env.clone_env();
        let dek_clone = dek.clone();
        let derived_keys_clone = derived_keys.clone();
        let mut rx = start_backup(
            env_clone,
            dek_clone,
            derived_keys_clone,
            device_id,
            config_id,
        );

        // Drain all events to completion
        let mut failed = false;
        while let Some(event) = rx.recv().await {
            match &event {
                BackupResult::Failed { reason, .. } => {
                    warn!(
                        "Scheduler: backup failed for config {}: {}",
                        config_id, reason
                    );
                    failed = true;
                }
                BackupResult::Completed { .. } => {
                    info!("Scheduler: backup completed for config {}", config_id);
                }
                _ => {}
            }
        }

        if failed {
            let _ = event_tx
                .send(SchedulerEvent::BackupFailed {
                    config_id,
                    reason: "Backup failed".to_string(),
                })
                .await;
        } else {
            let _ = event_tx
                .send(SchedulerEvent::BackupCompleted { config_id })
                .await;
        }
    }

    info!("Scheduler: backup cycle finished");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_config_defaults() {
        let config = SchedulerConfig {
            interval: Duration::from_secs(3600),
            enabled: true,
        };
        assert!(config.enabled);
        assert_eq!(config.interval.as_secs(), 3600);
    }

    #[test]
    fn scheduler_event_serialization() {
        let event = SchedulerEvent::BackupStarting {
            config_id: uuid::Uuid::nil(),
            config_name: "Test".to_string(),
        };
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("BackupStarting"));
        assert!(json.contains("Test"));
    }
}
