use std::error::Error;

use api_types::remote_storage::OneDriveCredentials;
use async_trait::async_trait;
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde::Deserialize;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::model::app_error::AppError;
use crate::model::base::AppResult;
use crate::model::file::ObjectKey;
use crate::ports::storage::StoragePort;

const GRAPH_API: &str = "https://graph.microsoft.com/v1.0";
const TOKEN_BASE: &str = "https://login.microsoftonline.com";
const ONEDRIVE_SCOPE: &str = "openid profile email offline_access Files.ReadWrite.AppFolder";
const PATH_SEGMENT_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'/')
    .add(b'\\')
    .add(b':');

#[derive(thiserror::Error, Debug)]
enum OneDriveBackendError {
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

    #[error("Auth error")]
    Auth {
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },

    #[error("User-facing Microsoft OneDrive error")]
    UserFacing { message: String },

    #[error("Other Microsoft OneDrive error")]
    Other {
        #[source]
        source: Option<Box<dyn Error + Send + Sync>>,
    },
}

impl From<OneDriveBackendError> for AppError {
    fn from(err: OneDriveBackendError) -> Self {
        match err {
            OneDriveBackendError::Network { source } => AppError::Network {
                message: "temporary Microsoft OneDrive network failure".into(),
                source,
            },
            OneDriveBackendError::NotFound { source } => AppError::NotFound {
                message: "Microsoft OneDrive object not found".into(),
                source,
            },
            OneDriveBackendError::Auth { source } => AppError::PermissionDenied {
                message: "Microsoft OneDrive authentication failed".into(),
                source,
            },
            OneDriveBackendError::UserFacing { message } => AppError::Network {
                message,
                source: None,
            },
            OneDriveBackendError::Other { source } => {
                let message = source
                    .as_ref()
                    .map(|s| format!("Microsoft OneDrive error: {s}"))
                    .unwrap_or_else(|| "unexpected Microsoft OneDrive error".into());
                AppError::Internal { message, source }
            }
        }
    }
}

impl From<reqwest::Error> for OneDriveBackendError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_connect() || err.is_timeout() {
            OneDriveBackendError::Network {
                source: Some(Box::new(err)),
            }
        } else {
            OneDriveBackendError::Network {
                source: Some(Box::new(err)),
            }
        }
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct DriveChildrenResponse {
    #[serde(default)]
    value: Vec<DriveItem>,
    #[serde(rename = "@odata.nextLink")]
    next_link: Option<String>,
}

#[derive(Deserialize)]
struct DriveItem {
    #[serde(default)]
    name: String,
    #[serde(default, rename = "webUrl")]
    web_url: Option<String>,
}

/// Microsoft OneDrive implementation of [`StoragePort`].
///
/// Stores encrypted chunks under `cloudless/{user_id}/{hash}` inside the
/// Microsoft Graph app folder (`/me/drive/special/approot`) so CloudLess only
/// receives app-folder access, never full-drive access.
pub struct OneDriveStorageAdaptor {
    client: reqwest::Client,
    access_token: RwLock<String>,
    refresh_token: RwLock<String>,
    client_id: String,
    tenant: String,
    graph_api: String,
    token_base: String,
    layout: OneDriveStorageLayout,
}

#[derive(Clone, Copy)]
enum OneDriveStorageLayout {
    BackupChunks,
    RestoredFiles,
}

impl OneDriveStorageAdaptor {
    /// Creates a OneDrive adapter and immediately refreshes an access token.
    ///
    /// Early token refresh fails fast when the user revoked CloudLess access,
    /// letting callers mark the remote storage as needing reauthorization.
    pub async fn new(creds: OneDriveCredentials, storage_id: Uuid) -> AppResult<Self> {
        Self::new_with_base_urls(
            creds,
            storage_id,
            GRAPH_API.to_string(),
            TOKEN_BASE.to_string(),
        )
        .await
    }

