use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::common::Base64EncryptedData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteStorageEntity {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub storage_type: RemoteStorageType,
    pub config: Base64EncryptedData,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRemoteStorageRequest {
    pub name: String,
    pub storage_type: RemoteStorageType,
    pub config: Base64EncryptedData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateRemoteStorageResponse {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RemoteStorageType {
    Aws,
    GoogleDrive,
    LocalFilesystem,
    Sftp,
    OneDrive,
}

impl std::fmt::Display for RemoteStorageType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Aws => write!(f, "AWS S3"),
            Self::GoogleDrive => write!(f, "Google Drive"),
            Self::LocalFilesystem => write!(f, "Local Filesystem"),
            Self::Sftp => write!(f, "SFTP Server"),
            Self::OneDrive => write!(f, "Microsoft OneDrive"),
        }
    }
}

impl std::str::FromStr for RemoteStorageType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Aws" | "AWS S3" => Ok(Self::Aws),
            "GoogleDrive" | "Google Drive" => Ok(Self::GoogleDrive),
            "LocalFilesystem" | "Local Filesystem" => Ok(Self::LocalFilesystem),
            "Sftp" | "SFTP" | "SFTP Server" => Ok(Self::Sftp),
            "OneDrive" | "Microsoft OneDrive" | "MS OneDrive" => Ok(Self::OneDrive),
            _ => Err(format!("unknown storage type: {}", s)),
        }
    }
}

/// Returns the database-safe serialization key for `RemoteStorageType`.
///
/// `Display` is user-facing ("AWS S3", "Google Drive") while this method
/// returns the stable string stored in the `storage_type` DB column and
/// parsed back via `FromStr`.
impl RemoteStorageType {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::Aws => "Aws",
            Self::GoogleDrive => "GoogleDrive",
            Self::LocalFilesystem => "LocalFilesystem",
            Self::Sftp => "Sftp",
            Self::OneDrive => "OneDrive",
        }
    }
}

/// Status of a remote storage connection.
///
/// Tracks whether the storage's auth credentials are still valid.
/// Stored in the `status` column of the `remote_storages` DB table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemoteStorageStatus {
    Active,
    AuthTokenExpired,
}

impl std::fmt::Display for RemoteStorageStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Active => write!(f, "Active"),
            Self::AuthTokenExpired => write!(f, "AuthTokenExpired"),
        }
    }
}

impl std::str::FromStr for RemoteStorageStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Active" | "active" => Ok(Self::Active),
            "AuthTokenExpired" | "auth_token_expired" => Ok(Self::AuthTokenExpired),
            _ => Err(format!("unknown remote storage status: {}", s)),
        }
    }
}

impl Default for RemoteStorageStatus {
    fn default() -> Self {
        Self::Active
    }
}

/// Returns the database-safe serialization key for `RemoteStorageStatus`.
impl RemoteStorageStatus {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::Active => "Active",
            Self::AuthTokenExpired => "AuthTokenExpired",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct S3Credentials {
    pub access_key: String,
    pub secret: String,
    pub region: String,
    pub bucket: String,
}

/// Google Drive OAuth2 credentials.
///
/// Stores the refresh token and app-managed root folder ID. The access token
/// is short-lived and obtained at runtime by exchanging the refresh token.
/// Identity fields (`google_user_id`, `google_user_email`) are captured during
/// initial OAuth and used for reauth identity verification on the client side.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoogleDriveCredentials {
    pub client_id: String,
    /// Legacy field — no longer written for new authorisations.
    ///
    /// Google treats desktop/installed-app OAuth clients as public clients, so
    /// the secret provides no security benefit and should not be embedded in
    /// distributed binaries. Kept as `Option` so existing encrypted credentials
    /// that were stored before this change can still be deserialised.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_secret: Option<String>,
    pub refresh_token: String,
    /// The root folder ID in Google Drive where all backup data is stored.
    pub root_folder_id: String,
    /// Stable Google account identifier from the userinfo endpoint.
    /// Used to verify reauth uses the same account (prevents data loss).
    /// `None` for storages created before identity capture was added.
    #[serde(default)]
    pub google_user_id: Option<String>,
    /// Google account email, displayed to help users pick the correct account during reauth.
    #[serde(default)]
    pub google_user_email: Option<String>,
}

/// Microsoft OneDrive OAuth2 credentials.
///
/// The refresh token is encrypted client-side inside `RemoteStorageConfig`.
/// Identity fields are captured during OAuth and used to prevent reauth with a
/// different Microsoft account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OneDriveCredentials {
    pub client_id: String,
    pub tenant: String,
    pub refresh_token: String,
    #[serde(default)]
    pub microsoft_user_id: Option<String>,
    #[serde(default)]
    pub microsoft_user_email: Option<String>,
}

