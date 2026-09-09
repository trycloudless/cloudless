use crate::core::{CoreError, CoreResult, blog::env::BlogEnv, ports::BlogPostRepo};
use api_types::blog::*;
use uuid::Uuid;

pub async fn create_post<E: BlogEnv>(
    env: &E,
    user_id: Uuid,
    req: CreateBlogPostRequest,
) -> CoreResult<BlogPostResponse> {
    env.blog_repo().create(user_id, req).await
}

pub async fn update_post<E: BlogEnv>(
    env: &E,
    user_id: Uuid,
    req: UpdateBlogPostRequest,
) -> CoreResult<BlogPostResponse> {
    env.blog_repo().update(user_id, req).await
}

pub async fn delete_post<E: BlogEnv>(env: &E, user_id: Uuid, id: Uuid) -> CoreResult<()> {
    env.blog_repo().delete(user_id, id).await
}

pub async fn get_by_slug<E: BlogEnv>(env: &E, slug: &str) -> CoreResult<BlogPostResponse> {
    env.blog_repo()
        .get_by_slug(slug)
        .await?
        .ok_or_else(|| CoreError::not_found(format!("Blog post: {}", slug)))
}

/// Fetches a post by slug regardless of published status. Used by admin edit routes.
pub async fn get_by_slug_admin<E: BlogEnv>(env: &E, slug: &str) -> CoreResult<BlogPostResponse> {
    env.blog_repo()
        .get_by_slug_any(slug)
        .await?
        .ok_or_else(|| CoreError::not_found(format!("Blog post: {}", slug)))
}

pub async fn list_published<E: BlogEnv>(env: &E) -> CoreResult<Vec<BlogPostSummary>> {
    env.blog_repo().list_published().await
}

/// Returns all posts including unpublished drafts. Used by admin listing.
pub async fn list_all_admin<E: BlogEnv>(env: &E) -> CoreResult<Vec<BlogPostSummary>> {
    env.blog_repo().list_all().await
}

// pub async fn list_all_by_user<E: BlogEnv>(
//     env: &E,
//     user_id: Uuid,
// ) -> CoreResult<Vec<BlogPostSummary>> {
//     env.blog_repo().list_all_by_user(user_id).await
// }
