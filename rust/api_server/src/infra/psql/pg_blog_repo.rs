use api_types::blog::*;
use async_trait::async_trait;
use sqlx::postgres::PgPool;
use uuid::Uuid;

use crate::core::{CoreResult, ports::BlogPostRepo};

use super::map_sqlx_error;

#[derive(Clone)]
pub struct PgBlogPostRepo {
    pub pool: PgPool,
}

impl PgBlogPostRepo {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl BlogPostRepo for PgBlogPostRepo {
    async fn create(
        &self,
        user_id: Uuid,
        req: CreateBlogPostRequest,
    ) -> CoreResult<BlogPostResponse> {
        let id = Uuid::now_v7();
        let row = sqlx::query!(
            r#"
            INSERT INTO blog_posts (id, user_id, slug, title, content, summary, published, is_page, og_image)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING id, user_id, slug, title, content, summary, published, is_page, created_at, updated_at, og_image
            "#,
            id,
            user_id,
            req.slug,
            req.title,
            req.content,
            req.summary,
            req.published,
            req.is_page,
            req.og_image,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(BlogPostResponse {
            id: row.id,
            user_id: row.user_id,
            slug: row.slug,
            title: row.title,
            content: row.content,
            summary: row.summary,
            published: row.published,
            is_page: row.is_page,
            created_at: row.created_at,
            updated_at: row.updated_at,
            og_image: row.og_image,
        })
    }

    async fn update(
        &self,
        user_id: Uuid,
        req: UpdateBlogPostRequest,
    ) -> CoreResult<BlogPostResponse> {
        let row = sqlx::query!(
            r#"
            UPDATE blog_posts
            SET slug = COALESCE($3, slug),
                title = COALESCE($4, title),
                content = COALESCE($5, content),
                summary = COALESCE($6, summary),
                published = COALESCE($7, published),
                is_page = COALESCE($8, is_page),
                og_image = $9,
                updated_at = now()
            WHERE id = $1 AND user_id = $2
            RETURNING id, user_id, slug, title, content, summary, published, is_page, created_at, updated_at, og_image
            "#,
            req.id,
            user_id,
            req.slug,
            req.title,
            req.content,
            req.summary,
            req.published,
            req.is_page,
            req.og_image,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(BlogPostResponse {
            id: row.id,
            user_id: row.user_id,
            slug: row.slug,
            title: row.title,
            content: row.content,
            summary: row.summary,
            published: row.published,
            is_page: row.is_page,
            created_at: row.created_at,
            updated_at: row.updated_at,
            og_image: row.og_image,
        })
    }

    async fn delete(&self, user_id: Uuid, id: Uuid) -> CoreResult<()> {
        sqlx::query!(
            r#"DELETE FROM blog_posts WHERE id = $1 AND user_id = $2"#,
            id,
            user_id,
        )
        .execute(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(())
    }

    async fn get_by_slug(&self, slug: &str) -> CoreResult<Option<BlogPostResponse>> {
        let row = sqlx::query!(
            r#"
            SELECT id, user_id, slug, title, content, summary, published, is_page, created_at, updated_at, og_image
            FROM blog_posts
            WHERE slug = $1 AND published = true
            "#,
            slug,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(|r| BlogPostResponse {
            id: r.id,
            user_id: r.user_id,
            slug: r.slug,
            title: r.title,
            content: r.content,
            summary: r.summary,
            published: r.published,
            is_page: r.is_page,
            created_at: r.created_at,
            updated_at: r.updated_at,
            og_image: r.og_image,
        }))
    }

    /// Fetches a post by slug without filtering by published — used by admin edit routes.
    async fn get_by_slug_any(&self, slug: &str) -> CoreResult<Option<BlogPostResponse>> {
        let row = sqlx::query!(
            r#"
            SELECT id, user_id, slug, title, content, summary, published, is_page, created_at, updated_at, og_image
            FROM blog_posts
            WHERE slug = $1
            "#,
            slug,
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(row.map(|r| BlogPostResponse {
            id: r.id,
            user_id: r.user_id,
            slug: r.slug,
            title: r.title,
            content: r.content,
            summary: r.summary,
            published: r.published,
            is_page: r.is_page,
            created_at: r.created_at,
            updated_at: r.updated_at,
            og_image: r.og_image,
        }))
    }

    async fn list_published(&self) -> CoreResult<Vec<BlogPostSummary>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, slug, title, summary, published, is_page, created_at
            FROM blog_posts
            WHERE published = true AND is_page = false
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(rows
            .into_iter()
            .map(|r| BlogPostSummary {
                id: r.id,
                slug: r.slug,
                title: r.title,
                summary: r.summary,
                published: r.published,
                is_page: r.is_page,
                created_at: r.created_at,
            })
            .collect())
    }

    /// Returns all posts including unpublished drafts, ordered by created_at DESC.
    async fn list_all(&self) -> CoreResult<Vec<BlogPostSummary>> {
        let rows = sqlx::query!(
            r#"
            SELECT id, slug, title, summary, published, is_page, created_at
            FROM blog_posts
            WHERE is_page = false
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(map_sqlx_error)?;

        Ok(rows
            .into_iter()
            .map(|r| BlogPostSummary {
                id: r.id,
                slug: r.slug,
                title: r.title,
                summary: r.summary,
                published: r.published,
                is_page: r.is_page,
                created_at: r.created_at,
            })
            .collect())
    }

    // async fn list_all_by_user(&self, user_id: Uuid) -> CoreResult<Vec<BlogPostSummary>> {
    //     let rows = sqlx::query!(
    //         r#"
    //         SELECT id, slug, title, summary, published, is_page, created_at
    //         FROM blog_posts
    //         WHERE user_id = $1
    //         ORDER BY created_at DESC
    //         "#,
    //         user_id,
    //     )
    //     .fetch_all(&self.pool)
    //     .await
    //     .map_err(map_sqlx_error)?;

    //     Ok(rows
    //         .into_iter()
    //         .map(|r| BlogPostSummary {
    //             id: r.id,
    //             slug: r.slug,
    //             title: r.title,
    //             summary: r.summary,
    //             published: r.published,
    //             is_page: r.is_page,
    //             created_at: r.created_at,
    //         })
    //         .collect())
    // }
}
