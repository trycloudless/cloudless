use crate::core::{CoreResult, ports::RemoteStorageRepo, remote_storage::env::RemoteStorageEnv};
use api_types::remote_storage::{
    CreateRemoteStorageRequest, CreateRemoteStorageResponse, GetRemoteStorageResponse,
    ListRemoteStoragesResponse, ReauthRemoteStorageRequest, ReauthRemoteStorageResponse,
    RemoteStorageStatus, UpdateRemoteStorageStatusRequest, UpdateRemoteStorageStatusResponse,
};
use uuid::Uuid;

pub async fn create_remote_storage<E: RemoteStorageEnv>(
    env: E,
    user_id: Uuid,
    payload: CreateRemoteStorageRequest,
) -> CoreResult<CreateRemoteStorageResponse> {
    let storage = env.remote_storage_repo().create(user_id, payload).await?;
    Ok(storage)
}

pub async fn get_by_id<E: RemoteStorageEnv>(
    env: E,
    user_id: Uuid,
    id: Uuid,
) -> CoreResult<GetRemoteStorageResponse> {
    let storage = env.remote_storage_repo().get_by_id(user_id, id).await?;
    Ok(GetRemoteStorageResponse { storage })
}

pub async fn list_by_user<E: RemoteStorageEnv>(
    env: E,
    user_id: Uuid,
) -> CoreResult<ListRemoteStoragesResponse> {
    let list = env.remote_storage_repo().list_by_user(user_id).await?;
    Ok(ListRemoteStoragesResponse { list })
}

/// Updates the status of a remote storage (e.g. mark as AuthTokenExpired
/// when a token refresh fails with invalid_grant).
pub async fn update_status<E: RemoteStorageEnv>(
    env: E,
    user_id: Uuid,
    payload: UpdateRemoteStorageStatusRequest,
) -> CoreResult<UpdateRemoteStorageStatusResponse> {
    env.remote_storage_repo()
        .update_status(user_id, payload.id, payload.status)
        .await?;
    Ok(UpdateRemoteStorageStatusResponse { success: true })
}

/// Reauth a remote storage with new encrypted credentials.
/// Identity verification is done client-side; the server stores the new
/// encrypted config blob and resets status to Active.
pub async fn reauth<E: RemoteStorageEnv>(
    env: E,
    user_id: Uuid,
    payload: ReauthRemoteStorageRequest,
) -> CoreResult<ReauthRemoteStorageResponse> {
    env.remote_storage_repo()
        .update_config(
            user_id,
            payload.id,
            payload.config,
            RemoteStorageStatus::Active,
        )
        .await?;
    Ok(ReauthRemoteStorageResponse { success: true })
}