    /// Creates a OneDrive adapter using explicit API bases for production or tests.
    async fn new_with_base_urls(
        creds: OneDriveCredentials,
        storage_id: Uuid,
        graph_api: String,
        token_base: String,
    ) -> AppResult<Self> {
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
            refresh_token: RwLock::new(creds.refresh_token),
            client_id: creds.client_id,
            tenant: normalize_tenant(&creds.tenant),
            graph_api: graph_api.trim_end_matches('/').to_string(),
            token_base: token_base.trim_end_matches('/').to_string(),
            layout: OneDriveStorageLayout::BackupChunks,
        };

        if let Err(e) = adaptor.do_refresh_access_token().await {
            return match e {
                OneDriveBackendError::Auth { .. } => {
                    tracing::warn!(
                        %storage_id,
                        "Microsoft OneDrive refresh token expired or revoked"
                    );
                    Err(AppError::AuthTokenExpired { storage_id })
                }
                other => Err(AppError::from(other)),
            };
        }

        Ok(adaptor)
    }

    /// Creates a OneDrive adapter that interprets object keys as restored file
    /// paths under `cloudless-restored/{user_id}/`.
    pub async fn new_for_restored_files(
        creds: OneDriveCredentials,
        storage_id: Uuid,
    ) -> AppResult<Self> {
        let mut adaptor = Self::new(creds, storage_id).await?;
        adaptor.layout = OneDriveStorageLayout::RestoredFiles;
        Ok(adaptor)
    }

    /// Resolves a restored file key to the provider URL opened in OneDrive.
    ///
    /// Restore reports persist object keys, not provider-specific file IDs, so
    /// this performs a fresh metadata lookup and lets Microsoft Graph return the
    /// current web URL for that restored file.
    pub async fn web_url_for_restored_file(&self, key: &ObjectKey) -> AppResult<String> {
        let url = self.metadata_url(key).map_err(AppError::from)?;
        self.with_retry(|token| {
            let client = self.client.clone();
            let url = url.clone();

            async move {
                let resp = client
                    .get(&url)
                    .bearer_auth(token)
                    .query(&[("$select", "webUrl")])
                    .send()
                    .await?;
                let status = resp.status();
                if status.is_success() {
                    let item: DriveItem =
                        resp.json().await.map_err(|e| OneDriveBackendError::Other {
                            source: Some(Box::new(e)),
                        })?;
                    return item.web_url.ok_or_else(|| OneDriveBackendError::Other {
                        source: Some("OneDrive did not return a webUrl".into()),
                    });
                }
                Err(Self::graph_error(status, resp, "file metadata lookup").await)
            }
        })
        .await
        .map_err(AppError::from)
    }

    /// Exchanges the refresh token for a short-lived access token.
    async fn do_refresh_access_token(&self) -> Result<(), OneDriveBackendError> {
        tracing::debug!("Refreshing Microsoft OneDrive OAuth access token");
        let refresh_token = self.refresh_token.read().await.clone();
        let token_url = format!(
            "{}/{}/oauth2/v2.0/token",
            self.token_base,
            encode_path_segment(&self.tenant)
        );
        let resp = self
            .client
            .post(token_url)
            .form(&[
                ("client_id", self.client_id.as_str()),
                ("refresh_token", refresh_token.as_str()),
                ("grant_type", "refresh_token"),
                ("scope", ONEDRIVE_SCOPE),
            ])
            .send()
            .await?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            tracing::error!(
                status = %status,
                body = %body,
                "Microsoft OneDrive OAuth token refresh failed"
            );
            return Err(
                if status == reqwest::StatusCode::UNAUTHORIZED
                    || status == reqwest::StatusCode::BAD_REQUEST
                {
                    OneDriveBackendError::Auth {
                        source: Some(body.into()),
                    }
                } else {
                    let message = readable_graph_error(status, &body, "token refresh");
                    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
                    {
                        OneDriveBackendError::UserFacing { message }
                    } else {
                        OneDriveBackendError::Other {
                            source: Some(message.into()),
                        }
                    }
                },
            );
        }

        let token_resp: TokenResponse =
            resp.json().await.map_err(|e| OneDriveBackendError::Other {
                source: Some(Box::new(e)),
            })?;

        *self.access_token.write().await = token_resp.access_token;
        if let Some(next_refresh_token) = token_resp.refresh_token {
            *self.refresh_token.write().await = next_refresh_token;
        }
        tracing::info!("Microsoft OneDrive OAuth access token refreshed successfully");
        Ok(())
    }

    /// Returns the current bearer token.
    async fn get_token(&self) -> String {
        self.access_token.read().await.clone()
    }

    /// Executes an operation with one token-refresh retry on auth failure.
    async fn with_retry<F, Fut, T>(&self, f: F) -> Result<T, OneDriveBackendError>
    where
        F: Fn(String) -> Fut,
        Fut: std::future::Future<Output = Result<T, OneDriveBackendError>>,
    {
        let token = self.get_token().await;
        match f(token).await {
            Ok(value) => Ok(value),
            Err(OneDriveBackendError::Auth { .. }) => {
                tracing::warn!("Microsoft OneDrive 401 — refreshing access token");
                self.do_refresh_access_token().await?;
                let new_token = self.get_token().await;
                f(new_token).await
            }
            Err(e) => Err(e),
        }
    }

    /// Builds the Graph metadata URL for a CloudLess object key.
    fn metadata_url(&self, key: &ObjectKey) -> Result<String, OneDriveBackendError> {
        let path = match self.layout {
            OneDriveStorageLayout::BackupChunks => graph_cloudless_path(key)?,
            OneDriveStorageLayout::RestoredFiles => graph_restored_path(key)?,
        };
        Ok(format!(
            "{}/me/drive/special/approot:/{}",
            self.graph_api, path
        ))
    }

    /// Builds the Graph content URL for a CloudLess object key.
    fn content_url(&self, key: &ObjectKey) -> Result<String, OneDriveBackendError> {
        Ok(format!("{}:/content", self.metadata_url(key)?))
    }

    /// Builds the Graph children URL for a `{user_id}/` prefix.
    fn children_url(&self, prefix: &ObjectKey) -> Result<(String, String), OneDriveBackendError> {
        let (user_id, _) = parse_object_key(prefix)?;
        let user_path = format!("cloudless/{}", encode_path_segment(user_id));
        Ok((
            format!(
                "{}/me/drive/special/approot:/{}:/children",
                self.graph_api, user_path
            ),
            user_id.to_string(),
        ))
    }

    /// Maps a non-success Graph response to a typed adapter error.
    async fn graph_error(
        status: reqwest::StatusCode,
        resp: reqwest::Response,
        context: &str,
    ) -> OneDriveBackendError {
        if status == reqwest::StatusCode::UNAUTHORIZED {
            return OneDriveBackendError::Auth { source: None };
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return OneDriveBackendError::NotFound { source: None };
        }
        let body = resp.text().await.unwrap_or_default();
        tracing::warn!(
            status = %status,
            context = %context,
            body = %body,
            "Microsoft OneDrive request failed"
        );
        let message = readable_graph_error(status, &body, context);
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error() {
            return OneDriveBackendError::UserFacing { message };
        }
        OneDriveBackendError::Other {
            source: Some(message.into()),
        }
    }

    /// Returns `true` when the stored token can still access OneDrive app root.
    pub async fn test_connection(creds: OneDriveCredentials, storage_id: Uuid) -> AppResult<bool> {
        let adaptor = Self::new(creds, storage_id).await?;
        adaptor
            .with_retry(|token| {
                let client = adaptor.client.clone();
                let url = format!("{}/me/drive/special/approot", adaptor.graph_api);

                async move {
                    let resp = client.get(url).bearer_auth(token).send().await?;
                    let status = resp.status();
                    if status.is_success() {
                        return Ok(true);
                    }
                    Err(Self::graph_error(status, resp, "app folder lookup").await)
                }
            })
            .await
            .map_err(AppError::from)
    }
}

