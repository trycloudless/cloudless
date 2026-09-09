use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use axum::{
    extract::Path,
    response::{Html, Redirect},
};
use axum_extra::extract::cookie::CookieJar;
use cloudless_core::ports::api::blog_api_port::BlogApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;
use serde::Deserialize;
use shared_ui::display::{ErrorAlert, SuccessAlert};

use crate::{
    components::{layout::Layout, markdown_editor::MarkdownEditor},
    error::WebsiteError,
};
use api_types::blog::{BlogPostResponse, UpdateBlogPostRequest};

#[derive(Deserialize)]
pub struct EditBlogFormData {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub content: String,
    pub published: Option<String>,
    pub is_page: Option<String>,
    pub og_image: Option<String>,
}

#[component]
fn EditBlogPage(
    post: BlogPostResponse,
    success: Option<String>,
    error: Option<String>,
) -> impl IntoView {
    let id = post.id.to_string();
    let slug_val = post.slug.clone();
    let title_val = post.title.clone();
    let summary_val = post.summary.clone();
    let content_val = post.content.clone();
    let is_published = post.published;
    let is_page = post.is_page;
    let og_image_val = post.og_image.clone().unwrap_or_default();
    let has_og_image = !og_image_val.is_empty();
    let og_image_src = og_image_val.clone();

    let cancel_href = format!("/blog/{}", post.slug);

    view! {
        <div class="max-w-7xl mx-auto px-6 py-16">
            <h1 class="text-4xl font-display font-bold gradient-text mb-8">"Edit Blog Post"</h1>

            {success.map(|msg| view! { <SuccessAlert message=msg /> })}
            {error.map(|e| view! { <ErrorAlert message=e /> })}

            <form method="POST" class="space-y-5">
                <input type="hidden" name="id" value=id />

                <div>
                    <label class="block text-sm font-medium text-text-secondary mb-2">"Slug"</label>
                    <input
                        type="text"
                        name="slug"
                        value=slug_val
                        class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                        required
                    />
                </div>

                <div>
                    <label class="block text-sm font-medium text-text-secondary mb-2">"Title"</label>
                    <input
                        type="text"
                        name="title"
                        value=title_val
                        class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                        required
                    />
                </div>

                <div>
                    <label class="block text-sm font-medium text-text-secondary mb-2">"Summary"</label>
                    <input
                        type="text"
                        name="summary"
                        value=summary_val
                        class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                    />
                </div>

                <MarkdownEditor initial_content=content_val />

                // OG / Social image
                <input type="hidden" name="og_image" id="og-image-field" value=og_image_val />
                <div class="space-y-2">
                    <label class="block text-sm font-medium text-text-secondary">"Social Preview Image (OG Image)"</label>
                    <div
                        id="og-image-set"
                        class="flex items-center gap-3"
                        style:display=move || if has_og_image { "flex" } else { "none" }
                    >
                        <img
                            id="og-image-preview"
                            src=og_image_src
                            class="h-16 rounded-lg object-cover border border-border"
                            alt="OG preview"
                        />
                        <button
                            type="button"
                            onclick="clearOgImage()"
                            class="text-sm text-error hover:text-error-light transition-colors"
                        >
                            "Clear"
                        </button>
                    </div>
                    <div
                        id="og-image-none"
                        class="text-xs text-text-secondary"
                        style:display=move || if has_og_image { "none" } else { "block" }
                    >
                        "No OG image set. Upload an image above and click \u{2018}Set as OG Image\u{2019}."
                    </div>
                </div>

                <div class="flex gap-6">
                    <label class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer">
                        <input type="checkbox" name="published" value="true" class="rounded border-border" checked=is_published />
                        "Published"
                    </label>
                    <label class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer">
                        <input type="checkbox" name="is_page" value="true" class="rounded border-border" checked=is_page />
                        "Is Page (home/pricing)"
                    </label>
                </div>

                <div class="flex gap-4">
                    <button
                        type="submit"
                        class="btn-gradient text-white px-8 py-3 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl hover:-translate-y-0.5 active:translate-y-0 transition-all"
                    >
                        "Update Post"
                    </button>
                    <a
                        href=cancel_href
                        class="border border-border text-text-secondary px-8 py-3 rounded-xl font-semibold hover:bg-surface transition-all"
                    >
                        "Cancel"
                    </a>
                </div>
            </form>
        </div>
    }
}

pub async fn edit_blog_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    Path(slug): Path<String>,
) -> Result<Html<String>, WebsiteError> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        let html = view! {
            <Layout title="Unauthorized - CloudLess".to_string() auth=state>
                <div class="max-w-2xl mx-auto px-6 py-24 text-center">
                    <h1 class="text-4xl font-display font-bold text-error mb-4">"Unauthorized"</h1>
                    <p class="text-text-secondary">"You don't have permission to edit this post."</p>
                </div>
            </Layout>
        }
        .to_html();
        return Ok(Html(html));
    }

    let post = env.blog_api().get_by_slug_admin(&slug).await?;

    let html = view! {
        <Layout title=format!("Edit: {} - CloudLess", &post.title) auth=state editor_mode=true>
            <EditBlogPage post=post success=None error=None />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}

pub async fn edit_blog_submit_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    Path(slug): Path<String>,
    axum::Form(form): axum::Form<EditBlogFormData>,
) -> Result<Html<String>, Redirect> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        return Err(Redirect::to("/login"));
    }

    let id = uuid::Uuid::parse_str(&form.id).map_err(|_| Redirect::to("/blog"))?;

    let req = UpdateBlogPostRequest {
        id,
        slug: Some(form.slug),
        title: Some(form.title),
        summary: Some(form.summary),
        content: Some(form.content),
        published: Some(form.published.is_some()),
        is_page: Some(form.is_page.is_some()),
        og_image: form.og_image.filter(|s| !s.is_empty()),
    };

    let result = env.blog_api().update(req).await;

    match result {
        Ok(post) => {
            let refetched = env.blog_api().get_by_slug_admin(&post.slug).await;
            let display_post = refetched.unwrap_or(post);
            let html = view! {
                <Layout title=format!("Edit: {} - CloudLess", &display_post.title) auth=state editor_mode=true>
                    <EditBlogPage post=display_post success=Some("Post updated successfully!".to_string()) error=None />
                </Layout>
            }
            .to_html();
            Ok(Html(html))
        }
        Err(e) => {
            let original = env.blog_api().get_by_slug_admin(&slug).await;
            match original {
                Ok(post) => {
                    let html = view! {
                        <Layout title=format!("Edit: {} - CloudLess", &post.title) auth=state editor_mode=true>
                            <EditBlogPage post=post success=None error=Some(format!("{}", e)) />
                        </Layout>
                    }
                    .to_html();
                    Ok(Html(html))
                }
                Err(_) => Err(Redirect::to("/blog")),
            }
        }
    }
}
