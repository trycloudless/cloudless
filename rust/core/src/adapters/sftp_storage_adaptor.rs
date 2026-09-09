use std::sync::{Arc, Mutex};
use std::time::Duration;

use api_types::remote_storage::{SftpAuthConfig, SftpCredentials};
use async_trait::async_trait;
use data_encoding::BASE64_NOPAD;
use rand::distr::{Alphanumeric, SampleString};
use russh::client::{self, AuthResult};
use russh::keys::{PrivateKeyWithHashAlg, decode_secret_key, ssh_key};
use russh_sftp::client::SftpSession;
use russh_sftp::protocol::OpenFlags;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;
use tokio::time::timeout;
use tracing::{debug, instrument};

use crate::model::app_error::AppError;
use crate::model::base::AppResult;
use crate::model::file::ObjectKey;
use crate::ports::storage::StoragePort;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const AUTH_TIMEOUT: Duration = Duration::from_secs(30);
const OPERATION_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SftpConnectionTestResult {
    pub host_key_fingerprint: String,
    pub root_exists: bool,
    pub root_writable: bool,
}

#[derive(Debug, Clone)]
pub struct SftpStorageAdaptor {
    creds: SftpCredentials,
    remote_root_path: String,
}

impl SftpStorageAdaptor {
    pub fn new(creds: SftpCredentials) -> AppResult<Self> {
        let remote_root_path = normalize_root_path(&creds.remote_root_path)?;
        validate_sftp_config(&creds)?;
        Ok(Self {
            creds,
            remote_root_path,
        })
    }

    pub async fn test_connection(creds: SftpCredentials) -> AppResult<SftpConnectionTestResult> {
        let adaptor = Self::new(creds)?;
        let connection = adaptor.connect().await?;
        let root_exists = connection
            .sftp
            .try_exists(&adaptor.remote_root_path)
            .await
            .map_err(|e| map_sftp_error(e, "failed to check remote folder"))?;
        if !root_exists {
            create_dir_all(&connection.sftp, &adaptor.remote_root_path).await?;
        }

        let test_path = join_remote_path(
            &adaptor.remote_root_path,
            &format!(".cloudless-connection-test-{}", random_suffix()),
        );
        write_sftp_file(&connection.sftp, &test_path, b"cloudless").await?;
        connection
            .sftp
            .remove_file(&test_path)
            .await
            .map_err(|e| map_sftp_error(e, "failed to remove connection test file"))?;

        Ok(SftpConnectionTestResult {
            host_key_fingerprint: connection.host_key_fingerprint,
            root_exists,
            root_writable: true,
        })
    }

    async fn connect(&self) -> AppResult<SftpConnection> {
        let seen_host_key = Arc::new(Mutex::new(None));
        let handler = SftpClient {
            expected_host_key: self.creds.known_host_key.clone(),
            seen_host_key: Arc::clone(&seen_host_key),
        };

        let mut config = client::Config::default();
        config.inactivity_timeout = Some(OPERATION_TIMEOUT);

        let addr = (self.creds.host.as_str(), self.creds.port);
        let mut session = timeout(
            CONNECT_TIMEOUT,
            client::connect(Arc::new(config), addr, handler),
        )
        .await
        .map_err(|_| AppError::Network {
            message: "SFTP connection timed out".to_string(),
            source: None,
        })?
        .map_err(|e| AppError::Network {
            message: format!("SFTP server not reachable: {}", e),
            source: Some(Box::new(e)),
        })?;

        let auth_result = timeout(AUTH_TIMEOUT, authenticate(&mut session, &self.creds))
            .await
            .map_err(|_| AppError::Network {
                message: "SFTP authentication timed out".to_string(),
                source: None,
            })??;

        if !auth_result.success() {
            return Err(AppError::PermissionDenied {
                message: "SFTP authentication failed".to_string(),
                source: None,
            });
        }

        let channel = session
            .channel_open_session()
            .await
            .map_err(|e| AppError::Network {
                message: format!("failed to open SFTP session channel: {}", e),
                source: Some(Box::new(e)),
            })?;
        channel
            .request_subsystem(true, "sftp")
            .await
            .map_err(|e| AppError::Network {
                message: format!("SFTP subsystem unavailable: {}", e),
                source: Some(Box::new(e)),
            })?;
        let sftp = SftpSession::new(channel.into_stream())
            .await
            .map_err(|e| map_sftp_error(e, "failed to start SFTP session"))?;
        sftp.set_timeout(OPERATION_TIMEOUT.as_secs());

        let host_key_fingerprint = seen_host_key
            .lock()
            .ok()
            .and_then(|guard| guard.clone())
            .ok_or_else(|| AppError::Internal {
                message: "SFTP host key was not captured".to_string(),
                source: None,
            })?;

        Ok(SftpConnection {
            session,
            sftp,
            host_key_fingerprint,
        })
    }

