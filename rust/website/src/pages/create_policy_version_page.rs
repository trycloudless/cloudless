use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use axum::{
    extract::Path,
    response::{Html, Redirect},
};
use axum_extra::extract::cookie::CookieJar;
use cloudless_core::ports::api::policy_api_port::PolicyApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;
use serde::Deserialize;
use shared_ui::display::{ErrorAlert, SuccessAlert};

use crate::components::layout::Layout;
use api_types::policy::CreatePolicyVersionRequest;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct CreateVersionFormData {
    pub content: String,
}

#[component]
fn CreatePolicyVersionPage(
    policy_id: Uuid,
    success: Option<String>,
    error: Option<String>,
) -> impl IntoView {
    let back_url = format!("/policy/{}/versions", policy_id);

    view! {
        <div class="max-w-3xl mx-auto px-6 py-16">
            <a href=back_url class="text-sm text-text-secondary hover:text-primary-light transition-colors">"Back to Versions"</a>
            <h1 class="text-4xl font-display font-bold gradient-text mt-2 mb-8">"Create New Version"</h1>

            {success.map(|msg| view! { <SuccessAlert message=msg /> })}
            {error.map(|e| view! { <ErrorAlert message=e /> })}

            <form method="POST" class="space-y-5">
                <div>
                    <label class="block text-sm font-medium text-text-secondary mb-2">"Content (Markdown)"</label>
                    <textarea
                        name="content"
                        rows="20"
                        placeholder="# Privacy Policy\n\nYour policy content here..."
                        class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary font-mono text-sm focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                        required
                    />
                </div>

                <button
                    type="submit"
                    class="btn-gradient text-white px-8 py-3 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl hover:-translate-y-0.5 active:translate-y-0 transition-all"
                >
                    "Create Draft"
                </button>
                <p class="text-xs text-text-secondary">"The version will be created as a draft. You can publish it later."</p>
            </form>
        </div>
    }
}

pub async fn create_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    Path(policy_id): Path<Uuid>,
) -> Result<Html<String>, Redirect> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        return Err(Redirect::to("/login"));
    }

    let html = view! {
        <Layout title="Create Policy Version - CloudLess".to_string() auth=state>
            <CreatePolicyVersionPage policy_id=policy_id success=None error=None />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}

pub async fn create_submit_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    Path(policy_id): Path<Uuid>,
    axum::Form(form): axum::Form<CreateVersionFormData>,
) -> Result<Html<String>, Redirect> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        return Err(Redirect::to("/login"));
    }

    let req = CreatePolicyVersionRequest {
        policy_id,
        content: form.content,
    };

    let result = env.policy_api().create_version(req).await;

    let (success, error) = match result {
        Ok(v) => (
            Some(format!("Version {} created as draft!", v.version)),
            None,
        ),
        Err(e) => (None, Some(format!("{}", e))),
    };

    let html = view! {
        <Layout title="Create Policy Version - CloudLess".to_string() auth=state>
            <CreatePolicyVersionPage policy_id=policy_id success=success error=error />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}
