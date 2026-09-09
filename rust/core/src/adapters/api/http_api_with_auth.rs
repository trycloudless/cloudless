use api_types::{
    auth::{LoginRequest, LoginResponse, RefreshRequest, Tokens},
    error::ApiError,
};
use base64::Engine as _;
use reqwest::RequestBuilder;
use serde::de::DeserializeOwned;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::ports::api::{ApiClientError, ApiResult};

#[derive(Clone)]
pub struct HttpApi {
    inner: Arc<RwLock<Option<Tokens>>>,
    base_url: reqwest::Url,
    pub client: reqwest::Client,
}

impl HttpApi {
    pub fn new(base_url: reqwest::Url) -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
            base_url,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_else(|_| reqwest::Client::new()),
        }
    }

    pub fn get_url(&self, uri: &str) -> ApiResult<reqwest::Url> {
        Ok(self
            .base_url
            .join(uri)
            .map_err(|e| ApiClientError::InvalidUrl(e.to_string()))?)
    }

    async fn get_access_token(&self) -> Option<String> {
        self.inner
            .read()
            .await
            .as_ref()
            .map(|t| t.access_token.clone())
    }

    async fn update_token(&self, tokens: Tokens) {
        *self.inner.write().await = Some(tokens);
    }

    /// Set tokens from external source (e.g. cookies in SSR website).
    pub async fn set_tokens(&self, access_token: String, refresh_token: String) {
        self.update_token(Tokens {
            access_token,
            refresh_token,
        })
        .await;
    }

    /// Creates a new `HttpApi` that shares the same HTTP client and base URL
    /// but has its own isolated token store. Use this in multi-user SSR contexts
    /// to avoid cross-request token leakage.
    pub fn with_tokens(&self, access_token: String, refresh_token: String) -> Self {
        Self {
            inner: Arc::new(RwLock::new(Some(Tokens {
                access_token,
                refresh_token,
            }))),
            base_url: self.base_url.clone(),
            client: self.client.clone(),
        }
    }

    /// Creates a new `HttpApi` sharing the same client/URL but with an empty
    /// (unauthenticated) token store.
    pub fn with_empty_tokens(&self) -> Self {
        Self {
            inner: Arc::new(RwLock::new(None)),
            base_url: self.base_url.clone(),
            client: self.client.clone(),
        }
    }

    async fn get_refresh_token(&self) -> Option<String> {
        self.inner
            .read()
            .await
            .as_ref()
            .map(|t| t.refresh_token.clone())
    }

    async fn refresh_access_token(&self) -> ApiResult<()> {
        let refresh_token = self
            .get_refresh_token()
            .await
            .ok_or_else(|| ApiClientError::InvalidUrl("Not authenticated".to_string()))?;

        let url = self.get_url("auth/refresh")?;
        let res = self
            .client
            .post(url)
            .json(&RefreshRequest { refresh_token })
            .send()
            .await?;

        if res.status().is_success() {
            let tokens = res.json::<Tokens>().await?;
            self.update_token(tokens).await;
            Ok(())
        } else {
            let api_err = res.json::<ApiError>().await?;
            Err(ApiClientError::Api(api_err))
        }
    }

    pub async fn login(&self, request: LoginRequest) -> ApiResult<LoginResponse> {
        let url = self.get_url("auth/login")?;
        let res = self.client.post(url).json(&request).send().await?;
        if res.status().is_success() {
            let login_response = res.json::<LoginResponse>().await?;
            self.update_token(Tokens {
                access_token: login_response.access_token.clone(),
                refresh_token: login_response.refresh_token.clone(),
            })
            .await;
            Ok(login_response)
        } else {
            let api_err = res.json::<ApiError>().await?;
            Err(crate::ports::api::ApiClientError::Api(api_err))
        }
    }

    #[tracing::instrument(skip(self, request))]
    pub async fn send<T: DeserializeOwned>(&self, request: RequestBuilder) -> ApiResult<T> {
        tracing::debug!("Sending request...");
        let res = request.send().await.map_err(|e| {
            tracing::error!("Request failed: {}", e);
            e
        })?;
        let status = res.status();
        tracing::debug!("Received response with status: {}", status);
        let body = res.text().await?;

        if status.is_success() {
            serde_json::from_str(&body).map_err(|e| {
                ApiClientError::InvalidUrl(format!(
                    "Failed to parse response: {}. Body: {}",
                    e, body
                ))
            })
        } else {
            let api_err: ApiError = serde_json::from_str(&body).map_err(|e| {
                ApiClientError::InvalidUrl(format!(
                    "Failed to parse error response: {}. Status: {}. Body: {}",
                    e, status, body
                ))
            })?;
            Err(ApiClientError::Api(api_err))
        }
    }

    /// Decodes the `exp` claim from a JWT without verifying the signature.
    /// Returns the expiry as a Unix timestamp in seconds, or `None` if unparseable.
    fn jwt_exp(token: &str) -> Option<u64> {
        let payload = token.split('.').nth(1)?;
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .decode(payload)
            .ok()?;
        let json: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
        json.get("exp")?.as_u64()
    }

    /// Refreshes the access token if it expires within the next 60 seconds.
    /// Called before every authenticated request so long-running operations
    /// (large file uploads, restores, multi-chunk backups) never hit a
    /// mid-operation 401 due to token expiry.
    async fn refresh_if_expiring_soon(&self) -> ApiResult<()> {
        let token = match self.get_access_token().await {
            Some(t) => t,
            None => return Ok(()),
        };
        if let Some(exp) = Self::jwt_exp(&token) {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            if exp <= now + 60 {
                tracing::debug!(
                    expires_in = exp.saturating_sub(now),
                    "Proactively refreshing token before it expires"
                );
                self.refresh_access_token().await?;
            }
        }
        Ok(())
    }

    pub async fn send_with_auth<T: DeserializeOwned>(
        &self,
        request: RequestBuilder,
    ) -> ApiResult<T> {
        self.refresh_if_expiring_soon().await?;

        let access_token = self
            .get_access_token()
            .await
            .ok_or_else(|| ApiClientError::InvalidUrl("Not authenticated".to_string()))?;

        // Clone the request for a potential retry after token refresh
        let retry_request = request.try_clone();

        let res = request.bearer_auth(&access_token).send().await?;
        if res.status().is_success() {
            return Self::parse_response(res).await;
        }

        // On 401, attempt token refresh and retry once
        if res.status() == reqwest::StatusCode::UNAUTHORIZED {
            if let Some(retry) = retry_request {
                tracing::debug!("Got 401, attempting token refresh");
                self.refresh_access_token().await?;
                let new_token = self
                    .get_access_token()
                    .await
                    .ok_or_else(|| ApiClientError::InvalidUrl("Not authenticated".to_string()))?;
                let res = retry.bearer_auth(&new_token).send().await?;
                if res.status().is_success() {
                    return Self::parse_response(res).await;
                }
                let api_err = res.json::<ApiError>().await?;
                return Err(ApiClientError::Api(api_err));
            }
        }

        let api_err = res.json::<ApiError>().await?;
        Err(ApiClientError::Api(api_err))
    }

    /// Parses a successful HTTP response body as JSON.
    ///
    /// Handles empty response bodies (e.g. from endpoints returning `()`) by
    /// deserializing from `"null"` instead of the empty string.
    async fn parse_response<T: DeserializeOwned>(res: reqwest::Response) -> ApiResult<T> {
        let body = res.bytes().await?;
        if body.is_empty() {
            return serde_json::from_str("null").map_err(|e| {
                ApiClientError::InvalidUrl(format!("Failed to parse empty response: {e}"))
            });
        }
        serde_json::from_slice(&body)
            .map_err(|e| ApiClientError::InvalidUrl(format!("Failed to parse response: {e}")))
    }
}
