use crate::{
    app_env::AppEnv,
    core::blog::application,
    web::{routes::AuthContext, web_app_error::WebAppResult},
};
use api_types::blog::*;
use axum::{
    Extension, Json, Router,
    extract::{Path, State},
    routing::{delete, get, post, put},
};

// Public routes (no auth required)
pub fn public_router() -> Router<AppEnv> {
    Router::new()
        .route("/", get(list_published_handler))
        .route("/{slug}", get(get_by_slug_handler))
}

// Protected routes (behind jwt_auth middleware)
pub fn protected_router() -> Router<AppEnv> {
    Router::new()
        .route("/", post(create_post_handler))
        .route("/{id}", put(update_post_handler))
        .route("/{id}", delete(delete_post_handler))
        // Admin-only: fetch a post by slug regardless of published status
        .route("/slug/{slug}", get(get_by_slug_admin_handler))
        // Admin-only: list all posts including drafts
        .route("/all", get(list_all_admin_handler))
}

pub async fn list_published_handler(
    State(env): State<AppEnv>,
) -> WebAppResult<Json<BlogPostListResponse>> {
    let posts = application::list_published(&env).await?;
    Ok(Json(BlogPostListResponse { posts }))
}

pub async fn get_by_slug_handler(
    State(env): State<AppEnv>,
    Path(slug): Path<String>,
) -> WebAppResult<Json<BlogPostResponse>> {
    let post = application::get_by_slug(&env, &slug).await?;
    Ok(Json(post))
}

async fn create_post_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Json(req): Json<CreateBlogPostRequest>,
) -> WebAppResult<Json<BlogPostResponse>> {
    let post = application::create_post(&env, ctx.user_id, req).await?;
    Ok(Json(post))
}

async fn update_post_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<uuid::Uuid>,
    Json(mut req): Json<UpdateBlogPostRequest>,
) -> WebAppResult<Json<BlogPostResponse>> {
    req.id = id;
    let post = application::update_post(&env, ctx.user_id, req).await?;
    Ok(Json(post))
}

/// Admin-only handler to list all posts including unpublished drafts.
async fn list_all_admin_handler(
    State(env): State<AppEnv>,
    Extension(_ctx): Extension<AuthContext>,
) -> WebAppResult<Json<BlogPostListResponse>> {
    let posts = application::list_all_admin(&env).await?;
    Ok(Json(BlogPostListResponse { posts }))
}

/// Admin-only handler to fetch a post by slug, including unpublished drafts.
async fn get_by_slug_admin_handler(
    State(env): State<AppEnv>,
    Extension(_ctx): Extension<AuthContext>,
    Path(slug): Path<String>,
) -> WebAppResult<Json<BlogPostResponse>> {
    let post = application::get_by_slug_admin(&env, &slug).await?;
    Ok(Json(post))
}

async fn delete_post_handler(
    State(env): State<AppEnv>,
    Extension(ctx): Extension<AuthContext>,
    Path(id): Path<uuid::Uuid>,
) -> WebAppResult<Json<DeleteBlogPostResponse>> {
    application::delete_post(&env, ctx.user_id, id).await?;
    Ok(Json(DeleteBlogPostResponse { success: true }))
}
