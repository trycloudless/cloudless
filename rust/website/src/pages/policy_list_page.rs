use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use axum::response::Html;
use axum_extra::extract::cookie::CookieJar;
use cloudless_core::ports::api::policy_api_port::PolicyApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;

use crate::{components::layout::Layout, error::WebsiteError};
use api_types::policy::PolicyResponse;

#[component]
fn PolicyListPage(policies: Vec<PolicyResponse>) -> impl IntoView {
    view! {
        <div class="max-w-4xl mx-auto px-6 py-16">
            <div class="flex items-center justify-between mb-8">
                <h1 class="text-4xl font-display font-bold gradient-text">"Policies"</h1>
                <a
                    href="/create-policy"
                    class="inline-flex items-center gap-2 btn-gradient text-white px-5 py-2 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl transition-all"
                >
                    <svg class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24">
                        <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 4v16m8-8H4" />
                    </svg>
                    "New Policy"
                </a>
            </div>

            {if policies.is_empty() {
                view! {
                    <p class="text-center text-text-secondary text-lg">"No policies yet. Create one to get started."</p>
                }.into_any()
            } else {
                view! {
                    <div class="space-y-4">
                        {policies.into_iter().map(|policy| {
                            let policy_type = policy.policy_type.to_string();
                            let versions_url = format!("/policy/{}/versions", policy.id);
                            view! {
                                <div class="bg-surface border border-border rounded-xl p-5 flex items-center justify-between">
                                    <div>
                                        <h3 class="text-lg font-semibold text-text-primary">{policy.title}</h3>
                                        <p class="text-sm text-text-secondary mt-1">
                                            {format!("Type: {} | Required: {} | Max skips: {}", policy_type, policy.is_required, policy.max_skips)}
                                        </p>
                                        {if !policy.description.is_empty() {
                                            view! { <p class="text-sm text-text-secondary mt-1">{policy.description}</p> }.into_any()
                                        } else {
                                            view! { <span /> }.into_any()
                                        }}
                                    </div>
                                    <a
                                        href=versions_url
                                        class="text-primary-light hover:text-primary text-sm font-medium transition-colors"
                                    >
                                        "Manage Versions"
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

    let response = env.policy_api().list_policies().await?;

    let html = view! {
        <Layout title="Policies - CloudLess".to_string() auth=state>
            <PolicyListPage policies=response.list />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}
