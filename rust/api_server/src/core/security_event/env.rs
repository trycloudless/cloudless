use crate::core::ports;

pub trait SecurityEventEnv {
    type Repo: ports::SecurityEventRepo;

    fn security_event_repo(&self) -> &Self::Repo;
}
