use crate::core::ports::{
    self,
    payment_provider::PaymentProvider,
    payment_webhook::PaymentWebhookVerifier,
};

/// Repository access for subscription operations.
pub trait SubscriptionEnv {
    type Repo: ports::SubscriptionRepo;

    fn subscription_repo(&self) -> &Self::Repo;
}

/// Payment provider and configuration access for billing operations.
pub trait PaymentEnv {
    type Provider: PaymentProvider;
    type Verifier: PaymentWebhookVerifier;

    fn payment_provider(&self) -> &Self::Provider;
    fn payment_webhook_verifier(&self) -> &Self::Verifier;

    /// Returns the product ID for the given subscription tier, or `None` if
    /// the tier is not a paid tier or the product ID is not configured.
    fn product_id_for_tier(
        &self,
        tier: api_types::subscription::SubscriptionTier,
    ) -> Option<String>;

    /// Returns the environment name used as webhook metadata (e.g. "test_mode", "live_mode").
    fn payment_environment(&self) -> &str;

    /// Base URL for the billing return page (e.g. "https://trycloudless.io/billing/return").
    fn return_url_base(&self) -> &str;

    /// Base URL for the billing cancel page.
    fn cancel_url_base(&self) -> &str;

    /// Returns true when billing is active (hosted deployment with a configured provider).
    ///
    /// Self-hosted instances return false — entitlements are treated as unlimited.
    /// Defaults to true so existing impls are unaffected until `AppEnv` overrides.
    fn billing_enabled(&self) -> bool {
        true
    }
}
