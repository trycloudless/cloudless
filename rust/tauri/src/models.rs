use api_types::local_device::DeviceSummary;
use cloudless_core::{
    app_env::AppEnv,
    applications::backup::scheduler::SchedulerConfig,
    domain::{dek::Dek, derived_keys::DerivedKeys},
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::{watch, RwLock};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// A single backup config entry as shown in the system tray menu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrayConfigItem {
    pub id: Uuid,
    pub name: String,
}

/// Payload emitted to the frontend when a tray config item is clicked.
#[derive(Debug, Clone, Serialize)]
pub struct NavigateToConfigPayload {
    pub config_id: Uuid,
}

/// Desktop-only managed state: holds the live TrayIcon handle and the
/// last-known config list so the menu can be rebuilt on demand.
/// Kept separate from AppState to avoid cfging AppState's field list.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub struct TrayState {
    pub icon: Arc<RwLock<Option<tauri::tray::TrayIcon>>>,
    pub configs: Arc<RwLock<Vec<TrayConfigItem>>>,
    pub last_backup_label: Arc<RwLock<Option<String>>>,
}

pub struct AppState {
    pub env: Arc<AppEnv>,
    pub dek_cache: Arc<RwLock<Option<Dek>>>,
    pub derived_keys_cache: Arc<RwLock<Option<DerivedKeys>>>,
    pub device_id_cache: Arc<RwLock<Option<Uuid>>>,
    pub scheduler_handle: Arc<RwLock<Option<SchedulerHandle>>>,
}

/// Handle to control the running auto-backup scheduler.
pub struct SchedulerHandle {
    pub config_tx: watch::Sender<SchedulerConfig>,
    pub cancel: CancellationToken,
    pub join_handle: tokio::task::JoinHandle<()>,
}

/// Auto-backup settings persisted in tauri_plugin_store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoBackupSettings {
    pub enabled: bool,
    pub interval_minutes: u64,
}

#[derive(Debug, Deserialize)]
pub struct SignupAndLoginRequest {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct SignupAndLoginResponse {
    pub user_id: Uuid,
    pub email_verified: bool,
}

#[derive(Debug, Deserialize)]
pub struct SetupEncryptionRequest {
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct UnlockEncryptionRequest {
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct CompleteStorageSetupRequest {
    pub storage_name: String,
    pub s3_access_key: String,
    pub s3_secret: String,
    pub s3_region: String,
    pub s3_bucket: String,
    pub source_directory: String,
    pub local_device_id: Uuid,
}

#[derive(Debug, Serialize)]
pub struct CompleteStorageSetupResponse {
    pub storage_id: Uuid,
    pub config_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct TestS3ConnectionRequest {
    pub s3_access_key: String,
    pub s3_secret: String,
    pub s3_region: String,
    pub s3_bucket: String,
}

#[derive(Debug, Deserialize)]
pub struct AddRemoteStorageRequest {
    pub storage_name: String,
    pub s3_access_key: String,
    pub s3_secret: String,
    pub s3_region: String,
    pub s3_bucket: String,
}

#[derive(Debug, Deserialize)]
pub struct AddLocalStorageRequest {
    pub storage_name: String,
    pub root_path: String,
}

#[derive(Debug, Deserialize)]
pub struct TestLocalConnectionRequest {
    pub root_path: String,
}

#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct AddSftpStorageRequest {
    pub storage_name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: String,
    pub password: Option<String>,
    pub private_key_pem: Option<String>,
    pub private_key_passphrase: Option<String>,
    pub remote_root_path: String,
    pub known_host_key: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct TestSftpConnectionRequest {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth_method: String,
    pub password: Option<String>,
    pub private_key_pem: Option<String>,
    pub private_key_passphrase: Option<String>,
    pub remote_root_path: String,
    pub known_host_key: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct TestSftpConnectionResponse {
    pub host_key_fingerprint: String,
    pub root_exists: bool,
    pub root_writable: bool,
}

#[derive(Debug, Deserialize)]
pub struct StartBackupRequest {
    pub config_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct RestoreFileSelectionDto {
    pub version_id: Uuid,
    pub file_path: String,
    pub size: i64,
    pub version: u32,
}

#[derive(Debug, Deserialize)]
pub struct StartRestoreRequest {
    pub config_id: Uuid,
    pub file_selections: Vec<RestoreFileSelectionDto>,
    pub destination: api_types::restore_job::RestoreDestination,
    pub overwrite_behavior: api_types::restore_job::OverwriteBehavior,
}

#[derive(Debug, Deserialize)]
pub struct UnlockWithRecoveryKeyRequest {
    pub recovery_key: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordAfterRecoveryRequest {
    pub new_password: String,
}

#[derive(Debug, Serialize)]
pub struct ExportRecoveryKeyResponse {
    pub recovery_key: String,
}

#[derive(Debug, Deserialize)]
pub struct ChangeEncryptionPasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

// ── Biometric unlock ───────────────────────────────────────────────

/// Response from `enable_biometric_unlock`.
/// Contains the encrypted DEK blob (base64) that the UI should persist
/// via `save_biometric_dek`, and the biometric key (base64) that the UI
/// should store in the OS keychain via the biometry plugin.
#[derive(Debug, Serialize)]
pub struct EnableBiometricResponse {
    pub bk_base64: String,
    pub encrypted_dek_base64: String,
}

/// Request to persist the encrypted DEK blob for biometric unlock.
#[derive(Debug, Deserialize)]
pub struct SaveBiometricDekRequest {
    pub encrypted_dek_base64: String,
}

/// Request to unlock DEK using the biometric key retrieved from OS keychain.
#[derive(Debug, Deserialize)]
pub struct BiometricUnlockRequest {
    pub bk_base64: String,
}

/// Biometric hardware availability status returned to the UI.
#[derive(Debug, Serialize)]
pub struct BiometricStatusResponse {
    pub is_available: bool,
    pub biometry_type: String,
}

/// Result of the mobile device resolution flow.
/// Returned by `resolve_mobile_device` for the UI to act on.
#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum DeviceResolutionResult {
    /// Device auto-resolved (found in store or single platform match).
    Resolved { device: DeviceSummary },
    /// Multiple candidate devices found — UI must show a picker.
    MultipleFound { devices: Vec<DeviceSummary> },
    /// No device found for this platform — UI must show a new device form.
    NoneFound,
}
