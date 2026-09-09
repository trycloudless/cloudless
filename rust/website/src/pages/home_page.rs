use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use axum::response::Html;
use axum_extra::extract::cookie::CookieJar;
use cloudless_core::ports::api::blog_api_port::BlogApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;

use crate::{components::layout::Layout, error::WebsiteError};

#[component]
fn HomePage(content: String) -> impl IntoView {
    view! {
        <div class="max-w-4xl mx-auto px-6 py-16">
            <div inner_html=crate::markdown::markdown_to_safe_html(&content) />
        </div>
    }
}

#[component]
fn DefaultHomePage() -> impl IntoView {
    view! {
        <div class="max-w-4xl mx-auto px-6 py-16">

            // Hero
            <div class="text-center mb-16">
                <h1 class="text-5xl md:text-6xl font-display font-bold text-white mb-6">
                    "Encrypted backup to storage you control"
                </h1>
                <p class="text-xl text-text-secondary max-w-2xl mx-auto mb-10">
                    "CloudLess encrypts files on your device before upload, then stores backups in storage you choose: AWS S3, Google Drive, Microsoft OneDrive, SFTP, or an external disk. You keep the keys, you keep the storage, and you can restore files when something goes wrong."
                </p>
                <div class="flex flex-col sm:flex-row gap-4 justify-center">
                    <a
                        href="/download"
                        class="btn-gradient text-white px-8 py-3 rounded-xl font-semibold text-lg shadow-lg shadow-primary-tint hover:shadow-xl hover:-translate-y-0.5 transition-all inline-block"
                    >
                        "Download for macOS"
                    </a>
                    <a
                        href="/pricing"
                        class="border border-primary text-primary-light px-8 py-3 rounded-xl font-semibold text-lg hover:bg-primary-tint transition-all inline-block"
                    >
                        "View pricing"
                    </a>
                </div>
            </div>

            // Trust strip
            <div class="flex flex-wrap justify-center gap-3 mb-20">
                {["Private by design", "S3, Drive, OneDrive, SFTP, or disk", "Built for restore", "No access to your files"]
                    .iter()
                    .map(|label| view! {
                        <span class="inline-flex items-center gap-2 bg-primary-tint border border-primary-ring rounded-full px-4 py-2 text-sm text-primary-light font-medium">
                            <span class="w-1.5 h-1.5 rounded-full bg-primary-light"></span>
                            {*label}
                        </span>
                    })
                    .collect_view()}
            </div>

            // How it works
            <div class="mb-20">
                <h2 class="text-3xl font-display font-bold text-text-primary text-center mb-10">
                    "How it works"
                </h2>
                <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-6">
                    {[
                        ("1", "Select folders", "Choose which folders on your device to back up."),
                        ("2", "Connect storage", "Use your own cloud account, SFTP server, or external disk."),
                        ("3", "Encrypt and upload", "CloudLess encrypts your files locally and uploads backup versions."),
                        ("4", "Restore when needed", "Recover files to any device."),
                    ]
                    .iter()
                    .map(|(num, title, desc)| view! {
                        <div class="glass bg-surface border border-border rounded-2xl p-6">
                            <div class="w-10 h-10 bg-primary-tint rounded-xl flex items-center justify-center mb-4">
                                <span class="text-primary-light font-bold text-lg">{*num}</span>
                            </div>
                            <h3 class="text-base font-semibold text-text-primary mb-2">{*title}</h3>
                            <p class="text-sm text-text-secondary">{*desc}</p>
                        </div>
                    })
                    .collect_view()}
                </div>
            </div>

            // Security section
            <div class="mb-20">
                <h2 class="text-3xl font-display font-bold text-text-primary text-center mb-10">
                    "Your data stays private"
                </h2>
                <div class="glass bg-surface border border-border rounded-2xl p-8 max-w-2xl mx-auto">
                    <ul class="space-y-4">
                        {[
                            "Files are encrypted before they leave your device.",
                            "CloudLess does not store your file contents.",
                            "CloudLess does not have your encryption keys.",
                            "Storage stays in your own account, server, or local filesystem.",
                        ]
                        .iter()
                        .map(|point| view! {
                            <li class="flex items-start gap-3">
                                <svg class="w-5 h-5 text-primary-light mt-0.5 shrink-0" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                                    <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M5 13l4 4L19 7" />
                                </svg>
                                <span class="text-text-secondary">{*point}</span>
                            </li>
                        })
                        .collect_view()}
                    </ul>
                </div>
            </div>

            // Bottom CTA
            <div class="text-center">
                <h2 class="text-3xl font-display font-bold text-text-primary mb-6">
                    "Ready to protect your files?"
                </h2>
                <div class="flex flex-col sm:flex-row gap-4 justify-center">
                    <a
                        href="/download"
                        class="btn-gradient text-white px-8 py-3 rounded-xl font-semibold text-lg shadow-lg shadow-primary-tint hover:shadow-xl hover:-translate-y-0.5 transition-all inline-block"
                    >
                        "Download for macOS"
                    </a>
                    <a
                        href="/pricing"
                        class="border border-primary text-primary-light px-8 py-3 rounded-xl font-semibold text-lg hover:bg-primary-tint transition-all inline-block"
                    >
                        "View pricing"
                    </a>
                </div>
            </div>
        </div>
    }
}

pub async fn home_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
) -> Result<Html<String>, WebsiteError> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    let maybe_post = env.blog_api().get_by_slug("home").await;

    let html = match maybe_post {
        Ok(post) => view! {
            <Layout title="CloudLess - Secure Cloud Backup".to_string() auth=state description="Encrypted backup to storage you control. CloudLess supports AWS S3, Google Drive, Microsoft OneDrive, SFTP, and local filesystem storage.".to_string()>
                <HomePage content=post.content />
            </Layout>
        }
        .to_html(),
        Err(_) => view! {
            <Layout title="CloudLess - Secure Cloud Backup".to_string() auth=state description="Encrypted backup to storage you control. CloudLess supports AWS S3, Google Drive, Microsoft OneDrive, SFTP, and local filesystem storage.".to_string()>
                <DefaultHomePage />
            </Layout>
        }
        .to_html(),
    };

    Ok(Html(html))
}
