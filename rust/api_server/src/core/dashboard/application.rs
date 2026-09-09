use api_types::dashboard::GetDashboardStatsResponse;
use uuid::Uuid;

use crate::core::{CoreResult, dashboard::env::DashboardEnv, ports::DashboardRepo};

/// Returns aggregated dashboard statistics for the given user.
pub async fn get_stats<E: DashboardEnv>(
    env: E,
    user_id: Uuid,
) -> CoreResult<GetDashboardStatsResponse> {
    env.dashboard_repo().get_stats(user_id).await
}
