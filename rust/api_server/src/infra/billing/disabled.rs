use std::collections::HashMap;

use async_trait::async_trait;

use crate::core::{
    CoreError, CoreResult,
    ports::{
        payment_provider::{
            CheckoutResult, CreateCheckoutParams, PaymentProvider, ProviderSubscriptionState,
        },
        payment_webhook::{PaymentEvent, PaymentWebhookVerifier, VerifiedPaymentEvent},
    },
};

/// No-op billing adapter used in self-hosted deployments where billing is disabled.
///
/// Every method returns a descriptive error — these methods should not be reachable
/// because the self-hosted binary never mounts billing or checkout routes, and
/// `get_subscription` short-circuits to Pro limits before calling the provider.
#[derive(Clone)]
pub struct DisabledPaymentProvider;

#[async_trait]
impl PaymentProvider for DisabledPaymentProvider {
    async fn create_checkout_session(
        &self,
        _: CreateCheckoutParams,
    ) -> CoreResult<CheckoutResult> {
        Err(CoreError::validation(
            "Billing is disabled on this self-hosted instance.",
        ))
    }

    async fn create_customer_portal_session(&self, _: &str) -> CoreResult<String> {
        Err(CoreError::validation(
            "Billing is disabled on this self-hosted instance.",
        ))
    }

    async fn fetch_subscription(&self, _: &str) -> CoreResult<ProviderSubscriptionState> {
        Err(CoreError::validation(
            "Billing is disabled on this self-hosted instance.",
        ))
    }
}

impl PaymentWebhookVerifier for DisabledPaymentProvider {
    fn verify_and_parse(
        &self,
        _: &HashMap<String, String>,
        _: &[u8],
    ) -> CoreResult<VerifiedPaymentEvent> {
        // Webhook routes are not mounted in self-hosted mode; this is a safety guard.
        Err(CoreError::internal(
            "Webhook received but billing is disabled — route should not be mounted.",
        ))
    }

    fn parse_stored_event(&self, _: &str, _: serde_json::Value) -> CoreResult<PaymentEvent> {
        Err(CoreError::internal(
            "Billing is disabled — no stored events to replay.",
        ))
    }
}
