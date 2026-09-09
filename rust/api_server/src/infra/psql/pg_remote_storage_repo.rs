use api_types::{
    common::Base64EncryptedData,
    remote_storage::{
        CreateRemoteStorageRequest, CreateRemoteStorageResponse, RemoteStorageEntity,
        RemoteStorageStatus, RemoteStorageSummary,
    },
};
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    core::{CoreResult, ports::RemoteStorageRepo},
    infra::psql::map_sqlx_error,
};

#[derive(Clone)]
pub struct PgRemoteStorageRepo {
    pub pool: PgPool,
}

impl PgRemoteStorageRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RemoteStorageRepo for PgRemoteStorageRepo {
    async fn create(
        &self,
        user_id: Uuid,
        storage: CreateRemoteStorageRequest,
    ) -> CoreResult<CreateRemoteStorageResponse> {
        let storage_type_str = storage.storage_type.as_db_str();
        let config_json = serde_json::to_value(&storage.config).map_err(|e| {
            crate::core::CoreError::internal_with_source("Failed to serialize config", e)
        })?;
        let id = Uuid::now_v7();
        let row = sqlx::query!(
            r#"
            INSERT INTO remote_storages (id, user_id, name, storage_type, config)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING id
            "#,
            id,
            user_id,
            storage.name,
            storage_type_str,
            config_json
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        // let storage_type: RemoteStorageType = row.storage_type.parse().map_err(|e| {
        //     crate::core::CoreError::INTERNAL(format!("failed to parse storage type: {}", e))
        // })?;
        // let config = serde_json::from_value(row.config).map_err(|e| {
        //     crate::core::CoreError::INTERNAL(format!("failed to deserialize config: {}", e))
        // })?;

        Ok(CreateRemoteStorageResponse { id: row.id })
    }

    async fn list_by_user(&self, user_id: Uuid) -> CoreResult<Vec<RemoteStorageSummary>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, name, storage_type, status, created_at
            FROM remote_storages
            WHERE user_id = $1
            ORDER BY created_at DESC
            "#,
            user_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter()
            .map(|r| {
                let storage_type = r
                    .storage_type
                    .parse()
                    .map_err(|e: String| crate::core::CoreError::internal(e))?;
                let status: RemoteStorageStatus = r
                    .status
                    .parse()
                    .map_err(|e: String| crate::core::CoreError::internal(e))?;
                Ok(RemoteStorageSummary {
                    id: r.id,
                    name: r.name,
                    storage_type,
                    status,
                    created_at: r.created_at,
                })
            })
            .collect()
    }

    async fn get_by_id(&self, user_id: Uuid, id: Uuid) -> CoreResult<RemoteStorageEntity> {
        let row = sqlx::query!(
            r#"
            SELECT id, user_id, name, storage_type, config, created_at, updated_at
            FROM remote_storages
            WHERE id = $1 AND user_id = $2
            "#,
            id,
            user_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let storage_type = row
            .storage_type
            .parse()
            .map_err(|e: String| crate::core::CoreError::internal(e))?;
        let config = serde_json::from_value(row.config).map_err(|e| {
            crate::core::CoreError::internal_with_source("Failed to deserialize config", e)
        })?;

        Ok(RemoteStorageEntity {
            id: row.id,
            user_id: row.user_id,
            name: row.name,
            storage_type,
            config,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }

    /// Updates the status of a remote storage (e.g. Active → AuthTokenExpired).
    async fn update_status(
        &self,
        user_id: Uuid,
        id: Uuid,
        status: RemoteStorageStatus,
    ) -> CoreResult<()> {
        let status_str = status.as_db_str();
        sqlx::query!(
            r#"
            UPDATE remote_storages
            SET status = $1, updated_at = now()
            WHERE id = $2 AND user_id = $3
            "#,
            status_str,
            id,
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    /// Updates the encrypted config blob and resets status (used during reauth).
    /// The server does not decrypt or inspect the config — identity verification
    /// is performed client-side before calling this.
    async fn update_config(
        &self,
        user_id: Uuid,
        id: Uuid,
        config: Base64EncryptedData,
        status: RemoteStorageStatus,
    ) -> CoreResult<()> {
        let config_json = serde_json::to_value(&config).map_err(|e| {
            crate::core::CoreError::internal_with_source("Failed to serialize config", e)
        })?;
        let status_str = status.as_db_str();
        sqlx::query!(
            r#"
            UPDATE remote_storages
            SET config = $1, status = $2, updated_at = now()
            WHERE id = $3 AND user_id = $4
            "#,
            config_json,
            status_str,
            id,
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }
}
