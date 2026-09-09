use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Type of email template, stored as snake_case string in the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EmailTemplateType {
    PasswordReset,
    Promotions,
    Service,
}

impl std::fmt::Display for EmailTemplateType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmailTemplateType::PasswordReset => write!(f, "password_reset"),
            EmailTemplateType::Promotions => write!(f, "promotions"),
            EmailTemplateType::Service => write!(f, "service"),
        }
    }
}

impl EmailTemplateType {
    pub fn from_str(s: &str) -> Self {
        match s {
            "password_reset" => EmailTemplateType::PasswordReset,
            "promotions" => EmailTemplateType::Promotions,
            "service" => EmailTemplateType::Service,
            _ => EmailTemplateType::Service,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateEmailTemplateRequest {
    pub name: String,
    pub subject: String,
    pub body_html: String,
    pub template_type: EmailTemplateType,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateEmailTemplateRequest {
    pub id: Uuid,
    pub name: Option<String>,
    pub subject: Option<String>,
    pub body_html: Option<String>,
    pub template_type: Option<EmailTemplateType>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EmailTemplateResponse {
    pub id: Uuid,
    pub name: String,
    pub subject: String,
    pub body_html: String,
    pub template_type: EmailTemplateType,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EmailTemplateSummary {
    pub id: Uuid,
    pub name: String,
    pub subject: String,
    pub template_type: EmailTemplateType,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct EmailTemplateListResponse {
    pub templates: Vec<EmailTemplateSummary>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeleteEmailTemplateResponse {
    pub success: bool,
}
