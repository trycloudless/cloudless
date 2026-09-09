use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_config::Region;
use aws_sdk_sesv2::Client;
use aws_sdk_sesv2::config::Credentials;
use aws_sdk_sesv2::types::{Body, Content, Destination, EmailContent, Message};

use crate::config::AppServerConfig;
use crate::core::{CoreError, CoreResult, ports::EmailPort};
use crate::infra::resend::ResendEmailAdapter;

/// AWS SES v2 email adapter for production use.
#[derive(Clone)]
pub struct SesEmailAdapter {
    client: Client,
    from_address: String,
}

impl SesEmailAdapter {
    /// Construct from `AppServerConfig`.
    ///
    /// If `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY` are set, uses
    /// explicit credentials. Otherwise falls back to the default AWS
    /// credential chain (instance profile, ECS task role, etc.).
    pub async fn from_config(config: &AppServerConfig) -> Self {
        let mut aws_config = aws_config::defaults(BehaviorVersion::v2025_08_07())
            .region(Region::new(config.aws_region.clone()));

        if let (Some(key_id), Some(secret)) =
            (&config.aws_access_key_id, &config.aws_secret_access_key)
        {
            let credentials = Credentials::new(key_id, secret, None, None, "cloudless-api-server");
            aws_config = aws_config.credentials_provider(credentials);
        }

        let aws_config = aws_config.load().await;
        let client = Client::new(&aws_config);
        Self {
            client,
            from_address: config.ses_from_address.clone(),
        }
    }
}

#[async_trait]
impl EmailPort for SesEmailAdapter {
    async fn send_email(&self, to: &str, subject: &str, body_html: &str) -> CoreResult<()> {
        self.client
            .send_email()
            .from_email_address(&self.from_address)
            .destination(Destination::builder().to_addresses(to).build())
            .content(
                EmailContent::builder()
                    .simple(
                        Message::builder()
                            .subject(
                                Content::builder()
                                    .data(subject)
                                    .charset("UTF-8")
                                    .build()
                                    .map_err(|e| {
                                        CoreError::external_service("SES content error", e)
                                    })?,
                            )
                            .body(
                                Body::builder()
                                    .html(
                                        Content::builder()
                                            .data(body_html)
                                            .charset("UTF-8")
                                            .build()
                                            .map_err(|e| {
                                                CoreError::external_service("SES content error", e)
                                            })?,
                                    )
                                    .build(),
                            )
                            .build(),
                    )
                    .build(),
            )
            .send()
            .await
            .map_err(|e| CoreError::external_service("SES send_email failed", e))?;
        Ok(())
    }
}

/// Log-based email adapter for local development.
/// Logs the email details instead of actually sending.
#[derive(Clone)]
pub struct LogEmailAdapter;

#[async_trait]
impl EmailPort for LogEmailAdapter {
    async fn send_email(&self, to: &str, subject: &str, body_html: &str) -> CoreResult<()> {
        tracing::info!(
            "EMAIL [dev mode] to={}, subject={}, body_length={}",
            to,
            subject,
            body_html.len()
        );
        tracing::debug!("EMAIL body:\n{}", body_html);
        Ok(())
    }
}

/// Runtime email adapter — dispatches to SES, Resend, or the log sink.
///
/// Selected at startup via the `EMAIL_PROVIDER` env var ("ses", "resend", "log").
#[derive(Clone)]
pub enum EmailAdapter {
    Ses(SesEmailAdapter),
    Resend(ResendEmailAdapter),
    Log(LogEmailAdapter),
}

impl EmailAdapter {
    /// Build from config based on `email_provider`:
    /// - `"ses"`    → AWS SES v2
    /// - `"resend"` → Resend REST API (requires `RESEND_API_KEY`)
    /// - `"log"`    → log-only, no real sending (default for local dev)
    pub async fn from_config(config: &AppServerConfig) -> Self {
        match config.email_provider.as_str() {
            "ses" => {
                tracing::info!("Using SES email adapter");
                Self::Ses(SesEmailAdapter::from_config(config).await)
            }
            "resend" => {
                let api_key = config
                    .resend_api_key
                    .clone()
                    .expect("RESEND_API_KEY must be set when EMAIL_PROVIDER=resend");
                tracing::info!("Using Resend email adapter");
                Self::Resend(ResendEmailAdapter::new(
                    api_key,
                    config.resend_from_address.clone(),
                ))
            }
            _ => {
                tracing::info!("Using log email adapter (no real emails sent)");
                Self::Log(LogEmailAdapter)
            }
        }
    }
}

#[async_trait]
impl EmailPort for EmailAdapter {
    async fn send_email(&self, to: &str, subject: &str, body_html: &str) -> CoreResult<()> {
        match self {
            Self::Ses(adapter) => adapter.send_email(to, subject, body_html).await,
            Self::Resend(adapter) => adapter.send_email(to, subject, body_html).await,
            Self::Log(adapter) => adapter.send_email(to, subject, body_html).await,
        }
    }
}
