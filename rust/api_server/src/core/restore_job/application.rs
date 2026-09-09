use api_types::restore_job::{
    CompleteRestoreJobFileRequest, CompleteRestoreJobRequest, CreateRestoreJobFileRequest,
    CreateRestoreJobFileResponse, CreateRestoreJobRequest, CreateRestoreJobResponse,
    GetLatestRestoreJobResponse, GetRestoreJobDetailResponse, GetResumableRestoreJobResponse,
    ListRestoreJobsRequest, ListRestoreJobsResponse,
};
use uuid::Uuid;

use crate::core::{CoreResult, ports::RestoreJobRepo, restore_job::env::RestoreJobEnv};

pub async fn create_job<E: RestoreJobEnv>(
    env: E,
    user_id: Uuid,
    req: CreateRestoreJobRequest,
) -> CoreResult<CreateRestoreJobResponse> {
    env.restore_job_repo().create_job(user_id, req).await
}

pub async fn create_job_file<E: RestoreJobEnv>(
    env: E,
    user_id: Uuid,
    req: CreateRestoreJobFileRequest,
) -> CoreResult<CreateRestoreJobFileResponse> {
    env.restore_job_repo().create_job_file(user_id, req).await
}

pub async fn complete_job_file<E: RestoreJobEnv>(
    env: E,
    user_id: Uuid,
    req: CompleteRestoreJobFileRequest,
) -> CoreResult<()> {
    env.restore_job_repo().complete_job_file(user_id, req).await
}

pub async fn complete_job<E: RestoreJobEnv>(
    env: E,
    user_id: Uuid,
    req: CompleteRestoreJobRequest,
) -> CoreResult<()> {
    env.restore_job_repo().complete_job(user_id, req).await
}

pub async fn list_jobs<E: RestoreJobEnv>(
    env: E,
    user_id: Uuid,
    req: ListRestoreJobsRequest,
) -> CoreResult<ListRestoreJobsResponse> {
    env.restore_job_repo().list_jobs(user_id, req).await
}

pub async fn get_job_detail<E: RestoreJobEnv>(
    env: E,
    user_id: Uuid,
    job_id: Uuid,
) -> CoreResult<GetRestoreJobDetailResponse> {
    env.restore_job_repo().get_job_detail(user_id, job_id).await
}

pub async fn get_latest_job<E: RestoreJobEnv>(
    env: E,
    user_id: Uuid,
) -> CoreResult<GetLatestRestoreJobResponse> {
    env.restore_job_repo().get_latest_job(user_id).await
}

pub async fn get_resumable_job<E: RestoreJobEnv>(
    env: E,
    user_id: Uuid,
) -> CoreResult<GetResumableRestoreJobResponse> {
    env.restore_job_repo().get_resumable_job(user_id).await
}
