use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{common::Base64EncryptedData, remote_storage::RemoteStorageType};

/// Configures automatic local file cleanup after a successful backup.
///
/// `NoCleanup` (default) leaves all local files untouched. `DaysAfterLastBackup(n)`
/// deletes files from the source directory whose `synced_at` timestamp is older than
/// `n` days, meaning they have been safely backed up for at least that long.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
pub enum CleanupType {
    #[default]
    NoCleanup,
    DaysAfterLastBackup(u16),
}

// ── Exclusion types ───────────────────────────────────────────────────────────

/// The full exclusion config for a backup — a flat list of entries.
///
/// Struct wrapper (not bare Vec) keeps the API boundary stable, centralises
/// `Default`/serde, and provides a home for helper methods.
/// Existing configs deserialise to the default (empty, no exclusions).
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq)]
pub struct BackupExclusionConfig {
    #[serde(default)]
    pub entries: Vec<BackupExclusionEntry>,
}

impl BackupExclusionConfig {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn presets(&self) -> impl Iterator<Item = &BackupExclusionPreset> {
        self.entries.iter().filter_map(|e| {
            if let BackupExclusionEntry::Preset(p) = e {
                Some(p)
            } else {
                None
            }
        })
    }

    pub fn custom_rules(&self) -> impl Iterator<Item = &BackupExclusionRule> {
        self.entries.iter().filter_map(|e| {
            if let BackupExclusionEntry::Custom(r) = e {
                Some(r)
            } else {
                None
            }
        })
    }

    pub fn globs(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().filter_map(|e| {
            if let BackupExclusionEntry::Glob(g) = e {
                Some(g.as_str())
            } else {
                None
            }
        })
    }

    /// Human-readable summary for display on config cards.
    /// Returns `"None"` when there are no exclusions.
    pub fn summary_label(&self) -> String {
        let mut parts: Vec<String> = Vec::new();

        for preset in self.presets() {
            parts.push(preset.display_name().to_string());
        }

        let custom_count = self.custom_rules().filter(|r| r.enabled).count();
        let glob_count = self.globs().count();
        let extra = custom_count + glob_count;
        if extra > 0 {
            parts.push(format!(
                "{} custom rule{}",
                extra,
                if extra == 1 { "" } else { "s" }
            ));
        }

        if parts.is_empty() {
            "None".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// A single exclusion entry. All three sources (presets, simple rules, raw globs)
/// are unified into one enum so downstream code iterates one collection and
/// exhaustive pattern matching enforces handling of new variants.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(tag = "kind", content = "data")]
pub enum BackupExclusionEntry {
    Preset(BackupExclusionPreset),
    /// A structured rule built through the simple rule builder UI.
    Custom(BackupExclusionRule),
    /// A raw glob pattern entered in the advanced section.
    Glob(String),
}

/// Built-in exclusion presets covering common categories.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum BackupExclusionPreset {
    TemporaryAndCache,
    InstallersAndArchives,
    DeveloperDependencies,
    OsMetadata,
}

impl BackupExclusionPreset {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::TemporaryAndCache => "Temporary and cache files",
            Self::InstallersAndArchives => "Installers and archives",
            Self::DeveloperDependencies => "Developer dependency folders",
            Self::OsMetadata => "OS metadata",
        }
    }
}

/// A single user-defined exclusion rule built through the simple rule builder.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct BackupExclusionRule {
    pub id: String,
    pub enabled: bool,
    pub kind: BackupExclusionRuleKind,
    pub value: String,
}

/// How a simple custom rule matches files or folders.
/// Advanced glob patterns are represented as `BackupExclusionEntry::Glob` directly.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub enum BackupExclusionRuleKind {
    FileExtension,
    FolderName,
    FileOrFolderName,
    PathContains,
}

// ── Preview types (local-only, never persisted on server) ─────────────────────

/// Request to preview which files would be excluded by a given exclusion config.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PreviewBackupExclusionsRequest {
    pub source_directory: String,
    pub exclusions: BackupExclusionConfig,
    pub sample_limit: usize,
}

/// Result of a local exclusion preview walk.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PreviewBackupExclusionsResponse {
    pub included_files: usize,
    pub excluded_files: usize,
    pub included_bytes: u64,
    pub excluded_bytes: u64,
    pub sample_excluded_files: Vec<PreviewExcludedFile>,
    pub errors: Vec<String>,
}

