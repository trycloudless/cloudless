use api_types::policy::{
    AcceptPolicyRequest, AcceptPolicyResponse, CreatePolicyRequest, CreatePolicyResponse,
    CreatePolicyVersionRequest, CreatePolicyVersionResponse, GetCheckoutPolicyResponse,
    GetPendingPoliciesResponse, ListPoliciesResponse, ListPolicyVersionsResponse, PolicyResponse,
    PolicyVersionResponse, PublicPolicyResponse, PublishPolicyVersionRequest, SkipPolicyRequest,
    SkipPolicyResponse, UpdatePolicyRequest, UpdatePolicyVersionRequest,
};
use async_trait::async_trait;
use uuid::Uuid;

use crate::ports::api::ApiResult;

#[async_trait]
pub trait PolicyApiPort: Send + Sync {
    // User-facing
    async fn get_pending_policies(&self) -> ApiResult<GetPendingPoliciesResponse>;
    async fn accept_policy(&self, req: AcceptPolicyRequest) -> ApiResult<AcceptPolicyResponse>;
    async fn skip_policy(&self, req: SkipPolicyRequest) -> ApiResult<SkipPolicyResponse>;

    // Admin (used by website)
    async fn list_policies(&self) -> ApiResult<ListPoliciesResponse>;
    async fn create_policy(&self, req: CreatePolicyRequest) -> ApiResult<CreatePolicyResponse>;
    async fn update_policy(&self, req: UpdatePolicyRequest) -> ApiResult<PolicyResponse>;
    async fn list_versions(&self, policy_id: Uuid) -> ApiResult<ListPolicyVersionsResponse>;
    async fn get_version(&self, id: Uuid) -> ApiResult<PolicyVersionResponse>;
    async fn create_version(
        &self,
        req: CreatePolicyVersionRequest,
    ) -> ApiResult<CreatePolicyVersionResponse>;
    async fn update_version(
        &self,
        req: UpdatePolicyVersionRequest,
    ) -> ApiResult<PolicyVersionResponse>;
    async fn publish_version(
        &self,
        req: PublishPolicyVersionRequest,
    ) -> ApiResult<PolicyVersionResponse>;

    /// Fetch the latest published version of a policy by slug (kebab-case, no auth required).
    async fn get_public_policy(&self, slug: &str) -> ApiResult<Option<PublicPolicyResponse>>;

    /// Returns the pending Refund & Cancellation policy, or None if already accepted.
    async fn get_checkout_policy(&self) -> ApiResult<GetCheckoutPolicyResponse>;
}
