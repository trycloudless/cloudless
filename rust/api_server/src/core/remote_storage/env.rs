use crate::core::ports;

pub trait RemoteStorageEnv {
    type Repo: ports::RemoteStorageRepo;

    fn remote_storage_repo(&self) -> &Self::Repo;
}
