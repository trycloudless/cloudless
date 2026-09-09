use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use axum::response::{Html, Redirect};
use axum_extra::extract::cookie::CookieJar;
use cloudless_core::ports::api::policy_api_port::PolicyApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;
use serde::Deserialize;
use shared_ui::display::{ErrorAlert, SuccessAlert};

use crate::components::layout::Layout;
use api_types::policy::{CreatePolicyRequest, PolicyType};

#[derive(Deserialize)]
pub struct CreatePolicyFormData {
    pub policy_type: String,
    pub title: String,
    pub description: String,
    pub is_required: Option<String>,
    pub max_skips: i32,
}

#[component]
fn CreatePolicyPage(success: Option<String>, error: Option<String>) -> impl IntoView {
    view! {
        <div class="max-w-3xl mx-auto px-6 py-16">
            <h1 class="text-4xl font-display font-bold gradient-text mb-8">"Create Policy"</h1>

            {success.map(|msg| view! { <SuccessAlert message=msg /> })}
            {error.map(|e| view! { <ErrorAlert message=e /> })}

            <form method="POST" action="/create-policy" class="space-y-5">
                <div>
                    <label class="block text-sm font-medium text-text-secondary mb-2">"Policy Type"</label>
                    <select
                        name="policy_type"
                        class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                        required
                    >
                        <option value="terms_of_service">"Terms of Service"</option>
                        <option value="privacy_policy">"Privacy Policy"</option>
                        <option value="cookie_policy">"Cookie Policy"</option>
                        <option value="acceptable_use">"Acceptable Use"</option>
                        <option value="refund_and_cancellation">"Refund &amp; Cancellation"</option>
                        <option value="security_and_data_handling">"Security &amp; Data Handling"</option>
                        <option value="subprocessors">"Subprocessors"</option>
                        <option value="data_retention_and_deletion">"Data Retention &amp; Deletion"</option>
                    </select>
                </div>

                <div>
                    <label class="block text-sm font-medium text-text-secondary mb-2">"Title"</label>
                    <input
                        type="text"
                        name="title"
                        placeholder="Privacy Policy"
                        class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                        required
                    />
                </div>

                <div>
                    <label class="block text-sm font-medium text-text-secondary mb-2">"Description"</label>
                    <input
                        type="text"
                        name="description"
                        placeholder="A brief description of this policy"
                        class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                    />
                </div>

                <div>
                    <label class="block text-sm font-medium text-text-secondary mb-2">"Max Skips"</label>
                    <input
                        type="number"
                        name="max_skips"
                        value="0"
                        min="0"
                        class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                    />
                    <p class="text-xs text-text-secondary mt-1">"0 = user must accept immediately. Higher values allow deferral."</p>
                </div>

                <div>
                    <label class="flex items-center gap-2 text-sm text-text-secondary cursor-pointer">
                        <input type="checkbox" name="is_required" value="true" checked class="rounded border-border" />
                        "Required (users must accept to continue)"
                    </label>
                </div>

                <button
                    type="submit"
                    class="btn-gradient text-white px-8 py-3 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl hover:-translate-y-0.5 active:translate-y-0 transition-all"
                >
                    "Create Policy"
                </button>
            </form>
        </div>
    }
}

pub async fn create_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
) -> Result<Html<String>, Redirect> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        return Err(Redirect::to("/login"));
    }

    let html = view! {
        <Layout title="Create Policy - CloudLess".to_string() auth=state>
            <CreatePolicyPage success=None error=None />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}

pub async fn create_submit_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    axum::Form(form): axum::Form<CreatePolicyFormData>,
) -> Result<Html<String>, Redirect> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        return Err(Redirect::to("/login"));
    }

    let policy_type: PolicyType = match form.policy_type.parse() {
        Ok(pt) => pt,
        Err(e) => {
            let html = view! {
                <Layout title="Create Policy - CloudLess".to_string() auth=state>
                    <CreatePolicyPage success=None error=Some(e) />
                </Layout>
            }
            .to_html();
            return Ok(Html(html));
        }
    };

    let req = CreatePolicyRequest {
        policy_type,
        title: form.title,
        description: form.description,
        is_required: form.is_required.is_some(),
        max_skips: form.max_skips,
    };

    let result = env.policy_api().create_policy(req).await;

    let (success, error) = match result {
        Ok(_) => (Some("Policy created successfully!".to_string()), None),
        Err(e) => (None, Some(format!("{}", e))),
    };

    let html = view! {
        <Layout title="Create Policy - CloudLess".to_string() auth=state>
            <CreatePolicyPage success=success error=error />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}
