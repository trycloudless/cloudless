use axum::response::{Html, Redirect};
use axum_extra::extract::cookie::CookieJar;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;

use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::components::layout::Layout;

#[component]
fn AdminCard(href: &'static str, title: &'static str, description: &'static str) -> impl IntoView {
    view! {
        <a
            href=href
            class="glass bg-surface border border-border rounded-2xl p-6 hover:border-primary-ring hover:bg-primary-tint transition-all block"
        >
            <h3 class="text-base font-semibold text-text-primary mb-1">{title}</h3>
            <p class="text-sm text-text-secondary">{description}</p>
        </a>
    }
}

#[component]
fn AdminDashboardPage() -> impl IntoView {
    view! {
        <div class="max-w-4xl mx-auto px-6 py-16">
            <div class="mb-10">
                <h1 class="text-3xl font-display font-bold text-white mb-2">"Admin Dashboard"</h1>
                <p class="text-text-secondary">"Manage content, users, and policies."</p>
            </div>

            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
                <AdminCard
                    href="/users"
                    title="Users"
                    description="View and manage user accounts."
                />
                <AdminCard
                    href="/blog"
                    title="Blog Posts"
                    description="View, edit, and delete published blog posts."
                />
                <AdminCard
                    href="/create-blog"
                    title="Create Blog Post"
                    description="Write and publish a new blog post."
                />
                <AdminCard
                    href="/upload-image"
                    title="Media Uploads"
                    description="Upload images for use in blog posts."
                />
                <AdminCard
                    href="/policies"
                    title="Policies"
                    description="Manage legal policy documents and versions."
                />
                <AdminCard
                    href="/email-templates"
                    title="Email Templates"
                    description="Edit transactional email templates."
                />
            </div>
        </div>
    }
}

pub async fn admin_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
) -> Result<Html<String>, Redirect> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        return Err(Redirect::to("/login"));
    }

    let html = view! {
        <Layout title="Admin - CloudLess".to_string() auth=state>
            <AdminDashboardPage />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}
