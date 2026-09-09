use std::collections::HashMap;
use std::error::Error;

use api_types::remote_storage::GoogleDriveCredentials;
use async_trait::async_trait;
use serde::Deserialize;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::model::app_error::AppError;
use crate::model::base::AppResult;
use crate::model::file::ObjectKey;
use crate::ports::storage::StoragePort;

// ── Google Drive API base URLs ──────────────────────────────────────

const DRIVE_API: &str = "https://www.googleapis.com/drive/v3/files";
const DRIVE_UPLOAD_API: &str = "https://www.googleapis.com/upload/drive/v3/files";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";

// ── Internal error type ─────────────────────────────────────────────

#[derive(thiserror::Error, Debug)]
enum GDriveBackendError {
    #[error("Network error")]
    Network {
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    #[error("Not found")]
    NotFound {
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    #[error("Auth error (token expired or revoked)")]
    Auth {
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    #[error("User-facing Google Drive error")]
    UserFacing { message: String },

    #[error("Other Google Drive error")]
    Other {
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },
}

impl From<GDriveBackendError> for AppError {
    fn from(err: GDriveBackendError) -> Self {
        match err {
            GDriveBackendError::Network { source } => AppError::Network {
                message: "temporary Google Drive network failure".into(),
                source,
            },
            GDriveBackendError::NotFound { source } => AppError::NotFound {
                message: "Google Drive object not found".into(),
                source,
            },
            GDriveBackendError::Auth { source } => AppError::PermissionDenied {
                message: "Google Drive authentication failed (token expired or revoked)".into(),
                source,
            },
            GDriveBackendError::UserFacing { message } => AppError::Network {
                message,
                source: None,
            },
            GDriveBackendError::Other { source } => {
                let message = source
                    .as_ref()
                    .map(|s| format!("Google Drive error: {s}"))
                    .unwrap_or_else(|| "unexpected Google Drive error".into());
                AppError::Internal { message, source }
            }
        }
    }
}

impl From<reqwest::Error> for GDriveBackendError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_connect() || err.is_timeout() {
            GDriveBackendError::Network {
                source: Some(Box::new(err)),
            }
        } else if err.is_status() {
            match err.status() {
                Some(status) if status == reqwest::StatusCode::UNAUTHORIZED => {
                    GDriveBackendError::Auth {
                        source: Some(Box::new(err)),
                    }
                }
                Some(status) if status == reqwest::StatusCode::NOT_FOUND => {
                    GDriveBackendError::NotFound {
                        source: Some(Box::new(err)),
                    }
                }
                _ => GDriveBackendError::Other {
                    source: Some(Box::new(err)),
                },
            }
        } else {
            GDriveBackendError::Network {
                source: Some(Box::new(err)),
            }
        }
    }
}

// ── API response types ──────────────────────────────────────────────

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
struct FileListResponse {
    files: Option<Vec<FileResource>>,
}

#[derive(Deserialize)]
struct FileResource {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "webViewLink")]
    web_view_link: Option<String>,
}

#[derive(Clone, Copy)]
enum GoogleDriveStorageLayout {
    BackupChunks,
    RestoredFiles,
}

// ── Adaptor ─────────────────────────────────────────────────────────

/// Google Drive implementation of [`StoragePort`].
///
/// Chunks are stored under a `cloudless/{user_id}/` folder hierarchy inside the
/// user's configured root Drive folder. The `user_id` comes from the first
/// segment of the `ObjectKey` (format: `{user_id}/{hash}`). Folder IDs for
/// `cloudless` and each `{user_id}` subfolder are cached to avoid repeated
/// Drive API lookups. Token refresh is handled transparently on 401 responses.
pub struct GoogleDriveStorageAdaptor {
    client: reqwest::Client,
    access_token: RwLock<String>,
    client_id: String,
    /// Present only for credentials created before the Desktop-app migration.
    ///
    /// Web-application OAuth clients require the secret in every token refresh;
    /// Desktop-app clients (PKCE) do not. Kept so existing users' stored
    /// credentials continue to work until they re-authorise with the new client.
    client_secret: Option<String>,
    refresh_token: String,
    root_folder_id: String,
    /// Cached Drive folder ID for the `cloudless` folder under `root_folder_id`.
    cloudless_folder_id: RwLock<Option<String>>,
    /// Cached Drive folder IDs for `{user_id}` subfolders under `cloudless`.
    user_folder_ids: RwLock<HashMap<String, String>>,
    layout: GoogleDriveStorageLayout,
}

