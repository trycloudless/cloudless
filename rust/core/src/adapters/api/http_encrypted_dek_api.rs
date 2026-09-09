use api_types::encrypted_dek::{
    GetEncryptedDekRequest, GetEncryptedDekResponse, StoreEncryptedDekRequest,
    StoreEncryptedDekResponse,
};
use async_trait::async_trait;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, encrypted_dek_api_port::EncryptedDekApiPort},
};

#[derive(Clone)]
pub struct HttpEncryptedDekApi {
    api: HttpApi,
}

impl HttpEncryptedDekApi {
    pub fn new(api: HttpApi) -> Self {
        HttpEncryptedDekApi { api }
    }
}

#[async_trait]
impl EncryptedDekApiPort for HttpEncryptedDekApi {
    async fn store(
        &self,
        request: StoreEncryptedDekRequest,
    ) -> ApiResult<StoreEncryptedDekResponse> {
        let url = self.api.get_url("api/encrypted_dek/store")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn get(&self, request: GetEncryptedDekRequest) -> ApiResult<GetEncryptedDekResponse> {
        let url = self.api.get_url("api/encrypted_dek/get")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }
}