    fn resolve_path(&self, key: &ObjectKey) -> AppResult<String> {
        validate_object_key(key)?;
        Ok(join_remote_path(&self.remote_root_path, key.as_str()))
    }
}

#[async_trait]
impl StoragePort for SftpStorageAdaptor {
    #[instrument(skip(self, data), fields(key = %key.as_str(), size = data.len()))]
    async fn put(&self, key: &ObjectKey, data: Vec<u8>) -> AppResult<()> {
        let connection = self.connect().await?;
        let path = self.resolve_path(key)?;
        if let Some(parent) = parent_remote_path(&path) {
            create_dir_all(&connection.sftp, &parent).await?;
        }

        let temp_path = format!("{}.cloudless-uploading-{}", path, random_suffix());
        write_sftp_file(&connection.sftp, &temp_path, &data).await?;

        if let Err(rename_error) = connection.sftp.rename(&temp_path, &path).await {
            if connection.sftp.try_exists(&path).await.unwrap_or(false) {
                connection
                    .sftp
                    .remove_file(&path)
                    .await
                    .map_err(|e| map_sftp_error(e, "failed to replace existing SFTP object"))?;
                connection
                    .sftp
                    .rename(&temp_path, &path)
                    .await
                    .map_err(|e| map_sftp_error(e, "failed to finalize SFTP object replacement"))?;
                debug!("replaced existing SFTP object after upload rename conflict");
                return Ok(());
            }
            let _ = connection.sftp.remove_file(&temp_path).await;
            return Err(map_sftp_error(
                rename_error,
                "failed to finalize SFTP object upload",
            ));
        }

        debug!("stored {} bytes", data.len());
        Ok(())
    }

    #[instrument(skip(self), fields(key = %key.as_str()))]
    async fn get(&self, key: &ObjectKey) -> AppResult<Vec<u8>> {
        let connection = self.connect().await?;
        let path = self.resolve_path(key)?;
        connection
            .sftp
            .read(&path)
            .await
            .map_err(|e| map_sftp_error(e, "SFTP object not found"))
    }

    #[instrument(skip(self), fields(key = %key.as_str()))]
    async fn delete(&self, key: &ObjectKey) -> AppResult<()> {
        let connection = self.connect().await?;
        let path = self.resolve_path(key)?;
        match connection.sftp.remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if is_not_found_error(&e) => Ok(()),
            Err(e) => Err(map_sftp_error(e, "failed to delete SFTP object")),
        }
    }

    #[instrument(skip(self), fields(prefix = %prefix.as_str()))]
    async fn list(&self, prefix: &ObjectKey) -> AppResult<Vec<ObjectKey>> {
        validate_object_key(prefix)?;
        let connection = self.connect().await?;
        let prefix_path = self.resolve_path(prefix)?;
        let search_dir =
            match connection.sftp.metadata(&prefix_path).await {
                Ok(metadata) if metadata.is_dir() => prefix_path,
                Ok(_) => parent_remote_path(&prefix_path)
                    .unwrap_or_else(|| self.remote_root_path.clone()),
                Err(e) if is_not_found_error(&e) => parent_remote_path(&prefix_path)
                    .unwrap_or_else(|| self.remote_root_path.clone()),
                Err(e) => return Err(map_sftp_error(e, "failed to inspect SFTP prefix")),
            };

        if !connection
            .sftp
            .try_exists(&search_dir)
            .await
            .unwrap_or(false)
        {
            return Ok(vec![]);
        }

        let mut results = Vec::new();
        collect_keys_recursive(
            &connection.sftp,
            &self.remote_root_path,
            &search_dir,
            prefix.as_str(),
            &mut results,
        )
        .await?;
        Ok(results)
    }

    #[instrument(skip(self), fields(key = %key.as_str()))]
    async fn exists(&self, key: &ObjectKey) -> AppResult<bool> {
        let connection = self.connect().await?;
        let path = self.resolve_path(key)?;
        connection
            .sftp
            .try_exists(&path)
            .await
            .map_err(|e| map_sftp_error(e, "failed to check SFTP object"))
    }
}