impl GoogleDriveStorageAdaptor {
    /// Creates a new adaptor by exchanging the refresh token for a fresh
    /// access token. The `storage_id` is used to identify the storage in
    /// `AuthTokenExpired` errors when the refresh token is invalid/revoked.
    pub async fn new(creds: GoogleDriveCredentials, storage_id: Uuid) -> AppResult<Self> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| AppError::Internal {
                message: "failed to build HTTP client".into(),
                source: Some(Box::new(e)),
            })?;

        let adaptor = Self {
            client,
            access_token: RwLock::new(String::new()),
            client_id: creds.client_id,
            client_secret: creds.client_secret,
            refresh_token: creds.refresh_token,
            root_folder_id: creds.root_folder_id,
            cloudless_folder_id: RwLock::new(None),
            user_folder_ids: RwLock::new(HashMap::new()),
            layout: GoogleDriveStorageLayout::BackupChunks,
        };

        // If the initial token refresh fails with an Auth error (invalid_grant),
        // return AuthTokenExpired so callers can mark the storage accordingly.
        if let Err(e) = adaptor.do_refresh_access_token().await {
            return match e {
                GDriveBackendError::Auth { .. } => {
                    tracing::warn!(
                        %storage_id,
                        "Google Drive refresh token expired or revoked — marking as AuthTokenExpired"
                    );
                    Err(AppError::AuthTokenExpired { storage_id })
                }
                other => Err(AppError::from(other)),
            };
        }

        Ok(adaptor)
    }

    /// Creates a Google Drive adaptor that interprets object keys as restored
    /// file paths under `cloudless-restored/{user_id}/`.
    pub async fn new_for_restored_files(
        creds: GoogleDriveCredentials,
        storage_id: Uuid,
    ) -> AppResult<Self> {
        let mut adaptor = Self::new(creds, storage_id).await?;
        adaptor.layout = GoogleDriveStorageLayout::RestoredFiles;
        Ok(adaptor)
    }

    /// Resolves a restored file key to the provider URL opened in Google Drive.
    ///
    /// Restore reports persist the CloudLess object key. This method maps that
    /// key to the restored folder/file and asks Drive for its current web link.
    pub async fn web_url_for_restored_file(&self, key: &ObjectKey) -> AppResult<String> {
        let (folder_id, file_name) = self
            .prepare_restored_file(key)
            .await
            .map_err(AppError::from)?;
        self.with_retry(|token| {
            let client = self.client.clone();
            let folder_id = folder_id.clone();
            let file_name = file_name.clone();

            async move {
                let file = Self::search_file(
                    &client,
                    &token,
                    &folder_id,
                    &file_name,
                    "files(id, webViewLink)",
                )
                .await?
                .ok_or(GDriveBackendError::NotFound { source: None })?;

                file.web_view_link.ok_or_else(|| GDriveBackendError::Other {
                    source: Some("Google Drive did not return a webViewLink".into()),
                })
            }
        })
        .await
        .map_err(AppError::from)
    }

    /// Exchanges the refresh token for a new access token.
    async fn do_refresh_access_token(&self) -> Result<(), GDriveBackendError> {
        tracing::debug!("Refreshing Google Drive OAuth access token");
        // Desktop-app clients (PKCE): no secret needed.
        // Legacy Web-app clients: include the stored secret so existing users'
        // tokens continue to refresh until they re-authorise with the new client.
        let mut form: Vec<(&str, &str)> = vec![
            ("client_id", self.client_id.as_str()),
            ("refresh_token", self.refresh_token.as_str()),
            ("grant_type", "refresh_token"),
        ];
        if let Some(ref secret) = self.client_secret {
            form.push(("client_secret", secret.as_str()));
        }
        let resp = self
            .client
            .post(TOKEN_ENDPOINT)
            .form(&form)
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::error!(
                status = %status,
                body = %body,
                "Google Drive OAuth token refresh failed"
            );
            return Err(
                if status == reqwest::StatusCode::UNAUTHORIZED
                    || status == reqwest::StatusCode::BAD_REQUEST
                {
                    GDriveBackendError::Auth {
                        source: Some(body.into()),
                    }
                } else {
                    google_drive_http_error(status, &body, "token refresh")
                },
            );
        }

        let token_resp: TokenResponse =
            resp.json().await.map_err(|e| GDriveBackendError::Other {
                source: Some(Box::new(e)),
            })?;

        tracing::info!("Google Drive OAuth access token refreshed successfully");
        *self.access_token.write().await = token_resp.access_token;
        Ok(())
    }

    /// Returns the current access token.
    async fn get_token(&self) -> String {
        self.access_token.read().await.clone()
    }

    /// Parses `{user_id}/{hash}` into `(user_id_str, hash_str)`.
    fn parse_key(key: &ObjectKey) -> Result<(&str, &str), GDriveBackendError> {
        key.as_str()
            .split_once('/')
            .ok_or_else(|| GDriveBackendError::Other {
                source: Some(
                    format!(
                        "invalid chunk key (expected {{user_id}}/{{hash}}): {}",
                        key.as_str()
                    )
                    .into(),
                ),
            })
    }

    /// Finds a Drive folder by name under the given parent. Returns the folder ID if found.
    async fn find_folder(
        client: &reqwest::Client,
        token: &str,
        parent_id: &str,
        name: &str,
    ) -> Result<Option<String>, GDriveBackendError> {
        let query = format!(
            "'{}' in parents and name = '{}' and mimeType = 'application/vnd.google-apps.folder' and trashed = false",
            parent_id, name
        );
        let resp = client
            .get(DRIVE_API)
            .bearer_auth(token)
            .query(&[("q", query.as_str()), ("fields", "files(id)")])
            .send()
            .await?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(GDriveBackendError::Auth { source: None });
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(google_drive_http_error(status, &body, "folder search"));
        }
        let list: FileListResponse = resp.json().await.map_err(|e| GDriveBackendError::Other {
            source: Some(Box::new(e)),
        })?;
        Ok(list.files.and_then(|f| f.into_iter().next()).map(|f| f.id))
    }

    /// Creates a Drive folder under the given parent. Returns the new folder ID.
    async fn create_folder(
        client: &reqwest::Client,
        token: &str,
        parent_id: &str,
        name: &str,
    ) -> Result<String, GDriveBackendError> {
        let metadata = serde_json::json!({
            "name": name,
            "mimeType": "application/vnd.google-apps.folder",
            "parents": [parent_id],
        });
        let resp = client
            .post(DRIVE_API)
            .bearer_auth(token)
            .json(&metadata)
            .send()
            .await?;
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(GDriveBackendError::Auth { source: None });
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(google_drive_http_error(status, &body, "folder create"));
        }
        let file: FileResource = resp.json().await.map_err(|e| GDriveBackendError::Other {
            source: Some(Box::new(e)),
        })?;
        Ok(file.id)
    }

    /// Returns the Drive folder ID for `cloudless/{user_id_str}` under `root_folder_id`,
    /// creating the `cloudless` and `{user_id_str}` folders on demand. Results are cached.
    async fn resolve_user_folder(
        &self,
        token: &str,
        user_id_str: &str,
    ) -> Result<String, GDriveBackendError> {
        // Resolve (and cache) the `cloudless` folder.
        let cloudless_id = {
            let cached = self.cloudless_folder_id.read().await.clone();
            if let Some(id) = cached {
                id
            } else {
                let id =
                    match Self::find_folder(&self.client, token, &self.root_folder_id, "cloudless")
                        .await?
                    {
                        Some(id) => id,
                        None => {
                            Self::create_folder(
                                &self.client,
                                token,
                                &self.root_folder_id,
                                "cloudless",
                            )
                            .await?
                        }
                    };
                *self.cloudless_folder_id.write().await = Some(id.clone());
                id
            }
        };

        // Resolve (and cache) the `{user_id_str}` folder under `cloudless`.
        let cached = self.user_folder_ids.read().await.get(user_id_str).cloned();
        if let Some(id) = cached {
            return Ok(id);
        }
        let id = match Self::find_folder(&self.client, token, &cloudless_id, user_id_str).await? {
            Some(id) => id,
            None => Self::create_folder(&self.client, token, &cloudless_id, user_id_str).await?,
        };
        self.user_folder_ids
            .write()
            .await
            .insert(user_id_str.to_string(), id.clone());
        Ok(id)
    }

    /// Searches for a file by exact name in the root folder via the Drive
    /// API. Returns the file resource if found.
    async fn search_file(
        client: &reqwest::Client,
        token: &str,
        root_id: &str,
        name: &str,
        fields: &str,
    ) -> Result<Option<FileResource>, GDriveBackendError> {
        let query = format!(
            "'{}' in parents and name = '{}' and trashed = false",
            root_id, name
        );

        let resp = client
            .get(DRIVE_API)
            .bearer_auth(token)
            .query(&[("q", query.as_str()), ("fields", fields)])
            .send()
            .await?;

        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(GDriveBackendError::Auth { source: None });
        }
        if !status.is_success() {
            let body = resp.text().await.unwrap_or_default();
            return Err(google_drive_http_error(status, &body, "file search"));
        }

        let list: FileListResponse = resp.json().await.map_err(|e| GDriveBackendError::Other {
            source: Some(Box::new(e)),
        })?;

        Ok(list.files.and_then(|f| f.into_iter().next()))
    }

    /// Executes an operation with automatic token refresh on 401.
    async fn with_retry<F, Fut, T>(&self, f: F) -> Result<T, GDriveBackendError>
    where
        F: Fn(String) -> Fut,
        Fut: std::future::Future<Output = Result<T, GDriveBackendError>>,
    {
        let token = self.get_token().await;
        match f(token).await {
            Ok(val) => Ok(val),
            Err(GDriveBackendError::Auth { .. }) => {
                tracing::warn!("Google Drive 401 — refreshing access token");
                self.do_refresh_access_token().await?;
                let new_token = self.get_token().await;
                f(new_token).await
            }
            Err(e) => Err(e),
        }
    }

    /// Parses the key and resolves (or creates) the `cloudless/{user_id}` Drive folder,
    /// refreshing the token once on 401. Returns `(folder_id, hash_str)`.
    async fn prepare_folder(
        &self,
        key: &ObjectKey,
    ) -> Result<(String, String), GDriveBackendError> {
        let (user_id_str, hash_str) = Self::parse_key(key)?;
        let (user_id_str, hash_str) = (user_id_str.to_owned(), hash_str.to_owned());

        let token = self.get_token().await;
        let folder_id = match self.resolve_user_folder(&token, &user_id_str).await {
            Ok(id) => id,
            Err(GDriveBackendError::Auth { .. }) => {
                tracing::warn!("Google Drive 401 resolving user folder — refreshing token");
                self.do_refresh_access_token().await?;
                let new_token = self.get_token().await;
                self.resolve_user_folder(&new_token, &user_id_str).await?
            }
            Err(e) => return Err(e),
        };
        Ok((folder_id, hash_str))
    }

    /// Resolves the final parent folder and file name for a restored-file key.
    async fn prepare_restored_file(
        &self,
        key: &ObjectKey,
    ) -> Result<(String, String), GDriveBackendError> {
        let parts: Vec<&str> = key.as_str().split('/').filter(|p| !p.is_empty()).collect();
        if parts.len() < 3 || parts[0] != "cloudless-restored" {
            return Err(GDriveBackendError::Other {
                source: Some(
                    format!(
                        "invalid restored file key (expected cloudless-restored/{{user_id}}/...): {}",
                        key.as_str()
                    )
                    .into(),
                ),
            });
        }

        let file_name = parts.last().unwrap().to_string();
        let folder_parts = &parts[..parts.len() - 1];
        let token = self.get_token().await;

        let folder_id = match self
            .resolve_restored_parent_folder(&token, folder_parts)
            .await
        {
            Ok(id) => id,
            Err(GDriveBackendError::Auth { .. }) => {
                tracing::warn!("Google Drive 401 resolving restored folder — refreshing token");
                self.do_refresh_access_token().await?;
                let new_token = self.get_token().await;
                self.resolve_restored_parent_folder(&new_token, folder_parts)
                    .await?
            }
            Err(e) => return Err(e),
        };

        Ok((folder_id, file_name))
    }

    /// Resolves or creates the folder chain for restored file output.
    async fn resolve_restored_parent_folder(
        &self,
        token: &str,
        folder_parts: &[&str],
    ) -> Result<String, GDriveBackendError> {
        let mut parent_id = self.root_folder_id.clone();
        for folder in folder_parts {
            parent_id = match Self::find_folder(&self.client, token, &parent_id, folder).await? {
                Some(id) => id,
                None => Self::create_folder(&self.client, token, &parent_id, folder).await?,
            };
        }
        Ok(parent_id)
    }
}

