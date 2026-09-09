use crate::core::ports;

pub trait UserEnv {
    type Repo: ports::UserRepo;

    fn user_repo(&self) -> &Self::Repo;
}
