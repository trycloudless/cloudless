use leptos::prelude::*;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppPhase {
    Auth,
    EmailVerification,
    Setup(SetupStep),
    PolicyAcceptance,
    Main(MainView),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupStep {
    EncryptionPassword,
    Device,
    RemoteStorage,
    BackupConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainView {
    Dashboard,
    Files,
    Backup,
    BackupReports,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMode {
    Login,
    Signup,
    ForgotPassword,
    UnlockEncryption,
}

#[derive(Debug, Clone)]
pub struct BackupProgress {
    pub phase: String,
    pub current_file: String,
    pub total_files: usize,
    pub completed_files: usize,
    pub failed_files: usize,
    pub total_bytes: u64,
    pub uploaded_bytes: u64,
    pub deduplicated_bytes: u64,
    pub completed_bytes: u64,
    /// Sum of original file sizes for files that have fully completed (not in-progress).
    /// Used to compute accurate storage savings mid-backup without conflating
    /// "not yet uploaded" files with actual dedup/compression savings.
    pub completed_original_bytes: u64,
    pub current_file_size: u64,
    pub current_file_uploaded: u64,
    pub is_running: bool,
    /// Which backup config is currently running, so per-card UI can check
    /// `active_config_id == Some(config_id)` rather than the global `is_running`.
    pub active_config_id: Option<Uuid>,
}

impl Default for BackupProgress {
    fn default() -> Self {
        Self {
            phase: "Idle".to_string(),
            current_file: String::new(),
            total_files: 0,
            completed_files: 0,
            failed_files: 0,
            total_bytes: 0,
            uploaded_bytes: 0,
            deduplicated_bytes: 0,
            completed_bytes: 0,
            completed_original_bytes: 0,
            current_file_size: 0,
            current_file_uploaded: 0,
            is_running: false,
            active_config_id: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub user_id: Option<Uuid>,
    pub user_name: String,
    pub user_email: String,
    pub physical_device_id: String,
    pub device_display_name: Option<String>,
    pub device_platform: String,
    pub local_device_id: Option<Uuid>,
    pub storage_id: Option<Uuid>,
    pub config_id: Option<Uuid>,
    pub source_directory: Option<String>,
    pub storage_type: Option<String>,
    pub device_registered: bool,
    pub storage_configured: bool,
    /// True when the user is returning (via UnlockForm) rather than going through
    /// the setup wizard for the first time. Used by DeviceRegistration to navigate
    /// to Dashboard instead of continuing the setup wizard.
    pub returning_user: bool,
    /// True when the user has just completed email verification (new account).
    /// PolicyAcceptance uses this to route to Setup(EncryptionPassword) instead
    /// of Dashboard once all pending policies are resolved.
    pub is_first_time_setup: bool,
    /// One-shot banner shown after encryption setup routes to the next setup step.
    pub encryption_ready_banner: bool,
    /// Temporary: plaintext password held in memory during the email verification
    /// step so we can auto-login immediately after the code is confirmed.
    /// Cleared as soon as login succeeds or the user leaves the verification screen.
    pub pending_password: String,
}

impl Default for SessionInfo {
    fn default() -> Self {
        Self {
            user_id: None,
            user_name: String::new(),
            user_email: String::new(),
            physical_device_id: String::new(),
            device_display_name: None,
            device_platform: String::new(),
            local_device_id: None,
            storage_id: None,
            config_id: None,
            source_directory: None,
            storage_type: None,
            device_registered: false,
            storage_configured: false,
            returning_user: false,
            is_first_time_setup: false,
            encryption_ready_banner: false,
            pending_password: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct RestoreProgress {
    pub phase: String,
    pub current_file: String,
    pub total_files: usize,
    pub completed_files: usize,
    pub total_bytes: u64,
    pub restored_bytes: u64,
    pub current_file_size: u64,
    pub current_file_restored: u64,
    pub is_running: bool,
}

impl Default for RestoreProgress {
    fn default() -> Self {
        Self {
            phase: "Idle".to_string(),
            current_file: String::new(),
            total_files: 0,
            completed_files: 0,
            total_bytes: 0,
            restored_bytes: 0,
            current_file_size: 0,
            current_file_restored: 0,
            is_running: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SchedulerState {
    pub enabled: bool,
    pub interval_minutes: u64,
    pub next_backup_at: Option<String>,
}

impl Default for SchedulerState {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_minutes: 60,
            next_backup_at: None,
        }
    }
}

pub type PhaseSignal = (ReadSignal<AppPhase>, WriteSignal<AppPhase>);
pub type AuthModeSignal = (ReadSignal<AuthMode>, WriteSignal<AuthMode>);
pub type BackupProgressSignal = (ReadSignal<BackupProgress>, WriteSignal<BackupProgress>);
pub type RestoreProgressSignal = (ReadSignal<RestoreProgress>, WriteSignal<RestoreProgress>);
pub type SessionSignal = (ReadSignal<SessionInfo>, WriteSignal<SessionInfo>);
pub type SchedulerSignal = (ReadSignal<SchedulerState>, WriteSignal<SchedulerState>);
/// Active tab within the Settings view. Values: "system", "infrastructure", "security", "data", "preferences".
pub type SettingsTabSignal = (ReadSignal<&'static str>, WriteSignal<&'static str>);
/// A config_id to auto-select in FilesView when the user clicks a config in the system tray.
/// Set by the navigate-to-config event listener; consumed and cleared by FilesView on mount.
pub type TrayNavSignal = RwSignal<Option<Uuid>>;
/// True after the user has successfully unlocked encryption; false on lock or logout.
/// All data-fetching Effects gate on this to avoid firing before the DEK is available.
pub type IsUnlockedSignal = RwSignal<bool>;
