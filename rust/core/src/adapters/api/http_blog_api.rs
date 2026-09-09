use api_types::blog::{
    BlogPostListResponse, BlogPostResponse, CreateBlogPostRequest, DeleteBlogPostResponse,
    UpdateBlogPostRequest,
};
use async_trait::async_trait;
use uuid::Uuid;

use crate::{
    adapters::api::http_api_with_auth::HttpApi,
    ports::api::{ApiResult, blog_api_port::BlogApiPort},
};

#[derive(Clone)]
pub struct HttpBlogApi {
    api: HttpApi,
}

impl HttpBlogApi {
    pub fn new(api: HttpApi) -> Self {
        HttpBlogApi { api }
    }
}

#[async_trait]
impl BlogApiPort for HttpBlogApi {
    async fn get_by_slug(&self, slug: &str) -> ApiResult<BlogPostResponse> {
        let url = self.api.get_url(&format!("blog/{}", slug))?;
        let request = self.api.client.get(url);
        self.api.send(request).await
    }

    async fn get_by_slug_admin(&self, slug: &str) -> ApiResult<BlogPostResponse> {
        let url = self.api.get_url(&format!("api/blog/slug/{}", slug))?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }

    async fn list_published(&self) -> ApiResult<BlogPostListResponse> {
        let url = self.api.get_url("blog")?;
        let request = self.api.client.get(url);
        self.api.send(request).await
    }

    async fn list_all_admin(&self) -> ApiResult<BlogPostListResponse> {
        let url = self.api.get_url("api/blog/all")?;
        let request = self.api.client.get(url);
        self.api.send_with_auth(request).await
    }

    async fn create(&self, request: CreateBlogPostRequest) -> ApiResult<BlogPostResponse> {
        let url = self.api.get_url("api/blog")?;
        let request = self.api.client.post(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn update(&self, request: UpdateBlogPostRequest) -> ApiResult<BlogPostResponse> {
        let url = self.api.get_url(&format!("api/blog/{}", request.id))?;
        let request = self.api.client.put(url).json(&request);
        self.api.send_with_auth(request).await
    }

    async fn delete(&self, id: Uuid) -> ApiResult<DeleteBlogPostResponse> {
        let url = self.api.get_url(&format!("api/blog/{}", id))?;
        let request = self.api.client.delete(url);
        self.api.send_with_auth(request).await
    }
}
