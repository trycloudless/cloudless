use api_types::backup_job::{
    AbandonStaleJobsResponse, CompleteBackupJobFileRequest, CompleteBackupJobRequest,
    CreateBackupJobFileRequest, CreateBackupJobFileResponse, CreateBackupJobRequest,
    CreateBackupJobResponse, GetBackupJobDetailResponse, GetLatestBackupJobResponse,
    GetResumableBackupJobRequest, GetResumableBackupJobResponse, ListBackupJobsRequest,
    ListBackupJobsResponse, LogCleanupFilesRequest,
};
use uuid::Uuid;

use crate::core::{CoreResult, backup_job::env::BackupJobEnv, ports::BackupJobRepo};

pub async fn create_job<E: BackupJobEnv>(
    env: E,
    user_id: Uuid,
    req: CreateBackupJobRequest,
) -> CoreResult<CreateBackupJobResponse> {
    env.backup_job_repo().create_job(user_id, req).await
}

pub async fn create_job_file<E: BackupJobEnv>(
    env: E,
    user_id: Uuid,
    req: CreateBackupJobFileRequest,
) -> CoreResult<CreateBackupJobFileResponse> {
    env.backup_job_repo().create_job_file(user_id, req).await
}

pub async fn complete_job_file<E: BackupJobEnv>(
    env: E,
    user_id: Uuid,
    req: CompleteBackupJobFileRequest,
) -> CoreResult<()> {
    env.backup_job_repo().complete_job_file(user_id, req).await
}

pub async fn complete_job<E: BackupJobEnv>(
    env: E,
    user_id: Uuid,
    req: CompleteBackupJobRequest,
) -> CoreResult<()> {
    env.backup_job_repo().complete_job(user_id, req).await
}

pub async fn list_jobs<E: BackupJobEnv>(
    env: E,
    user_id: Uuid,
    req: ListBackupJobsRequest,
) -> CoreResult<ListBackupJobsResponse> {
    env.backup_job_repo().list_jobs(user_id, req).await
}

pub async fn get_job_detail<E: BackupJobEnv>(
    env: E,
    user_id: Uuid,
    job_id: Uuid,
) -> CoreResult<GetBackupJobDetailResponse> {
    env.backup_job_repo().get_job_detail(user_id, job_id).await
}

pub async fn get_latest_job<E: BackupJobEnv>(
    env: E,
    user_id: Uuid,
) -> CoreResult<GetLatestBackupJobResponse> {
    env.backup_job_repo().get_latest_job(user_id).await
}

pub async fn get_resumable_job<E: BackupJobEnv>(
    env: E,
    user_id: Uuid,
    req: GetResumableBackupJobRequest,
) -> CoreResult<GetResumableBackupJobResponse> {
    env.backup_job_repo().get_resumable_job(user_id, req).await
}

pub async fn log_cleanup_files<E: BackupJobEnv>(
    env: E,
    user_id: Uuid,
    req: LogCleanupFilesRequest,
) -> CoreResult<()> {
    env.backup_job_repo().log_cleanup_files(user_id, req).await
}

pub async fn abandon_stale_jobs<E: BackupJobEnv>(
    env: E,
    user_id: Uuid,
) -> CoreResult<AbandonStaleJobsResponse> {
    env.backup_job_repo().abandon_stale_jobs(user_id).await
}