/// A single file that would be excluded, with the matching rule.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PreviewExcludedFile {
    pub path: String,
    pub size: u64,
    /// The glob pattern that matched this file, if determinable.
    pub matched_rule: Option<String>,
}

// ── Update exclusion config request/response ──────────────────────────────────

/// Client sends encrypted exclusion config when updating an existing backup config.
/// The server stores the encrypted bytes opaquely — it never sees plaintext exclusions.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateExclusionConfigRequest {
    pub id: Uuid,
    pub encrypted_exclusion_config: Vec<u8>,
    pub exclusion_config_nonce: Vec<u8>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateExclusionConfigResponse {
    pub id: Uuid,
}

// ── Server wire types ─────────────────────────────────────────────────────────

/// Encrypted backup config as stored on the server.
/// The source directory is encrypted client-side — the server never sees plaintext.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackupConfig {
    pub id: Uuid,
    pub user_id: Uuid,
    pub storage_id: Uuid,
    pub local_device_id: Uuid,
    pub encrypted_source_dir: Vec<u8>,
    pub source_dir_nonce: Vec<u8>,
    pub source_dir_blind_index: Vec<u8>,
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub cleanup_type: CleanupType,
    /// Encrypted exclusion config. `None` for configs created before this feature.
    #[serde(default)]
    pub encrypted_exclusion_config: Option<Vec<u8>>,
    #[serde(default)]
    pub exclusion_config_nonce: Option<Vec<u8>>,
}