#[async_trait]
impl StoragePort for OneDriveStorageAdaptor {
    /// Uploads encrypted chunk bytes into the OneDrive app folder.
    async fn put(&self, key: &ObjectKey, data: Vec<u8>) -> AppResult<()> {
        let url = self.content_url(key).map_err(AppError::from)?;
        self.with_retry(|token| {
            let client = self.client.clone();
            let url = url.clone();
            let data = data.clone();

            async move {
                let resp = client
                    .put(url)
                    .bearer_auth(token)
                    .header("Content-Type", "application/octet-stream")
                    .body(data)
                    .send()
                    .await?;
                let status = resp.status();
                if status.is_success() {
                    return Ok(());
                }
                Err(Self::graph_error(status, resp, "file upload").await)
            }
        })
        .await
        .map_err(AppError::from)
    }

    /// Downloads encrypted chunk bytes from the OneDrive app folder.
    async fn get(&self, key: &ObjectKey) -> AppResult<Vec<u8>> {
        let url = self.content_url(key).map_err(AppError::from)?;
        self.with_retry(|token| {
            let client = self.client.clone();
            let url = url.clone();

            async move {
                let resp = client.get(url).bearer_auth(token).send().await?;
                let status = resp.status();
                if status.is_success() {
                    let bytes = resp
                        .bytes()
                        .await
                        .map_err(|e| OneDriveBackendError::Network {
                            source: Some(Box::new(e)),
                        })?;
                    return Ok(bytes.to_vec());
                }
                Err(Self::graph_error(status, resp, "file download").await)
            }
        })
        .await
        .map_err(AppError::from)
    }

