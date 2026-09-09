use api_types::local_device::{
    CreateLocalDeviceRequest, CreateLocalDeviceResponse, DeviceSummary,
    GetLocalDeviceByPhysicalIdResponse, ListAllDevicesResponse, ListDevicesByPlatformResponse,
};
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    core::{CoreResult, ports::LocalDeviceRepo},
    infra::psql::map_sqlx_error,
};

#[derive(Clone)]
pub struct PgLocalDeviceRepo {
    pub pool: PgPool,
}

impl PgLocalDeviceRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl LocalDeviceRepo for PgLocalDeviceRepo {
    async fn create(
        &self,
        user_id: Uuid,
        local_device: CreateLocalDeviceRequest,
    ) -> CoreResult<CreateLocalDeviceResponse> {
        let id = Uuid::now_v7();
        let row = sqlx::query!(
            r#"
            INSERT INTO local_devices (id, user_id, physical_device_id, display_name, platform)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id, physical_device_id, display_name, platform, created_at
            "#,
            id,
            user_id,
            local_device.physical_device_id,
            local_device.display_name,
            local_device.platform
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(CreateLocalDeviceResponse {
            id: row.id,
            physical_device_id: row.physical_device_id,
            display_name: row.display_name,
            platform: row.platform,
            created_at: row.created_at,
        })
    }

    async fn get_by_physical_id(
        &self,
        user_id: Uuid,
        physical_device_id: &str,
    ) -> CoreResult<GetLocalDeviceByPhysicalIdResponse> {
        let row = sqlx::query!(
            r#"
            SELECT id, physical_device_id, display_name, platform, created_at
            FROM local_devices
            WHERE user_id = $1 AND physical_device_id = $2
            "#,
            user_id,
            physical_device_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(GetLocalDeviceByPhysicalIdResponse {
            id: row.id,
            physical_device_id: row.physical_device_id,
            display_name: row.display_name,
            platform: row.platform,
            created_at: row.created_at,
        })
    }

    async fn list_by_platform(
        &self,
        user_id: Uuid,
        platform: &str,
    ) -> CoreResult<ListDevicesByPlatformResponse> {
        let rows = sqlx::query!(
            r#"
            SELECT id, physical_device_id, display_name, platform, created_at
            FROM local_devices
            WHERE user_id = $1 AND platform = $2
            ORDER BY created_at DESC
            "#,
            user_id,
            platform
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let devices = rows
            .into_iter()
            .map(|row| DeviceSummary {
                id: row.id,
                physical_device_id: row.physical_device_id,
                display_name: row.display_name,
                platform: row.platform,
                created_at: row.created_at,
            })
            .collect();

        Ok(ListDevicesByPlatformResponse { devices })
    }

    async fn list_all(&self, user_id: Uuid) -> CoreResult<ListAllDevicesResponse> {
        let rows = sqlx::query!(
            r#"
            SELECT id, physical_device_id, display_name, platform, created_at
            FROM local_devices
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
            user_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let devices = rows
            .into_iter()
            .map(|row| DeviceSummary {
                id: row.id,
                physical_device_id: row.physical_device_id,
                display_name: row.display_name,
                platform: row.platform,
                created_at: row.created_at,
            })
            .collect();

        Ok(ListAllDevicesResponse { devices })
    }

    async fn count_for_user(&self, user_id: Uuid) -> CoreResult<u32> {
        let row = sqlx::query!(
            r#"SELECT COUNT(*)::int AS count FROM local_devices WHERE user_id = $1"#,
            user_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.count.unwrap_or(0) as u32)
    }

    async fn find_matching_device(
        &self,
        user_id: Uuid,
        physical_device_id: &str,
        platform: &str,
        display_name: Option<&str>,
    ) -> CoreResult<Option<GetLocalDeviceByPhysicalIdResponse>> {
        // Desktop devices carry a stable physical_device_id (machine_uid) — exact match on it alone.
        // Each branch maps its row to GetLocalDeviceByPhysicalIdResponse inline: sqlx::query!
        // generates a distinct anonymous Record type per macro invocation, so the two branches
        // can't share a `let row = if .. { .. } else { .. }` binding before mapping.
        if !physical_device_id.is_empty() {
            let row = sqlx::query!(
                r#"
                SELECT id, physical_device_id, display_name, platform, created_at
                FROM local_devices
                WHERE user_id = $1 AND physical_device_id = $2
                "#,
                user_id,
                physical_device_id
            )
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

            Ok(row.map(|r| GetLocalDeviceByPhysicalIdResponse {
                id: r.id,
                physical_device_id: r.physical_device_id,
                display_name: r.display_name,
                platform: r.platform,
                created_at: r.created_at,
            }))
        } else {
            // Mobile devices have no stable id — match on (platform, display_name) instead.
            // `display_name = $3` uses SQL equality semantics, so two devices with a NULL
            // display_name never match each other (NULL <> NULL) and are never merged.
            let row = sqlx::query!(
                r#"
                SELECT id, physical_device_id, display_name, platform, created_at
                FROM local_devices
                WHERE user_id = $1 AND physical_device_id = '' AND platform = $2 AND display_name = $3
                "#,
                user_id,
                platform,
                display_name
            )
            .fetch_optional(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

            Ok(row.map(|r| GetLocalDeviceByPhysicalIdResponse {
                id: r.id,
                physical_device_id: r.physical_device_id,
                display_name: r.display_name,
                platform: r.platform,
                created_at: r.created_at,
            }))
        }
    }
}
