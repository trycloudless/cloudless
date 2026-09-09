use api_types::remote_storage::RemoteStorageConfig;
use uuid::Uuid;

use crate::{
    adapters::{
        google_drive_storage_adaptor::GoogleDriveStorageAdaptor,
        local_fs_storage_adaptor::LocalFsStorageAdaptor,
        onedrive_storage_adaptor::OneDriveStorageAdaptor, s3_storage_adaptor::S3StorageAdaptor,
        sftp_storage_adaptor::SftpStorageAdaptor,
    },
    model::base::AppResult,
    ports::storage::StoragePort,
};

/// Selects how a storage adapter should interpret object keys for restore.
pub enum StorageLayout {
    BackupChunks,
    RestoredFiles,
}

/// Builds a storage port from decrypted storage config for either backup chunks
/// or restored plaintext output.
pub async fn create_storage_port(
    config: &RemoteStorageConfig,
    storage_id: Uuid,
    layout: StorageLayout,
) -> AppResult<Box<dyn StoragePort>> {
    match config {
        RemoteStorageConfig::Aws(s3_creds) => {
            Ok(Box::new(S3StorageAdaptor::new(s3_creds.clone()).await?))
        }
        RemoteStorageConfig::GoogleDrive(gdrive_creds) => match layout {
            StorageLayout::BackupChunks => Ok(Box::new(
                GoogleDriveStorageAdaptor::new(gdrive_creds.clone(), storage_id).await?,
            )),
            StorageLayout::RestoredFiles => Ok(Box::new(
                GoogleDriveStorageAdaptor::new_for_restored_files(gdrive_creds.clone(), storage_id)
                    .await?,
            )),
        },
        RemoteStorageConfig::OneDrive(onedrive_creds) => match layout {
            StorageLayout::BackupChunks => Ok(Box::new(
                OneDriveStorageAdaptor::new(onedrive_creds.clone(), storage_id).await?,
            )),
            StorageLayout::RestoredFiles => Ok(Box::new(
                OneDriveStorageAdaptor::new_for_restored_files(onedrive_creds.clone(), storage_id)
                    .await?,
            )),
        },
        RemoteStorageConfig::LocalFilesystem(cfg) => {
            Ok(Box::new(LocalFsStorageAdaptor::new(&cfg.root_path)?))
        }
        RemoteStorageConfig::Sftp(sftp_creds) => {
            Ok(Box::new(SftpStorageAdaptor::new(sftp_creds.clone())?))
        }
    }
}