/// Local filesystem storage configuration.
/// The root path is where chunk files are stored on the local disk
/// (e.g. an external USB drive or NAS mount point).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalFilesystemConfig {
    pub root_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpCredentials {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub auth: SftpAuthConfig,
    pub remote_root_path: String,
    pub host_key_policy: SftpHostKeyPolicy,
    pub known_host_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SftpAuthConfig {
    Password {
        password: String,
    },
    PrivateKey {
        private_key_pem: String,
        passphrase: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SftpHostKeyPolicy {
    Strict,
    TrustOnFirstUse,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RemoteStorageConfig {
    Aws(S3Credentials),
    GoogleDrive(GoogleDriveCredentials),
    LocalFilesystem(LocalFilesystemConfig),
    Sftp(SftpCredentials),
    OneDrive(OneDriveCredentials),
}

impl RemoteStorageConfig {
    pub fn get_type(&self) -> RemoteStorageType {
        match self {
            Self::Aws(_) => RemoteStorageType::Aws,
            Self::GoogleDrive(_) => RemoteStorageType::GoogleDrive,
            Self::LocalFilesystem(_) => RemoteStorageType::LocalFilesystem,
            Self::Sftp(_) => RemoteStorageType::Sftp,
            Self::OneDrive(_) => RemoteStorageType::OneDrive,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn sftp_storage_type_display_parse_and_db_string() {
        assert_eq!(RemoteStorageType::Sftp.to_string(), "SFTP Server");
        assert_eq!(RemoteStorageType::Sftp.as_db_str(), "Sftp");
        assert!(matches!(
            RemoteStorageType::from_str("Sftp").unwrap(),
            RemoteStorageType::Sftp
        ));
        assert!(matches!(
            RemoteStorageType::from_str("SFTP").unwrap(),
            RemoteStorageType::Sftp
        ));
        assert!(matches!(
            RemoteStorageType::from_str("SFTP Server").unwrap(),
            RemoteStorageType::Sftp
        ));
    }

    #[test]
    fn sftp_storage_config_json_round_trip() {
        let config = RemoteStorageConfig::Sftp(SftpCredentials {
            host: "sftp.example.com".to_string(),
            port: 22,
            username: "backup-user".to_string(),
            auth: SftpAuthConfig::PrivateKey {
                private_key_pem: "-----BEGIN OPENSSH PRIVATE KEY-----".to_string(),
                passphrase: Some("passphrase".to_string()),
            },
            remote_root_path: "/backups/cloudless".to_string(),
            host_key_policy: SftpHostKeyPolicy::Strict,
            known_host_key: Some("SHA256:abc123".to_string()),
        });

        let json = serde_json::to_string(&config).unwrap();
        let decoded: RemoteStorageConfig = serde_json::from_str(&json).unwrap();

        match decoded {
            RemoteStorageConfig::Sftp(creds) => {
                assert_eq!(creds.host, "sftp.example.com");
                assert_eq!(creds.port, 22);
                assert_eq!(creds.known_host_key.as_deref(), Some("SHA256:abc123"));
            }
            _ => panic!("expected SFTP config"),
        }
    }

    #[test]
    fn onedrive_storage_type_display_parse_and_db_string() {
        assert_eq!(
            RemoteStorageType::OneDrive.to_string(),
            "Microsoft OneDrive"
        );
        assert_eq!(RemoteStorageType::OneDrive.as_db_str(), "OneDrive");
        assert!(matches!(
            RemoteStorageType::from_str("OneDrive").unwrap(),
            RemoteStorageType::OneDrive
        ));
        assert!(matches!(
            RemoteStorageType::from_str("Microsoft OneDrive").unwrap(),
            RemoteStorageType::OneDrive
        ));
        assert!(matches!(
            RemoteStorageType::from_str("MS OneDrive").unwrap(),
            RemoteStorageType::OneDrive
        ));
    }

    #[test]
    fn onedrive_storage_config_json_round_trip() {
        let config = RemoteStorageConfig::OneDrive(OneDriveCredentials {
            client_id: "client-id".to_string(),
            tenant: "common".to_string(),
            refresh_token: "refresh-token".to_string(),
            microsoft_user_id: Some("user-id".to_string()),
            microsoft_user_email: Some("user@example.com".to_string()),
        });

        let json = serde_json::to_string(&config).unwrap();
        let decoded: RemoteStorageConfig = serde_json::from_str(&json).unwrap();

        match decoded {
            RemoteStorageConfig::OneDrive(creds) => {
                assert_eq!(creds.client_id, "client-id");
                assert_eq!(creds.tenant, "common");
                assert_eq!(creds.microsoft_user_id.as_deref(), Some("user-id"));
                assert_eq!(
                    creds.microsoft_user_email.as_deref(),
                    Some("user@example.com")
                );
            }
            _ => panic!("expected OneDrive config"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRemoteStorageRequest {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRemoteStorageResponse {
    pub storage: RemoteStorageEntity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRemoteStoragesResponse {
    pub list: Vec<RemoteStorageSummary>,
}

/// Lightweight summary for listing remote storages.
/// Includes `status` so the UI can show expiry indicators without decrypting config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteStorageSummary {
    pub id: Uuid,
    pub name: String,
    pub storage_type: RemoteStorageType,
    pub status: RemoteStorageStatus,
    pub created_at: DateTime<Utc>,
}

/// Request to update the status of a remote storage (e.g. mark as AuthTokenExpired).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRemoteStorageStatusRequest {
    pub id: Uuid,
    pub status: RemoteStorageStatus,
}

/// Response after updating remote storage status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRemoteStorageStatusResponse {
    pub success: bool,
}

/// Request to reauth a remote storage with new encrypted credentials.
/// Identity verification is done client-side; the server just stores the
/// new encrypted config and resets status to Active.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReauthRemoteStorageRequest {
    pub id: Uuid,
    pub config: Base64EncryptedData,
}

/// Response after successful reauth.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReauthRemoteStorageResponse {
    pub success: bool,
}
