use api_types::{
    chunk::{ChunkObjectStoreMeta, ChunkStorageMeta},
    gc::{
        ConfirmChunkDeletionsRequest, ConfirmChunkDeletionsResponse, GcCollectResponse,
        GcRunChunkInfo, GcRunDetailResponse, GcRunStatus, GcRunSummary, GcRunVersionInfo,
        GetRetentionSettingsResponse, ListGcRunsResponse, OrphanedChunkInfo,
    },
    remote_file_version::FileVersionStatus,
};
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    core::{CoreError, CoreResult, ports::GcRepo},
    infra::psql::map_sqlx_error,
};

/// PostgreSQL implementation of the garbage collection repository.
#[derive(Clone)]
pub struct PgGcRepo {
    pub pool: PgPool,
}

impl PgGcRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GcRepo for PgGcRepo {
    /// Phase A: Find expired MovedToBin versions, mark them Deleted, record audit trail,
    /// find orphaned chunks, and return them for client-side S3 deletion.
    async fn collect_expired_versions(&self, user_id: Uuid) -> CoreResult<GcCollectResponse> {
        let gc_run_id = Uuid::now_v7();

        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;

        // Serialize concurrent GC runs for this user only — GC runs per-user (see
        // gc_route.rs), not as a single global process, so the lock must be keyed
        // per-user rather than using one global key that would wrongly serialize
        // every user's GC through a single lock.
        sqlx::query!(
            r#"SELECT pg_advisory_xact_lock(hashtextextended($1, 0))"#,
            format!("gc:{user_id}"),
        )
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        // Create the gc_run record.
        sqlx::query!(
            r#"
            INSERT INTO gc_runs (id, user_id, status, started_at)
            VALUES ($1, $2, 'running', now())
            "#,
            gc_run_id,
            user_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        // Find expired MovedToBin versions.
        // The moved-to-bin timestamp is the last entry in status_history where status = "MovedToBin".
        // status_history is a JSONB array of [timestamp, status] pairs.
        // We find the latest "MovedToBin" entry and check if it's older than bin_retention_days.
        let expired_rows = sqlx::query!(
            r#"
            SELECT rfv.id, rfv.encrypted_name, rfv.name_nonce, rfv.version, rfv.size
            FROM remote_file_versions rfv
            JOIN users u ON u.id = rfv.user_id
            WHERE rfv.user_id = $1
              AND rfv.status = '"MovedToBin"'
              AND (
                  SELECT MAX((elem->>0)::timestamptz)
                  FROM jsonb_array_elements(rfv.status_history) AS elem
                  WHERE elem->1 = '"MovedToBin"'
              ) < now() - (u.bin_retention_days || ' days')::interval
            "#,
            user_id,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        let versions_deleted = expired_rows.len() as u64;
        let expired_version_ids: Vec<Uuid> = expired_rows.iter().map(|r| r.id).collect();

        // Record expired versions in gc_run_versions for audit trail.
        for row in &expired_rows {
            let audit_id = Uuid::now_v7();
            sqlx::query!(
                r#"
                INSERT INTO gc_run_versions (id, gc_run_id, file_version_id, encrypted_name, name_nonce, version, size)
                VALUES ($1, $2, $3, $4, $5, $6, $7)
                "#,
                audit_id,
                gc_run_id,
                row.id,
                &row.encrypted_name,
                &row.name_nonce,
                row.version,
                row.size,
            )
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;
        }

        if !expired_version_ids.is_empty() {
            // Delete junction table entries for expired versions.
            sqlx::query!(
                r#"
                DELETE FROM remote_file_version_chunks
                WHERE remote_file_version_id = ANY($1)
                "#,
                &expired_version_ids,
            )
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;

            // Mark expired versions as Deleted.
            let deleted_status = serde_json::to_value(&FileVersionStatus::Deleted)
                .map_err(|e| CoreError::internal_with_source("Failed to serialize status", e))?;

            sqlx::query!(
                r#"
                UPDATE remote_file_versions
                SET status = $1,
                    status_history = status_history || jsonb_build_array(jsonb_build_array(to_jsonb(now()), $1::jsonb))
                WHERE id = ANY($2) AND user_id = $3
                "#,
                deleted_status,
                &expired_version_ids,
                user_id,
            )
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;
        }

        // Find orphaned chunks (no remaining junction entries).
        let orphaned_rows = sqlx::query!(
            r#"
            SELECT c.id, c.storage_id, c.size, c.storage_meta
            FROM chunks c
            WHERE c.user_id = $1
              AND NOT EXISTS (
                  SELECT 1 FROM remote_file_version_chunks rvc
                  WHERE rvc.chunk_id = c.id
              )
            "#,
            user_id,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        // Record orphaned chunks in gc_run_chunks for audit trail.
        let mut orphaned_chunks = Vec::with_capacity(orphaned_rows.len());
        for row in &orphaned_rows {
            let audit_id = Uuid::now_v7();
            sqlx::query!(
                r#"
                INSERT INTO gc_run_chunks (id, gc_run_id, chunk_id, storage_id, size, deleted_from_storage)
                VALUES ($1, $2, $3, $4, $5, false)
                "#,
                audit_id,
                gc_run_id,
                row.id,
                row.storage_id,
                row.size,
            )
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;

            let storage_meta: ChunkStorageMeta = serde_json::from_value(row.storage_meta.clone())
                .unwrap_or_else(|e| {
                    tracing::error!(
                        chunk_id = %row.id,
                        error = %e,
                        "Failed to deserialize chunk storage_meta, falling back to empty"
                    );
                    ChunkStorageMeta::ObjectStore(ChunkObjectStoreMeta {
                        key: String::new(),
                        hash: String::new(),
                        encryption: None,
                    })
                });

            orphaned_chunks.push(OrphanedChunkInfo {
                chunk_id: row.id,
                storage_id: row.storage_id,
                size: row.size,
                storage_meta,
            });
        }

        // Update gc_run with versions_deleted count.
        sqlx::query!(
            r#"
            UPDATE gc_runs
            SET versions_deleted = $1
            WHERE id = $2
            "#,
            versions_deleted as i32,
            gc_run_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        tx.commit().await.map_err(map_sqlx_error)?;

        Ok(GcCollectResponse {
            gc_run_id,
            versions_deleted,
            orphaned_chunks,
        })
    }

    /// Phase C: Confirm that orphaned chunks have been deleted from S3.
    /// Deletes chunk rows from DB (with safety re-check), updates gc_run record.
    async fn confirm_chunk_deletions(
        &self,
        user_id: Uuid,
        request: ConfirmChunkDeletionsRequest,
    ) -> CoreResult<ConfirmChunkDeletionsResponse> {
        if request.chunk_ids.is_empty() {
            // No chunks to confirm — just finalize the gc_run.
            sqlx::query!(
                r#"
                UPDATE gc_runs
                SET status = 'completed', completed_at = now()
                WHERE id = $1 AND user_id = $2
                "#,
                request.gc_run_id,
                user_id,
            )
            .execute(&self.pool)
            .await
            .map_err(map_sqlx_error)?;

            return Ok(ConfirmChunkDeletionsResponse {
                chunks_deleted: 0,
                storage_freed_bytes: 0,
            });
        }

        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;

        // Per-user lock — see collect_expired_versions for why this must not be a global key.
        sqlx::query!(
            r#"SELECT pg_advisory_xact_lock(hashtextextended($1, 0))"#,
            format!("gc:{user_id}"),
        )
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        // Safety re-check: only delete chunks that truly have no remaining junction entries.
        // This protects against race conditions (e.g., new version uploaded referencing same chunk).
        let deletable_rows = sqlx::query!(
            r#"
            SELECT c.id, c.size
            FROM chunks c
            WHERE c.id = ANY($1)
              AND c.user_id = $2
              AND NOT EXISTS (
                  SELECT 1 FROM remote_file_version_chunks rvc
                  WHERE rvc.chunk_id = c.id
              )
            "#,
            &request.chunk_ids,
            user_id,
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        let chunks_deleted = deletable_rows.len() as u64;
        let storage_freed_bytes: i64 = deletable_rows.iter().map(|r| r.size as i64).sum();
        let deletable_ids: Vec<Uuid> = deletable_rows.iter().map(|r| r.id).collect();

        if !deletable_ids.is_empty() {
            // Delete chunk rows from DB.
            sqlx::query!(
                r#"
                DELETE FROM chunks
                WHERE id = ANY($1) AND user_id = $2
                "#,
                &deletable_ids,
                user_id,
            )
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;

            // Mark gc_run_chunks as deleted_from_storage.
            sqlx::query!(
                r#"
                UPDATE gc_run_chunks
                SET deleted_from_storage = true
                WHERE gc_run_id = $1 AND chunk_id = ANY($2)
                "#,
                request.gc_run_id,
                &deletable_ids,
            )
            .execute(&mut *tx)
            .await
            .map_err(map_sqlx_error)?;
        }

        // Finalize the gc_run record.
        sqlx::query!(
            r#"
            UPDATE gc_runs
            SET chunks_deleted = $1,
                storage_freed_bytes = $2,
                status = 'completed',
                completed_at = now()
            WHERE id = $3 AND user_id = $4
            "#,
            chunks_deleted as i32,
            storage_freed_bytes,
            request.gc_run_id,
            user_id,
        )
        .execute(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        tx.commit().await.map_err(map_sqlx_error)?;

        Ok(ConfirmChunkDeletionsResponse {
            chunks_deleted,
            storage_freed_bytes,
        })
    }

    /// List recent GC runs for a user, ordered by most recent first.
    async fn list_gc_runs(&self, user_id: Uuid, limit: i32) -> CoreResult<ListGcRunsResponse> {
        let rows = sqlx::query!(
            r#"
            SELECT id, status, versions_deleted, chunks_deleted,
                   storage_freed_bytes, error_message, started_at, completed_at
            FROM gc_runs
            WHERE user_id = $1
            ORDER BY started_at DESC
            LIMIT $2
            "#,
            user_id,
            limit as i64,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let runs = rows
            .into_iter()
            .map(|r| GcRunSummary {
                id: r.id,
                status: parse_gc_run_status(&r.status),
                versions_deleted: r.versions_deleted,
                chunks_deleted: r.chunks_deleted,
                storage_freed_bytes: r.storage_freed_bytes,
                error_message: r.error_message,
                started_at: r.started_at,
                completed_at: r.completed_at,
            })
            .collect();

        Ok(ListGcRunsResponse { runs })
    }

    /// Get detailed info for a specific GC run including processed versions and chunks.
    async fn get_gc_run_detail(
        &self,
        user_id: Uuid,
        gc_run_id: Uuid,
    ) -> CoreResult<GcRunDetailResponse> {
        // Fetch the run summary.
        let run_row = sqlx::query!(
            r#"
            SELECT id, status, versions_deleted, chunks_deleted,
                   storage_freed_bytes, error_message, started_at, completed_at
            FROM gc_runs
            WHERE id = $1 AND user_id = $2
            "#,
            gc_run_id,
            user_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?
        .ok_or_else(|| CoreError::not_found("GC run not found"))?;

        let run = GcRunSummary {
            id: run_row.id,
            status: parse_gc_run_status(&run_row.status),
            versions_deleted: run_row.versions_deleted,
            chunks_deleted: run_row.chunks_deleted,
            storage_freed_bytes: run_row.storage_freed_bytes,
            error_message: run_row.error_message,
            started_at: run_row.started_at,
            completed_at: run_row.completed_at,
        };

        // Fetch processed versions.
        let version_rows = sqlx::query!(
            r#"
            SELECT file_version_id, encrypted_name, name_nonce, version, size
            FROM gc_run_versions
            WHERE gc_run_id = $1
            "#,
            gc_run_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let versions = version_rows
            .into_iter()
            .map(|r| GcRunVersionInfo {
                file_version_id: r.file_version_id,
                encrypted_name: r.encrypted_name,
                name_nonce: r.name_nonce,
                version: r.version,
                size: r.size,
            })
            .collect();

        // Fetch processed chunks.
        let chunk_rows = sqlx::query!(
            r#"
            SELECT chunk_id, storage_id, size, deleted_from_storage
            FROM gc_run_chunks
            WHERE gc_run_id = $1
            "#,
            gc_run_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let chunks = chunk_rows
            .into_iter()
            .map(|r| GcRunChunkInfo {
                chunk_id: r.chunk_id,
                storage_id: r.storage_id,
                size: r.size,
                deleted_from_storage: r.deleted_from_storage,
            })
            .collect();

        Ok(GcRunDetailResponse {
            run,
            versions,
            chunks,
        })
    }

    /// Get the user's bin retention setting.
    async fn get_retention_settings(
        &self,
        user_id: Uuid,
    ) -> CoreResult<GetRetentionSettingsResponse> {
        let row = sqlx::query!(
            r#"
            SELECT bin_retention_days FROM users WHERE id = $1
            "#,
            user_id,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?
        .ok_or_else(|| CoreError::not_found("User not found"))?;

        Ok(GetRetentionSettingsResponse {
            bin_retention_days: row.bin_retention_days,
        })
    }

    /// Update the user's bin retention period.
    async fn update_retention_settings(
        &self,
        user_id: Uuid,
        bin_retention_days: i32,
    ) -> CoreResult<()> {
        let result = sqlx::query!(
            r#"
            UPDATE users SET bin_retention_days = $1 WHERE id = $2
            "#,
            bin_retention_days,
            user_id,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            return Err(CoreError::not_found("User not found"));
        }

        Ok(())
    }
}

/// Parse GC run status string from the database.
fn parse_gc_run_status(status: &str) -> GcRunStatus {
    match status {
        "running" => GcRunStatus::Running,
        "completed" => GcRunStatus::Completed,
        "failed" => GcRunStatus::Failed,
        other => {
            tracing::warn!(
                status = other,
                "Unknown GC run status, defaulting to Failed"
            );
            GcRunStatus::Failed
        }
    }
}