/// Client sends encrypted source directory when creating a backup config.
#[derive(Debug, Deserialize, Serialize)]
pub struct CreateBackupConfigRequest {
    pub storage_id: Uuid,
    pub display_name: String,
    pub local_device_id: Uuid,
    pub encrypted_source_dir: Vec<u8>,
    pub source_dir_nonce: Vec<u8>,
    pub source_dir_blind_index: Vec<u8>,
    #[serde(default)]
    pub cleanup_type: CleanupType,
    #[serde(default)]
    pub encrypted_exclusion_config: Option<Vec<u8>>,
    #[serde(default)]
    pub exclusion_config_nonce: Option<Vec<u8>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateBackupConfigResponse {
    pub id: Uuid,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ListBackupConfigWithRemoteStorageRequest {
    pub physical_device_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListBackupConfigWithRemoteStorageResponse {
    pub list: Vec<BackupConfigWithRemoteStorage>,
}

/// Backup config joined with remote storage info — encrypted source directory.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BackupConfigWithRemoteStorage {
    pub storage_id: Uuid,
    pub encrypted_source_dir: Vec<u8>,
    pub source_dir_nonce: Vec<u8>,
    pub config_id: Uuid,
    pub display_name: String,
    pub storage_type: RemoteStorageType,
    pub config: Base64EncryptedData,
    #[serde(default)]
    pub local_device_id: Option<Uuid>,
    #[serde(default)]
    pub physical_device_id: Option<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub cleanup_type: CleanupType,
    #[serde(default)]
    pub encrypted_exclusion_config: Option<Vec<u8>>,
    #[serde(default)]
    pub exclusion_config_nonce: Option<Vec<u8>>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ListAllBackupConfigsResponse {
    pub list: Vec<BackupConfigWithRemoteStorage>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetBackupConfigRequest {
    pub id: Uuid,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GetBackupConfigResponse {
    pub config: BackupConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToggleBackupConfigRequest {
    pub id: Uuid,
    pub is_active: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToggleBackupConfigResponse {
    pub id: Uuid,
    pub is_active: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenameBackupConfigRequest {
    pub id: Uuid,
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RenameBackupConfigResponse {
    pub id: Uuid,
    pub display_name: String,
}

// ── UI request types (plaintext, sent from UI to Tauri) ────────────

/// UI-facing request for creating a backup config with plaintext source directory.
/// Tauri encrypts this before sending to the server.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateBackupConfigUiRequest {
    pub storage_id: Uuid,
    pub display_name: String,
    pub local_device_id: Uuid,
    pub source_directory: String,
    #[serde(default)]
    pub cleanup_type: CleanupType,
    #[serde(default)]
    pub exclusion_config: BackupExclusionConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateCleanupTypeRequest {
    pub id: Uuid,
    pub cleanup_type: CleanupType,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UpdateCleanupTypeResponse {
    pub id: Uuid,
    pub cleanup_type: CleanupType,
}

// ── Decrypted types (used by core → Tauri → UI) ───────────────────

/// Decrypted backup config with plaintext source directory.
/// Produced by `cloudless_core` after decrypting the encrypted server response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DecryptedBackupConfig {
    pub id: Uuid,
    pub user_id: Uuid,
    pub storage_id: Uuid,
    pub local_device_id: Uuid,
    pub source_directory: String,
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub cleanup_type: CleanupType,
    #[serde(default)]
    pub exclusion_config: BackupExclusionConfig,
}

/// Decrypted backup config joined with remote storage info.
/// Produced by `cloudless_core` after decrypting the encrypted server response.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DecryptedBackupConfigWithRemoteStorage {
    pub storage_id: Uuid,
    pub source_directory: String,
    pub config_id: Uuid,
    pub display_name: String,
    pub storage_type: RemoteStorageType,
    pub config: Base64EncryptedData,
    #[serde(default)]
    pub local_device_id: Option<Uuid>,
    #[serde(default)]
    pub physical_device_id: Option<String>,
    #[serde(default = "default_true")]
    pub is_active: bool,
    #[serde(default)]
    pub cleanup_type: CleanupType,
    #[serde(default)]
    pub exclusion_config: BackupExclusionConfig,
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusion_config_default_is_empty() {
        let config = BackupExclusionConfig::default();
        assert!(config.is_empty());
        assert_eq!(config.summary_label(), "None");
    }

    #[test]
    fn exclusion_config_serde_roundtrip() {
        let config = BackupExclusionConfig {
            entries: vec![
                BackupExclusionEntry::Preset(BackupExclusionPreset::OsMetadata),
                BackupExclusionEntry::Custom(BackupExclusionRule {
                    id: "r1".to_string(),
                    enabled: true,
                    kind: BackupExclusionRuleKind::FileExtension,
                    value: ".dmg".to_string(),
                }),
                BackupExclusionEntry::Glob("**/node_modules/**".to_string()),
            ],
        };
        let json = serde_json::to_string(&config).unwrap();
        let decoded: BackupExclusionConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn exclusion_config_backwards_compat_missing_field() {
        // Old BackupConfig JSON without exclusion fields must deserialise cleanly.
        let json = r#"{
            "id": "00000000-0000-0000-0000-000000000001",
            "user_id": "00000000-0000-0000-0000-000000000002",
            "storage_id": "00000000-0000-0000-0000-000000000003",
            "local_device_id": "00000000-0000-0000-0000-000000000004",
            "encrypted_source_dir": [1,2,3],
            "source_dir_nonce": [4,5,6],
            "source_dir_blind_index": [7,8,9]
        }"#;
        let config: BackupConfig = serde_json::from_str(json).unwrap();
        assert!(config.encrypted_exclusion_config.is_none());
        assert!(config.exclusion_config_nonce.is_none());
    }

    #[test]
    fn summary_label_with_mixed_entries() {
        let config = BackupExclusionConfig {
            entries: vec![
                BackupExclusionEntry::Preset(BackupExclusionPreset::OsMetadata),
                BackupExclusionEntry::Preset(BackupExclusionPreset::InstallersAndArchives),
                BackupExclusionEntry::Custom(BackupExclusionRule {
                    id: "r1".to_string(),
                    enabled: true,
                    kind: BackupExclusionRuleKind::FolderName,
                    value: "build".to_string(),
                }),
                BackupExclusionEntry::Custom(BackupExclusionRule {
                    id: "r2".to_string(),
                    enabled: false, // disabled — not counted
                    kind: BackupExclusionRuleKind::FileExtension,
                    value: ".log".to_string(),
                }),
                BackupExclusionEntry::Glob("**/.idea/**".to_string()),
            ],
        };
        let label = config.summary_label();
        // 2 presets + 1 enabled custom + 1 glob = "OS metadata, Installers and archives, 2 custom rules"
        assert!(label.contains("OS metadata"));
        assert!(label.contains("Installers and archives"));
        assert!(label.contains("2 custom rules"));
    }
}
