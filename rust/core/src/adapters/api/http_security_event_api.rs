use api_types::security_event::{
    ListSecurityEventsResponse, StoreSecurityEventRequest, StoreSecurityEventResponse,
};
use async_trait::async_trait;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, security_event_api_port::SecurityEventApiPort},
};

#[derive(Clone)]
pub struct HttpSecurityEventApi {
    api: HttpApi,
}

impl HttpSecurityEventApi {
    pub fn new(api: HttpApi) -> Self {
        Self { api }
    }
}

#[async_trait]
impl SecurityEventApiPort for HttpSecurityEventApi {
    async fn store(
        &self,
        request: StoreSecurityEventRequest,
    ) -> ApiResult<StoreSecurityEventResponse> {
        let url = self.api.get_url("api/security_event/store")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn list(&self) -> ApiResult<ListSecurityEventsResponse> {
        let url = self.api.get_url("api/security_event/list")?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }
}
