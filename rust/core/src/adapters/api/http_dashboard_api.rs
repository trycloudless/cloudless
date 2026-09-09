use api_types::dashboard::GetDashboardStatsResponse;
use async_trait::async_trait;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, dashboard_api_port::DashboardApiPort},
};

#[derive(Clone)]
pub struct HttpDashboardApi {
    api: HttpApi,
}

impl HttpDashboardApi {
    pub fn new(api: HttpApi) -> Self {
        HttpDashboardApi { api }
    }
}

#[async_trait]
impl DashboardApiPort for HttpDashboardApi {
    async fn get_stats(&self) -> ApiResult<GetDashboardStatsResponse> {
        let url = self.api.get_url("api/dashboard/stats")?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }
}