#[async_trait]
impl StoragePort for GoogleDriveStorageAdaptor {
    /// Uploads data as a file named after the hash portion of the key, stored
    /// under `cloudless/{user_id}/` inside the root Drive folder.
    async fn put(&self, key: &ObjectKey, data: Vec<u8>) -> AppResult<()> {
        let (folder_id, hash_str) = match self.layout {
            GoogleDriveStorageLayout::BackupChunks => {
                self.prepare_folder(key).await.map_err(AppError::from)?
            }
            GoogleDriveStorageLayout::RestoredFiles => self
                .prepare_restored_file(key)
                .await
                .map_err(AppError::from)?,
        };

        self.with_retry(|token| {
            let client = self.client.clone();
            let folder_id = folder_id.clone();
            let hash_str = hash_str.clone();
            let data = data.clone();

            async move {
                let existing =
                    Self::search_file(&client, &token, &folder_id, &hash_str, "files(id)").await?;

                if let Some(file) = existing {
                    let url = format!("{}/{}?uploadType=media", DRIVE_UPLOAD_API, file.id);
                    let resp = client
                        .patch(&url)
                        .bearer_auth(&token)
                        .header("Content-Type", "application/octet-stream")
                        .body(data)
                        .send()
                        .await?;
                    let status = resp.status();
                    if status == reqwest::StatusCode::UNAUTHORIZED {
                        return Err(GDriveBackendError::Auth { source: None });
                    }
                    if !status.is_success() {
                        let body = resp.text().await.unwrap_or_default();
                        return Err(google_drive_http_error(status, &body, "file update"));
                    }
                } else {
                    let metadata = serde_json::json!({
                        "name": hash_str,
                        "parents": [folder_id],
                    });
                    let metadata_part = reqwest::multipart::Part::text(metadata.to_string())
                        .mime_str("application/json")
                        .map_err(|e| GDriveBackendError::Other {
                            source: Some(Box::new(e)),
                        })?;
                    let file_part = reqwest::multipart::Part::bytes(data)
                        .mime_str("application/octet-stream")
                        .map_err(|e| GDriveBackendError::Other {
                            source: Some(Box::new(e)),
                        })?;
                    let form = reqwest::multipart::Form::new()
                        .part("metadata", metadata_part)
                        .part("file", file_part);
                    let url = format!("{}?uploadType=multipart", DRIVE_UPLOAD_API);
                    let resp = client
                        .post(&url)
                        .bearer_auth(&token)
                        .multipart(form)
                        .send()
                        .await?;
                    let status = resp.status();
                    if status == reqwest::StatusCode::UNAUTHORIZED {
                        return Err(GDriveBackendError::Auth { source: None });
                    }
                    if !status.is_success() {
                        let body = resp.text().await.unwrap_or_default();
                        return Err(google_drive_http_error(status, &body, "file create"));
                    }
                }
                Ok(())
            }
        })
        .await
        .map_err(AppError::from)
    }

