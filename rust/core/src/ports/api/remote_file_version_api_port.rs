use api_types::remote_file_version::{
    CreateFileVersionRequest, CreateFileVersionResponse, ListAllVersionsRequest,
    ListAllVersionsResponse, ListBackedUpFilesRequest, ListBackedUpFilesResponse,
    ListBinVersionsRequest, ListBinVersionsResponse, MoveAllVersionsToBinRequest,
    MoveVersionToBinRequest, RestoreVersionFromBinRequest, UpdateFileVersionStatusRequest,
};
use async_trait::async_trait;

use crate::ports::api::ApiResult;

#[async_trait]
pub trait RemoteFileVersionApiPort: Send + Sync {
    async fn create(
        &self,
        request: CreateFileVersionRequest,
    ) -> ApiResult<CreateFileVersionResponse>;

    async fn update_status(&self, request: UpdateFileVersionStatusRequest) -> ApiResult<()>;

    async fn list_backed_up_files(
        &self,
        request: ListBackedUpFilesRequest,
    ) -> ApiResult<ListBackedUpFilesResponse>;

    async fn list_all_versions(
        &self,
        request: ListAllVersionsRequest,
    ) -> ApiResult<ListAllVersionsResponse>;

    /// Move a single file version to bin (soft-delete).
    async fn move_to_bin(&self, request: MoveVersionToBinRequest) -> ApiResult<()>;

    /// Move all versions of a file (by blind index within a backup config) to bin.
    async fn move_all_to_bin(&self, request: MoveAllVersionsToBinRequest) -> ApiResult<()>;

    /// Restore a single file version from bin.
    async fn restore_from_bin(&self, request: RestoreVersionFromBinRequest) -> ApiResult<()>;

    /// List file versions currently in bin for a backup config.
    async fn list_bin_versions(
        &self,
        request: ListBinVersionsRequest,
    ) -> ApiResult<ListBinVersionsResponse>;
}
