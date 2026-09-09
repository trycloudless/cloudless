use crate::core::ports;

pub trait PolicyEnv {
    type Repo: ports::PolicyRepo;

    fn policy_repo(&self) -> &Self::Repo;
}