struct SftpConnection {
    #[allow(dead_code)]
    session: client::Handle<SftpClient>,
    sftp: SftpSession,
    host_key_fingerprint: String,
}

#[derive(Debug, Clone)]
struct SftpClient {
    expected_host_key: Option<String>,
    seen_host_key: Arc<Mutex<Option<String>>>,
}

impl client::Handler for SftpClient {
    type Error = SftpClientError;

    async fn check_server_key(
        &mut self,
        server_public_key: &ssh_key::PublicKey,
    ) -> Result<bool, Self::Error> {
        let fingerprint = host_key_fingerprint(server_public_key)?;
        if let Some(expected) = &self.expected_host_key {
            if expected != &fingerprint {
                return Ok(false);
            }
        }
        if let Ok(mut seen) = self.seen_host_key.lock() {
            *seen = Some(fingerprint);
        }
        Ok(true)
    }
}

#[derive(Debug, thiserror::Error)]
enum SftpClientError {
    #[error(transparent)]
    Russh(#[from] russh::Error),
    #[error("failed to encode SFTP host key: {0}")]
    HostKey(String),
}

impl From<SftpClientError> for AppError {
    fn from(error: SftpClientError) -> Self {
        match error {
            SftpClientError::Russh(e) => AppError::Network {
                message: e.to_string(),
                source: Some(Box::new(e)),
            },
            SftpClientError::HostKey(message) => AppError::Internal {
                message,
                source: None,
            },
        }
    }
}

async fn authenticate(
    session: &mut client::Handle<SftpClient>,
    creds: &SftpCredentials,
) -> AppResult<AuthResult> {
    match &creds.auth {
        SftpAuthConfig::Password { password } => session
            .authenticate_password(&creds.username, password)
            .await
            .map_err(|e| AppError::PermissionDenied {
                message: format!("SFTP authentication failed: {}", e),
                source: Some(Box::new(e)),
            }),
        SftpAuthConfig::PrivateKey {
            private_key_pem,
            passphrase,
        } => {
            let key = decode_secret_key(private_key_pem, passphrase.as_deref()).map_err(|e| {
                AppError::Validation {
                    message: format!("SFTP private key could not be read: {}", e),
                    source: Some(Box::new(e)),
                }
            })?;
            let key = PrivateKeyWithHashAlg::new(Arc::new(key), None);
            session
                .authenticate_publickey(&creds.username, key)
                .await
                .map_err(|e| AppError::PermissionDenied {
                    message: format!("SFTP authentication failed: {}", e),
                    source: Some(Box::new(e)),
                })
        }
    }
}

pub fn validate_sftp_config(creds: &SftpCredentials) -> AppResult<()> {
    if creds.host.trim().is_empty() {
        return Err(validation_error("SFTP server is required"));
    }
    if creds.port == 0 {
        return Err(validation_error("SFTP port must be between 1 and 65535"));
    }
    if creds.username.trim().is_empty() {
        return Err(validation_error("SFTP username is required"));
    }
    normalize_root_path(&creds.remote_root_path)?;
    match &creds.auth {
        SftpAuthConfig::Password { password } if password.is_empty() => {
            Err(validation_error("SFTP password is required"))
        }
        SftpAuthConfig::PrivateKey {
            private_key_pem, ..
        } if private_key_pem.trim().is_empty() => {
            Err(validation_error("SFTP private key is required"))
        }
        _ => Ok(()),
    }
}

pub fn normalize_root_path(path: &str) -> AppResult<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(validation_error("SFTP remote folder is required"));
    }
    if !trimmed.starts_with('/') {
        return Err(validation_error("SFTP remote folder must start with /"));
    }
    if trimmed == "/" {
        return Ok("/".to_string());
    }
    Ok(trimmed.trim_end_matches('/').to_string())
}

