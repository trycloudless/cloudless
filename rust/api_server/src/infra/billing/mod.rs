pub mod disabled;

use std::collections::HashMap;

use async_trait::async_trait;

use crate::{
    core::{
        CoreResult,
        ports::{
            payment_provider::{
                CheckoutResult, CreateCheckoutParams, PaymentProvider, ProviderSubscriptionState,
            },
            payment_webhook::{PaymentEvent, PaymentWebhookVerifier, VerifiedPaymentEvent},
        },
    },
    infra::{billing::disabled::DisabledPaymentProvider, dodo::client::DodoClient},
};

/// Top-level billing discriminant.
///
/// Follows the same enum pattern as `EmailAdapter` — no `Arc<dyn>` overhead,
/// no generics on `AppEnv`.  Add new variants here when integrating a second
/// payment provider.
#[derive(Clone)]
pub enum BillingAdapter {
    /// Hosted deployment — all billing features active.
    Dodo(DodoClient),
    /// Self-hosted deployment — billing features disabled; all methods return errors.
    Disabled(DisabledPaymentProvider),
}

// ---------------------------------------------------------------------------
// PaymentProvider delegation
// ---------------------------------------------------------------------------

#[async_trait]
impl PaymentProvider for BillingAdapter {
    async fn create_checkout_session(
        &self,
        params: CreateCheckoutParams,
    ) -> CoreResult<CheckoutResult> {
        match self {
            BillingAdapter::Dodo(c) => c.create_checkout_session(params).await,
            BillingAdapter::Disabled(d) => d.create_checkout_session(params).await,
        }
    }

    async fn create_customer_portal_session(&self, customer_id: &str) -> CoreResult<String> {
        match self {
            BillingAdapter::Dodo(c) => c.create_customer_portal_session(customer_id).await,
            BillingAdapter::Disabled(d) => d.create_customer_portal_session(customer_id).await,
        }
    }

    async fn fetch_subscription(
        &self,
        provider_subscription_id: &str,
    ) -> CoreResult<ProviderSubscriptionState> {
        match self {
            BillingAdapter::Dodo(c) => c.fetch_subscription(provider_subscription_id).await,
            BillingAdapter::Disabled(d) => d.fetch_subscription(provider_subscription_id).await,
        }
    }
}

// ---------------------------------------------------------------------------
// PaymentWebhookVerifier delegation
// ---------------------------------------------------------------------------

impl PaymentWebhookVerifier for BillingAdapter {
    fn verify_and_parse(
        &self,
        headers: &HashMap<String, String>,
        raw_body: &[u8],
    ) -> CoreResult<VerifiedPaymentEvent> {
        match self {
            BillingAdapter::Dodo(c) => c.verify_and_parse(headers, raw_body),
            BillingAdapter::Disabled(d) => d.verify_and_parse(headers, raw_body),
        }
    }

    fn parse_stored_event(
        &self,
        event_type: &str,
        payload: serde_json::Value,
    ) -> CoreResult<PaymentEvent> {
        match self {
            BillingAdapter::Dodo(c) => c.parse_stored_event(event_type, payload),
            BillingAdapter::Disabled(d) => d.parse_stored_event(event_type, payload),
        }
    }
}
