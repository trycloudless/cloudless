use api_types::blog::{
    BlogPostListResponse, BlogPostResponse, CreateBlogPostRequest, DeleteBlogPostResponse,
    UpdateBlogPostRequest,
};
use async_trait::async_trait;
use uuid::Uuid;

use crate::ports::api::ApiResult;

#[async_trait]
pub trait BlogApiPort: Send + Sync {
    async fn get_by_slug(&self, slug: &str) -> ApiResult<BlogPostResponse>;
    /// Fetches a post by slug including unpublished drafts. Requires auth (admin only).
    async fn get_by_slug_admin(&self, slug: &str) -> ApiResult<BlogPostResponse>;
    async fn list_published(&self) -> ApiResult<BlogPostListResponse>;
    /// Lists all posts including drafts. Requires auth (admin only).
    async fn list_all_admin(&self) -> ApiResult<BlogPostListResponse>;
    async fn create(&self, request: CreateBlogPostRequest) -> ApiResult<BlogPostResponse>;
    async fn update(&self, request: UpdateBlogPostRequest) -> ApiResult<BlogPostResponse>;
    async fn delete(&self, id: Uuid) -> ApiResult<DeleteBlogPostResponse>;
}
