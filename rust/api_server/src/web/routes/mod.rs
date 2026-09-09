use crate::app_env::AppEnv;
use crate::web::routes::middlewares::jwt_auth;
use axum::http::{
    Method,
    header::{AUTHORIZATION, CONTENT_TYPE},
};
use axum::middleware::from_fn_with_state;
use axum::{Router, routing::get};
use tower_http::cors::{AllowOrigin, CorsLayer};

pub mod auth_route;
pub mod backup_config_route;
pub mod backup_job_route;
pub mod blog_route;
pub mod chunk_route;
pub mod dashboard_route;
pub mod email_template_route;
pub mod encrypted_dek_route;
pub mod file_version_route;
pub mod gc_route;
pub mod hash_route;
pub mod local_device_route;
pub mod media_route;
pub mod middlewares;
pub mod password_reset_route;
pub mod policy_route;
pub mod remote_file_version_route;
pub mod remote_storage_route;
pub mod restore_job_route;
pub mod security_event_route;
pub mod subscription_route;
pub mod user_route;
pub mod webhook_route;

use axum::{Json, response::IntoResponse};
use serde_json::json;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AuthContext {
    pub user_id: uuid::Uuid,
}

/// Options controlling which routes are mounted.
///
/// Defaults to all billing routes disabled — correct for self-hosted deployments.
/// The hosted binary (private `cloudless-hosting` crate) sets both fields to `true`.
#[derive(Clone, Debug)]
pub struct RouterOptions {
    /// When true, mounts `POST /webhooks/dodo`.
    /// Must be false in self-hosted mode: the Dodo route is meaningless without
    /// provider credentials and should not appear in the public route table.
    pub enable_billing_webhooks: bool,
    /// When true, mounts `POST /api/subscription/checkout` and
    /// `POST /api/subscription/portal`.
    /// Must be false in self-hosted mode to avoid exposing dead endpoints.
    pub enable_checkout_routes: bool,
    /// Extra CORS origins to allow, in addition to the built-in Tauri and
    /// localhost origins. Parsed from the `CORS_ORIGINS` env var (comma-separated).
    /// Example: `["https://backup.example.com"]`
    pub extra_cors_origins: Vec<String>,
}

impl Default for RouterOptions {
    fn default() -> Self {
        Self {
            enable_billing_webhooks: false,
            enable_checkout_routes: false,
            extra_cors_origins: Vec::new(),
        }
    }
}

pub fn create_router(env: AppEnv, opts: RouterOptions) -> Router {
    let mut api_routes = Router::<AppEnv>::new()
        .nest("/file_version", file_version_route::create_router())
        .nest("/hash", hash_route::create_router())
        .nest("/user", user_route::create_router())
        .nest("/remote_storage", remote_storage_route::create_router())
        .nest(
            "/remote_file_version",
            remote_file_version_route::create_router(),
        )
        .nest("/backup_config", backup_config_route::create_router())
        .nest("/backup_job", backup_job_route::create_router())
        .nest("/local_device", local_device_route::create_router())
        .nest("/chunk", chunk_route::create_router())
        .nest("/encrypted_dek", encrypted_dek_route::create_router())
        .nest("/restore_job", restore_job_route::create_router())
        .nest("/security_event", security_event_route::create_router())
        .nest("/subscription", subscription_route::protected_router())
        .nest("/admin/subscription", subscription_route::admin_router())
        .nest("/dashboard", dashboard_route::create_router())
        .nest("/gc", gc_route::create_router())
        .nest("/blog", blog_route::protected_router())
        .nest("/media", media_route::protected_router())
        .nest("/email_template", email_template_route::protected_router())
        .nest("/policy", policy_route::admin_router())
        .nest("/policy", policy_route::authenticated_router());

    // Checkout and portal routes are absent in self-hosted (billing-disabled) mode.
    if opts.enable_checkout_routes {
        api_routes = api_routes
            .nest("/subscription", subscription_route::payment_router());
    }

    let api_routes = api_routes
        .with_state(env.clone())
        .layer(from_fn_with_state(env.clone(), jwt_auth));

    let mut root = Router::<AppEnv>::new()
        .route("/health", get(health_check))
        .nest("/auth", auth_route::create_router())
        .nest("/blog", blog_route::public_router())
        .nest("/media", media_route::public_router())
        .nest("/password-reset", password_reset_route::create_router())
        .nest("/api/policy", policy_route::public_router())
        .nest("/api", api_routes);

    // Webhook route is absent in self-hosted mode: no Dodo credentials,
    // no reason to expose the endpoint.
    if opts.enable_billing_webhooks {
        root = root.nest("/webhooks", webhook_route::create_router());
    }

    // Build CORS origin list: fixed Tauri + localhost origins, plus any extras
    // supplied via CORS_ORIGINS env var (comma-separated list of URLs).
    let mut cors_origins: Vec<axum::http::HeaderValue> = vec![
        "tauri://localhost".parse().expect("valid origin"),
        "https://tauri.localhost".parse().expect("valid origin"),
        "http://localhost:3000".parse().expect("valid origin"),
    ];
    for origin in &opts.extra_cors_origins {
        match origin.parse::<axum::http::HeaderValue>() {
            Ok(v) => cors_origins.push(v),
            Err(_) => tracing::warn!("Ignoring invalid CORS origin: {}", origin),
        }
    }

    root.layer(
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(cors_origins))
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PUT,
                    Method::DELETE,
                    Method::OPTIONS,
                    Method::PATCH,
                ])
                .allow_headers([CONTENT_TYPE, AUTHORIZATION]),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(env)
}

pub async fn health_check() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION")
    }))
}
