use crate::auth::RequestEnv;
use crate::env::WebsiteEnv;
use axum::{extract::Path, response::Html};
use cloudless_core::ports::api::policy_api_port::PolicyApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;

use crate::components::layout::Layout;
use crate::error::WebsiteError;
use api_types::policy::PublicPolicyResponse;

#[component]
fn LegalPage(policy: PublicPolicyResponse) -> impl IntoView {
    let date = policy.published_at.format("%B %d, %Y").to_string();
    let version_label = format!("Version {}", policy.version);
    let content_html = crate::markdown::markdown_to_safe_html(&policy.content);

    view! {
        <article class="max-w-3xl mx-auto px-6 py-16">
            <header class="mb-10">
                <div class="flex items-center gap-3 mb-3">
                    <time class="text-sm text-text-secondary uppercase tracking-wider">{date}</time>
                    <span class="text-text-secondary/40">"-"</span>
                    <span class="text-sm text-text-secondary">{version_label}</span>
                </div>
                <h1 class="text-4xl md:text-5xl font-display font-bold gradient-text">
                    {policy.title}
                </h1>
            </header>

            <div
                class="prose prose-invert prose-lg max-w-none
                    [&_h1]:hidden
                    [&_h2]:text-2xl [&_h2]:font-display [&_h2]:font-bold [&_h2]:text-text-primary [&_h2]:mt-10 [&_h2]:mb-4
                    [&_h3]:text-xl [&_h3]:font-semibold [&_h3]:text-text-primary [&_h3]:mt-8 [&_h3]:mb-3
                    [&_p]:text-text-secondary [&_p]:leading-relaxed [&_p]:mb-4
                    [&_a]:text-primary-light [&_a]:hover:text-text-primary [&_a]:transition-colors
                    [&_ul]:text-text-secondary [&_ul]:space-y-2 [&_ul]:my-4 [&_ul]:list-disc [&_ul]:pl-6
                    [&_ol]:text-text-secondary [&_ol]:space-y-2 [&_ol]:my-4 [&_ol]:list-decimal [&_ol]:pl-6
                    [&_code]:bg-surface [&_code]:px-2 [&_code]:py-0.5 [&_code]:rounded [&_code]:text-sm [&_code]:text-accent-light
                    [&_pre]:bg-surface [&_pre]:border [&_pre]:border-border [&_pre]:rounded-xl [&_pre]:p-4 [&_pre]:overflow-x-auto
                    [&_blockquote]:border-l-4 [&_blockquote]:border-primary [&_blockquote]:pl-4 [&_blockquote]:text-text-secondary [&_blockquote]:italic"
                inner_html=content_html
            />

            <div class="mt-12 pt-8 border-t border-border">
                <a href="/" class="text-primary-light hover:text-text-primary font-medium transition-colors">
                    "Back to Home"
                </a>
            </div>
        </article>
    }
}

pub async fn legal_page_handler(
    RequestEnv(env): RequestEnv,
    Path(slug): Path<String>,
) -> Result<Html<String>, WebsiteError> {
    let policy = match env.policy_api().get_public_policy(&slug).await? {
        Some(p) => p,
        None => return Err(WebsiteError::NotFound(slug)),
    };

    let page_title = format!("{} - CloudLess", policy.title);

    let html = view! {
        <Layout title=page_title>
            <LegalPage policy=policy />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}
