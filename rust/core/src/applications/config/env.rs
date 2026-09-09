use crate::ports::api::{
    backup_config_api_port::BackupConfigApiPort, local_device_api_port::LocalDeviceApiPort,
    remote_storage_api_port::RemoteStorageApiPort,
};

pub trait ConfigEnv {
    type LocalDeviceApi: LocalDeviceApiPort;
    type RemoteStorageApi: RemoteStorageApiPort;
    type BackupConfigApi: BackupConfigApiPort;

    fn local_device_api(&self) -> &Self::LocalDeviceApi;
    fn remote_storage_api(&self) -> &Self::RemoteStorageApi;
    fn backup_config_api(&self) -> &Self::BackupConfigApi;
}
