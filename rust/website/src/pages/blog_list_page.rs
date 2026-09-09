use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use axum::extract::Path;
use axum::response::{Html, Redirect};
use axum_extra::extract::cookie::CookieJar;
use cloudless_core::ports::api::blog_api_port::BlogApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;

use crate::components::{blog_card::BlogCard, layout::Layout};
use crate::error::WebsiteError;
use api_types::blog::BlogPostSummary;

#[component]
fn BlogListPage(posts: Vec<BlogPostSummary>, is_admin: bool) -> impl IntoView {
    view! {
        <div class="max-w-4xl mx-auto px-6 py-16">
            <div class="text-center mb-12">
                <h1 class="text-4xl md:text-5xl font-display font-bold gradient-text mb-4">"Blog"</h1>
                <p class="text-lg text-text-secondary">
                    "Insights on backup, security, and data protection."
                </p>
                {if is_admin {
                    view! {
                        <a
                            href="/create-blog"
                            class="inline-flex items-center gap-2 mt-4 btn-gradient text-white px-5 py-2 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl transition-all"
                        >
                            <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
                            </svg>
                            "New Post"
                        </a>
                    }.into_any()
                } else {
                    view! { <span /> }.into_any()
                }}
            </div>

            {if posts.is_empty() {
                view! {
                    <p class="text-center text-text-secondary text-lg">"No posts yet. Check back soon!"</p>
                }.into_any()
            } else {
                let is_admin_clone = is_admin;
                view! {
                    <div class="grid grid-cols-1 md:grid-cols-2 gap-6">
                        {posts.into_iter().map(|post| {
                            if is_admin_clone {
                                let slug = post.slug.clone();
                                let id = post.id;
                                let is_draft = !post.published;
                                view! {
                                    <div class="flex flex-col gap-2">
                                        {if is_draft {
                                            view! {
                                                <span class="self-start text-xs font-semibold px-2 py-0.5 rounded bg-yellow-500/20 text-yellow-400 border border-yellow-500/30">
                                                    "Draft"
                                                </span>
                                            }.into_any()
                                        } else {
                                            view! { <span /> }.into_any()
                                        }}
                                        <BlogCard post=post />
                                        <div class="flex gap-2 px-1">
                                            <a
                                                href=format!("/edit-blog/{slug}")
                                                class="text-xs text-primary-light hover:underline"
                                            >
                                                "Edit"
                                            </a>
                                            <form method="POST" action=format!("/delete-blog/{id}") class="inline">
                                                <button
                                                    type="submit"
                                                    class="text-xs text-red-400 hover:underline"
                                                    onclick="return confirm('Delete this post?')"
                                                >
                                                    "Delete"
                                                </button>
                                            </form>
                                        </div>
                                    </div>
                                }.into_any()
                            } else {
                                view! { <BlogCard post=post /> }.into_any()
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

pub async fn blog_list_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
) -> Result<Html<String>, WebsiteError> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    let is_admin = state.is_admin();
    let list = if is_admin {
        env.blog_api().list_all_admin().await?
    } else {
        env.blog_api().list_published().await?
    };

    let html = view! {
        <Layout
            title="Blog - CloudLess".to_string()
            auth=state
            description="CloudLess articles on encrypted backup, restore, ransomware recovery, zero-knowledge privacy, and bring-your-own storage.".to_string()
        >
            <BlogListPage posts=list.posts is_admin=is_admin />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}

pub async fn delete_blog_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    Path(id): Path<uuid::Uuid>,
) -> Result<Redirect, Redirect> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        return Err(Redirect::to("/login"));
    }

    let _ = env.blog_api().delete(id).await;

    Ok(Redirect::to("/blog"))
}
