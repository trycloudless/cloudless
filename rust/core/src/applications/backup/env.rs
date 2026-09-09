use crate::ports::{
    api::{
        backup_config_api_port::BackupConfigApiPort, backup_job_api_port::BackupJobApiPort,
        chunk_api_port::ChunkApiPort, local_device_api_port::LocalDeviceApiPort,
        remote_file_version_api_port::RemoteFileVersionApiPort,
        remote_storage_api_port::RemoteStorageApiPort, user_api_port::UserApiPort,
    },
    local_index::LocalIndexPort,
};

pub trait BackupEnv {
    type UserApi: UserApiPort;
    type BackupConfigApi: BackupConfigApiPort;
    type LocalIndex: LocalIndexPort;
    type LocalDeviceApi: LocalDeviceApiPort;
    type RemoteStorageApi: RemoteStorageApiPort;
    type RemoteFileVersionApi: RemoteFileVersionApiPort;
    type ChunkApi: ChunkApiPort;
    type BackupJobApi: BackupJobApiPort;

    fn clone_env(&self) -> Self
    where
        Self: Sized;

    fn user_api(&self) -> &Self::UserApi;
    fn backup_config_api(&self) -> &Self::BackupConfigApi;
    fn local_index(&self) -> &Self::LocalIndex;
    fn local_device_api(&self) -> &Self::LocalDeviceApi;
    fn remote_storage_api(&self) -> &Self::RemoteStorageApi;
    fn remote_file_version_api(&self) -> &Self::RemoteFileVersionApi;
    fn chunk_api(&self) -> &Self::ChunkApi;
    fn backup_job_api(&self) -> &Self::BackupJobApi;
}

// pub struct SyncEnvImpl<S, E, Comp, Ch, UA> {
//     storage: S,
//     encryptor: E,
//     compressor: Comp,
//     chunker: Ch,
//     user_api: UA,
// }

// impl<S, E, Comp, Ch, UA> SyncEnv for SyncEnvImpl<S, E, Comp, Ch, UA>
// where
//     S: StoragePort,
//     E: Encryptor,
//     Comp: Compressor,
//     Ch: AsyncChunker,
//     UA: UserApiPort,
// {
//     type Storage = S;
//     type Encryptor = E;
//     type Compressor = Comp;
//     type Chunker = Ch;
//     type UserApi = UA;

//     fn storage(&self) -> &Self::Storage {
//         &self.storage
//     }

//     fn encryptor(&self) -> &Self::Encryptor {
//         &self.encryptor
//     }

//     fn compressor(&self) -> &Self::Compressor {
//         &self.compressor
//     }

//     fn chunker(&self) -> &Self::Chunker {
//         &self.chunker
//     }

//     fn user_api(&self) -> &Self::UserApi {
//         &self.user_api
//     }
// }
