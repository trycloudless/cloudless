use crate::auth::{RequestEnv, extract_auth, verify_auth};
use crate::env::WebsiteEnv;
use axum::{
    extract::Path,
    response::{Html, Redirect},
};
use axum_extra::extract::cookie::CookieJar;
use cloudless_core::ports::api::email_template_api_port::EmailTemplateApiPort;
use leptos::prelude::*;
use leptos::tachys::view::RenderHtml;
use serde::Deserialize;
use shared_ui::display::{ErrorAlert, SuccessAlert};

use crate::{components::layout::Layout, error::WebsiteError};
use api_types::email_template::{
    EmailTemplateResponse, EmailTemplateType, UpdateEmailTemplateRequest,
};

#[derive(Deserialize)]
pub struct EditEmailTemplateFormData {
    pub name: String,
    pub subject: String,
    pub body_html: String,
    pub template_type: String,
}

#[component]
fn EditEmailTemplatePage(
    template: EmailTemplateResponse,
    success: Option<String>,
    error: Option<String>,
) -> impl IntoView {
    let name_val = template.name.clone();
    let subject_val = template.subject.clone();
    let body_html_val = template.body_html.clone();
    let template_type_str = template.template_type.to_string();
    let preview_html = template.body_html.clone();

    view! {
        <div class="max-w-5xl mx-auto px-6 py-16">
            <h1 class="text-4xl font-display font-bold gradient-text mb-8">"Edit Email Template"</h1>

            {success.map(|msg| view! { <SuccessAlert message=msg /> })}
            {error.map(|e| view! { <ErrorAlert message=e /> })}

            <div class="grid grid-cols-1 lg:grid-cols-2 gap-8">
                <form method="POST" class="space-y-5">
                    <div>
                        <label class="block text-sm font-medium text-text-secondary mb-2">"Name"</label>
                        <input
                            type="text"
                            name="name"
                            value=name_val
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
                            <option value="password_reset" selected={template_type_str == "password_reset"}>"Password Reset"</option>
                            <option value="promotions" selected={template_type_str == "promotions"}>"Promotions"</option>
                            <option value="service" selected={template_type_str == "service"}>"Service"</option>
                        </select>
                    </div>

                    <div>
                        <label class="block text-sm font-medium text-text-secondary mb-2">"Subject"</label>
                        <input
                            type="text"
                            name="subject"
                            value=subject_val
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
                            class="w-full bg-input border border-border rounded-xl px-4 py-3 text-text-primary font-mono text-sm focus:outline-none focus:border-primary focus:ring-2 focus:ring-primary-tint transition-all"
                            oninput="document.getElementById('preview-frame').srcdoc = this.value"
                            required
                        >{body_html_val}</textarea>
                    </div>

                    <div class="flex gap-4">
                        <button
                            type="submit"
                            class="btn-gradient text-white px-8 py-3 rounded-xl font-semibold shadow-lg shadow-primary-tint hover:shadow-xl hover:-translate-y-0.5 active:translate-y-0 transition-all"
                        >
                            "Update Template"
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
                            srcdoc=preview_html
                        />
                    </div>
                </div>
            </div>
        </div>
    }
}

pub async fn edit_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    Path(id): Path<uuid::Uuid>,
) -> Result<Html<String>, WebsiteError> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        let html = view! {
            <Layout title="Unauthorized - CloudLess".to_string() auth=state>
                <div class="max-w-2xl mx-auto px-6 py-24 text-center">
                    <h1 class="text-4xl font-display font-bold text-error mb-4">"Unauthorized"</h1>
                    <p class="text-text-secondary">"Only admins can manage email templates."</p>
                </div>
            </Layout>
        }
        .to_html();
        return Ok(Html(html));
    }

    let template = env.email_template_api().get_by_id(id).await?;

    let html = view! {
        <Layout title=format!("Edit: {} - CloudLess", &template.name) auth=state>
            <EditEmailTemplatePage template=template success=None error=None />
        </Layout>
    }
    .to_html();

    Ok(Html(html))
}

pub async fn edit_submit_handler(
    RequestEnv(env): RequestEnv,
    jar: CookieJar,
    Path(id): Path<uuid::Uuid>,
    axum::Form(form): axum::Form<EditEmailTemplateFormData>,
) -> Result<Html<String>, Redirect> {
    let auth = extract_auth(&jar);
    let state = verify_auth(&env, &auth).await;
    if !state.is_admin() {
        return Err(Redirect::to("/login"));
    }

    let template_type = EmailTemplateType::from_str(&form.template_type);

    let req = UpdateEmailTemplateRequest {
        id,
        name: Some(form.name),
        subject: Some(form.subject),
        body_html: Some(form.body_html),
        template_type: Some(template_type),
    };

    let result = env.email_template_api().update(req).await;

    match result {
        Ok(template) => {
            let html = view! {
                <Layout title=format!("Edit: {} - CloudLess", &template.name) auth=state>
                    <EditEmailTemplatePage template=template success=Some("Template updated successfully!".to_string()) error=None />
                </Layout>
            }
            .to_html();
            Ok(Html(html))
        }
        Err(e) => {
            let original = env.email_template_api().get_by_id(id).await;
            match original {
                Ok(template) => {
                    let html = view! {
                        <Layout title=format!("Edit: {} - CloudLess", &template.name) auth=state>
                            <EditEmailTemplatePage template=template success=None error=Some(format!("{}", e)) />
                        </Layout>
                    }
                    .to_html();
                    Ok(Html(html))
                }
                Err(_) => Err(Redirect::to("/email-templates")),
            }
        }
    }
}
