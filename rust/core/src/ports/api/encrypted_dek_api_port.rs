use api_types::encrypted_dek::{
    GetEncryptedDekRequest, GetEncryptedDekResponse, StoreEncryptedDekRequest,
    StoreEncryptedDekResponse,
};
use async_trait::async_trait;

use crate::ports::api::ApiResult;

#[async_trait]
pub trait EncryptedDekApiPort: Send + Sync {
    async fn store(
        &self,
        request: StoreEncryptedDekRequest,
    ) -> ApiResult<StoreEncryptedDekResponse>;
    async fn get(&self, request: GetEncryptedDekRequest) -> ApiResult<GetEncryptedDekResponse>;
}
