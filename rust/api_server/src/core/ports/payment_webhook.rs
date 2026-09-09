use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::core::CoreResult;

// ---------------------------------------------------------------------------
// Provider-neutral webhook data types
// ---------------------------------------------------------------------------

/// Normalised subscription webhook data.
///
/// Dodo field names are mapped to standard names on ingestion inside
/// `DodoClient::verify_and_parse`:
///   previous_billing_date       → period_start
///   next_billing_date           → period_end
///   cancel_at_next_billing_date → cancel_at_period_end
pub struct WebhookSubscriptionData {
    pub provider_subscription_id: String,
    pub provider_customer_id: String,
    pub provider_status: String,
    pub product_id: String,
    pub period_start: Option<DateTime<Utc>>,
    pub period_end: Option<DateTime<Utc>>,
    pub cancel_at_period_end: bool,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

/// Normalised payment webhook data.
///
/// The verifier resolves Dodo's dual product_id / product_cart shape into a
/// single `product_id` field (top-level wins; falls back to cart[0]).
pub struct WebhookPaymentData {
    pub provider_payment_id: String,
    pub provider_customer_id: String,
    /// Unified product ID — None when the provider didn't include one.
    pub product_id: Option<String>,
    pub provider_status: String,
    pub provider_subscription_id: Option<String>,
    pub metadata: Option<HashMap<String, serde_json::Value>>,
}

// ---------------------------------------------------------------------------
// PaymentEvent enum
// ---------------------------------------------------------------------------

/// Provider-neutral representation of a webhook event.
pub enum PaymentEvent {
    /// subscription.active / renewed / updated / plan_changed
    SubscriptionActive(WebhookSubscriptionData),
    /// subscription.on_hold
    SubscriptionOnHold(WebhookSubscriptionData),
    /// subscription.cancelled
    SubscriptionCancelled(WebhookSubscriptionData),
    /// subscription.expired / failed
    SubscriptionExpired(WebhookSubscriptionData),
    /// payment.succeeded
    PaymentSucceeded(WebhookPaymentData),
    /// payment.failed
    PaymentFailed(WebhookPaymentData),
    /// Any event type not recognised by this version of the adapter.
    Unknown { event_type: String },
}

impl PaymentEvent {
    /// Extract the provider subscription ID from any event variant that carries one.
    ///
    /// Returns `None` for `PaymentSucceeded`/`PaymentFailed` events where the
    /// subscription ID is absent, and for `Unknown` events.
    pub fn provider_subscription_id(&self) -> Option<&str> {
        match self {
            PaymentEvent::SubscriptionActive(d)
            | PaymentEvent::SubscriptionOnHold(d)
            | PaymentEvent::SubscriptionCancelled(d)
            | PaymentEvent::SubscriptionExpired(d) => Some(&d.provider_subscription_id),
            PaymentEvent::PaymentSucceeded(d) | PaymentEvent::PaymentFailed(d) => {
                d.provider_subscription_id.as_deref()
            }
            PaymentEvent::Unknown { .. } => None,
        }
    }
}

// ---------------------------------------------------------------------------
// VerifiedPaymentEvent
// ---------------------------------------------------------------------------

/// A webhook event that has passed signature verification and been parsed.
pub struct VerifiedPaymentEvent {
    /// Raw event_type string as returned by the provider — stored in DB for audit.
    pub event_type: String,
    pub event: PaymentEvent,
    /// The full raw JSON body, stored for idempotency replay and debugging.
    pub raw_payload: serde_json::Value,
}

// ---------------------------------------------------------------------------
// PaymentWebhookVerifier trait
// ---------------------------------------------------------------------------

/// Combines signature verification + event parsing in a single synchronous call.
///
/// Not async — HMAC verification is CPU-only.
pub trait PaymentWebhookVerifier: Send + Sync {
    /// Verify the webhook signature and parse the body into a [`VerifiedPaymentEvent`].
    ///
    /// Returns `CoreError::Authentication` on signature mismatch, `CoreError::Validation`
    /// on a malformed body, and `CoreError::Internal` on configuration problems (e.g.
    /// missing secret).
    fn verify_and_parse(
        &self,
        headers: &HashMap<String, String>,
        raw_body: &[u8],
    ) -> CoreResult<VerifiedPaymentEvent>;

    /// Re-parse a stored webhook payload into a [`PaymentEvent`] without re-verifying.
    ///
    /// Called by the recovery path where the original request headers are gone.
    /// `payload` is the full raw JSON stored at original delivery time.
    fn parse_stored_event(
        &self,
        event_type: &str,
        payload: serde_json::Value,
    ) -> CoreResult<PaymentEvent>;
}
