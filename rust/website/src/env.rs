use crate::releases::ReleaseMetadataCache;
use cloudless_core::{
    adapters::api::{
        http_api_with_auth::HttpApi, http_blog_api::HttpBlogApi,
        http_email_template_api::HttpEmailTemplateApi, http_policy_api::HttpPolicyApi,
        http_user_api::HttpUserApi,
    },
    ports::api::{
        blog_api_port::BlogApiPort, email_template_api_port::EmailTemplateApiPort,
        policy_api_port::PolicyApiPort, user_api_port::UserApiPort,
    },
};

pub trait WebsiteEnv: Clone + Send + Sync + 'static {
    type UserApi: UserApiPort;
    type BlogApi: BlogApiPort;
    type EmailTemplateApi: EmailTemplateApiPort;
    type PolicyApi: PolicyApiPort;

    fn user_api(&self) -> &Self::UserApi;
    fn blog_api(&self) -> &Self::BlogApi;
    fn email_template_api(&self) -> &Self::EmailTemplateApi;
    fn policy_api(&self) -> &Self::PolicyApi;
    fn http_api(&self) -> &HttpApi;
}

#[derive(Clone)]
pub struct AppEnv {
    http_api: HttpApi,
    user_api: HttpUserApi,
    blog_api: HttpBlogApi,
    email_template_api: HttpEmailTemplateApi,
    policy_api: HttpPolicyApi,
    releases: ReleaseMetadataCache,
}

impl AppEnv {
    pub fn new(base_url: url::Url) -> Self {
        let releases_url = std::env::var("RELEASES_METADATA_URL").ok();
        let releases = ReleaseMetadataCache::new(releases_url);
        let http_api = HttpApi::new(base_url);
        let user_api = HttpUserApi::new(http_api.clone());
        let blog_api = HttpBlogApi::new(http_api.clone());
        let email_template_api = HttpEmailTemplateApi::new(http_api.clone());
        let policy_api = HttpPolicyApi::new(http_api.clone());
        AppEnv {
            http_api,
            user_api,
            blog_api,
            email_template_api,
            policy_api,
            releases,
        }
    }

    pub fn releases(&self) -> &ReleaseMetadataCache {
        &self.releases
    }

    /// Creates a per-request copy with an isolated token store seeded with the
    /// given tokens. The underlying HTTP client is shared (cheap Arc clone).
    pub fn with_tokens(&self, access_token: String, refresh_token: String) -> Self {
        let http_api = self.http_api.with_tokens(access_token, refresh_token);
        Self::from_http_api(http_api, self.releases.clone())
    }

    /// Creates a per-request copy with an empty (unauthenticated) token store.
    pub fn with_empty_tokens(&self) -> Self {
        let http_api = self.http_api.with_empty_tokens();
        Self::from_http_api(http_api, self.releases.clone())
    }

    fn from_http_api(http_api: HttpApi, releases: ReleaseMetadataCache) -> Self {
        let user_api = HttpUserApi::new(http_api.clone());
        let blog_api = HttpBlogApi::new(http_api.clone());
        let email_template_api = HttpEmailTemplateApi::new(http_api.clone());
        let policy_api = HttpPolicyApi::new(http_api.clone());
        AppEnv {
            http_api,
            user_api,
            blog_api,
            email_template_api,
            policy_api,
            releases,
        }
    }
}

impl WebsiteEnv for AppEnv {
    type UserApi = HttpUserApi;
    type BlogApi = HttpBlogApi;
    type EmailTemplateApi = HttpEmailTemplateApi;
    type PolicyApi = HttpPolicyApi;

    fn user_api(&self) -> &Self::UserApi {
        &self.user_api
    }

    fn blog_api(&self) -> &Self::BlogApi {
        &self.blog_api
    }

    fn email_template_api(&self) -> &Self::EmailTemplateApi {
        &self.email_template_api
    }

    fn policy_api(&self) -> &Self::PolicyApi {
        &self.policy_api
    }

    fn http_api(&self) -> &HttpApi {
        &self.http_api
    }
}
