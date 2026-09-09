use crate::ports::api::dashboard_api_port::DashboardApiPort;

pub trait DashboardEnv {
    type DashboardApi: DashboardApiPort;

    fn dashboard_api(&self) -> &Self::DashboardApi;
}
