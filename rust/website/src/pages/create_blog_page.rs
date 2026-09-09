use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use axum::response::{Html, Redirect};
use axum_extra::extract::cookie::CookieJar;
use cloudless_core::ports::api::blog_api_port::BlogApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;
use serde::Deserialize;
use shared_ui::display::{ErrorAlert, SuccessAlert};

use crate::components::{layout::Layout, markdown_editor::MarkdownEditor};
use api_types::blog::CreateBlogPostRequest;

#[derive(Deserialize)]
pub struct CreateBlogFormData {
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub content: String,
    pub published: Option<String>,
    pub is_page: Option<String>,
    pub og_image: Option<String>,
}

#[component]
fn CreateBlogPage(success: Option<String>, error: Option<String>) -> impl IntoView {
    view! {
        <div class="max-w-7xl mx-auto px-6 py-16">
            <h1 class="text-4xl font-display font-bold gradient-text mb-8">"Create Blog Post"</h1>

            {success.map(|msg| view! { <SuccessAlert message=msg /> })}
            {error.map(|e| view! { <ErrorAlert message=e /> })}

            <form method="POST" action="/create-blog" class="space-y-5">
                <div>
                    <label class="block text-sm font-medium text-text-secondary mb-2">"Slug"</label>
                    <input
                        type="text"
                        name="slug"
                        placeholder="my-blog-post"
                        class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                        required
                    />
                    <p class="text-xs text-text-secondary mt-1">"URL-friendly identifier. Use 'home' or 'pricing' for page content."</p>
                </div>

                <div>
                    <label class="block text-sm font-medium text-text-secondary mb-2">"Title"</label>
                    <input
                        type="text"
                        name="title"
                        placeholder="Blog Post Title"
                        class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                        required
                    />
                </div>

                <div>
                    <label class="block text-sm font-medium text-text-secondary mb-2">"Summary"</label>
                    <input
                        type="text"
                        name="summary"
                        placeholder="A brief description of the post"
                        class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                    />
                </div>

                <MarkdownEditor />

                // OG / Social image
                <input type="hidden" name="og_image" id="og-image-field" />
                <div class="space-y-2">
                    <label class="block text-sm font-medium text-text-secondary">"Social Preview Image (OG Image)"</label>
                    <div id="og-image-set" class="flex items-center gap-3" style="display:none">
                        <img
                            id="og-image-preview"
                            src=""
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
                    <div id="og-image-none" class="text-xs text-text-secondary">
                        "No OG image set. Upload an image above and click \u{2018}Set as OG Image\u{2019}."
                    </div>
                </div>

                <div class="flex gap-6">
                    <label class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer">
                        <input type="checkbox" name="published" value="true" class="rounded border-border" />
                        "Published"
                    </label>
                    <label class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer">
                        <input type="checkbox" name="is_page" value="true" class="rounded border-border" />
                        "Is Page (home/pricing)"
                    </label>
                </div>

                <button
                    type="submit"
                    class="btn-gradient text-white px-8 py-3 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl hover:-translate-y-0.5 active:translate-y-0 transition-all"
                >
                    "Create Post"
                </button>
            </form>
        </div>
    }
}

pub async fn create_blog_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
) -> Result<Html<String>, Redirect> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        return Err(Redirect::to("/login"));
    }

    let html = view! {
        <Layout title="Create Blog Post - CloudLess".to_string() auth=state editor_mode=true>
            <CreateBlogPage success=None error=None />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}

pub async fn create_blog_submit_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    axum::Form(form): axum::Form<CreateBlogFormData>,
) -> Result<Html<String>, Redirect> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        return Err(Redirect::to("/login"));
    }

    let req = CreateBlogPostRequest {
        slug: form.slug,
        title: form.title,
        summary: form.summary,
        content: form.content,
        published: form.published.is_some(),
        is_page: form.is_page.is_some(),
        og_image: form.og_image.filter(|s| !s.is_empty()),
    };

    let result = env.blog_api().create(req).await;

    let (success, error) = match result {
        Ok(post) => (
            Some(format!("Post '{}' created successfully!", post.title)),
            None,
        ),
        Err(e) => (None, Some(format!("{}", e))),
    };

    let html = view! {
        <Layout title="Create Blog Post - CloudLess".to_string() auth=state editor_mode=true>
            <CreateBlogPage success=success error=error />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}
