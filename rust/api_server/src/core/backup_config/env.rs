use crate::core::ports;

pub trait BackupConfigEnv {
    type Repo: ports::BackupConfigRepo;

    fn backup_config_repo(&self) -> &Self::Repo;
}
