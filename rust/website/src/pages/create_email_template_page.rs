use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use axum::response::{Html, Redirect};
use axum_extra::extract::cookie::CookieJar;
use cloudless_core::ports::api::email_template_api_port::EmailTemplateApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;
use serde::Deserialize;
use shared_ui::display::{ErrorAlert, SuccessAlert};

use crate::components::layout::Layout;
use api_types::email_template::{CreateEmailTemplateRequest, EmailTemplateType};

#[derive(Deserialize)]
pub struct CreateEmailTemplateFormData {
    pub name: String,
    pub subject: String,
    pub body_html: String,
    pub template_type: String,
}

#[component]
fn CreateEmailTemplatePage(success: Option<String>, error: Option<String>) -> impl IntoView {
    view! {
        <div class="max-w-5xl mx-auto px-6 py-16">
            <h1 class="text-4xl font-display font-bold gradient-text mb-8">"Create Email Template"</h1>

            {success.map(|msg| view! { <SuccessAlert message=msg /> })}
            {error.map(|e| view! { <ErrorAlert message=e /> })}

            <div class="grid grid-cols-1 lg:grid-cols-2 gap-8">
                <form method="POST" action="/create-email-template" class="space-y-5" id="template-form">
                    <div>
                        <label class="block text-sm font-medium text-text-secondary mb-2">"Name"</label>
                        <input
                            type="text"
                            name="name"
                            placeholder="Password Reset Email"
                            class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                            required
                        />
                    </div>

                    <div>
                        <label class="block text-sm font-medium text-text-secondary mb-2">"Type"</label>
                        <select
                            name="template_type"
                            class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                        >
                            <option value="password_reset">"Password Reset"</option>
                            <option value="promotions">"Promotions"</option>
                            <option value="service">"Service"</option>
                        </select>
                    </div>

                    <div>
                        <label class="block text-sm font-medium text-text-secondary mb-2">"Subject"</label>
                        <input
                            type="text"
                            name="subject"
                            placeholder="Reset your password"
                            class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                            required
                        />
                        <p class="text-xs text-text-secondary mt-1">"Supports Tera variables: {{ user_name }}, {{ reset_link }}"</p>
                    </div>

                    <div>
                        <label class="block text-sm font-medium text-text-secondary mb-2">"Body (HTML)"</label>
                        <textarea
                            name="body_html"
                            id="body_html"
                            rows="15"
                            placeholder="<h1>Hello {{ user_name }}</h1>\n<p>Click <a href=\"{{ reset_link }}\">here</a> to reset your password.</p>"
                            class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary font-mono text-sm focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                            oninput="document.getElementById('preview-frame').srcdoc = this.value"
                            required
                        />
                    </div>

                    <div class="flex gap-4">
                        <button
                            type="submit"
                            class="btn-gradient text-white px-8 py-3 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl hover:-translate-y-0.5 active:translate-y-0 transition-all"
                        >
                            "Create Template"
                        </button>
                        <a
                            href="/email-templates"
                            class="border border-border text-text-secondary px-8 py-3 rounded-xl font-semibold hover:bg-surface transition-all"
                        >
                            "Cancel"
                        </a>
                    </div>
                </form>

                <div>
                    <label class="block text-sm font-medium text-text-secondary mb-2">"Preview"</label>
                    <div class="border border-border rounded-xl overflow-hidden bg-white" style="min-height: 400px">
                        <iframe
                            id="preview-frame"
                            class="w-full"
                            style="min-height: 400px; border: none;"
                            srcdoc=""
                        />
                    </div>
                </div>
            </div>
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
        <Layout title="Create Email Template - CloudLess".to_string() auth=state>
            <CreateEmailTemplatePage success=None error=None />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}

pub async fn create_submit_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    axum::Form(form): axum::Form<CreateEmailTemplateFormData>,
) -> Result<Html<String>, Redirect> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        return Err(Redirect::to("/login"));
    }

    let template_type = EmailTemplateType::from_str(&form.template_type);

    let req = CreateEmailTemplateRequest {
        name: form.name,
        subject: form.subject,
        body_html: form.body_html,
        template_type,
    };

    let result = env.email_template_api().create(req).await;

    let (success, error) = match result {
        Ok(t) => (
            Some(format!("Template '{}' created successfully!", t.name)),
            None,
        ),
        Err(e) => (None, Some(format!("{}", e))),
    };

    let html = view! {
        <Layout title="Create Email Template - CloudLess".to_string() auth=state>
            <CreateEmailTemplatePage success=success error=error />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}
