use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use axum::response::Html;
use axum_extra::extract::cookie::CookieJar;
use cloudless_core::ports::api::blog_api_port::BlogApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;

use crate::{components::layout::Layout, error::WebsiteError};

#[component]
fn PricingPage(content: String) -> impl IntoView {
    view! {
        <div class="max-w-4xl mx-auto px-6 py-16">
            <div inner_html=crate::markdown::markdown_to_safe_html(&content) />
        </div>
    }
}

#[component]
fn CheckIcon() -> impl IntoView {
    view! {
        <svg class="w-5 h-5 text-accent flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
        </svg>
    }
}

#[component]
fn CrossIcon() -> impl IntoView {
    view! {
        <svg class="w-5 h-5 text-text-secondary/40 flex-shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" />
        </svg>
    }
}

#[component]
fn DefaultPricingPage() -> impl IntoView {
    view! {
        <div class="max-w-6xl mx-auto px-6 py-16">
            <div class="text-center mb-12">
                <h1 class="text-4xl md:text-5xl font-display font-bold text-white mb-4">
                    "Simple pricing"
                </h1>
                <p class="text-lg text-text-secondary">
                    "Bring your own storage. Pay only for what you use."
                </p>
                <p class="text-sm text-text-secondary/70 mt-2">
                    "Use AWS S3, Google Drive, Microsoft OneDrive, SFTP, or local filesystem storage."
                </p>
            </div>

            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-6">
                // Free
                <div class="glass bg-surface border border-border rounded-2xl p-6 flex flex-col">
                    <h3 class="text-lg font-semibold text-text-secondary uppercase tracking-wider mb-2">"Free"</h3>
                    <div class="text-4xl font-display font-bold text-text-primary mb-6">
                        "$0"
                        <span class="text-lg text-text-secondary font-normal">"/month"</span>
                    </div>
                    <ul class="space-y-3 text-sm text-text-secondary mb-8 flex-1">
                        <li class="flex items-center gap-2">
                            <CheckIcon />
                            "1 device"
                        </li>
                        <li class="flex items-center gap-2">
                            <CheckIcon />
                            "1 backup configuration"
                        </li>
                        <li class="flex items-center gap-2">
                            <CheckIcon />
                            "7-day metadata retention"
                        </li>
                        <li class="flex items-center gap-2">
                            <CrossIcon />
                            "No scheduled backups"
                        </li>
                    </ul>
                    <a
                        href="/login"
                        class="block w-full text-center border border-primary text-primary-light py-3 rounded-xl font-semibold hover:bg-primary-tint transition-all"
                    >
                        "Get Started Free"
                    </a>
                </div>

                // Starter
                <div class="glass bg-surface border border-border rounded-2xl p-6 flex flex-col">
                    <h3 class="text-lg font-semibold text-text-secondary uppercase tracking-wider mb-2">"Starter"</h3>
                    <div class="text-4xl font-display font-bold text-text-primary mb-6">
                        "$5"
                        <span class="text-lg text-text-secondary font-normal">"/month"</span>
                    </div>
                    <ul class="space-y-3 text-sm text-text-secondary mb-8 flex-1">
                        <li class="flex items-center gap-2">
                            <CheckIcon />
                            "3 devices"
                        </li>
                        <li class="flex items-center gap-2">
                            <CheckIcon />
                            "3 backup configurations"
                        </li>
                        <li class="flex items-center gap-2">
                            <CheckIcon />
                            "Unlimited metadata retention"
                        </li>
                        <li class="flex items-center gap-2">
                            <CheckIcon />
                            "Scheduled backups"
                        </li>
                    </ul>
                    <a
                        href="/checkout?tier=starter"
                        class="block w-full text-center border border-primary text-primary-light py-3 rounded-xl font-semibold hover:bg-primary-tint transition-all"
                    >
                        "Get Starter"
                    </a>
                </div>

                // Pro (Popular)
                <div class="glass bg-surface border border-primary-glow rounded-2xl p-6 flex flex-col relative">
                    <div class="absolute -top-3 left-1/2 -translate-x-1/2 bg-primary text-white text-xs px-3 py-1 rounded-full font-semibold">
                        "Popular"
                    </div>
                    <h3 class="text-lg font-semibold text-text-secondary uppercase tracking-wider mb-2">"Pro"</h3>
                    <div class="text-4xl font-display font-bold text-text-primary mb-6">
                        "$9"
                        <span class="text-lg text-text-secondary font-normal">"/month"</span>
                    </div>
                    <ul class="space-y-3 text-sm text-text-secondary mb-8 flex-1">
                        <li class="flex items-center gap-2">
                            <CheckIcon />
                            "Unlimited devices"
                        </li>
                        <li class="flex items-center gap-2">
                            <CheckIcon />
                            "Unlimited backup configurations"
                        </li>
                        <li class="flex items-center gap-2">
                            <CheckIcon />
                            "Unlimited metadata retention"
                        </li>
                        <li class="flex items-center gap-2">
                            <CheckIcon />
                            "Scheduled backups"
                        </li>
                    </ul>
                    <a
                        href="/checkout?tier=pro"
                        class="block w-full text-center btn-gradient text-white py-3 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl transition-all"
                    >
                        "Get Pro"
                    </a>
                </div>

                // Lifetime
                <div class="glass bg-surface border border-border rounded-2xl p-6 flex flex-col">
                    <h3 class="text-lg font-semibold text-text-secondary uppercase tracking-wider mb-2">"Lifetime"</h3>
                    <div class="text-4xl font-display font-bold text-text-primary mb-6">
                        "$199"
                        <span class="text-lg text-text-secondary font-normal">" once"</span>
                    </div>
                    <ul class="space-y-3 text-sm text-text-secondary mb-8 flex-1">
                        <li class="flex items-center gap-2">
                            <CheckIcon />
                            "Unlimited devices"
                        </li>
                        <li class="flex items-center gap-2">
                            <CheckIcon />
                            "Unlimited backup configurations"
                        </li>
                        <li class="flex items-center gap-2">
                            <CheckIcon />
                            "Unlimited metadata retention"
                        </li>
                        <li class="flex items-center gap-2">
                            <CheckIcon />
                            "Scheduled backups"
                        </li>
                    </ul>
                    <a
                        href="/checkout?tier=lifetime"
                        class="block w-full text-center border border-primary text-primary-light py-3 rounded-xl font-semibold hover:bg-primary-tint transition-all"
                    >
                        "Buy Lifetime"
                    </a>
                </div>
            </div>
            <p class="text-center text-xs text-text-secondary/60 mt-8">
                "All plans use storage you control. If a storage provider charges separately, those costs are billed by that provider, not CloudLess."
            </p>
        </div>
    }
}

pub async fn pricing_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
) -> Result<Html<String>, WebsiteError> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    let maybe_post = env.blog_api().get_by_slug("pricing").await;

    let html = match maybe_post {
        Ok(post) => view! {
            <Layout title="Pricing - CloudLess".to_string() auth=state description="CloudLess plans with a free tier. Use AWS S3, Google Drive, Microsoft OneDrive, SFTP, or local filesystem storage with client-side encryption.".to_string()>
                <PricingPage content=post.content />
            </Layout>
        }
        .to_html(),
        Err(_) => view! {
            <Layout title="Pricing - CloudLess".to_string() auth=state description="CloudLess plans with a free tier. Use AWS S3, Google Drive, Microsoft OneDrive, SFTP, or local filesystem storage with client-side encryption.".to_string()>
                <DefaultPricingPage />
            </Layout>
        }
        .to_html(),
    };

    Ok(Html(html))
}
