use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Type of policy, stored as snake_case string in the database.
///
/// Consent-tracked (CRUD-managed, may be required at login):
///   `TermsOfService`, `PrivacyPolicy`
///
/// Informational (admin-manageable, publicly visible, not login-gated):
///   `CookiePolicy`, `AcceptableUse`, `RefundAndCancellation`,
///   `SecurityAndDataHandling`, `Subprocessors`, `DataRetentionAndDeletion`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PolicyType {
    PrivacyPolicy,
    TermsOfService,
    CookiePolicy,
    AcceptableUse,
    RefundAndCancellation,
    SecurityAndDataHandling,
    Subprocessors,
    DataRetentionAndDeletion,
}

impl std::fmt::Display for PolicyType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PolicyType::PrivacyPolicy => write!(f, "privacy_policy"),
            PolicyType::TermsOfService => write!(f, "terms_of_service"),
            PolicyType::CookiePolicy => write!(f, "cookie_policy"),
            PolicyType::AcceptableUse => write!(f, "acceptable_use"),
            PolicyType::RefundAndCancellation => write!(f, "refund_and_cancellation"),
            PolicyType::SecurityAndDataHandling => write!(f, "security_and_data_handling"),
            PolicyType::Subprocessors => write!(f, "subprocessors"),
            PolicyType::DataRetentionAndDeletion => write!(f, "data_retention_and_deletion"),
        }
    }
}

impl std::str::FromStr for PolicyType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "privacy_policy" => Ok(PolicyType::PrivacyPolicy),
            "terms_of_service" => Ok(PolicyType::TermsOfService),
            "cookie_policy" => Ok(PolicyType::CookiePolicy),
            "acceptable_use" => Ok(PolicyType::AcceptableUse),
            "refund_and_cancellation" => Ok(PolicyType::RefundAndCancellation),
            "security_and_data_handling" => Ok(PolicyType::SecurityAndDataHandling),
            "subprocessors" => Ok(PolicyType::Subprocessors),
            "data_retention_and_deletion" => Ok(PolicyType::DataRetentionAndDeletion),
            _ => Err(format!("unknown policy type: {}", s)),
        }
    }
}

// ── Policy (admin management) ──────────────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct CreatePolicyRequest {
    pub policy_type: PolicyType,
    pub title: String,
    pub description: String,
    pub is_required: bool,
    pub max_skips: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreatePolicyResponse {
    pub id: Uuid,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdatePolicyRequest {
    pub id: Uuid,
    pub title: Option<String>,
    pub description: Option<String>,
    pub is_required: Option<bool>,
    pub max_skips: Option<i32>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PolicyResponse {
    pub id: Uuid,
    pub policy_type: PolicyType,
    pub title: String,
    pub description: String,
    pub is_required: bool,
    pub max_skips: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ListPoliciesResponse {
    pub list: Vec<PolicyResponse>,
}

// ── Policy Version (admin management) ──────────────────────────────

#[derive(Debug, Deserialize, Serialize)]
pub struct CreatePolicyVersionRequest {
    pub policy_id: Uuid,
    pub content: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreatePolicyVersionResponse {
    pub id: Uuid,
    pub version: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdatePolicyVersionRequest {
    pub id: Uuid,
    pub content: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PublishPolicyVersionRequest {
    pub id: Uuid,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PolicyVersionResponse {
    pub id: Uuid,
    pub policy_id: Uuid,
    pub version: i32,
    pub content: String,
    pub status: String,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PolicyVersionSummary {
    pub id: Uuid,
    pub policy_id: Uuid,
    pub version: i32,
    pub status: String,
    pub published_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ListPolicyVersionsResponse {
    pub list: Vec<PolicyVersionSummary>,
}

// ── User-facing (acceptance & skipping) ────────────────────────────

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PendingPolicyResponse {
    pub policy_version_id: Uuid,
    pub policy_type: PolicyType,
    pub policy_title: String,
    pub version: i32,
    pub content: String,
    pub max_skips: i32,
    pub times_skipped: i32,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct GetPendingPoliciesResponse {
    pub list: Vec<PendingPolicyResponse>,
}

/// The pending Refund & Cancellation policy version, or `None` if already accepted.
/// Returned before opening checkout so the user can accept it in-app.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GetCheckoutPolicyResponse {
    pub policy: Option<PendingPolicyResponse>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AcceptPolicyRequest {
    pub policy_version_id: Uuid,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub physical_device_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AcceptPolicyResponse {
    pub id: Uuid,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SkipPolicyRequest {
    pub policy_version_id: Uuid,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub physical_device_id: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SkipPolicyResponse {
    pub id: Uuid,
    pub remaining_skips: i32,
}

// ── Public (unauthenticated) ────────────────────────────────────────

/// Latest published version of a policy, returned by the public endpoint.
/// No auth required — intended for public legal pages.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PublicPolicyResponse {
    pub policy_type: PolicyType,
    pub title: String,
    pub version: i32,
    pub content: String,
    pub published_at: DateTime<Utc>,
}
