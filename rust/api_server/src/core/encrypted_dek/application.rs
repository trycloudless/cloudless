use crate::core::{CoreResult, encrypted_dek::env::EncryptedDekEnv, ports::EncryptedDekRepo};
use api_types::encrypted_dek::{
    GetEncryptedDekRequest, GetEncryptedDekResponse, StoreEncryptedDekRequest,
    StoreEncryptedDekResponse,
};
use uuid::Uuid;

pub async fn store<E: EncryptedDekEnv>(
    env: E,
    user_id: Uuid,
    request: StoreEncryptedDekRequest,
) -> CoreResult<StoreEncryptedDekResponse> {
    let response = env.encrypted_dek_repo().store(user_id, request).await?;
    Ok(response)
}

pub async fn get<E: EncryptedDekEnv>(
    env: E,
    user_id: Uuid,
    request: GetEncryptedDekRequest,
) -> CoreResult<GetEncryptedDekResponse> {
    let response = env
        .encrypted_dek_repo()
        .get(user_id, request.key_type)
        .await?;
    Ok(response)
}