    /// Downloads a chunk from `cloudless/{user_id}/{hash}` in the root Drive folder.
    async fn get(&self, key: &ObjectKey) -> AppResult<Vec<u8>> {
        let (folder_id, hash_str) = match self.layout {
            GoogleDriveStorageLayout::BackupChunks => {
                self.prepare_folder(key).await.map_err(AppError::from)?
            }
            GoogleDriveStorageLayout::RestoredFiles => self
                .prepare_restored_file(key)
                .await
                .map_err(AppError::from)?,
        };

        self.with_retry(|token| {
            let client = self.client.clone();
            let folder_id = folder_id.clone();
            let hash_str = hash_str.clone();

            async move {
                let file = Self::search_file(&client, &token, &folder_id, &hash_str, "files(id)")
                    .await?
                    .ok_or(GDriveBackendError::NotFound { source: None })?;

                let url = format!("{}/{}?alt=media", DRIVE_API, file.id);
                let resp = client.get(&url).bearer_auth(&token).send().await?;

                let status = resp.status();
                if status == reqwest::StatusCode::UNAUTHORIZED {
                    return Err(GDriveBackendError::Auth { source: None });
                }
                if status == reqwest::StatusCode::NOT_FOUND {
                    return Err(GDriveBackendError::NotFound { source: None });
                }
                if !status.is_success() {
                    let body = resp.text().await.unwrap_or_default();
                    return Err(google_drive_http_error(status, &body, "file download"));
                }

                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| GDriveBackendError::Network {
                        source: Some(Box::new(e)),
                    })?;
                Ok(bytes.to_vec())
            }
        })
        .await
        .map_err(AppError::from)
    }

    /// Deletes the chunk at `cloudless/{user_id}/{hash}`. No-op if not found.
    async fn delete(&self, key: &ObjectKey) -> AppResult<()> {
        let (folder_id, hash_str) = match self.layout {
            GoogleDriveStorageLayout::BackupChunks => {
                self.prepare_folder(key).await.map_err(AppError::from)?
            }
            GoogleDriveStorageLayout::RestoredFiles => self
                .prepare_restored_file(key)
                .await
                .map_err(AppError::from)?,
        };

        self.with_retry(|token| {
            let client = self.client.clone();
            let folder_id = folder_id.clone();
            let hash_str = hash_str.clone();

            async move {
                let file =
                    Self::search_file(&client, &token, &folder_id, &hash_str, "files(id)").await?;

                if let Some(file) = file {
                    let url = format!("{}/{}", DRIVE_API, file.id);
                    let resp = client.delete(&url).bearer_auth(&token).send().await?;

                    let status = resp.status();
                    if status == reqwest::StatusCode::UNAUTHORIZED {
                        return Err(GDriveBackendError::Auth { source: None });
                    }
                    if !status.is_success() && status != reqwest::StatusCode::NOT_FOUND {
                        let body = resp.text().await.unwrap_or_default();
                        return Err(google_drive_http_error(status, &body, "file delete"));
                    }
                }
                Ok(())
            }
        })
        .await
        .map_err(AppError::from)
    }

    /// Lists chunk keys under `cloudless/{user_id}/`. The prefix must be `{user_id}/`
    /// (or empty to list all chunks under the `cloudless` folder is not supported;
    /// pass a `{user_id}/` prefix instead).
    async fn list(&self, prefix: &ObjectKey) -> AppResult<Vec<ObjectKey>> {
        let (folder_id, _) = self.prepare_folder(prefix).await.map_err(AppError::from)?;

        self.with_retry(|token| {
            let client = self.client.clone();
            let folder_id = folder_id.clone();

            async move {
                let mut all_files = Vec::new();
                let mut page_token: Option<String> = None;

                loop {
                    let query = format!(
                        "'{}' in parents and mimeType != 'application/vnd.google-apps.folder' and trashed = false",
                        folder_id
                    );

                    let mut req = client.get(DRIVE_API).bearer_auth(&token).query(&[
                        ("q", query.as_str()),
                        ("fields", "nextPageToken,files(id,name)"),
                        ("pageSize", "1000"),
                    ]);

                    if let Some(ref pt) = page_token {
                        req = req.query(&[("pageToken", pt.as_str())]);
                    }

                    let resp = req.send().await?;

                    let status = resp.status();
                    if status == reqwest::StatusCode::UNAUTHORIZED {
                        return Err(GDriveBackendError::Auth { source: None });
                    }
                    if !status.is_success() {
                        let body = resp.text().await.unwrap_or_default();
                        return Err(google_drive_http_error(status, &body, "file list"));
                    }

                    #[derive(Deserialize)]
                    struct PagedResponse {
                        files: Option<Vec<FileResource>>,
                        #[serde(rename = "nextPageToken")]
                        next_page_token: Option<String>,
                    }

                    let paged: PagedResponse =
                        resp.json().await.map_err(|e| GDriveBackendError::Other {
                            source: Some(Box::new(e)),
                        })?;

                    if let Some(files) = paged.files {
                        for f in files {
                            all_files.push(ObjectKey::new(f.name));
                        }
                    }

                    match paged.next_page_token {
                        Some(pt) if !pt.is_empty() => page_token = Some(pt),
                        _ => break,
                    }
                }

                Ok(all_files)
            }
        })
        .await
        .map_err(AppError::from)
    }

    /// Returns whether a chunk exists at `cloudless/{user_id}/{hash}`.
    async fn exists(&self, key: &ObjectKey) -> AppResult<bool> {
        let (folder_id, hash_str) = match self.layout {
            GoogleDriveStorageLayout::BackupChunks => {
                self.prepare_folder(key).await.map_err(AppError::from)?
            }
            GoogleDriveStorageLayout::RestoredFiles => self
                .prepare_restored_file(key)
                .await
                .map_err(AppError::from)?,
        };

        self.with_retry(|token| {
            let client = self.client.clone();
            let folder_id = folder_id.clone();
            let hash_str = hash_str.clone();

            async move {
                let file =
                    Self::search_file(&client, &token, &folder_id, &hash_str, "files(id)").await?;
                Ok(file.is_some())
            }
        })
        .await
        .map_err(AppError::from)
    }
}

