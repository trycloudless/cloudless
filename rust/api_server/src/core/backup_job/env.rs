use crate::core::ports;

pub trait BackupJobEnv {
    type Repo: ports::BackupJobRepo;

    fn backup_job_repo(&self) -> &Self::Repo;
}
