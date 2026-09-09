use axum::http::{HeaderValue, header};
use axum::response::{IntoResponse, Response};
use cloudless_core::ports::api::blog_api_port::BlogApiPort;

use crate::auth::RequestEnv;
use crate::env::WebsiteEnv;

pub async fn robots_handler() -> Response {
    let env = std::env::var("SITE_ENV").unwrap_or_else(|_| "production".to_string());
    let base =
        std::env::var("SITE_BASE_URL").unwrap_or_else(|_| "https://trycloudless.io".to_string());
    let base = base.trim_end_matches('/');
    let body = if env == "production" {
        format!("User-agent: *\nAllow: /\n\nSitemap: {base}/sitemap.xml\n")
    } else {
        "User-agent: *\nDisallow: /\n".to_string()
    };
    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        )],
        body,
    )
        .into_response()
}

pub async fn sitemap_handler(RequestEnv(env): RequestEnv) -> Response {
    let base =
        std::env::var("SITE_BASE_URL").unwrap_or_else(|_| "https://trycloudless.io".to_string());
    let base = base.trim_end_matches('/');

    let static_pages = format!(
        r#"  <url><loc>{base}/</loc><changefreq>weekly</changefreq><priority>1.0</priority></url>
  <url><loc>{base}/pricing</loc><changefreq>monthly</changefreq><priority>0.8</priority></url>
  <url><loc>{base}/download</loc><changefreq>weekly</changefreq><priority>0.9</priority></url>
  <url><loc>{base}/blog</loc><changefreq>weekly</changefreq><priority>0.7</priority></url>
  <url><loc>{base}/legal/privacy-policy</loc><changefreq>monthly</changefreq><priority>0.3</priority></url>
  <url><loc>{base}/legal/terms-of-service</loc><changefreq>monthly</changefreq><priority>0.3</priority></url>
  <url><loc>{base}/legal/cookie-policy</loc><changefreq>monthly</changefreq><priority>0.3</priority></url>
  <url><loc>{base}/legal/acceptable-use</loc><changefreq>monthly</changefreq><priority>0.3</priority></url>
  <url><loc>{base}/legal/refund-and-cancellation</loc><changefreq>monthly</changefreq><priority>0.3</priority></url>
  <url><loc>{base}/legal/security-and-data-handling</loc><changefreq>monthly</changefreq><priority>0.3</priority></url>
  <url><loc>{base}/legal/subprocessors</loc><changefreq>monthly</changefreq><priority>0.3</priority></url>
  <url><loc>{base}/legal/data-retention-and-deletion</loc><changefreq>monthly</changefreq><priority>0.3</priority></url>"#
    );

    let blog_entries = match env.blog_api().list_published().await {
        Ok(list) => list
            .posts
            .into_iter()
            .filter(|p| p.published && !p.is_page)
            .map(|p| {
                format!(
                    r#"  <url><loc>{base}/blog/{}</loc><changefreq>monthly</changefreq><priority>0.7</priority></url>"#,
                    p.slug
                )
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Err(_) => String::new(),
    };

    let entries = if blog_entries.is_empty() {
        static_pages
    } else {
        format!("{static_pages}\n{blog_entries}")
    };

    let body = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n{entries}\n</urlset>\n"
    );

    (
        [(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/xml; charset=utf-8"),
        )],
        body,
    )
        .into_response()
}
