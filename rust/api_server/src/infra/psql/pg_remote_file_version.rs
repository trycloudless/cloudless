use api_types::remote_file_version::{
    BackedUpFile, CreateFileVersionRequest, CreateFileVersionResponse, FileVersionStatus,
    FileVersionStatusHistory, FileVersionSummary, ListAllVersionsRequest, ListAllVersionsResponse,
    ListBackedUpFilesRequest, ListBackedUpFilesResponse, ListBinVersionsRequest,
    ListBinVersionsResponse, MoveAllVersionsToBinRequest, RemoteFileVersion,
    UpdateFileVersionStatusRequest,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    core::{CoreError, CoreResult, ports::RemoteFileVersionRepo},
    infra::psql::{fingerprint_request, map_ownership_violation, map_sqlx_error},
};

#[derive(Clone)]
pub struct PgRemoteFileVersionRepo {
    pub pool: PgPool,
}

impl PgRemoteFileVersionRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl RemoteFileVersionRepo for PgRemoteFileVersionRepo {
    async fn create(
        &self,
        user_id: Uuid,
        request: CreateFileVersionRequest,
    ) -> CoreResult<CreateFileVersionResponse> {
        let fingerprint = fingerprint_request(&request)?;

        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;

        // Idempotency check: same key + same request body replays the stored response
        // verbatim; same key + a different body is a conflict, not a silent overwrite.
        // FOR UPDATE serializes concurrent callers racing on the same key.
        let existing = sqlx::query!(
            r#"
            SELECT request_fingerprint, response_snapshot
            FROM file_version_idempotency_keys
            WHERE idempotency_key = $1
            FOR UPDATE
            "#,
            request.idempotency_key,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        if let Some(existing) = existing {
            if existing.request_fingerprint != fingerprint {
                return Err(CoreError::conflict(
                    "idempotency key was already used with a different request",
                ));
            }
            let response: CreateFileVersionResponse =
                serde_json::from_value(existing.response_snapshot).map_err(|e| {
                    CoreError::internal_with_source(
                        "Failed to deserialize stored idempotency response",
                        e,
                    )
                })?;
            tx.commit().await.map_err(map_sqlx_error)?;
            return Ok(response);
        }

        // Serialize the file identity into a stable lock key so concurrent writers for
        // the same (backup_config_id, name_blind_index) serialize on version assignment
        // instead of racing to read the same MAX(version).
        let lock_key = format!(
            "{}:{}",
            request.backup_config_id,
            hex::encode(&request.name_blind_index)
        );
        sqlx::query!(
            r#"SELECT pg_advisory_xact_lock(hashtextextended($1, 0))"#,
            lock_key,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        let max_version = sqlx::query!(
            r#"
            SELECT MAX(version) as max_version
            FROM remote_file_versions
            WHERE backup_config_id = $1 AND name_blind_index = $2 AND user_id = $3
            "#,
            request.backup_config_id,
            &request.name_blind_index,
            user_id,
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx_error)?
        .max_version;

        if max_version.map(|v| v as u32) != request.base_version {
            return Err(CoreError::conflict(
                "base_version does not match the current server version for this file",
            ));
        }

        let new_version = max_version.map(|v| v + 1).unwrap_or(1);
        let id = Uuid::now_v7();
        let status_json = serde_json::to_value(&request.status).map_err(|e| {
            CoreError::internal_with_source("Failed to serialize status", e)
        })?;
        let status_history = FileVersionStatusHistory(vec![(Utc::now(), request.status.clone())]);
        let status_history_json = serde_json::to_value(&status_history).map_err(|e| {
            CoreError::internal_with_source("Failed to serialize status_history", e)
        })?;

        sqlx::query!(
            r#"
            INSERT INTO remote_file_versions
                (id, user_id, backup_config_id, device_id, storage_id, encrypted_name, name_nonce,
                 name_blind_index, version, size, status, status_history, local_file_updated_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
            "#,
            id,
            user_id,
            request.backup_config_id,
            request.device_id,
            request.storage_id,
            &request.encrypted_name,
            &request.name_nonce,
            &request.name_blind_index,
            new_version,
            request.size,
            status_json,
            status_history_json,
            request.local_file_updated_at,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_ownership_violation)?;

        let response = CreateFileVersionResponse {
            id,
            version: new_version as u32,
        };
        let response_snapshot = serde_json::to_value(&response).map_err(|e| {
            CoreError::internal_with_source("Failed to serialize idempotency response snapshot", e)
        })?;

        sqlx::query!(
            r#"
            INSERT INTO file_version_idempotency_keys
                (idempotency_key, user_id, request_fingerprint, response_snapshot)
            VALUES ($1, $2, $3, $4)
            "#,
            request.idempotency_key,
            user_id,
            fingerprint,
            response_snapshot,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        tx.commit().await.map_err(map_sqlx_error)?;

        Ok(response)
    }

    async fn list_backed_up_files(
        &self,
        user_id: Uuid,
        request: ListBackedUpFilesRequest,
    ) -> CoreResult<ListBackedUpFilesResponse> {
        let limit = request.limit.min(200).max(1);
        // Fetch limit+1 files to detect has_more
        let fetch_limit = limit + 1;

        // Step 1: Get the page of distinct blind_indexes using keyset cursor.
        // Uses a CTE to first select the page of file identifiers, then
        // joins back to get all versions for those files.
        let rows = sqlx::query(
            r#"
            WITH page_files AS (
                SELECT DISTINCT ON (rfv.name_blind_index)
                    rfv.name_blind_index
                FROM remote_file_versions rfv
                WHERE rfv.user_id = $1
                  AND rfv.backup_config_id = $2
                  AND rfv.status NOT IN ('"MovedToBin"', '"Deleted"')
                  AND ($3::bytea IS NULL OR rfv.name_blind_index > $3)
                ORDER BY rfv.name_blind_index
                LIMIT $4
            )
            SELECT
                rfv.id as version_id,
                rfv.encrypted_name,
                rfv.name_nonce,
                rfv.name_blind_index,
                rfv.version,
                rfv.size,
                rfv.status,
                rfv.local_file_updated_at as file_updated_at,
                rfv.created_at
            FROM remote_file_versions rfv
            INNER JOIN page_files pf ON rfv.name_blind_index = pf.name_blind_index
            WHERE rfv.user_id = $1
              AND rfv.backup_config_id = $2
              AND rfv.status NOT IN ('"MovedToBin"', '"Deleted"')
            ORDER BY rfv.name_blind_index, rfv.version DESC
            "#,
        )
        .bind(user_id)
        .bind(request.backup_config_id)
        .bind(&request.cursor)
        .bind(fetch_limit)
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        // Step 2: Group rows into files
        let mut files: Vec<BackedUpFile> = Vec::new();
        let mut current_blind_index: Option<Vec<u8>> = None;

        for row in &rows {
            use sqlx::Row;
            let status: FileVersionStatus =
                serde_json::from_value(row.get::<serde_json::Value, _>("status")).map_err(|e| {
                    crate::core::CoreError::internal_with_source(
                        "Failed to deserialize file version status",
                        e,
                    )
                })?;

            let blind_index: Vec<u8> = row.get("name_blind_index");

            let version_summary = FileVersionSummary {
                version_id: row.get("version_id"),
                version: row.get::<i32, _>("version") as u32,
                size: row.get("size"),
                status,
                created_at: row.get("created_at"),
            };

            if current_blind_index.as_ref() == Some(&blind_index) {
                files.last_mut().unwrap().versions.push(version_summary);
            } else {
                current_blind_index = Some(blind_index.clone());
                files.push(BackedUpFile {
                    file_id: row.get("version_id"),
                    encrypted_name: row.get("encrypted_name"),
                    name_nonce: row.get("name_nonce"),
                    blind_index,
                    path: String::new(),
                    size: row.get("size"),
                    file_updated_at: row.get("file_updated_at"),
                    versions: vec![version_summary],
                });
            }
        }

        // Step 3: Detect has_more — if we got more files than limit, trim
        let has_more = files.len() as i64 > limit;
        if has_more {
            files.truncate(limit as usize);
        }

        let next_cursor = if has_more {
            files.last().map(|f| f.blind_index.clone())
        } else {
            None
        };

        // Step 4: Count total files on first page only
        let total_files = if request.cursor.is_none() {
            let count_row = sqlx::query(
                r#"
                SELECT COUNT(DISTINCT rfv.name_blind_index) as total
                FROM remote_file_versions rfv
                WHERE rfv.user_id = $1
                  AND rfv.backup_config_id = $2
                  AND rfv.status NOT IN ('"MovedToBin"', '"Deleted"')
                "#,
            )
            .bind(user_id)
            .bind(request.backup_config_id)
            .fetch_one(&self.pool)
            .await
            .map_err(map_sqlx_error)?;
            use sqlx::Row;
            Some(count_row.get::<i64, _>("total"))
        } else {
            None
        };

        Ok(ListBackedUpFilesResponse {
            files,
            has_more,
            next_cursor,
            total_files,
        })
    }

    async fn list_all_versions(
        &self,
        user_id: Uuid,
        request: ListAllVersionsRequest,
        retention_cutoff: Option<DateTime<Utc>>,
    ) -> CoreResult<ListAllVersionsResponse> {
        let rows = sqlx::query!(
            r#"
            SELECT
                rfv.id,
                rfv.device_id,
                rfv.encrypted_name,
                rfv.name_nonce,
                rfv.name_blind_index,
                rfv.version,
                rfv.size,
                rfv.status,
                rfv.local_file_updated_at,
                rfv.created_at
            FROM remote_file_versions rfv
            WHERE rfv.user_id = $1
              AND rfv.backup_config_id = $2
              AND rfv.status NOT IN ('"MovedToBin"', '"Deleted"')
              AND (
                rfv.created_at >= COALESCE($3, '-infinity'::timestamptz)
                OR rfv.version = (
                    SELECT MAX(rfv2.version)
                    FROM remote_file_versions rfv2
                    WHERE rfv2.backup_config_id = rfv.backup_config_id
                      AND rfv2.name_blind_index = rfv.name_blind_index
                      AND rfv2.status NOT IN ('"MovedToBin"', '"Deleted"')
                )
              )
            ORDER BY rfv.name_blind_index, rfv.version DESC
            "#,
            user_id,
            request.backup_config_id,
            retention_cutoff as Option<DateTime<Utc>>,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let versions = rows
            .into_iter()
            .map(|row| {
                let status: FileVersionStatus =
                    serde_json::from_value(row.status).map_err(|e| {
                        crate::core::CoreError::internal_with_source(
                            "Failed to deserialize file version status",
                            e,
                        )
                    })?;

                Ok(RemoteFileVersion {
                    id: row.id,
                    device_id: row.device_id,
                    encrypted_name: row.encrypted_name,
                    name_nonce: row.name_nonce,
                    name_blind_index: row.name_blind_index,
                    version: row.version as u32,
                    size: row.size,
                    status,
                    local_file_updated_at: row.local_file_updated_at,
                    created_at: row.created_at,
                })
            })
            .collect::<CoreResult<Vec<_>>>()?;

        Ok(ListAllVersionsResponse { versions })
    }

    async fn update_status(&self, request: UpdateFileVersionStatusRequest) -> CoreResult<()> {
        let status_json = serde_json::to_value(&request.status).unwrap();

        sqlx::query!(
            r#"
            UPDATE remote_file_versions
            SET status = $1,
                status_history = status_history || jsonb_build_array(jsonb_build_array(to_jsonb(now()), $1::jsonb))
            WHERE id = $2
            "#,
            status_json,
            request.id
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn move_to_bin(&self, user_id: Uuid, version_id: Uuid) -> CoreResult<()> {
        let new_status = serde_json::to_value(&FileVersionStatus::MovedToBin).unwrap();

        let result = sqlx::query!(
            r#"
            UPDATE remote_file_versions
            SET status = $1,
                status_history = status_history || jsonb_build_array(jsonb_build_array(to_jsonb(now()), $1::jsonb))
            WHERE id = $2
              AND user_id = $3
              AND (status = '"VerifiedOnRemoteStorage"' OR status = '"Restored"')
            "#,
            new_status,
            version_id,
            user_id,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            return Err(crate::core::CoreError::not_found(
                "File version not found or not in a deletable state",
            ));
        }

        Ok(())
    }

    async fn move_all_to_bin(
        &self,
        user_id: Uuid,
        request: MoveAllVersionsToBinRequest,
    ) -> CoreResult<()> {
        let new_status = serde_json::to_value(&FileVersionStatus::MovedToBin).unwrap();

        let result = sqlx::query!(
            r#"
            UPDATE remote_file_versions
            SET status = $1,
                status_history = status_history || jsonb_build_array(jsonb_build_array(to_jsonb(now()), $1::jsonb))
            WHERE user_id = $2
              AND name_blind_index = $3
              AND backup_config_id = $4
              AND (status = '"VerifiedOnRemoteStorage"' OR status = '"Restored"')
            "#,
            new_status,
            user_id,
            &request.name_blind_index,
            request.backup_config_id,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            return Err(crate::core::CoreError::not_found(
                "No file versions found or none in a deletable state",
            ));
        }

        Ok(())
    }

    async fn restore_from_bin(&self, user_id: Uuid, version_id: Uuid) -> CoreResult<()> {
        let new_status = serde_json::to_value(&FileVersionStatus::Restored).unwrap();

        let result = sqlx::query!(
            r#"
            UPDATE remote_file_versions
            SET status = $1,
                status_history = status_history || jsonb_build_array(jsonb_build_array(to_jsonb(now()), $1::jsonb))
            WHERE id = $2
              AND user_id = $3
              AND status = '"MovedToBin"'
            "#,
            new_status,
            version_id,
            user_id,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            return Err(crate::core::CoreError::not_found(
                "File version not found or not in bin",
            ));
        }

        Ok(())
    }

    async fn list_bin_versions(
        &self,
        user_id: Uuid,
        request: ListBinVersionsRequest,
    ) -> CoreResult<ListBinVersionsResponse> {
        let rows = sqlx::query!(
            r#"
            SELECT
                rfv.id as version_id,
                rfv.encrypted_name,
                rfv.name_nonce,
                rfv.name_blind_index,
                rfv.version,
                rfv.size,
                rfv.status,
                rfv.local_file_updated_at as file_updated_at,
                rfv.created_at
            FROM remote_file_versions rfv
            WHERE rfv.user_id = $1
              AND rfv.backup_config_id = $2
              AND rfv.status = '"MovedToBin"'
            ORDER BY rfv.name_blind_index, rfv.version DESC
            "#,
            user_id,
            request.backup_config_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let mut files: Vec<BackedUpFile> = Vec::new();
        let mut current_blind_index: Option<Vec<u8>> = None;

        for row in rows {
            let status: FileVersionStatus = serde_json::from_value(row.status).map_err(|e| {
                crate::core::CoreError::internal_with_source(
                    "Failed to deserialize file version status",
                    e,
                )
            })?;

            let version_summary = FileVersionSummary {
                version_id: row.version_id,
                version: row.version as u32,
                size: row.size,
                status,
                created_at: row.created_at,
            };

            if current_blind_index.as_ref() == Some(&row.name_blind_index) {
                files.last_mut().unwrap().versions.push(version_summary);
            } else {
                current_blind_index = Some(row.name_blind_index.clone());
                files.push(BackedUpFile {
                    file_id: row.version_id,
                    encrypted_name: row.encrypted_name,
                    name_nonce: row.name_nonce,
                    blind_index: row.name_blind_index,
                    path: String::new(),
                    size: row.size,
                    file_updated_at: row.file_updated_at,
                    versions: vec![version_summary],
                });
            }
        }

        Ok(ListBinVersionsResponse { files })
    }
}
