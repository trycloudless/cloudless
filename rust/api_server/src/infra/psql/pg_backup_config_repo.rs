use api_types::backup_config::{
    BackupConfig, BackupConfigWithRemoteStorage, CleanupType, CreateBackupConfigRequest,
    CreateBackupConfigResponse, RenameBackupConfigResponse, ToggleBackupConfigResponse,
    UpdateCleanupTypeResponse, UpdateExclusionConfigResponse,
};
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    core::{CoreResult, ports::BackupConfigRepo},
    infra::psql::{map_ownership_violation, map_sqlx_error},
};

#[derive(Clone)]
pub struct PgBackupConfigRepo {
    pub pool: PgPool,
}

impl PgBackupConfigRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BackupConfigRepo for PgBackupConfigRepo {
    async fn create(
        &self,
        user_id: Uuid,
        config: CreateBackupConfigRequest,
    ) -> CoreResult<CreateBackupConfigResponse> {
        let id = Uuid::now_v7();
        let cleanup_type_json = serde_json::to_value(&config.cleanup_type).map_err(|e| {
            crate::core::CoreError::internal_with_source("Failed to serialize cleanup_type", e)
        })?;
        let row = sqlx::query!(
            r#"
            INSERT INTO backup_config (id, user_id, storage_id, local_device_id, encrypted_source_dir, source_dir_nonce, source_dir_blind_index, display_name, cleanup_type, encrypted_exclusion_config, exclusion_config_nonce)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id
            "#,
            id,
            user_id,
            config.storage_id,
            config.local_device_id,
            config.encrypted_source_dir,
            config.source_dir_nonce,
            config.source_dir_blind_index,
            config.display_name,
            cleanup_type_json,
            config.encrypted_exclusion_config as Option<Vec<u8>>,
            config.exclusion_config_nonce as Option<Vec<u8>>,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_ownership_violation)?;

        Ok(CreateBackupConfigResponse { id: row.id })
    }

    async fn get_by_id(&self, user_id: Uuid, id: Uuid) -> CoreResult<BackupConfig> {
        let row = sqlx::query!(
            r#"
            SELECT id, user_id, storage_id, local_device_id,
                   encrypted_source_dir, source_dir_nonce, source_dir_blind_index,
                   created_at, cleanup_type,
                   encrypted_exclusion_config, exclusion_config_nonce
            FROM backup_config
            WHERE id = $1 AND user_id = $2
            "#,
            id,
            user_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let cleanup_type =
            serde_json::from_value::<CleanupType>(row.cleanup_type).map_err(|e| {
                crate::core::CoreError::internal_with_source(
                    "Failed to deserialize cleanup_type",
                    e,
                )
            })?;

        Ok(BackupConfig {
            id: row.id,
            user_id: row.user_id,
            storage_id: row.storage_id,
            local_device_id: row.local_device_id,
            encrypted_source_dir: row.encrypted_source_dir,
            source_dir_nonce: row.source_dir_nonce,
            source_dir_blind_index: row.source_dir_blind_index,
            created_at: Some(row.created_at),
            cleanup_type,
            encrypted_exclusion_config: row.encrypted_exclusion_config,
            exclusion_config_nonce: row.exclusion_config_nonce,
        })
    }

    async fn list_config_with_storage(
        &self,
        user_id: Uuid,
        physical_device_id: &str,
    ) -> CoreResult<Vec<BackupConfigWithRemoteStorage>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                bc.id as config_id,
                bc.storage_id,
                bc.local_device_id,
                bc.encrypted_source_dir,
                bc.source_dir_nonce,
                bc.display_name,
                bc.is_active,
                bc.cleanup_type,
                bc.encrypted_exclusion_config,
                bc.exclusion_config_nonce,
                rs.storage_type,
                rs.config,
                ld.physical_device_id
            FROM backup_config bc
            JOIN remote_storages rs ON bc.storage_id = rs.id
            JOIN local_devices ld ON bc.local_device_id = ld.id
            WHERE bc.user_id = $1 AND ld.physical_device_id = $2
            "#,
            user_id,
            physical_device_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter()
            .map(|row| {
                let storage_type = row
                    .storage_type
                    .parse()
                    .map_err(|e: String| crate::core::CoreError::internal(e))?;
                let config = serde_json::from_value(row.config).map_err(|e| {
                    crate::core::CoreError::internal_with_source("Failed to deserialize config", e)
                })?;
                let cleanup_type = serde_json::from_value::<CleanupType>(row.cleanup_type)
                    .map_err(|e| {
                        crate::core::CoreError::internal_with_source(
                            "Failed to deserialize cleanup_type",
                            e,
                        )
                    })?;

                Ok(BackupConfigWithRemoteStorage {
                    config_id: row.config_id,
                    storage_id: row.storage_id,
                    encrypted_source_dir: row.encrypted_source_dir,
                    source_dir_nonce: row.source_dir_nonce,
                    display_name: row.display_name,
                    storage_type,
                    config,
                    local_device_id: Some(row.local_device_id),
                    physical_device_id: Some(row.physical_device_id),
                    is_active: row.is_active,
                    cleanup_type,
                    encrypted_exclusion_config: row.encrypted_exclusion_config,
                    exclusion_config_nonce: row.exclusion_config_nonce,
                })
            })
            .collect()
    }

