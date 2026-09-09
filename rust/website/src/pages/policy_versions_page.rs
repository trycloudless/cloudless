use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use axum::{extract::Path, response::Html};
use axum_extra::extract::cookie::CookieJar;
use cloudless_core::ports::api::policy_api_port::PolicyApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;

use crate::{components::layout::Layout, error::WebsiteError};
use api_types::policy::PolicyVersionSummary;
use uuid::Uuid;

#[component]
fn PolicyVersionsPage(policy_id: Uuid, versions: Vec<PolicyVersionSummary>) -> impl IntoView {
    let create_url = format!("/create-policy-version/{}", policy_id);

    view! {
        <div class="max-w-4xl mx-auto px-6 py-16">
            <div class="flex items-center justify-between mb-8">
                <div>
                    <a href="/policies" class="text-sm text-text-secondary hover:text-primary-light transition-colors">"Back to Policies"</a>
                    <h1 class="text-4xl font-display font-bold gradient-text mt-2">"Policy Versions"</h1>
                </div>
                <a
                    href=create_url
                    class="inline-flex items-center gap-2 btn-gradient text-white px-5 py-2 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl transition-all"
                >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
                    </svg>
                    "New Version"
                </a>
            </div>

            {if versions.is_empty() {
                view! {
                    <p class="text-center text-text-secondary text-lg">"No versions yet. Create the first version."</p>
                }.into_any()
            } else {
                view! {
                    <div class="space-y-3">
                        {versions.into_iter().map(|v| {
                            let status_class = if v.status == "published" {
                                "bg-success-tint text-success border-success"
                            } else {
                                "bg-warning-tint text-warning border-warning"
                            };
                            let edit_url = format!("/edit-policy-version/{}", v.id);
                            let published_text = v.published_at
                                .map(|dt| format!("Published: {}", dt.format("%Y-%m-%d %H:%M")))
                                .unwrap_or_default();

                            view! {
                                <div class="bg-surface border border-border rounded-xl p-4 flex items-center justify-between">
                                    <div class="flex items-center gap-4">
                                        <span class="text-lg font-semibold text-text-primary">
                                            {format!("v{}", v.version)}
                                        </span>
                                        <span class={format!("text-xs px-2 py-1 rounded-lg border {}", status_class)}>
                                            {v.status.clone()}
                                        </span>
                                        <span class="text-sm text-text-secondary">{published_text}</span>
                                    </div>
                                    <a
                                        href=edit_url
                                        class="text-primary-light hover:text-primary text-sm font-medium transition-colors"
                                    >
                                        {if v.status == "draft" { "Edit" } else { "View" }}
                                    </a>
                                </div>
                            }
                        }).collect::<Vec<_>>()}
                    </div>
                }.into_any()
            }}
        </div>
    }
}

pub async fn handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    Path(policy_id): Path<Uuid>,
) -> Result<Html<String>, WebsiteError> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        let html = view! {
            <Layout title="Unauthorized - CloudLess".to_string() auth=state>
                <div class="max-w-2xl mx-auto px-6 py-24 text-center">
                    <h1 class="text-4xl font-display font-bold text-error mb-4">"Unauthorized"</h1>
                    <p class="text-text-secondary">"You don't have permission to view this page."</p>
                </div>
            </Layout>
        }
        .to_html();
        return Ok(Html(html));
    }

    let response = env.policy_api().list_versions(policy_id).await?;

    let html = view! {
        <Layout title="Policy Versions - CloudLess".to_string() auth=state>
            <PolicyVersionsPage policy_id=policy_id versions=response.list />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}
