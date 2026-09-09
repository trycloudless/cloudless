use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Types of security-relevant events tracked for audit logging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityEventType {
    ExportRecoveryKey,
    ChangeRecoveryKey,
    RecoverWithRecoveryKey,
    ChangePassword,
}

impl std::fmt::Display for SecurityEventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ExportRecoveryKey => write!(f, "export_recovery_key"),
            Self::ChangeRecoveryKey => write!(f, "change_recovery_key"),
            Self::RecoverWithRecoveryKey => write!(f, "recover_with_recovery_key"),
            Self::ChangePassword => write!(f, "change_password"),
        }
    }
}

impl std::str::FromStr for SecurityEventType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "export_recovery_key" => Ok(Self::ExportRecoveryKey),
            "change_recovery_key" => Ok(Self::ChangeRecoveryKey),
            "recover_with_recovery_key" => Ok(Self::RecoverWithRecoveryKey),
            "change_password" => Ok(Self::ChangePassword),
            _ => Err(format!("unknown security event type: {s}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreSecurityEventRequest {
    pub event_type: SecurityEventType,
    pub physical_device_id: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoreSecurityEventResponse {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityEventSummary {
    pub id: Uuid,
    pub event_type: SecurityEventType,
    pub physical_device_id: String,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListSecurityEventsResponse {
    pub events: Vec<SecurityEventSummary>,
}
