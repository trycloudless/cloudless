use crate::core::ports;

pub trait RestoreJobEnv {
    type Repo: ports::RestoreJobRepo;

    fn restore_job_repo(&self) -> &Self::Repo;
}
