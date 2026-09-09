use crate::{
    adapters::{
        api::{
            http_api_with_auth::HttpApi, http_backup_config_api::HttpBackupConfigApi,
            http_backup_job_api::HttpBackupJobApi, http_chunk_api::HttpChunkApi,
            http_dashboard_api::HttpDashboardApi, http_encrypted_dek_api::HttpEncryptedDekApi,
            http_gc_api::HttpGcApi, http_local_device_api::HttpLocalDeviceApi,
            http_policy_api::HttpPolicyApi, http_remote_file_version_api::HttpRemoteFileVersionApi,
            http_remote_storage_api::HttpRemoteStorageApi, http_restore_job_api::HttpRestoreJobApi,
            http_security_event_api::HttpSecurityEventApi,
            http_subscription_api::HttpSubscriptionApi, http_user_api::HttpUserApi,
        },
        sqlite_local_index::SqliteLocalIndex,
    },
    applications::{
        backup::env::BackupEnv, config::env::ConfigEnv, dashboard::env::DashboardEnv,
        gc::env::GcEnv, restore::env::RestoreEnv, user::env::UserEnv,
    },
};

#[derive(Clone)]
pub struct AppEnv {
    user_api: HttpUserApi,
    backup_config_api: HttpBackupConfigApi,
    local_index: SqliteLocalIndex,
    local_device_api: HttpLocalDeviceApi,
    remote_storage_api: HttpRemoteStorageApi,
    remote_file_version_api: HttpRemoteFileVersionApi,
    chunk_api: HttpChunkApi,
    dashboard_api: HttpDashboardApi,
    backup_job_api: HttpBackupJobApi,
    restore_job_api: HttpRestoreJobApi,
    encrypted_dek_api: HttpEncryptedDekApi,
    security_event_api: HttpSecurityEventApi,
    subscription_api: HttpSubscriptionApi,
    policy_api: HttpPolicyApi,
    gc_api: HttpGcApi,
}

impl AppEnv {
    pub fn new(base_url: url::Url, local_index: SqliteLocalIndex) -> Self {
        let http_api = HttpApi::new(base_url);
        let user_api = HttpUserApi::new(http_api.clone());
        let backup_config_api = HttpBackupConfigApi::new(http_api.clone());
        let local_device_api = HttpLocalDeviceApi::new(http_api.clone());
        let remote_storage_api = HttpRemoteStorageApi::new(http_api.clone());
        let remote_file_version_api = HttpRemoteFileVersionApi::new(http_api.clone());
        let chunk_api = HttpChunkApi::new(http_api.clone());
        let dashboard_api = HttpDashboardApi::new(http_api.clone());
        let backup_job_api = HttpBackupJobApi::new(http_api.clone());
        let restore_job_api = HttpRestoreJobApi::new(http_api.clone());
        let encrypted_dek_api = HttpEncryptedDekApi::new(http_api.clone());
        let security_event_api = HttpSecurityEventApi::new(http_api.clone());
        let subscription_api = HttpSubscriptionApi::new(http_api.clone());
        let policy_api = HttpPolicyApi::new(http_api.clone());
        let gc_api = HttpGcApi::new(http_api.clone());
        AppEnv {
            user_api,
            backup_config_api,
            local_index,
            local_device_api,
            remote_storage_api,
            remote_file_version_api,
            chunk_api,
            dashboard_api,
            backup_job_api,
            restore_job_api,
            encrypted_dek_api,
            security_event_api,
            subscription_api,
            policy_api,
            gc_api,
        }
    }

    pub fn subscription_api(&self) -> &HttpSubscriptionApi {
        &self.subscription_api
    }

    pub fn encrypted_dek_api(&self) -> &HttpEncryptedDekApi {
        &self.encrypted_dek_api
    }

    pub fn security_event_api(&self) -> &HttpSecurityEventApi {
        &self.security_event_api
    }

    pub fn policy_api(&self) -> &HttpPolicyApi {
        &self.policy_api
    }