/// Converts Google Drive HTTP failures into short messages safe for reports.
fn google_drive_http_error(
    status: reqwest::StatusCode,
    body: &str,
    context: &str,
) -> GDriveBackendError {
    tracing::warn!(
        status = %status,
        context = %context,
        body = %body,
        "Google Drive request failed"
    );
    let message = readable_google_drive_error(status, body, context);
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
        GDriveBackendError::UserFacing { message }
    } else {
        GDriveBackendError::Other {
            source: Some(message.into()),
        }
    }
}

/// Produces a concise Google Drive error without raw HTML or long JSON bodies.
fn readable_google_drive_error(status: reqwest::StatusCode, body: &str, context: &str) -> String {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return "Google Drive is rate limiting requests. Try again in a few minutes.".to_string();
    }
    if status.is_server_error() {
        return "Google Drive is temporarily unavailable. Try again in a few minutes.".to_string();
    }

    let detail = sanitized_google_drive_error_detail(body);
    if detail.is_empty() {
        format!("Google Drive {context} failed ({status}).")
    } else {
        format!("Google Drive {context} failed ({status}): {detail}")
    }
}

/// Removes raw HTML and caps provider error details before they reach users.
fn sanitized_google_drive_error_detail(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('<')
        || trimmed.to_ascii_lowercase().contains("<html")
    {
        return String::new();
    }
    const MAX_DETAIL_CHARS: usize = 240;
    let mut detail: String = trimmed.chars().take(MAX_DETAIL_CHARS).collect();
    if trimmed.chars().count() > MAX_DETAIL_CHARS {
        detail.push_str("...");
    }
    detail
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ensures transient Google HTML error pages are converted into a short
    /// retryable message suitable for restore reports.
    #[test]
    fn readable_google_drive_error_hides_html_for_service_unavailable() {
        let message = readable_google_drive_error(
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            "<!DOCTYPE html><html><body>Something went wrong</body></html>",
            "file download",
        );

        assert_eq!(
            message,
            "Google Drive is temporarily unavailable. Try again in a few minutes."
        );
    }

    /// Ensures non-HTML Google Drive provider errors are capped before display.
    #[test]
    fn sanitized_google_drive_error_detail_caps_long_non_html_body() {
        let detail = sanitized_google_drive_error_detail(&"x".repeat(300));

        assert_eq!(detail.len(), 243);
        assert!(detail.ends_with("..."));
    }
}
