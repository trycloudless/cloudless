use crate::ports::api::{gc_api_port::GcApiPort, remote_storage_api_port::RemoteStorageApiPort};

pub trait GcEnv {
    type GcApi: GcApiPort;
    type RemoteStorageApi: RemoteStorageApiPort;

    fn gc_api(&self) -> &Self::GcApi;
    fn remote_storage_api(&self) -> &Self::RemoteStorageApi;
}
