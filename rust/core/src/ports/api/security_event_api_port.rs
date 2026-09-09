use api_types::security_event::{
    ListSecurityEventsResponse, StoreSecurityEventRequest, StoreSecurityEventResponse,
};
use async_trait::async_trait;

use crate::ports::api::ApiResult;

#[async_trait]
pub trait SecurityEventApiPort: Send + Sync {
    async fn store(
        &self,
        request: StoreSecurityEventRequest,
    ) -> ApiResult<StoreSecurityEventResponse>;
    async fn list(&self) -> ApiResult<ListSecurityEventsResponse>;
}
