use crate::core::ports;

pub trait DashboardEnv {
    type Repo: ports::DashboardRepo;

    fn dashboard_repo(&self) -> &Self::Repo;
}
