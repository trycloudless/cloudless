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

use crate::{components::layout::Layout, error::WebsiteError};
use api_types::policy::{
    PolicyVersionResponse, PublishPolicyVersionRequest, UpdatePolicyVersionRequest,
};
use uuid::Uuid;

#[derive(Deserialize)]
pub struct EditVersionFormData {
    pub content: String,
}

#[component]
fn EditPolicyVersionPage(
    version: PolicyVersionResponse,
    success: Option<String>,
    error: Option<String>,
) -> impl IntoView {
    let is_draft = version.status == "draft";
    let back_url = format!("/policy/{}/versions", version.policy_id);
    let publish_url = format!("/publish-policy-version/{}", version.id);

    view! {
        <div class="max-w-3xl mx-auto px-6 py-16">
            <a href=back_url class="text-sm text-text-secondary hover:text-primary-light transition-colors">"Back to Versions"</a>
            <h1 class="text-4xl font-display font-bold gradient-text mt-2 mb-2">
                {format!("Version {} - {}", version.version, if is_draft { "Draft" } else { "Published" })}
            </h1>

            {success.map(|msg| view! { <SuccessAlert message=msg /> })}
            {error.map(|e| view! { <ErrorAlert message=e /> })}

            {if is_draft {
                let edit_url = format!("/edit-policy-version/{}", version.id);
                view! {
                    <form method="POST" action=edit_url class="space-y-5 mt-6">
                        <div>
                            <label class="block text-sm font-medium text-text-secondary mb-2">"Content (Markdown)"</label>
                            <textarea
                                name="content"
                                rows="20"
                                class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary font-mono text-sm focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                                required
                            >
                                {version.content.clone()}
                            </textarea>
                        </div>

                        <div class="flex gap-3">
                            <button
                                type="submit"
                                class="btn-gradient text-white px-8 py-3 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl hover:-translate-y-0.5 active:translate-y-0 transition-all"
                            >
                                "Save Draft"
                            </button>
                        </div>
                    </form>

                    <form method="POST" action=publish_url class="mt-4">
                        <button
                            type="submit"
                            class="border border-success text-success px-6 py-2 rounded-xl text-sm font-medium hover:bg-success-tint transition-all"
                        >
                            "Publish This Version"
                        </button>
                        <p class="text-xs text-text-secondary mt-2">"Once published, this version becomes immutable and visible to users."</p>
                    </form>
                }.into_any()
            } else {
                view! {
                    <div class="mt-6">
                        <div class="bg-surface border border-border rounded-xl p-6">
                            <p class="text-sm text-text-secondary mb-3">"This version is published and cannot be edited."</p>
                            <div class="whitespace-pre-wrap text-text-primary text-sm font-mono bg-bg rounded-xl p-4 border border-border-alpha">
                                {version.content.clone()}
                            </div>
                        </div>
                    </div>
                }.into_any()
            }}
        </div>
    }
}

pub async fn edit_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    Path(id): Path<Uuid>,
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

    let version = env.policy_api().get_version(id).await?;

    let html = view! {
        <Layout title="Edit Policy Version - CloudLess".to_string() auth=state>
            <EditPolicyVersionPage version=version success=None error=None />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}

pub async fn edit_submit_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    Path(id): Path<Uuid>,
    axum::Form(form): axum::Form<EditVersionFormData>,
) -> Result<Html<String>, Redirect> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        return Err(Redirect::to("/login"));
    }

    let req = UpdatePolicyVersionRequest {
        id,
        content: form.content,
    };

    let result = env.policy_api().update_version(req).await;

    let (version, success, error) = match result {
        Ok(v) => (v, Some("Draft saved successfully!".to_string()), None),
        Err(e) => match env.policy_api().get_version(id).await {
            Ok(v) => (v, None, Some(format!("{}", e))),
            Err(_) => return Err(Redirect::to("/policies")),
        },
    };

    let html = view! {
        <Layout title="Edit Policy Version - CloudLess".to_string() auth=state>
            <EditPolicyVersionPage version=version success=success error=error />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}

pub async fn publish_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    Path(id): Path<Uuid>,
) -> Result<Html<String>, Redirect> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        return Err(Redirect::to("/login"));
    }

    let req = PublishPolicyVersionRequest { id };
    let result = env.policy_api().publish_version(req).await;

    let (version, success, error) = match result {
        Ok(v) => (v, Some("Version published successfully!".to_string()), None),
        Err(e) => match env.policy_api().get_version(id).await {
            Ok(v) => (v, None, Some(format!("{}", e))),
            Err(_) => return Err(Redirect::to("/policies")),
        },
    };

    let html = view! {
        <Layout title="Edit Policy Version - CloudLess".to_string() auth=state>
            <EditPolicyVersionPage version=version success=success error=error />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}
