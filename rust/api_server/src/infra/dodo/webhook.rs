use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use hmac::{Hmac, Mac};
use sha2::Sha256;

const TIMESTAMP_TOLERANCE_SECS: i64 = 300;

/// Verify a Standard Webhooks signature from Dodo.
///
/// Dodo sends three headers per the Standard Webhooks spec:
///   webhook-id        — unique delivery ID (idempotency key)
///   webhook-timestamp — Unix timestamp (seconds) when the event was sent
///   webhook-signature — "v1,<base64-hmac-sha256>" (comma-separated; multiple sigs possible)
///
/// The signed content is:  webhook-id + "." + webhook-timestamp + "." + raw-body
/// The HMAC key is the base64-decoded webhook secret.
///
/// Returns `Ok(())` if the signature is valid and the timestamp is within tolerance.
pub(super) fn verify_webhook_signature(
    secret: &str,
    webhook_id: &str,
    webhook_timestamp: &str,
    raw_body: &[u8],
    webhook_signature: &str,
) -> Result<(), WebhookVerifyError> {
    // Replay-attack protection: reject events older than TIMESTAMP_TOLERANCE_SECS.
    let ts: i64 = webhook_timestamp
        .parse()
        .map_err(|_| WebhookVerifyError::InvalidTimestamp)?;
    let now = chrono::Utc::now().timestamp();
    if (now - ts).abs() > TIMESTAMP_TOLERANCE_SECS {
        return Err(WebhookVerifyError::TimestampOutOfRange);
    }

    // Dodo secrets are prefixed with "whsec_"; strip it before base64-decoding.
    let secret_b64 = secret.strip_prefix("whsec_").unwrap_or(secret);
    let key_bytes = BASE64
        .decode(secret_b64)
        .map_err(|_| WebhookVerifyError::InvalidSecret)?;

    // Build the signed content string.
    let signed_content = format!(
        "{}.{}.{}",
        webhook_id,
        webhook_timestamp,
        // SAFETY: raw_body is UTF-8 because HTTP bodies from Dodo are JSON.
        std::str::from_utf8(raw_body).map_err(|_| WebhookVerifyError::InvalidBody)?
    );

    // Compute HMAC-SHA256.
    let mut mac = Hmac::<Sha256>::new_from_slice(&key_bytes)
        .map_err(|_| WebhookVerifyError::InvalidSecret)?;
    mac.update(signed_content.as_bytes());
    let expected = BASE64.encode(mac.finalize().into_bytes());

    // The signature header may contain multiple comma-separated "v1,<sig>" entries.
    // Accept if any entry matches.
    let matched = webhook_signature
        .split(' ')
        .filter_map(|entry| entry.strip_prefix("v1,"))
        .any(|candidate| constant_time_eq(candidate, &expected));

    if matched {
        Ok(())
    } else {
        Err(WebhookVerifyError::SignatureMismatch)
    }
}

/// Constant-time string comparison to prevent timing attacks.
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}

#[derive(Debug, thiserror::Error)]
pub enum WebhookVerifyError {
    #[error("invalid webhook timestamp")]
    InvalidTimestamp,
    #[error("webhook timestamp is outside the allowed tolerance window")]
    TimestampOutOfRange,
    #[error("webhook secret could not be decoded")]
    InvalidSecret,
    #[error("webhook body is not valid UTF-8")]
    InvalidBody,
    #[error("webhook signature does not match")]
    SignatureMismatch,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::STANDARD as BASE64};
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    fn make_signature(secret_b64: &str, webhook_id: &str, timestamp: &str, body: &str) -> String {
        let key = BASE64.decode(secret_b64).unwrap();
        let signed = format!("{}.{}.{}", webhook_id, timestamp, body);
        let mut mac = Hmac::<Sha256>::new_from_slice(&key).unwrap();
        mac.update(signed.as_bytes());
        format!("v1,{}", BASE64.encode(mac.finalize().into_bytes()))
    }

    fn fresh_ts() -> String {
        chrono::Utc::now().timestamp().to_string()
    }

    #[test]
    fn test_valid_signature() {
        let secret = BASE64.encode(b"test_secret_key_32_bytes_long_xx");
        let id = "wh_123";
        let ts = fresh_ts();
        let body = r#"{"type":"subscription.active"}"#;
        let sig = make_signature(&secret, id, &ts, body);

        assert!(verify_webhook_signature(&secret, id, &ts, body.as_bytes(), &sig).is_ok());
    }

    #[test]
    fn test_wrong_signature_rejected() {
        let secret = BASE64.encode(b"test_secret_key_32_bytes_long_xx");
        let id = "wh_123";
        let ts = fresh_ts();
        let body = r#"{"type":"subscription.active"}"#;

        let result = verify_webhook_signature(&secret, id, &ts, body.as_bytes(), "v1,badsig");
        assert!(matches!(result, Err(WebhookVerifyError::SignatureMismatch)));
    }

    #[test]
    fn test_stale_timestamp_rejected() {
        let secret = BASE64.encode(b"test_secret_key_32_bytes_long_xx");
        let id = "wh_123";
        let old_ts = (chrono::Utc::now().timestamp() - 400).to_string();
        let body = r#"{"type":"subscription.active"}"#;
        let sig = make_signature(&secret, id, &old_ts, body);

        let result = verify_webhook_signature(&secret, id, &old_ts, body.as_bytes(), &sig);
        assert!(matches!(
            result,
            Err(WebhookVerifyError::TimestampOutOfRange)
        ));
    }

    #[test]
    fn test_multiple_signatures_one_valid() {
        let secret = BASE64.encode(b"test_secret_key_32_bytes_long_xx");
        let id = "wh_123";
        let ts = fresh_ts();
        let body = r#"{"type":"subscription.active"}"#;
        let valid = make_signature(&secret, id, &ts, body);
        // Dodo sends space-separated entries per Standard Webhooks spec
        let header = format!("v1,oldsig {}", valid);

        assert!(verify_webhook_signature(&secret, id, &ts, body.as_bytes(), &header).is_ok());
    }
}
