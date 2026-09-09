mod auth;
mod components;
mod env;
mod error;
pub(crate) mod markdown;
mod pages;
mod releases;
mod state;

use axum::{
    Router, middleware,
    routing::{get, post},
};
use state::WebsiteState;
use std::net::SocketAddr;
use tower_http::services::ServeDir;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "website=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let api_base =
        std::env::var("API_BASE_URL").unwrap_or_else(|_| "http://localhost:9000".to_string());
    let base_url = url::Url::parse(&api_base)?;

    let state = WebsiteState::new(base_url);

    let app = Router::new()
        .route("/robots.txt", get(pages::seo::robots_handler))
        .route("/sitemap.xml", get(pages::seo::sitemap_handler))
        .route("/admin", get(pages::admin_page::admin_handler))
        .route("/", get(pages::home_page::home_handler))
        .route("/pricing", get(pages::pricing_page::pricing_handler))
        .route("/download", get(pages::download_page::download_handler))
        .route(
            "/api/download",
            get(pages::download_page::download_redirect_handler),
        )
        .route(
            "/checkout",
            get(pages::billing_page::checkout_initiate_handler),
        )
        .route(
            "/billing/return/{checkout_id}",
            get(pages::billing_page::billing_return_handler),
        )
        .route(
            "/billing/cancel",
            get(pages::billing_page::billing_cancel_handler),
        )
        .route("/blog", get(pages::blog_list_page::blog_list_handler))
        .route(
            "/delete-blog/{id}",
            post(pages::blog_list_page::delete_blog_handler),
        )
        .route(
            "/blog/{slug}",
            get(pages::blog_post_page::blog_post_handler),
        )
        .route("/legal/{slug}", get(pages::legal_page::legal_page_handler))
        .route(
            "/login",
            get(pages::login_page::login_page_handler)
                .post(pages::login_page::login_submit_handler),
        )
        .route("/logout", post(pages::login_page::logout_handler))
        .route(
            "/create-blog",
            get(pages::create_blog_page::create_blog_handler)
                .post(pages::create_blog_page::create_blog_submit_handler),
        )
        .route(
            "/edit-blog/{slug}",
            get(pages::edit_blog_page::edit_blog_handler)
                .post(pages::edit_blog_page::edit_blog_submit_handler),
        )
        .route("/users", get(pages::users_page::users_handler))
        .route(
            "/email-templates",
            get(pages::email_template_list_page::list_handler),
        )
        .route(
            "/create-email-template",
            get(pages::create_email_template_page::create_handler)
                .post(pages::create_email_template_page::create_submit_handler),
        )
        .route(
            "/edit-email-template/{id}",
            get(pages::edit_email_template_page::edit_handler)
                .post(pages::edit_email_template_page::edit_submit_handler),
        )
        .route(
            "/delete-email-template/{id}",
            post(pages::email_template_list_page::delete_handler),
        )
        .route(
            "/forgot-password",
            get(pages::forgot_password_page::forgot_password_handler)
                .post(pages::forgot_password_page::forgot_password_submit_handler),
        )
        .route(
            "/reset-password",
            get(pages::reset_password_page::reset_password_handler)
                .post(pages::reset_password_page::reset_password_submit_handler),
        )
        .route("/policies", get(pages::policy_list_page::handler))
        .route(
            "/create-policy",
            get(pages::create_policy_page::create_handler)
                .post(pages::create_policy_page::create_submit_handler),
        )
        .route(
            "/policy/{id}/versions",
            get(pages::policy_versions_page::handler),
        )
        .route(
            "/create-policy-version/{policy_id}",
            get(pages::create_policy_version_page::create_handler)
                .post(pages::create_policy_version_page::create_submit_handler),
        )
        .route(
            "/edit-policy-version/{id}",
            get(pages::edit_policy_version_page::edit_handler)
                .post(pages::edit_policy_version_page::edit_submit_handler),
        )
        .route(
            "/publish-policy-version/{id}",
            post(pages::edit_policy_version_page::publish_handler),
        )
        .route(
            "/upload-image",
            post(pages::upload_image::upload_image_handler),
        )
        .route(
            "/media/{filename}",
            get(pages::upload_image::serve_media_handler),
        )
        .nest_service(
            "/static",
            ServeDir::new(
                std::env::var("STATIC_DIR")
                    .unwrap_or_else(|_| concat!(env!("CARGO_MANIFEST_DIR"), "/static").to_string()),
            ),
        )
        .fallback(pages::not_found_page::not_found_handler)
        .layer(middleware::from_fn(auth::csrf_middleware))
        .with_state(state.clone())
        // Token refresh runs outermost so every handler always sees a valid session.
        .layer(middleware::from_fn_with_state(
            state,
            auth::token_refresh_middleware,
        ));

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Website listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
