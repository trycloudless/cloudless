use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
pub struct LocalDeviceEntity {
    pub id: Uuid,
    pub user_id: Uuid,
    pub physical_device_id: String,
    pub display_name: Option<String>,
    pub platform: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateLocalDeviceRequest {
    pub physical_device_id: String,
    pub display_name: Option<String>,
    pub platform: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateLocalDeviceResponse {
    pub id: Uuid,
    pub physical_device_id: String,
    pub display_name: Option<String>,
    pub platform: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetLocalDeviceByPhysicalIdRequest {
    pub physical_device_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetLocalDeviceByPhysicalIdResponse {
    pub id: Uuid,
    pub physical_device_id: String,
    pub display_name: Option<String>,
    pub platform: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetOrCreateLocalDeviceRequest {
    pub physical_device_id: String,
    pub display_name: Option<String>,
    pub platform: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GetOrCreateLocalDeviceResponse {
    pub id: Uuid,
    pub physical_device_id: String,
    pub display_name: Option<String>,
    pub platform: String,
    pub created_at: DateTime<Utc>,
    pub was_created: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListDevicesByPlatformRequest {
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceSummary {
    pub id: Uuid,
    pub physical_device_id: String,
    pub display_name: Option<String>,
    pub platform: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListDevicesByPlatformResponse {
    pub devices: Vec<DeviceSummary>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListAllDevicesResponse {
    pub devices: Vec<DeviceSummary>,
}
