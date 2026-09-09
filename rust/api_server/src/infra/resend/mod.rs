use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::core::{CoreError, CoreResult, ports::EmailPort};

const RESEND_API_URL: &str = "https://api.resend.com/emails";

/// Resend (resend.com) email adapter.
///
/// Sends transactional emails via the Resend REST API using a Bearer API key.
/// Use this as a drop-in replacement for SES while awaiting SES sandbox removal.
#[derive(Clone)]
pub struct ResendEmailAdapter {
    client: reqwest::Client,
    api_key: String,
    from_address: String,
}

impl ResendEmailAdapter {
    /// Build adapter from an API key and sender address.
    pub fn new(api_key: String, from_address: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            from_address,
        }
    }
}

/// JSON body sent to the Resend API.
#[derive(Debug, Serialize)]
struct ResendEmailRequest<'a> {
    from: &'a str,
    to: Vec<&'a str>,
    subject: &'a str,
    html: &'a str,
}

/// Minimal fields we care about from the Resend API response.
#[derive(Debug, Deserialize)]
struct ResendEmailResponse {
    id: Option<String>,
    // Resend returns `message` on errors alongside a non-2xx status.
    // We surface the raw body via the error path instead.
}

#[async_trait]
impl EmailPort for ResendEmailAdapter {
    /// Send an HTML email via the Resend API.
    ///
    /// Returns `CoreError::ExternalService` on HTTP or API-level failures.
    async fn send_email(&self, to: &str, subject: &str, body_html: &str) -> CoreResult<()> {
        let payload = ResendEmailRequest {
            from: &self.from_address,
            to: vec![to],
            subject,
            html: body_html,
        };

        let response = self
            .client
            .post(RESEND_API_URL)
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|e| CoreError::external_service("Resend HTTP request failed", e))?;

        let status = response.status();
        if !status.is_success() {
            // Capture the body for diagnostics without swallowing the error.
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(CoreError::ExternalService {
                message: format!("Resend API returned {}: {}", status, body),
                source: None,
            });
        }

        let resend_response: ResendEmailResponse = response
            .json()
            .await
            .map_err(|e| CoreError::external_service("Resend response parse error", e))?;

        tracing::info!(
            email_id = ?resend_response.id,
            to = to,
            subject = subject,
            "Resend email sent"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify the adapter is constructable and fields are set correctly.
    /// Does not make real HTTP calls — live sending is covered by integration tests.
    #[test]
    fn test_new_stores_fields() {
        let adapter = ResendEmailAdapter::new(
            "re_test_key_123".to_string(),
            "noreply@example.com".to_string(),
        );
        assert_eq!(adapter.api_key, "re_test_key_123");
        assert_eq!(adapter.from_address, "noreply@example.com");
    }

    /// Verify the request payload serialises to the shape Resend expects.
    ///
    /// Resend requires `from`, `to` (array), `subject`, and `html` fields.
    /// A mismatch here would cause silent 422 errors from their API.
    #[test]
    fn test_request_payload_serialization() {
        let payload = ResendEmailRequest {
            from: "noreply@example.com",
            to: vec!["user@example.com"],
            subject: "Verify your email",
            html: "<p>Your code is <b>123456</b></p>",
        };

        let json = serde_json::to_value(&payload).unwrap();
        assert_eq!(json["from"], "noreply@example.com");
        assert_eq!(json["to"][0], "user@example.com");
        assert_eq!(json["subject"], "Verify your email");
        assert_eq!(json["html"], "<p>Your code is <b>123456</b></p>");
    }

    /// Verify `to` is always serialised as an array, even for a single recipient.
    ///
    /// Resend's API requires `to` to be an array; passing a plain string causes a 422.
    #[test]
    fn test_to_field_is_always_array() {
        let payload = ResendEmailRequest {
            from: "noreply@example.com",
            to: vec!["single@example.com"],
            subject: "Test",
            html: "<p>hi</p>",
        };

        let json = serde_json::to_value(&payload).unwrap();
        assert!(
            json["to"].is_array(),
            "`to` must be a JSON array for the Resend API"
        );
        assert_eq!(json["to"].as_array().unwrap().len(), 1);
    }
}