    pub fn dashboard_api(&self) -> &HttpDashboardApi {
        &self.dashboard_api
    }
}

impl UserEnv for AppEnv {
    type UserApi = HttpUserApi;
    type EncryptedDekApi = HttpEncryptedDekApi;

    fn user_api(&self) -> &Self::UserApi {
        &self.user_api
    }

    fn encrypted_dek_api(&self) -> &Self::EncryptedDekApi {
        &self.encrypted_dek_api
    }
}

impl ConfigEnv for AppEnv {
    type LocalDeviceApi = HttpLocalDeviceApi;
    type RemoteStorageApi = HttpRemoteStorageApi;
    type BackupConfigApi = HttpBackupConfigApi;

    fn local_device_api(&self) -> &Self::LocalDeviceApi {
        &self.local_device_api
    }

    fn remote_storage_api(&self) -> &Self::RemoteStorageApi {
        &self.remote_storage_api
    }

    fn backup_config_api(&self) -> &Self::BackupConfigApi {
        &self.backup_config_api
    }
}

impl BackupEnv for AppEnv {
    type UserApi = HttpUserApi;
    type BackupConfigApi = HttpBackupConfigApi;
    type LocalIndex = SqliteLocalIndex;
    type LocalDeviceApi = HttpLocalDeviceApi;
    type RemoteStorageApi = HttpRemoteStorageApi;
    type RemoteFileVersionApi = HttpRemoteFileVersionApi;
    type ChunkApi = HttpChunkApi;
    type BackupJobApi = HttpBackupJobApi;

    fn user_api(&self) -> &Self::UserApi {
        &self.user_api
    }

    fn backup_config_api(&self) -> &Self::BackupConfigApi {
        &self.backup_config_api
    }

    fn local_index(&self) -> &Self::LocalIndex {
        &self.local_index
    }

    fn local_device_api(&self) -> &Self::LocalDeviceApi {
        &self.local_device_api
    }

    fn remote_storage_api(&self) -> &Self::RemoteStorageApi {
        &self.remote_storage_api
    }

    fn remote_file_version_api(&self) -> &Self::RemoteFileVersionApi {
        &self.remote_file_version_api
    }

    fn chunk_api(&self) -> &Self::ChunkApi {
        &self.chunk_api
    }

    fn backup_job_api(&self) -> &Self::BackupJobApi {
        &self.backup_job_api
    }

    fn clone_env(&self) -> Self {
        self.clone()
    }
}

impl GcEnv for AppEnv {
    type GcApi = HttpGcApi;
    type RemoteStorageApi = HttpRemoteStorageApi;

    fn gc_api(&self) -> &Self::GcApi {
        &self.gc_api
    }

    fn remote_storage_api(&self) -> &Self::RemoteStorageApi {
        &self.remote_storage_api
    }
}

impl DashboardEnv for AppEnv {
    type DashboardApi = HttpDashboardApi;

    fn dashboard_api(&self) -> &Self::DashboardApi {
        &self.dashboard_api
    }
}

impl RestoreEnv for AppEnv {
    type BackupConfigApi = HttpBackupConfigApi;
    type RemoteStorageApi = HttpRemoteStorageApi;
    type RemoteFileVersionApi = HttpRemoteFileVersionApi;
    type ChunkApi = HttpChunkApi;
    type RestoreJobApi = HttpRestoreJobApi;

    fn backup_config_api(&self) -> &Self::BackupConfigApi {
        &self.backup_config_api
    }

    fn remote_storage_api(&self) -> &Self::RemoteStorageApi {
        &self.remote_storage_api
    }

    fn remote_file_version_api(&self) -> &Self::RemoteFileVersionApi {
        &self.remote_file_version_api
    }

    fn chunk_api(&self) -> &Self::ChunkApi {
        &self.chunk_api
    }

    fn restore_job_api(&self) -> &Self::RestoreJobApi {
        &self.restore_job_api
    }

    fn clone_env(&self) -> Self {
        self.clone()
    }
}
