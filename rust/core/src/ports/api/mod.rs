use api_types::error::ApiError;

pub mod backup_config_api_port;
pub mod backup_job_api_port;
pub mod blog_api_port;
pub mod chunk_api_port;
pub mod dashboard_api_port;
pub mod email_template_api_port;
pub mod encrypted_dek_api_port;
pub mod gc_api_port;
pub mod local_device_api_port;
pub mod policy_api_port;
pub mod remote_file_version_api_port;
pub mod remote_storage_api_port;
pub mod restore_job_api_port;
pub mod security_event_api_port;
pub mod subscription_api_port;
pub mod user_api_port;

#[derive(Debug, thiserror::Error)]
pub enum ApiClientError {
    #[error("transport error")]
    Transport(#[from] reqwest::Error),
    #[error("Invalid Url {0}")]
    InvalidUrl(String),
    #[error("{0}")]
    Api(ApiError),
}

pub type ApiResult<T> = Result<T, ApiClientError>;
