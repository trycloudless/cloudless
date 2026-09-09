use api_types::subscription::{SubscriptionStatus, SubscriptionTier};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::core::CoreResult;

// ---------------------------------------------------------------------------
// Input / output types (provider-agnostic)
// ---------------------------------------------------------------------------

/// Parameters for creating a hosted checkout session.
pub struct CreateCheckoutParams {
    pub cloudless_user_id: Uuid,
    pub cloudless_checkout_id: Uuid,
    pub tier: SubscriptionTier,
    pub product_id: String,
    pub customer_email: String,
    pub customer_name: String,
    /// If the user already has a Dodo customer ID, pass it to link the session.
    pub provider_customer_id: Option<String>,
    pub return_url: String,
    pub cancel_url: String,
    pub cloudless_environment: String,
}

/// Result returned after successfully creating a checkout session.
pub struct CheckoutResult {
    pub checkout_url: String,
    /// Some providers return a session ID separately from the URL.
    pub provider_checkout_session_id: Option<String>,
}

/// Current subscription state fetched directly from the provider (for reconciliation).
pub struct ProviderSubscriptionState {
    pub provider_subscription_id: String,
    pub provider_customer_id: String,
    /// Raw status string from the provider (e.g. "active", "on_hold") — stored in DB.
    pub provider_status: String,
    /// Normalized status mapped by the provider adapter from `provider_status`.
    pub subscription_status: SubscriptionStatus,
    pub provider_product_id: String,
    pub currency: Option<String>,
    pub period_start: Option<DateTime<Utc>>,
    pub period_end: Option<DateTime<Utc>>,
    pub next_billing_date: Option<DateTime<Utc>>,
    pub trial_period_days: Option<i32>,
    /// True when the subscription is cancelled but the user retains paid access until
    /// `period_end` (derived from provider status + billing dates).
    pub cancel_at_period_end: bool,
}

// ---------------------------------------------------------------------------
// Port trait
// ---------------------------------------------------------------------------

/// Abstraction over a payment provider (checkout, portal, subscription fetch).
///
/// Webhook verification is handled separately by `PaymentWebhookVerifier` because
/// it requires raw bytes and is not async over the network.
#[async_trait]
pub trait PaymentProvider: Send + Sync {
    async fn create_checkout_session(
        &self,
        params: CreateCheckoutParams,
    ) -> CoreResult<CheckoutResult>;

    /// Returns the portal URL for the given provider customer ID.
    async fn create_customer_portal_session(&self, customer_id: &str) -> CoreResult<String>;

    /// Fetch current subscription state from the provider for reconciliation.
    async fn fetch_subscription(
        &self,
        provider_subscription_id: &str,
    ) -> CoreResult<ProviderSubscriptionState>;
}