    /// Deletes a chunk from OneDrive, treating missing objects as already deleted.
    async fn delete(&self, key: &ObjectKey) -> AppResult<()> {
        let url = self.metadata_url(key).map_err(AppError::from)?;
        self.with_retry(|token| {
            let client = self.client.clone();
            let url = url.clone();

            async move {
                let resp = client.delete(url).bearer_auth(token).send().await?;
                let status = resp.status();
                if status.is_success() || status == reqwest::StatusCode::NOT_FOUND {
                    return Ok(());
                }
                Err(Self::graph_error(status, resp, "file delete").await)
            }
        })
        .await
        .map_err(AppError::from)
    }

    /// Lists chunks under the user-prefixed folder for adapter parity.
    async fn list(&self, prefix: &ObjectKey) -> AppResult<Vec<ObjectKey>> {
        let (url, user_id) = self.children_url(prefix).map_err(AppError::from)?;
        self.with_retry(|token| {
            let client = self.client.clone();
            let initial_url = url.clone();
            let user_id = user_id.clone();

            async move {
                let mut out = Vec::new();
                let mut next_url = Some(initial_url);

                while let Some(url) = next_url {
                    let resp = client.get(url).bearer_auth(&token).send().await?;
                    let status = resp.status();
                    if status == reqwest::StatusCode::NOT_FOUND {
                        return Ok(out);
                    }
                    if !status.is_success() {
                        return Err(Self::graph_error(status, resp, "file list").await);
                    }

                    let page: DriveChildrenResponse =
                        resp.json().await.map_err(|e| OneDriveBackendError::Other {
                            source: Some(Box::new(e)),
                        })?;
                    for item in page.value {
                        out.push(ObjectKey::new(format!("{}/{}", user_id, item.name)));
                    }
                    next_url = page.next_link;
                }

                Ok(out)
            }
        })
        .await
        .map_err(AppError::from)
    }

