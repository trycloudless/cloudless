use crate::ports::api::{
    backup_config_api_port::BackupConfigApiPort, chunk_api_port::ChunkApiPort,
    remote_file_version_api_port::RemoteFileVersionApiPort,
    remote_storage_api_port::RemoteStorageApiPort, restore_job_api_port::RestoreJobApiPort,
};

pub trait RestoreEnv {
    type BackupConfigApi: BackupConfigApiPort;
    type RemoteStorageApi: RemoteStorageApiPort;
    type RemoteFileVersionApi: RemoteFileVersionApiPort;
    type ChunkApi: ChunkApiPort;
    type RestoreJobApi: RestoreJobApiPort;

    fn clone_env(&self) -> Self
    where
        Self: Sized;

    fn backup_config_api(&self) -> &Self::BackupConfigApi;
    fn remote_storage_api(&self) -> &Self::RemoteStorageApi;
    fn remote_file_version_api(&self) -> &Self::RemoteFileVersionApi;
    fn chunk_api(&self) -> &Self::ChunkApi;
    fn restore_job_api(&self) -> &Self::RestoreJobApi;
}
