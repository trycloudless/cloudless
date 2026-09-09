use serde::Deserialize;

/// Central configuration for the API server.
///
/// All fields are populated from environment variables (or a `.env` file).
/// Each field has a sensible default for local development.
#[derive(Debug, Deserialize)]
pub struct AppServerConfig {
    #[serde(default = "default_database_url")]
    pub database_url: String,

    #[serde(default = "default_host")]
    pub host: String,

    #[serde(default = "default_port")]
    pub port: u16,

    #[serde(default = "default_jwt_secret")]
    pub jwt_secret: String,

    #[serde(default = "default_jwt_access_ttl_secs")]
    pub jwt_access_ttl_secs: u64,

    #[serde(default = "default_frontend_base_url")]
    pub frontend_base_url: String,

    #[serde(default = "default_rust_log")]
    pub rust_log: String,

    /// Which email backend to use: "log" (dev), "ses", or "resend".
    /// Defaults to "log" so local dev never accidentally sends real email.
    #[serde(default = "default_email_provider")]
    pub email_provider: String,

    #[serde(default = "default_ses_from_address")]
    pub ses_from_address: String,

    // Resend (resend.com) configuration
    #[serde(default)]
    pub resend_api_key: Option<String>,

    #[serde(default = "default_resend_from_address")]
    pub resend_from_address: String,

    #[serde(default)]
    pub aws_access_key_id: Option<String>,

    #[serde(default)]
    pub aws_secret_access_key: Option<String>,

    #[serde(default = "default_aws_region")]
    pub aws_region: String,

    #[serde(default = "default_media_s3_bucket")]
    pub media_s3_bucket: String,

    /// Additional allowed CORS origins, comma-separated.
    ///
    /// The Tauri origins (`tauri://localhost`, `https://tauri.localhost`) and
    /// `http://localhost:3000` are always included. Add your frontend domain here
    /// when deploying behind a reverse proxy, e.g. `https://backup.example.com`.
    #[serde(default)]
    pub cors_origins: Option<String>,

    /// Billing mode:
    /// - `"disabled"` (default) — no tier limits; all users get Pro-equivalent limits (self-hosted).
    /// - `"enforce"`  — tier limits enforced using DB subscription rows; no real payment provider.
    /// - `"external"` — used by the private hosted binary to enable Dodo billing.
    #[serde(default = "default_billing_mode")]
    pub billing_mode: String,
}

impl AppServerConfig {
    /// Load configuration from environment variables and an optional `.env` file.
    ///
    /// The `.env` file is loaded first (if present), then environment variables
    /// take precedence. Missing values fall back to defaults.
    pub fn load() -> Result<Self, envy::Error> {
        // Try .env relative to current dir first, then fall back to the
        // crate's own directory so `cargo run -p api_server` from the
        // workspace root still picks up api_server/.env.
        if dotenvy::dotenv().is_err() {
            let manifest_env = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(".env");
            let _ = dotenvy::from_path(&manifest_env);
        }
        envy::from_env::<Self>()
    }
}

fn default_database_url() -> String {
    "postgres://postgres:admin@localhost:54321/cloudless".to_string()
}

fn default_host() -> String {
    "0.0.0.0".to_string()
}

fn default_port() -> u16 {
    9000
}

fn default_jwt_secret() -> String {
    "secret".to_string()
}

fn default_jwt_access_ttl_secs() -> u64 {
    1800
}

fn default_frontend_base_url() -> String {
    "http://localhost:3000".to_string()
}

fn default_rust_log() -> String {
    "api_server=debug,tower_http=debug,core=debug".to_string()
}

fn default_email_provider() -> String {
    "log".to_string()
}

fn default_ses_from_address() -> String {
    "noreply@example.com".to_string()
}

fn default_resend_from_address() -> String {
    "noreply@example.com".to_string()
}

fn default_aws_region() -> String {
    "us-east-1".to_string()
}

fn default_media_s3_bucket() -> String {
    "cloudless-media".to_string()
}

fn default_billing_mode() -> String {
    "disabled".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_are_applied() {
        // Clear relevant env vars to ensure defaults are used
        let config: AppServerConfig = envy::prefixed("_TEST_NONEXISTENT_PREFIX_")
            .from_env()
            .unwrap();

        assert_eq!(config.port, 9000);
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.jwt_access_ttl_secs, 1800);
    }

    /// Verify email provider defaults to "log" so local dev never sends real email.
    #[test]
    fn test_email_provider_defaults_to_log() {
        let config: AppServerConfig = envy::prefixed("_TEST_NONEXISTENT_PREFIX_")
            .from_env()
            .unwrap();

        assert_eq!(config.email_provider, "log");
        assert!(
            config.resend_api_key.is_none(),
            "resend_api_key must be absent by default"
        );
        assert_eq!(config.resend_from_address, "noreply@example.com");
    }
}