pub fn validate_object_key(key: &ObjectKey) -> AppResult<()> {
    let key = key.as_str();
    if key.is_empty() {
        return Err(validation_error("SFTP object key is required"));
    }
    if key.starts_with('/') || key.starts_with('\\') || key.contains('\\') {
        return Err(validation_error("SFTP object key must be relative"));
    }
    if key.split('/').any(|part| part == "..") {
        return Err(validation_error(
            "SFTP object key must not contain path traversal",
        ));
    }
    Ok(())
}

fn validation_error(message: &str) -> AppError {
    AppError::Validation {
        message: message.to_string(),
        source: None,
    }
}

fn host_key_fingerprint(public_key: &ssh_key::PublicKey) -> Result<String, SftpClientError> {
    let key_bytes = public_key
        .to_bytes()
        .map_err(|e| SftpClientError::HostKey(e.to_string()))?;
    let digest = Sha256::digest(&key_bytes);
    Ok(format!("SHA256:{}", BASE64_NOPAD.encode(digest.as_slice())))
}

fn join_remote_path(root: &str, child: &str) -> String {
    if root == "/" {
        format!("/{}", child.trim_start_matches('/'))
    } else {
        format!(
            "{}/{}",
            root.trim_end_matches('/'),
            child.trim_start_matches('/')
        )
    }
}

fn parent_remote_path(path: &str) -> Option<String> {
    if path == "/" {
        return None;
    }
    let trimmed = path.trim_end_matches('/');
    let index = trimmed.rfind('/')?;
    if index == 0 {
        Some("/".to_string())
    } else {
        Some(trimmed[..index].to_string())
    }
}

async fn create_dir_all(sftp: &SftpSession, path: &str) -> AppResult<()> {
    let normalized = normalize_root_path(path)?;
    if normalized == "/" || sftp.try_exists(&normalized).await.unwrap_or(false) {
        return Ok(());
    }
    let mut current = String::new();
    for segment in normalized.trim_start_matches('/').split('/') {
        if segment.is_empty() {
            continue;
        }
        current.push('/');
        current.push_str(segment);
        if sftp.try_exists(&current).await.unwrap_or(false) {
            continue;
        }
        match sftp.create_dir(&current).await {
            Ok(()) => {}
            Err(e) if is_already_exists_error(&e) => {}
            Err(e) => return Err(map_sftp_error(e, "failed to create SFTP directory")),
        }
    }
    Ok(())
}

async fn write_sftp_file(sftp: &SftpSession, path: &str, data: &[u8]) -> AppResult<()> {
    let mut file = sftp
        .open_with_flags(
            path,
            OpenFlags::CREATE | OpenFlags::TRUNCATE | OpenFlags::WRITE,
        )
        .await
        .map_err(|e| map_sftp_error(e, "failed to open SFTP file for writing"))?;
    file.write_all(data).await.map_err(|e| AppError::Network {
        message: format!("failed to write SFTP file: {}", e),
        source: Some(Box::new(e)),
    })?;
    file.flush().await.map_err(|e| AppError::Network {
        message: format!("failed to flush SFTP file: {}", e),
        source: Some(Box::new(e)),
    })?;
    file.shutdown().await.map_err(|e| AppError::Network {
        message: format!("failed to close SFTP file: {}", e),
        source: Some(Box::new(e)),
    })?;
    Ok(())
}

