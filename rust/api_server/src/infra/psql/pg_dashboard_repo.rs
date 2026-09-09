use api_types::dashboard::GetDashboardStatsResponse;
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    core::{CoreResult, ports::DashboardRepo},
    infra::psql::map_sqlx_error,
};

#[derive(Clone)]
pub struct PgDashboardRepo {
    pub pool: PgPool,
}

impl PgDashboardRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl DashboardRepo for PgDashboardRepo {
    async fn get_stats(&self, user_id: Uuid) -> CoreResult<GetDashboardStatsResponse> {
        let row = sqlx::query!(
            r#"
            SELECT
                (SELECT COUNT(DISTINCT name_blind_index)
                 FROM remote_file_versions
                 WHERE user_id = $1) AS "total_files_protected!",
                COALESCE((SELECT SUM(size)
                 FROM remote_file_versions
                 WHERE user_id = $1), 0)::BIGINT AS "total_original_bytes!",
                COALESCE((SELECT SUM(size)
                 FROM chunks
                 WHERE user_id = $1), 0)::BIGINT AS "total_uploaded_bytes!",
                GREATEST(0,
                    COALESCE((SELECT SUM(size) FROM remote_file_versions WHERE user_id = $1), 0) -
                    COALESCE((SELECT SUM(size) FROM chunks WHERE user_id = $1), 0)
                )::BIGINT AS "total_deduplicated_bytes!",
                (SELECT COUNT(*) FROM backup_config WHERE user_id = $1 AND is_active = true) AS "active_backup_configs!",
                (SELECT COUNT(*) FROM backup_jobs WHERE user_id = $1) AS "total_backup_jobs!",
                (SELECT COUNT(*) FROM restore_jobs WHERE user_id = $1) AS "total_restore_jobs!"
            "#,
            user_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(GetDashboardStatsResponse {
            total_files_protected: row.total_files_protected,
            total_original_bytes: row.total_original_bytes,
            total_uploaded_bytes: row.total_uploaded_bytes,
            total_deduplicated_bytes: row.total_deduplicated_bytes,
            active_backup_configs: row.active_backup_configs,
            total_backup_jobs: row.total_backup_jobs,
            total_restore_jobs: row.total_restore_jobs,
        })
    }
}
