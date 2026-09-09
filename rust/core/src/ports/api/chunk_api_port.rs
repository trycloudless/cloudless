use api_types::{
    chunk::{CreateChunkRequest, CreateChunkResponse, UpdateChunkStorageMetaRequest},
    restore_file_info::{GetFileVersionChunksRequest, GetFileVersionChunksResponse},
};
use async_trait::async_trait;

use crate::ports::api::ApiResult;

#[async_trait]
pub trait ChunkApiPort: Send + Sync {
    async fn create(&self, chunk: CreateChunkRequest) -> ApiResult<CreateChunkResponse>;

    /// Fetches the ordered list of chunks for a file version.
    /// Used during restore to know which chunks to download.
    async fn get_chunks_for_version(
        &self,
        req: GetFileVersionChunksRequest,
    ) -> ApiResult<GetFileVersionChunksResponse>;

    /// Updates a chunk's storage metadata after re-encryption.
    async fn update_storage_meta(&self, req: UpdateChunkStorageMetaRequest) -> ApiResult<()>;
}
