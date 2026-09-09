use api_types::dashboard::GetDashboardStatsResponse;
use async_trait::async_trait;

use crate::ports::api::ApiResult;

#[async_trait]
pub trait DashboardApiPort: Send + Sync {
    /// Returns aggregated dashboard statistics for the authenticated user.
    async fn get_stats(&self) -> ApiResult<GetDashboardStatsResponse>;
}