async fn collect_keys_recursive(
    sftp: &SftpSession,
    root: &str,
    dir: &str,
    prefix: &str,
    results: &mut Vec<ObjectKey>,
) -> AppResult<()> {
    let entries = match sftp.read_dir(dir).await {
        Ok(entries) => entries,
        Err(e) if is_not_found_error(&e) => return Ok(()),
        Err(e) => return Err(map_sftp_error(e, "failed to list SFTP directory")),
    };

    for entry in entries {
        let name = entry.file_name();
        if name == "." || name == ".." {
            continue;
        }
        let path = join_remote_path(dir, &name);
        let metadata = match sftp.metadata(&path).await {
            Ok(metadata) => metadata,
            Err(e) if is_not_found_error(&e) => continue,
            Err(e) => return Err(map_sftp_error(e, "failed to inspect SFTP path")),
        };
        if metadata.is_dir() {
            Box::pin(collect_keys_recursive(sftp, root, &path, prefix, results)).await?;
        } else if let Some(relative) = path.strip_prefix(root) {
            let key = relative.trim_start_matches('/').to_string();
            if key.starts_with(prefix) {
                results.push(ObjectKey::new(key));
            }
        }
    }
    Ok(())
}

fn random_suffix() -> String {
    Alphanumeric.sample_string(&mut rand::rng(), 16)
}

fn map_sftp_error(error: russh_sftp::client::error::Error, context: &str) -> AppError {
    let message = error.to_string();
    if is_not_found_error(&error) {
        AppError::NotFound {
            message: format!("{}: {}", context, message),
            source: Some(Box::new(error)),
        }
    } else if is_permission_error(&error) {
        AppError::PermissionDenied {
            message: format!("{}: {}", context, message),
            source: Some(Box::new(error)),
        }
    } else {
        AppError::Network {
            message: format!("{}: {}", context, message),
            source: Some(Box::new(error)),
        }
    }
}

fn is_not_found_error(error: &russh_sftp::client::error::Error) -> bool {
    let message = error.to_string().to_lowercase();
    message.contains("no such file")
        || message.contains("not found")
        || message.contains("no such path")
        || message.contains("does not exist")
}

fn is_permission_error(error: &russh_sftp::client::error::Error) -> bool {
    let message = error.to_string().to_lowercase();
    message.contains("permission denied") || message.contains("access denied")
}

fn is_already_exists_error(error: &russh_sftp::client::error::Error) -> bool {
    let message = error.to_string().to_lowercase();
    message.contains("already exists") || message.contains("file exists")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn password_creds() -> SftpCredentials {
        SftpCredentials {
            host: "localhost".to_string(),
            port: 22,
            username: "backup".to_string(),
            auth: SftpAuthConfig::Password {
                password: "secret".to_string(),
            },
            remote_root_path: "/backups/cloudless".to_string(),
            host_key_policy: api_types::remote_storage::SftpHostKeyPolicy::Strict,
            known_host_key: Some("SHA256:abc123".to_string()),
        }
    }

    #[test]
    fn normalizes_root_path() {
        assert_eq!(
            normalize_root_path("/backups/cloudless/").unwrap(),
            "/backups/cloudless"
        );
        assert_eq!(normalize_root_path("/").unwrap(), "/");
        assert!(normalize_root_path("").is_err());
        assert!(normalize_root_path("backups/cloudless").is_err());
    }

    #[test]
    fn validates_object_keys() {
        assert!(validate_object_key(&ObjectKey::new("chunks/abc".to_string())).is_ok());
        assert!(validate_object_key(&ObjectKey::new("../abc".to_string())).is_err());
        assert!(validate_object_key(&ObjectKey::new("chunks/../abc".to_string())).is_err());
        assert!(validate_object_key(&ObjectKey::new("/chunks/abc".to_string())).is_err());
        assert!(validate_object_key(&ObjectKey::new("chunks\\abc".to_string())).is_err());
    }

    #[test]
    fn validates_required_config_fields() {
        let mut creds = password_creds();
        assert!(validate_sftp_config(&creds).is_ok());

        creds.host = String::new();
        assert!(validate_sftp_config(&creds).is_err());
    }

    #[test]
    fn joins_remote_paths() {
        assert_eq!(join_remote_path("/", "chunks/abc"), "/chunks/abc");
        assert_eq!(
            join_remote_path("/backups/cloudless", "chunks/abc"),
            "/backups/cloudless/chunks/abc"
        );
    }
}
