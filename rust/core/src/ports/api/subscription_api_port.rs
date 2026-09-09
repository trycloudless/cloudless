use api_types::subscription::{SubscriptionResponse, SubscriptionTier};
use async_trait::async_trait;
use uuid::Uuid;

use crate::ports::api::ApiResult;

#[async_trait]
pub trait SubscriptionApiPort: Send + Sync {
    /// Returns the authenticated user's subscription with computed tier limits.
    async fn get_subscription(&self) -> ApiResult<SubscriptionResponse>;

    /// Creates a Dodo checkout session for the given tier.
    /// Returns `(checkout_url, checkout_id)`.
    async fn create_checkout_session(&self, tier: SubscriptionTier) -> ApiResult<(String, Uuid)>;

    /// Returns the status of a checkout session: "pending", "completed", or "failed".
    async fn get_checkout_status(&self, checkout_id: Uuid) -> ApiResult<String>;

    /// Creates a Dodo customer portal session and returns the portal URL.
    async fn create_portal_session(&self) -> ApiResult<String>;
}
