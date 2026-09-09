use crate::core::ports;

pub trait RemoteFileVersionEnv {
    type Repo: ports::RemoteFileVersionRepo;

    fn remote_file_version_repo(&self) -> &Self::Repo;
}
