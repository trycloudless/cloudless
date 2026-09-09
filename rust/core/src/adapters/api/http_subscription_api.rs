use api_types::subscription::{
    CheckoutStatusResponse, CreateCheckoutRequest, CreateCheckoutResponse, CreatePortalResponse,
    SubscriptionResponse, SubscriptionTier,
};
use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, subscription_api_port::SubscriptionApiPort},
};

#[derive(Clone)]
pub struct HttpSubscriptionApi {
    api: HttpApi,
}

impl HttpSubscriptionApi {
    pub fn new(api: HttpApi) -> Self {
        HttpSubscriptionApi { api }
    }
}

#[async_trait]
impl SubscriptionApiPort for HttpSubscriptionApi {
    async fn get_subscription(&self) -> ApiResult<SubscriptionResponse> {
        let url = self.api.get_url("api/subscription/current")?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }

    async fn create_checkout_session(&self, tier: SubscriptionTier) -> ApiResult<(String, Uuid)> {
        let url = self.api.get_url("api/subscription/checkout")?;
        let request = self
            .api
            .client
            .post(url)
            .json(&CreateCheckoutRequest { tier });
        let response: CreateCheckoutResponse = self.api.send_with_auth(request).await?;
        Ok((response.checkout_url, response.checkout_id))
    }

    async fn get_checkout_status(&self, checkout_id: Uuid) -> ApiResult<String> {
        let url = self
            .api
            .get_url(&format!("api/subscription/checkout/{}/status", checkout_id))?;
        let request = self.api.client.get(url);
        let response: CheckoutStatusResponse = self.api.send_with_auth(request).await?;
        Ok(response.status)
    }

    async fn create_portal_session(&self) -> ApiResult<String> {
        let url = self.api.get_url("api/subscription/portal")?;
        let request = self.api.client.post(url).json(&serde_json::json!({}));
        let response: CreatePortalResponse = self.api.send_with_auth(request).await?;
        Ok(response.portal_url)
    }
}
