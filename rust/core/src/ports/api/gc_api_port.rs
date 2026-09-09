use api_types::gc::{
    ConfirmChunkDeletionsRequest, ConfirmChunkDeletionsResponse, GcCollectResponse,
    GcRunDetailResponse, GetGcRunDetailRequest, GetRetentionSettingsResponse, ListGcRunsRequest,
    ListGcRunsResponse, UpdateRetentionSettingsRequest,
};
use async_trait::async_trait;

use crate::ports::api::ApiResult;

/// Port for garbage collection API calls.
#[async_trait]
pub trait GcApiPort: Send + Sync {
    /// Trigger GC collection: marks expired bin versions as Deleted and returns orphaned chunks.
    async fn collect(&self) -> ApiResult<GcCollectResponse>;

    /// Confirm that orphaned chunks have been deleted from storage.
    async fn confirm_chunk_deletions(
        &self,
        request: ConfirmChunkDeletionsRequest,
    ) -> ApiResult<ConfirmChunkDeletionsResponse>;

    /// List recent GC runs for the authenticated user.
    async fn list_runs(&self, request: ListGcRunsRequest) -> ApiResult<ListGcRunsResponse>;

    /// Get detailed info for a specific GC run.
    async fn get_run_detail(
        &self,
        request: GetGcRunDetailRequest,
    ) -> ApiResult<GcRunDetailResponse>;

    /// Get the user's bin retention settings.
    async fn get_retention_settings(&self) -> ApiResult<GetRetentionSettingsResponse>;

    /// Update the user's bin retention period.
    async fn update_retention_settings(
        &self,
        request: UpdateRetentionSettingsRequest,
    ) -> ApiResult<()>;
}