    async fn list_all_for_user(
        &self,
        user_id: Uuid,
    ) -> CoreResult<Vec<BackupConfigWithRemoteStorage>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                bc.id as config_id,
                bc.storage_id,
                bc.local_device_id,
                bc.encrypted_source_dir,
                bc.source_dir_nonce,
                bc.display_name,
                bc.is_active,
                bc.cleanup_type,
                bc.encrypted_exclusion_config,
                bc.exclusion_config_nonce,
                rs.storage_type,
                rs.config,
                ld.physical_device_id
            FROM backup_config bc
            JOIN remote_storages rs ON bc.storage_id = rs.id
            JOIN local_devices ld ON bc.local_device_id = ld.id
            WHERE bc.user_id = $1
            ORDER BY bc.created_at DESC
            "#,
            user_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        rows.into_iter()
            .map(|row| {
                let storage_type = row
                    .storage_type
                    .parse()
                    .map_err(|e: String| crate::core::CoreError::internal(e))?;
                let config = serde_json::from_value(row.config).map_err(|e| {
                    crate::core::CoreError::internal_with_source("Failed to deserialize config", e)
                })?;
                let cleanup_type = serde_json::from_value::<CleanupType>(row.cleanup_type)
                    .map_err(|e| {
                        crate::core::CoreError::internal_with_source(
                            "Failed to deserialize cleanup_type",
                            e,
                        )
                    })?;

                Ok(BackupConfigWithRemoteStorage {
                    config_id: row.config_id,
                    storage_id: row.storage_id,
                    encrypted_source_dir: row.encrypted_source_dir,
                    source_dir_nonce: row.source_dir_nonce,
                    display_name: row.display_name,
                    storage_type,
                    config,
                    local_device_id: Some(row.local_device_id),
                    physical_device_id: Some(row.physical_device_id),
                    is_active: row.is_active,
                    cleanup_type,
                    encrypted_exclusion_config: row.encrypted_exclusion_config,
                    exclusion_config_nonce: row.exclusion_config_nonce,
                })
            })
            .collect()
    }

    async fn set_active(
        &self,
        user_id: Uuid,
        id: Uuid,
        is_active: bool,
    ) -> CoreResult<ToggleBackupConfigResponse> {
        let row = sqlx::query!(
            r#"
            UPDATE backup_config
            SET is_active = $1
            WHERE id = $2 AND user_id = $3
            RETURNING id, is_active
            "#,
            is_active,
            id,
            user_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(ToggleBackupConfigResponse {
            id: row.id,
            is_active: row.is_active,
        })
    }

    async fn rename(
        &self,
        user_id: Uuid,
        id: Uuid,
        display_name: String,
    ) -> CoreResult<RenameBackupConfigResponse> {
        let row = sqlx::query!(
            r#"
            UPDATE backup_config
            SET display_name = $1
            WHERE id = $2 AND user_id = $3
            RETURNING id, display_name
            "#,
            display_name,
            id,
            user_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(RenameBackupConfigResponse {
            id: row.id,
            display_name: row.display_name,
        })
    }

    async fn update_cleanup_type(
        &self,
        user_id: Uuid,
        id: Uuid,
        cleanup_type: CleanupType,
    ) -> CoreResult<UpdateCleanupTypeResponse> {
        let cleanup_type_json = serde_json::to_value(&cleanup_type).map_err(|e| {
            crate::core::CoreError::internal_with_source("Failed to serialize cleanup_type", e)
        })?;

        sqlx::query!(
            r#"
            UPDATE backup_config
            SET cleanup_type = $1
            WHERE id = $2 AND user_id = $3
            "#,
            cleanup_type_json,
            id,
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(UpdateCleanupTypeResponse { id, cleanup_type })
    }

    async fn update_exclusion_config(
        &self,
        user_id: Uuid,
        id: Uuid,
        encrypted_exclusion_config: Vec<u8>,
        exclusion_config_nonce: Vec<u8>,
    ) -> CoreResult<UpdateExclusionConfigResponse> {
        sqlx::query!(
            r#"
            UPDATE backup_config
            SET encrypted_exclusion_config = $1, exclusion_config_nonce = $2
            WHERE id = $3 AND user_id = $4
            "#,
            encrypted_exclusion_config,
            exclusion_config_nonce,
            id,
            user_id
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(UpdateExclusionConfigResponse { id })
    }

    async fn count_for_user(&self, user_id: Uuid) -> CoreResult<u32> {
        let row = sqlx::query!(
            r#"SELECT COUNT(*)::int AS count FROM backup_config WHERE user_id = $1"#,
            user_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;
        Ok(row.count.unwrap_or(0) as u32)
    }
}
