use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateBlogPostRequest {
    pub slug: String,
    pub title: String,
    pub content: String,
    pub summary: String,
    pub published: bool,
    pub is_page: bool,
    pub og_image: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateBlogPostRequest {
    pub id: Uuid,
    pub slug: Option<String>,
    pub title: Option<String>,
    pub content: Option<String>,
    pub summary: Option<String>,
    pub published: Option<bool>,
    pub is_page: Option<bool>,
    pub og_image: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BlogPostResponse {
    pub id: Uuid,
    pub user_id: Uuid,
    pub slug: String,
    pub title: String,
    pub content: String,
    pub summary: String,
    pub published: bool,
    pub is_page: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub og_image: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct BlogPostSummary {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub published: bool,
    pub is_page: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BlogPostListResponse {
    pub posts: Vec<BlogPostSummary>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DeleteBlogPostResponse {
    pub success: bool,
}
