use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use axum::{extract::Path, response::Html};
use axum_extra::extract::cookie::CookieJar;
use cloudless_core::ports::api::blog_api_port::BlogApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;

use crate::{components::layout::Layout, error::WebsiteError};
use api_types::blog::BlogPostResponse;

#[component]
fn BlogPostPage(post: BlogPostResponse, is_admin: bool) -> impl IntoView {
    let date = post.created_at.format("%B %d, %Y").to_string();
    let title = post.title.clone();
    let summary = post.summary.clone();
    let slug = post.slug.clone();
    let has_summary = !summary.is_empty();

    view! {
        <article class="max-w-3xl mx-auto px-6 py-16">
            <header class="mb-10">
                <div class="flex items-center justify-between">
                    <time class="text-sm text-text-secondary uppercase tracking-wider">{date}</time>
                    {if is_admin {
                        view! {
                            <a
                                href=format!("/edit-blog/{}", slug)
                                class="inline-flex items-center gap-1.5 text-sm text-primary-light hover:text-text-primary border border-primary-glow hover:border-primary/60 px-3 py-1.5 rounded-lg transition-all"
                            >
                                <svg class="w-3.5 h-3.5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M11 5H6a2 2 0 00-2 2v11a2 2 0 002 2h11a2 2 0 002-2v-5m-1.414-9.414a2 2 0 112.828 2.828L11.828 15H9v-2.828l8.586-8.586z" />
                                </svg>
                                "Edit"
                            </a>
                        }.into_any()
                    } else {
                        view! { <span /> }.into_any()
                    }}
                </div>
                <h1 class="text-4xl md:text-5xl font-display font-bold gradient-text mt-3 mb-4">
                    {title}
                </h1>
                {if has_summary {
                    view! {
                        <p class="text-lg text-text-secondary">{summary}</p>
                    }.into_any()
                } else {
                    view! { <span /> }.into_any()
                }}
            </header>

            <div
                class="prose prose-invert prose-lg max-w-none
                    [&_h2]:text-2xl [&_h2]:font-display [&_h2]:font-bold [&_h2]:text-text-primary [&_h2]:mt-10 [&_h2]:mb-4
                    [&_h3]:text-xl [&_h3]:font-semibold [&_h3]:text-text-primary [&_h3]:mt-8 [&_h3]:mb-3
                    [&_p]:text-text-secondary [&_p]:leading-relaxed [&_p]:mb-4
                    [&_a]:text-primary-light [&_a]:hover:text-text-primary [&_a]:transition-colors
                    [&_ul]:text-text-secondary [&_ul]:space-y-2 [&_ul]:my-4 [&_ul]:list-disc [&_ul]:pl-6
                    [&_ol]:text-text-secondary [&_ol]:space-y-2 [&_ol]:my-4 [&_ol]:list-decimal [&_ol]:pl-6
                    [&_code]:bg-surface [&_code]:px-2 [&_code]:py-0.5 [&_code]:rounded [&_code]:text-sm [&_code]:text-accent-light
                    [&_pre]:bg-surface [&_pre]:border [&_pre]:border-border [&_pre]:rounded-xl [&_pre]:p-4 [&_pre]:overflow-x-auto
                    [&_blockquote]:border-l-4 [&_blockquote]:border-primary [&_blockquote]:pl-4 [&_blockquote]:text-text-secondary [&_blockquote]:italic"
                inner_html=crate::markdown::markdown_to_safe_html(&post.content)
            />

            <div class="mt-12 pt-8 border-t border-border">
                <a href="/blog" class="text-primary-light hover:text-text-primary font-medium transition-colors">
                    "Back to Blog"
                </a>
            </div>
        </article>
    }
}

pub async fn blog_post_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    Path(slug): Path<String>,
) -> Result<Html<String>, WebsiteError> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    let is_admin = state.is_admin();
    let post = env.blog_api().get_by_slug(&slug).await?;
    let title = format!("{} - CloudLess", &post.title);
    let description = post.summary.clone();
    let og_image = post.og_image.clone().unwrap_or_default();

    let html = view! {
        <Layout title=title auth=state description=description og_image=og_image>
            <BlogPostPage post=post is_admin=is_admin />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}
