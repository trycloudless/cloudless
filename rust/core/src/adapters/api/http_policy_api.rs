use api_types::policy::{
    AcceptPolicyRequest, AcceptPolicyResponse, CreatePolicyRequest, CreatePolicyResponse,
    CreatePolicyVersionRequest, CreatePolicyVersionResponse, GetCheckoutPolicyResponse,
    GetPendingPoliciesResponse, ListPoliciesResponse, ListPolicyVersionsResponse, PolicyResponse,
    PolicyVersionResponse, PublicPolicyResponse, PublishPolicyVersionRequest, SkipPolicyRequest,
    SkipPolicyResponse, UpdatePolicyRequest, UpdatePolicyVersionRequest,
};
use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, policy_api_port::PolicyApiPort},
};

#[derive(Clone)]
pub struct HttpPolicyApi {
    api: HttpApi,
}

impl HttpPolicyApi {
    pub fn new(api: HttpApi) -> Self {
        HttpPolicyApi { api }
    }
}

#[async_trait]
impl PolicyApiPort for HttpPolicyApi {
    async fn get_pending_policies(&self) -> ApiResult<GetPendingPoliciesResponse> {
        let url = self.api.get_url("api/policy/pending")?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }

    async fn accept_policy(&self, req: AcceptPolicyRequest) -> ApiResult<AcceptPolicyResponse> {
        let url = self.api.get_url("api/policy/accept")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn skip_policy(&self, req: SkipPolicyRequest) -> ApiResult<SkipPolicyResponse> {
        let url = self.api.get_url("api/policy/skip")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn list_policies(&self) -> ApiResult<ListPoliciesResponse> {
        let url = self.api.get_url("api/policy")?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }

    async fn create_policy(&self, req: CreatePolicyRequest) -> ApiResult<CreatePolicyResponse> {
        let url = self.api.get_url("api/policy")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn update_policy(&self, req: UpdatePolicyRequest) -> ApiResult<PolicyResponse> {
        let url = self.api.get_url(&format!("api/policy/{}", req.id))?;
        let request = self.api.client.put(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn list_versions(&self, policy_id: Uuid) -> ApiResult<ListPolicyVersionsResponse> {
        let url = self
            .api
            .get_url(&format!("api/policy/{}/versions", policy_id))?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }

    async fn get_version(&self, id: Uuid) -> ApiResult<PolicyVersionResponse> {
        let url = self.api.get_url(&format!("api/policy/version/{}", id))?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }

    async fn create_version(
        &self,
        req: CreatePolicyVersionRequest,
    ) -> ApiResult<CreatePolicyVersionResponse> {
        let url = self.api.get_url("api/policy/version")?;
        let request = self.api.client.post(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn update_version(
        &self,
        req: UpdatePolicyVersionRequest,
    ) -> ApiResult<PolicyVersionResponse> {
        let url = self
            .api
            .get_url(&format!("api/policy/version/{}", req.id))?;
        let request = self.api.client.put(url).json(&req);
        self.api.send_with_auth(request).await
    }

    async fn publish_version(
        &self,
        req: PublishPolicyVersionRequest,
    ) -> ApiResult<PolicyVersionResponse> {
        let url = self
            .api
            .get_url(&format!("api/policy/version/{}/publish", req.id))?;
        let request = self.api.client.post(url);
        self.api.send_with_auth(request).await
    }

    async fn get_checkout_policy(&self) -> ApiResult<GetCheckoutPolicyResponse> {
        let url = self.api.get_url("api/policy/checkout-policy")?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }

    async fn get_public_policy(&self, slug: &str) -> ApiResult<Option<PublicPolicyResponse>> {
        let url = self.api.get_url(&format!("api/policy/public/{}", slug))?;
        let request = self.api.client.get(url);
        match self.api.send::<PublicPolicyResponse>(request).await {
            Ok(policy) => Ok(Some(policy)),
            Err(crate::ports::api::ApiClientError::Api(api_types::error::ApiError::NotFound {
                ..
            })) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
