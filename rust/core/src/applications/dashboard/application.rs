use api_types::dashboard::GetDashboardStatsResponse;
use tracing::debug;

use crate::{
    applications::dashboard::env::DashboardEnv, model::base::AppResult,
    ports::api::dashboard_api_port::DashboardApiPort,
};

/// Returns aggregated dashboard statistics for the authenticated user.
pub async fn get_dashboard_stats<E: DashboardEnv>(env: &E) -> AppResult<GetDashboardStatsResponse> {
    let dashboard_api = env.dashboard_api();
    let response = dashboard_api.get_stats().await?;
    debug!(
        files_protected = response.total_files_protected,
        uploaded_bytes = response.total_uploaded_bytes,
        backup_jobs = response.total_backup_jobs,
        restore_jobs = response.total_restore_jobs,
        "Dashboard stats fetched"
    );
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::api::{ApiClientError, ApiResult, dashboard_api_port::DashboardApiPort};
    use async_trait::async_trait;

    struct MockDashboardApi {
        should_fail: bool,
    }

    #[async_trait]
    impl DashboardApiPort for MockDashboardApi {
        async fn get_stats(&self) -> ApiResult<GetDashboardStatsResponse> {
            if self.should_fail {
                return Err(ApiClientError::InvalidUrl("stats failed".into()));
            }
            Ok(GetDashboardStatsResponse {
                total_files_protected: 42,
                total_original_bytes: 1_000_000,
                total_uploaded_bytes: 800_000,
                total_deduplicated_bytes: 200_000,
                active_backup_configs: 2,
                total_backup_jobs: 10,
                total_restore_jobs: 1,
            })
        }
    }

    struct MockDashboardEnv {
        api: MockDashboardApi,
    }

    impl DashboardEnv for MockDashboardEnv {
        type DashboardApi = MockDashboardApi;
        fn dashboard_api(&self) -> &Self::DashboardApi {
            &self.api
        }
    }

    #[tokio::test]
    async fn test_get_dashboard_stats_success() {
        let env = MockDashboardEnv {
            api: MockDashboardApi { should_fail: false },
        };
        let result = get_dashboard_stats(&env).await;
        assert!(result.is_ok());
        let stats = result.unwrap();
        assert_eq!(stats.total_files_protected, 42);
        assert_eq!(stats.active_backup_configs, 2);
        assert_eq!(stats.total_backup_jobs, 10);
        assert_eq!(stats.total_restore_jobs, 1);
    }

    #[tokio::test]
    async fn test_get_dashboard_stats_failure() {
        let env = MockDashboardEnv {
            api: MockDashboardApi { should_fail: true },
        };
        let result = get_dashboard_stats(&env).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_dashboard_stats_returns_correct_byte_counts() {
        let env = MockDashboardEnv {
            api: MockDashboardApi { should_fail: false },
        };
        let stats = get_dashboard_stats(&env).await.unwrap();
        assert_eq!(stats.total_original_bytes, 1_000_000);
        assert_eq!(stats.total_uploaded_bytes, 800_000);
        assert_eq!(stats.total_deduplicated_bytes, 200_000);
    }
}