    /// Checks whether a chunk exists in the OneDrive app folder.
    async fn exists(&self, key: &ObjectKey) -> AppResult<bool> {
        let url = self.metadata_url(key).map_err(AppError::from)?;
        self.with_retry(|token| {
            let client = self.client.clone();
            let url = url.clone();

            async move {
                let resp = client.get(url).bearer_auth(token).send().await?;
                let status = resp.status();
                if status.is_success() {
                    return Ok(true);
                }
                if status == reqwest::StatusCode::NOT_FOUND {
                    return Ok(false);
                }
                Err(Self::graph_error(status, resp, "file metadata lookup").await)
            }
        })
        .await
        .map_err(AppError::from)
    }
}

/// Normalizes an empty tenant to `common` so public Microsoft accounts work.
fn normalize_tenant(tenant: &str) -> String {
    let trimmed = tenant.trim();
    if trimmed.is_empty() {
        "common".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Percent-encodes a path segment for Microsoft Graph path syntax.
fn encode_path_segment(segment: &str) -> String {
    utf8_percent_encode(segment, PATH_SEGMENT_ENCODE_SET).to_string()
}

/// Parses and validates the CloudLess chunk object key.
fn parse_object_key(key: &ObjectKey) -> Result<(&str, &str), OneDriveBackendError> {
    let raw = key.as_str();
    let (user_id, hash) = raw
        .split_once('/')
        .ok_or_else(|| invalid_key_error(raw, "expected {user_id}/{hash}"))?;
    if user_id.is_empty() || hash.is_empty() {
        return Err(invalid_key_error(raw, "empty path segment"));
    }
    if hash.contains('/') {
        return Err(invalid_key_error(raw, "too many path segments"));
    }
    validate_segment(raw, user_id)?;
    validate_segment(raw, hash)?;
    Ok((user_id, hash))
}

/// Builds the app-folder relative path for a validated CloudLess object key.
fn graph_cloudless_path(key: &ObjectKey) -> Result<String, OneDriveBackendError> {
    let (user_id, hash) = parse_object_key(key)?;
    Ok(format!(
        "cloudless/{}/{}",
        encode_path_segment(user_id),
        encode_path_segment(hash)
    ))
}

/// Builds the app-folder relative path for a restored plaintext output key.
fn graph_restored_path(key: &ObjectKey) -> Result<String, OneDriveBackendError> {
    let raw = key.as_str();
    let parts: Vec<&str> = raw.split('/').collect();
    if parts.len() < 3 || parts[0] != "cloudless-restored" {
        return Err(invalid_key_error(
            raw,
            "expected cloudless-restored/{user_id}/path",
        ));
    }
    for segment in &parts {
        if segment.is_empty() {
            return Err(invalid_key_error(raw, "empty path segment"));
        }
        validate_segment(raw, segment)?;
    }
    Ok(parts
        .iter()
        .map(|segment| encode_path_segment(segment))
        .collect::<Vec<_>>()
        .join("/"))
}

/// Rejects path traversal and platform separators before URL construction.
fn validate_segment(raw_key: &str, segment: &str) -> Result<(), OneDriveBackendError> {
    if segment == "." || segment == ".." || segment.contains('\\') || segment.contains('\0') {
        return Err(invalid_key_error(raw_key, "unsafe path segment"));
    }
    Ok(())
}

/// Creates a typed invalid-key error without leaking into storage calls.
fn invalid_key_error(raw_key: &str, reason: &str) -> OneDriveBackendError {
    OneDriveBackendError::Other {
        source: Some(format!("invalid OneDrive object key `{raw_key}`: {reason}").into()),
    }
}

/// Converts Microsoft Graph HTTP failures into short messages safe for reports.
fn readable_graph_error(status: reqwest::StatusCode, body: &str, context: &str) -> String {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return "Microsoft OneDrive is rate limiting requests. Try again in a few minutes."
            .to_string();
    }
    if status.is_server_error() {
        return "Microsoft OneDrive is temporarily unavailable. Try again in a few minutes."
            .to_string();
    }

    let detail = sanitized_error_detail(body);
    if detail.is_empty() {
        format!("Microsoft OneDrive {context} failed ({status}).")
    } else {
        format!("Microsoft OneDrive {context} failed ({status}): {detail}")
    }
}

/// Removes raw HTML and caps provider error details before they reach users.
fn sanitized_error_detail(body: &str) -> String {
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

    /// Builds an adapter without token refresh so URL helpers can be unit tested.
    fn test_adaptor() -> OneDriveStorageAdaptor {
        OneDriveStorageAdaptor {
            client: reqwest::Client::new(),
            access_token: RwLock::new("token".to_string()),
            refresh_token: RwLock::new("refresh".to_string()),
            client_id: "client-id".to_string(),
            tenant: "common".to_string(),
            graph_api: "https://graph.example.test/v1.0".to_string(),
            token_base: "https://login.example.test".to_string(),
            layout: OneDriveStorageLayout::BackupChunks,
        }
    }

    /// Ensures app-folder object paths preserve the CloudLess key layout.
    #[test]
    fn graph_cloudless_path_encodes_each_segment() {
        let key = ObjectKey::new("user id/hash:value".to_string());

        let path = graph_cloudless_path(&key).unwrap();

        assert_eq!(path, "cloudless/user%20id/hash%3Avalue");
    }

    /// Ensures traversal-like keys are rejected before any Graph request is made.
    #[test]
    fn parse_object_key_rejects_unsafe_segments() {
        for raw in [
            "user/../hash",
            "../hash",
            "user/hash/extra",
            "/hash",
            "user/",
        ] {
            let key = ObjectKey::new(raw.to_string());
            assert!(parse_object_key(&key).is_err(), "{raw} should be rejected");
        }
    }

    /// Ensures empty tenant config still targets Microsoft's public `common` authority.
    #[test]
    fn normalize_tenant_defaults_to_common() {
        assert_eq!(normalize_tenant(""), "common");
        assert_eq!(normalize_tenant("  "), "common");
        assert_eq!(normalize_tenant("organizations"), "organizations");
    }

    /// Ensures metadata and content URLs use Microsoft Graph app-folder syntax.
    #[test]
    fn adaptor_builds_app_folder_urls() {
        let adaptor = test_adaptor();
        let key = ObjectKey::new("user/hash".to_string());

        assert_eq!(
            adaptor.metadata_url(&key).unwrap(),
            "https://graph.example.test/v1.0/me/drive/special/approot:/cloudless/user/hash"
        );
        assert_eq!(
            adaptor.content_url(&key).unwrap(),
            "https://graph.example.test/v1.0/me/drive/special/approot:/cloudless/user/hash:/content"
        );
    }

    /// Ensures listing stays scoped to a single user folder under app root.
    #[test]
    fn adaptor_builds_children_url_for_user_prefix() {
        let adaptor = test_adaptor();
        let prefix = ObjectKey::new("user/anything".to_string());

        let (url, user_id) = adaptor.children_url(&prefix).unwrap();

        assert_eq!(user_id, "user");
        assert_eq!(
            url,
            "https://graph.example.test/v1.0/me/drive/special/approot:/cloudless/user:/children"
        );
    }

    /// Ensures transient Microsoft HTML error pages are converted into a short
    /// retryable message suitable for restore reports.
    #[test]
    fn readable_graph_error_hides_html_for_service_unavailable() {
        let message = readable_graph_error(
            reqwest::StatusCode::SERVICE_UNAVAILABLE,
            "<!DOCTYPE html><html><body>Something went wrong</body></html>",
            "file download",
        );

        assert_eq!(
            message,
            "Microsoft OneDrive is temporarily unavailable. Try again in a few minutes."
        );
    }

    /// Ensures non-HTML provider errors are capped so reports do not become
    /// unreadable when the provider returns a long JSON/string body.
    #[test]
    fn sanitized_error_detail_caps_long_non_html_body() {
        let detail = sanitized_error_detail(&"x".repeat(300));

        assert_eq!(detail.len(), 243);
        assert!(detail.ends_with("..."));
    }
}
