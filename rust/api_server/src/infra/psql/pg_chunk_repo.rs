use api_types::{
    chunk::{
        ChunkObjectStoreMeta, ChunkStorageMeta, CreateChunkRequest, CreateChunkResponse,
        UpdateChunkStorageMetaRequest,
    },
    restore_file_info::{
        GetFileVersionChunksRequest, GetFileVersionChunksResponse, RestoreChunkInfo,
    },
};
use async_trait::async_trait;
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
    core::{CoreError, CoreResult, ports::ChunkRepo},
    infra::psql::{fingerprint_request, map_ownership_violation, map_sqlx_error},
};

#[derive(Clone)]
pub struct PgChunkRepo {
    pub pool: PgPool,
}

impl PgChunkRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ChunkRepo for PgChunkRepo {
    async fn create(
        &self,
        user_id: Uuid,
        chunk: CreateChunkRequest,
    ) -> CoreResult<CreateChunkResponse> {
        let id = Uuid::now_v7();
        let has_encryption = match &chunk.storage_meta {
            api_types::chunk::ChunkStorageMeta::ObjectStore(s3) => s3.encryption.is_some(),
        };
        tracing::debug!(
            chunk_size = chunk.size,
            has_encryption,
            "Creating chunk record"
        );

        let fingerprint = fingerprint_request(&chunk)?;

        let mut tx = self.pool.begin().await.map_err(map_sqlx_error)?;

        // Idempotency check, same shape as file-version creation: same key + same
        // body replays the stored response; same key + a different body is a conflict.
        let existing = sqlx::query!(
            r#"
            SELECT request_fingerprint, response_snapshot
            FROM chunk_idempotency_keys
            WHERE idempotency_key = $1
            FOR UPDATE
            "#,
            chunk.idempotency_key,
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
            let response: CreateChunkResponse = serde_json::from_value(existing.response_snapshot)
                .map_err(|e| {
                    CoreError::internal_with_source(
                        "Failed to deserialize stored idempotency response",
                        e,
                    )
                })?;
            tx.commit().await.map_err(map_sqlx_error)?;
            return Ok(response);
        }

        let storage_meta = serde_json::to_value(&chunk.storage_meta).map_err(|e| {
            CoreError::internal_with_source("Failed to serialize storage_meta", e)
        })?;
        let status_history = serde_json::to_value(&chunk.status_history).map_err(|e| {
            CoreError::internal_with_source("Failed to serialize status_history", e)
        })?;

        // Insert into chunks table with dedup. On conflict (same hash+user+storage),
        // keep the existing row intact so we never overwrite encryption metadata.
        sqlx::query!(
            r#"
            INSERT INTO chunks (id, hash, size, user_id, storage_id, storage_meta, status_history)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (hash, user_id, storage_id) DO NOTHING
            "#,
            id,
            &chunk.hash,
            chunk.size as i32,
            user_id,
            chunk.storage_id,
            storage_meta,
            status_history
        )
        .execute(&mut *tx)
        .await
        .map_err(map_ownership_violation)?;

        // Retrieve the chunk id (either the newly inserted row or the existing one)
        let row = sqlx::query!(
            r#"
            SELECT id FROM chunks
            WHERE hash = $1 AND user_id = $2 AND storage_id = $3
            "#,
            &chunk.hash,
            user_id,
            chunk.storage_id
        )
        .fetch_one(&mut *tx)
        .await
        .map_err(map_sqlx_error)?;

        // If the returned id differs from the one we tried to insert,
        // the chunk already existed in the DB (ON CONFLICT DO NOTHING kept it).
        let existed_in_db = row.id != id;

        // Insert into remote_file_version_chunks junction table. Unlike the chunk
        // dedup above, a conflict here is NOT silently kept: if a different chunk
        // is already registered at this exact (version, index) slot, the file content
        // must have changed between attempts, so it's surfaced instead of silently
        // preserving stale data.
        let inserted = sqlx::query!(
            r#"
            INSERT INTO remote_file_version_chunks (remote_file_version_id, chunk_id, chunk_index, user_id)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (remote_file_version_id, chunk_index) DO NOTHING
            RETURNING chunk_id
            "#,
            chunk.remote_file_version_id,
            row.id,
            chunk.chunk_index,
            user_id,
        )
        .fetch_optional(&mut *tx)
        .await
        .map_err(map_ownership_violation)?;

        if inserted.is_none() {
            let existing_chunk_id = sqlx::query!(
                r#"
                SELECT chunk_id FROM remote_file_version_chunks
                WHERE remote_file_version_id = $1 AND chunk_index = $2
                "#,
                chunk.remote_file_version_id,
                chunk.chunk_index,
            )
            .fetch_one(&mut *tx)
            .await
            .map_err(map_sqlx_error)?
            .chunk_id;

            if existing_chunk_id != row.id {
                return Err(CoreError::conflict(
                    "chunk_index already registered with a different chunk for this file version",
                ));
            }
        }

        let response = CreateChunkResponse {
            id: row.id,
            existed_in_db,
        };
        let response_snapshot = serde_json::to_value(&response).map_err(|e| {
            CoreError::internal_with_source("Failed to serialize idempotency response snapshot", e)
        })?;

        sqlx::query!(
            r#"
            INSERT INTO chunk_idempotency_keys
                (idempotency_key, user_id, request_fingerprint, response_snapshot)
            VALUES ($1, $2, $3, $4)
            "#,
            chunk.idempotency_key,
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

    /// Fetches all chunks for a given file version, ordered by chunk_index.
    /// Joins remote_file_version_chunks with chunks to return storage metadata
    /// needed for downloading during restore.
    async fn get_chunks_for_version(
        &self,
        user_id: Uuid,
        request: GetFileVersionChunksRequest,
    ) -> CoreResult<GetFileVersionChunksResponse> {
        let rows = sqlx::query!(
            r#"
            SELECT c.id, rvc.chunk_index, c.hash, c.size, c.storage_meta
            FROM remote_file_version_chunks rvc
            JOIN chunks c ON c.id = rvc.chunk_id
            WHERE rvc.remote_file_version_id = $1 AND c.user_id = $2
            ORDER BY rvc.chunk_index
            "#,
            request.version_id,
            user_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        let chunks = rows
            .into_iter()
            .map(|r| {
                let storage_meta: ChunkStorageMeta = serde_json::from_value(r.storage_meta.clone())
                    .unwrap_or_else(|e| {
                        tracing::error!(
                            chunk_id = %r.id,
                            error = %e,
                            raw_meta = ?r.storage_meta,
                            "Failed to deserialize chunk storage_meta, falling back to empty"
                        );
                        ChunkStorageMeta::ObjectStore(ChunkObjectStoreMeta {
                            key: String::new(),
                            hash: String::new(),
                            encryption: None,
                        })
                    });
                RestoreChunkInfo {
                    chunk_id: r.id,
                    chunk_index: r.chunk_index,
                    hash: r.hash,
                    size: r.size as i64,
                    storage_meta,
                }
            })
            .collect();

        Ok(GetFileVersionChunksResponse { chunks })
    }

    async fn update_storage_meta(
        &self,
        user_id: Uuid,
        request: UpdateChunkStorageMetaRequest,
    ) -> CoreResult<()> {
        let storage_meta = serde_json::to_value(&request.storage_meta).map_err(|e| {
            crate::core::CoreError::internal_with_source("Failed to serialize storage_meta", e)
        })?;

        let result = sqlx::query!(
            r#"
            UPDATE chunks
            SET storage_meta = $1
            WHERE id = $2 AND user_id = $3
            "#,
            storage_meta,
            request.chunk_id,
            user_id,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        if result.rows_affected() == 0 {
            return Err(crate::core::CoreError::not_found("Chunk not found"));
        }

        Ok(())
    }
}
